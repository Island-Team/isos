mod auth;
mod component;
mod config;
mod core_setup;
mod graph;
mod http;
mod isosui;
mod lifecycle;
mod log_store;
mod manager_log;
mod node_options;
mod node_sets;
mod packages;
mod readiness;
mod release;
mod services;
mod v2_backend;
mod v2_bins;
mod v2_build;
mod v2_process;
mod v2_starter;
mod versions;

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use graph::{Deployment, NodeKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn interrupt_handler(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

fn catch_interrupts() -> Result<()> {
    INTERRUPTED.store(false, Ordering::SeqCst);
    unsafe {
        let handler = interrupt_handler as *const () as libc::sighandler_t;
        ensure!(
            libc::signal(libc::SIGINT, handler) != libc::SIG_ERR,
            "install SIGINT handler"
        );
        ensure!(
            libc::signal(libc::SIGTERM, handler) != libc::SIG_ERR,
            "install SIGTERM handler"
        );
    }
    Ok(())
}

#[derive(Parser)]
#[command(
    name = "isos",
    version,
    about = "Manage Island EOS and EGIA as one local deployment",
    after_help = "MODES\n  dev (default): local source and installed binaries; setup can render configuration.\n  product: exact verified release pins; no source build at Run.\n\nSTART\n  Run `isos` to initialize and open the local workbench.\n  Run `isos <command> --help` for command details."
)]
struct Cli {
    #[arg(long = "generate-completion", hide = true)]
    generate_completion: Option<Shell>,
    #[arg(
        long,
        global = true,
        env = "ISOS_DEPLOYMENT",
        default_value = "default"
    )]
    deployment: String,
    #[command(subcommand)]
    command: Option<CommandLine>,
}

