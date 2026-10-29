//! Loopback configuration API and standalone Island-web bundle.
use crate::{
    component,
    config::{self, Paths},
    graph::Deployment,
    lifecycle,
};
use anyhow::{Context, Result};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path as UrlPath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, net::SocketAddr, os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc};

#[derive(Clone)]
struct App {
    paths: Paths,
    deployment: String,
    token: String,
    web: PathBuf,
}

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
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if host != "127.0.0.1:16000" {
        return Err(err(StatusCode::FORBIDDEN, "invalid Host"));
    }
    if let Some(origin) = headers.get(header::ORIGIN)
        && origin.to_str().ok() != Some("http://127.0.0.1:16000")
    {
        return Err(err(StatusCode::FORBIDDEN, "invalid Origin"));
    }
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());
    if bearer != Some(format!("Bearer {}", state.token).as_str()) {
        return Err(err(StatusCode::UNAUTHORIZED, "invalid bearer token"));
    }
    Ok(())
}
fn revision(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
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
        .map(|e| json!({"path":"/","code":"invalid_graph","message":e.to_string()}));
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
    let path = component::binary(&role);
    let mut command = std::process::Command::new(path);
    command.args([&op, "--json"]);
    if let Some(Json(payload)) = body {
        use std::io::Write;
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|e| err(StatusCode::SERVICE_UNAVAILABLE, e))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(payload.to_string().as_bytes())
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
        let value: Value =
            serde_json::from_slice(&output.stdout).map_err(|e| err(StatusCode::BAD_GATEWAY, e))?;
        return ok(value);
    }
    let output = command
        .output()
        .map_err(|e| err(StatusCode::SERVICE_UNAVAILABLE, e))?;
    let value: Value =
        serde_json::from_slice(&output.stdout).map_err(|e| err(StatusCode::BAD_GATEWAY, e))?;
    ok(value)
}
async fn static_file(State(app): State<Arc<App>>, UrlPath(path): UrlPath<String>) -> Response {
    let relative = path.trim_start_matches('/');
    let candidate = app.web.join(relative);
    let file =
        if relative.is_empty() || !candidate.is_file() || relative.split('/').any(|p| p == "..") {
            app.web.join("index.html")
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
    Ok(format!(
        "http://127.0.0.1:16000/en/isos#token={}",
        token(paths)?
    ))
}
pub fn serve(paths: Paths, deployment: String) -> Result<()> {
    paths.prepare()?;
    let web = std::env::var_os("ISOS_WEB_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.data.join("ui"));
    let app = Arc::new(App {
        token: token(&paths)?,
        paths,
        deployment,
        web,
    });
    let router = Router::new()
        .route("/api/isos/v2/graph", get(graph).put(apply))
        .route("/api/isos/v2/graph/validate", post(validate))
        .route("/api/isos/v2/profiles", get(profiles))
        .route(
            "/api/isos/v2/config/{role}/{op}",
            get(component_route).post(component_route),
        )
        .route("/", get(static_file_root))
        .route("/{*path}", get(static_file))
        .with_state(app);
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
async fn static_file_root(State(app): State<Arc<App>>) -> Response {
    static_file(State(app), UrlPath(String::new())).await
}
