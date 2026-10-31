//! Loopback configuration API and verified static IsosUI server.
use crate::{
    component,
    config::{self, Paths},
    core_setup,
    graph::Deployment,
    lifecycle,
    log_store::LogStore,
    node_options, node_sets, packages, v2_build, v2_process,
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
use std::{fs, net::SocketAddr, path::PathBuf, sync::Arc};
use tower_http::services::{ServeDir, ServeFile};

struct App {
    paths: Paths,
    deployment: String,
}

pub fn local_port() -> Result<u16> {
    let port = std::env::var("ISOS_HTTP_PORT")
        .unwrap_or_else(|_| "16000".into())
        .parse::<u16>()?;
    anyhow::ensure!(port >= 1024, "ISOS_HTTP_PORT must be 1024..65535");
    Ok(port)
}
pub fn local_origin() -> String {
    format!("http://127.0.0.1:{}", local_port().unwrap_or(16000))
}
pub fn ui_port() -> Result<u16> {
    let port = std::env::var("ISOSUI_PORT")
        .unwrap_or_else(|_| "16001".into())
        .parse::<u16>()?;
    anyhow::ensure!(port >= 1024, "ISOSUI_PORT must be 1024..65535");
    anyhow::ensure!(
        port != local_port()?,
        "ISOSUI_PORT must differ from ISOS_HTTP_PORT"
    );
    Ok(port)
}
pub fn ui_url() -> Result<String> {
    Ok(format!("http://127.0.0.1:{}/en/isos", ui_port()?))
}
const HOSTED_ORIGINS: [&str; 2] = ["https://www.os-island.com", "https://os-island.com"];

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
fn record(app: &App, source: &str, message: &str) {
    if let Ok(logs) = LogStore::new(&app.paths, &app.deployment) {
        let _ = logs.append(source, "info", message);
    }
}
fn authorized(
    _state: &App,
    headers: &HeaderMap,
) -> std::result::Result<(), (StatusCode, Json<Value>)> {
    if headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        == Some("cross-site")
        && !headers.contains_key(header::ORIGIN)
    {
        return Err(err(
            StatusCode::FORBIDDEN,
            "cross-site request requires Origin",
        ));
    }
    Ok(())
}
fn allowed_origin(origin: &str) -> bool {
    origin == local_origin()
        || ui_port().is_ok_and(|port| origin == format!("http://127.0.0.1:{port}"))
        || HOSTED_ORIGINS.contains(&origin)
}

async fn cors(request: Request<Body>, next: Next) -> Response {
    let headers = request.headers();
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if host != format!("127.0.0.1:{}", local_port().unwrap_or(16000)) {
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
            && names
                .split(',')
                .any(|name| !matches!(name.trim().to_ascii_lowercase().as_str(), "content-type"))
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
            HeaderValue::from_static("Content-Type"),
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
            if let Some((_, field)) = message
                .split_once("settings.")
                .or_else(|| message.split_once("values."))
            {
                let field = field
                    .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
                    .next()
                    .unwrap_or("");
                if !field.is_empty() {
                    return format!("/nodes/{index}/settings/{}", field.replace('.', "/"));
                }
            }
            let suffix = if message.contains("UDS path") || message.contains("socket_path") {
                "/settings/socket_path"
            } else if message.contains("ready_tcp") {
                "/launch/ready_tcp"
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

async fn node_graph(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    v2_process::validate_deployment_name(&app.deployment)
        .map_err(|error| err(StatusCode::BAD_REQUEST, error))?;
    let bytes = match fs::read(app.paths.v2_node_graph(&app.deployment)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ok(json!({
                "revision":"",
                "graph":{"version":"2.0.0","robot":{"id":"robot","name":"Robot"},"cores":{}}
            }));
        }
        Err(error) => return Err(err(StatusCode::INTERNAL_SERVER_ERROR, error)),
    };
    let graph: Value = serde_json::from_slice(&bytes)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    ok(json!({"revision":revision(&bytes),"graph":graph}))
}

#[derive(Deserialize)]
struct NodeGraphEdit {
    schema_version: u32,
    graph: Value,
    #[serde(default)]
    revision: Option<String>,
}

async fn validate_node_graph(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(edit): Json<NodeGraphEdit>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if edit.schema_version != 2 {
        return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 2"));
    }
    let diagnostics: Vec<Value> = v2_process::validate_value(&app.paths, &edit.graph)
        .err()
        .map(|error| json!({"path":"/","code":"invalid_graph","message":error.to_string()}))
        .into_iter()
        .collect();
    ok(json!({"valid":diagnostics.is_empty(),"diagnostics":diagnostics}))
}

async fn apply_node_graph(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(edit): Json<NodeGraphEdit>,
) -> ApiResult {
    authorized(&app, &headers)?;
    v2_process::validate_deployment_name(&app.deployment)
        .map_err(|error| err(StatusCode::BAD_REQUEST, error))?;
    if edit.schema_version != 2 {
        return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 2"));
    }
    v2_process::validate_value(&app.paths, &edit.graph)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    let _lock = lifecycle::Lock::take(&app.paths)
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let path = app.paths.v2_node_graph(&app.deployment);
    let previous = fs::read(&path).ok();
    if let Some(bytes) = &previous {
        if edit.revision.as_deref() != Some(revision(bytes).as_str()) {
            return Err(err(
                StatusCode::CONFLICT,
                "node graph revision changed; reload before applying",
            ));
        }
    } else if edit
        .revision
        .as_deref()
        .is_some_and(|revision| !revision.is_empty())
    {
        return Err(err(
            StatusCode::CONFLICT,
            "node graph was removed; reload before applying",
        ));
    }
    let bytes = serde_json::to_vec_pretty(&edit.graph)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    config::atomic_write(&path, &bytes)
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    record(&app, "graph", "Applied node graph revision");
    ok(json!({"revision":revision(&bytes),"graph":edit.graph}))
}

async fn node_types(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let catalog =
        v2_process::catalog().map_err(|error| err(StatusCode::SERVICE_UNAVAILABLE, error))?;
    ok(catalog)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeOptionsRequest {
    schema_version: u32,
    graph: Value,
}

async fn node_options_for_graph(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(request): Json<NodeOptionsRequest>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if request.schema_version != 2 {
        return Err(err(StatusCode::BAD_REQUEST, "schema_version must be 2"));
    }
    let options = node_options::for_graph(&request.graph)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    ok(options)
}

async fn node_options_current(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let bytes = fs::read(app.paths.v2_node_graph(&app.deployment))
        .map_err(|error| err(StatusCode::NOT_FOUND, error))?;
    let graph: Value = serde_json::from_slice(&bytes)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    let options = node_options::for_graph(&graph)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    ok(options)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildBinariesRequest {
    schema_version: u32,
    target: String,
    #[serde(default)]
    install_deps: bool,
}

async fn build_binaries(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(request): Json<BuildBinariesRequest>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if request.schema_version != 2 || request.target != "eos" || request.install_deps {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "Rust v2 build requires schema_version 2, target eos, and install_deps false",
        ));
    }
    let paths = app.paths.clone();
    let receipt = tokio::task::spawn_blocking(move || v2_build::build_eos(&paths))
        .await
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    let suggested_graph = fs::read(app.paths.v2_node_graph(&app.deployment))
        .ok()
        .and_then(|bytes| {
            serde_json::from_slice::<Value>(&bytes)
                .ok()
                .map(|graph| (bytes, graph))
        })
        .and_then(|(bytes, graph)| {
            v2_build::suggest_graph(&graph, &receipt)
                .ok()
                .map(|graph| json!({"revision":revision(&bytes),"graph":graph}))
        });
    record(&app, "isos", "Built and staged Rust Aether executables");
    ok(json!({"receipt":receipt,"suggested_graph":suggested_graph}))
}

async fn build_binaries_status(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let selected = v2_build::selected_verified(&app.paths)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    let stale_reason = selected
        .as_ref()
        .map(|receipt| v2_build::source_drift(&app.paths, receipt))
        .transpose()
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?
        .flatten();
    let source_current = selected.is_some() && stale_reason.is_none();
    ok(json!({"selected":selected,"source_current":source_current,"stale_reason":stale_reason}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectBinariesRequest {
    schema_version: u32,
    selector: String,
    revision: String,
}

async fn select_binaries(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(request): Json<SelectBinariesRequest>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if request.schema_version != 2 {
        return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 2"));
    }
    let paths = app.paths.clone();
    let deployment = app.deployment.clone();
    let receipt = tokio::task::spawn_blocking(move || {
        v2_build::select_eos(&paths, &deployment, &request.selector, &request.revision)
    })
    .await
    .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?
    .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    let bytes = fs::read(app.paths.v2_node_graph(&app.deployment))
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let graph: Value = serde_json::from_slice(&bytes)
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    record(&app, "isos", "Selected exact Rust Aether executable set");
    ok(json!({"receipt":receipt,"graph":{"revision":revision(&bytes),"graph":graph}}))
}

async fn package_node_sets(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    ok(node_sets::list(&app.paths)
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeSetPreview {
    package: crate::graph::PackageRef,
    set_id: String,
    graph: Value,
    #[serde(default)]
    values: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    replace_legacy_mujoco: bool,
}

async fn preview_node_set(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(request): Json<NodeSetPreview>,
) -> ApiResult {
    authorized(&app, &headers)?;
    let preview = node_sets::compose_with_migration(
        &app.paths,
        &request.package.id,
        &request.package.version,
        &request.set_id,
        &request.graph,
        &request.values,
        request.replace_legacy_mujoco,
    )
    .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    v2_process::validate_value(&app.paths, &preview["graph"])
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    record(&app, "graph", "Previewed package node set");
    ok(preview)
}

async fn process_status(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let status = crate::v2_call(&app.paths, &app.deployment, "v2_status")
        .map_err(|error| err(StatusCode::SERVICE_UNAVAILABLE, error))?;
    ok(status)
}
async fn backends(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    ok(json!({"backends":crate::v2_backend::installed(&app.paths)}))
}

async fn package_list(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let installed = packages::list(&app.paths)
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    ok(json!({"installed":installed}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageRequest {
    id: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageLinkRequest {
    id: String,
    version: String,
    path: PathBuf,
    #[serde(default)]
    manifest: Option<PathBuf>,
}

async fn package_install(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(request): Json<PackageRequest>,
) -> ApiResult {
    authorized(&app, &headers)?;
    packages::package_path(&app.paths, &request.id, &request.version)
        .map_err(|error| err(StatusCode::BAD_REQUEST, error))?;
    let paths = app.paths.clone();
    let installed = tokio::task::spawn_blocking(move || -> Result<PathBuf> {
        let release = packages::catalog_release(&request.id, &request.version)?;
        anyhow::ensure!(
            release.id == request.id && release.version == request.version,
            "catalog returned a different package identity"
        );
        packages::install(&paths, &release, false, None)
    })
    .await
    .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?
    .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    record(&app, "packages", "Installed package from catalog");
    ok(
        json!({"path":installed,"installed":packages::list(&app.paths).map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR,error))?}),
    )
}

async fn package_link(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(request): Json<PackageLinkRequest>,
) -> ApiResult {
    authorized(&app, &headers)?;
    packages::package_path(&app.paths, &request.id, &request.version)
        .map_err(|error| err(StatusCode::BAD_REQUEST, error))?;
    if !request.path.is_absolute() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "package link path must be absolute",
        ));
    }
    if request
        .manifest
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "package manifest path must be absolute",
        ));
    }
    let paths = app.paths.clone();
    let linked = tokio::task::spawn_blocking(move || {
        packages::link_with_manifest(
            &paths,
            &request.id,
            &request.version,
            &request.path,
            request.manifest.as_deref(),
        )
    })
    .await
    .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?
    .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    record(&app, "packages", "Linked local package");
    ok(
        json!({"path":linked,"installed":packages::list(&app.paths).map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR,error))?}),
    )
}

async fn setup_status(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let graph_path = app.paths.v2_node_graph(&app.deployment);
    let revision = fs::read(&graph_path).ok().map(|bytes| revision(&bytes));
    let backends = crate::v2_backend::installed(&app.paths);
    let mut issues: Vec<String> = Vec::new();
    match fs::read(&graph_path) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(graph) => {
                if let Err(error) = v2_process::validate_value(&app.paths, &graph) {
                    issues.push(error.to_string());
                }
            }
            Err(error) => issues.push(error.to_string()),
        },
        Err(error) => issues.push(format!("graph is missing: {error}")),
    }
    let process = crate::v2_call(&app.paths, &app.deployment, "v2_status").ok();
    if process.is_none() {
        issues.push("IsOS process manager is not running".into());
    }
    ok(
        json!({"ready":issues.is_empty(),"issues":issues,"graph_revision":revision,"backends":backends,"process":process,"graph_path":graph_path}),
    )
}

