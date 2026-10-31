use super::{ConfigCommand, InstallArgs, LibraryCommand, PackageCommand, ServiceCommand};
use crate::manager::service_call;
use crate::{config, core_setup, lock, node_sets, packages, process, release, services, starter};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::net::UnixStream, path::Path};

pub(crate) fn config_command(paths: &config::Paths, name: &str, cmd: ConfigCommand) -> Result<()> {
    match cmd {
        ConfigCommand::Init { output } => {
            let default = starter::init(paths, name)?;
            if let Some(output) = output {
                ensure!(!output.exists(), "{} already exists", output.display());
                config::atomic_write(&output, &fs::read(default)?)?;
                println!("created {}", output.display());
            } else {
                if let Some(revision) = core_setup::refresh_after_init(paths, name)? {
                    println!(
                        "refreshed core launch inputs for graph {revision}; Run or Restart to use the configured build"
                    );
                    if let Err(error) = clear_saved_setup_error(paths, name) {
                        eprintln!(
                            "core inputs refreshed, but could not clear prior setup error: {error}"
                        );
                    }
                }
                println!("editable graph: {}", default.display());
            }
        }
        ConfigCommand::Validate { file } => {
            process::validate_full_graph(&file)?;
            let specs = process::project_with_packages(paths, &fs::read_to_string(&file)?)?;
            println!("valid node graph: {} IsOS processes", specs.len());
        }
        ConfigCommand::Apply { file } => {
            let specs = apply_graph_file(paths, name, &file)?;
            println!("applied node graph: {} IsOS processes", specs.len());
        }
        ConfigCommand::Show => {
            println!("{}", fs::read_to_string(paths.node_graph_path(name))?);
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
    let graph = process::normalize_source(&source)?;
    process::validate_value(paths, &graph)?;
    let normalized = serde_json::to_vec_pretty(&graph)?;
    let specs = process::project_with_packages(paths, &String::from_utf8(normalized.clone())?)?;
    let revision = hex::encode(Sha256::digest(&normalized));
    let rendered = core_setup::render_graph(paths, name, &graph, &revision)?;
    let _lock = lock::Lock::take(paths)?;
    let destination = paths.node_graph_path(name);
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
            let preview = node_sets::compose(paths, &id, &version, &set_id, &graph, &values)?;
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

fn configured_nodes(paths: &config::Paths, name: &str, group_kind: &str) -> Result<Vec<Value>> {
    let graph: Value = serde_json::from_slice(&fs::read(paths.node_graph_path(name))?)?;
    let mut found = Vec::new();
    let cores = graph["cores"]
        .as_object()
        .context("node graph lacks cores")?;
    for (core_id, core) in cores {
        let Some(layers) = core["layers"].as_object() else {
            continue;
        };
        for (layer_id, layer) in layers {
            let Some(groups) = layer["groups"].as_object() else {
                continue;
            };
            for (group_id, group) in groups {
                if group_id != group_kind {
                    continue;
                }
                let Some(nodes) = group["nodes"].as_object() else {
                    continue;
                };
                for (node_id, node) in nodes {
                    found.push(json!({
                        "id": node_id,
                        "path": format!("cores/{core_id}/layers/{layer_id}/groups/{group_id}/nodes/{node_id}"),
                        "core": core_id,
                        "enabled": node["enabled"].as_bool().unwrap_or(true),
                        "node": node,
                    }));
                }
            }
        }
    }
    Ok(found)
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
    let nodes = configured_nodes(paths, name, expected_kind)?;
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
                        node["id"].as_str().unwrap_or(""),
                        if node["enabled"] == true {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    );
                }
            }
        }
        LibraryCommand::Info { node } => {
            let selected = nodes
                .iter()
                .find(|entry| entry["id"] == node)
                .with_context(|| {
                    format!("{node} is not a {expected_kind} node in deployment {name}")
                })?;
            println!("{}", serde_json::to_string_pretty(selected)?);
        }
        LibraryCommand::Status { node, json } => {
            let selected = nodes
                .iter()
                .find(|entry| entry["id"] == node)
                .with_context(|| {
                    format!("{node} is not a {expected_kind} node in deployment {name}")
                })?;
            let deployment = if UnixStream::connect(paths.socket()).is_ok() {
                crate::manager::manager_call(paths, name, "deployment_status")?
            } else {
                json!({"state":"stopped"})
            };
            let status = json!({"node":selected,"deployment_state":deployment["state"]});
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                println!(
                    "{node}: {} in {} deployment",
                    if selected["enabled"] == true {
                        "enabled"
                    } else {
                        "disabled"
                    },
                    deployment["state"].as_str().unwrap_or("unknown")
                );
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
