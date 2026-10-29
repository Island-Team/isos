//! Loopback configuration API and optional pinned IsosUI bundle.
use crate::{
    component,
    config::{self, Paths},
    graph::Deployment,
    isosui, lifecycle,
};
use anyhow::{Context, Result};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path as UrlPath, State},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    net::SocketAddr,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::SystemTime,
};

struct App {
    paths: Paths,
    deployment: String,
    token: String,
    web: Option<PathBuf>,
    pin_cache: Mutex<PinCache>,
}

#[derive(Default)]
struct PinCache {
    observed: bool,
    signature: Option<(SystemTime, u64)>,
    directory: Option<PathBuf>,
}

const LOCAL_ORIGIN: &str = "http://127.0.0.1:16000";
const HOSTED_ORIGINS: [&str; 2] = ["https://www.os-island.com", "https://os-island.com"];
const HOSTED_EDITOR: &str = "https://www.os-island.com/en/isos";

type ApiResult = std::result::Result<Json<Value>, (StatusCode, Json<Value>)>;
fn err(status: StatusCode, message: impl ToString) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({"schema_version":2,"ok":false,"data":null,"error":message.to_string()})),
    )
}
fn ok(data: Value) -> ApiResult {
    Ok(Json(
        json!({"schema_version":2,"ok":true,"data":data,"error":null}),
    ))
}
fn authorized(
    state: &App,
    headers: &HeaderMap,
) -> std::result::Result<(), (StatusCode, Json<Value>)> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());
    if bearer != Some(format!("Bearer {}", state.token).as_str()) {
        return Err(err(StatusCode::UNAUTHORIZED, "invalid bearer token"));
    }
    Ok(())
}
fn allowed_origin(origin: &str) -> bool {
    origin == LOCAL_ORIGIN || HOSTED_ORIGINS.contains(&origin)
}

