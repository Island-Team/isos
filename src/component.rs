//! Calls isolated EOS/EGIA configuration adapters; never imports their runtimes.
use crate::{
    config::{self, Paths},
    graph::Deployment,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub fn binary(role: &str) -> String {
    let key = format!("ISOS_{}_CONFIG_BIN", role.to_ascii_uppercase());
    if let Ok(path) = std::env::var(key) {
        return path;
    }
    let root_key = if role == "eos" {
        "EOS_REPO_PATH"
    } else {
        "EGIA_REPO_PATH"
    };
    if let Ok(root) = std::env::var(root_key) {
        let path = if role == "eos" {
            PathBuf::from(root).join("build/aether/src/eos/bin/eos-config")
        } else {
            PathBuf::from(root).join("bin/egia-config")
        };
        if path.exists() {
            return path.to_string_lossy().into_owned();
        }
    }
    if let Ok(paths) = Paths::discover() {
        let pin = paths.config.join("pins").join(format!("{role}.json"));
        if let Ok(bytes) = fs::read(pin)
            && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
            && let Some(path) = value.get("config_binary").and_then(Value::as_str)
            && Path::new(path).is_file()
        {
            return path.into();
        }
    }
    format!("{role}-config")
}
fn source_root(paths: &Paths, role: &str) -> Option<PathBuf> {
    let key = if role == "eos" {
        "EOS_REPO_PATH"
    } else {
        "EGIA_REPO_PATH"
    };
    if let Some(path) = std::env::var_os(key) {
        return Some(PathBuf::from(path));
    }
    let bytes = fs::read(paths.config.join("pins").join(format!("{role}.json"))).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    Some(PathBuf::from(value.get("source_root")?.as_str()?))
}
pub fn call(role: &str, op: &str, values: Option<Value>) -> Result<Value> {
    ensure!(matches!(role, "eos" | "egia"), "unknown component");
    ensure!(
        matches!(
            op,
            "describe" | "defaults" | "profiles" | "render" | "validate"
        ),
        "unknown config operation"
    );
    let mut command = Command::new(binary(role));
    command
        .args([op, "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if values.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("start {role}-config"))?;
    if let Some(values) = values {
        let payload = json!({"schema_version":1,"values":values});
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.to_string().as_bytes())?;
    }
    let output = child.wait_with_output()?;
    let reply: Value = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "{role}-config {op}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    ensure!(
        output.status.success() && reply["ok"] == true,
        "{role}-config {op}: {}",
        reply["diagnostics"]
    );
    Ok(reply["data"].clone())
}
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    for item in fs::read_dir(source)? {
        let item = item?;
        let kind = item.file_type()?;
        let to = destination.join(item.file_name());
        if kind.is_dir() {
            fs::create_dir_all(&to)?;
            copy_tree(&item.path(), &to)?;
        } else if kind.is_file() {
            fs::create_dir_all(to.parent().unwrap())?;
            fs::copy(item.path(), to)?;
        } else {
            anyhow::bail!("EGIA config tree contains unsupported link");
        }
    }
    Ok(())
}
pub fn render(paths: &Paths, graph: &Deployment, staging: &Path) -> Result<()> {
    if graph.schema_version != 2 {
        return Ok(());
    }
    let eos = graph
        .nodes
        .iter()
        .find(|n| n.role.as_deref() == Some("eos"));
    let egia = graph
        .nodes
        .iter()
        .find(|n| n.role.as_deref() == Some("egia"));
    let socket = graph
        .nodes
        .iter()
        .find(|n| n.profile.as_ref().is_some_and(|p| p.id == "eos.uds"))
        .and_then(|n| n.settings.get("socket_path"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            paths
                .runtime
                .join(format!("{}-eos.sock", graph.name))
                .to_string_lossy()
                .into_owned()
        });
    for (role, node) in [("eos", eos), ("egia", egia)] {
        let Some(node) = node else {
            continue;
        };
        let mut values = call(role, "defaults", None)?;
        if role == "eos" {
            values["grpc_uds_path"] = json!(socket);
            for (key, value) in &node.settings {
                if values.get(key).is_some() {
                    values[key] = value.clone();
                }
            }
        } else {
            values["gateway"] = json!({"transport":"uds","path":socket});
            for (key, value) in &node.settings {
                if values.get(key).is_some() {
                    values[key] = value.clone();
                }
            }
            if let Some(root) = source_root(paths, "egia") {
                copy_tree(&root.join("etc/egia"), &staging.join("egia"))?;
            }
        }
        let data = call(role, "render", Some(values.clone()))?;
        let files = data["files"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("{role}-config render has no files"))?;
        for file in files {
            let path = file["path"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("render path missing"))?;
            ensure!(
                !path.starts_with('/') && path.split('/').all(|p| !matches!(p, "" | "." | "..")),
                "unsafe rendered path"
            );
            let content = file["content"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("render content missing"))?;
            config::atomic_write(&staging.join(path), content.as_bytes())?;
        }
        if role == "eos" {
            let backend = values["binding_backend"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("EOS binding backend missing"))?;
            let profile = values["binding_profile"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("EOS binding profile missing"))?;
            let executable = node
                .launch
                .as_ref()
                .and_then(|l| l.command.first())
                .map(String::as_str)
                .unwrap_or("/usr/bin/false");
            let stack = format!(
                "version = 1\neos_config = {}\nmodules = []\n\n[[service]]\nname = \"eosd\"\nrole = \"managed\"\ncommand = [{}]\nready_tcp = \"127.0.0.1:16200\"\n\n[[binding]]\nbackend = {}\nprofile = {}\n",
                json!(
                    paths
                        .generated(&graph.name)
                        .join("eos/eos.toml")
                        .to_string_lossy()
                        .to_string()
                ),
                json!(executable),
                json!(backend),
                json!(profile)
            );
            config::atomic_write(&staging.join("eos-runtime/stack.toml"), stack.as_bytes())?;
        }
    }
    Ok(())
}

