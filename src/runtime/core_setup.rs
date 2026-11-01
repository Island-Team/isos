//! Bounded, on-demand setup adapters for the saved node graph.
use crate::{
    config::{self, Paths},
    graph_compat, packages, process,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_REQUEST: usize = 1024 * 1024;
const MAX_REPLY: usize = 4 * 1024 * 1024;
const MAX_FILES: usize = 32;
const MAX_FILE: usize = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const EGIA_NORTHBOUND_PORT: u16 = 16060;

#[derive(Deserialize)]
struct SetupDiagnostic {
    path: String,
    code: String,
    message: String,
}

#[derive(Deserialize)]
struct SetupFile {
    path: String,
    content: String,
}

#[derive(Deserialize)]
struct SetupReply {
    schema_version: u32,
    ok: bool,
    diagnostics: Vec<SetupDiagnostic>,
    files: Vec<SetupFile>,
}

fn revision(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn selected_adapter(paths: &Paths, role: &str) -> Result<PathBuf> {
    match role {
        "eos" => crate::core_bins::resolve("aether-profile"),
        "egia" => {
            let _ = paths;
            let directory = PathBuf::from(
                std::env::var_os("ISOS_EGIA_BIN_DIR")
                    .context("configure paths.egia_bin_dir before enabling EGIA")?,
            );
            ensure!(
                directory.is_absolute(),
                "ISOS_EGIA_BIN_DIR must be absolute"
            );
            let path = directory.join("egia-config");
            ensure!(
                path.is_file(),
                "EGIA setup adapter missing at {}",
                path.display()
            );
            Ok(path)
        }
        _ => bail!("unknown setup core {role}"),
    }
}

pub fn adapter_path(paths: &Paths, role: &str) -> Result<PathBuf> {
    selected_adapter(paths, role)
}

fn package_refs(paths: &Paths, graph: &Value) -> Result<Vec<Value>> {
    packages::verify_graph_packages(paths, graph)?.into_iter().map(|package| {
        Ok(json!({"id":package.release.id,"version":package.release.version,"path":package.store}))
    }).collect()
}

fn command_arg(graph: &Value, node: &str, flag: &str) -> Result<Option<String>> {
    let pointer = format!("/cores/isos/layers/process/groups/component/nodes/{node}/settings");
    let Some(settings) = graph.pointer(&pointer) else {
        return Ok(None);
    };
    let args = settings["args"].as_array().with_context(|| {
        format!("{node} process requires settings.args; save the migrated graph")
    })?;
    ensure!(
        args.iter().all(Value::is_string),
        "{node} settings.args must contain strings"
    );
    let mut values = args.iter().filter_map(Value::as_str);
    let mut selected = None;
    while let Some(arg) = values.next() {
        if arg == flag {
            ensure!(selected.is_none(), "{node} has duplicate {flag}");
            let value = values
                .next()
                .with_context(|| format!("{node} {flag} needs a value"))?;
            ensure!(
                !value.is_empty() && !value.starts_with("--"),
                "{node} {flag} needs a value"
            );
            selected = Some(value.to_owned());
        }
    }
    selected
        .map(Some)
        .with_context(|| format!("{node} process requires {flag}"))
}

fn listener(value: Option<String>, default: u16) -> Result<Value> {
    let Some(value) = value else {
        return Ok(json!({"host":"127.0.0.1","port":default}));
    };
    let (host, port) = value
        .rsplit_once(':')
        .with_context(|| format!("listener {value} must be 127.0.0.1:<port>"))?;
    ensure!(host == "127.0.0.1", "listener {value} must bind 127.0.0.1");
    let port = port
        .parse::<u16>()
        .with_context(|| format!("listener {value} has invalid port"))?;
    ensure!(port >= 1024, "listener {value} port must be 1024–65535");
    Ok(json!({"host":host,"port":port}))
}

pub(crate) fn endpoints(paths: &Paths, name: &str, graph: &Value) -> Result<Value> {
    let api_port = crate::http::local_port()?;
    let eos_socket = command_arg(graph, "eos", "--socket")?
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.runtime.join(format!("{name}-eos.sock")));
    ensure!(
        eos_socket.is_absolute(),
        "EOS --socket must be an absolute path"
    );
    Ok(json!({
        "isos_api":{"url":format!("http://127.0.0.1:{api_port}")},
        "eos_control":listener(command_arg(graph,"bridge","--listen")?,16020)?,
        "eos_catalog":listener(command_arg(graph,"config","--listen")?,16021)?,
        "egia_northbound":listener(None,EGIA_NORTHBOUND_PORT)?,
        "eos_socket":{"path":eos_socket}
    }))
}

fn request(
    paths: &Paths,
    name: &str,
    role: &str,
    op: &str,
    graph: &Value,
    graph_revision: &str,
) -> Result<Value> {
    ensure!(matches!(role, "eos" | "egia"), "unknown setup core");
    ensure!(
        matches!(op, "catalog" | "validate" | "render"),
        "unknown setup operation"
    );
    let request = json!({"schema_version":1,"op":op,"core":role,"revision":graph_revision,
        "graph":graph,"endpoints":endpoints(paths,name,graph)?,"packages":package_refs(paths, graph)?});
    ensure!(
        serde_json::to_vec(&request)?.len() <= MAX_REQUEST,
        "setup request exceeds 1 MiB"
    );
    Ok(request)
}

fn invoke(program: &Path, request: &Value) -> Result<Value> {
    let mut child = Command::new(program)
        .args(["setup", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start setup adapter {}", program.display()))?;
    let input = serde_json::to_vec(request)?;
    let writer = child.stdin.take().unwrap();
    let write_task = thread::spawn(move || {
        let mut writer = writer;
        writer.write_all(&input)
    });
    let stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut data = Vec::new();
        stdout
            .take((MAX_REPLY + 1) as u64)
            .read_to_end(&mut data)
            .map(|_| data)
    });
    let stderr = child.stderr.take().unwrap();
    let error_reader = thread::spawn(move || {
        let mut data = Vec::new();
        stderr.take(8193).read_to_end(&mut data).map(|_| data)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            bail!("setup adapter timed out after 10 seconds");
        }
        thread::sleep(Duration::from_millis(10));
    };
    write_task
        .join()
        .map_err(|_| anyhow::anyhow!("setup adapter request writer failed"))??;
    let output = reader
        .join()
        .map_err(|_| anyhow::anyhow!("setup adapter reader failed"))??;
    let stderr = error_reader
        .join()
        .map_err(|_| anyhow::anyhow!("setup adapter stderr reader failed"))??;
    ensure!(
        output.len() <= MAX_REPLY,
        "setup adapter response exceeds 4 MiB"
    );
    ensure!(
        status.success(),
        "setup adapter failed: {}",
        String::from_utf8_lossy(&stderr)
    );
    let reply: Value = serde_json::from_slice(&output).context("parse setup adapter response")?;
    let typed: SetupReply = serde_json::from_value(reply.clone())
        .context("setup adapter response violates schema 2")?;
    ensure!(typed.schema_version == 1, "setup adapter schema mismatch");
    ensure!(
        typed
            .diagnostics
            .iter()
            .all(|item| !item.path.is_empty() && !item.code.is_empty() && !item.message.is_empty()),
        "setup adapter diagnostic fields must be nonempty"
    );
    ensure!(
        typed
            .files
            .iter()
            .all(|item| !item.path.is_empty() && item.content.len() <= MAX_FILE),
        "setup adapter file is invalid or exceeds 1 MiB"
    );
    ensure!(
        typed.ok || !typed.diagnostics.is_empty(),
        "setup adapter rejected request without diagnostics"
    );
    Ok(reply)
}

fn safe_file(role: &str, entry: &Value) -> Result<(PathBuf, Vec<u8>)> {
    let path = entry["path"].as_str().context("setup file has no path")?;
    ensure!(path.len() <= 256, "setup file path too long");
    let relative = Path::new(path);
    ensure!(
        !relative.is_absolute() && !path.contains('\\'),
        "setup file path must be relative"
    );
    ensure!(
        relative
            .components()
            .all(|part| matches!(part, Component::Normal(_))),
        "setup file path is unsafe"
    );
    ensure!(
        relative
            .components()
            .next()
            .is_some_and(|part| part.as_os_str() == role),
        "setup file must stay under its core directory"
    );
    let content = entry["content"]
        .as_str()
        .context("setup file has no string content")?
        .as_bytes()
        .to_vec();
    ensure!(content.len() <= MAX_FILE, "setup file exceeds 1 MiB");
    Ok((relative.to_path_buf(), content))
}

pub fn call(
    paths: &Paths,
    name: &str,
    role: &str,
    op: &str,
    graph: &Value,
    graph_revision: &str,
) -> Result<Value> {
    graph_compat::admit_legacy_mapped(graph, process::catalog().ok().as_ref())?;
    let request = request(paths, name, role, op, graph, graph_revision)?;
    invoke(&selected_adapter(paths, role)?, &request)
}

pub fn identity(paths: &Paths, role: &str) -> Result<Value> {
    ensure!(matches!(role, "eos" | "egia"), "unknown setup core {role}");
    let reply = invoke(
        &selected_adapter(paths, role)?,
        &json!({"schema_version":1,"op":"identity","core":role}),
    )?;
    ensure!(reply["ok"] == true, "{role} identity rejected");
    let identity = reply
        .get("identity")
        .context("identity reply missing identity")?;
    ensure!(
        identity["core"] == role
            && identity["graph_version"] == "1.0.0"
            && identity["node_type_version"] == "1.0"
            && identity["setup_schema_version"] == 1
            && identity["product_version"]
                .as_str()
                .is_some_and(|version| !version.is_empty()),
        "{role} identity is incompatible with IsOS",
    );
    Ok(identity.clone())
}

pub fn prepare_run(paths: &Paths, name: &str, expected_revision: &str) -> Result<()> {
    let graph_path = paths.node_graph_path(name);
    let saved = graph_compat::read(&graph_path)
        .with_context(|| format!("read {}", graph_path.display()))?;
    ensure!(
        saved.revision == expected_revision,
        "node graph revision changed during setup"
    );
    let graph = saved.value;
    graph_compat::admit_legacy_mapped(&graph, process::catalog().ok().as_ref())?;
    if graph["version"] == "0.1.0" {
        let proposal = crate::graph_migration::Proposal {
            graph: graph.clone(),
            bytes: Vec::new(),
            revision: expected_revision.to_owned(),
            changes: Vec::new(),
        };
        let prepared = crate::graph_migration::prepare(paths, name, &proposal)
            .map_err(|diagnostic| anyhow::anyhow!(diagnostic.to_string()))?;
        verify_rendered_snapshot(paths, name, &graph, expected_revision, &prepared.files)?;
        verify_launch_inputs(paths, name)?;
        return Ok(());
    }
    let files = render_graph(paths, name, &graph, expected_revision)?;
    let _lock = crate::lock::Lock::take(paths)?;
    ensure!(
        revision(&fs::read(&graph_path)?) == expected_revision,
        "node graph revision changed during setup"
    );
    write_rendered(paths, name, files)
}

/// Revalidate a previously applied graph against the currently configured cores.
/// A fresh draft has no launch inputs and remains unapplied.
pub fn refresh_after_init(paths: &Paths, name: &str) -> Result<Option<String>> {
    let graph_path = paths.node_graph_path(name);
    let saved = graph_compat::read(&graph_path)
        .with_context(|| format!("read {}", graph_path.display()))?;
    if saved.value["version"] == "0.1.0" {
        if !snapshot_root(paths, name, &saved.revision).exists()
            && !paths.generated(name).join("selection.json").exists()
            && !paths
                .generated(name)
                .join("v2/required")
                .join(format!("{}.json", saved.revision))
                .exists()
        {
            return Ok(None);
        }
        verify_launch_inputs(paths, name).map_err(|error| {
            anyhow::anyhow!(
                "0.1.0 generated inputs are stale or invalid: {error:#}; restore the selected inputs or Apply the same saved revision again; `isos init` keeps the generation immutable"
            )
        })?;
        return Ok(None);
    }
    if !paths
        .generated(name)
        .join("core-setup/launch-inputs.json")
        .is_file()
    {
        return Ok(None);
    }
    let graph = saved.value;
    if let Some(diagnostic) = graph_compat::version_diagnostic(&graph) {
        bail!(
            "saved graph version is incompatible: {diagnostic}; run `isos init` to create a v1 starter after resolving migration; {} was left untouched",
            graph_path.display()
        );
    }
    graph_compat::admit_legacy_mapped(&graph, process::catalog().ok().as_ref())?;
    ensure!(
        graph["version"] == "1.0.0",
        "saved graph version {} cannot be refreshed as graph format 1.0.0; back up {}, run `isos init` to create a v1 starter, then add nodes and Save graph; the old file was left untouched",
        graph["version"],
        graph_path.display()
    );
    crate::process::validate_value(paths, &graph)?;
    let specs = crate::process::project_with_packages(paths, &serde_json::to_string(&graph)?)?;
    crate::preflight::verify_processes(paths, &specs)?;
    let expected_revision = saved.revision;
    let files = render_graph(paths, name, &graph, &expected_revision)?;
    let _lock = crate::lock::Lock::take(paths)?;
    ensure!(
        revision(&fs::read(&graph_path)?) == expected_revision,
        "node graph changed during init; retry after saving the graph"
    );
    write_rendered(paths, name, files)?;
    Ok(Some(expected_revision))
}

pub fn render_graph(
    paths: &Paths,
    name: &str,
    graph: &Value,
    expected_revision: &str,
) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let egia_process = graph
        .pointer("/cores/isos/layers/process/groups/component/nodes/egia")
        .is_some_and(|node| node["enabled"] != false);
    let roles = ["eos", "egia"].into_iter().filter(|role| {
        *role == "eos" && graph["cores"].get("eos").is_some() || *role == "egia" && egia_process
    });
    let mut files = Vec::new();
    let mut seen_files = BTreeSet::new();
    for role in roles {
        let reply =
            call(paths, name, role, "render", graph, expected_revision).map_err(|error| {
                if graph["version"] == "0.1.0" {
                    anyhow::Error::new(crate::generated_v2::owner_rejected(role))
                } else {
                    error
                }
            })?;
        if graph["version"] == "0.1.0" && reply["ok"] != true {
            return Err(crate::generated_v2::owner_rejected(role).into());
        }
        ensure!(
            reply["ok"] == true,
            "{role} setup rejected graph: {}",
            reply["diagnostics"]
        );
        let entries = reply["files"].as_array().unwrap();
        ensure!(
            entries.len() <= MAX_FILES,
            "setup adapter returned too many files"
        );
        for entry in entries {
            let file = safe_file(role, entry).map_err(|error| {
                if graph["version"] == "0.1.0" {
                    anyhow::Error::new(crate::generated_v2::unsafe_render())
                } else {
                    error
                }
            })?;
            ensure!(
                seen_files.insert(file.0.clone()),
                "duplicate setup file path"
            );
            files.push(file);
        }
    }
    let mut inputs = Vec::new();
    if graph["cores"].get("eos").is_some() {
        for binary in [
            "aether-profile",
            "aether-runtime",
            "aether-http-bridge",
            "aether-config-server",
        ] {
            let path = crate::core_bins::resolve(binary)?;
            inputs.push(json!({"core":"eos","binary":binary,"path":path,"sha256":revision(&fs::read(&path)?)}));
        }
    }
    if egia_process {
        for binary in ["egia-config", "egia"] {
            let path = selected_adapter(paths, "egia")?.with_file_name(binary);
            inputs.push(json!({"core":"egia","binary":binary,"path":path,"sha256":revision(&fs::read(&path)?)}));
        }
    }
    files.push((
        PathBuf::from("launch-inputs.json"),
        serde_json::to_vec_pretty(&inputs)?,
    ));
    Ok(files)
}