#[derive(Deserialize)]
struct ProcessControl {
    schema_version: u32,
    operation: String,
    #[serde(default)]
    revision: Option<String>,
}

async fn process_control(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(control): Json<ProcessControl>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if control.schema_version != 2 {
        return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 2"));
    }
    let op = match control.operation.as_str() {
        "run" | "start" => "v2_start",
        "stop" => "v2_stop",
        "restart" => "v2_restart",
        _ => {
            return Err(err(
                StatusCode::BAD_REQUEST,
                "operation must be run, start, stop, or restart",
            ));
        }
    };
    let result = crate::v2_call_revision(&app.paths, &app.deployment, op, control.revision)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    ok(result)
}

async fn process_logs(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    axum::extract::Path(node): axum::extract::Path<String>,
    axum::extract::Query(query): axum::extract::Query<std::collections::BTreeMap<String, String>>,
) -> ApiResult {
    authorized(&app, &headers)?;
    crate::v2_process::validate_deployment_name(&node)
        .map_err(|error| err(StatusCode::BAD_REQUEST, error))?;
    let lines = query
        .get("lines")
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(100)
        .min(1000);
    let store = LogStore::new(&app.paths, &app.deployment)
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let selected = store
        .tail_filtered(&node, lines.max(1), None)
        .map_err(|error| err(StatusCode::NOT_FOUND, error))?;
    let lines: Vec<_> = selected
        .into_iter()
        .filter_map(|entry| entry["message"].as_str().map(str::to_owned))
        .collect();
    ok(json!({"node":node,"lines":lines}))
}