#[derive(Subcommand)]
enum CommandLine {
    /// Build in the owning repository; this command prints migration guidance.
    Build {
        #[arg(default_value = "all", value_parser = ["isos", "eos", "egia", "all", "isosui"])]
        target: String,
        #[arg(long)]
        install_deps: bool,
    },
    /// Report configured core build and launch readiness.
    Version {
        #[command(subcommand)]
        command: VersionCommand,
    },
    /// Initialize the deployment without starting processes.
    Init {
        #[arg(long, value_parser = ["dev", "product"])]
        mode: Option<String>,
    },
    /// Open the local configuration page.
    Open,
    /// Run the deployment graph.
    Run,
    /// Serve the local configuration UI, API, and owner-only control socket.
    Serve {
        #[arg(long)]
        ui_live: bool,
    },
    /// Manage the local Island-port account session.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// Start every enabled deployment node in dependency order.
    Start,
    /// Stop the active deployment and its IsOS-owned processes.
    Stop,
    /// Control only the applied deployment.
    Deployment {
        #[command(subcommand)]
        command: DeploymentCommand,
    },
    /// Stop and start the active deployment using its saved graph.
    Restart,
    /// Show deployment and per-node process status; optionally emit JSON.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Read recent unified logs for a source or all sources.
    Logs {
        #[arg(default_value = "all")]
        source: String,
        #[arg(long, default_value_t = 100)]
        lines: u32,
        #[arg(long, default_value = "info", value_parser = ["trace", "debug", "info", "warn", "error"])]
        level: String,
    },
    /// Check the shared graph, package, binaries, and manager readiness.
    Doctor,
    /// Start, attach to, or stop the EOS component and its dependencies.
    Eos {
        #[command(subcommand)]
        command: ComponentCommand,
    },
    /// Start or stop the EGIA component managed by this deployment.
    Egia {
        #[command(subcommand)]
        command: ComponentCommand,
    },
    /// Initialize, inspect, validate, or apply a deployment graph.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Search, install, verify, link, or remove versioned packages.
    #[command(name = "pkg", alias = "packages")]
    Packages {
        #[command(subcommand)]
        command: PackageCommand,
    },
    /// Inspect skill nodes and manage installed skill packages.
    Skills {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    /// Inspect service nodes and manage installed service packages.
    Services {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// Inspect backend nodes and manage hardware or simulator packages.
    Backends {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    /// Serve the owner-only manager API on a Unix socket.
    Daemon,
    /// Manage the shared v2 node graph and its IsOS-owned direct children.
    V2 {
        #[command(subcommand)]
        command: V2Command,
    },
}

#[derive(Subcommand)]
enum V2Command {
    Validate { file: PathBuf },
    Apply { file: PathBuf },
    Run,
    Stop,
    Status,
}

#[derive(Subcommand)]
enum DeploymentCommand {
    Stop,
    Status,
}

#[derive(Subcommand)]
enum ServiceCommand {
    List,
    Validate,
    Start {
        id: String,
    },
    Stop {
        id: String,
    },
    Restart {
        id: String,
    },
    Status {
        id: Option<String>,
    },
    Logs {
        id: String,
        #[arg(long, default_value_t = 100)]
        lines: usize,
    },
    Adapter {
        #[arg(value_parser = ["models"])]
        provider: String,
        repo: PathBuf,
        alias: String,
    },
    Install(InstallArgs),
    Remove {
        id: String,
        version: String,
    },
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Sign in with the IslandOS GitHub OAuth app.
    Login,
    /// Revoke and remove the locally saved Island-port session.
    Logout,
    /// Show the signed-in Island-port account and role.
    Whoami,
}

#[derive(Subcommand)]
enum VersionCommand {
    Check {
        #[arg(long)]
        json: bool,
    },
    #[command(name = "select", alias = "pin")]
    Pin {
        #[arg(value_parser = ["isos", "eos", "egia", "isosui"])]
        component: String,
        version: String,
    },
}

#[derive(Subcommand)]
enum ComponentCommand {
    Start,
    Stop,
    Restart,
    Status {
        #[arg(long)]
        json: bool,
    },
    Enter,
    Shutdown,
}

#[derive(Subcommand)]
enum ConfigCommand {
    Init {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Validate {
        file: PathBuf,
    },
    Apply {
        file: PathBuf,
    },
    Show,
}

#[derive(Subcommand)]
enum PackageCommand {
    /// Upload an asset and manifest to a draft GitHub Release, then submit for review.
    Upload {
        manifest: PathBuf,
        asset: PathBuf,
    },
    Search {
        #[arg(default_value = "")]
        query: String,
    },
    Info {
        id: String,
        #[arg(long, default_value = "latest")]
        version: String,
    },
    Install(InstallArgs),
    Link {
        id: String,
        version: String,
        path: PathBuf,
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    List {
        #[arg(long)]
        json: bool,
    },
    /// List node sets contributed by installed package releases.
    NodeSets,
    /// Compose one installed node set into a graph and print the preview JSON.
    PreviewSet {
        id: String,
        version: String,
        set_id: String,
        graph: PathBuf,
        #[arg(long)]
        values: Option<PathBuf>,
        #[arg(long)]
        replace_legacy_mujoco: bool,
    },
    Verify {
        manifest: PathBuf,
        asset: PathBuf,
    },
    /// Publish an already-uploaded GitHub release manifest to Island-port.
    Publish {
        #[arg(long)]
        manifest: PathBuf,
    },
    Remove {
        id: String,
        version: String,
    },
}

#[derive(Args)]
struct InstallArgs {
    id: Option<String>,
    #[arg(long, default_value = "latest")]
    version: String,
    #[arg(long)]
    manifest: Option<PathBuf>,
    #[arg(long)]
    asset: Option<PathBuf>,
    #[arg(long)]
    allow_unapproved: bool,
}

#[derive(Subcommand)]
enum LibraryCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Info {
        node: String,
    },
    Status {
        node: String,
        #[arg(long)]
        json: bool,
    },
    Install(InstallArgs),
    Remove {
        id: String,
        version: String,
    },
}

fn print_status(status: &lifecycle::Status, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(status)?);
    } else {
        println!("{}", if status.running { "running" } else { "stopped" });
        for node in &status.nodes {
            println!(
                "  {:<20} {}",
                node.id,
                if node.running { "running" } else { "stopped" }
            );
        }
        if status.running && !status.north_http_url.is_empty() {
            println!("EOS HTTP: {}", status.north_http_url);
        }
    }
    Ok(())
}

fn component(paths: &config::Paths, name: &str, role: &str, cmd: ComponentCommand) -> Result<()> {
    match cmd {
        ComponentCommand::Start => {
            for id in lifecycle::start(paths, name, Some(role))? {
                println!("started {id}");
            }
        }
        ComponentCommand::Stop => {
            for id in lifecycle::stop(paths, name, Some(role))? {
                println!("stopped {id}");
            }
        }
        ComponentCommand::Shutdown => {
            ensure!(role == "eos", "shutdown is available for EOS only");
            for id in lifecycle::stop(paths, name, Some(role))? {
                println!("stopped {id}");
            }
            let caller = std::env::var("ISOS_SESSION_TOKEN").ok();
            let closed = lifecycle::close_other_sessions(paths, name, caller.as_deref())?;
            if closed > 0 {
                println!("closed {closed} other EOS CLI session(s)");
            }
        }
        ComponentCommand::Restart => {
            lifecycle::stop(paths, name, Some(role))?;
            for id in lifecycle::start(paths, name, Some(role))? {
                println!("started {id}");
            }
        }
        ComponentCommand::Status { json } => {
            print_status(&lifecycle::status(paths, name, Some(role))?, json)?
        }
        ComponentCommand::Enter => {
            ensure!(role == "eos", "enter is available for EOS only");
            let status = lifecycle::status(paths, name, Some("eos"))?;
            ensure!(status.running, "EOS is offline; run isos eos start first");
            let endpoint = status
                .north_http_url
                .strip_prefix("http://")
                .ok_or_else(|| {
                    anyhow::anyhow!("EOS north HTTP URL is missing; set settings.north_http_url")
                })?;
            let reachable = endpoint
                .to_socket_addrs()?
                .any(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok());
            ensure!(
                reachable,
                "EOS north HTTP is unreachable at {}; check its sidecar and isos logs eos-north-http",
                status.north_http_url
            );
            let executable = std::env::var("ISOS_EOS_CLI").unwrap_or_else(|_| "eos-cli".into());
            let token = format!(
                "{}-{}-{}",
                name,
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            );
            let mut child = Command::new(executable)
                .env("ISOS_DEPLOYMENT", name)
                .env("ISOS_BIN", std::env::current_exe()?)
                .env("ISOS_SESSION_TOKEN", &token)
                .spawn()?;
            lifecycle::register_session(paths, name, child.id() as i32, &token)?;
            let exit = child.wait()?;
            lifecycle::unregister_session(paths, name, &token)?;
            ensure!(exit.success(), "EOS CLI exited with {exit}");
        }
    }
    Ok(())
}

fn config_command(paths: &config::Paths, name: &str, cmd: ConfigCommand) -> Result<()> {
    match cmd {
        ConfigCommand::Init { output } => {
            let default = v2_starter::init(paths, name)?;
            if let Some(output) = output {
                ensure!(!output.exists(), "{} already exists", output.display());
                config::atomic_write(&output, &fs::read(default)?)?;
                println!("created {}", output.display());
            } else {
                println!("editable graph: {}", default.display());
            }
        }
        ConfigCommand::Validate { file } => {
            v2_process::validate_full_graph(&file)?;
            let specs = v2_process::project_with_packages(paths, &fs::read_to_string(&file)?)?;
            println!("valid v2 graph: {} IsOS processes", specs.len());
        }
        ConfigCommand::Apply { file } => {
            let specs = apply_graph_file(paths, name, &file)?;
            println!("applied v2 graph: {} IsOS processes", specs.len());
        }
        ConfigCommand::Show => {
            println!("{}", fs::read_to_string(paths.v2_node_graph(name))?);
        }
    }
    Ok(())
}

fn apply_graph_file(
    paths: &config::Paths,
    name: &str,
    file: &Path,
) -> Result<Vec<v2_process::ProcessSpec>> {
    let source = fs::read_to_string(file)?;
    let mut graph = v2_process::normalize_source(&source)?;
    v2_process::migrate_component_commands(&mut graph)?;
    v2_process::validate_value(paths, &graph)?;
    let normalized = serde_json::to_vec_pretty(&graph)?;
    let specs = v2_process::project_with_packages(paths, &String::from_utf8(normalized.clone())?)?;
    let revision = hex::encode(Sha256::digest(&normalized));
    let rendered = core_setup::render_graph(paths, name, &graph, &revision)?;
    let _lock = lifecycle::Lock::take(paths)?;
    let destination = paths.v2_node_graph(name);
    let previous = fs::read(&destination).ok();
    config::atomic_write(&destination, &normalized)?;
    if let Err(error) = core_setup::write_rendered(paths, name, rendered) {
        if let Some(previous) = previous {
            config::atomic_write(&destination, &previous)?;
        } else {
            let _ = fs::remove_file(&destination);
        }
        return Err(error);
    }
    Ok(specs)
}

fn install_package(paths: &config::Paths, args: InstallArgs, kind: Option<&str>) -> Result<()> {
    let release = match (&args.id, &args.manifest) {
        (Some(id), None) => packages::catalog_release(id, &args.version)?,
        (_, Some(file)) => {
            ensure!(
                args.allow_unapproved,
                "--manifest requires --allow-unapproved"
            );
            let release = packages::read_manifest(file)?;
            if let Some(id) = &args.id {
                ensure!(release.id == *id, "manifest ID differs from requested ID");
            }
            release
        }
        (None, None) => bail!("provide a package ID or --manifest <file>"),
    };
    if let Some(kind) = kind {
        ensure!(
            release.kind == kind,
            "{} is a {} package, expected {kind}",
            release.id,
            release.kind
        );
    }
    let path = packages::install(
        paths,
        &release,
        args.allow_unapproved,
        args.asset.as_deref(),
    )?;
    println!(
        "installed {} {} at {}",
        release.id,
        release.version,
        path.display()
    );
    println!("Add the package to a deployment graph and run isos config apply");
    Ok(())
}

fn package_command(paths: &config::Paths, cmd: PackageCommand) -> Result<()> {
    match cmd {
        PackageCommand::Upload { manifest, asset } => packages::upload(paths, &manifest, &asset)?,
        PackageCommand::Search { query } => println!(
            "{}",
            serde_json::to_string_pretty(&packages::search(&query)?)?
        ),
        PackageCommand::Info { id, version } => println!(
            "{}",
            serde_json::to_string_pretty(&packages::catalog_release(&id, &version)?)?
        ),
        PackageCommand::Install(args) => install_package(paths, args, None)?,
        PackageCommand::Link {
            id,
            version,
            path,
            manifest,
        } => {
            let installed =
                packages::link_with_manifest(paths, &id, &version, &path, manifest.as_deref())?;
            println!("linked {} {} at {}", id, version, installed.display());
        }
        PackageCommand::List { json } => {
            let receipts = packages::list(paths)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&receipts)?);
            } else if receipts.is_empty() {
                println!("no installed packages");
            } else {
                for receipt in receipts {
                    println!(
                        "{:<36} {:<14} {}",
                        receipt.id,
                        receipt.version,
                        if receipt.local_link.is_some() {
                            "linked"
                        } else {
                            "installed"
                        }
                    );
                }
            }
        }
        PackageCommand::NodeSets => println!(
            "{}",
            serde_json::to_string_pretty(&node_sets::list(paths)?)?
        ),
        PackageCommand::PreviewSet {
            id,
            version,
            set_id,
            graph,
            values,
            replace_legacy_mujoco,
        } => {
            let graph = v2_process::normalize_source(&fs::read_to_string(graph)?)?;
            let values = values
                .map(
                    |file| -> Result<std::collections::BTreeMap<String, String>> {
                        Ok(serde_json::from_slice(&fs::read(file)?)?)
                    },
                )
                .transpose()?
                .unwrap_or_default();
            let preview = node_sets::compose_with_migration(
                paths,
                &id,
                &version,
                &set_id,
                &graph,
                &values,
                replace_legacy_mujoco,
            )?;
            v2_process::validate_value(paths, &preview["graph"])?;
            println!("{}", serde_json::to_string_pretty(&preview)?);
        }
        PackageCommand::Verify { manifest, asset } => {
            let release = packages::read_manifest(&manifest)?;
            let selected = release.select_asset(&release::Platform::current())?;
            release::verify_file(selected, &asset)?;
            println!(
                "verified {} {} for this platform",
                release.id, release.version
            );
        }
        PackageCommand::Publish { manifest } => packages::publish(paths, &manifest)?,
        PackageCommand::Remove { id, version } => {
            packages::remove(paths, &id, &version)?;
            println!("removed {id} {version}");
        }
    }
    Ok(())
}