/// Check graph references and typed edges against component-owned descriptors.
pub fn validate_profiles(graph: &Deployment) -> Result<()> {
    if graph.schema_version != 2 {
        return Ok(());
    }
    let mut profiles = Vec::<Value>::new();
    for role in ["eos", "egia"] {
        let data = call(role, "profiles", None)?;
        if let Some(items) = data.get("profiles").unwrap_or(&data).as_array() {
            profiles.extend(items.iter().cloned());
        }
    }
    for node in &graph.nodes {
        if let Some(reference) = &node.profile {
            ensure!(
                profiles
                    .iter()
                    .any(|p| p["id"] == reference.id && p["version"] == reference.version),
                "{} references unavailable profile {} {}",
                node.id,
                reference.id,
                reference.version
            );
        }
    }
    for edge in &graph.edges {
        let (Some(input), Some(output)) = (&edge.input, &edge.output) else {
            continue;
        };
        let consumer = graph.nodes.iter().find(|n| n.id == edge.from).unwrap();
        let provider = graph.nodes.iter().find(|n| n.id == edge.to).unwrap();
        let a = profiles
            .iter()
            .find(|p| consumer.profile.as_ref().is_some_and(|r| p["id"] == r.id))
            .unwrap();
        let b = profiles
            .iter()
            .find(|p| provider.profile.as_ref().is_some_and(|r| p["id"] == r.id))
            .unwrap();
        let input_port = a["inputs"]
            .as_array()
            .and_then(|v| v.iter().find(|p| p["name"] == *input))
            .ok_or_else(|| anyhow::anyhow!("{} input {input} missing", edge.from))?;
        let output_port = b["outputs"]
            .as_array()
            .and_then(|v| v.iter().find(|p| p["name"] == *output))
            .ok_or_else(|| anyhow::anyhow!("{} output {output} missing", edge.to))?;
        ensure!(
            input_port["type"] == output_port["type"],
            "incompatible port type on {} -> {}",
            edge.from,
            edge.to
        );
        let requires = input_port["contracts"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("input has no contracts"))?;
        let provides = output_port["contracts"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("output has no contracts"))?;
        for required in requires {
            ensure!(
                provides
                    .iter()
                    .any(|p| p["name"] == required["name"] && p["version"] == required["version"]),
                "{} -> {} contract mismatch: {} {}",
                edge.from,
                edge.to,
                required["name"],
                required["version"]
            );
        }
    }
    Ok(())
}