pub fn verify_launch_inputs(paths: &Paths, name: &str) -> Result<()> {
    let graph_path = paths.node_graph_path(name);
    let saved = graph_compat::read(&graph_path)?;
    let file = if saved.value["version"] == "0.1.0" {
        verify_revisioned(paths, name, &saved.value, &saved.revision)?;
        crate::generated_v2::selected_root(paths, name, &saved.value, &saved.revision)?
            .join("core-setup/launch-inputs.json")
    } else {
        paths.generated(name).join("core-setup/launch-inputs.json")
    };
    let inputs: Value = serde_json::from_slice(&fs::read(&file).with_context(|| {
        format!(
            "saved launch inputs are missing at {}; save the graph",
            file.display()
        )
    })?)?;
    let remedy = if saved.value["version"] == "0.1.0" {
        "restore the pinned binary or create a new graph revision and Apply"
    } else {
        "run `isos init`, then restart"
    };
    for input in inputs.as_array().context("invalid saved launch inputs")? {
        let path = Path::new(
            input["path"]
                .as_str()
                .context("launch input path is missing")?,
        );
        ensure!(path.is_absolute(), "launch input path must be absolute");
        let configured = match input["core"].as_str() {
            Some("eos") => crate::core_bins::resolve(
                input["binary"]
                    .as_str()
                    .context("launch input binary is missing")?,
            )?,
            Some("egia") => selected_adapter(paths, "egia")?.with_file_name(
                input["binary"]
                    .as_str()
                    .context("EGIA launch input binary missing")?,
            ),
            _ => bail!("unknown launch input core"),
        };
        ensure!(
            path == configured,
            "configured path for {} changed since graph Apply; {remedy}",
            input["binary"]
        );
        let current = revision(&fs::read(path).with_context(|| {
            format!(
                "configured file {} is missing; save the graph after rebuilding",
                path.display()
            )
        })?);
        ensure!(
            input["sha256"] == current,
            "configured file {} changed since graph Apply; {remedy}",
            path.display()
        );
    }
    Ok(())
}