fn library(paths: &config::Paths, name: &str, kind: &str, command: LibraryCommand) -> Result<()> {
    let expected_kind = match kind {
        "skills" => "skill",
        "services" => "service",
        "backends" => "backend",
        _ => unreachable!(),
    };
    match command {
        LibraryCommand::Install(args) => return install_package(paths, args, Some(expected_kind)),
        LibraryCommand::Remove { id, version } => {
            let path = packages::package_path(paths, &id, &version)?;
            let release = packages::read_manifest(&path.join("release.json"))?;
            ensure!(
                release.kind == expected_kind,
                "{} is a {} package, expected {expected_kind}",
                id,
                release.kind
            );
            packages::remove(paths, &id, &version)?;
            println!("removed {id} {version}");
            return Ok(());
        }
        _ => {}
    }
    let graph = config::load_selected(paths, name)?;
    let nodes: Vec<_> = graph
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                (kind, &n.kind),
                ("skills", NodeKind::Skill)
                    | ("services", NodeKind::Service)
                    | ("backends", NodeKind::Backend)
            )
        })
        .collect();
    match command {
        LibraryCommand::List { json } => {
            if json {
                println!("{}", serde_json::to_string_pretty(&nodes)?);
            } else if nodes.is_empty() {
                println!("no {kind} in deployment {name}");
            } else {
                for node in nodes {
                    println!(
                        "{:<20} {}",
                        node.id,
                        if node.enabled { "enabled" } else { "disabled" }
                    );
                }
            }
        }
        LibraryCommand::Info { node } => {
            let selected = nodes.into_iter().find(|n| n.id == node).ok_or_else(|| {
                anyhow::anyhow!("{node} is not a {expected_kind} node in deployment {name}")
            })?;
            println!("{}", serde_json::to_string_pretty(selected)?);
        }
        LibraryCommand::Status { node, json } => {
            ensure!(
                nodes.iter().any(|n| n.id == node),
                "{node} is not a {expected_kind} node in deployment {name}"
            );
            let status = lifecycle::status(paths, name, None)?;
            let running = status.nodes.iter().any(|n| n.id == node && n.running);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &json!({"schema_version":1,"id":node,"running":running})
                    )?
                );
            } else {
                println!("{node}: {}", if running { "running" } else { "stopped" });
            }
        }
        LibraryCommand::Install(_) | LibraryCommand::Remove { .. } => unreachable!(),
    }
    Ok(())
}

