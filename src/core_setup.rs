//! Bounded, on-demand setup adapters for the saved v2 graph.
use crate::{
    config::{self, Paths},
    packages, v2_build,
};
use anyhow::{Context, Result, bail, ensure};
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

fn revision(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn selected_adapter(paths: &Paths, role: &str) -> Result<PathBuf> {
    match role {
        "eos" => {
            if let Some(receipt) = v2_build::selected_verified(paths)? {
                return Ok(receipt.binaries["aether-profile"].clone());
            }
            crate::v2_bins::resolve("aether-profile")
        }
        "egia" => {
            let pin = paths.config.join("pins/egia.json");
            if pin.is_file() {
                let value: Value = serde_json::from_slice(&fs::read(&pin)?)?;
                let artifact = value["artifact"]
                    .as_str()
                    .context("EGIA pin lacks artifact")?;
                let expected = value["sha256"].as_str().context("EGIA pin lacks digest")?;
                ensure!(
                    revision(&fs::read(artifact)?) == expected,
                    "selected EGIA artifact digest changed"
                );
                if let Some(path) = value["config_binary"].as_str() {
                    let path = PathBuf::from(path);
                    ensure!(path.is_file(), "selected EGIA setup adapter is missing");
                    return Ok(path);
                }
            }
            Ok(PathBuf::from(crate::component::binary("egia")))
        }
        _ => bail!("unknown setup core {role}"),
    }
}

fn package_refs(paths: &Paths, graph: &Value) -> Result<Vec<Value>> {
    fn collect(value: &Value, found: &mut BTreeSet<(String, String)>) {
        if let Some(package) = value.get("package")
            && let (Some(id), Some(version)) = (package["id"].as_str(), package["version"].as_str())
        {
            found.insert((id.into(), version.into()));
        }
        match value {
            Value::Object(object) => {
                for value in object.values() {
                    collect(value, found);
                }
            }
            Value::Array(items) => {
                for value in items {
                    collect(value, found);
                }
            }
            _ => (),
        }
    }
    let mut found = BTreeSet::new();
    collect(graph, &mut found);
    found
        .into_iter()
        .map(|(id, version)| {
            let path = packages::package_path(paths, &id, &version)?;
            let receipt: Value =
                serde_json::from_slice(&fs::read(path.join("receipt.json")).with_context(
                    || format!("package {id}@{version} is not installed or linked"),
                )?)?;
            ensure!(
                receipt["id"] == id && receipt["version"] == version,
                "package {id}@{version} receipt identity differs"
            );
            Ok(json!({"id":id,"version":version,"path":path}))
        })
        .collect()
}

fn command_arg(graph: &Value, node: &str, flag: &str) -> Option<String> {
    let pointer =
        format!("/cores/isos/layers/process/groups/component/nodes/{node}/settings/command");
    let args = graph.pointer(&pointer)?.as_array()?;
    args.windows(2)
        .find(|pair| pair[0].as_str() == Some(flag))?
        .get(1)?
        .as_str()
        .map(str::to_owned)
}

fn listener(value: Option<String>, default: u16) -> Value {
    let (host, port) = value
        .as_deref()
        .and_then(|value| value.rsplit_once(':'))
        .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host.to_owned(), port)))
        .unwrap_or_else(|| ("127.0.0.1".into(), default));
    json!({"host":host,"port":port})
}

