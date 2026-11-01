use crate::foreground::INTERRUPTED;
use crate::{
    config, core_setup, dependent_stop, graph_compat, graph_migration, http, log_store, process,
    reader_features, services, starter,
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
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

mod protocol;
use protocol::*;

pub(crate) struct ActiveDeployment {
    active_revision: String,
    managed: bool,
    supervisor: process::Supervisor,
    last_error: Option<String>,
    expected_processes: usize,
}
pub(crate) type Deployments = BTreeMap<String, ActiveDeployment>;

#[derive(Debug)]
pub(crate) struct ScopePublishFailure;

impl std::fmt::Display for ScopePublishFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("log scope publication failed")
    }
}
impl std::error::Error for ScopePublishFailure {}

fn publish_run_scope(supervisor: &process::Supervisor) -> Result<()> {
    supervisor
        .publish_run_scope()
        .map_err(|_| anyhow::Error::new(ScopePublishFailure))
}

fn ensure_launch_history(paths: &config::Paths, name: &str) -> Result<()> {
    let (_, state) = log_store::LogStore::new(paths, name)?.run_identity();
    ensure!(
        state == "none",
        "previous run requires owner-reviewed stop recovery"
    );
    Ok(())
}

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

pub(crate) fn next_run_id() -> String {
    static NEXT_RUN: AtomicU64 = AtomicU64::new(1);
    format!(
        "r{}-{}",
        manager_session_id().trim_start_matches('m'),
        NEXT_RUN.fetch_add(1, Ordering::Relaxed)
    )
}

std::thread_local! {
    static RUN_STARTED_IN_REQUEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn stop_log_gate() -> &'static (std::sync::Barrier, std::sync::Barrier) {
    static GATE: OnceLock<(std::sync::Barrier, std::sync::Barrier)> = OnceLock::new();
    GATE.get_or_init(|| (std::sync::Barrier::new(2), std::sync::Barrier::new(2)))
}

#[cfg(test)]
fn stalled_stop_log_gate() -> &'static (std::sync::Barrier, std::sync::Barrier) {
    static GATE: OnceLock<(std::sync::Barrier, std::sync::Barrier)> = OnceLock::new();
    GATE.get_or_init(|| (std::sync::Barrier::new(2), std::sync::Barrier::new(2)))
}

struct StopLogJob {
    paths: config::Paths,
    deployment: String,
    run_id: String,
    correlation: String,
    terminal: Receiver<(dependent_stop::Terminal, bool)>,
}

fn stop_log_queue() -> Option<&'static SyncSender<StopLogJob>> {
    static QUEUE: OnceLock<Option<SyncSender<StopLogJob>>> = OnceLock::new();
    QUEUE
        .get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel::<StopLogJob>(32);
            std::thread::Builder::new()
                .name("isos-stop-events".into())
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        write_stop_log_job(job);
                    }
                })
                .ok()
                .map(|_| sender)
        })
        .as_ref()
}

fn write_stop_log_job(job: StopLogJob) {
    let logs = log_store::LogStore::new(&job.paths, &job.deployment);
    if let Ok(logs) = &logs {
        let _ = logs.append_event_context(
            "isos",
            "info",
            "deployment_stopping",
            "Deployment stopping",
            log_store::LogContext {
                run_id: Some(&job.run_id),
                correlation_id: Some(&job.correlation),
                node_path: None,
                fields: json!({}),
            },
        );
    }
    let Ok((terminal, failed_candidate)) = job.terminal.recv_timeout(Duration::from_secs(125))
    else {
        log_store::record_stop_event_loss();
        return;
    };
    let (code, level, message) = if terminal.outcome == "unknown" {
        (
            "deployment_stop_unconfirmed",
            "error",
            "Deployment stop unconfirmed",
        )
    } else if failed_candidate {
        ("deployment_failed", "error", "Deployment failed")
    } else {
        ("deployment_stopped", "info", "Deployment stopped")
    };
    #[cfg(test)]
    if job.deployment == "p06-stop-order" && code == "deployment_stopped" {
        let gate = stop_log_gate();
        gate.0.wait();
        gate.1.wait();
    }
    #[cfg(test)]
    if job.deployment == "r06-stop-log-stall" {
        let gate = stalled_stop_log_gate();
        gate.0.wait();
        gate.1.wait();
    }
    if let Ok(logs) = &logs {
        let _ = logs.append_event_context(
            "isos",
            level,
            code,
            message,
            log_store::LogContext {
                run_id: Some(&terminal.run_id),
                correlation_id: Some(&job.correlation),
                node_path: None,
                fields: json!({"outcome":terminal.outcome,"reason_code":terminal.reason_code}),
            },
        );
    } else {
        log_store::record_stop_event_loss();
    }
}

fn queue_stop_events(
    paths: &config::Paths,
    deployment: &str,
    run_id: &str,
    correlation: String,
) -> Option<SyncSender<(dependent_stop::Terminal, bool)>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let Some(queue) = stop_log_queue() else {
        log_store::record_stop_event_loss();
        log_store::record_stop_event_loss();
        return None;
    };
    let job = StopLogJob {
        paths: paths.clone(),
        deployment: deployment.into(),
        run_id: run_id.into(),
        correlation,
        terminal: receiver,
    };
    if queue.try_send(job).is_err() {
        log_store::record_stop_event_loss();
        log_store::record_stop_event_loss();
        None
    } else {
        Some(sender)
    }
}

fn log_deployment(paths: &config::Paths, name: &str, run_id: &str, code: &str, level: &str) {
    #[cfg(test)]
    if name == "p06-stop-order" && code == "deployment_stopped" {
        let gate = stop_log_gate();
        gate.0.wait();
        gate.1.wait();
    }
    if code == "deployment_starting" {
        RUN_STARTED_IN_REQUEST.with(|started| started.set(true));
    }
    if let Ok(logs) = log_store::LogStore::new(paths, name) {
        let correlation = next_correlation_id();
        let message = match code {
            "deployment_starting" => "Deployment starting",
            "deployment_ready" => "Deployment ready",
            "deployment_failed" => "Deployment failed",
            "deployment_stopped" => "Deployment stopped",
            _ => "Deployment state changed",
        };
        let _ = logs.append_event_context(
            "isos",
            level,
            code,
            message,
            log_store::LogContext {
                run_id: Some(run_id),
                correlation_id: Some(&correlation),
                node_path: None,
                fields: json!({}),
            },
        );
    }
}