fn services_command(paths: &config::Paths, name: &str, command: ServiceCommand) -> Result<()> {
    let result = match command {
        ServiceCommand::List => {
            json!({"services":services::load_specs(paths)?.into_keys().collect::<Vec<_>>()})
        }
        ServiceCommand::Validate => json!({"services":services::validate_all(paths)?}),
        ServiceCommand::Start { id } => {
            service_call(paths, name, "services_start", Some(id), None)?
        }
        ServiceCommand::Stop { id } => {
            if UnixStream::connect(paths.socket()).is_err() {
                json!({"stopped":[]})
            } else {
                service_call(paths, name, "services_stop", Some(id), None)?
            }
        }
        ServiceCommand::Restart { id } => {
            service_call(paths, name, "services_restart", Some(id), None)?
        }
        ServiceCommand::Status { id } => {
            let status = if UnixStream::connect(paths.socket()).is_ok() {
                service_call(paths, name, "services_status", None, None)?
            } else {
                json!({"services":[]})
            };
            if let Some(id) = id {
                let entries = status["services"]
                    .as_array()
                    .context("invalid services status")?;
                json!({"services":entries.iter().filter(|entry| entry["id"] == id).collect::<Vec<_>>()})
            } else {
                status
            }
        }
        ServiceCommand::Logs { id, lines } => {
            service_call(paths, name, "services_logs", Some(id), Some(lines))?
        }
        ServiceCommand::Adapter {
            provider,
            repo,
            alias,
        } => {
            let command = services::island_models_adapter(&repo, &alias)?;
            json!({"provider":provider,"alias":alias,"managed_command":command})
        }
        ServiceCommand::Install(args) => {
            return library(paths, name, "services", LibraryCommand::Install(args));
        }
        ServiceCommand::Remove { id, version } => {
            return library(
                paths,
                name,
                "services",
                LibraryCommand::Remove { id, version },
            );
        }
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[derive(Deserialize, Serialize)]
struct ApiRequest {
    schema_version: u32,
    op: String,
    #[serde(default)]
    deployment: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    graph: Option<Deployment>,
    #[serde(default)]
    revision: Option<String>,
    #[serde(default)]
    service_id: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    log: Option<LogEventInput>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LogEventInput {
    source: String,
    level: String,
    message: String,
    #[serde(default)]
    event_code: Option<String>,
}
#[derive(Deserialize, Serialize)]
struct ApiResponse {
    schema_version: u32,
    ok: bool,
    data: Value,
    error: Option<String>,
}

struct V2Deployment {
    active_revision: String,
    supervisor: v2_process::Supervisor,
    last_error: Option<String>,
}
type V2Supervisors = BTreeMap<String, V2Deployment>;

fn restart_v2_processes(
    paths: &config::Paths,
    managers: &mut V2Supervisors,
    name: &str,
    revision: &str,
    specs: &[v2_process::ProcessSpec],
) -> Result<Vec<String>> {
    let mut supervisor = v2_process::Supervisor::new(paths, name)?;
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
        V2Deployment {
            active_revision: revision.to_owned(),
            supervisor,
            last_error: None,
        },
    );
    Ok(started)
}

fn manager_identity(paths: &config::Paths) -> Result<Value> {
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
        "api_schema_version": 2,
        "service_contract_version": "2.0.0",
        "api_origin": http::local_origin(),
        "ui_url": http::ui_url()?,
    }))
}

