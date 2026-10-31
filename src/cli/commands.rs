use super::*;

pub(crate) fn print_status(status: &lifecycle::Status, as_json: bool) -> Result<()> {
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

pub(crate) fn component(
    paths: &config::Paths,
    name: &str,
    role: &str,
    cmd: ComponentCommand,
) -> Result<()> {
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

pub(crate) fn config_command(paths: &config::Paths, name: &str, cmd: ConfigCommand) -> Result<()> {
    match cmd {
        ConfigCommand::Init { output } => {
            let default = starter::init(paths, name)?;
            if let Some(output) = output {
                ensure!(!output.exists(), "{} already exists", output.display());
                config::atomic_write(&output, &fs::read(default)?)?;
                println!("created {}", output.display());
            } else {
                println!("editable graph: {}", default.display());
            }
        }
        ConfigCommand::Validate { file } => {
            process::validate_full_graph(&file)?;
            let specs = process::project_with_packages(paths, &fs::read_to_string(&file)?)?;
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

pub(crate) fn apply_graph_file(
    paths: &config::Paths,
    name: &str,
    file: &Path,
) -> Result<Vec<process::ProcessSpec>> {
    let source = fs::read_to_string(file)?;
    let mut graph = process::normalize_source(&source)?;
    process::migrate_component_commands(&mut graph)?;
    process::validate_value(paths, &graph)?;
    let normalized = serde_json::to_vec_pretty(&graph)?;
    let specs = process::project_with_packages(paths, &String::from_utf8(normalized.clone())?)?;
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
    if let Err(error) = clear_saved_setup_error(paths, name) {
        eprintln!("saved graph applied, but could not clear prior setup error: {error}");
    }
    Ok(specs)
}

pub(crate) fn clear_saved_setup_error(paths: &config::Paths, name: &str) -> Result<()> {
    let path = paths.runtime.join(format!("{name}.last-error"));
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn install_package(
    paths: &config::Paths,
    args: InstallArgs,
    kind: Option<&str>,
) -> Result<()> {
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

pub(crate) fn package_command(paths: &config::Paths, cmd: PackageCommand) -> Result<()> {
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
            let graph = process::normalize_source(&fs::read_to_string(graph)?)?;
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
            process::validate_value(paths, &preview["graph"])?;
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

pub(crate) fn library(
    paths: &config::Paths,
    name: &str,
    kind: &str,
    command: LibraryCommand,
) -> Result<()> {
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

pub(crate) fn services_command(
    paths: &config::Paths,
    name: &str,
    command: ServiceCommand,
) -> Result<()> {
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

pub(crate) fn print_init_next_steps(mode: &str) {
    println!("\nNext steps:");
    println!("  Build EOS, EGIA, and IsosUI in their owning repositories ({mode} mode).");
    println!("  Configure ISOS_AETHER_BIN_DIR, ISOS_EGIA_BIN_DIR, and ISOSUI_DIST_DIR.");
    println!("  isos version check                # inspect build and launch readiness");
    println!("  isos                               # open the workbench; cores stay stopped");
    println!("  In Graph, add a backend, then Check graph and Save graph before `isos run`.");
    println!("  Run `isos --help` for mode rules or `isos doctor` to check readiness.");
}