fn stop_active(
    paths: &config::Paths,
    name: &str,
    active: &mut ActiveDeployment,
    failed_candidate: bool,
) -> (Vec<String>, dependent_stop::Terminal) {
    let already = active.supervisor.terminal().is_some();
    let correlation = next_correlation_id();
    let pending_event = if already {
        None
    } else {
        queue_stop_events(paths, name, active.supervisor.run_id(), correlation)
    };
    let (stopped, terminal) = active.supervisor.stop_with_terminal();
    if let Some(sender) = pending_event
        && sender
            .try_send((terminal.clone(), failed_candidate))
            .is_err()
    {
        log_store::record_stop_event_loss();
    }
    if terminal.outcome == "unknown" {
        active.last_error = Some("dependent stop unconfirmed; owner recovery required".into());
    }
    (stopped, terminal)
}

fn log_prelaunch_failure(
    paths: &config::Paths,
    name: &str,
    error: &anyhow::Error,
    correlation: &str,
) {
    let Some(logs) = log_store::LogStore::new(paths, name).ok() else {
        return;
    };
    let diagnostic = error.chain().find_map(|cause| {
        cause
            .downcast_ref::<graph_compat::Diagnostic>()
            .map(|d| (d.code, d.path.as_str()))
            .or_else(|| {
                cause
                    .downcast_ref::<crate::packages::PackageFailure>()
                    .map(|d| (d.code, d.path.as_str()))
            })
            .or_else(|| {
                cause
                    .downcast_ref::<crate::generated_v2::Diagnostic>()
                    .map(|d| (d.code, d.path.as_str()))
            })
            .or_else(|| {
                cause
                    .downcast_ref::<config::StopBudgetDiagnostic>()
                    .map(|d| (d.code, d.path))
            })
            .or_else(|| {
                cause
                    .downcast_ref::<crate::control_grants::Diagnostic>()
                    .map(|d| (d.code, d.path.as_str()))
            })
    });
    let mut fields = serde_json::Map::new();
    if let Some((code, path)) = diagnostic {
        fields.insert("diagnostic_code".into(), json!(code));
        if path.len() <= 256 && !path.chars().any(char::is_control) {
            fields.insert("diagnostic_path".into(), json!(path));
        }
    }
    let _ = logs.append_event_context(
        "isos",
        "error",
        "deployment_preflight_failed",
        "Deployment preflight failed before launch",
        log_store::LogContext {
            run_id: Some(manager_session_id()),
            correlation_id: Some(correlation),
            node_path: None,
            fields: Value::Object(fields),
        },
    );
}

fn log_package_load_failure(
    paths: &config::Paths,
    name: &str,
    error: &anyhow::Error,
    correlation: &str,
) {
    let Some(failure) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<process::PackageLoadFailure>())
    else {
        return;
    };
    if let Ok(logs) = log_store::LogStore::new(paths, name) {
        let _ = logs.append_event_context(
            "packages", "error", "package_load_failed",
            "Deployment package load failed before launch",
            log_store::LogContext {
                run_id: Some(manager_session_id()),
                correlation_id: Some(correlation),
                node_path: failure.node_path.as_deref(),
                fields: json!({"package_id":failure.id,"package_version":failure.version,"stage":failure.stage,
                    "source_kind":null,"source_commit":null,"asset_sha256":null,"release_sha256":null,"payload_sha256":null}),
            },
        );
    }
}

fn log_request_preflight_failure(
    paths: &config::Paths,
    name: &str,
    error: &anyhow::Error,
    correlation: &str,
) {
    log_package_load_failure(paths, name, error, correlation);
    log_prelaunch_failure(paths, name, error, correlation);
}

fn preflight_packages(
    paths: &config::Paths,
    name: &str,
    source: &str,
    revision: &str,
) -> Result<Vec<process::ProcessSpec>> {
    let graph: Value = serde_json::from_str(source)?;
    let selected = if graph["version"] == "0.1.0" {
        Some(crate::generated_v2::selected_v2_root(
            paths, name, &graph, revision,
        )?)
    } else {
        None
    };
    reader_features::check_setup_basics(paths, &graph)?;
    let mut specs = process::project_with_packages(paths, source)?;
    core_setup::verify_launch_inputs(paths, name)?;
    if let Some(selected) = selected {
        crate::control_grants::validate_plan(&specs)?;
        let generated = selected.join("eos/graph.json");
        process::use_generated_eos_profile(&mut specs, &generated)?;
        process::use_generated_egia_config(&mut specs, &selected.join("egia"))?;
    }
    let catalog = process::catalog_for_graph(paths, &graph)?;
    reader_features::check(paths, &graph, &specs, &catalog)?;
    if graph["version"] == "0.1.0"
        && let Some(eos) = specs.iter().find(|spec| spec.id == "eos")
    {
        let output = std::process::Command::new(&eos.settings.command[0])
            .arg("--capabilities")
            .output()?;
        ensure!(
            output.status.success() && output.stdout.len() <= 65_536,
            "selected EOS managed stop support is unavailable"
        );
        let capabilities: Value = serde_json::from_slice(&output.stdout)?;
        ensure!(
            capabilities["features"]
                .as_array()
                .is_some_and(|features| features
                    .iter()
                    .any(|value| value.as_str() == Some("managed-stop-v1"))),
            "selected EOS lacks managed-stop-v1"
        );
    }
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
        graph_migration::prepare(paths, name, &proposal).map_err(anyhow::Error::new)?;
        Ok(())
    } else {
        crate::packages::verify_graph_packages(paths, &saved.value)?;
        graph_compat::admit_legacy_mapped(&saved.value, process::catalog().ok().as_ref())?;
        process::validate_full_graph(&paths.node_graph_path(name))
    }
}

