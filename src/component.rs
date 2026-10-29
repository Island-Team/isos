//! Calls isolated EOS/EGIA configuration adapters; never imports their runtimes.
use crate::{
    config::{self, Paths},
    graph::Deployment,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
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

fn set_path(root: &mut Value, path: &str, value: Value) -> Result<()> {
    let mut cursor = root;
    let mut parts = path.split('.').peekable();
    while let Some(part) = parts.next() {
        ensure!(!part.is_empty(), "empty settings field path");
        let object = cursor
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("non-object settings path {path}"))?;
        if parts.peek().is_none() {
            object.insert(part.to_owned(), value);
            return Ok(());
        }
        cursor = object.entry(part).or_insert_with(|| json!({}));
    }
    Ok(())
}

fn normalized_fields(profile: &Value) -> Vec<Value> {
    let source = profile
        .get("settings_fields")
        .or_else(|| profile.get("fields"));
    let entries: Vec<(String, &Value)> = match source {
        Some(Value::Array(fields)) => fields
            .iter()
            .filter_map(|field| {
                field
                    .get("path")
                    .or_else(|| field.get("name"))?
                    .as_str()
                    .map(|name| (name.to_owned(), field))
            })
            .collect(),
        Some(Value::Object(fields)) => fields
            .iter()
            .map(|(name, field)| (name.clone(), field))
            .collect(),
        _ => Vec::new(),
    };
    entries
        .into_iter()
        .map(|(path, field)| {
            let mut output = Map::new();
            output.insert("path".into(), json!(path));
            output.insert(
                "type".into(),
                field.get("type").cloned().unwrap_or(json!("string")),
            );
            output.insert(
                "label".into(),
                field.get("label").cloned().unwrap_or(json!(path)),
            );
            for key in [
                "description",
                "required",
                "default",
                "min",
                "max",
                "options",
                "read_only",
            ] {
                if let Some(value) = field.get(key) {
                    output.insert(key.into(), value.clone());
                }
            }
            if !output.contains_key("options")
                && let Some(value) = field.get("values")
            {
                output.insert("options".into(), value.clone());
            }
            Value::Object(output)
        })
        .collect()
}

