//! Loopback configuration API and verified static IsosUI server.
use crate::{
    config::{self, Paths},
    core_setup, graph_compat, graph_migration, lock,
    log_store::LogStore,
    node_options, node_sets, packages, process, readiness,
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
        Json(json!({"schema_version":1,"ok":false,"data":null,"error":message.to_string()})),
    )
}
fn package_err(error: &anyhow::Error, id: &str, version: &str) -> (StatusCode, Json<Value>) {
    let failure = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<packages::PackageFailure>());
    let diagnostic = failure.map_or_else(|| json!({"code":"package_invalid_manifest","path":"",
        "message":format!("Reinstall or relink {id}@{version}"),"id":id,"version":version,"stage":"manifest"}),
        packages::PackageFailure::diagnostic);
    let status = if diagnostic["code"] == "package_version_exists" {
        StatusCode::CONFLICT
    } else {
        StatusCode::UNPROCESSABLE_ENTITY
    };
    (
        status,
        Json(
            json!({"schema_version":1,"ok":false,"data":{"diagnostics":[diagnostic]},
        "error":format!("Reinstall or relink {id}@{version}")}),
        ),
    )
}
fn admission_err(error: &anyhow::Error) -> (StatusCode, Json<Value>) {
    if let Some(failure) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<packages::PackageFailure>())
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"schema_version":1,"ok":false,
                "data":{"diagnostics":[failure.diagnostic()]},"error":failure.to_string()})),
        );
    }
    err(StatusCode::UNPROCESSABLE_ENTITY, error)
}
fn admission_diagnostic(error: &anyhow::Error) -> Value {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<packages::PackageFailure>())
        .map_or_else(
            || json!({"path":"/","code":"invalid_graph","message":error.to_string()}),
            packages::PackageFailure::diagnostic,
        )
}
fn ok(data: Value) -> ApiResult {
    Ok(Json(
        json!({"schema_version":1,"ok":true,"data":data,"error":null}),
    ))
}
fn record(app: &App, source: &str, message: &str) {
    if let Ok(logs) = LogStore::new(&app.paths, &app.deployment) {
        let _ = logs.append(source, "info", message);
    }
}
fn record_package(
    app: &App,
    level: &str,
    code: &str,
    id: &str,
    version: &str,
    source_kind: &str,
    path: Option<&std::path::Path>,
) {
    if let Ok(logs) = LogStore::new(&app.paths, &app.deployment) {
        let receipt = path
            .and_then(|path| fs::read(path.join("receipt.json")).ok())
            .and_then(|bytes| serde_json::from_slice::<packages::Receipt>(&bytes).ok());
        let correlation = crate::manager::next_correlation_id();
        let _ = logs.append_event_context("packages", level, code,
            if level == "error" { "Package verification or installation failed" } else if code == "package_verified" { "Verified package release recorded" } else { "Local package link recorded without a release manifest" },
            crate::log_store::LogContext { run_id: Some(crate::manager::manager_session_id()), correlation_id: Some(&correlation),
            fields: json!({"package_id":id,"package_version":version,
                "source_commit":receipt.as_ref().and_then(|receipt| receipt.source_commit.as_deref()),
                "asset_sha256":receipt.as_ref().and_then(|receipt| receipt.asset_sha256.as_deref()),
                "release_sha256":receipt.as_ref().and_then(|receipt| receipt.release_sha256.as_deref()),
                "payload_sha256":receipt.as_ref().and_then(|receipt| receipt.payload_sha256.as_deref()),
                "source_kind":source_kind}) });
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
    ok(json!({"api_schema_version":1,"isos_version":env!("CARGO_PKG_VERSION")}))
}
fn revision(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn edge_diagnostics(graph: &Value, catalog: &Value) -> Vec<Value> {
    if graph["version"] != "1.0.0" {
        return Vec::new();
    }
    let Some(types) = catalog["types"].as_array() else {
        return Vec::new();
    };
    let definitions: std::collections::BTreeMap<&str, &Value> = types
        .iter()
        .filter_map(|definition| Some((definition["id"].as_str()?, definition)))
        .collect();
    let mut nodes = std::collections::BTreeMap::new();
    let Some(cores) = graph["cores"].as_object() else {
        return Vec::new();
    };
    for (core_id, core) in cores {
        let Some(layers) = core["layers"].as_object() else {
            continue;
        };
        for (layer_id, layer) in layers {
            let Some(groups) = layer["groups"].as_object() else {
                continue;
            };
            for (group_id, group) in groups {
                let Some(entries) = group["nodes"].as_object() else {
                    continue;
                };
                for (node_id, node) in entries {
                    nodes.insert(format!("{core_id}/{layer_id}/{group_id}/{node_id}"), node);
                }
            }
        }
    }
    let mut diagnostics = Vec::new();
    for (path, node) in &nodes {
        let Some(definition) = node["node_type"]
            .as_str()
            .and_then(|id| definitions.get(id))
        else {
            continue;
        };
        let Some(inputs) = node["inputs"].as_object() else {
            continue;
        };
        for (input_name, source_value) in inputs {
            let Some(input_type) = definition["inputs"]
                .as_array()
                .and_then(|ports| {
                    ports
                        .iter()
                        .find(|port| port["name"] == input_name.as_str())
                })
                .and_then(|port| port["data_type"].as_str())
            else {
                continue;
            };
            let sources: Vec<&Value> = source_value
                .as_array()
                .map_or_else(|| vec![source_value], |values| values.iter().collect());
            for (index, source) in sources.iter().enumerate() {
                let Some(source_path) = source["node"].as_str() else {
                    continue;
                };
                let Some(source_node) = nodes.get(source_path) else {
                    continue;
                };
                let Some(source_definition) = source_node["node_type"]
                    .as_str()
                    .and_then(|id| definitions.get(id))
                else {
                    continue;
                };
                let Some(output_name) = source["output"].as_str() else {
                    continue;
                };
                let Some(output_type) = source_definition["outputs"]
                    .as_array()
                    .and_then(|ports| ports.iter().find(|port| port["name"] == output_name))
                    .and_then(|port| port["data_type"].as_str())
                else {
                    continue;
                };
                if input_type != output_type {
                    diagnostics.push(json!({
                        "path":format!("/cores/{}/layers/{}/groups/{}/nodes/{}/inputs/{input_name}/{index}",
                            path.split('/').next().unwrap_or(""), path.split('/').nth(1).unwrap_or(""),
                            path.split('/').nth(2).unwrap_or(""), path.split('/').nth(3).unwrap_or("")),
                        "code":"edge_type_mismatch",
                        "message":format!("{source_path}.{output_name} ({output_type}) cannot feed {path}.{input_name} ({input_type}); compare the registered data type and its unit schema"),
                        "source":source_path,"target":path,"input":input_name
                    }));
                }
            }
        }
    }
    diagnostics
}
async fn node_graph(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    process::validate_deployment_name(&app.deployment)
        .map_err(|error| err(StatusCode::BAD_REQUEST, error))?;
    let bytes = match fs::read(app.paths.node_graph_path(&app.deployment)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ok(json!({
                "revision":"",
                "graph":crate::starter::draft(&app.paths, &app.deployment)
                    .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?,
                "read_only":false,"diagnostics":[]
            }));
        }
        Err(error) => return Err(err(StatusCode::INTERNAL_SERVER_ERROR, error)),
    };
    let saved = graph_compat::parse(&bytes)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    let applied = app
        .paths
        .generated(&app.deployment)
        .join("core-setup/launch-inputs.json")
        .is_file();
    let catalog = process::catalog().ok();
    let projection =
        graph_compat::legacy_projection(&saved.value, &saved.revision, catalog.as_ref());
    let editable_starter = !applied
        && saved.value["version"] == "1.0.0"
        && projection.as_ref().is_ok_and(Option::is_some);
    let editable_new = saved.value["version"] == "0.1.0"
        && graph_compat::admission_diagnostic(&saved.value, catalog.as_ref()).is_none();
    let mut diagnostics = saved.diagnostics;
    if let Err(problem) = &projection
        && diagnostics.is_empty()
    {
        diagnostics.push(problem.clone());
    }
    let mut data = json!({"revision":saved.revision,"graph":saved.value,
        "read_only":saved.read_only && !editable_starter && !editable_new,"diagnostics":diagnostics});
    if let Ok(Some(projection)) = projection {
        data["legacy_projection"] = json!(projection);
    }
    ok(data)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrationRequest {
    schema_version: u32,
    revision: String,
    #[serde(default)]
    proposed_revision: Option<String>,
}

fn migration_error(error: graph_migration::MigrationError) -> (StatusCode, Json<Value>) {
    match error {
        graph_migration::MigrationError::Conflict(message) => err(StatusCode::CONFLICT, message),
        graph_migration::MigrationError::Diagnostic(diagnostic) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"schema_version":1,"ok":false,
                "data":{"diagnostics":[diagnostic]},"error":"migration rejected"})),
        ),
        graph_migration::MigrationError::Generated(diagnostic) => {
            let status = if diagnostic.code == "generated_commit_uncertain" {
                StatusCode::INTERNAL_SERVER_ERROR
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            (
                status,
                Json(json!({"schema_version":1,"ok":false,
                "data":{"diagnostics":[diagnostic]},"error":"generated configuration rejected"})),
            )
        }
        graph_migration::MigrationError::Filesystem(error) => {
            err(StatusCode::INTERNAL_SERVER_ERROR, error)
        }
        graph_migration::MigrationError::OutcomeUncertain => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"schema_version":1,"ok":false,
                "data":{"diagnostics":[crate::generated_v2::commit_uncertain()]},
                "error":"generated configuration publication is uncertain"})),
        ),
    }
}

