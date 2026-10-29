//! Explicit local build candidates, selection, and digest checks.
use crate::{
    component,
    config::{self, Paths},
    graph::Deployment,
    isosui,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub schema_version: u32,
    pub component: String,
    pub product_version: String,
    pub source_commit: String,
    pub artifact: PathBuf,
    #[serde(default)]
    pub config_binary: Option<PathBuf>,
    #[serde(default)]
    pub source_root: Option<PathBuf>,
    pub sha256: String,
    pub descriptor: Value,
    #[serde(default)]
    pub profiles: Value,
}
fn repo(role: &str) -> Result<PathBuf> {
    let key = if role == "eos" {
        "EOS_REPO_PATH"
    } else {
        "EGIA_REPO_PATH"
    };
    let path = std::env::var_os(key)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("{key} is required"))?;
    ensure!(path.is_dir(), "{key} is not a directory");
    Ok(path.canonicalize()?)
}
fn command_output(tool: &str, args: &[&str], dir: &Path) -> Result<String> {
    let output = Command::new(tool)
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("run {tool}"))?;
    ensure!(
        output.status.success(),
        "{tool}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn sha(path: &Path) -> Result<String> {
    Ok(hex::encode(Sha256::digest(fs::read(path)?)))
}
fn receipt_path(paths: &Paths, role: &str, commit: &str) -> PathBuf {
    paths
        .data
        .join("builds")
        .join(role)
        .join(format!("{commit}.json"))
}
fn pin_path(paths: &Paths, role: &str) -> PathBuf {
    paths.config.join("pins").join(format!("{role}.json"))
}
pub fn build(paths: &Paths, role: &str, install_deps: bool) -> Result<Receipt> {
    ensure!(
        matches!(role, "eos" | "egia"),
        "build target must be eos or egia"
    );
    let root = repo(role)?;
    let commit = command_output("git", &["rev-parse", "HEAD"], &root)?;
    ensure!(
        command_output("git", &["status", "--porcelain"], &root)?.is_empty(),
        "{role} source has uncommitted changes; commit first"
    );
    let script = if role == "eos" {
        "build.sh"
    } else {
        "egia-build.sh"
    };
    let mut command = Command::new(root.join(script));
    command.current_dir(&root);
    if install_deps {
        ensure!(role == "eos", "--install-deps applies to EOS only");
        command.arg("--install-grpc");
    }
    let status = command.status()?;
    ensure!(status.success(), "{role} build failed");
    let artifact = if role == "eos" {
        let build_dir = command_output(
            root.join(script).to_str().unwrap(),
            &["--print-build-dir"],
            &root,
        )?;
        PathBuf::from(build_dir).join("src/eos/bin/eosd")
    } else {
        root.join("bin/egia")
    };
    ensure!(
        artifact.is_file(),
        "build artifact missing: {}",
        artifact.display()
    );
    let descriptor = component::call(role, "describe", None)?;
    let profiles = component::call(role, "profiles", None)?;
    let version = descriptor
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("0.0.0")
        .to_owned();
    let receipt = Receipt {
        schema_version: 1,
        component: role.into(),
        product_version: version,
        source_commit: commit.clone(),
        sha256: sha(&artifact)?,
        artifact,
        config_binary: Some(PathBuf::from(component::binary(role))),
        source_root: Some(root.clone()),
        descriptor,
        profiles,
    };
    config::atomic_write(
        &receipt_path(paths, role, &commit),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}
fn candidates(paths: &Paths, role: &str) -> Result<Vec<Receipt>> {
    let dir = paths.data.join("builds").join(role);
    if !dir.exists() {
        return Ok(vec![]);
    }
    fs::read_dir(dir)?
        .map(|entry| {
            let path = entry?.path();
            Ok(serde_json::from_slice(&fs::read(path)?)?)
        })
        .collect()
}
fn read_pin(paths: &Paths, role: &str) -> Result<Option<Receipt>> {
    let path = pin_path(paths, role);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}
pub fn pin(paths: &Paths, role: &str, version: &str, name: &str) -> Result<Receipt> {
    ensure!(
        matches!(role, "eos" | "egia"),
        "pin target must be eos or egia"
    );
    let matches: Vec<_> = candidates(paths, role)?
        .into_iter()
        .filter(|r| {
            r.product_version == version
                || r.source_commit == version
                || r.source_commit.starts_with(version)
        })
        .collect();
    ensure!(
        matches.len() == 1,
        "expected one local {role} build matching {version}; found {}",
        matches.len()
    );
    let receipt = matches.into_iter().next().unwrap();
    verify(&receipt)?;
    let path = paths.graph(name);
    let mut graph: Deployment = serde_json::from_slice(&fs::read(&path)?)?;
    ensure!(
        graph.schema_version == 2,
        "migrate graph to v2 before pinning"
    );
    let node = graph
        .nodes
        .iter_mut()
        .find(|n| n.role.as_deref() == Some(role))
        .ok_or_else(|| anyhow::anyhow!("graph has no {role} node"))?;
    let command = if role == "eos" {
        let root = receipt
            .source_root
            .clone()
            .or_else(|| repo("eos").ok())
            .ok_or_else(|| anyhow::anyhow!("EOS source root missing from receipt"))?;
        vec![
            receipt.artifact.to_string_lossy().into_owned(),
            "--etc".into(),
            paths
                .generated(name)
                .join("eos")
                .to_string_lossy()
                .into_owned(),
            "--contracts".into(),
            root.join("share/eos").to_string_lossy().into_owned(),
            "--config".into(),
            paths
                .generated(name)
                .join("eos/eos.toml")
                .to_string_lossy()
                .into_owned(),
        ]
    } else {
        vec![
            receipt.artifact.to_string_lossy().into_owned(),
            "--etc".into(),
            paths
                .generated(name)
                .join("egia")
                .to_string_lossy()
                .into_owned(),
            "--managed".into(),
            "--no-tui".into(),
        ]
    };
    let mut env = std::collections::BTreeMap::new();
    for (key, path) in [
        ("ISLAND_LOG", paths.data.join("log")),
        ("ISLAND_STATE", paths.data.join("state")),
        ("ISLAND_CACHE", paths.data.join("cache")),
        ("ISLAND_RUN", paths.runtime.clone()),
        ("ISLAND_TMP", paths.data.join("tmp")),
    ] {
        fs::create_dir_all(&path)?;
        env.insert(key.into(), path.to_string_lossy().into_owned());
    }
    if role == "eos" {
        env.insert(
            "EOS_STACK_CONFIG".into(),
            paths
                .generated(name)
                .join("eos-runtime/stack.toml")
                .to_string_lossy()
                .into_owned(),
        );
        if let Some(worker) = receipt
            .artifact
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("runtime/skills/worker/eos-skill-worker"))
            && worker.is_file()
        {
            env.insert(
                "EOS_SKILL_WORKER".into(),
                worker.to_string_lossy().into_owned(),
            );
        }
    }
    let sidecar_ready = crate::component::listener_address(
        node.settings
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or("127.0.0.1"),
        node.settings
            .get("north_http_port")
            .and_then(Value::as_u64)
            .unwrap_or(16201),
    );
    node.launch = Some(crate::graph::Launch {
        command,
        ready_tcp: Some(if role == "eos" {
            crate::component::listener_address(
                node.settings
                    .get("host")
                    .and_then(Value::as_str)
                    .unwrap_or("127.0.0.1"),
                node.settings
                    .get("grpc_port")
                    .and_then(Value::as_u64)
                    .unwrap_or(16200),
            )
        } else {
            let server = node.settings.get("server");
            crate::component::listener_address(
                server
                    .and_then(|value| value.get("host"))
                    .and_then(Value::as_str)
                    .unwrap_or("127.0.0.1"),
                server
                    .and_then(|value| value.get("port"))
                    .and_then(Value::as_u64)
                    .unwrap_or(16100),
            )
        }),
        timeout_ms: 30_000,
        env,
    });
    if role == "eos" {
        let north = receipt
            .artifact
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("ext/north_http/eos-north-http"));
        if let Some(sidecar) = graph.nodes.iter_mut().find(|n| {
            n.settings.get("sidecar_of").and_then(Value::as_str) == Some("eos")
                && n.id == "eos-north-http"
        }) && let Some(north) = north.filter(|p| p.is_file())
        {
            sidecar.launch = Some(crate::graph::Launch {
                command: vec![
                    north.to_string_lossy().into_owned(),
                    "--config".into(),
                    paths
                        .generated(name)
                        .join("ext/north-http/config.toml")
                        .to_string_lossy()
                        .into_owned(),
                ],
                ready_tcp: Some(sidecar_ready),
                timeout_ms: 30_000,
                env: Default::default(),
            });
        }
    }
    graph.validate()?;
    config::apply(paths, &graph)?;
    config::atomic_write(
        &pin_path(paths, role),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}
