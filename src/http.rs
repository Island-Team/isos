//! Loopback HTTP bridge for the hosted WebUI. The token exists only for the
//! lifetime of this process and never enters browser persistent storage.

use crate::{ApiRequest, ApiResponse, api_call, config::Paths, packages};
use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    body::Body,
    extract::{
        DefaultBodyLimit, Query, State,
        rejection::{JsonRejection, QueryRejection},
        ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs::File,
    io::Read,
    net::{SocketAddr, ToSocketAddrs},
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message as TungsteniteMessage, protocol::WebSocketConfig},
};

const EGIA_PROXY_PATH: &str = "/api/isos/v1/egia/ws";
const EGIA_ROUTES: &[&str] = &[
    "/api/aether/egia/health",
    "/api/aether/egia/backend",
    "/api/aether/egia/chat",
    "/api/aether/egia/missions/stream",
    "/api/aether/egia/missions/command",
    "/api/aether/egia/review",
    "/api/aether/egia/context",
    "/api/aether/egia/model_io",
    "/api/aether/egia/prompts",
    "/api/aether/egia/primitives",
    "/api/aether/egia/skills",
    "/api/aether/egia/skills/command",
    "/api/aether/egia/skills/execution",
    "/api/aether/egia/perceptions",
    "/api/aether/egia/perception/world/state",
    "/api/aether/egia/perception/world/detection",
    "/api/aether/egia/perception/world/asr",
    "/api/aether/egia/primitive/invoke",
];
const EGIA_AUTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const EGIA_MAX_FRAME_SIZE: usize = 1024 * 1024;

#[derive(Clone)]
struct HttpState {
    paths: Paths,
    token: String,
    port: u16,
    extra_origin: Option<String>,
}