async fn migration_preview(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(request): Json<MigrationRequest>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if request.schema_version != 1 || request.proposed_revision.is_some() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "preview requires schema_version 1 and revision only",
        ));
    }
    let preview = graph_migration::preview(&app.paths, &app.deployment, &request.revision)
        .map_err(migration_error)?;
    let diagnostics: Vec<_> = preview.diagnostic.into_iter().collect();
    ok(
        json!({"revision":request.revision,"proposed_revision":preview.proposal.revision,
        "graph":preview.proposal.graph,"changes":preview.proposal.changes,
        "can_apply":diagnostics.is_empty(),"diagnostics":diagnostics}),
    )
}

async fn migration_apply(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(request): Json<MigrationRequest>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if request.schema_version != 1 {
        return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 1"));
    }
    let proposed_revision = request
        .proposed_revision
        .as_deref()
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "proposed_revision is required"))?;
    let proposal = graph_migration::commit(
        &app.paths,
        &app.deployment,
        &request.revision,
        proposed_revision,
    )
    .map_err(migration_error)?;
    let active_revision =
        crate::manager::manager_call(&app.paths, &app.deployment, "deployment_status")
            .ok()
            .and_then(|status| status.get("active_revision").cloned());
    record(&app, "graph", "Migrated saved node graph revision");
    ok(json!({"revision":proposal.revision,"graph":proposal.graph,
        "active_revision":active_revision}))
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
    if edit.schema_version != 1 {
        return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 1"));
    }
    if edit.graph["version"] == "0.1.0" {
        let bytes = serde_json::to_vec_pretty(&edit.graph)
            .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
        let proposal = graph_migration::Proposal {
            revision: graph_compat::revision(&bytes),
            graph: edit.graph,
            bytes,
            changes: Vec::new(),
        };
        return match graph_migration::prepare(&app.paths, &app.deployment, &proposal) {
            Ok(prepared) => ok(json!({"valid":true,"diagnostics":[],"edges":prepared.edges})),
            Err(diagnostic) => ok(json!({"valid":false,"diagnostics":[diagnostic]})),
        };
    }
    if let Err(error) = packages::verify_graph_packages(&app.paths, &edit.graph) {
        return ok(
            json!({"valid":false,"diagnostics":[graph_migration::package_diagnostic(&error)]}),
        );
    }
    let catalog = process::catalog().ok();
    if let Some(diagnostic) = graph_compat::legacy_mapping_diagnostic(&edit.graph, catalog.as_ref())
    {
        return ok(json!({"valid":false,"diagnostics":[diagnostic]}));
    }
    let edge_diagnostics = process::catalog()
        .map(|catalog| edge_diagnostics(&edit.graph, &catalog))
        .unwrap_or_default();
    let diagnostics: Vec<Value> = if edge_diagnostics.is_empty() {
        process::validate_value(&app.paths, &edit.graph)
            .err()
            .map(|error| admission_diagnostic(&error))
            .into_iter()
            .collect()
    } else {
        edge_diagnostics
    };
    ok(json!({"valid":diagnostics.is_empty(),"diagnostics":diagnostics}))
}