fn api_call(
    paths: &config::Paths,
    managers: &mut V2Supervisors,
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
    if request.op.starts_with("v2_") {
        v2_process::validate_deployment_name(&name)?;
    }
    match request.op.as_str() {
        "manager_identity" => manager_identity(paths),
        "log_append" => {
            v2_process::validate_deployment_name(&name)?;
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
        "v2_active_packages" => {
            let packages: Vec<_> = managers
                .values()
                .flat_map(|active| active.supervisor.active_packages())
                .collect();
            Ok(json!({"packages":packages}))
        }
        "v2_restart" => {
            let path = paths.v2_node_graph(&name);
            v2_process::validate_full_graph(&path)?;
            let source = fs::read_to_string(&path)?;
            let revision = v2_starter::revision(&path)?;
            if let Some(expected) = request.revision.as_deref() {
                ensure!(
                    expected == revision,
                    "saved graph revision changed; reload before restart"
                );
            }
            let specs = v2_process::project_with_packages(paths, &source)?;
            core_setup::verify_launch_inputs(paths, &name)?;
            v2_build::preflight_processes(paths, &specs)?;
            core_setup::prepare_run(paths, &name, &revision)?;
            ensure!(
                v2_starter::revision(&path)? == revision,
                "saved graph revision changed during core setup; reload before restart"
            );
            let started = restart_v2_processes(paths, managers, &name, &revision, &specs)?;
            Ok(json!({"started":started,"active_revision":revision}))
        }
        "v2_start" => {
            let path = paths.v2_node_graph(&name);
            v2_process::validate_full_graph(&path)?;
            let source = fs::read_to_string(&path)?;
            let revision = v2_starter::revision(&path)?;
            if let Some(expected) = request.revision.as_deref() {
                ensure!(
                    expected == revision,
                    "saved graph revision changed; reload before run"
                );
            }
            let specs = v2_process::project_with_packages(paths, &source)?;
            core_setup::verify_launch_inputs(paths, &name)?;
            v2_build::preflight_processes(paths, &specs)?;
            core_setup::prepare_run(paths, &name, &revision)?;
            ensure!(
                v2_starter::revision(&path)? == revision,
                "saved graph revision changed during core setup; reload before run"
            );
            let manager = if let Some(active) = managers.get_mut(&name) {
                ensure!(
                    active.active_revision == revision,
                    "v2 graph changed while running; stop before restart"
                );
                active.supervisor.verify_specs(&specs)?;
                &mut active.supervisor
            } else {
                managers.insert(
                    name.clone(),
                    V2Deployment {
                        active_revision: revision.clone(),
                        supervisor: v2_process::Supervisor::new(paths, &name)?,
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
        "v2_stop" => {
            let stopped = managers
                .remove(&name)
                .map_or_else(Vec::new, |mut active| active.supervisor.stop());
            Ok(json!({"stopped": stopped}))
        }
        "v2_status" => {
            let saved_revision = v2_starter::revision(&paths.v2_node_graph(&name)).ok();
            let pinned_package = fs::read(paths.v2_node_graph(&name)).ok()
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
                json!({"schema_version":"2.0.0","saved_revision":saved_revision,"active_revision":active_revision,"state":state,"last_error":last_error,"package":pinned_package,"processes":processes}),
            )
        }
        "status" => Ok(serde_json::to_value(lifecycle::status(
            paths,
            &name,
            request.role.as_deref(),
        )?)?),
        "graph" => Ok(serde_json::to_value(config::load_selected(paths, &name)?)?),
        "start" => Ok(json!({"started":lifecycle::start(paths,&name,request.role.as_deref())?})),
        "stop" => Ok(json!({"stopped":lifecycle::stop(paths,&name,request.role.as_deref())?})),
        "apply" => {
            let graph = request
                .graph
                .ok_or_else(|| anyhow::anyhow!("apply requires graph"))?;
            ensure!(
                graph.name == name,
                "graph name differs from selected deployment"
            );
            let _lock = lifecycle::Lock::take(paths)?;
            config::apply(paths, &graph)?;
            Ok(json!({"applied":name}))
        }
        _ => bail!("unknown API operation {}", request.op),
    }
}

fn handle_client(
    paths: &config::Paths,
    managers: &mut V2Supervisors,
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
            request.op.starts_with("v2_")
                && !matches!(request.op.as_str(), "v2_status" | "v2_active_packages")
        })
        .map(|request| config::deployment_name(request.deployment.clone()));
    let clear_error = request
        .as_ref()
        .ok()
        .is_some_and(|request| matches!(request.op.as_str(), "v2_start" | "v2_restart"));
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
        failed_deployment.filter(|name| v2_process::validate_deployment_name(name).is_ok())
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

fn bind_daemon(paths: &config::Paths) -> Result<UnixListener> {
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

fn daemon(paths: &config::Paths, deployment: &str, listener: UnixListener) -> Result<()> {
    let mut managers = V2Supervisors::new();
    let mut service_manager = services::ServiceManager::new(paths, deployment)?;
    println!("IsOS API listening on {}", paths.socket().display());
    while !INTERRUPTED.load(Ordering::SeqCst) {
        for active in managers.values_mut() {
            if let Err(error) = active.supervisor.poll() {
                active.last_error = Some(error.to_string());
                tracing::error!(
                    event_code = "deployment_supervisor_failed",
                    "v2 process supervisor: {error:#}"
                );
                eprintln!("v2 process supervisor: {error:#}");
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

fn ui_source() -> Result<PathBuf> {
    let source = isosui::source_root()?;
    ensure!(
        source.join("node_modules").is_dir(),
        "IsosUI dependencies missing at {}; run npm ci there",
        source.display()
    );
    Ok(source)
}

struct Frontend(Child);

impl Frontend {
    fn spawn(source: &Path, logs: log_store::LogStore) -> Result<Self> {
        let port = http::ui_port()?;
        ensure!(
            TcpStream::connect(("127.0.0.1", port)).is_err(),
            "IsosUI port 127.0.0.1:{port} is already in use"
        );
        let mut command = Command::new("npm");
        command
            .arg("start")
            .current_dir(source)
            .env("ISOSUI_HOST", "127.0.0.1")
            .env("ISOSUI_PORT", port.to_string())
            .env("ISOSUI_API_ORIGIN", http::local_origin())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let parent_pid = std::process::id();
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() as u32 != parent_pid {
                    return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
                }
                Ok(())
            });
        }
        let mut child = command.spawn()?;
        if let Some(stdout) = child.stdout.take() {
            log_store::capture(stdout, logs.clone(), "ui".into(), "info");
        }
        if let Some(stderr) = child.stderr.take() {
            log_store::capture(stderr, logs, "ui".into(), "warn");
        }
        Ok(Self(child))
    }

    fn ready(&mut self, port: u16) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if let Some(status) = self.0.try_wait()? {
                bail!("IsosUI npm server exited before ready: {status}");
            }
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Ok(());
            }
            if INTERRUPTED.load(Ordering::SeqCst) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        bail!("IsosUI npm server did not listen on 127.0.0.1:{port} within 30 seconds")
    }
}

impl Drop for Frontend {
    fn drop(&mut self) {
        let group = -(self.0.id() as i32);
        unsafe { libc::kill(group, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.0.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if unsafe { libc::kill(group, 0) } == 0 {
            unsafe { libc::kill(group, libc::SIGKILL) };
        }
        let _ = self.0.wait();
    }
}

fn open_browser() -> Result<()> {
    let url = http::ui_url()?;
    Command::new("xdg-open")
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("open IsosUI in browser")?;
    Ok(())
}

fn ensure_setup(paths: &config::Paths, name: &str) -> Result<()> {
    paths.prepare()?;
    if !paths.config.join("mode").exists() && std::env::var_os("ISOS_MODE").is_none() {
        paths.set_mode("dev")?;
    }
    v2_starter::init(paths, name)?;
    Ok(())
}

fn serve_foreground(
    paths: config::Paths,
    name: String,
    ui_live: bool,
    browser: bool,
    run_graph: bool,
) -> Result<()> {
    ensure_setup(&paths, &name)?;
    v2_build::verify_active_isos(&paths)?;
    ensure!(
        !ui_live || paths.mode()? == "dev",
        "--ui-live is available only in dev mode"
    );
    if UnixStream::connect(paths.socket()).is_ok() {
        let identity = v2_call(&paths, &name, "manager_identity")?;
        bail!("IsOS manager is already running: {identity}");
    }
    let port = http::ui_port()?;
    for (label, port) in [("IsOS API", http::local_port()?), ("IsosUI", port)] {
        ensure!(
            TcpStream::connect(("127.0.0.1", port)).is_err(),
            "{label} port 127.0.0.1:{port} belongs to another listener; stop that listener or configure a different port"
        );
    }
    catch_interrupts()?;
    let logs = log_store::LogStore::new(&paths, &name)?;
    manager_log::install(logs.clone())?;
    logs.append("isos", "info", "Starting IsOS workbench")?;
    let main_log = logs.root().join("isos.log");
    let mut frontend = if ui_live {
        Some(Frontend::spawn(&ui_source()?, logs.clone())?)
    } else {
        None
    };
    let static_ui = if ui_live {
        None
    } else {
        let directory = isosui::bundle_for_serve(&paths)?;
        Some(std::thread::spawn(move || http::serve_ui(directory)))
    };
    let listener = bind_daemon(&paths)?;
    let api_paths = paths.clone();
    let manager_name = name.clone();
    let manager = std::thread::spawn(move || daemon(&api_paths, &manager_name, listener));
    let http_paths = paths.clone();
    let http_name = name.clone();
    let http_server = std::thread::spawn(move || http::serve(http_paths, http_name));
    let mut result = if let Some(frontend) = frontend.as_mut() {
        frontend.ready(port)
    } else {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break Ok(());
            }
            if static_ui.as_ref().is_some_and(|ui| ui.is_finished()) {
                break Err(anyhow::anyhow!("IsosUI static server exited before ready"));
            }
            if Instant::now() >= deadline {
                break Err(anyhow::anyhow!("IsosUI static server did not become ready"));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    if result.is_ok() && !INTERRUPTED.load(Ordering::SeqCst) {
        println!("{}", http::ui_url()?);
        println!("IsOS API: {}", http::local_origin());
        println!("IsOS log: {}", main_log.display());
        println!("IsOS workbench is running; press Ctrl+C to stop it.");
        if browser && let Err(error) = open_browser() {
            eprintln!(
                "Could not open browser: {error}. Open {} manually.",
                http::ui_url()?
            );
        }
        if run_graph {
            match v2_call(&paths, &name, "v2_start") {
                Ok(started) => {
                    if let Err(error) = print_v2_run_startup(&paths, &name, &started) {
                        eprintln!("Could not display graph status: {error}");
                    }
                }
                Err(error) => {
                    eprintln!("Graph did not start: {error}. Use the workbench to finish setup.")
                }
            }
        }
        while !INTERRUPTED.load(Ordering::SeqCst) {
            if let Some(frontend) = frontend.as_mut()
                && let Some(status) = frontend.0.try_wait()?
            {
                result = Err(anyhow::anyhow!("IsosUI npm server exited: {status}"));
                break;
            }
            if static_ui.as_ref().is_some_and(|ui| ui.is_finished()) {
                result = Err(anyhow::anyhow!("IsosUI static server exited"));
                break;
            }
            if http_server.is_finished() || manager.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    INTERRUPTED.store(true, Ordering::SeqCst);
    drop(frontend);
    if let Some(ui) = static_ui {
        ui.join()
            .map_err(|_| anyhow::anyhow!("IsosUI server thread panicked"))??;
    }
    let http_result = http_server
        .join()
        .map_err(|_| anyhow::anyhow!("IsOS HTTP thread panicked"))?;
    let manager_result = manager
        .join()
        .map_err(|_| anyhow::anyhow!("IsOS manager thread panicked"))?;
    manager_result?;
    http_result?;
    if let Err(error) = &result {
        tracing::error!(
            event_code = "workbench_failed",
            "IsOS workbench failed: {error:#}"
        );
    }
    result
}

fn print_v2_run_startup(paths: &config::Paths, name: &str, result: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(result)?);
    println!("IsosUI: {}", http::ui_url()?);
    println!("IsOS API: {}", http::local_origin());
    println!(
        "Active revision: {}",
        result["active_revision"]
            .as_str()
            .context("start result lacks active revision")?
    );
    println!(
        "IsOS log: {}",
        paths
            .data
            .join("logs")
            .join(name)
            .join("isos.log")
            .display()
    );
    Ok(())
}

fn wait_for_interrupt(paths: &config::Paths, name: &str) -> Result<()> {
    println!("Deployment is running; press Ctrl+C to stop it.");
    loop {
        if INTERRUPTED.load(Ordering::SeqCst) {
            println!(
                "{}",
                serde_json::to_string_pretty(&v2_call(paths, name, "v2_stop")?)?
            );
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
        let status = v2_call(paths, name, "v2_status")?;
        if status["state"] == "stopped" {
            return Ok(());
        }
    }
}

fn v2_call(paths: &config::Paths, name: &str, op: &str) -> Result<Value> {
    v2_call_revision(paths, name, op, None)
}

fn stop_legacy_server() -> Result<bool> {
    let active = Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", "isos-server.service"])
        .status();
    if !active.is_ok_and(|status| status.success()) {
        return Ok(false);
    }
    let stopped = Command::new("systemctl")
        .args(["--user", "stop", "isos-server.service"])
        .status()?;
    ensure!(stopped.success(), "failed to stop legacy IsOS server");
    Ok(true)
}

fn v2_call_revision(
    paths: &config::Paths,
    name: &str,
    op: &str,
    revision: Option<String>,
) -> Result<Value> {
    if matches!(
        op,
        "v2_start" | "v2_restart" | "services_start" | "services_restart"
    ) {
        check_manager_binary(paths, name)?;
    }
    let request = ApiRequest {
        schema_version: 1,
        op: op.to_owned(),
        deployment: Some(name.to_owned()),
        role: None,
        graph: None,
        revision,
        service_id: None,
        cursor: None,
        limit: None,
        log: None,
    };
    manager_request(paths, request)
}

fn manager_request(paths: &config::Paths, request: ApiRequest) -> Result<Value> {
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

fn check_manager_binary(paths: &config::Paths, name: &str) -> Result<()> {
    let active = v2_call(paths, name, "manager_identity")?;
    let own = hex::encode(Sha256::digest(fs::read(std::env::current_exe()?)?));
    ensure!(
        active["executable_sha256"] == own,
        "running IsOS manager uses a different binary; run `isos stop`, then `isos serve` with this version"
    );
    Ok(())
}

fn service_call(
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
            graph: None,
            revision: None,
            service_id: id,
            cursor: None,
            limit,
            log: None,
        },
    )
}

fn print_init_next_steps(mode: &str) {
    println!("\nNext steps:");
    println!("  Build EOS, EGIA, and IsosUI in their owning repositories ({mode} mode).");
    println!("  Configure ISOS_AETHER_BIN_DIR, ISOS_EGIA_BIN_DIR, and ISOSUI_DIST_DIR.");
    println!("  isos version check                # inspect build and launch readiness");
    println!("  isos                               # open the workbench; cores stay stopped");
    println!("  In Graph, add a backend, then Check graph and Save graph before `isos run`.");
    println!("  Run `isos --help` for mode rules or `isos doctor` to check readiness.");
}

fn main_run() -> Result<()> {
    config::load_runtime_config()?;
    let cli = Cli::parse();
    if let Some(shell) = cli.generate_completion {
        generate(shell, &mut Cli::command(), "isos", &mut std::io::stdout());
        return Ok(());
    }
    let paths = config::Paths::discover()?;
    let name = cli.deployment;
    let Some(command) = cli.command else {
        ensure_setup(&paths, &name)?;
        if UnixStream::connect(paths.socket()).is_ok() {
            let identity = v2_call(&paths, &name, "manager_identity")?;
            println!("IsOS workbench already running: {}", identity["ui_url"]);
            if let Err(error) = open_browser() {
                eprintln!(
                    "Could not open browser: {error}. Open {} manually.",
                    http::ui_url()?
                );
            }
            return Ok(());
        }
        return serve_foreground(paths, name, false, true, false);
    };
    match command {
        CommandLine::Build { target, .. } => {
            anyhow::bail!(
                "Build {target} in its owning repository, then configure ISOS_AETHER_BIN_DIR, ISOS_EGIA_BIN_DIR, or ISOSUI_DIST_DIR. Run `isos version check` for readiness."
            );
        }
        CommandLine::Version { command } => match command {
            VersionCommand::Check { json } => {
                let active = if UnixStream::connect(paths.socket()).is_ok() {
                    v2_call(&paths, &name, "v2_status").ok()
                } else {
                    None
                };
                let report = readiness::report(&paths, &name, active.as_ref());
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    println!(
                        "IsOS {} | graph {}",
                        env!("CARGO_PKG_VERSION"),
                        report["graph"]["path"].as_str().unwrap_or("unavailable")
                    );
                    for core in report["cores"].as_array().into_iter().flatten() {
                        let name = core["core"].as_str().unwrap_or("core");
                        let version = core["version"].as_str().unwrap_or("unknown");
                        let state = if core["ready"] == true {
                            "ready"
                        } else {
                            "blocked"
                        };
                        println!(
                            "{name}: {state} | version {version} | {}",
                            core["path"].as_str().unwrap_or("not configured")
                        );
                        println!(
                            "  Graph configuration: {} | Running: {}",
                            if core["graph_configured"] == true {
                                "saved"
                            } else {
                                "missing"
                            },
                            if core["running"] == true { "yes" } else { "no" }
                        );
                        if let Some(reason) = core["reason"].as_str() {
                            println!("  Reason: {reason}");
                        }
                        if let Some(action) = core["action"].as_str() {
                            println!("  Next: {action}");
                        }
                    }
                    println!(
                        "IsosUI: {} | {}",
                        if report["isosui"]["ready"] == true {
                            "ready"
                        } else {
                            "blocked"
                        },
                        report["isosui"]["path"]
                            .as_str()
                            .unwrap_or("not configured")
                    );
                }
            }
            VersionCommand::Pin { .. } => {
                anyhow::bail!(
                    "Version selection is retired. Configure one binary directory per core and ISOSUI_DIST_DIR, then run `isos version check`."
                );
            }
        },
        CommandLine::Init { mode } => {
            paths.prepare()?;
            if let Some(mode) = mode {
                paths.set_mode(&mode)?;
            } else if !paths.config.join("mode").exists() {
                paths.set_mode("dev")?;
            }
            let own_pin = paths.config.join("isos-version");
            if !own_pin.exists() {
                config::atomic_write(&own_pin, env!("CARGO_PKG_VERSION").as_bytes())?;
            }
            let graph_path = v2_starter::init(&paths, &name)?;
            println!("editable graph: {}", graph_path.display());
            if v2_backend::installed(&paths).is_empty() {
                println!(
                    "MuJoCo package missing: install or link island/mujoco-sim 2.0.0, then validate and apply the graph"
                );
            }
            print_init_next_steps(paths.mode()?);
        }
        CommandLine::Open => {
            if UnixStream::connect(paths.socket()).is_err() {
                return serve_foreground(paths, name, false, true, false);
            }
            v2_call(&paths, &name, "manager_identity")?;
            let url = http::ui_url()?;
            Command::new("xdg-open")
                .arg(&url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("launch IsosUI with xdg-open")?;
            println!("{url}");
        }
        CommandLine::Run => {
            ensure_setup(&paths, &name)?;
            if UnixStream::connect(paths.socket()).is_err() {
                return serve_foreground(paths, name, false, true, true);
            }
            catch_interrupts()?;
            print_v2_run_startup(&paths, &name, &v2_call(&paths, &name, "v2_start")?)?;
            wait_for_interrupt(&paths, &name)?;
        }
        CommandLine::Serve { ui_live } => {
            serve_foreground(paths, name, ui_live, false, false)?;
        }
        CommandLine::Auth { command } => match command {
            AuthCommand::Login => auth::login(&paths)?,
            AuthCommand::Logout => auth::logout(&paths)?,
            AuthCommand::Whoami => auth::status(&paths)?,
        },
        CommandLine::Start => {
            catch_interrupts()?;
            print_v2_run_startup(&paths, &name, &v2_call(&paths, &name, "v2_start")?)?;
            wait_for_interrupt(&paths, &name)?;
        }
        CommandLine::Stop => {
            let legacy_stopped = stop_legacy_server()?;
            match v2_call(&paths, &name, "manager_shutdown") {
                Ok(result) => println!("{}", serde_json::to_string_pretty(&result)?),
                Err(_) if legacy_stopped => {
                    println!("{}", json!({"stopped":["isos-server.service"]}))
                }
                Err(_) if UnixStream::connect(paths.socket()).is_err() => {
                    println!("{}", json!({"stopped":[],"manager":"stopped"}))
                }
                Err(error) => return Err(error),
            }
        }
        CommandLine::Deployment { command } => match command {
            DeploymentCommand::Stop => println!(
                "{}",
                serde_json::to_string_pretty(&v2_call(&paths, &name, "v2_stop")?)?
            ),
            DeploymentCommand::Status => println!(
                "{}",
                serde_json::to_string_pretty(&v2_call(&paths, &name, "v2_status")?)?
            ),
        },
        CommandLine::Restart => {
            catch_interrupts()?;
            print_v2_run_startup(&paths, &name, &v2_call(&paths, &name, "v2_restart")?)?;
            wait_for_interrupt(&paths, &name)?;
        }
        CommandLine::Status { json: _ } => {
            let mut status = if UnixStream::connect(paths.socket()).is_ok() {
                v2_call(&paths, &name, "v2_status")?
            } else {
                json!({"schema_version":"2.0.0","state":"stopped","active_revision":null,"processes":[]})
            };
            status["manager"] = if UnixStream::connect(paths.socket()).is_ok() {
                v2_call(&paths, &name, "manager_identity")?
            } else {
                json!({"state":"stopped"})
            };
            status["services"] = if UnixStream::connect(paths.socket()).is_ok() {
                service_call(&paths, &name, "services_status", None, None)?["services"].clone()
            } else {
                json!([])
            };
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        CommandLine::Logs {
            source,
            lines,
            level,
        } => {
            let store = log_store::LogStore::new(&paths, &name)?;
            for entry in store.tail_filtered(&source, lines as usize, Some(&level))? {
                println!(
                    "{} {} [{}] {}",
                    entry["timestamp"].as_str().unwrap_or(""),
                    entry["level"].as_str().unwrap_or("").to_ascii_uppercase(),
                    entry["source"].as_str().unwrap_or(""),
                    entry["message"].as_str().unwrap_or("")
                );
            }
        }
        CommandLine::Doctor => {
            println!("IsOS {}", env!("CARGO_PKG_VERSION"));
            println!("graph: {}", paths.v2_node_graph(&name).display());
            println!("data: {}", paths.data.display());
            println!("runtime: {}", paths.runtime.display());
            let graph = paths.v2_node_graph(&name);
            match v2_process::validate_full_graph(&graph).and_then(|_| {
                v2_process::project_with_packages(&paths, &fs::read_to_string(&graph)?)
            }) {
                Ok(specs) => println!("graph and package: ready ({} processes)", specs.len()),
                Err(error) => println!("graph and package: {error}"),
            }
            println!(
                "manager: {}",
                if UnixStream::connect(paths.socket()).is_ok() {
                    "running"
                } else {
                    "stopped"
                }
            );
        }
        CommandLine::Eos { command } => component(&paths, &name, "eos", command)?,
        CommandLine::Egia { command } => component(&paths, &name, "egia", command)?,
        CommandLine::Config { command } => config_command(&paths, &name, command)?,
        CommandLine::Packages { command } => package_command(&paths, command)?,
        CommandLine::Skills { command } => library(&paths, &name, "skills", command)?,
        CommandLine::Services { command } => services_command(&paths, &name, command)?,
        CommandLine::Backends { command } => library(&paths, &name, "backends", command)?,
        CommandLine::Daemon => {
            catch_interrupts()?;
            manager_log::install(log_store::LogStore::new(&paths, &name)?)?;
            let listener = bind_daemon(&paths)?;
            daemon(&paths, &name, listener)?;
        }
        CommandLine::V2 { command } => {
            v2_process::validate_deployment_name(&name)?;
            match command {
                V2Command::Validate { file } => {
                    v2_process::validate_full_graph(&file)?;
                    let specs =
                        v2_process::project_with_packages(&paths, &fs::read_to_string(&file)?)?;
                    println!("valid v2 node graph: {} IsOS process nodes", specs.len());
                }
                V2Command::Apply { file } => {
                    let specs = apply_graph_file(&paths, &name, &file)?;
                    println!("applied v2 node graph: {} IsOS process nodes", specs.len());
                }
                V2Command::Run => println!(
                    "{}",
                    serde_json::to_string_pretty(&v2_call(&paths, &name, "v2_start")?)?
                ),
                V2Command::Stop => println!(
                    "{}",
                    serde_json::to_string_pretty(&v2_call(&paths, &name, "v2_stop")?)?
                ),
                V2Command::Status => println!(
                    "{}",
                    serde_json::to_string_pretty(&v2_call(&paths, &name, "v2_status")?)?
                ),
            }
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = main_run() {
        eprintln!("isos: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod restart_tests {
    use super::*;

    #[test]
    fn external_log_operation_records_selected_event_and_rejects_reserved_source() {
        let temp = tempfile::tempdir().unwrap();
        let paths = config::Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let mut managers = V2Supervisors::new();
        let mut services = services::ServiceManager::new(&paths, "default").unwrap();
        let make_request = |source: &str| ApiRequest {
            schema_version: 1,
            op: "log_append".into(),
            deployment: Some("default".into()),
            role: None,
            graph: None,
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
        let spec = |program: &str| v2_process::ProcessSpec {
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
        let mut managers = V2Supervisors::new();
        restart_v2_processes(&paths, &mut managers, "test", "old", &[spec("/bin/sleep")]).unwrap();
        let error =
            restart_v2_processes(&paths, &mut managers, "test", "new", &[spec("/bin/false")])
                .unwrap_err();
        assert!(error.to_string().contains("previous revision restored"));
        let active = managers.get_mut("test").unwrap();
        assert_eq!(active.active_revision, "old");
        assert_eq!(active.supervisor.status().unwrap().len(), 1);
        active.supervisor.stop();
    }
}
