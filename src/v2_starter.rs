//! Create the editable shared graph from the built-in MuJoCo preset.

use crate::{config::Paths, v2_backend};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn executable(name: &str) -> String {
    let override_key = format!("ISOS_{}_BIN", name.to_ascii_uppercase().replace('-', "_"));
    if let Ok(path) = std::env::var(&override_key) {
        return path;
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }
    format!("/usr/bin/{name}")
}

fn at<'a>(graph: &'a mut Value, pointer: &str) -> Result<&'a mut Value> {
    graph
        .pointer_mut(pointer)
        .with_context(|| format!("preset lacks {pointer}"))
}

pub fn draft(paths: &Paths, name: &str) -> Result<Value> {
    let preset: toml::Value = toml::from_str(include_str!("../templates/mujoco-graph-v2.toml"))?;
    let mut graph = serde_json::to_value(preset)?;
    at(&mut graph, "/robot/id")?.clone_from(&json!(name));
    let socket = paths.runtime.join(format!("{name}-mujoco.sock"));
    let socket = socket.to_string_lossy().into_owned();
    at(
        &mut graph,
        "/cores/isos/layers/process/groups/backend/nodes/simulator/settings/socket",
    )?
    .clone_from(&json!(socket));
    for group in ["camera", "base", "manipulator"] {
        let node = match group {
            "camera" => "front",
            "base" => "drive",
            _ => "arm",
        };
        let pointer = format!(
            "/cores/eos/layers/hardware/groups/{group}/nodes/{node}/adapter/settings/socket_path"
        );
        at(&mut graph, &pointer)?.clone_from(&json!(socket));
    }
    let graph_path = paths.v2_node_graph(name).to_string_lossy().into_owned();
    let eos_socket = paths
        .runtime
        .join(format!("{name}-eos.sock"))
        .to_string_lossy()
        .into_owned();
    let journal = paths
        .data
        .join("journal")
        .join(name)
        .to_string_lossy()
        .into_owned();
    let eos = "/cores/isos/layers/process/groups/component/nodes/eos/settings";
    at(&mut graph, &format!("{eos}/command"))?.clone_from(&json!([
        executable("eosd"),
        "--profile",
        graph_path,
        "--journal-dir",
        journal,
        "--socket",
        eos_socket
    ]));
    at(&mut graph, &format!("{eos}/ready_unix"))?.clone_from(&json!(eos_socket));
    at(
        &mut graph,
        "/cores/isos/layers/process/groups/component/nodes/bridge/settings/command",
    )?
    .clone_from(&json!([
        executable("eos-north-http"),
        "--socket",
        eos_socket,
        "--listen",
        "127.0.0.1:16201"
    ]));
    at(
        &mut graph,
        "/cores/isos/layers/process/groups/component/nodes/config/settings/command",
    )?
    .clone_from(&json!([
        executable("aether-config-server"),
        "--profile",
        graph_path,
        "--listen",
        "127.0.0.1:16202"
    ]));
    at(
        &mut graph,
        "/cores/isos/layers/process/groups/component/nodes/config/settings",
    )?
    .as_object_mut()
    .context("config settings must be an object")?
    .insert(
        "depends_on".into(),
        json!(["cores/isos/layers/process/groups/backend/nodes/simulator"]),
    );
    if let Some(first) = v2_backend::installed(paths).first()
        && let Some(version) = first.get("version")
    {
        at(
            &mut graph,
            "/cores/isos/layers/process/groups/backend/nodes/simulator/settings/version",
        )?
        .clone_from(version);
    }
    Ok(graph)
}

pub fn init(paths: &Paths, name: &str) -> Result<PathBuf> {
    let path = paths.v2_node_graph(name);
    if !path.exists() {
        let bytes = serde_json::to_vec_pretty(&draft(paths, name)?)?;
        crate::config::atomic_write(&path, &bytes)?;
    }
    Ok(path)
}

pub fn revision(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(hex::encode(Sha256::digest(fs::read(path)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_keeps_authored_graph_and_uses_private_runtime_socket() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        let path = init(&paths, "robot-a").unwrap();
        let graph: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let socket = graph
            .pointer("/cores/isos/layers/process/groups/backend/nodes/simulator/settings/socket")
            .and_then(Value::as_str)
            .unwrap();
        assert!(socket.starts_with(paths.runtime.to_str().unwrap()));
        assert_eq!(graph.pointer("/cores/eos/layers/hardware/groups/camera/nodes/front/adapter/settings/socket_path").and_then(Value::as_str), Some(socket));
        assert_eq!(
            graph
                .pointer(
                    "/cores/isos/layers/process/groups/component/nodes/config/settings/depends_on/0"
                )
                .and_then(Value::as_str),
            Some("cores/isos/layers/process/groups/backend/nodes/simulator")
        );
        fs::write(&path, "authored").unwrap();
        assert_eq!(init(&paths, "robot-a").unwrap(), path);
        assert_eq!(fs::read_to_string(path).unwrap(), "authored");
    }
}
