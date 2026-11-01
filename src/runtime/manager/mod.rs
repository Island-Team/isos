use crate::foreground::INTERRUPTED;
use crate::{
    config, core_setup, graph_compat, graph_migration, http, log_store, preflight, process,
    services, starter,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

mod protocol;
use protocol::*;

pub(crate) struct ActiveDeployment {
    active_revision: String,
    supervisor: process::Supervisor,
    last_error: Option<String>,
}
pub(crate) type Deployments = BTreeMap<String, ActiveDeployment>;

pub(crate) fn manager_session_id() -> &'static str {
    static SESSION: OnceLock<String> = OnceLock::new();
    SESSION.get_or_init(|| {
        format!(
            "m{}-{:x}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    })
}

pub(crate) fn next_correlation_id() -> String {
    static NEXT_CORRELATION: AtomicU64 = AtomicU64::new(1);
    format!("p{}", NEXT_CORRELATION.fetch_add(1, Ordering::Relaxed))
}

fn log_package_load_failure(paths: &config::Paths, name: &str, error: &anyhow::Error) {
    let Some(failure) = error.downcast_ref::<process::PackageLoadFailure>() else {
        return;
    };
    if let Ok(logs) = log_store::LogStore::new(paths, name) {
        let correlation = next_correlation_id();
        let _ = logs.append_event_context(
            "packages", "error", "package_load_failed",
            "Deployment package load failed before launch",
            log_store::LogContext {
                run_id: Some(manager_session_id()),
                correlation_id: Some(&correlation),
                fields: json!({"package_id":failure.id,"package_version":failure.version,"stage":failure.stage}),
            },
        );
    }
}

fn preflight_packages(
    paths: &config::Paths,
    name: &str,
    source: &str,
    revision: &str,
) -> Result<Vec<process::ProcessSpec>> {
    let mut specs = process::project_with_packages(paths, source).inspect_err(|error| {
        log_package_load_failure(paths, name, error);
    })?;
    core_setup::verify_launch_inputs(paths, name)?;
    let graph: Value = serde_json::from_str(source)?;
    if graph["version"] == "0.1.0" {
        let generated = core_setup::snapshot_root(paths, name, revision).join("eos/graph.json");
        process::use_generated_eos_profile(&mut specs, &generated)?;
        preflight::verify_new_graph_readers(&specs)?;
    }
    preflight::verify_processes(paths, &specs)?;
    Ok(specs)
}

fn validate_saved_graph(paths: &config::Paths, name: &str, source: &str) -> Result<()> {
    let saved = graph_compat::parse(source.as_bytes())?;
    if saved.value["version"] == "0.1.0" {
        let proposal = graph_migration::Proposal {
            graph: saved.value,
            bytes: source.as_bytes().to_vec(),
            revision: saved.revision,
            changes: Vec::new(),
        };
        graph_migration::prepare(paths, name, &proposal)
            .map_err(|diagnostic| anyhow::anyhow!(diagnostic.to_string()))?;
        Ok(())
    } else {
        graph_compat::admit_legacy_mapped(&saved.value, process::catalog().ok().as_ref())?;
        process::validate_full_graph(&paths.node_graph_path(name))
    }
}

pub(crate) fn restart_processes(
    paths: &config::Paths,
    managers: &mut Deployments,
    name: &str,
    revision: &str,
    specs: &[process::ProcessSpec],
) -> Result<Vec<String>> {
    let mut supervisor = process::Supervisor::new(paths, name)?;
    let previous = managers.remove(name).map(|mut active| {
        let old_specs = active.supervisor.specs();
        active.supervisor.stop();
        (active, old_specs)
    });
    let attempt = (|| -> Result<Vec<String>> {
        let started = supervisor.start(specs)?;
        std::thread::sleep(Duration::from_millis(100));
        let running = supervisor.status()?;
        ensure!(
            running.len() == specs.len(),
            "a managed process exited during restart; inspect categorized child logs"
        );
        Ok(started)
    })();
    let started = match attempt {
        Ok(started) => started,
        Err(error) => {
            supervisor.stop();
            if let Some((mut active, old_specs)) = previous {
                let restore = active.supervisor.start(&old_specs).and_then(|_| {
                    std::thread::sleep(Duration::from_millis(100));
                    ensure!(
                        active.supervisor.status()?.len() == old_specs.len(),
                        "a previous child exited during restoration"
                    );
                    Ok(())
                });
                match restore {
                    Ok(_) => {
                        active.last_error = None;
                        managers.insert(name.to_owned(), active);
                        bail!("restart failed; previous revision restored: {error:#}");
                    }
                    Err(restore_error) => {
                        active.last_error = Some(format!(
                            "restart failed: {error:#}; previous revision could not be restored: {restore_error:#}"
                        ));
                        managers.insert(name.to_owned(), active);
                        bail!(
                            "restart failed: {error:#}; previous revision could not be restored: {restore_error:#}"
                        );
                    }
                }
            }
            return Err(error);
        }
    };
    managers.insert(
        name.to_owned(),
        ActiveDeployment {
            active_revision: revision.to_owned(),
            supervisor,
            last_error: None,
        },
    );
    Ok(started)
}

pub(crate) fn manager_identity(paths: &config::Paths) -> Result<Value> {
    let executable = std::env::current_exe()?;
    let stat = fs::read_to_string("/proc/self/stat")?;
    let fields = stat
        .rsplit_once(") ")
        .context("read process start identity")?
        .1;
    let start_ticks = fields
        .split_whitespace()
        .nth(19)
        .context("process start identity is incomplete")?;
    Ok(json!({
        "pid": std::process::id(),
        "start_ticks": start_ticks,
        "mode": paths.mode()?,
        "executable": executable,
        "executable_sha256": hex::encode(Sha256::digest(fs::read(executable)?)),
        "isos_version": env!("CARGO_PKG_VERSION"),
        "api_schema_version": 1,
        "service_contract_version": "1.0.0",
        "api_origin": http::local_origin(),
        "ui_url": http::ui_url()?,
    }))
}

pub(crate) fn api_call(
    paths: &config::Paths,
    managers: &mut Deployments,
    service_manager: &mut services::ServiceManager,
    request: ApiRequest,
) -> Result<Value> {
    ensure!(request.schema_version == 1, "API schema_version must be 1");
    ensure!(
        request
            .role
            .as_deref()
            .is_none_or(|role| matches!(role, "eos" | "egia")),
        "role must be eos or egia"
    );
    let name = config::deployment_name(request.deployment);
    if request.op.starts_with("deployment_") {
        process::validate_deployment_name(&name)?;
    }
    match request.op.as_str() {
        "manager_identity" => manager_identity(paths),
        "log_append" => {
            process::validate_deployment_name(&name)?;
            let event = request.log.context("log is required")?;
            ensure!(
                !matches!(
                    event.source.as_str(),
                    "all" | "events" | "services" | "isos" | "browser"
                ),
                "reserved log source"
            );
            ensure!(
                !event.message.is_empty() && event.message.len() <= 8192,
                "log message must be 1..8192 bytes"
            );
            let code = event.event_code.as_deref().unwrap_or("external_event");
            ensure!(
                !code.is_empty()
                    && code.len() <= 64
                    && code.bytes().all(|byte| byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'),
                "event_code must be 1..64 lowercase letters, digits, or underscores"
            );
            ensure!(
                log_store::level_rank(&event.level).is_some(),
                "log level must be trace, debug, info, warn, or error"
            );
            log_store::LogStore::new(paths, &name)?.append_event(
                &event.source,
                &event.level,
                code,
                &event.message,
            )?;
            Ok(json!({"accepted":true}))
        }
        "manager_shutdown" => {
            tracing::info!(
                event_code = "manager_shutdown",
                "IsOS manager shutdown requested"
            );
            let mut stopped = Vec::new();
            for active in managers.values_mut() {
                stopped.extend(active.supervisor.stop());
            }
            managers.clear();
            let services = service_manager.stop_all();
            INTERRUPTED.store(true, Ordering::SeqCst);
            Ok(json!({"stopped":stopped,"services":services,"manager":"stopping"}))
        }
        "services_list" => Ok(json!({"services":service_manager.list()?})),
        "services_validate" => Ok(json!({"services":service_manager.validate()?})),
        "services_status" => Ok(json!({"services":service_manager.status()?})),
        "services_start" => Ok(json!({"started":service_manager.start(
            request.service_id.as_deref().context("service_id is required")?
        )?})),
        "services_stop" => Ok(json!({"stopped":service_manager.stop(
            request.service_id.as_deref().context("service_id is required")?
        )?})),
        "services_restart" => Ok(json!({"started":service_manager.restart(
            request.service_id.as_deref().context("service_id is required")?
        )?})),
        "services_logs" => service_manager.logs(
            request
                .service_id
                .as_deref()
                .context("service_id is required")?,
            request.cursor.as_deref(),
            request.limit.unwrap_or(100),
        ),
        "deployment_active_packages" => {
            let packages: Vec<_> = managers
                .values()
                .flat_map(|active| active.supervisor.active_packages())
                .collect();
            Ok(json!({"packages":packages}))
        }
        "deployment_restart" => {
            let path = paths.node_graph_path(&name);
            let source = fs::read_to_string(&path)?;
            validate_saved_graph(paths, &name, &source)?;
            let revision = graph_compat::revision(source.as_bytes());
            if let Some(expected) = request.revision.as_deref() {
                ensure!(
                    expected == revision,
                    "saved graph revision changed; reload before restart"
                );
            }
            let specs = preflight_packages(paths, &name, &source, &revision)?;
            core_setup::prepare_run(paths, &name, &revision)?;
            ensure!(
                starter::revision(&path)? == revision,
                "saved graph revision changed during core setup; reload before restart"
            );
            let started = restart_processes(paths, managers, &name, &revision, &specs)?;
            Ok(json!({"started":started,"active_revision":revision}))
        }
        "deployment_start" => {
            let path = paths.node_graph_path(&name);
            let source = fs::read_to_string(&path)?;
            validate_saved_graph(paths, &name, &source)?;
            let revision = graph_compat::revision(source.as_bytes());
            if let Some(expected) = request.revision.as_deref() {
                ensure!(
                    expected == revision,
                    "saved graph revision changed; reload before run"
                );
            }
            let specs = preflight_packages(paths, &name, &source, &revision)?;
            core_setup::prepare_run(paths, &name, &revision)?;
            ensure!(
                starter::revision(&path)? == revision,
                "saved graph revision changed during core setup; reload before run"
            );
            let manager = if let Some(active) = managers.get_mut(&name) {
                ensure!(
                    active.active_revision == revision,
                    "saved graph changed while running; stop before restart"
                );
                active.supervisor.verify_specs(&specs)?;
                &mut active.supervisor
            } else {
                managers.insert(
                    name.clone(),
                    ActiveDeployment {
                        active_revision: revision.clone(),
                        supervisor: process::Supervisor::new(paths, &name)?,
                        last_error: None,
                    },
                );
                &mut managers.get_mut(&name).unwrap().supervisor
            };
            let started = match manager.start(&specs).and_then(|started| {
                std::thread::sleep(Duration::from_millis(100));
                let running = manager.status()?;
                ensure!(
                    running.len() == specs.len(),
                    "a managed process exited during startup; inspect categorized child logs"
                );
                Ok(started)
            }) {
                Ok(started) => started,
                Err(error) => {
                    if let Some(active) = managers.get_mut(&name) {
                        active.supervisor.stop();
                        active.last_error = Some(error.to_string());
                    }
                    return Err(error);
                }
            };
            if let Some(active) = managers.get_mut(&name) {
                active.last_error = None;
            }
            Ok(json!({"started":started,"active_revision":revision}))
        }
        "deployment_stop" => {
            let stopped = managers
                .remove(&name)
                .map_or_else(Vec::new, |mut active| active.supervisor.stop());
            Ok(json!({"stopped": stopped}))
        }
        "deployment_status" => {
            let saved_revision = starter::revision(&paths.node_graph_path(&name)).ok();
            let pinned_package = fs::read(paths.node_graph_path(&name)).ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .and_then(|graph| {
                    ["mujoco", "backend"].into_iter().find_map(|group| {
                        graph.pointer(&format!("/cores/eos/layers/process/groups/{group}/nodes"))
                            .and_then(Value::as_object).and_then(|nodes| nodes.values().next()).cloned()
                    })
                })
                .map(|node| json!({
                    "id":node["package"].get("id").or_else(|| node["settings"].get("package")),
                    "version":node["package"].get("version").or_else(|| node["settings"].get("version")),
                    "scene":node["settings"].get("scene")
                }));
            let active = managers.get_mut(&name);
            let processes = if let Some(active) = active {
                match active.supervisor.status() {
                    Ok(processes) => processes,
                    Err(error) => {
                        active.last_error = Some(error.to_string());
                        active.supervisor.status_snapshot()
                    }
                }
            } else {
                Vec::new()
            };
            let active = managers.get(&name);
            let active_revision = active.map(|a| a.active_revision.clone());
            let supervisor_failed = active.is_some_and(|a| a.last_error.is_some());
            let last_error = active.and_then(|a| a.last_error.clone()).or_else(|| {
                fs::read_to_string(paths.runtime.join(format!("{name}.last-error"))).ok()
            });
            let state = if active.is_none() {
                "stopped"
            } else if supervisor_failed {
                "degraded"
            } else {
                "running"
            };
            Ok(
                json!({"schema_version":"1.0.0","saved_revision":saved_revision,"active_revision":active_revision,"state":state,"last_error":last_error,"package":pinned_package,"processes":processes}),
            )
        }
        _ => bail!("unknown API operation {}", request.op),
    }
}

pub(crate) fn handle_client(
    paths: &config::Paths,
    managers: &mut Deployments,
    service_manager: &mut services::ServiceManager,
    mut stream: UnixStream,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut line = String::new();
    BufReader::new(stream.try_clone()?)
        .take(2_000_001)
        .read_line(&mut line)?;
    if line.is_empty() {
        return Ok(());
    }
    ensure!(line.len() <= 2_000_000, "API request exceeds 2 MB");
    let request = serde_json::from_str::<ApiRequest>(&line);
    let failed_deployment = request
        .as_ref()
        .ok()
        .filter(|request| {
            request.op.starts_with("deployment_")
                && !matches!(
                    request.op.as_str(),
                    "deployment_status" | "deployment_active_packages"
                )
        })
        .map(|request| config::deployment_name(request.deployment.clone()));
    let clear_error = request.as_ref().ok().is_some_and(|request| {
        matches!(
            request.op.as_str(),
            "deployment_start" | "deployment_restart"
        )
    });
    let outcome = request
        .map_err(anyhow::Error::from)
        .and_then(|request| api_call(paths, managers, service_manager, request));
    if let Err(error) = &outcome {
        tracing::warn!(
            event_code = "manager_request_failed",
            "manager request failed: {error:#}"
        );
    }
    if let Some(name) =
        failed_deployment.filter(|name| process::validate_deployment_name(name).is_ok())
    {
        let path = paths.runtime.join(format!("{name}.last-error"));
        match &outcome {
            Err(error) => {
                let message: String = error.to_string().chars().take(4096).collect();
                let _ = config::atomic_write(&path, message.as_bytes());
            }
            Ok(_) if clear_error => {
                let _ = fs::remove_file(path);
            }
            _ => {}
        }
    }
    let response = match outcome {
        Ok(data) => ApiResponse {
            schema_version: 1,
            ok: true,
            data,
            error: None,
        },
        Err(error) => ApiResponse {
            schema_version: 1,
            ok: false,
            data: Value::Null,
            error: Some(error.to_string()),
        },
    };
    serde_json::to_writer(&mut stream, &response)?;
    stream.write_all(b"\n")?;
    Ok(())
}

pub(crate) fn bind_daemon(paths: &config::Paths) -> Result<UnixListener> {
    paths.prepare()?;
    let path = paths.socket();
    if path.exists() {
        ensure!(
            UnixStream::connect(&path).is_err(),
            "manager socket is already active"
        );
        fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

pub(crate) fn daemon(
    paths: &config::Paths,
    deployment: &str,
    listener: UnixListener,
) -> Result<()> {
    let mut managers = Deployments::new();
    let mut service_manager = services::ServiceManager::new(paths, deployment)?;
    println!("IsOS API listening on {}", paths.socket().display());
    while !INTERRUPTED.load(Ordering::SeqCst) {
        for active in managers.values_mut() {
            if let Err(error) = active.supervisor.poll() {
                active.last_error = Some(error.to_string());
                tracing::error!(
                    event_code = "deployment_supervisor_failed",
                    "process supervisor: {error:#}"
                );
                eprintln!("process supervisor: {error:#}");
            }
        }
        if let Err(error) = service_manager.poll() {
            tracing::error!(
                event_code = "service_supervisor_failed",
                "service supervisor: {error:#}"
            );
            eprintln!("service supervisor: {error:#}");
        }
        match listener.accept() {
            Ok((client, _)) => {
                if let Err(error) =
                    handle_client(paths, &mut managers, &mut service_manager, client)
                {
                    tracing::warn!(
                        event_code = "manager_client_failed",
                        "API client: {error:#}"
                    );
                    eprintln!("API client: {error}");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                tracing::error!(
                    event_code = "manager_accept_failed",
                    "API accept: {error:#}"
                );
                eprintln!("API accept: {error}");
            }
        }
    }
    for active in managers.values_mut() {
        active.supervisor.stop();
    }
    service_manager.stop_all();
    tracing::info!(event_code = "manager_stopped", "IsOS manager stopped");
    fs::remove_file(paths.socket())?;
    Ok(())
}

pub(crate) fn manager_call(paths: &config::Paths, name: &str, op: &str) -> Result<Value> {
    manager_call_revision(paths, name, op, None)
}

pub(crate) fn manager_call_revision(
    paths: &config::Paths,
    name: &str,
    op: &str,
    revision: Option<String>,
) -> Result<Value> {
    if matches!(
        op,
        "deployment_start" | "deployment_restart" | "services_start" | "services_restart"
    ) {
        check_manager_binary(paths, name)?;
    }
    let request = ApiRequest {
        schema_version: 1,
        op: op.to_owned(),
        deployment: Some(name.to_owned()),
        role: None,
        revision,
        service_id: None,
        cursor: None,
        limit: None,
        log: None,
    };
    manager_request(paths, request)
}

pub(crate) fn manager_request(paths: &config::Paths, request: ApiRequest) -> Result<Value> {
    let mut stream = UnixStream::connect(paths.socket()).map_err(|error| {
        anyhow::anyhow!("IsOS manager is not running; run `isos serve` first: {error}")
    })?;
    serde_json::to_writer(&mut stream, &request)?;
    stream.write_all(b"\n")?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    let response: ApiResponse = serde_json::from_str(&line)?;
    ensure!(
        response.ok,
        "{}",
        response.error.unwrap_or_else(|| "manager error".into())
    );
    Ok(response.data)
}

pub(crate) fn check_manager_binary(paths: &config::Paths, name: &str) -> Result<()> {
    let active = manager_call(paths, name, "manager_identity")?;
    let own = hex::encode(Sha256::digest(fs::read(std::env::current_exe()?)?));
    ensure!(
        active["executable_sha256"] == own,
        "running IsOS manager uses a different binary; run `isos stop`, then `isos serve` with this version"
    );
    Ok(())
}

pub(crate) fn service_call(
    paths: &config::Paths,
    name: &str,
    op: &str,
    id: Option<String>,
    limit: Option<usize>,
) -> Result<Value> {
    if matches!(op, "services_start" | "services_restart") {
        check_manager_binary(paths, name)?;
    }
    manager_request(
        paths,
        ApiRequest {
            schema_version: 1,
            op: op.to_owned(),
            deployment: Some(name.to_owned()),
            role: None,
            revision: None,
            service_id: id,
            cursor: None,
            limit,
            log: None,
        },
    )
}

#[cfg(test)]
mod restart_tests {
    use super::*;

    #[test]
    fn run_rejects_unowned_core_before_projecting_children() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let mut graph: Value = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/graph_compat/migration_expected_0_1_0.json"
        ))
        .unwrap();
        graph["cores"]["evil"] = json!({"layers":{}});
        let error = validate_saved_graph(&paths, "default", &graph.to_string())
            .unwrap_err()
            .to_string();
        assert!(error.contains("/cores/evil"), "{error}");
        assert!(error.contains("unsupported_core"), "{error}");
    }

    #[test]
    fn typed_package_load_failure_records_exact_pin_and_session() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let failure = anyhow::Error::new(process::PackageLoadFailure {
            id: "island/mujoco-sim".into(),
            version: "0.1.1".into(),
            stage: "backend",
            source: anyhow::anyhow!("private path /tmp/secret"),
        });
        log_package_load_failure(&paths, "default", &failure);
        log_package_load_failure(
            &paths,
            "default",
            &anyhow::anyhow!("unrelated EOS binary missing"),
        );
        let lines = log_store::LogStore::new(&paths, "default")
            .unwrap()
            .read("packages", None, 10)
            .unwrap();
        assert_eq!(lines["lines"].as_array().unwrap().len(), 1);
        let line = &lines["lines"][0];
        assert_eq!(line["event_code"], "package_load_failed");
        assert_eq!(line["fields"]["package_id"], "island/mujoco-sim");
        assert_eq!(line["fields"]["package_version"], "0.1.1");
        assert_eq!(line["fields"]["stage"], "backend");
        assert_eq!(line["run_id"], manager_session_id());
        assert!(line["correlation_id"].as_str().is_some());
        assert!(!line.to_string().contains("/tmp/secret"));
    }

    #[test]
    fn successful_graph_apply_clears_previous_setup_failure() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        fs::create_dir_all(&paths.runtime).unwrap();
        let stale = paths.runtime.join("default.last-error");
        fs::write(&stale, "old setup failure").unwrap();
        crate::cli::clear_saved_setup_error(&paths, "default").unwrap();
        assert!(!stale.exists());
        crate::cli::clear_saved_setup_error(&paths, "default").unwrap();
    }

    #[test]
    fn external_log_operation_records_selected_event_and_rejects_reserved_source() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let mut managers = Deployments::new();
        let mut services = services::ServiceManager::new(&paths, "default").unwrap();
        let make_request = |source: &str| ApiRequest {
            schema_version: 1,
            op: "log_append".into(),
            deployment: Some("default".into()),
            role: None,
            revision: None,
            service_id: None,
            cursor: None,
            limit: None,
            log: Some(LogEventInput {
                source: source.into(),
                level: "warn".into(),
                message: "adapter unavailable".into(),
                event_code: Some("adapter_unavailable".into()),
            }),
        };
        assert_eq!(
            api_call(&paths, &mut managers, &mut services, make_request("eos")).unwrap()["accepted"],
            true
        );
        let stored = log_store::LogStore::new(&paths, "default")
            .unwrap()
            .read("eos", None, 10)
            .unwrap();
        assert_eq!(stored["lines"][0]["event_code"], "adapter_unavailable");
        assert_eq!(stored["lines"][0]["message"], "adapter unavailable");
        assert!(api_call(&paths, &mut managers, &mut services, make_request("isos")).is_err());
    }

    #[test]
    fn failed_restart_restores_previous_revision_and_child() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let spec = |program: &str| process::ProcessSpec {
            id: "stub".into(),
            path: "isos/process/component/stub".into(),
            settings: serde_json::from_value(json!({
                "command": [program, if program == "/bin/sleep" { "30" } else { "" }],
                "start_timeout_ms": 200
            }))
            .unwrap(),
            package: None,
            fingerprint: None,
        };
        let mut managers = Deployments::new();
        restart_processes(&paths, &mut managers, "test", "old", &[spec("/bin/sleep")]).unwrap();
        let error = restart_processes(&paths, &mut managers, "test", "new", &[spec("/bin/false")])
            .unwrap_err();
        assert!(error.to_string().contains("previous revision restored"));
        let active = managers.get_mut("test").unwrap();
        assert_eq!(active.active_revision, "old");
        assert_eq!(active.supervisor.status().unwrap().len(), 1);
        active.supervisor.stop();
    }
}