async fn cors(request: Request<Body>, next: Next) -> Response {
    let headers = request.headers();
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if host != "127.0.0.1:16000" {
        return err(StatusCode::FORBIDDEN, "invalid Host").into_response();
    }
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    if headers.contains_key(header::ORIGIN) && !origin.as_deref().is_some_and(allowed_origin) {
        return err(StatusCode::FORBIDDEN, "invalid Origin").into_response();
    }
    let preflight = request.method() == Method::OPTIONS;
    if preflight {
        if origin.is_none() {
            return err(StatusCode::FORBIDDEN, "preflight requires Origin").into_response();
        }
        let method = headers
            .get(header::ACCESS_CONTROL_REQUEST_METHOD)
            .and_then(|h| h.to_str().ok());
        if !matches!(method, Some("GET" | "POST" | "PUT")) {
            return err(StatusCode::FORBIDDEN, "invalid preflight method").into_response();
        }
        if let Some(names) = headers
            .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
            .and_then(|h| h.to_str().ok())
            && names.split(',').any(|name| {
                !matches!(
                    name.trim().to_ascii_lowercase().as_str(),
                    "authorization" | "content-type"
                )
            })
        {
            return err(StatusCode::FORBIDDEN, "invalid preflight headers").into_response();
        }
    }
    let mut response = if preflight {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    if let Some(origin) = origin {
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_str(&origin).expect("approved Origin is a header value"),
        );
        response
            .headers_mut()
            .insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    if preflight {
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, PUT"),
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
    response
}

async fn preflight() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn version(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    ok(json!({"api_schema_version":2,"isos_version":env!("CARGO_PKG_VERSION")}))
}
fn revision(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn diagnostic_path(graph: &Deployment, message: &str) -> String {
    for (index, edge) in graph.edges.iter().enumerate() {
        if message.contains(&format!("{} -> {}", edge.from, edge.to)) {
            return format!("/edges/{index}");
        }
    }
    for (index, node) in graph.nodes.iter().enumerate() {
        if message.contains(&format!("{}:", node.id))
            || message.starts_with(&format!("{} ", node.id))
            || message.contains(&format!("node ID: {}", node.id))
        {
            let suffix = if message.contains("UDS path") || message.contains("socket_path") {
                "/settings/socket_path"
            } else if message.contains("profile") {
                "/profile"
            } else {
                ""
            };
            return format!("/nodes/{index}{suffix}");
        }
    }
    "/".into()
}
async fn graph(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let bytes =
        fs::read(app.paths.graph(&app.deployment)).map_err(|e| err(StatusCode::NOT_FOUND, e))?;
    let graph: Value =
        serde_json::from_slice(&bytes).map_err(|e| err(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    ok(json!({"revision":revision(&bytes),"graph":graph}))
}
#[derive(Deserialize)]
struct Edit {
    schema_version: u32,
    graph: Deployment,
    #[serde(default)]
    revision: Option<String>,
}
async fn validate(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(edit): Json<Edit>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if edit.schema_version != 2 {
        return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 2"));
    }
    let diagnostic = edit
        .graph
        .validate()
        .and_then(|_| component::validate_profiles(&edit.graph))
        .err()
        .map(|e| {let message=e.to_string();json!({"path":diagnostic_path(&edit.graph,&message),"code":"invalid_graph","message":message})});
    ok(
        json!({"valid":diagnostic.is_none(),"diagnostics":diagnostic.into_iter().collect::<Vec<_>>()}),
    )
}
async fn apply(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(edit): Json<Edit>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if edit.schema_version != 2 {
        return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 2"));
    }
    if edit.graph.name != app.deployment {
        return Err(err(StatusCode::BAD_REQUEST, "deployment name mismatch"));
    }
    let _lock =
        lifecycle::Lock::take(&app.paths).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let current =
        fs::read(app.paths.graph(&app.deployment)).map_err(|e| err(StatusCode::NOT_FOUND, e))?;
    if edit.revision.as_deref() != Some(revision(&current).as_str()) {
        return Err(err(
            StatusCode::CONFLICT,
            "graph revision changed; reload before applying",
        ));
    }
    config::apply(&app.paths, &edit.graph).map_err(|e| err(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    let bytes = fs::read(app.paths.graph(&app.deployment))
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    ok(json!({"revision":revision(&bytes),"graph":edit.graph}))
}
async fn profiles(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    // The descriptors are queried without launching either component.
    let mut profiles = Vec::new();
    for role in ["eos", "egia"] {
        if let Ok(value) = component::call(role, "profiles", None)
            && let Some(items) = value
                .get("profiles")
                .or(Some(&value))
                .and_then(Value::as_array)
        {
            profiles.extend(items.iter().map(|profile| {
                            let mut defaults = serde_json::Map::new();
                            if let Some(fields)=profile.get("fields").and_then(Value::as_array) {
                                for field in fields {
                                    if let (Some(name),Some(default))=(field.get("name").and_then(Value::as_str),field.get("default")) { defaults.insert(name.into(),default.clone()); }
                                }
                            } else if let Some(fields)=profile.get("fields").and_then(Value::as_object) {
                                for (name,field) in fields { if let Some(default)=field.get("default") { defaults.insert(name.clone(),default.clone()); } }
                            }
                            let ports = |key: &str| -> Vec<Value> {
                                profile.get(key).and_then(Value::as_array).into_iter().flatten().map(|port| {
                                    let contract = port.get("contracts").and_then(Value::as_array).and_then(|a|a.first()).and_then(|c|c.get("name")).and_then(Value::as_str);
                                    json!({"name":port.get("name"),"type":port.get("type"),"contract":contract})
                                }).collect()
                            };
                            json!({"id":profile.get("id"),"version":profile.get("version"),"label":profile.get("label").or_else(||profile.get("id")),"description":profile.get("description"),"inputs":ports("inputs"),"outputs":ports("outputs"),"defaults":defaults})
                        }));
        }
    }
    ok(json!({"profiles":profiles}))
}
async fn component_route(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    UrlPath((role, op)): UrlPath<(String, String)>,
    body: Option<Json<Value>>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if !["eos", "egia"].contains(&role.as_str())
        || !["describe", "defaults", "profiles", "render", "validate"].contains(&op.as_str())
    {
        return Err(err(StatusCode::NOT_FOUND, "unknown component operation"));
    }
    if matches!(op.as_str(), "render" | "validate") && body.is_none() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "operation requires JSON values",
        ));
    }
    let values = body.map(|Json(payload)| payload.get("values").cloned().unwrap_or(payload));
    let value = component::call(&role, &op, values)
        .map_err(|e| err(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    ok(value)
}
async fn static_file(State(app): State<Arc<App>>, UrlPath(path): UrlPath<String>) -> Response {
    let web = if let Some(dev) = &app.web {
        Some(dev.clone())
    } else {
        match cached_pin(&app) {
            Ok(directory) => directory,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Pinned IsosUI is invalid: {error}"),
                )
                    .into_response();
            }
        }
    };
    let Some(web) = web else {
        return (
            StatusCode::NOT_FOUND,
            "No IsosUI build is pinned; run isos build isosui and isos version pin isosui <commit>",
        )
            .into_response();
    };
    let relative = path.trim_start_matches('/');
    let candidate = web.join(relative);
    let file =
        if relative.is_empty() || !candidate.is_file() || relative.split('/').any(|p| p == "..") {
            web.join("index.html")
        } else {
            candidate
        };
    let mime = match file.extension().and_then(|x| x.to_str()) {
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        _ => "text/html; charset=utf-8",
    };
    match fs::read(file) {
        Ok(bytes) => ([(header::CONTENT_TYPE, mime)], Body::from(bytes)).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "IsOS UI bundle is not installed").into_response(),
    }
}
fn cached_pin(app: &App) -> Result<Option<PathBuf>> {
    let path = app.paths.config.join("pins/isosui.json");
    let signature = match fs::metadata(path) {
        Ok(metadata) => Some((metadata.modified()?, metadata.len())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let mut cache = app
        .pin_cache
        .lock()
        .map_err(|_| anyhow::anyhow!("IsosUI pin cache poisoned"))?;
    if !cache.observed || cache.signature != signature {
        cache.directory = isosui::pinned_dir(&app.paths)?;
        cache.signature = signature;
        cache.observed = true;
    }
    Ok(cache.directory.clone())
}
fn token(paths: &Paths) -> Result<String> {
    let path = paths.runtime.join("ui-token");
    if path.exists() {
        return Ok(fs::read_to_string(path)?.trim().to_owned());
    }
    let mut bytes = [0u8; 32];
    use std::io::Read;
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let value = hex::encode(bytes);
    config::atomic_write(&path, value.as_bytes())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(value)
}
pub fn open_url(paths: &Paths) -> Result<String> {
    let local = std::env::var_os("ISOS_WEB_DIST")
        .map(PathBuf::from)
        .is_some_and(|p| p.join("index.html").is_file())
        || isosui::pinned_dir(paths)?.is_some();
    let base = if local {
        "http://127.0.0.1:16000/en/isos"
    } else {
        HOSTED_EDITOR
    };
    Ok(format!("{base}#token={}", token(paths)?))
}
pub fn serve(paths: Paths, deployment: String) -> Result<()> {
    paths.prepare()?;
    let web = std::env::var_os("ISOS_WEB_DIST").map(PathBuf::from);
    let app = Arc::new(App {
        token: token(&paths)?,
        paths,
        deployment,
        web,
        pin_cache: Mutex::new(PinCache::default()),
    });
    let router = router(app);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let address: SocketAddr = "127.0.0.1:16000".parse()?;
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .context("bind IsOS loopback API")?;
        axum::serve(listener, router).await?;
        Ok(())
    })
}
fn router(app: Arc<App>) -> Router {
    let api = Router::new()
        .route("/api/isos/v2/version", get(version).options(preflight))
        .route(
            "/api/isos/v2/graph",
            get(graph).put(apply).options(preflight),
        )
        .route(
            "/api/isos/v2/graph/validate",
            post(validate).options(preflight),
        )
        .route("/api/isos/v2/profiles", get(profiles).options(preflight))
        .route(
            "/api/isos/v2/config/{role}/{op}",
            get(component_route)
                .post(component_route)
                .options(preflight),
        )
        .route_layer(middleware::from_fn(cors));
    Router::new()
        .merge(api)
        .route("/", get(static_file_root))
        .route("/{*path}", get(static_file))
        .with_state(app)
}
async fn static_file_root(State(app): State<Arc<App>>) -> Response {
    static_file(State(app), UrlPath(String::new())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    fn test_router() -> Router {
        router(Arc::new(App {
            paths: Paths {
                config: PathBuf::from("/tmp/isos-test/config"),
                data: PathBuf::from("/tmp/isos-test/data"),
                runtime: PathBuf::from("/tmp/isos-test/run"),
            },
            deployment: "default".into(),
            token: "test-secret".into(),
            web: None,
            pin_cache: Mutex::new(PinCache::default()),
        }))
    }

    #[tokio::test]
    async fn hosted_origin_preflight_and_bearer_boundary() {
        let request = Request::builder()
            .method(Method::OPTIONS)
            .uri("/api/isos/v2/version")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::ORIGIN, "https://www.os-island.com")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
            .body(Body::empty())
            .unwrap();
        let response = test_router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "https://www.os-island.com"
        );

        let request = Request::builder()
            .uri("/api/isos/v2/version")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::ORIGIN, "https://www.os-island.com")
            .header(header::AUTHORIZATION, "Bearer test-secret")
            .body(Body::empty())
            .unwrap();
        let response = test_router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "https://www.os-island.com"
        );

        let request = Request::builder()
            .uri("/api/isos/v2/version")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::ORIGIN, "https://www.os-island.com")
            .body(Body::empty())
            .unwrap();
        let response = test_router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "https://www.os-island.com"
        );
    }

    #[tokio::test]
    async fn rejects_other_origins_and_hosts() {
        for (host, origin) in [
            ("127.0.0.1:16000", "https://evil.example"),
            ("localhost:16000", "https://www.os-island.com"),
        ] {
            let request = Request::builder()
                .uri("/api/isos/v2/version")
                .header(header::HOST, host)
                .header(header::ORIGIN, origin)
                .header(header::AUTHORIZATION, "Bearer test-secret")
                .body(Body::empty())
                .unwrap();
            let response = test_router().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
    }
    #[test]
    fn diagnostics_point_at_graph_fields() {
        let graph:Deployment=serde_json::from_value(json!({"schema_version":2,"name":"default","nodes":[{"id":"eos-uds","kind":"endpoint","owned":false,"enabled":true,"profile":{"id":"eos.uds","version":"1.0.0"},"settings":{"socket_path":"/tmp/../bad"}}],"edges":[]})).unwrap();
        assert_eq!(
            diagnostic_path(&graph, "eos-uds has unsafe UDS path"),
            "/nodes/0/settings/socket_path"
        );
        let mut graph = graph;
        graph.edges.push(crate::graph::Edge {
            from: "egia".into(),
            to: "eos-uds".into(),
            kind: crate::graph::EdgeKind::DependsOn,
            input: Some("eos".into()),
            output: Some("http".into()),
        });
        assert_eq!(
            diagnostic_path(&graph, "egia -> eos-uds has incompatible typed ports"),
            "/edges/0"
        );
    }
}