async fn logs(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<std::collections::BTreeMap<String, String>>,
) -> ApiResult {
    authorized(&app, &headers)?;
    let source = query
        .get("source")
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "source is required"))?;
    let limit = query
        .get("limit")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| err(StatusCode::BAD_REQUEST, "invalid log limit"))?
        .unwrap_or(100);
    let store = LogStore::new(&app.paths, &app.deployment)
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let data = store
        .read_filtered(
            source,
            query.get("cursor").map(String::as_str),
            limit,
            query.get("level").map(String::as_str),
        )
        .map_err(|error| {
            let message = error.to_string();
            let status = if message.contains("unavailable") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            err(status, error)
        })?;
    ok(data)
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
        if let (Ok(value), Ok(defaults)) = (
            component::call(role, "profiles", None),
            component::call(role, "defaults", None),
        ) {
            profiles.extend(component::editor_profiles(role, &value, &defaults));
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

#[derive(Deserialize)]
struct CoreSetupEdit {
    schema_version: u32,
    graph: Value,
    #[serde(default)]
    revision: Option<String>,
}

async fn core_setup_route(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    UrlPath((role, op)): UrlPath<(String, String)>,
    body: Option<Json<CoreSetupEdit>>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if !matches!(role.as_str(), "eos" | "egia")
        || !matches!(op.as_str(), "catalog" | "validate" | "render")
    {
        return Err(err(StatusCode::NOT_FOUND, "unknown core setup operation"));
    }
    if op != "catalog" && body.is_none() {
        return Err(err(StatusCode::BAD_REQUEST, "operation requires a graph"));
    }
    let (graph, revision) = if let Some(Json(edit)) = body {
        if edit.schema_version != 2 {
            return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 2"));
        }
        let revision = edit.revision.unwrap_or_else(|| {
            serde_json::to_vec(&edit.graph)
                .map(|bytes| revision(&bytes))
                .unwrap_or_default()
        });
        (edit.graph, revision)
    } else {
        let bytes = fs::read(app.paths.v2_node_graph(&app.deployment))
            .map_err(|error| err(StatusCode::NOT_FOUND, error))?;
        let graph = serde_json::from_slice(&bytes)
            .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
        (graph, revision(&bytes))
    };
    let reply = core_setup::call(&app.paths, &app.deployment, &role, &op, &graph, &revision)
        .map_err(|error| err(StatusCode::SERVICE_UNAVAILABLE, error))?;
    ok(reply)
}
pub fn serve(paths: Paths, deployment: String) -> Result<()> {
    paths.prepare()?;
    let app = Arc::new(App { paths, deployment });
    let router = router(app);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let address: SocketAddr = format!("127.0.0.1:{}", local_port()?).parse()?;
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .context("bind IsOS loopback API")?;
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                while !crate::INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst) {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            })
            .await?;
        Ok(())
    })
}