pub fn snapshot_root(paths: &Paths, name: &str, graph_revision: &str) -> PathBuf {
    paths.generated(name).join(graph_revision)
}

pub fn write_revisioned(
    paths: &Paths,
    name: &str,
    graph: &Value,
    graph_revision: &str,
    files: Vec<(PathBuf, Vec<u8>)>,
) -> Result<PathBuf> {
    crate::generated_v2::publish(paths, name, graph, graph_revision, &files)
        .map_err(anyhow::Error::new)
}

pub fn verify_revisioned(
    paths: &Paths,
    name: &str,
    graph: &Value,
    graph_revision: &str,
) -> Result<()> {
    if crate::generated_v2::selected(paths, name, graph, graph_revision, None)?.is_some() {
        return Ok(());
    }
    let root = snapshot_root(paths, name, graph_revision);
    let manifest: Value = serde_json::from_slice(&fs::read(root.join("manifest.json"))?)?;
    ensure!(
        manifest["schema_version"] == 1
            && manifest["source_revision"] == graph_revision
            && manifest["graph_sha256"] == graph_revision,
        "generated manifest differs from saved graph revision"
    );
    let entries = manifest["files"]
        .as_array()
        .context("generated manifest lacks files")?;
    for entry in entries {
        let relative = Path::new(
            entry["path"]
                .as_str()
                .context("generated file path missing")?,
        );
        ensure!(
            relative
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "generated manifest has unsafe file path"
        );
        let file = root.join(relative);
        ensure!(
            fs::symlink_metadata(&file)?.file_type().is_file(),
            "generated file is not regular: {}",
            file.display()
        );
        ensure!(
            entry["sha256"] == revision(&fs::read(&file)?),
            "generated file digest changed: {}",
            file.display()
        );
    }
    for required in ["eos/graph.json", "core-setup/launch-inputs.json"] {
        ensure!(
            entries.iter().any(|entry| entry["path"] == required),
            "generated manifest lacks {required}"
        );
    }
    let packages = package_refs(paths, graph)?;
    let recorded = manifest["packages"]
        .as_array()
        .context("generated manifest lacks packages")?;
    ensure!(
        packages.len() == recorded.len(),
        "generated package set changed"
    );
    for (package, saved) in packages.iter().zip(recorded) {
        let receipt = Path::new(package["path"].as_str().unwrap()).join("receipt.json");
        ensure!(
            saved["id"] == package["id"]
                && saved["version"] == package["version"]
                && saved["sha256"] == revision(&fs::read(receipt)?),
            "generated package receipt changed"
        );
    }
    Ok(())
}