async fn apply_node_graph(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(edit): Json<NodeGraphEdit>,
) -> ApiResult {
    authorized(&app, &headers)?;
    process::validate_deployment_name(&app.deployment)
        .map_err(|error| err(StatusCode::BAD_REQUEST, error))?;
    if edit.schema_version != 1 {
        return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 1"));
    }
    let graph = edit.graph;
    if graph["version"] == "0.1.0" {
        let expected = edit.revision.as_deref().ok_or_else(|| {
            err(
                StatusCode::BAD_REQUEST,
                "0.1.0 Apply requires an explicit revision, including an empty revision for a new graph",
            )
        })?;
        let (proposal, _) =
            graph_migration::apply_new(&app.paths, &app.deployment, graph, expected)
                .map_err(migration_error)?;
        record(&app, "graph", "Applied node graph revision");
        return ok(json!({"revision":proposal.revision,"graph":proposal.graph}));
    }
    if let Err(error) = packages::verify_graph_packages(&app.paths, &graph) {
        let diagnostic = graph_migration::package_diagnostic(&error);
        let message = diagnostic.message.clone();
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(
                json!({"schema_version":1,"ok":false,"data":{"diagnostics":[diagnostic]},"error":message}),
            ),
        ));
    }
    let catalog = process::catalog().ok();
    if let Some(diagnostic) = graph_compat::legacy_mapping_diagnostic(&graph, catalog.as_ref()) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"schema_version":1,"ok":false,
                "data":{"diagnostics":[diagnostic]},"error":"graph admission rejected"})),
        ));
    }
    let path = app.paths.node_graph_path(&app.deployment);
    {
        let _lock = lock::Lock::take(&app.paths)
            .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
        if path.exists() {
            let saved = graph_compat::read(&path)
                .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
            if edit.revision.as_deref() != Some(saved.revision.as_str()) {
                return Err(err(
                    StatusCode::CONFLICT,
                    "node graph revision changed; reload before applying",
                ));
            }
            if saved.value["version"] == "1.0.0" {
                if let Some(diagnostic) =
                    graph_compat::projected_input_diagnostic(&saved, &graph, catalog.as_ref())
                {
                    return Err((
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({"schema_version":1,"ok":false,
                            "data":{"diagnostics":[diagnostic]},"error":"projection is read-only"})),
                    ));
                }
                if graph_compat::changed_legacy_diagnostic(&saved, &graph).is_none() {
                    process::validate_value(&app.paths, &saved.value)
                        .map_err(|error| admission_err(&error))?;
                    let rendered = core_setup::render_graph(
                        &app.paths,
                        &app.deployment,
                        &saved.value,
                        &saved.revision,
                    )
                    .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
                    core_setup::write_rendered(&app.paths, &app.deployment, rendered)
                        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
                    return ok(json!({"revision":saved.revision,"graph":saved.value}));
                }
                if graph_compat::requires_migration(
                    &saved,
                    &graph,
                    app.paths
                        .generated(&app.deployment)
                        .join("core-setup/launch-inputs.json")
                        .is_file(),
                ) {
                    let diagnostic = graph_compat::changed_legacy_diagnostic(&saved, &graph)
                        .expect("changed legacy graph has migration diagnostic");
                    return Err((
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({"schema_version":1,"ok":false,
                            "data":{"diagnostics":[diagnostic]},"error":"migration required"})),
                    ));
                }
            }
            if saved.value["version"] != "1.0.0" {
                let diagnostic =
                    saved
                        .diagnostics
                        .into_iter()
                        .next()
                        .unwrap_or(graph_compat::Diagnostic {
                            path: "/version".into(),
                            code: "migration_required",
                            message: "saved graph requires a compatible migration writer".into(),
                            context: std::collections::BTreeMap::new(),
                        });
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({"schema_version":1,"ok":false,
                        "data":{"diagnostics":[diagnostic]},"error":"saved graph is read-only"})),
                ));
            }
        }
    }
    if graph["version"] != "1.0.0" {
        return Err(err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "0.1.0 writer is unavailable until core readers advertise support",
        ));
    }
    if let Ok(catalog) = process::catalog()
        && let Some(diagnostic) = edge_diagnostics(&graph, &catalog).into_iter().next()
    {
        return Err(err(
            StatusCode::UNPROCESSABLE_ENTITY,
            diagnostic["message"]
                .as_str()
                .unwrap_or("edge type mismatch"),
        ));
    }
    process::validate_value(&app.paths, &graph).map_err(|error| admission_err(&error))?;
    let bytes = serde_json::to_vec_pretty(&graph)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    let next_revision = revision(&bytes);
    let rendered = core_setup::render_graph(&app.paths, &app.deployment, &graph, &next_revision)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    let _lock = lock::Lock::take(&app.paths)
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    if let Err(error) = packages::verify_graph_packages(&app.paths, &graph) {
        let diagnostic = graph_migration::package_diagnostic(&error);
        let message = diagnostic.message.clone();
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"schema_version":1,"ok":false,
                "data":{"diagnostics":[diagnostic]},"error":message})),
        ));
    }
    let path = app.paths.node_graph_path(&app.deployment);
    let previous = fs::read(&path).ok();
    if let Some(bytes) = &previous {
        if edit.revision.as_deref() != Some(revision(bytes).as_str()) {
            return Err(err(
                StatusCode::CONFLICT,
                "node graph revision changed; reload before applying",
            ));
        }
        if app
            .paths
            .generated(&app.deployment)
            .join("core-setup/launch-inputs.json")
            .is_file()
        {
            let saved = graph_compat::parse(bytes)
                .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
            if graph_compat::requires_migration(&saved, &graph, true) {
                return Err(err(
                    StatusCode::CONFLICT,
                    "graph became applied during editing; reload before migration",
                ));
            }
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
    config::atomic_write(&path, &bytes)
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    if let Err(error) = core_setup::write_rendered(&app.paths, &app.deployment, rendered) {
        if let Some(previous) = previous {
            let _ = config::atomic_write(&path, &previous);
        } else {
            let _ = fs::remove_file(&path);
        }
        return Err(err(StatusCode::INTERNAL_SERVER_ERROR, error));
    }
    if let Err(error) = crate::cli::clear_saved_setup_error(&app.paths, &app.deployment) {
        tracing::warn!("saved graph applied, but could not clear prior setup error: {error}");
    }
    record(&app, "graph", "Applied node graph revision");
    ok(json!({"revision":next_revision,"graph":graph}))
}

async fn node_types(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let catalog =
        process::catalog().map_err(|error| err(StatusCode::SERVICE_UNAVAILABLE, error))?;
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
    if request.schema_version != 1 {
        return Err(err(StatusCode::BAD_REQUEST, "schema_version must be 1"));
    }
    let options = node_options::for_graph(&request.graph)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    ok(options)
}

async fn node_options_current(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let bytes = fs::read(app.paths.node_graph_path(&app.deployment))
        .map_err(|error| err(StatusCode::NOT_FOUND, error))?;
    let graph: Value = serde_json::from_slice(&bytes)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    let options = node_options::for_graph(&graph)
        .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    ok(options)
}

async fn core_readiness(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let status =
        crate::manager::manager_call(&app.paths, &app.deployment, "deployment_status").ok();
    ok(readiness::report(
        &app.paths,
        &app.deployment,
        status.as_ref(),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewLinkRequest {
    path: PathBuf,
    manifest: Option<PathBuf>,
}

async fn preview_link(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(request): Json<PreviewLinkRequest>,
) -> ApiResult {
    authorized(&app, &headers)?;
    if !request.path.is_absolute() || !request.path.is_dir() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "bundle path must be an absolute directory",
        ));
    }
    let manifest_path = request
        .manifest
        .unwrap_or_else(|| request.path.join("release.json"));
    if !manifest_path.is_absolute() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "manifest path must be absolute",
        ));
    }
    let manifest = packages::inspect_link_candidate(&request.path, &manifest_path)
        .map_err(|error| package_err(&error, "unknown/package", "unknown"))?;
    let mut issues = Vec::new();
    if manifest.kind != "backend" {
        issues.push("release manifest kind must be backend".to_owned());
    }
    if crate::packages::package_path(&app.paths, &manifest.id, &manifest.version)
        .is_ok_and(|path| path.exists())
    {
        issues.push("this package version is already linked or installed".to_owned());
    }
    ok(
        json!({"id":manifest.id,"version":manifest.version,"ready":issues.is_empty(),"issues":issues}),
    )
}