fn verify(receipt: &Receipt) -> Result<()> {
    ensure!(receipt.schema_version == 1, "unknown build receipt schema");
    ensure!(
        receipt.artifact.is_file(),
        "{} artifact missing",
        receipt.component
    );
    ensure!(
        sha(&receipt.artifact)? == receipt.sha256,
        "{} artifact digest changed",
        receipt.component
    );
    Ok(())
}
pub fn check(paths: &Paths, graph: Option<&Deployment>) -> Result<Value> {
    let own_pin = paths.config.join("isos-version");
    if own_pin.exists() {
        ensure!(
            fs::read_to_string(&own_pin)?.trim() == env!("CARGO_PKG_VERSION"),
            "IsOS binary version differs from initialized version; run install.sh explicitly, then reinitialize"
        );
    }
    let mut components = serde_json::Map::new();
    let mut selected = std::collections::BTreeMap::new();
    for role in ["eos", "egia"] {
        let available: Vec<Value> = candidates(paths, role)?
            .into_iter()
            .map(|r| serde_json::json!({"version":r.product_version,"commit":r.source_commit}))
            .collect();
        if let Some(receipt) = read_pin(paths, role)? {
            verify(&receipt)?;
            let live = component::call(role, "describe", None).ok();
            ensure!(
                live.as_ref() == Some(&receipt.descriptor),
                "{role} descriptor changed after build"
            );
            if !receipt.profiles.is_null() {
                ensure!(
                    component::call(role, "profiles", None).ok().as_ref()
                        == Some(&receipt.profiles),
                    "{role} profiles changed after build"
                );
            }
            components.insert(role.into(),serde_json::json!({"status":"pinned","version":receipt.product_version,"commit":receipt.source_commit,"sha256":receipt.sha256,"available_local_builds":available}));
            selected.insert(role, receipt.descriptor);
        } else {
            if graph.is_some_and(|g| {
                g.nodes
                    .iter()
                    .any(|n| n.enabled && n.role.as_deref() == Some(role))
            }) {
                bail!("enabled {role} has no pinned version");
            }
            components.insert(
                role.into(),
                serde_json::json!({"status":"unpinned","available_local_builds":available}),
            );
        }
    }
    if let (Some(eos), Some(egia)) = (selected.get("eos"), selected.get("egia")) {
        let provides = eos["provides"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("EOS descriptor lacks provides"))?;
        for required in egia["requires"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("EGIA descriptor lacks requires"))?
        {
            ensure!(
                provides
                    .iter()
                    .any(|p| p["name"] == required["name"] && p["version"] == required["version"]),
                "EGIA requires unsupported EOS contract {} {}",
                required["name"],
                required["version"]
            );
        }
    }
    Ok(
        serde_json::json!({"isos":env!("CARGO_PKG_VERSION"),"components":components,"isosui":isosui::check(paths)?}),
    )
}