pub fn serve_ui(directory: PathBuf) -> Result<()> {
    let index = directory.join("index.html");
    anyhow::ensure!(index.is_file(), "selected IsosUI index.html is missing");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let address: SocketAddr = format!("127.0.0.1:{}", ui_port()?).parse()?;
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .context("bind IsosUI loopback server")?;
        let files = ServeDir::new(directory).fallback(ServeFile::new(index));
        axum::serve(listener, Router::new().fallback_service(files))
            .with_graceful_shutdown(async {
                while !crate::INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst) {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            })
            .await?;
        Ok(())
    })
}
fn router(app: Arc<App>) -> Router {
    let api = Router::new()
        .route("/api/isos/v2/version", get(version).options(preflight))
        .route(
            "/api/isos/v2/node-types",
            get(node_types).options(preflight),
        )
        .route(
            "/api/isos/v2/build-binaries",
            get(build_binaries_status)
                .post(build_binaries)
                .options(preflight),
        )
        .route(
            "/api/isos/v2/build-binaries/select",
            post(select_binaries).options(preflight),
        )
        .route(
            "/api/isos/v2/node-options",
            get(node_options_current)
                .post(node_options_for_graph)
                .options(preflight),
        )
        .route(
            "/api/isos/v2/node-sets",
            get(package_node_sets).options(preflight),
        )
        .route(
            "/api/isos/v2/node-sets/preview",
            post(preview_node_set).options(preflight),
        )
        .route(
            "/api/isos/v2/process-status",
            get(process_status).options(preflight),
        )
        .route("/api/isos/v2/backends", get(backends).options(preflight))
        .route(
            "/api/isos/v2/packages",
            get(package_list).options(preflight),
        )
        .route(
            "/api/isos/v2/packages/install",
            post(package_install).options(preflight),
        )
        .route(
            "/api/isos/v2/packages/link",
            post(package_link).options(preflight),
        )
        .route(
            "/api/isos/v2/setup-status",
            get(setup_status).options(preflight),
        )
        .route(
            "/api/isos/v2/process-control",
            post(process_control).options(preflight),
        )
        .route(
            "/api/isos/v2/process-logs/{node}",
            get(process_logs).options(preflight),
        )
        .route("/api/isos/v2/logs", get(logs).options(preflight))
        .route(
            "/api/isos/v2/node-graph",
            get(node_graph).put(apply_node_graph).options(preflight),
        )
        .route(
            "/api/isos/v2/node-graph/validate",
            post(validate_node_graph).options(preflight),
        )
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
        .route(
            "/api/isos/v2/core-setup/{role}/{op}",
            get(core_setup_route)
                .post(core_setup_route)
                .options(preflight),
        )
        .route_layer(middleware::from_fn(cors));
    Router::new().merge(api).with_state(app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use tower::ServiceExt;

    #[tokio::test]
    async fn core_setup_api_rejects_unknown_role_and_missing_graph() {
        for (route, expected) in [
            (
                "/api/isos/v2/core-setup/other/catalog",
                StatusCode::NOT_FOUND,
            ),
            (
                "/api/isos/v2/core-setup/eos/validate",
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let request = Request::builder()
                .uri(route)
                .header(header::HOST, "127.0.0.1:16000")
                .body(Body::empty())
                .unwrap();
            let response = test_router().oneshot(request).await.unwrap();
            assert_eq!(response.status(), expected);
        }
    }

    #[tokio::test]
    async fn binary_build_api_rejects_legacy_dependency_mode() {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/isos/v2/build-binaries")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"schema_version":2,"target":"eos","install_deps":true}"#,
            ))
            .unwrap();
        let response = test_router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1000).await.unwrap()).unwrap();
        assert_eq!(body["ok"], false);
    }

    fn test_router() -> Router {
        router(Arc::new(App {
            paths: Paths {
                config: PathBuf::from("/tmp/isos-test/config"),
                data: PathBuf::from("/tmp/isos-test/data"),
                runtime: PathBuf::from("/tmp/isos-test/run"),
            },
            deployment: "default".into(),
        }))
    }

    #[tokio::test]
    async fn categorized_logs_return_bounded_cursor_envelope() {
        let temp = tempfile::tempdir().unwrap();
        let app = Arc::new(App {
            paths: Paths {
                config: temp.path().join("config"),
                data: temp.path().join("data"),
                runtime: temp.path().join("run"),
            },
            deployment: "default".into(),
        });
        let store = LogStore::new(&app.paths, "default").unwrap();
        store.append("isos", "info", "ready").unwrap();
        for (route, expected) in [
            ("/api/isos/v2/logs?source=isos&limit=1", StatusCode::OK),
            (
                "/api/isos/v2/logs?source=isos&limit=201",
                StatusCode::BAD_REQUEST,
            ),
            (
                "/api/isos/v2/logs?source=..%2Fsecret",
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let request = Request::builder()
                .uri(route)
                .header(header::HOST, "127.0.0.1:16000")
                .body(Body::empty())
                .unwrap();
            let response = router(app.clone()).oneshot(request).await.unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::OK {
                let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(body["data"]["lines"][0]["message"], "ready");
                assert_eq!(body["data"]["lines"][0]["source"], "isos");
                assert!(body["data"]["next_cursor"].is_string());
            }
        }
    }

    #[tokio::test]
    async fn logs_filter_level_and_legacy_process_view_reads_unified_records() {
        let temp = tempfile::tempdir().unwrap();
        let app = Arc::new(App {
            paths: Paths {
                config: temp.path().join("config"),
                data: temp.path().join("data"),
                runtime: temp.path().join("run"),
            },
            deployment: "default".into(),
        });
        let store = LogStore::new(&app.paths, "default").unwrap();
        store.append("eos", "info", "ready").unwrap();
        store.append("eos", "error", "failed").unwrap();
        for (route, status, expected) in [
            (
                "/api/isos/v2/logs?source=eos&level=warn",
                StatusCode::OK,
                "failed",
            ),
            (
                "/api/isos/v2/process-logs/eos?lines=1",
                StatusCode::OK,
                "failed",
            ),
            (
                "/api/isos/v2/logs?source=eos&level=verbose",
                StatusCode::BAD_REQUEST,
                "",
            ),
        ] {
            let request = Request::builder()
                .uri(route)
                .header(header::HOST, "127.0.0.1:16000")
                .body(Body::empty())
                .unwrap();
            let response = router(app.clone()).oneshot(request).await.unwrap();
            assert_eq!(response.status(), status);
            if status == StatusCode::OK {
                let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(body["data"]["lines"].as_array().unwrap().len(), 1);
                let line = &body["data"]["lines"][0];
                assert_eq!(
                    line["message"].as_str().or_else(|| line.as_str()),
                    Some(expected)
                );
            }
        }
    }

    #[tokio::test]
    async fn first_node_graph_read_returns_editable_v2_draft() {
        let temp = tempfile::tempdir().unwrap();
        let app = Arc::new(App {
            paths: Paths {
                config: temp.path().join("config"),
                data: temp.path().join("data"),
                runtime: temp.path().join("run"),
            },
            deployment: "default".into(),
        });
        let request = Request::builder()
            .uri("/api/isos/v2/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::ORIGIN, "https://www.os-island.com")
            .body(Body::empty())
            .unwrap();
        let response = router(app).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["data"]["revision"], "");
        assert_eq!(body["data"]["graph"]["version"], "2.0.0");
    }

    #[tokio::test]
    async fn build_status_reports_unselected_state_without_error() {
        let temp = tempfile::tempdir().unwrap();
        let app = Arc::new(App {
            paths: Paths {
                config: temp.path().join("config"),
                data: temp.path().join("data"),
                runtime: temp.path().join("run"),
            },
            deployment: "default".into(),
        });
        let request = Request::builder()
            .uri("/api/isos/v2/build-binaries")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::ORIGIN, "https://www.os-island.com")
            .body(Body::empty())
            .unwrap();
        let response = router(app).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["data"]["selected"], Value::Null);
        assert_eq!(body["data"]["source_current"], false);
        assert_eq!(body["data"]["stale_reason"], Value::Null);
    }

    #[tokio::test]
    async fn home_reports_missing_package_and_graph() {
        let temp = tempfile::tempdir().unwrap();
        let app = Arc::new(App {
            paths: Paths {
                config: temp.path().join("config"),
                data: temp.path().join("data"),
                runtime: temp.path().join("run"),
            },
            deployment: "default".into(),
        });
        for route in ["/api/isos/v2/backends", "/api/isos/v2/setup-status"] {
            let request = Request::builder()
                .uri(route)
                .header(header::HOST, "127.0.0.1:16000")
                .header(header::ORIGIN, "https://www.os-island.com")
                .body(Body::empty())
                .unwrap();
            let response = router(app.clone()).oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            if route.ends_with("backends") {
                assert_eq!(body["data"]["backends"], json!([]));
            } else {
                assert_eq!(body["data"]["ready"], false);
                assert!(body["data"]["issues"].as_array().unwrap().len() >= 2);
                assert!(
                    !body["data"]["issues"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|issue| issue
                            .as_str()
                            .is_some_and(|message| message.contains("MuJoCo backend package")))
                );
            }
        }
    }

    #[tokio::test]
    async fn package_link_is_listed_and_rejects_duplicate_version() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = temp.path().join("bundle");
        fs::create_dir(&bundle).unwrap();
        let app = Arc::new(App {
            paths: Paths {
                config: temp.path().join("config"),
                data: temp.path().join("data"),
                runtime: temp.path().join("run"),
            },
            deployment: "default".into(),
        });
        app.paths.prepare().unwrap();
        let invalid = json!({"id":"island/mujoco-sim","version":"2.0.0","path":bundle,"manifest":"relative.json"});
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/isos/v2/packages/link")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(invalid.to_string()))
            .unwrap();
        assert_eq!(
            router(app.clone()).oneshot(request).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
        let body = json!({"id":"island/mujoco-sim","version":"2.0.0","path":bundle});
        for expected in [StatusCode::OK, StatusCode::UNPROCESSABLE_ENTITY] {
            let request = Request::builder()
                .method(Method::POST)
                .uri("/api/isos/v2/packages/link")
                .header(header::HOST, "127.0.0.1:16000")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            assert_eq!(
                router(app.clone()).oneshot(request).await.unwrap().status(),
                expected
            );
        }
        let request = Request::builder()
            .uri("/api/isos/v2/packages")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        let response = router(app).oneshot(request).await.unwrap();
        let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["data"]["installed"][0]["version"], "2.0.0");
        assert_eq!(body["data"]["installed"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn loopback_and_trusted_browser_origins_connect_without_token() {
        let request = Request::builder()
            .method(Method::OPTIONS)
            .uri("/api/isos/v2/version")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::ORIGIN, "https://www.os-island.com")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
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
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "https://www.os-island.com"
        );

        for headers in [
            [("origin", "http://127.0.0.1:16000")],
            [("origin", "http://127.0.0.1:16001")],
            [("sec-fetch-site", "same-origin")],
        ] {
            let mut request = Request::builder()
                .uri("/api/isos/v2/version")
                .header(header::HOST, "127.0.0.1:16000");
            for (name, value) in headers {
                request = request.header(name, value);
            }
            let response = test_router()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = Request::builder()
            .uri("/api/isos/v2/version")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            test_router().oneshot(request).await.unwrap().status(),
            StatusCode::OK
        );
        let request = Request::builder()
            .uri("/api/isos/v2/version")
            .header(header::HOST, "127.0.0.1:16000")
            .header("sec-fetch-site", "cross-site")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            test_router().oneshot(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn rejects_other_origins_and_hosts() {
        for (host, origin) in [
            ("127.0.0.1:16000", "https://evil.example"),
            ("127.0.0.1:16000", "http://127.0.0.1:16002"),
            ("localhost:16000", "https://www.os-island.com"),
        ] {
            let request = Request::builder()
                .uri("/api/isos/v2/version")
                .header(header::HOST, host)
                .header(header::ORIGIN, origin)
                .body(Body::empty())
                .unwrap();
            let response = test_router().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
    }
    #[tokio::test]
    async fn api_does_not_serve_frontend_pages() {
        for route in ["/", "/en/isos", "/assets/app.js"] {
            let response = test_router()
                .oneshot(
                    Request::builder()
                        .uri(route)
                        .header(header::HOST, "127.0.0.1:16000")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }
    #[test]
    fn diagnostics_point_at_graph_fields() {
        let graph:Deployment=serde_json::from_value(json!({"schema_version":2,"name":"default","nodes":[{"id":"eos-uds","kind":"endpoint","owned":false,"enabled":true,"profile":{"id":"eos.uds","version":"1.0.0"},"settings":{"socket_path":"/tmp/../bad"}}],"edges":[]})).unwrap();
        assert_eq!(
            diagnostic_path(&graph, "eos-uds has unsafe UDS path"),
            "/nodes/0/settings/socket_path"
        );
        assert_eq!(
            diagnostic_path(&graph, "eos-uds: settings.socket_path invalid"),
            "/nodes/0/settings/socket_path"
        );
        assert_eq!(
            diagnostic_path(&graph, "eos-uds: launch.ready_tcp mismatch"),
            "/nodes/0/launch/ready_tcp"
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