#[derive(Default, Deserialize)]
struct QueryParams {
    deployment: Option<String>,
    role: Option<String>,
    q: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EgiaProxyAuth {
    #[serde(rename = "type")]
    kind: String,
    token: String,
    route: String,
    #[serde(default = "default_deployment")]
    deployment: String,
}

fn default_deployment() -> String {
    "default".into()
}

fn allowed_egia_route(route: &str) -> bool {
    EGIA_ROUTES.contains(&route)
}

fn token_matches(expected: &str, received: &str) -> bool {
    expected.len() == received.len()
        && expected
            .bytes()
            .zip(received.bytes())
            .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

fn validate_egia_auth(auth: &EgiaProxyAuth, expected: &str) -> Result<()> {
    ensure!(
        auth.kind == "authenticate" && token_matches(expected, &auth.token),
        "pairing token rejected"
    );
    ensure!(allowed_egia_route(&auth.route), "EGIA route is not allowed");
    ensure!(!auth.deployment.is_empty(), "deployment name is required");
    Ok(())
}

fn resolve_loopback_target(address: &str) -> Result<SocketAddr> {
    let (host, port) = if let Some(rest) = address.strip_prefix('[') {
        let (host, suffix) = rest
            .split_once(']')
            .ok_or_else(|| anyhow::anyhow!("invalid EGIA ready_tcp address"))?;
        let port = suffix
            .strip_prefix(':')
            .ok_or_else(|| anyhow::anyhow!("invalid EGIA ready_tcp address"))?;
        (host, port)
    } else {
        address
            .rsplit_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid EGIA ready_tcp address"))?
    };
    ensure!(
        !host.is_empty() && !host.contains(['/', '?', '#', '@']) && !port.is_empty(),
        "invalid EGIA ready_tcp address"
    );
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| anyhow::anyhow!("EGIA ready_tcp port must be 1..65535"))?;
    let addresses = (host, port).to_socket_addrs()?.collect::<Vec<_>>();
    ensure!(
        !addresses.is_empty() && addresses.iter().all(|address| address.ip().is_loopback()),
        "EGIA ready_tcp must resolve only to loopback addresses"
    );
    Ok(addresses[0])
}

fn egia_target(paths: &Paths, deployment: &str) -> Result<SocketAddr> {
    ensure!(
        !deployment.is_empty()
            && deployment.len() <= 48
            && deployment.as_bytes()[0].is_ascii_lowercase()
            && deployment
                .bytes()
                .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-'),
        "invalid deployment name"
    );
    let graph = crate::config::load_selected(paths, deployment)?;
    let node = graph
        .nodes
        .iter()
        .find(|node| node.enabled && node.role.as_deref() == Some("egia"))
        .ok_or_else(|| anyhow::anyhow!("EGIA component is not enabled in this deployment"))?;
    let address = node
        .launch
        .as_ref()
        .and_then(|launch| launch.ready_tcp.as_deref())
        .ok_or_else(|| anyhow::anyhow!("EGIA component has no launch.ready_tcp address"))?;
    resolve_loopback_target(address)
}

async fn egia_proxy(State(state): State<HttpState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.max_frame_size(EGIA_MAX_FRAME_SIZE)
        .max_message_size(EGIA_MAX_FRAME_SIZE)
        .on_upgrade(move |socket| proxy_egia_socket(socket, state))
}

async fn ws_error(socket: &mut WebSocket, reason: &str) {
    let _ = socket
        .send(AxumMessage::Text(
            json!({"type":"error","error":reason}).to_string().into(),
        ))
        .await;
    let _ = socket.send(AxumMessage::Close(None)).await;
}

async fn proxy_egia_socket(mut client: WebSocket, state: HttpState) {
    let authentication = tokio::time::timeout(EGIA_AUTH_TIMEOUT, client.recv()).await;
    let auth = match authentication {
        Ok(Some(Ok(AxumMessage::Text(text)))) => serde_json::from_str::<EgiaProxyAuth>(&text).ok(),
        _ => None,
    };
    let Some(auth) = auth else {
        ws_error(
            &mut client,
            "first frame must authenticate within 10 seconds",
        )
        .await;
        return;
    };
    if let Err(error) = validate_egia_auth(&auth, &state.token) {
        ws_error(&mut client, &error.to_string()).await;
        return;
    }
    let target = match egia_target(&state.paths, &auth.deployment) {
        Ok(target) => target,
        Err(error) => {
            ws_error(&mut client, &format!("EGIA target unavailable: {error:#}")).await;
            return;
        }
    };
    let url = format!("ws://{target}{}", auth.route);
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(EGIA_MAX_FRAME_SIZE);
    config.max_frame_size = Some(EGIA_MAX_FRAME_SIZE);
    let upstream = match connect_async_with_config(url, Some(config), false).await {
        Ok((socket, _)) => socket,
        Err(error) => {
            ws_error(
                &mut client,
                &format!("could not connect to configured EGIA: {error}"),
            )
            .await;
            return;
        }
    };
    if client
        .send(AxumMessage::Text(
            json!({"type":"ready","route":auth.route})
                .to_string()
                .into(),
        ))
        .await
        .is_err()
    {
        return;
    }
    relay_egia(client, upstream).await;
}

async fn relay_egia<S>(client: WebSocket, upstream: tokio_tungstenite::WebSocketStream<S>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut client_tx, mut client_rx) = client.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();
    loop {
        tokio::select! {
            message = client_rx.next() => match message {
                Some(Ok(message)) => {
                    let close = matches!(message, AxumMessage::Close(_));
                    let converted = axum_to_tungstenite(message);
                    if upstream_tx.send(converted).await.is_err() || close { break; }
                }
                _ => { let _ = upstream_tx.send(TungsteniteMessage::Close(None)).await; break; }
            },
            message = upstream_rx.next() => match message {
                Some(Ok(message)) => {
                    let close = matches!(message, TungsteniteMessage::Close(_));
                    if client_tx.send(tungstenite_to_axum(message)).await.is_err() || close { break; }
                }
                _ => { let _ = client_tx.send(AxumMessage::Close(None)).await; break; }
            },
        }
    }
}

fn axum_to_tungstenite(message: AxumMessage) -> TungsteniteMessage {
    match message {
        AxumMessage::Text(text) => TungsteniteMessage::Text(text.to_string().into()),
        AxumMessage::Binary(bytes) => TungsteniteMessage::Binary(bytes),
        AxumMessage::Ping(bytes) => TungsteniteMessage::Ping(bytes),
        AxumMessage::Pong(bytes) => TungsteniteMessage::Pong(bytes),
        AxumMessage::Close(_) => TungsteniteMessage::Close(None),
    }
}

