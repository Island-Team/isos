mod auth;
mod config;
mod graph;
mod lifecycle;
mod packages;
mod release;

use anyhow::{Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use graph::{Deployment, NodeKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpStream, ToSocketAddrs},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Parser)]
#[command(
    name = "isos",
    version,
    about = "Manage Island EOS and EGIA as one local deployment",
    after_help = "FIRST USE\n  1. Run `isos doctor` to check Linux and the user systemd session. Before installation, use `./bin/isos`.\n  2. Run `isos config init`, then edit the deployment JSON under ~/.config/isos/deployments/.\n  3. Run `isos config validate <file>` and `isos config apply <file>`.\n  4. Start with `isos start`; inspect with `isos status` and `isos logs <node>`.\n\nACCOUNT\n  Sign in:  isos auth login\n  Check:    isos auth whoami\n  Sign out: isos auth logout\n  Island-port credentials are saved to auth.json under the IsOS config directory\n  with mode 0600. Logout revokes the refresh session and deletes the local file.\n  Sign-in is needed for protected catalog operations such as publishing.\n\nMORE\n  Run `isos <command> --help` for command details. Read the IsOS quick start\n  and authentication guide in the project documentation."
)]
struct Cli {
    #[arg(
        long,
        global = true,
        env = "ISOS_DEPLOYMENT",
        default_value = "default"
    )]
    deployment: String,
    #[command(subcommand)]
    command: CommandLine,
}