fn verify_selected_before_reader(
    paths: &config::Paths,
    name: &str,
    source: &str,
    revision: &str,
) -> Result<()> {
    let saved = graph_compat::parse(source.as_bytes())?;
    if saved.value["version"] == "0.1.0" {
        crate::generated_v2::selected_v2_root(paths, name, &saved.value, revision)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn restart_processes(
    paths: &config::Paths,
    managers: &mut Deployments,
    name: &str,
    revision: &str,
    specs: &[process::ProcessSpec],
) -> Result<Vec<String>> {
    let plan = crate::generated_v2::secret_plan(specs)?;
    restart_processes_with_plan(paths, managers, name, revision, specs, &plan, false)
}

fn restart_processes_with_plan(
    paths: &config::Paths,
    managers: &mut Deployments,
    name: &str,
    revision: &str,
    specs: &[process::ProcessSpec],
    plan: &crate::generated_v2::SecretPlan,
    managed: bool,
) -> Result<Vec<String>> {
    if !managers.contains_key(name) {
        ensure_launch_history(paths, name)?;
    }
    if managers
        .get(name)
        .and_then(|active| active.supervisor.terminal())
        .is_some_and(|terminal| terminal.outcome == "unknown")
    {
        bail!("dependent stop unconfirmed; replacement blocked");
    }
    if managed {
        crate::control_grants::validate_plan(specs)?;
    }
    if let Some(active) = managers.get(name) {
        active
            .supervisor
            .verify_control_grants(active.managed && active.last_error.is_none())?;
    }
    let old_plan = managers
        .get(name)
        .map(|active| crate::generated_v2::secret_plan(&active.supervisor.specs()))
        .transpose()?;
    let mut supervisor = process::Supervisor::new(paths, name)?;
    if managed {
        supervisor.prepare_control_grants(revision)?;
    }
    let candidate_run = supervisor.run_id().to_owned();
    let mut previous = managers.remove(name).map(|mut active| {
        let old_specs = active.supervisor.specs();
        let (_, terminal) = stop_active(paths, name, &mut active, false);
        (active, old_specs, terminal)
    });
    if previous
        .as_ref()
        .is_some_and(|(_, _, terminal)| terminal.outcome == "unknown")
    {
        let (active, _, _) = previous.take().expect("checked previous stop");
        managers.insert(name.to_owned(), active);
        bail!("dependent stop unconfirmed; replacement blocked");
    }
    // Replacing a run is ordered after the previous children have been reaped.
    // The new scope is durable before any replacement child can start.
    publish_run_scope(&supervisor)?;
    supervisor.prepare_run(specs, revision)?;
    log_deployment(paths, name, &candidate_run, "deployment_starting", "info");
    let attempt = (|| -> Result<Vec<String>> {
        let started = supervisor.start_with_secrets(specs, plan)?;
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
            let mut failed = ActiveDeployment {
                active_revision: revision.to_owned(),
                managed,
                supervisor,
                last_error: None,
                expected_processes: specs.len(),
            };
            let (_, rollback) = stop_active(paths, name, &mut failed, true);
            if rollback.outcome == "unknown" {
                managers.insert(name.to_owned(), failed);
                bail!("candidate stop unconfirmed; previous generation cannot be restored");
            }
            if let Some((mut active, old_specs, _)) = previous {
                let recovery_run = next_run_id();
                active.supervisor.set_run_id(recovery_run.clone());
                if let Err(recovery_error) = if active.managed {
                    active
                        .supervisor
                        .prepare_control_grants(&active.active_revision)
                } else {
                    Ok(())
                } {
                    log_deployment(paths, name, &recovery_run, "deployment_failed", "error");
                    active.last_error = Some(format!(
                        "restart failed: {error:#}; recovery grant preparation failed: {recovery_error}"
                    ));
                    managers.insert(name.to_owned(), active);
                    bail!(
                        "restart failed: {error:#}; recovery grant preparation failed: {recovery_error}"
                    );
                }
                publish_run_scope(&active.supervisor)?;
                active
                    .supervisor
                    .prepare_run(&old_specs, &active.active_revision)?;
                log_deployment(paths, name, &recovery_run, "deployment_starting", "info");
                let empty = crate::generated_v2::SecretPlan::new();
                let restore = active
                    .supervisor
                    .start_with_secrets(&old_specs, old_plan.as_ref().unwrap_or(&empty))
                    .and_then(|_| {
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
                        log_deployment(paths, name, &recovery_run, "deployment_ready", "info");
                        managers.insert(name.to_owned(), active);
                        bail!("restart failed; previous revision restored: {error:#}");
                    }
                    Err(restore_error) => {
                        log_deployment(paths, name, &recovery_run, "deployment_failed", "error");
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
    log_deployment(paths, name, &candidate_run, "deployment_ready", "info");
    managers.insert(
        name.to_owned(),
        ActiveDeployment {
            active_revision: revision.to_owned(),
            managed,
            supervisor,
            last_error: None,
            expected_processes: specs.len(),
        },
    );
    Ok(started)
}

fn start_prepared_processes(
    paths: &config::Paths,
    managers: &mut Deployments,
    name: &str,
    revision: &str,
    specs: &[process::ProcessSpec],
    plan: &crate::generated_v2::SecretPlan,
    managed: bool,
) -> Result<(Vec<String>, String)> {
    let live_confirmed = managers
        .get(name)
        .and_then(|active| active.supervisor.terminal())
        .is_some_and(|terminal| terminal.outcome == "confirmed");
    if managed {
        crate::control_grants::validate_plan(specs)?;
    }
    if managers
        .get(name)
        .and_then(|active| active.supervisor.terminal())
        .is_some()
    {
        let previous = managers.remove(name).expect("terminal run exists");
        if previous
            .supervisor
            .terminal()
            .is_some_and(|terminal| terminal.outcome == "unknown")
        {
            managers.insert(name.to_owned(), previous);
            bail!("dependent stop unconfirmed; launch blocked");
        }
    }
    if let Some(active) = managers.get_mut(name) {
        ensure!(
            active.managed == managed,
            "saved graph management mode changed; restart required"
        );
        ensure!(
            active.active_revision == revision,
            "saved graph changed while running; stop before restart"
        );
        active.supervisor.verify_specs(specs)?;
    } else {
        if !live_confirmed {
            ensure_launch_history(paths, name)?;
        }
        let supervisor = process::Supervisor::new(paths, name)?;
        let mut supervisor = supervisor;
        if managed {
            supervisor.prepare_control_grants(revision)?;
        }
        publish_run_scope(&supervisor)?;
        supervisor.prepare_run(specs, revision)?;
        log_deployment(
            paths,
            name,
            supervisor.run_id(),
            "deployment_starting",
            "info",
        );
        managers.insert(
            name.to_owned(),
            ActiveDeployment {
                active_revision: revision.to_owned(),
                managed,
                supervisor,
                last_error: None,
                expected_processes: specs.len(),
            },
        );
    }
    let active = managers.get_mut(name).expect("start inserted active run");
    let attempt = active
        .supervisor
        .start_with_secrets(specs, plan)
        .and_then(|started| {
            std::thread::sleep(Duration::from_millis(100));
            let running = active.supervisor.status()?;
            ensure!(
                running.len() == specs.len(),
                "a managed process exited during startup; inspect categorized child logs"
            );
            Ok(started)
        });
    let started = match attempt {
        Ok(started) => started,
        Err(error) => {
            let mut failed = managers.remove(name).expect("failed run exists");
            let _ = stop_active(paths, name, &mut failed, true);
            managers.insert(name.to_owned(), failed);
            return Err(error);
        }
    };
    let active = managers.get_mut(name).expect("active run exists");
    active.last_error = None;
    if !started.is_empty() {
        log_deployment(
            paths,
            name,
            active.supervisor.run_id(),
            "deployment_ready",
            "info",
        );
    }
    Ok((started, active.supervisor.run_id().to_owned()))
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
            ensure!(log_store::valid_source(&event.source), "invalid log source");
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
            let correlation = next_correlation_id();
            log_store::LogStore::new(paths, &name)?.append_event_context(
                "legacy_external",
                &event.level,
                "legacy_external_event",
                "Legacy external event received",
                log_store::LogContext {
                    run_id: Some(manager_session_id()),
                    correlation_id: Some(&correlation),
                    node_path: None,
                    fields: json!({}),
                },
            )?;
            Ok(json!({"accepted":true}))
        }
        "manager_shutdown" => {
            tracing::info!(
                event_code = "manager_shutdown",
                "IsOS manager shutdown requested"
            );
            let mut stopped = Vec::new();
            let mut terminals = Vec::new();
            for (deployment, active) in managers.iter_mut() {
                let (ids, terminal) = stop_active(paths, deployment, active, false);
                stopped.extend(ids);
                terminals.push(terminal);
            }
            let services = service_manager.stop_all();
            INTERRUPTED.store(true, Ordering::SeqCst);
            Ok(
                json!({"stopped":stopped,"terminal":terminals,"services":services,"manager":"stopping"}),
            )
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
            let revision = graph_compat::revision(source.as_bytes());
            if let Some(expected) = request.revision.as_deref() {
                ensure!(
                    expected == revision,
                    "saved graph revision changed; reload before restart"
                );
            }
            verify_selected_before_reader(paths, &name, &source, &revision)?;
            validate_saved_graph(paths, &name, &source)?;
            let specs = preflight_packages(paths, &name, &source, &revision)?;
            core_setup::prepare_run(paths, &name, &revision)?;
            let plan = crate::generated_v2::secret_plan(&specs)?;
            ensure!(
                starter::revision(&path)? == revision,
                "saved graph revision changed during core setup; reload before restart"
            );
            let managed = serde_json::from_str::<Value>(&source)?["version"] == "0.1.0";
            let started = restart_processes_with_plan(
                paths, managers, &name, &revision, &specs, &plan, managed,
            )?;
            Ok(json!({"started":started,"active_revision":revision}))
        }
        "deployment_start" => {
            let path = paths.node_graph_path(&name);
            let source = fs::read_to_string(&path)?;
            let revision = graph_compat::revision(source.as_bytes());
            if let Some(expected) = request.revision.as_deref() {
                ensure!(
                    expected == revision,
                    "saved graph revision changed; reload before run"
                );
            }
            verify_selected_before_reader(paths, &name, &source, &revision)?;
            validate_saved_graph(paths, &name, &source)?;
            let specs = preflight_packages(paths, &name, &source, &revision)?;
            core_setup::prepare_run(paths, &name, &revision)?;
            let plan = crate::generated_v2::secret_plan(&specs)?;
            ensure!(
                starter::revision(&path)? == revision,
                "saved graph revision changed during core setup; reload before run"
            );
            let managed = serde_json::from_str::<Value>(&source)?["version"] == "0.1.0";
            let (started, run_id) = start_prepared_processes(
                paths, managers, &name, &revision, &specs, &plan, managed,
            )?;
            Ok(json!({"started":started,"active_revision":revision,"run_id":run_id}))
        }
        "deployment_stop" => {
            if let Some(expected) = request.revision.as_deref() {
                ensure!(
                    starter::revision(&paths.node_graph_path(&name))? == expected,
                    "saved graph revision changed; reload before stop"
                );
            }
            let (stopped, terminal) = if let Some(active) = managers.get_mut(&name) {
                let (ids, terminal) = stop_active(paths, &name, active, false);
                (ids, Some(terminal))
            } else {
                let identity = log_store::LogStore::new(paths, &name)
                    .ok()
                    .map(|logs| logs.run_identity());
                let terminal = identity.and_then(|(run_id, state)| {
                    if state == "known" {
                        run_id.map(|run_id| dependent_stop::recover(paths, &name, &run_id))
                    } else {
                        None
                    }
                });
                (Vec::new(), terminal)
            };
            Ok(json!({"stopped": stopped,"terminal":terminal}))
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
                    Ok(processes) => {
                        if active.supervisor.terminal().is_none()
                            && processes.len() < active.expected_processes
                            && active.last_error.is_none()
                        {
                            log_deployment(
                                paths,
                                &name,
                                active.supervisor.run_id(),
                                "deployment_failed",
                                "error",
                            );
                            active.last_error = Some("a managed process exited".into());
                        }
                        processes
                    }
                    Err(error) => {
                        active.last_error = Some(error.to_string());
                        let _ = stop_active(paths, &name, active, false);
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
            let (run_id, run_id_state) = match log_store::LogStore::new(paths, &name) {
                Ok(logs) => logs.run_identity(),
                Err(_) => (None, "unavailable"),
            };
            let terminal = active
                .and_then(|active| active.supervisor.terminal())
                .or_else(|| {
                    if active.is_none() && run_id_state == "known" {
                        run_id
                            .as_deref()
                            .map(|id| dependent_stop::recover(paths, &name, id))
                    } else {
                        None
                    }
                });
            let state = if run_id_state == "unavailable"
                || supervisor_failed
                || terminal
                    .as_ref()
                    .is_some_and(|value| value.outcome == "unknown")
            {
                "degraded"
            } else if active.is_none() || terminal.is_some() {
                "stopped"
            } else {
                "running"
            };
            let last_error = last_error.or_else(|| {
                if state == "degraded" {
                    Some("dependent stop unconfirmed; owner recovery required".into())
                } else {
                    None
                }
            });
            let (log_ingest_rejected, log_write_failed) = log_store::counters();
            Ok(
                json!({"schema_version":"1.0.0","saved_revision":saved_revision,"active_revision":active_revision,"state":state,"last_error":last_error,"package":pinned_package,"processes":processes,"run_id":run_id,"run_id_state":run_id_state,"terminal":terminal,"log_ingest_rejected":log_ingest_rejected,"log_ingest_rejected_by_reason":log_store::rejection_reasons(),"log_write_failed":log_write_failed}),
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
            matches!(
                request.op.as_str(),
                "deployment_start" | "deployment_restart" | "deployment_stop"
            )
        })
        .map(|request| config::deployment_name(request.deployment.clone()));
    let clear_error = request.as_ref().ok().is_some_and(|request| {
        matches!(
            request.op.as_str(),
            "deployment_start" | "deployment_restart"
        )
    });
    RUN_STARTED_IN_REQUEST.with(|started| started.set(false));
    let request_correlation = next_correlation_id();
    let outcome = request
        .map_err(anyhow::Error::from)
        .and_then(|request| api_call(paths, managers, service_manager, request));
    if let Err(error) = &outcome {
        if let Some(name) = failed_deployment.as_deref().filter(|name| {
            clear_error
                && process::validate_deployment_name(name).is_ok()
                && !RUN_STARTED_IN_REQUEST.with(std::cell::Cell::get)
        }) {
            log_request_preflight_failure(paths, name, error, &request_correlation);
        }
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
            data: error
                .chain()
                .find_map(|cause| {
                    cause
                        .downcast_ref::<graph_compat::Diagnostic>()
                        .map(|diagnostic| json!({"diagnostics":[diagnostic]}))
                        .or_else(|| {
                            cause
                                .downcast_ref::<crate::packages::PackageFailure>()
                                .map(|failure| json!({"diagnostics":[failure.diagnostic()]}))
                        })
                        .or_else(|| {
                            cause
                                .downcast_ref::<crate::generated_v2::Diagnostic>()
                                .map(|diagnostic| json!({"diagnostics":[diagnostic]}))
                        })
                        .or_else(|| {
                            cause
                                .downcast_ref::<config::StopBudgetDiagnostic>()
                                .map(|diagnostic| json!({"diagnostics":[diagnostic]}))
                        })
                        .or_else(|| {
                            cause
                                .downcast_ref::<crate::control_grants::Diagnostic>()
                                .map(|diagnostic| json!({"diagnostics":[diagnostic]}))
                        })
                        .or_else(|| {
                            cause.downcast_ref::<ScopePublishFailure>().map(|_| json!({
                                "diagnostics":[{"path":"/run_id","code":"log_scope_publish_failed",
                                    "message":"Log scope publication failed"}]
                            }))
                        })
                })
                .unwrap_or(Value::Null),
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
    // A manager session boundary is explicit; one-shot CLI reads must not
    // silently rotate the pre-run scope between commands.
    log_store::LogStore::new(paths, deployment)?.begin_manager_session()?;
    let mut managers = Deployments::new();
    let mut service_manager = services::ServiceManager::new(paths, deployment)?;
    println!("IsOS API listening on {}", paths.socket().display());
    while !INTERRUPTED.load(Ordering::SeqCst) {
        for (name, active) in managers.iter_mut() {
            if let Err(error) = active.supervisor.poll() {
                if active.last_error.is_none() {
                    log_deployment(
                        paths,
                        name,
                        active.supervisor.run_id(),
                        "deployment_failed",
                        "error",
                    );
                }
                active.last_error = Some(error.to_string());
                tracing::error!(
                    event_code = "deployment_supervisor_failed",
                    "process supervisor: {error:#}"
                );
                eprintln!("process supervisor: {error:#}");
            } else if active.supervisor.status_snapshot().len() < active.expected_processes
                && active.last_error.is_none()
            {
                log_deployment(
                    paths,
                    name,
                    active.supervisor.run_id(),
                    "deployment_failed",
                    "error",
                );
                active.last_error = Some("a managed process exited".into());
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
    let mut unknown = false;
    for (deployment, active) in managers.iter_mut() {
        let (_, terminal) = stop_active(paths, deployment, active, false);
        unknown |= terminal.outcome == "unknown";
    }
    service_manager.stop_all();
    tracing::info!(event_code = "manager_stopped", "IsOS manager stopped");
    fs::remove_file(paths.socket())?;
    ensure!(
        !unknown,
        "dependent stop unconfirmed during manager shutdown"
    );
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
    if !response.ok {
        return Err(anyhow::Error::new(RemoteDiagnosticError {
            message: response.error.unwrap_or_else(|| "manager error".into()),
            data: response.data,
        }));
    }
    Ok(response.data)
}

#[derive(Debug)]
pub(crate) struct RemoteDiagnosticError {
    pub message: String,
    pub data: Value,
}
impl std::fmt::Display for RemoteDiagnosticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for RemoteDiagnosticError {}

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
    use std::net::Shutdown;

    #[test]
    fn owner_stop_is_idempotent_and_recovered_status_is_unknown() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let spec = process::ProcessSpec {
            id: "stub".into(),
            path: "isos/process/component/stub".into(),
            settings: serde_json::from_value(json!({"command":["/bin/sleep","30"]})).unwrap(),
            package: None,
            fingerprint: None,
        };
        let mut managers = Deployments::new();
        restart_processes(&paths, &mut managers, "test", "a", &[spec]).unwrap();
        let mut services = services::ServiceManager::new(&paths, "test").unwrap();
        let request = |op: &str| ApiRequest {
            schema_version: 1,
            op: op.into(),
            deployment: Some("test".into()),
            role: None,
            revision: None,
            service_id: None,
            cursor: None,
            limit: None,
            log: None,
        };
        let first = api_call(
            &paths,
            &mut managers,
            &mut services,
            request("deployment_stop"),
        )
        .unwrap();
        assert_eq!(first["stopped"], json!(["stub"]));
        assert_eq!(first["terminal"]["outcome"], "confirmed");
        let second = api_call(
            &paths,
            &mut managers,
            &mut services,
            request("deployment_stop"),
        )
        .unwrap();
        assert_eq!(second["stopped"], json!([]));
        assert_eq!(second["terminal"], first["terminal"]);
        let status = api_call(
            &paths,
            &mut managers,
            &mut services,
            request("deployment_status"),
        )
        .unwrap();
        assert_eq!(status["state"], "stopped");
        assert_eq!(status["terminal"], first["terminal"]);
        let mut recovered = Deployments::new();
        let status = api_call(
            &paths,
            &mut recovered,
            &mut services,
            request("deployment_status"),
        )
        .unwrap();
        assert_eq!(status["state"], "degraded");
        assert_eq!(status["terminal"]["outcome"], "unknown");
        assert_eq!(status["terminal"]["reason_code"], "stop_interrupted");
    }

    #[test]
    fn grant_plan_preflight_leaves_active_pid_and_current_run_scope_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        let active_spec = process::ProcessSpec {
            id: "stub".into(),
            path: "isos/process/component/stub".into(),
            settings: serde_json::from_value(
                json!({"command":["/bin/sleep","30"],"start_timeout_ms":500}),
            )
            .unwrap(),
            package: None,
            fingerprint: None,
        };
        let mut managers = Deployments::new();
        restart_processes(&paths, &mut managers, "test", "old", &[active_spec]).unwrap();
        let original = managers.get_mut("test").unwrap();
        let old_run = original.supervisor.run_id().to_owned();
        let old_pid = original.supervisor.status().unwrap()[0].pid;
        let mut candidate = process::ProcessSpec {
            id: "eos".into(),
            path: "isos/process/component/eos".into(),
            settings: serde_json::from_value(json!({"command":["/bin/true"],"role":"eos"}))
                .unwrap(),
            package: None,
            fingerprint: None,
        };
        let revision = "a".repeat(64);
        let empty = crate::generated_v2::SecretPlan::new();
        let error = restart_processes_with_plan(
            &paths,
            &mut managers,
            "test",
            &revision,
            &[candidate.clone()],
            &empty,
            true,
        )
        .unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<crate::control_grants::Diagnostic>()
                .unwrap()
                .code,
            "control_grant_delivery_failed"
        );
        log_prelaunch_failure(&paths, "test", &error, "p-grant");
        let preflight = log_store::LogStore::new(&paths, "test")
            .unwrap()
            .read("isos", None, 20)
            .unwrap();
        let event = preflight["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|line| line["event_code"] == "deployment_preflight_failed")
            .unwrap();
        assert_eq!(event["run_id"], manager_session_id());
        assert_eq!(event["correlation_id"], "p-grant");
        assert_eq!(
            event["fields"]["diagnostic_code"],
            "control_grant_delivery_failed"
        );
        let active = managers.get_mut("test").unwrap();
        assert_eq!(active.supervisor.status().unwrap()[0].pid, old_pid);
        assert_eq!(active.supervisor.run_id(), old_run);
        assert_eq!(active.active_revision, "old");
        assert_eq!(
            log_store::LogStore::new(&paths, "test")
                .unwrap()
                .run_identity()
                .0
                .as_deref(),
            Some(old_run.as_str())
        );
        candidate.settings.args = vec!["--run-id=spoof".into()];
        let mut config = candidate.clone();
        config.id = "config".into();
        config.path = "isos/process/component/config".into();
        config.settings.args.clear();
        let error = restart_processes_with_plan(
            &paths,
            &mut managers,
            "test",
            &revision,
            &[candidate, config],
            &empty,
            true,
        )
        .unwrap_err();
        let diagnostic = error
            .downcast_ref::<crate::control_grants::Diagnostic>()
            .unwrap();
        assert_eq!(
            diagnostic.path,
            "/cores/isos/layers/process/groups/component/nodes/eos/settings/args/0"
        );
        assert_eq!(
            managers
                .get_mut("test")
                .unwrap()
                .supervisor
                .status()
                .unwrap()[0]
                .pid,
            old_pid
        );
        managers.get_mut("test").unwrap().supervisor.stop();
    }

    #[test]
    fn failed_managed_replacement_rotates_grants_again_for_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        let script = temp.path().join("owner-stub.sh");
        fs::write(
            &script,
            concat!(
                "#!/bin/sh\nset -eu\nrun=missing\n",
                "while [ \"$#\" -gt 0 ]; do\n",
                "  if [ \"$1\" = --run-id ]; then shift; run=\"$1\"; fi\n",
                "  shift\ndone\n",
                "if [ \"$STUB_FAIL\" = 1 ]; then exit 1; fi\n",
                "if [ \"$STUB_ID\" = eos ]; then fields='{\"state\":\"ready\",\"node_count\":1}'; message='EOS runtime ready'; ",
                "else fields='{\"state\":\"ready\"}'; message='EOS config ready'; fi\n",
                "printf '{\"timestamp\":\"2026-11-01T10:00:01.000Z\",\"level\":\"info\",",
                "\"source\":\"%s\",\"core\":\"eos\",\"event_code\":\"%s_ready\",",
                "\"run_id\":\"%s\",\"message\":\"%s\",\"fields\":%s}\\n' ",
                "\"$STUB_ID\" \"$STUB_ID\" \"$run\" \"$message\" \"$fields\" >&2\n",
                "sleep 10\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let make = |id: &str, fail: bool| process::ProcessSpec {
            id: id.into(),
            path: format!("isos/process/component/{id}"),
            settings: serde_json::from_value(json!({
                "command":[script], "role":"eos", "start_timeout_ms":1000,
                "env":{"STUB_ID":id,"STUB_FAIL":if fail {"1"} else {"0"}}
            }))
            .unwrap(),
            package: None,
            fingerprint: None,
        };
        let old_specs = [make("eos", false), make("config", false)];
        let mut managers = Deployments::new();
        let revision = "a".repeat(64);
        let empty = crate::generated_v2::SecretPlan::new();
        restart_processes_with_plan(
            &paths,
            &mut managers,
            "test",
            &revision,
            &old_specs,
            &empty,
            true,
        )
        .unwrap();
        let old_run = managers.get("test").unwrap().supervisor.run_id().to_owned();
        let old_bundle = paths.runtime.join(format!("control-grants/test/{old_run}"));
        assert!(old_bundle.is_dir());
        let old_pid = managers
            .get_mut("test")
            .unwrap()
            .supervisor
            .status()
            .unwrap()[0]
            .pid;
        let grant = old_bundle.join("egia-control.grant");
        fs::set_permissions(&grant, fs::Permissions::from_mode(0o644)).unwrap();
        let error = restart_processes_with_plan(
            &paths,
            &mut managers,
            "test",
            &revision,
            &old_specs,
            &empty,
            true,
        )
        .unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<crate::control_grants::Diagnostic>()
                .unwrap()
                .code,
            "control_grant_unsafe_path"
        );
        assert_eq!(
            managers
                .get_mut("test")
                .unwrap()
                .supervisor
                .status()
                .unwrap()[0]
                .pid,
            old_pid
        );
        assert_eq!(managers.get("test").unwrap().supervisor.run_id(), old_run);
        fs::set_permissions(&grant, fs::Permissions::from_mode(0o600)).unwrap();
        let mut candidate_specs = [make("eos", false), make("config", false)];
        candidate_specs[1]
            .settings
            .env
            .insert("ISOS_R03_TEST_FAIL_DELIVERY".into(), "1".into());
        let error = restart_processes_with_plan(
            &paths,
            &mut managers,
            "test",
            &revision,
            &candidate_specs,
            &empty,
            true,
        )
        .unwrap_err();
        assert!(error.to_string().contains("previous revision restored"));
        assert!(error.to_string().contains("control_grant_delivery_failed"));
        let recovered = managers.get_mut("test").unwrap();
        let recovery_run = recovered.supervisor.run_id().to_owned();
        assert_ne!(recovery_run, old_run);
        assert_eq!(recovered.supervisor.status().unwrap().len(), 2);
        assert!(!old_bundle.exists());
        assert!(
            paths
                .runtime
                .join(format!("control-grants/test/{recovery_run}"))
                .exists()
        );
        let events = log_store::LogStore::new(&paths, "test")
            .unwrap()
            .read("all", None, 100)
            .unwrap();
        let candidate_run = events["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|line| line["event_code"] == "deployment_failed")
            .unwrap()["run_id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_ne!(candidate_run, old_run);
        assert_ne!(candidate_run, recovery_run);
        assert!(
            !paths
                .runtime
                .join(format!("control-grants/test/{candidate_run}"))
                .exists()
        );
        recovered.supervisor.stop();
    }

    #[test]
    fn rejected_start_is_logged_under_manager_session_before_reply() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let mut managers = Deployments::new();
        let mut services = services::ServiceManager::new(&paths, "default").unwrap();
        let (mut client, server) = UnixStream::pair().unwrap();
        client
            .write_all(
                b"{\"schema_version\":1,\"op\":\"deployment_start\",\"deployment\":\"default\"}\n",
            )
            .unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        handle_client(&paths, &mut managers, &mut services, server).unwrap();
        let mut reply = String::new();
        client.read_to_string(&mut reply).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&reply).unwrap()["ok"], false);
        assert!(managers.is_empty());
        let lines = log_store::LogStore::new(&paths, "default")
            .unwrap()
            .read("isos", None, 10)
            .unwrap();
        let event = lines["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|line| line["event_code"] == "deployment_preflight_failed")
            .unwrap();
        assert_eq!(event["run_id"], manager_session_id());
        assert!(event["correlation_id"].as_str().is_some());
    }

    #[test]
    fn hostile_diagnostic_path_is_omitted_without_losing_preflight_event() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let error = anyhow::Error::new(graph_compat::Diagnostic {
            path: "/cores/bad\nkey".into(),
            code: "unsupported_core",
            message: "unsupported core".into(),
            context: BTreeMap::new(),
        });
        log_prelaunch_failure(&paths, "default", &error, "p-hostile");
        let lines = log_store::LogStore::new(&paths, "default")
            .unwrap()
            .read("isos", None, 10)
            .unwrap();
        assert_eq!(
            lines["lines"][0]["event_code"],
            "deployment_preflight_failed"
        );
        assert_eq!(
            lines["lines"][0]["fields"]["diagnostic_code"],
            "unsupported_core"
        );
        assert!(lines["lines"][0]["fields"]["diagnostic_path"].is_null());
    }

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
            node_path: Some("/cores/eos/layers/process/groups/mujoco/nodes/sim".into()),
            source: anyhow::anyhow!("private path /tmp/secret"),
        });
        log_request_preflight_failure(&paths, "default", &failure, "p-request");
        log_package_load_failure(
            &paths,
            "default",
            &anyhow::anyhow!("unrelated EOS binary missing"),
            "p-other",
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
        assert_eq!(
            line["node_path"],
            "/cores/eos/layers/process/groups/mujoco/nodes/sim"
        );
        assert!(line["fields"]["release_sha256"].is_null());
        assert!(line["fields"]["payload_sha256"].is_null());
        assert_eq!(line["run_id"], manager_session_id());
        assert_eq!(line["correlation_id"], "p-request");
        assert!(!line.to_string().contains("/tmp/secret"));
        let preflight = log_store::LogStore::new(&paths, "default")
            .unwrap()
            .read("isos", None, 10)
            .unwrap();
        assert_eq!(preflight["lines"][0]["correlation_id"], "p-request");
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
            .read("legacy_external", None, 10)
            .unwrap();
        assert_eq!(stored["lines"][0]["event_code"], "legacy_external_event");
        assert_eq!(
            stored["lines"][0]["message"],
            "Legacy external event received"
        );
        assert!(api_call(&paths, &mut managers, &mut services, make_request("isos")).is_err());
    }

    #[test]
    fn failed_restart_with_unexpected_candidate_exit_blocks_restoration() {
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
        let old_run = managers["test"].supervisor.run_id().to_owned();
        let error = restart_processes(&paths, &mut managers, "test", "new", &[spec("/bin/false")])
            .unwrap_err();
        assert!(error.to_string().contains("candidate stop unconfirmed"));
        assert_eq!(
            managers["test"].supervisor.terminal().unwrap().outcome,
            "unknown"
        );
        let (run, state) = log_store::LogStore::new(&paths, "test")
            .unwrap()
            .run_identity();
        assert_eq!(state, "known");
        assert_ne!(run.unwrap(), old_run);
        assert!(
            restart_processes(&paths, &mut managers, "test", "old", &[spec("/bin/sleep")]).is_err()
        );
    }

    #[test]
    fn failed_initial_start_retains_unknown_and_blocks_retry() {
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
        let plan = crate::generated_v2::SecretPlan::new();
        let mut managers = Deployments::new();
        assert!(
            start_prepared_processes(
                &paths,
                &mut managers,
                "test",
                "revision",
                &[spec("/bin/false")],
                &plan,
                false
            )
            .is_err()
        );
        let terminal = managers["test"].supervisor.terminal().unwrap();
        assert_eq!(terminal.outcome, "unknown");
        let old_run = terminal.run_id.clone();
        assert!(
            start_prepared_processes(
                &paths,
                &mut managers,
                "test",
                "revision",
                &[spec("/bin/sleep")],
                &plan,
                false
            )
            .is_err()
        );
        assert_eq!(managers["test"].supervisor.run_id(), old_run);
    }

    #[test]
    fn scope_publication_failure_blocks_child_spawn() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let spec = process::ProcessSpec {
            id: "stub".into(),
            path: "isos/process/component/stub".into(),
            settings: serde_json::from_value(json!({
                "command": ["/bin/sleep", "30"], "start_timeout_ms": 200
            }))
            .unwrap(),
            package: None,
            fingerprint: None,
        };
        let mut managers = Deployments::new();
        let error = start_prepared_processes(
            &paths,
            &mut managers,
            "p07-publish-fail",
            "revision",
            &[spec],
            &crate::generated_v2::SecretPlan::new(),
            false,
        )
        .unwrap_err();
        assert!(error.downcast_ref::<ScopePublishFailure>().is_some());
        assert!(managers.is_empty());
        let store = log_store::LogStore::new(&paths, "p07-publish-fail").unwrap();
        assert_eq!(store.run_identity(), (None, "none"));
        assert_eq!(
            store.read("all", None, 10).unwrap()["lines"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn managed_scope_publication_failure_retires_prepared_grants() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        let specs = ["eos", "config"].map(|id| process::ProcessSpec {
            id: id.into(),
            path: format!("isos/process/component/{id}"),
            settings: serde_json::from_value(json!({
                "command": ["/bin/sleep", "30"], "role": "eos", "start_timeout_ms": 200
            }))
            .unwrap(),
            package: None,
            fingerprint: None,
        });
        let mut managers = Deployments::new();
        let error = start_prepared_processes(
            &paths,
            &mut managers,
            "p07-publish-fail",
            &"a".repeat(64),
            &specs,
            &crate::generated_v2::SecretPlan::new(),
            true,
        )
        .unwrap_err();
        assert!(error.downcast_ref::<ScopePublishFailure>().is_some());
        assert!(managers.is_empty());
        assert_eq!(
            fs::read_dir(paths.runtime.join("control-grants/p07-publish-fail"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn restart_reaps_old_child_before_stalled_stop_event() {
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
        restart_processes(
            &paths,
            &mut managers,
            "p06-stop-order",
            "old",
            &[spec("/bin/sleep")],
        )
        .unwrap();
        let old_pid = managers["p06-stop-order"].supervisor.status_snapshot()[0].pid;
        let thread = std::thread::spawn(move || {
            let result = restart_processes(
                &paths,
                &mut managers,
                "p06-stop-order",
                "new",
                &[spec("/bin/false")],
            );
            if let Some(active) = managers.get_mut("p06-stop-order") {
                active.supervisor.stop();
            }
            result.is_err()
        });
        let gate = stop_log_gate();
        gate.0.wait();
        let reaped = unsafe { libc::kill(old_pid as i32, 0) } == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        gate.1.wait();
        assert!(thread.join().unwrap());
        assert!(
            reaped,
            "old child remained live while stop event was stalled"
        );
    }

    #[test]
    fn stalled_terminal_log_does_not_hold_stop_response() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let spec = process::ProcessSpec {
            id: "stub".into(),
            path: "isos/process/component/stub".into(),
            settings: serde_json::from_value(json!({"command":["/bin/sleep","30"]})).unwrap(),
            package: None,
            fingerprint: None,
        };
        let mut managers = Deployments::new();
        restart_processes(&paths, &mut managers, "r06-stop-log-stall", "old", &[spec]).unwrap();
        let mut active = managers.remove("r06-stop-log-stall").unwrap();
        let run_id = active.supervisor.run_id().to_owned();
        let log_paths = paths.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let (_, terminal) = stop_active(&paths, "r06-stop-log-stall", &mut active, false);
            sender.send((terminal, started.elapsed())).unwrap();
        });
        let gate = stalled_stop_log_gate();
        gate.0.wait();
        let response = receiver.recv_timeout(Duration::from_secs(1));
        gate.1.wait();
        let (terminal, elapsed) = response.unwrap();
        assert_eq!(terminal.outcome, "confirmed");
        assert!(elapsed < Duration::from_secs(1));
        worker.join().unwrap();
        let logs = log_store::LogStore::new(&log_paths, "r06-stop-log-stall").unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let lines = logs.read("all", None, 100).unwrap();
            let events: Vec<_> = lines["lines"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|line| line["run_id"].as_str() == Some(run_id.as_str()))
                .filter(|line| {
                    matches!(
                        line["event_code"].as_str(),
                        Some("deployment_stopping" | "deployment_stopped")
                    )
                })
                .collect();
            if events.len() == 2 {
                assert_eq!(events[0]["event_code"], "deployment_stopping");
                assert_eq!(events[1]["event_code"], "deployment_stopped");
                assert_eq!(events[0]["correlation_id"], events[1]["correlation_id"]);
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "stop events were not written"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