fn tungstenite_to_axum(message: TungsteniteMessage) -> AxumMessage {
    match message {
        TungsteniteMessage::Text(text) => AxumMessage::Text(text.to_string().into()),
        TungsteniteMessage::Binary(bytes) => AxumMessage::Binary(bytes),
        TungsteniteMessage::Ping(bytes) => AxumMessage::Ping(bytes),
        TungsteniteMessage::Pong(bytes) => AxumMessage::Pong(bytes),
        TungsteniteMessage::Close(_) => AxumMessage::Close(None),
        TungsteniteMessage::Frame(_) => AxumMessage::Close(None),
    }
}

fn static_headers(content_type: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
    );
    headers
}

async fn local_page() -> (HeaderMap, Html<&'static str>) {
    (
        static_headers("text/html; charset=utf-8"),
        Html(include_str!("../web/local/index.html")),
    )
}

async fn local_script() -> (HeaderMap, &'static str) {
    (
        static_headers("text/javascript; charset=utf-8"),
        include_str!("../web/local/app.js"),
    )
}

async fn local_style() -> (HeaderMap, &'static str) {
    (
        static_headers("text/css; charset=utf-8"),
        include_str!("../web/local/style.css"),
    )
}

#[derive(Deserialize)]
struct ActionBody {
    schema_version: u32,
    deployment: Option<String>,
    role: Option<String>,
    graph: Option<crate::graph::Deployment>,
}

fn reply(result: Result<Value>) -> (StatusCode, Json<ApiResponse>) {
    match result {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse {
                schema_version: 1,
                ok: true,
                data,
                error: None,
            }),
        ),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                schema_version: 1,
                ok: false,
                data: Value::Null,
                error: Some(error.to_string()),
            }),
        ),
    }
}

async fn execute(state: HttpState, request: ApiRequest) -> (StatusCode, Json<ApiResponse>) {
    match tokio::task::spawn_blocking(move || api_call(&state.paths, request)).await {
        Ok(result) => reply(result),
        Err(error) => reply(Err(error.into())),
    }
}

async fn health() -> (StatusCode, Json<ApiResponse>) {
    reply(Ok(
        json!({"service":"isos","version":env!("CARGO_PKG_VERSION")}),
    ))
}

async fn status(
    State(state): State<HttpState>,
    params: Result<Query<QueryParams>, QueryRejection>,
) -> (StatusCode, Json<ApiResponse>) {
    let Query(params) = match params {
        Ok(value) => value,
        Err(error) => return reply(Err(error.into())),
    };
    execute(
        state,
        ApiRequest {
            schema_version: 1,
            op: "status".into(),
            deployment: params.deployment,
            role: params.role,
            graph: None,
        },
    )
    .await
}

async fn graph(
    State(state): State<HttpState>,
    params: Result<Query<QueryParams>, QueryRejection>,
) -> (StatusCode, Json<ApiResponse>) {
    let Query(params) = match params {
        Ok(value) => value,
        Err(error) => return reply(Err(error.into())),
    };
    execute(
        state,
        ApiRequest {
            schema_version: 1,
            op: "graph".into(),
            deployment: params.deployment,
            role: None,
            graph: None,
        },
    )
    .await
}

async fn installed_packages(State(state): State<HttpState>) -> (StatusCode, Json<ApiResponse>) {
    let paths = state.paths;
    match tokio::task::spawn_blocking(move || Ok(serde_json::to_value(packages::list(&paths)?)?))
        .await
    {
        Ok(result) => reply(result),
        Err(error) => reply(Err(error.into())),
    }
}

async fn catalog_search(
    params: Result<Query<QueryParams>, QueryRejection>,
) -> (StatusCode, Json<ApiResponse>) {
    let Query(params) = match params {
        Ok(value) => value,
        Err(error) => return reply(Err(error.into())),
    };
    let query = params.q.unwrap_or_default();
    match tokio::task::spawn_blocking(move || packages::search(&query)).await {
        Ok(result) => reply(result),
        Err(error) => reply(Err(error.into())),
    }
}

async fn action(
    State(state): State<HttpState>,
    method: &'static str,
    body: Result<Json<ActionBody>, JsonRejection>,
) -> (StatusCode, Json<ApiResponse>) {
    let Json(body) = match body {
        Ok(value) => value,
        Err(error) => return reply(Err(error.into())),
    };
    execute(
        state,
        ApiRequest {
            schema_version: body.schema_version,
            op: method.into(),
            deployment: body.deployment,
            role: body.role,
            graph: body.graph,
        },
    )
    .await
}