pub fn verify_rendered_snapshot(
    paths: &Paths,
    name: &str,
    graph: &Value,
    graph_revision: &str,
    expected: &[(PathBuf, Vec<u8>)],
) -> Result<()> {
    if crate::generated_v2::selected(paths, name, graph, graph_revision, Some(expected))?.is_some()
    {
        return Ok(());
    }
    verify_revisioned(paths, name, graph, graph_revision)?;
    let root = snapshot_root(paths, name, graph_revision);
    let manifest: Value = serde_json::from_slice(&fs::read(root.join("manifest.json"))?)?;
    let entries = manifest["files"]
        .as_array()
        .context("generated manifest lacks files")?;
    ensure!(
        entries.len() == expected.len(),
        "generated file set differs from owner render"
    );
    for (relative, content) in expected {
        let relative = if relative == Path::new("launch-inputs.json") {
            Path::new("core-setup/launch-inputs.json")
        } else {
            relative.as_path()
        };
        let path = relative.to_string_lossy();
        ensure!(
            entries.iter().any(|entry| entry["path"] == path.as_ref()),
            "generated manifest lacks owner-rendered file {}",
            relative.display()
        );
        ensure!(
            fs::read(root.join(relative))? == *content,
            "generated file differs from owner render: {}",
            relative.display()
        );
    }
    Ok(())
}