async fn package_node_sets(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    ok(node_sets::list(&app.paths)
        .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeSetPreview {
    package: crate::backend::PackageIdentity,
    set_id: String,
    graph: Value,
    #[serde(default)]
    values: std::collections::BTreeMap<String, String>,
}

async fn preview_node_set(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(request): Json<NodeSetPreview>,
) -> ApiResult {
    authorized(&app, &headers)?;
    let preview = node_sets::compose(
        &app.paths,
        &request.package.id,
        &request.package.version,
        &request.set_id,
        &request.graph,
        &request.values,
    )
    .map_err(|error| err(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    process::validate_value(&app.paths, &preview["graph"])
        .map_err(|error| admission_err(&error))?;
    record(&app, "graph", "Previewed package node set");
    ok(preview)
}

async fn process_status(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let status = crate::manager::manager_call(&app.paths, &app.deployment, "deployment_status")
        .map_err(|error| err(StatusCode::SERVICE_UNAVAILABLE, error))?;
    ok(status)
}
async fn backends(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    ok(json!({"backends":crate::backend::installed(&app.paths)}))
}

async fn package_list(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let installed = packages::listing(&app.paths)
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
    let id = request.id.clone();
    let version = request.version.clone();
    let paths = app.paths.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<PathBuf> {
        let release = packages::catalog_release(&request.id, &request.version)?;
        anyhow::ensure!(
            release.id == request.id && release.version == request.version,
            "catalog returned a different package identity"
        );
        packages::install(&paths, &release, false, None)
    })
    .await
    .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let installed = match result {
        Ok(path) => path,
        Err(error) => {
            let code = error
                .chain()
                .find_map(|cause| cause.downcast_ref::<packages::PackageFailure>())
                .map_or("package_invalid_manifest", |failure| failure.code);
            record_package(&app, "error", code, &id, &version, "approved_catalog", None);
            return Err(package_err(&error, &id, &version));
        }
    };
    record_package(
        &app,
        "info",
        "package_verified",
        &id,
        &version,
        "approved_catalog",
        Some(&installed),
    );
    ok(
        json!({"path":installed,"verified":true,"source_kind":"approved_catalog","installed":packages::listing(&app.paths).map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR,error))?}),
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
    let id = request.id.clone();
    let version = request.version.clone();
    let paths = app.paths.clone();
    let result = tokio::task::spawn_blocking(move || {
        packages::link_with_manifest(
            &paths,
            &request.id,
            &request.version,
            &request.path,
            request.manifest.as_deref(),
        )
    })
    .await
    .map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let linked = match result {
        Ok(path) => path,
        Err(error) => {
            let code = error
                .chain()
                .find_map(|cause| cause.downcast_ref::<packages::PackageFailure>())
                .map_or("package_invalid_manifest", |failure| failure.code);
            record_package(&app, "error", code, &id, &version, "local_link", None);
            return Err(package_err(&error, &id, &version));
        }
    };
    record_package(
        &app,
        "info",
        if linked.join("release.json").is_file() {
            "package_verified"
        } else {
            "package_linked"
        },
        &id,
        &version,
        "local_link",
        Some(&linked),
    );
    ok(
        json!({"path":linked,"verified":linked.join("release.json").is_file(),"source_kind":"local_link","installed":packages::listing(&app.paths).map_err(|error| err(StatusCode::INTERNAL_SERVER_ERROR,error))?}),
    )
}

async fn setup_status(State(app): State<Arc<App>>, headers: HeaderMap) -> ApiResult {
    authorized(&app, &headers)?;
    let graph_path = app.paths.node_graph_path(&app.deployment);
    let revision = fs::read(&graph_path).ok().map(|bytes| revision(&bytes));
    let backends = crate::backend::installed(&app.paths);
    let mut issues: Vec<String> = Vec::new();
    match fs::read(&graph_path) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(graph) => {
                if let Err(error) = process::validate_value(&app.paths, &graph) {
                    issues.push(error.to_string());
                }
            }
            Err(error) => issues.push(error.to_string()),
        },
        Err(error) => issues.push(format!("graph is missing: {error}")),
    }
    let process =
        crate::manager::manager_call(&app.paths, &app.deployment, "deployment_status").ok();
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
    if control.schema_version != 1 {
        return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 1"));
    }
    let op = match control.operation.as_str() {
        "run" | "start" => "deployment_start",
        "stop" => "deployment_stop",
        "restart" => "deployment_restart",
        _ => {
            return Err(err(
                StatusCode::BAD_REQUEST,
                "operation must be run, start, stop, or restart",
            ));
        }
    };
    let result =
        crate::manager::manager_call_revision(&app.paths, &app.deployment, op, control.revision)
            .map_err(|error| {
                if let Some(remote) = error.downcast_ref::<crate::manager::RemoteDiagnosticError>()
                    && remote.data["diagnostics"].is_array()
                {
                    let status =
                        if remote.data["diagnostics"][0]["code"] == "generated_commit_uncertain" {
                            StatusCode::INTERNAL_SERVER_ERROR
                        } else {
                            StatusCode::UNPROCESSABLE_ENTITY
                        };
                    (
                        status,
                        Json(json!({"schema_version":1,"ok":false,
                        "data":remote.data,"error":remote.message})),
                    )
                } else {
                    err(StatusCode::UNPROCESSABLE_ENTITY, error)
                }
            })?;
    ok(result)
}