async fn start(
    state: State<HttpState>,
    body: Result<Json<ActionBody>, JsonRejection>,
) -> (StatusCode, Json<ApiResponse>) {
    action(state, "start", body).await
}
async fn stop(
    state: State<HttpState>,
    body: Result<Json<ActionBody>, JsonRejection>,
) -> (StatusCode, Json<ApiResponse>) {
    action(state, "stop", body).await
}
async fn apply(
    state: State<HttpState>,
    body: Result<Json<ActionBody>, JsonRejection>,
) -> (StatusCode, Json<ApiResponse>) {
    action(state, "apply", body).await
}

fn allowed_origin(origin: &str, extra: Option<&str>, port: u16) -> bool {
    matches!(
        origin,
        "https://www.os-island.com" | "https://os-island.com"
    ) || origin == format!("http://127.0.0.1:{port}")
        || extra == Some(origin)
}

fn cors_headers(response: &mut Response, origin: &str) {
    if let Ok(value) = HeaderValue::from_str(origin) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
        response
            .headers_mut()
            .insert(header::VARY, HeaderValue::from_static("Origin"));
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Authorization, Content-Type"),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
    }
}

fn denied(code: StatusCode, message: &str, origin: Option<&str>) -> Response {
    let mut response = (
        code,
        Json(ApiResponse {
            schema_version: 1,
            ok: false,
            data: Value::Null,
            error: Some(message.into()),
        }),
    )
        .into_response();
    if let Some(origin) = origin {
        cors_headers(&mut response, origin);
    }
    response
}

async fn authorize(State(state): State<HttpState>, request: Request<Body>, next: Next) -> Response {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    if origin
        .as_deref()
        .is_some_and(|value| !allowed_origin(value, state.extra_origin.as_deref(), state.port))
    {
        return denied(StatusCode::FORBIDDEN, "origin is not allowed", None);
    }
    let expected_host = format!("127.0.0.1:{}", state.port);
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    if host != Some(expected_host.as_str()) {
        return denied(
            StatusCode::FORBIDDEN,
            "connect through 127.0.0.1",
            origin.as_deref(),
        );
    }
    if request.method() == Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        if let Some(origin) = &origin {
            cors_headers(&mut response, origin);
            if request
                .headers()
                .get("access-control-request-private-network")
                .is_some_and(|value| value == "true")
            {
                response.headers_mut().insert(
                    "access-control-allow-private-network",
                    HeaderValue::from_static("true"),
                );
            }
        }
        return response;
    }
    let public_path = matches!(
        request.uri().path(),
        "/" | "/ui.js" | "/ui.css" | "/api/isos/v1/health"
    );
    let websocket_pairing = request.uri().path() == EGIA_PROXY_PATH
        && request
            .headers()
            .get(header::UPGRADE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    if !public_path && !websocket_pairing {
        let expected = format!("Bearer {}", state.token);
        let received = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok());
        if received != Some(expected.as_str()) {
            return denied(
                StatusCode::UNAUTHORIZED,
                "pairing token required",
                origin.as_deref(),
            );
        }
    }
    let mut response = next.run(request).await;
    if let Some(origin) = &origin {
        cors_headers(&mut response, origin);
    }
    response
}

fn token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(hex::encode(bytes))
}

