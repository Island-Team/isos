mod auth;
mod backend;
mod build;
mod cli;
mod component;
mod config;
mod core_bins;
mod core_setup;
mod foreground;
mod graph;
mod http;
mod isosui;
mod lifecycle;
mod log_store;
mod manager;
mod manager_log;
mod node_options;
mod node_sets;
mod packages;
mod process;
mod readiness;
mod release;
mod services;
mod starter;
mod versions;

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use cli::*;
use foreground::*;
use graph::{Deployment, NodeKind};
use manager::*;
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
                        let state = if core["enabled"] == false {
                            "not enabled"
                        } else if core["ready"] == true {
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
            let graph_path = starter::init(&paths, &name)?;
            println!("editable graph: {}", graph_path.display());
            if backend::installed(&paths).is_empty() {
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
            match process::validate_full_graph(&graph)
                .and_then(|_| process::project_with_packages(&paths, &fs::read_to_string(&graph)?))
            {
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
            process::validate_deployment_name(&name)?;
            match command {
                V2Command::Validate { file } => {
                    process::validate_full_graph(&file)?;
                    let specs =
                        process::project_with_packages(&paths, &fs::read_to_string(&file)?)?;
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