async fn logs(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<std::collections::BTreeMap<String, String>>,
) -> ApiResult {
    authorized(&app, &headers)?;
    let default_source = std::env::var("ISOS_LOG_DEFAULT_SOURCE").unwrap_or_else(|_| "all".into());
    let source = query.get("source").unwrap_or(&default_source);
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
        if edit.schema_version != 1 {
            return Err(err(StatusCode::BAD_REQUEST, "API schema_version must be 1"));
        }
        let graph = edit.graph;
        let revision = edit.revision.unwrap_or_else(|| {
            serde_json::to_vec(&graph)
                .map(|bytes| revision(&bytes))
                .unwrap_or_default()
        });
        (graph, revision)
    } else {
        let bytes = fs::read(app.paths.node_graph_path(&app.deployment))
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
                while !crate::foreground::INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst) {
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
                while !crate::foreground::INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst) {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            })
            .await?;
        Ok(())
    })
}
fn router(app: Arc<App>) -> Router {
    let api = Router::new()
        .route("/api/isos/v1/version", get(version).options(preflight))
        .route(
            "/api/isos/v1/node-types",
            get(node_types).options(preflight),
        )
        .route(
            "/api/isos/v1/node-options",
            get(node_options_current)
                .post(node_options_for_graph)
                .options(preflight),
        )
        .route(
            "/api/isos/v1/node-sets",
            get(package_node_sets).options(preflight),
        )
        .route(
            "/api/isos/v1/node-sets/preview",
            post(preview_node_set).options(preflight),
        )
        .route(
            "/api/isos/v1/process-status",
            get(process_status).options(preflight),
        )
        .route("/api/isos/v1/backends", get(backends).options(preflight))
        .route(
            "/api/isos/v1/packages",
            get(package_list).options(preflight),
        )
        .route(
            "/api/isos/v1/packages/install",
            post(package_install).options(preflight),
        )
        .route(
            "/api/isos/v1/packages/link",
            post(package_link).options(preflight),
        )
        .route(
            "/api/isos/v1/packages/preview-link",
            post(preview_link).options(preflight),
        )
        .route(
            "/api/isos/v1/core-readiness",
            get(core_readiness).options(preflight),
        )
        .route(
            "/api/isos/v1/setup-status",
            get(setup_status).options(preflight),
        )
        .route(
            "/api/isos/v1/process-control",
            post(process_control).options(preflight),
        )
        .route("/api/isos/v1/logs", get(logs).options(preflight))
        .route(
            "/api/isos/v1/node-graph",
            get(node_graph).put(apply_node_graph).options(preflight),
        )
        .route(
            "/api/isos/v1/node-graph/validate",
            post(validate_node_graph).options(preflight),
        )
        .route(
            "/api/isos/v1/node-graph/migration/preview",
            post(migration_preview).options(preflight),
        )
        .route(
            "/api/isos/v1/node-graph/migration/apply",
            post(migration_apply).options(preflight),
        )
        .route(
            "/api/isos/v1/core-setup/{role}/{op}",
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

    #[test]
    fn edge_diagnostic_names_the_wire_and_registered_data_types() {
        let graph = json!({"version":"1.0.0","cores":{"eos":{"layers":{"control":{"groups":{"manual":{"nodes":{"target":{"node_type":"target","inputs":{"status":{"node":"eos/control/manual/source","output":"out"}}},"source":{"node_type":"source"}}}}}}}}});
        let catalog = json!({"types":[
            {"id":"target","inputs":[{"name":"status","data_type":"status"}]},
            {"id":"source","outputs":[{"name":"out","data_type":"base_observation"}]}
        ]});
        let diagnostics = edge_diagnostics(&graph, &catalog);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0]["code"], "edge_type_mismatch");
        assert_eq!(
            diagnostics[0]["path"],
            "/cores/eos/layers/control/groups/manual/nodes/target/inputs/status/0"
        );
        assert_eq!(diagnostics[0]["source"], "eos/control/manual/source");
        assert!(
            diagnostics[0]["message"]
                .as_str()
                .unwrap()
                .contains("base_observation")
        );
        let mut future = graph;
        future["version"] = json!("9.0.0");
        assert!(edge_diagnostics(&future, &catalog).is_empty());
    }

    #[tokio::test]
    async fn legacy_get_projects_separately_and_unmapped_validate_apply_keep_raw_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let app = Arc::new(App {
            paths: Paths {
                config: temp.path().join("config"),
                data: temp.path().join("data"),
                runtime: temp.path().join("run"),
            },
            deployment: "default".into(),
        });
        let path = app.paths.node_graph_path("default");
        let mapped =
            include_bytes!("../../tests/fixtures/graph_compat/legacy_optional_defaults_1_0_0.json");
        config::atomic_write(&path, mapped).unwrap();
        let get = Request::builder()
            .uri("/api/isos/v1/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        let response = router(app.clone()).oneshot(get).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(body["data"]["revision"], graph_compat::revision(mapped));
        assert_eq!(
            body["data"]["graph"],
            serde_json::from_slice::<Value>(mapped).unwrap()
        );
        assert_eq!(
            body["data"]["legacy_projection"]["graph"]["version"],
            "1.0.0"
        );
        assert_eq!(
            body["data"]["legacy_projection"]["source_revision"],
            graph_compat::revision(mapped)
        );
        assert!(body["data"]["legacy_projection"]["display_descriptions"].is_object());
        assert_eq!(fs::read(&path).unwrap(), mapped);

        let projection_input = body["data"]["legacy_projection"]["graph"].clone();
        let apply = Request::builder()
            .method(Method::PUT)
            .uri("/api/isos/v1/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({"schema_version":1,
                "revision":graph_compat::revision(mapped),"graph":projection_input}))
                .unwrap(),
            ))
            .unwrap();
        let response = router(app.clone()).oneshot(apply).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let reply: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(
            reply["data"]["diagnostics"][0]["code"],
            "projection_read_only"
        );
        assert_eq!(fs::read(&path).unwrap(), mapped);

        let unmapped =
            include_bytes!("../../tests/fixtures/graph_compat/legacy_unmapped_1_0_0.json");
        config::atomic_write(&path, unmapped).unwrap();
        let graph: Value = serde_json::from_slice(unmapped).unwrap();
        let get = Request::builder()
            .uri("/api/isos/v1/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        let response = router(app.clone()).oneshot(get).await.unwrap();
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(body["data"]["read_only"], true);
        assert!(body["data"].get("legacy_projection").is_none());
        let expected = "/cores/eos/layers/control/groups/manual/nodes/manual_0/node_type";
        assert_eq!(body["data"]["diagnostics"][0]["path"], expected);
        assert_eq!(body["data"]["diagnostics"][0]["code"], "unmapped_node_type");
        let validate = Request::builder()
            .method(Method::POST)
            .uri("/api/isos/v1/node-graph/validate")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({"schema_version":1,"graph":graph})).unwrap(),
            ))
            .unwrap();
        let response = router(app.clone()).oneshot(validate).await.unwrap();
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(body["data"]["valid"], false);
        assert_eq!(body["data"]["diagnostics"][0]["path"], expected);
        let apply = Request::builder()
            .method(Method::PUT)
            .uri("/api/isos/v1/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({"schema_version":1,
                "revision":graph_compat::revision(unmapped),"graph":graph}))
                .unwrap(),
            ))
            .unwrap();
        let response = router(app.clone()).oneshot(apply).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(body["data"]["diagnostics"][0]["path"], expected);
        let status = Request::builder()
            .uri("/api/isos/v1/setup-status")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        let response = router(app.clone()).oneshot(status).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(body["data"]["ready"], false);
        assert!(
            body["data"]["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue
                    .as_str()
                    .is_some_and(|message| message.contains(expected)
                        && message.contains("unmapped_node_type")))
        );
        assert_eq!(fs::read(path).unwrap(), unmapped);
    }

    #[tokio::test]
    async fn core_setup_api_rejects_unknown_role_and_missing_graph() {
        for (route, expected) in [
            (
                "/api/isos/v1/core-setup/other/catalog",
                StatusCode::NOT_FOUND,
            ),
            (
                "/api/isos/v1/core-setup/eos/validate",
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
    async fn saved_future_graph_is_inspectable_but_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let app = Arc::new(App {
            paths: Paths {
                config: temp.path().join("config"),
                data: temp.path().join("data"),
                runtime: temp.path().join("run"),
            },
            deployment: "default".into(),
        });
        let path = app.paths.node_graph_path("default");
        let source = include_bytes!("../../tests/fixtures/graph_compat/future_2_0_0.json");
        config::atomic_write(&path, source).unwrap();
        let request = Request::builder()
            .uri("/api/isos/v1/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        let response = router(app).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(body["data"]["revision"], graph_compat::revision(source));
        assert_eq!(body["data"]["graph"]["version"], "2.0.0");
        assert_eq!(body["data"]["read_only"], true);
        assert_eq!(body["data"]["diagnostics"][0]["path"], "/version");
        assert_eq!(fs::read(path).unwrap(), source);
    }

    #[tokio::test]
    async fn saved_new_graph_is_editable_and_missing_revision_apply_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let app = Arc::new(App {
            paths: Paths {
                config: temp.path().join("config"),
                data: temp.path().join("data"),
                runtime: temp.path().join("run"),
            },
            deployment: "default".into(),
        });
        let graph = crate::starter::draft(&app.paths, "default").unwrap();
        let source = serde_json::to_vec_pretty(&graph).unwrap();
        config::atomic_write(&app.paths.node_graph_path("default"), &source).unwrap();
        let request = Request::builder()
            .uri("/api/isos/v1/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        let response = router(app.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(body["data"]["graph"]["version"], "0.1.0");
        assert_eq!(body["data"]["read_only"], false, "{body}");
        let request = Request::builder()
            .method(Method::PUT)
            .uri("/api/isos/v1/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({"schema_version":1,"graph":graph})).unwrap(),
            ))
            .unwrap();
        let response = router(app).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn changed_legacy_apply_reports_migration_without_writing() {
        let temp = tempfile::tempdir().unwrap();
        let app = Arc::new(App {
            paths: Paths {
                config: temp.path().join("config"),
                data: temp.path().join("data"),
                runtime: temp.path().join("run"),
            },
            deployment: "default".into(),
        });
        let path = app.paths.node_graph_path("default");
        let source =
            include_bytes!("../../tests/fixtures/graph_compat/legacy_1_0_0_whitespace.json");
        config::atomic_write(&path, source).unwrap();
        let inspection = Request::builder()
            .uri("/api/isos/v1/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        let inspected = router(app.clone()).oneshot(inspection).await.unwrap();
        let body: Value =
            serde_json::from_slice(&to_bytes(inspected.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(body["data"]["read_only"], false);
        config::atomic_write(
            &app.paths
                .generated("default")
                .join("core-setup/launch-inputs.json"),
            b"[]",
        )
        .unwrap();
        let inspection = Request::builder()
            .uri("/api/isos/v1/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        let inspected = router(app.clone()).oneshot(inspection).await.unwrap();
        let body: Value =
            serde_json::from_slice(&to_bytes(inspected.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(body["data"]["read_only"], true);
        let mut changed: Value = serde_json::from_slice(source).unwrap();
        changed["robot"]["name"] = json!("changed");
        let request = Request::builder()
            .method(Method::PUT)
            .uri("/api/isos/v1/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({"schema_version":1,
                "revision":graph_compat::revision(source),"graph":changed}))
                .unwrap(),
            ))
            .unwrap();
        let response = router(app).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(body["data"]["diagnostics"][0]["code"], "migration_required");
        assert_eq!(body["data"]["diagnostics"][0]["path"], "/version");
        assert_eq!(fs::read(path).unwrap(), source);
    }

    #[tokio::test]
    async fn validation_rejects_future_version_and_unsafe_process_at_exact_paths() {
        for (source, path, code) in [
            (
                include_bytes!("../../tests/fixtures/graph_compat/future_2_0_0.json").as_slice(),
                "/version",
                "unsupported_graph_version",
            ),
            (
                include_bytes!("../../tests/fixtures/graph_compat/unsafe_component_1_0_0.json")
                    .as_slice(),
                "/cores/isos/layers/process/groups/component/nodes/eos/node_type",
                "unsafe_process_node",
            ),
            (
                include_bytes!("../../tests/fixtures/graph_compat/unknown_type_1_0_0.json")
                    .as_slice(),
                "/cores/eos/layers/dispatcher/groups/control/nodes/command/node_type",
                "unsupported_node_type",
            ),
        ] {
            let graph: Value = serde_json::from_slice(source).unwrap();
            let request = Request::builder()
                .method(Method::POST)
                .uri("/api/isos/v1/node-graph/validate")
                .header(header::HOST, "127.0.0.1:16000")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({"schema_version":1,"graph":graph})).unwrap(),
                ))
                .unwrap();
            let response = test_router().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                    .unwrap();
            assert_eq!(body["data"]["valid"], false);
            assert_eq!(body["data"]["diagnostics"][0]["path"], path);
            assert_eq!(body["data"]["diagnostics"][0]["code"], code);
        }
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
    async fn obsolete_component_and_build_routes_are_absent() {
        for route in [
            "/api/isos/v1/graph",
            "/api/isos/v1/graph/validate",
            "/api/isos/v1/profiles",
            "/api/isos/v1/config/eos/describe",
            "/api/isos/v1/build-binaries",
            "/api/isos/v1/build-binaries/select",
        ] {
            let request = Request::builder()
                .uri(route)
                .header(header::HOST, "127.0.0.1:16000")
                .body(Body::empty())
                .unwrap();
            let response = test_router().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{route}");
        }
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
            ("/api/isos/v1/logs?source=isos&limit=1", StatusCode::OK),
            (
                "/api/isos/v1/logs?source=isos&limit=201",
                StatusCode::BAD_REQUEST,
            ),
            (
                "/api/isos/v1/logs?source=..%2Fsecret",
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
    async fn logs_filter_level_and_default_to_unified_stream() {
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
                "/api/isos/v1/logs?source=eos&level=warn",
                StatusCode::OK,
                "failed",
            ),
            (
                "/api/isos/v1/logs?source=eos&level=verbose",
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
        let request = Request::builder()
            .uri("/api/isos/v1/logs?limit=2")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        let response = router(app).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["data"]["lines"].as_array().unwrap().len(), 2);
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
            .uri("/api/isos/v1/node-graph")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::ORIGIN, "https://www.os-island.com")
            .body(Body::empty())
            .unwrap();
        let response = router(app).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["data"]["revision"], "");
        assert_eq!(body["data"]["graph"]["version"], "0.1.0");
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
        for route in ["/api/isos/v1/backends", "/api/isos/v1/setup-status"] {
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
        let invalid = json!({"id":"island/mujoco-sim","version":"1.0.0","path":bundle,"manifest":"relative.json"});
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/isos/v1/packages/link")
            .header(header::HOST, "127.0.0.1:16000")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(invalid.to_string()))
            .unwrap();
        assert_eq!(
            router(app.clone()).oneshot(request).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
        let body = json!({"id":"island/mujoco-sim","version":"1.0.0","path":bundle});
        for expected in [StatusCode::OK, StatusCode::CONFLICT] {
            let request = Request::builder()
                .method(Method::POST)
                .uri("/api/isos/v1/packages/link")
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
            .uri("/api/isos/v1/packages")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        let response = router(app.clone()).oneshot(request).await.unwrap();
        let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["data"]["installed"][0]["version"], "1.0.0");
        assert_eq!(body["data"]["installed"].as_array().unwrap().len(), 1);
        let lines = LogStore::new(&app.paths, "default")
            .unwrap()
            .read("packages", None, 10)
            .unwrap();
        assert_eq!(lines["lines"][0]["event_code"], "package_linked");
        assert_eq!(
            lines["lines"][0]["fields"]["package_id"],
            "island/mujoco-sim"
        );
        assert_eq!(lines["lines"][0]["fields"]["package_version"], "1.0.0");
        assert_eq!(lines["lines"][0]["fields"]["source_kind"], "local_link");
        assert_eq!(lines["lines"][1]["event_code"], "package_version_exists");
        assert_eq!(
            lines["lines"][1]["fields"]["package_id"],
            "island/mujoco-sim"
        );
        assert_eq!(
            lines["lines"][1]["run_id"],
            crate::manager::manager_session_id()
        );
        assert!(lines["lines"][1]["correlation_id"].as_str().is_some());
    }

    #[tokio::test]
    async fn loopback_and_trusted_browser_origins_connect_without_token() {
        let request = Request::builder()
            .method(Method::OPTIONS)
            .uri("/api/isos/v1/version")
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
            .uri("/api/isos/v1/version")
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
            .uri("/api/isos/v1/version")
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
                .uri("/api/isos/v1/version")
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
            .uri("/api/isos/v1/version")
            .header(header::HOST, "127.0.0.1:16000")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            test_router().oneshot(request).await.unwrap().status(),
            StatusCode::OK
        );
        let request = Request::builder()
            .uri("/api/isos/v1/version")
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
                .uri("/api/isos/v1/version")
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
}