#[derive(Subcommand)]
enum CommandLine {
    /// Manage the local Island-port account session.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// Start every enabled deployment node in dependency order.
    Start,
    /// Stop the active deployment and its IsOS-owned processes.
    Stop,
    /// Stop and start the active deployment using its saved graph.
    Restart,
    /// Show deployment and per-node process status; optionally emit JSON.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Read recent user-systemd logs for a deployment node.
    Logs {
        node: String,
        #[arg(long, default_value_t = 100)]
        lines: u32,
    },
    /// Check local configuration, graph validity, and systemd prerequisites.
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
        command: LibraryCommand,
    },
    /// Inspect backend nodes and manage hardware or simulator packages.
    Backends {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    /// Serve the owner-only manager API on a Unix socket.
    Daemon,
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
    },
    List {
        #[arg(long)]
        json: bool,
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

fn example(name: &str) -> Deployment {
    serde_json::from_value(json!({"schema_version":1,"name":name,
      "nodes":[
        {"id":"eos","kind":"component","role":"eos","enabled":false,"contract":"island.eos.v1",
         "settings":{"north_http_url":"http://127.0.0.1:16005","eos_config":"/path/to/eos.toml"}},
        {"id":"eos-north-http","kind":"service","enabled":false,
         "settings":{"sidecar_of":"eos"}},
        {"id":"egia","kind":"component","role":"egia","enabled":false,"contract":"island.eos.v1"}
      ],"edges":[{"from":"egia","to":"eos","kind":"depends_on"},
                 {"from":"eos-north-http","to":"eos","kind":"depends_on"}]}))
    .unwrap()
}

fn config_command(paths: &config::Paths, name: &str, cmd: ConfigCommand) -> Result<()> {
    match cmd {
        ConfigCommand::Init { output } => {
            let path = output.unwrap_or_else(|| paths.graph(name));
            ensure!(!path.exists(), "{} already exists", path.display());
            let mut bytes = serde_json::to_vec_pretty(&example(name))?;
            bytes.push(b'\n');
            config::atomic_write(&path, &bytes)?;
            println!("created {}", path.display());
            println!(
                "Edit launch commands and enable EOS/EGIA, then run isos config apply {}",
                path.display()
            );
        }
        ConfigCommand::Validate { file } => {
            let graph = config::load(&file)?;
            println!(
                "valid deployment {}: {} nodes, {} edges",
                graph.name,
                graph.nodes.len(),
                graph.edges.len()
            );
        }
        ConfigCommand::Apply { file } => {
            let graph = config::load(&file)?;
            ensure!(
                graph.name == name,
                "graph name {} differs from --deployment {name}",
                graph.name
            );
            let changed_running = lifecycle::read_active(paths, name)?.is_some();
            let _lock = lifecycle::Lock::take(paths)?;
            config::apply(paths, &graph)?;
            println!("applied {} to {}", graph.name, paths.graph(name).display());
            if changed_running {
                println!(
                    "running nodes still use their active snapshot; run isos restart to use the new graph"
                );
            }
        }
        ConfigCommand::Show => {
            println!(
                "{}",
                fs::read_to_string(paths.graph(name))
                    .map_err(|_| anyhow::anyhow!("no applied deployment; run isos config init"))?
            );
        }
    }
    Ok(())
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
        PackageCommand::Search { query } => println!(
            "{}",
            serde_json::to_string_pretty(&packages::search(&query)?)?
        ),
        PackageCommand::Info { id, version } => println!(
            "{}",
            serde_json::to_string_pretty(&packages::catalog_release(&id, &version)?)?
        ),
        PackageCommand::Install(args) => install_package(paths, args, None)?,
        PackageCommand::Link { id, version, path } => {
            let installed = packages::link(paths, &id, &version, &path)?;
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

#[derive(Deserialize)]
struct ApiRequest {
    schema_version: u32,
    op: String,
    #[serde(default)]
    deployment: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    graph: Option<Deployment>,
}
#[derive(Serialize)]
struct ApiResponse {
    schema_version: u32,
    ok: bool,
    data: Value,
    error: Option<String>,
}

fn api_call(paths: &config::Paths, request: ApiRequest) -> Result<Value> {
    ensure!(request.schema_version == 1, "API schema_version must be 1");
    ensure!(
        request
            .role
            .as_deref()
            .is_none_or(|role| matches!(role, "eos" | "egia")),
        "role must be eos or egia"
    );
    let name = config::deployment_name(request.deployment);
    match request.op.as_str() {
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

fn handle_client(paths: &config::Paths, mut stream: UnixStream) -> Result<()> {
    let mut line = String::new();
    BufReader::new(stream.try_clone()?).read_line(&mut line)?;
    ensure!(line.len() <= 2_000_000, "API request exceeds 2 MB");
    let response = match serde_json::from_str::<ApiRequest>(&line)
        .map_err(anyhow::Error::from)
        .and_then(|request| api_call(paths, request))
    {
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

fn daemon(paths: &config::Paths) -> Result<()> {
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
    println!("IsOS API listening on {}", path.display());
    for client in listener.incoming() {
        match client {
            Ok(client) => {
                if let Err(error) = handle_client(paths, client) {
                    eprintln!("API client: {error}");
                }
            }
            Err(error) => eprintln!("API accept: {error}"),
        }
    }
    Ok(())
}

fn main_run() -> Result<()> {
    let cli = Cli::parse();
    let paths = config::Paths::discover()?;
    let name = cli.deployment;
    match cli.command {
        CommandLine::Auth { command } => match command {
            AuthCommand::Login => auth::login(&paths)?,
            AuthCommand::Logout => auth::logout(&paths)?,
            AuthCommand::Whoami => auth::status(&paths)?,
        },
        CommandLine::Start => {
            lifecycle::ensure_config_exists(&paths.graph(&name))?;
            for id in lifecycle::start(&paths, &name, None)? {
                println!("started {id}");
            }
        }
        CommandLine::Stop => {
            for id in lifecycle::stop(&paths, &name, None)? {
                println!("stopped {id}");
            }
        }
        CommandLine::Restart => {
            lifecycle::stop(&paths, &name, None)?;
            for id in lifecycle::start(&paths, &name, None)? {
                println!("started {id}");
            }
        }
        CommandLine::Status { json } => {
            print_status(&lifecycle::status(&paths, &name, None)?, json)?
        }
        CommandLine::Logs { node, lines } => println!("{}", lifecycle::logs(&name, &node, lines)?),
        CommandLine::Doctor => {
            println!("IsOS {}", env!("CARGO_PKG_VERSION"));
            println!("config: {}", paths.graph(&name).display());
            println!("data: {}", paths.data.display());
            println!("runtime: {}", paths.runtime.display());
            match config::load_selected(&paths, &name) {
                Ok(graph) => println!("graph: valid ({} nodes)", graph.nodes.len()),
                Err(error) => println!("graph: {error}"),
            }
            let output = Command::new("systemctl")
                .arg("--user")
                .arg("is-system-running")
                .output();
            println!(
                "user systemd: {}",
                if output.is_ok_and(|o| o.status.success()) {
                    "available"
                } else {
                    "unavailable"
                }
            );
        }
        CommandLine::Eos { command } => component(&paths, &name, "eos", command)?,
        CommandLine::Egia { command } => component(&paths, &name, "egia", command)?,
        CommandLine::Config { command } => config_command(&paths, &name, command)?,
        CommandLine::Packages { command } => package_command(&paths, command)?,
        CommandLine::Skills { command } => library(&paths, &name, "skills", command)?,
        CommandLine::Services { command } => library(&paths, &name, "services", command)?,
        CommandLine::Backends { command } => library(&paths, &name, "backends", command)?,
        CommandLine::Daemon => daemon(&paths)?,
    }
    Ok(())
}

fn main() {
    if let Err(error) = main_run() {
        eprintln!("isos: {error:#}");
        std::process::exit(1);
    }
}