fn endpoints(paths: &Paths, name: &str, graph: &Value) -> Value {
    let api_port = crate::http::local_port().unwrap_or(16000);
    let eos_socket = command_arg(graph, "eos", "--socket")
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.runtime.join(format!("{name}-eos.sock")));
    json!({
        "isos_api":{"url":format!("http://127.0.0.1:{api_port}")},
        "eos_control":listener(command_arg(graph,"bridge","--listen"),16020),
        "eos_catalog":listener(command_arg(graph,"config","--listen"),16021),
        "egia_northbound":listener(command_arg(graph,"egia","--listen"),16060),
        "eos_socket":{"path":eos_socket}
    })
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
    let request = json!({"schema_version":2,"op":op,"core":role,"revision":graph_revision,
        "graph":graph,"endpoints":endpoints(paths,name,graph),"packages":package_refs(paths, graph)?});
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
    let _ = write_task.join();
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
    ensure!(
        reply["schema_version"] == 2,
        "setup adapter schema mismatch"
    );
    ensure!(reply["ok"].is_boolean(), "setup adapter response lacks ok");
    ensure!(
        reply["diagnostics"].is_array(),
        "setup adapter response lacks diagnostics"
    );
    ensure!(
        reply["files"].is_array(),
        "setup adapter response lacks files"
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
    let request = request(paths, name, role, op, graph, graph_revision)?;
    invoke(&selected_adapter(paths, role)?, &request)
}

pub fn prepare_run(paths: &Paths, name: &str, expected_revision: &str) -> Result<()> {
    let graph_path = paths.v2_node_graph(name);
    let bytes = fs::read(&graph_path).with_context(|| format!("read {}", graph_path.display()))?;
    ensure!(bytes.len() <= MAX_REQUEST, "v2 graph exceeds 1 MiB");
    ensure!(
        revision(&bytes) == expected_revision,
        "node graph revision changed during setup"
    );
    let graph: Value = serde_json::from_slice(&bytes)?;
    let egia_process = graph
        .pointer("/cores/isos/layers/process/groups/component/nodes/egia")
        .is_some_and(|node| node["enabled"] != false);
    let roles = ["eos", "egia"].into_iter().filter(|role| {
        *role == "eos" && graph["cores"].get("eos").is_some() || *role == "egia" && egia_process
    });
    let mut files = Vec::new();
    for role in roles {
        let reply = match call(paths, name, role, "render", &graph, expected_revision) {
            Ok(reply) => reply,
            Err(error)
                if role == "eos" && {
                    let message = error.to_string();
                    message.contains("unrecognized subcommand")
                        || message.contains("unexpected argument 'setup'")
                        || message.contains("unknown command 'setup'")
                } =>
            {
                // Older verified EOS v2 binaries still use the authored graph directly.
                crate::v2_process::validate_value(paths, &graph)?;
                continue;
            }
            Err(error) => return Err(error),
        };
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
            files.push(safe_file(role, entry)?);
        }
    }
    let _lock = crate::lifecycle::Lock::take(paths)?;
    ensure!(
        revision(&fs::read(&graph_path)?) == expected_revision,
        "node graph revision changed during setup"
    );
    let root = paths.generated(name).join("core-setup");
    for (relative, content) in files {
        config::atomic_write(&root.join(relative), &content)?;
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
        fs::write(&script, "#!/bin/sh\ncat >/dev/null\nprintf '{\"schema_version\":2,\"ok\":true,\"diagnostics\":[],\"files\":[]}'\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let reply = invoke(&script, &json!({"schema_version":2})).unwrap();
        assert_eq!(reply["ok"], true);
        fs::write(&script, "#!/bin/sh\ncat >/dev/null\nprintf '{\"schema_version\":1,\"ok\":true,\"diagnostics\":[],\"files\":[]}'\n").unwrap();
        assert!(invoke(&script, &json!({"schema_version":2})).is_err());
    }

    #[test]
    fn changed_graph_revision_aborts_without_output() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths(temp.path());
        paths.prepare().unwrap();
        config::atomic_write(
            &paths.v2_node_graph("default"),
            br#"{"version":"2.0.0","robot":{},"cores":{}}"#,
        )
        .unwrap();
        assert!(prepare_run(&paths, "default", "stale").is_err());
        assert!(!paths.generated("default").exists());
        let revision = crate::v2_starter::revision(&paths.v2_node_graph("default")).unwrap();
        prepare_run(&paths, "default", &revision).unwrap();
    }
}
