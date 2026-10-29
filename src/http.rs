//! Loopback HTTP bridge for the hosted WebUI. The token exists only for the
//! lifetime of this process and never enters browser persistent storage.

use crate::{ApiRequest, ApiResponse, api_call, config::Paths};
use anyhow::{Context, Result};
use axum::{
    Json, Router,
    body::Body,
    extract::{
        DefaultBodyLimit, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderValue, Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs::File, io::Read, net::SocketAddr};

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

fn allowed_origin(origin: &str, extra: Option<&str>) -> bool {
    matches!(
        origin,
        "https://www.os-island.com" | "https://os-island.com"
    ) || extra == Some(origin)
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
        .is_some_and(|value| !allowed_origin(value, state.extra_origin.as_deref()))
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
    if request.uri().path() != "/api/isos/v1/health" {
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
            .route("/api/isos/v1/health", get(health))
            .route("/api/isos/v1/status", get(status))
            .route("/api/isos/v1/graph", get(graph))
            .route("/api/isos/v1/start", post(start))
            .route("/api/isos/v1/stop", post(stop))
            .route("/api/isos/v1/apply", post(apply))
            .layer(DefaultBodyLimit::max(2_000_000))
            .layer(middleware::from_fn_with_state(state.clone(), authorize))
            .with_state(state.clone());
        let listener =
            tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
        println!("IsOS local manager: http://127.0.0.1:{port}");
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
    #[test]
    fn only_known_origins_are_allowed() {
        assert!(allowed_origin("https://www.os-island.com", None));
        assert!(!allowed_origin("https://evil.example", None));
        assert!(allowed_origin(
            "http://localhost:5173",
            Some("http://localhost:5173")
        ));
    }
    #[test]
    fn pairing_token_has_256_bits() {
        let a = token().unwrap();
        let b = token().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }
}