pub fn serve(paths: Paths, port: u16, extra_origin: Option<String>) -> Result<()> {
    let token = token()?;
    let state = HttpState {
        paths,
        token,
        port,
        extra_origin,
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("start IsOS HTTP runtime")?;
    runtime.block_on(async move {
        let app = Router::new()
            .route("/", get(local_page))
            .route("/ui.js", get(local_script))
            .route("/ui.css", get(local_style))
            .route("/api/isos/v1/health", get(health))
            .route("/api/isos/v1/status", get(status))
            .route("/api/isos/v1/graph", get(graph))
            .route("/api/isos/v1/packages", get(installed_packages))
            .route("/api/isos/v1/catalog", get(catalog_search))
            .route("/api/isos/v1/start", post(start))
            .route("/api/isos/v1/stop", post(stop))
            .route("/api/isos/v1/apply", post(apply))
            .route(EGIA_PROXY_PATH, get(egia_proxy))
            .layer(DefaultBodyLimit::max(2_000_000))
            .layer(middleware::from_fn_with_state(state.clone(), authorize))
            .with_state(state.clone());
        let listener =
            tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
        println!(
            "IsOS local manager: http://127.0.0.1:{port}/#token={}",
            state.token
        );
        println!(
            "Pairing token (valid only while this command runs): {}",
            state.token
        );
        println!("Paste the token into the IsOS page at https://www.os-island.com/isos.");
        axum::serve(listener, app).await?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, routing::get};
    use futures_util::{SinkExt, StreamExt};
    use tokio::sync::oneshot;
    use tokio_tungstenite::{
        accept_async, accept_hdr_async, connect_async,
        tungstenite::{
            client::IntoClientRequest,
            handshake::server::{Request as ServerRequest, Response as ServerResponse},
        },
    };

    #[test]
    fn only_known_origins_are_allowed() {
        assert!(allowed_origin("https://www.os-island.com", None, 17600));
        assert!(allowed_origin("http://127.0.0.1:17600", None, 17600));
        assert!(!allowed_origin("http://127.0.0.1:17601", None, 17600));
        assert!(!allowed_origin("https://evil.example", None, 17600));
        assert!(allowed_origin(
            "http://localhost:5173",
            Some("http://localhost:5173"),
            17600
        ));
    }
    #[test]
    fn pairing_token_has_256_bits() {
        let a = token().unwrap();
        let b = token().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn egia_auth_requires_pairing_token_and_allowlisted_route() {
        let valid = EgiaProxyAuth {
            kind: "authenticate".into(),
            token: "secret".into(),
            route: "/api/aether/egia/health".into(),
            deployment: "default".into(),
        };
        assert!(validate_egia_auth(&valid, "secret").is_ok());
        assert!(
            validate_egia_auth(&valid, "wrong")
                .unwrap_err()
                .to_string()
                .contains("token")
        );
        let mut invalid_route = valid;
        invalid_route.route = "ws://127.0.0.1:1/private".into();
        assert!(
            validate_egia_auth(&invalid_route, "secret")
                .unwrap_err()
                .to_string()
                .contains("route")
        );
    }

    #[test]
    fn egia_target_must_be_loopback_with_a_valid_port() {
        assert_eq!(
            resolve_loopback_target("127.0.0.1:16002").unwrap(),
            "127.0.0.1:16002".parse().unwrap()
        );
        assert_eq!(
            resolve_loopback_target("[::1]:16002").unwrap(),
            "[::1]:16002".parse().unwrap()
        );
        assert!(
            resolve_loopback_target("192.0.2.1:16002")
                .unwrap_err()
                .to_string()
                .contains("loopback")
        );
        assert!(resolve_loopback_target("127.0.0.1:0").is_err());
        assert!(resolve_loopback_target("127.0.0.1:99999").is_err());
        assert!(resolve_loopback_target("127.0.0.1:16002/private").is_err());
    }

    async fn relay_handler(
        ws: WebSocketUpgrade,
        State(upstream): State<String>,
    ) -> impl IntoResponse {
        ws.on_upgrade(move |client| async move {
            if let Ok((server, _)) = connect_async(upstream).await {
                relay_egia(client, server).await;
            }
        })
    }

    #[tokio::test]
    async fn proxy_relays_text_frames_and_propagates_client_close() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_address = upstream_listener.local_addr().unwrap();
        let (close_tx, close_rx) = oneshot::channel();
        tokio::spawn(async move {
            let (stream, _) = upstream_listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let frame = socket.next().await.unwrap().unwrap();
            assert_eq!(frame.into_text().unwrap(), "hello from browser");
            socket
                .send(TungsteniteMessage::Text("hello from EGIA".into()))
                .await
                .unwrap();
            let closed = matches!(socket.next().await, Some(Ok(TungsteniteMessage::Close(_))));
            let _ = close_tx.send(closed);
        });

        let app = Router::new()
            .route("/ws", get(relay_handler))
            .with_state(format!("ws://{upstream_address}/"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(axum::serve(listener, app).into_future());
        let (mut client, _) = connect_async(format!("ws://{address}/ws")).await.unwrap();
        client
            .send(TungsteniteMessage::Text("hello from browser".into()))
            .await
            .unwrap();
        assert_eq!(
            client.next().await.unwrap().unwrap().into_text().unwrap(),
            "hello from EGIA"
        );
        client.send(TungsteniteMessage::Close(None)).await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), close_rx)
                .await
                .unwrap()
                .unwrap()
        );
        server.abort();
    }

    #[tokio::test]
    #[allow(clippy::result_large_err)]
    async fn egia_route_authenticates_before_upstream_and_preserves_egia_frames() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_address = upstream_listener.local_addr().unwrap();
        let temp = std::env::temp_dir().join(format!("isos-egia-proxy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        let paths = Paths {
            config: temp.join("config"),
            data: temp.join("data"),
            runtime: temp.join("run"),
        };
        std::fs::create_dir_all(paths.config.join("deployments")).unwrap();
        let graph = json!({
            "schema_version": 1, "name": "default", "edges": [],
            "nodes": [{"id":"egia","kind":"component","role":"egia","launch":{
                "command":["egia","--managed","--no-tui"], "ready_tcp":upstream_address.to_string()
            }}]
        });
        std::fs::write(paths.graph("default"), serde_json::to_vec(&graph).unwrap()).unwrap();

        let (upstream_accepted_tx, mut upstream_accepted_rx) = oneshot::channel();
        let (upstream_close_tx, upstream_close_rx) = oneshot::channel();
        tokio::spawn(async move {
            let (stream, _) = upstream_listener.accept().await.unwrap();
            let _ = upstream_accepted_tx.send(());
            let mut socket = accept_hdr_async(
                stream,
                |request: &ServerRequest, response: ServerResponse| {
                    assert_eq!(request.uri().path(), "/api/aether/egia/health");
                    assert!(!request.headers().contains_key(header::ORIGIN));
                    Ok(response)
                },
            )
            .await
            .unwrap();
            socket
                .send(TungsteniteMessage::Text("egia-frame".into()))
                .await
                .unwrap();
            assert_eq!(
                socket.next().await.unwrap().unwrap().into_text().unwrap(),
                "browser-frame"
            );
            let closed = matches!(socket.next().await, Some(Ok(TungsteniteMessage::Close(_))));
            let _ = socket.send(TungsteniteMessage::Close(None)).await;
            let _ = upstream_close_tx.send(closed);
        });

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = HttpState {
            paths,
            token: "pairing-secret".into(),
            port,
            extra_origin: None,
        };
        let app = Router::new()
            .route(EGIA_PROXY_PATH, get(egia_proxy))
            .layer(middleware::from_fn_with_state(state.clone(), authorize))
            .with_state(state);
        let bridge = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let url = format!("ws://127.0.0.1:{port}{EGIA_PROXY_PATH}");
        let mut bad = url.clone().into_client_request().unwrap();
        bad.headers_mut().insert(
            header::ORIGIN,
            HeaderValue::from_static("https://www.os-island.com"),
        );
        let mut denied_origin = url.clone().into_client_request().unwrap();
        denied_origin.headers_mut().insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.example"),
        );
        assert!(connect_async(denied_origin).await.is_err());
        let (mut rejected, _) = connect_async(bad).await.unwrap();
        rejected.send(TungsteniteMessage::Text(json!({
            "type":"authenticate", "token":"wrong", "deployment":"default", "route":"/api/aether/egia/health"
        }).to_string().into())).await.unwrap();
        let error = rejected.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(error.contains("pairing token rejected"));
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(50),
                &mut upstream_accepted_rx
            )
            .await
            .is_err()
        );

        let mut request = url.into_client_request().unwrap();
        request.headers_mut().insert(
            header::ORIGIN,
            HeaderValue::from_static("https://www.os-island.com"),
        );
        let (mut client, _) = connect_async(request).await.unwrap();
        client.send(TungsteniteMessage::Text(json!({
            "type":"authenticate", "token":"pairing-secret", "deployment":"default", "route":"/api/aether/egia/health"
        }).to_string().into())).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), &mut upstream_accepted_rx)
            .await
            .unwrap()
            .unwrap();
        let ready = client.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(ready.contains("\"type\":\"ready\""));
        assert_eq!(
            client.next().await.unwrap().unwrap().into_text().unwrap(),
            "egia-frame"
        );
        client
            .send(TungsteniteMessage::Text("browser-frame".into()))
            .await
            .unwrap();
        client.send(TungsteniteMessage::Close(None)).await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), upstream_close_rx)
                .await
                .unwrap()
                .unwrap()
        );
        bridge.abort();
        let _ = std::fs::remove_dir_all(temp);
    }
}