pub fn editor_profiles(role: &str, data: &Value, adapter_defaults: &Value) -> Vec<Value> {
    let Some(items) = data.get("profiles").unwrap_or(data).as_array() else {
        return Vec::new();
    };
    items.iter().map(|profile| {
        let mut fields = normalized_fields(profile);
        let mut defaults = json!({});
        for field in &mut fields {
            let path = field["path"].as_str().unwrap_or("").to_owned();
            let default = path.split('.').try_fold(adapter_defaults, |value, key| value.get(key)).cloned()
                .or_else(|| field.get("default").cloned());
            if let Some(default) = default {
                let _ = set_path(&mut defaults, &path, default.clone());
                field["default"] = default;
            }
            if (role == "eos" && profile["id"] == "eos.native" && path == "grpc_uds_path")
                || (role == "egia" && path.starts_with("gateway.")) {
                field["read_only"] = json!(true);
            }
        }
        if role == "eos" && profile["id"] == "eos.native" {
            defaults.as_object_mut().unwrap().remove("grpc_uds_path");
        }
        if role == "egia" {
            defaults.as_object_mut().unwrap().remove("gateway");
        }
        if role == "eos" && profile["id"] == "eos.native" {
            let host = defaults["host"].as_str().unwrap_or("127.0.0.1");
            let port = defaults["north_http_port"].as_u64().unwrap_or(16201);
            let url = format!("http://{}", listener_address(host, port));
            defaults["north_http_url"] = json!(url);
            fields.push(json!({"path":"north_http_url","type":"string","label":"North HTTP URL","description":"Derived from the HTTP listener host and port.","default":url,"read_only":true}));
        }
        if role == "eos" && profile["id"] == "eos.uds" {
            // The endpoint owns the UDS path; EOS process configuration derives it.
            defaults.as_object_mut().unwrap().remove("grpc_uds_path");
        }
        let ports = |key: &str| -> Vec<Value> {
            profile.get(key).and_then(Value::as_array).into_iter().flatten().map(|port| {
                let contract = port.get("contracts").and_then(Value::as_array).and_then(|a|a.first()).and_then(|c|c.get("name")).and_then(Value::as_str);
                json!({"name":port.get("name"),"type":port.get("type"),"contract":contract})
            }).collect()
        };
        json!({"id":profile.get("id"),"version":profile.get("version"),"label":profile.get("label").or_else(||profile.get("id")),"description":profile.get("description"),"template":profile.get("template"),"parent_profile":profile.get("parent_profile"),"readiness":profile.get("readiness"),"inputs":ports("inputs"),"outputs":ports("outputs"),"settings_fields":fields,"defaults":defaults})
    }).collect()
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
fn merge_settings(target: &mut Value, edits: &Map<String, Value>, prefix: &str) -> Result<()> {
    for (key, edit) in edits {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        let slot = target
            .get_mut(key)
            .ok_or_else(|| anyhow::anyhow!("unsupported setting {path}"))?;
        if let (Some(object), Some(nested)) = (slot.as_object_mut(), edit.as_object()) {
            let mut nested_target = Value::Object(std::mem::take(object));
            merge_settings(&mut nested_target, nested, &path)?;
            *slot = nested_target;
        } else {
            *slot = edit.clone();
        }
    }
    Ok(())
}

pub fn listener_address(host: &str, port: u64) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn component_values(role: &str, node: &crate::graph::Node, socket: &str) -> Result<Value> {
    let mut values = call(role, "defaults", None)?;
    let mut edits = Map::from_iter(
        node.settings
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    if role == "eos" {
        ensure!(
            !edits.contains_key("grpc_uds_path"),
            "{}: settings.grpc_uds_path is derived from the UDS endpoint",
            node.id
        );
        edits.remove("north_http_url");
        merge_settings(&mut values, &edits, "").map_err(|error| {
            anyhow::anyhow!(
                "{}: settings.{}",
                node.id,
                error.to_string().trim_start_matches("unsupported setting ")
            )
        })?;
        values["grpc_uds_path"] = json!(socket);
    } else {
        ensure!(
            !edits.contains_key("gateway"),
            "{}: settings.gateway is derived from the EOS UDS endpoint",
            node.id
        );
        merge_settings(&mut values, &edits, "").map_err(|error| {
            anyhow::anyhow!(
                "{}: settings.{}",
                node.id,
                error.to_string().trim_start_matches("unsupported setting ")
            )
        })?;
        values["gateway"] = json!({"transport":"uds","path":socket});
    }
    call(role, "validate", Some(values.clone()))
        .map_err(|error| anyhow::anyhow!("{}: settings: {error}", node.id))?;
    Ok(values)
}

fn listener(node: &crate::graph::Node, values: &Value, role: &str) -> Result<()> {
    if !node.enabled {
        return Ok(());
    }
    let (host, port) = if role == "eos" {
        (values["host"].as_str(), values["grpc_port"].as_u64())
    } else {
        (
            values["server"]["host"].as_str(),
            values["server"]["port"].as_u64(),
        )
    };
    let expected = listener_address(
        host.ok_or_else(|| anyhow::anyhow!("{}: listener host missing", node.id))?,
        port.ok_or_else(|| anyhow::anyhow!("{}: listener port missing", node.id))?,
    );
    ensure!(
        node.launch
            .as_ref()
            .and_then(|launch| launch.ready_tcp.as_deref())
            == Some(expected.as_str()),
        "{}: launch.ready_tcp must match configured listener {expected}",
        node.id
    );
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
        let values = component_values(role, node, &socket)?;
        if role == "egia"
            && let Some(root) = source_root(paths, "egia")
        {
            copy_tree(&root.join("etc/egia"), &staging.join("egia"))?;
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
            let ready = listener_address(
                values["host"].as_str().unwrap_or("127.0.0.1"),
                values["grpc_port"].as_u64().unwrap_or(16200),
            );
            let stack = format!(
                "version = 1\neos_config = {}\nmodules = []\n\n[[service]]\nname = \"eosd\"\nrole = \"managed\"\ncommand = [{}]\nready_tcp = {}\n\n[[binding]]\nbackend = {}\nprofile = {}\n",
                json!(
                    paths
                        .generated(&graph.name)
                        .join("eos/eos.toml")
                        .to_string_lossy()
                        .to_string()
                ),
                json!(executable),
                json!(ready),
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
    let socket = graph
        .nodes
        .iter()
        .find(|node| {
            node.profile
                .as_ref()
                .is_some_and(|profile| profile.id == "eos.uds")
        })
        .and_then(|node| node.settings.get("socket_path"))
        .and_then(Value::as_str)
        .unwrap_or("");
    for node in &graph.nodes {
        if let Some(role @ ("eos" | "egia")) = node.role.as_deref() {
            let values = component_values(role, node, socket)?;
            listener(node, &values, role)?;
            if role == "eos" && node.enabled {
                let host = values["host"].as_str().unwrap_or("127.0.0.1");
                let port = values["north_http_port"].as_u64().unwrap_or(16201);
                let expected = format!("http://{}", listener_address(host, port));
                ensure!(
                    node.settings.get("north_http_url").and_then(Value::as_str)
                        == Some(expected.as_str()),
                    "{}: settings.north_http_url must match configured North HTTP listener {expected}",
                    node.id
                );
                for sidecar in graph.nodes.iter().filter(|candidate| {
                    candidate.settings.get("sidecar_of").and_then(Value::as_str) == Some("eos")
                }) {
                    let ready = listener_address(host, port);
                    ensure!(
                        sidecar
                            .launch
                            .as_ref()
                            .and_then(|launch| launch.ready_tcp.as_deref())
                            == Some(ready.as_str()),
                        "{}: launch.ready_tcp must match configured North HTTP listener {ready}",
                        sidecar.id
                    );
                }
            }
        }
        if node
            .profile
            .as_ref()
            .is_some_and(|profile| profile.id == "eos.uds")
        {
            for key in node.settings.keys() {
                ensure!(
                    key == "socket_path",
                    "{}: unsupported setting {key}",
                    node.id
                );
            }
            if let Some(path) = node.settings.get("socket_path") {
                let mut values = call("eos", "defaults", None)?;
                values.as_object_mut().unwrap().remove("grpc_uds_path");
                values["socket_path"] = path.clone();
                call("eos", "validate", Some(values))
                    .with_context(|| format!("{}: settings.socket_path", node.id))?;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_preserve_order_and_nested_defaults() {
        let source = json!({"profiles":[{"id":"egia.agent","version":"1.0.0","settings_fields":[
            {"name":"server.port","type":"integer","label":"Listener port","min":1,"max":65535},
            {"name":"gateway.path","type":"absolute_path","label":"Gateway path"}
        ]}]});
        let defaults = json!({"server":{"port":16100},"gateway":{"path":"/tmp/eos.sock"}});
        let profiles = editor_profiles("egia", &source, &defaults);
        assert_eq!(profiles[0]["settings_fields"][0]["path"], "server.port");
        assert_eq!(profiles[0]["settings_fields"][0]["min"], 1);
        assert_eq!(profiles[0]["settings_fields"][1]["read_only"], true);
        assert_eq!(profiles[0]["defaults"], json!({"server":{"port":16100}}));
    }

    #[test]
    fn profiles_normalize_eos_fields_and_derived_url() {
        let source = json!({"profiles":[{"id":"eos.native","version":"1.0.0","fields":[
            {"name":"host","type":"string","default":"127.0.0.1"},
            {"name":"north_http_port","type":"integer","default":16201},
            {"name":"grpc_uds_path","type":"absolute_path","default":""}
        ]}]});
        let profiles = editor_profiles(
            "eos",
            &source,
            &json!({"host":"127.0.0.1","north_http_port":16201,"grpc_uds_path":""}),
        );
        assert_eq!(
            profiles[0]["defaults"]["north_http_url"],
            "http://127.0.0.1:16201"
        );
        assert!(profiles[0]["defaults"].get("grpc_uds_path").is_none());
        assert_eq!(profiles[0]["settings_fields"][2]["read_only"], true);
    }

    #[test]
    fn settings_merge_rejects_ignored_nested_values() {
        let mut defaults = json!({"server":{"port":16100}});
        merge_settings(
            &mut defaults,
            &serde_json::from_value(json!({"server":{"port":17000}})).unwrap(),
            "",
        )
        .unwrap();
        assert_eq!(defaults["server"]["port"], 17000);
        let error = merge_settings(
            &mut defaults,
            &serde_json::from_value(json!({"server":{"typo":1}})).unwrap(),
            "",
        )
        .unwrap_err();
        assert!(error.to_string().contains("server.typo"));
    }

    #[test]
    fn listener_addresses_bracket_ipv6() {
        assert_eq!(listener_address("127.0.0.1", 16200), "127.0.0.1:16200");
        assert_eq!(listener_address("::1", 16200), "[::1]:16200");
    }
}