pub fn write_rendered(paths: &Paths, name: &str, files: Vec<(PathBuf, Vec<u8>)>) -> Result<()> {
    let generated = paths.generated(name);
    fs::create_dir_all(&generated)?;
    let root = generated.join("core-setup");
    let staging = tempfile::Builder::new()
        .prefix("core-setup-")
        .tempdir_in(&generated)?;
    for (relative, content) in files {
        config::atomic_write(&staging.path().join(relative), &content)?;
    }
    let backup = generated.join("core-setup.previous");
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    if root.exists() {
        fs::rename(&root, &backup)?;
    }
    if let Err(error) = fs::rename(staging.path(), &root) {
        if backup.exists() {
            let _ = fs::rename(&backup, &root);
        }
        return Err(error.into());
    }
    if backup.exists() {
        fs::remove_dir_all(backup)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn paths(root: &Path) -> Paths {
        Paths {
            config: root.join("config"),
            data: root.join("data"),
            runtime: root.join("run"),
        }
    }

    #[test]
    fn rejects_escaping_adapter_paths() {
        for path in ["../eos/x", "/tmp/x", "egia/x", "eos/../x", "eos\\x"] {
            assert!(
                safe_file("eos", &json!({"path":path,"content":"x"})).is_err(),
                "{path}"
            );
        }
        assert_eq!(
            safe_file("eos", &json!({"path":"eos/graph.json","content":"{}"}))
                .unwrap()
                .1,
            b"{}"
        );
    }

    #[test]
    fn adapter_reply_is_bounded_and_versioned() {
        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("adapter");
        fs::write(&script, "#!/bin/sh\ncat >/dev/null\nprintf '{\"schema_version\":1,\"ok\":true,\"diagnostics\":[],\"files\":[]}'\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let reply = invoke(&script, &json!({"schema_version":1})).unwrap();
        assert_eq!(reply["ok"], true);
        fs::write(&script, "#!/bin/sh\ncat >/dev/null\nprintf '{\"schema_version\":2,\"ok\":true,\"diagnostics\":[],\"files\":[]}'\n").unwrap();
        assert!(invoke(&script, &json!({"schema_version":1})).is_err());
        fs::write(&script, "#!/bin/sh\ncat >/dev/null\nprintf '{\"schema_version\":1,\"ok\":false,\"diagnostics\":[],\"files\":[]}'\n").unwrap();
        assert!(invoke(&script, &json!({"schema_version":1})).is_err());
        fs::write(&script, "#!/bin/sh\ncat >/dev/null\nprintf '{\"schema_version\":1,\"ok\":true,\"diagnostics\":[],\"files\":[{\"path\":\"eos/graph.json\"}]}'\n").unwrap();
        assert!(invoke(&script, &json!({"schema_version":1})).is_err());
    }

    #[test]
    fn explicit_endpoints_reject_missing_malformed_and_duplicate_flags() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths(temp.path());
        let mut graph = json!({"cores":{"isos":{"layers":{"process":{"groups":{"component":{"nodes":{
            "eos":{"settings":{"args":["--socket", "/tmp/eos.sock"]}},
            "bridge":{"settings":{"args":["--listen", "127.0.0.1:16020"]}}
        }}}}}}}});
        let resolved = endpoints(&paths, "default", &graph).unwrap();
        assert_eq!(resolved["eos_control"]["port"], 16020);
        assert_eq!(resolved["egia_northbound"]["port"], EGIA_NORTHBOUND_PORT);
        graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]["bridge"]["settings"]
            ["args"] = json!(["--listen", "bad"]);
        assert!(endpoints(&paths, "default", &graph).is_err());
        graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]["bridge"]["settings"]
            ["args"] = json!(["--listen", "127.0.0.1:16020", "--listen", "127.0.0.1:16021"]);
        assert!(endpoints(&paths, "default", &graph).is_err());
        graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]["bridge"]["settings"]
            ["args"] = json!([]);
        assert!(endpoints(&paths, "default", &graph).is_err());
    }

    #[test]
    fn changed_graph_revision_aborts_without_output() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths(temp.path());
        paths.prepare().unwrap();
        config::atomic_write(
            &paths.node_graph_path("default"),
            br#"{"version":"1.0.0","robot":{},"cores":{}}"#,
        )
        .unwrap();
        assert!(prepare_run(&paths, "default", "stale").is_err());
        assert!(!paths.generated("default").exists());
        let revision = crate::starter::revision(&paths.node_graph_path("default")).unwrap();
        let error = prepare_run(&paths, "default", &revision).unwrap_err();
        assert!(error.to_string().contains("/robot/id"));
        assert!(!paths.generated("default").exists());
    }

    #[test]
    fn paired_generated_file_and_manifest_tamper_cannot_replace_owner_render() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths(temp.path());
        paths.prepare().unwrap();
        let graph = json!({"version":"0.1.0","robot":{},"cores":{}});
        let graph_revision = revision(&serde_json::to_vec_pretty(&graph).unwrap());
        let expected = vec![
            (
                PathBuf::from("eos/graph.json"),
                b"original projection".to_vec(),
            ),
            (PathBuf::from("launch-inputs.json"), b"[]".to_vec()),
        ];
        let root =
            write_revisioned(&paths, "default", &graph, &graph_revision, expected.clone()).unwrap();
        verify_rendered_snapshot(&paths, "default", &graph, &graph_revision, &expected).unwrap();
        fs::write(root.join("eos/graph.json"), b"changed projection").unwrap();
        let manifest_path = root.join("manifest.json");
        let mut manifest: Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        let entry = manifest["files"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["path"] == "eos/graph.json")
            .unwrap();
        entry["sha256"] = json!(revision(b"changed projection"));
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        assert!(
            verify_revisioned(&paths, "default", &graph, &graph_revision)
                .unwrap_err()
                .to_string()
                .contains("generated_render_mismatch")
        );
        assert!(
            verify_rendered_snapshot(&paths, "default", &graph, &graph_revision, &expected)
                .unwrap_err()
                .to_string()
                .contains("generated_render_mismatch")
        );
    }

    #[test]
    fn init_reports_stale_new_graph_inputs_without_rewriting_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths(temp.path());
        paths.prepare().unwrap();
        let graph = json!({"version":"0.1.0","robot":{},"cores":{}});
        let bytes = serde_json::to_vec_pretty(&graph).unwrap();
        let graph_revision = revision(&bytes);
        config::atomic_write(&paths.node_graph_path("default"), &bytes).unwrap();
        let files = vec![
            (PathBuf::from("eos/graph.json"), b"{}".to_vec()),
            (PathBuf::from("launch-inputs.json"), b"[]".to_vec()),
        ];
        let root = write_revisioned(&paths, "default", &graph, &graph_revision, files).unwrap();
        let manifest_before = fs::read(root.join("manifest.json")).unwrap();
        fs::write(root.join("eos/graph.json"), b"tampered").unwrap();
        let error = refresh_after_init(&paths, "default")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("generated inputs are stale or invalid"),
            "{error}"
        );
        assert!(
            error.contains("Apply the same saved revision again"),
            "{error}"
        );
        assert_eq!(
            fs::read(root.join("manifest.json")).unwrap(),
            manifest_before
        );
        assert_eq!(fs::read(paths.node_graph_path("default")).unwrap(), bytes);
    }

    #[test]
    fn init_refresh_requires_applied_v1_graph_and_preserves_obsolete_graph() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths(temp.path());
        paths.prepare().unwrap();
        let graph_path = paths.node_graph_path("default");
        let old_graph = br#"{"version":"2.0.0","robot":{},"cores":{}}"#;
        config::atomic_write(&graph_path, old_graph).unwrap();
        assert!(refresh_after_init(&paths, "default").unwrap().is_none());
        let launch_inputs = paths
            .generated("default")
            .join("core-setup/launch-inputs.json");
        config::atomic_write(&launch_inputs, b"[]").unwrap();
        let error = refresh_after_init(&paths, "default")
            .unwrap_err()
            .to_string();
        assert!(error.contains("saved graph version"));
        assert!(error.contains("run `isos init` to create a v1 starter"));
        assert_eq!(fs::read(&graph_path).unwrap(), old_graph);
        assert_eq!(fs::read(launch_inputs).unwrap(), b"[]");
    }
}
