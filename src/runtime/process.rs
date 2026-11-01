//! IsOS-owned process projection of the shared v1 node graph.
//!
//! The graph remains the authored source. This module admits only registered
//! IsOS process nodes and keeps direct child handles in the manager process.

use crate::{
    backend, config::Paths, core_bins, edge_admission, graph_compat, graph_migration, log_store,
    mujoco_catalog, packages,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    net::{TcpStream, ToSocketAddrs},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const MAX_GRAPH_BYTES: u64 = 1024 * 1024;
const MAX_COMMAND_ARGS: usize = 64;
const MAX_RESTARTS: u32 = 10;

#[derive(Debug)]
pub(crate) struct PackageLoadFailure {
    pub id: String,
    pub version: String,
    pub stage: &'static str,
    pub node_path: Option<String>,
    pub source: anyhow::Error,
}

impl std::fmt::Display for PackageLoadFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Reinstall or relink {}@{}",
            self.id, self.version
        )
    }
}

impl std::error::Error for PackageLoadFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

fn canonical_node_path(path: &str) -> String {
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() == 8
        && parts[0] == "cores"
        && parts[2] == "layers"
        && parts[4] == "groups"
        && parts[6] == "nodes"
    {
        format!("{}/{}/{}/{}", parts[1], parts[3], parts[5], parts[7])
    } else {
        path.to_owned()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Graph {
    version: String,
    #[allow(dead_code)]
    robot: Value,
    cores: BTreeMap<String, Core>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Core {
    layers: BTreeMap<String, Layer>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Layer {
    groups: BTreeMap<String, Group>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Group {
    nodes: BTreeMap<String, GraphNode>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphNode {
    id: String,
    node_type: String,
    #[serde(default)]
    settings: Value,
    #[serde(default)]
    references: Vec<String>,
    #[serde(default)]
    adapter: Option<Value>,
    #[serde(default)]
    package: Option<backend::PackageIdentity>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackendSettings {
    scene: String,
    socket: PathBuf,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default = "default_start_timeout_ms")]
    start_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessSettings {
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub env_from: BTreeMap<String, String>,
    #[serde(default)]
    pub ready_tcp: Option<String>,
    #[serde(default)]
    pub ready_unix: Option<PathBuf>,
    #[serde(default = "default_start_timeout_ms")]
    pub start_timeout_ms: u64,
    #[serde(default)]
    pub restart: Restart,
    #[serde(default)]
    pub max_restarts: u32,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

fn default_start_timeout_ms() -> u64 {
    30_000
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Restart {
    #[default]
    Never,
    OnFailure,
    Always,
}

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub id: String,
    pub path: String,
    pub settings: ProcessSettings,
    pub package: Option<backend::PackageIdentity>,
    pub fingerprint: Option<String>,
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

pub fn validate_deployment_name(name: &str) -> Result<()> {
    ensure!(valid_segment(name), "invalid deployment name");
    Ok(())
}

impl ProcessSpec {
    fn validate(&self) -> Result<()> {
        ensure!(
            valid_segment(&self.id),
            "invalid process node id {}",
            self.id
        );
        let s = &self.settings;
        ensure!(
            !s.command.is_empty()
                && s.command.len() <= MAX_COMMAND_ARGS
                && s.command
                    .iter()
                    .all(|arg| !arg.is_empty() && arg.len() <= 4096),
            "{}: command must have 1..={MAX_COMMAND_ARGS} bounded arguments",
            self.id
        );
        ensure!(
            Path::new(&s.command[0]).is_absolute(),
            "{}: command executable must be an absolute path",
            self.id
        );
        if let Some(cwd) = &s.cwd {
            ensure!(cwd.is_absolute(), "{}: cwd must be absolute", self.id);
        }
        ensure!(
            (1..=120_000).contains(&s.start_timeout_ms),
            "{}: start_timeout_ms must be 1..=120000",
            self.id
        );
        ensure!(
            s.max_restarts <= MAX_RESTARTS,
            "{}: max_restarts exceeds {MAX_RESTARTS}",
            self.id
        );
        if let Some(endpoint) = &s.ready_tcp {
            ensure!(
                endpoint.to_socket_addrs()?.next().is_some(),
                "{}: ready_tcp has no address",
                self.id
            );
        }
        ensure!(
            s.ready_tcp.is_none() || s.ready_unix.is_none(),
            "{}: choose only one of ready_tcp and ready_unix",
            self.id
        );
        if let Some(path) = &s.ready_unix {
            ensure!(
                path.is_absolute()
                    && !path.as_os_str().as_bytes().contains(&0)
                    && path.as_os_str().as_bytes().len() <= 107,
                "{}: ready_unix must be an absolute Unix socket path of at most 107 bytes without NUL",
                self.id
            );
        }
        for (key, value) in &s.env {
            ensure!(
                !key.is_empty()
                    && key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                    && !value.contains('\0'),
                "{}: invalid environment variable {key}",
                self.id
            );
        }
        ensure!(
            s.env_from.len() <= 32,
            "{}: too many env_from entries",
            self.id
        );
        for (target, source) in &s.env_from {
            ensure!(
                crate::generated_v2::valid_env(target)
                    && crate::generated_v2::valid_env(source)
                    && !s.env.contains_key(target),
                "{}: invalid or duplicate env_from target",
                self.id
            );
        }
        Ok(())
    }
}

/// Extract and order IsOS-owned process nodes from the single authored graph.
#[cfg(test)]
pub fn project(source: &str) -> Result<Vec<ProcessSpec>> {
    project_inner(None, source)
}

pub fn project_with_packages(paths: &Paths, source: &str) -> Result<Vec<ProcessSpec>> {
    let specs = project_inner(Some(paths), source)?;
    for spec in &specs {
        let executable = Path::new(&spec.settings.command[0]);
        let metadata = fs::metadata(executable).with_context(|| {
            format!(
                "{}: executable {} is missing",
                spec.id,
                executable.display()
            )
        })?;
        ensure!(
            metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
            "{}: executable {} is not runnable",
            spec.id,
            executable.display()
        );
    }
    Ok(specs)
}

pub fn use_generated_eos_profile(specs: &mut [ProcessSpec], path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute() && path.is_file(),
        "verified EOS generated profile is missing"
    );
    for id in ["eos", "config"] {
        let spec = specs
            .iter_mut()
            .find(|spec| spec.id == id)
            .with_context(|| format!("graph lacks {id} process"))?;
        let mut matches = spec
            .settings
            .command
            .iter()
            .enumerate()
            .filter(|(_, arg)| arg.as_str() == "--profile");
        let index = matches
            .next()
            .map(|(index, _)| index)
            .with_context(|| format!("{id} lacks --profile"))?;
        ensure!(
            matches.next().is_none() && index + 1 < spec.settings.command.len(),
            "{id} has invalid --profile arguments"
        );
        spec.settings.command[index + 1] = path.to_string_lossy().into_owned();
    }
    Ok(())
}

pub fn use_generated_egia_config(specs: &mut [ProcessSpec], root: &Path) -> Result<()> {
    let Some(spec) = specs.iter_mut().find(|spec| spec.id == "egia") else {
        return Ok(());
    };
    ensure!(
        root.is_absolute() && root.is_dir(),
        "selected EGIA configuration is missing"
    );
    ensure!(
        !spec.settings.command.iter().any(|arg| arg == "--etc")
            && !spec.settings.env.contains_key("EGIA_ETC"),
        "EGIA configuration must use the selected generated root"
    );
    let path = root.to_string_lossy().into_owned();
    spec.settings.command.extend(["--etc".into(), path.clone()]);
    spec.settings.env.insert("EGIA_ETC".into(), path);
    for flag in ["--managed", "--no-tui"] {
        if !spec.settings.command.iter().any(|arg| arg == flag) {
            spec.settings.command.push(flag.into());
        }
    }
    if !spec.settings.command.iter().any(|arg| arg == "--mode") {
        spec.settings
            .command
            .extend(["--mode".into(), "isos".into()]);
    }
    Ok(())
}

fn project_inner(paths: Option<&Paths>, source: &str) -> Result<Vec<ProcessSpec>> {
    ensure!(
        source.len() as u64 <= MAX_GRAPH_BYTES,
        "node graph exceeds 1 MiB"
    );
    let value = normalize_source(source)?;
    let sidecars = paths
        .map(|paths| packages::node_type_sidecars_for_graph(paths, &value))
        .transpose()?
        .unwrap_or_default();
    if let Some(paths) = paths {
        mujoco_catalog::validate_graph(paths, &value)?;
    }
    let graph: Graph = serde_json::from_value(value.clone())?;
    ensure!(
        matches!(graph.version.as_str(), "1.0.0" | "0.1.0"),
        "graph version must be 1.0.0 or 0.1.0"
    );
    let mut specs = BTreeMap::new();
    let mut backends = BTreeMap::new();
    for (core_id, core) in &graph.cores {
        for (layer_id, layer) in &core.layers {
            for (group_id, group) in &layer.groups {
                for (key, node) in &group.nodes {
                    if !matches!(
                        node.node_type.as_str(),
                        "eos.process.mujoco@v1.0" | "eos.process.mujoco@v0.1"
                    ) {
                        continue;
                    }
                    let path = format!("{core_id}/{layer_id}/{group_id}/{key}");
                    ensure!(
                        core_id == "eos"
                            && layer_id == "process"
                            && group_id == "mujoco"
                            && key == &node.id,
                        "{path}: backend has invalid placement or id"
                    );
                    let paths =
                        paths.context("package-backed backend requires IsOS package resolver")?;
                    let settings: BackendSettings =
                        serde_json::from_value(node.settings.clone())
                            .with_context(|| format!("{path}: invalid backend settings"))?;
                    let package = node
                        .package
                        .as_ref()
                        .context("backend requires package identity and exact version")?;
                    let (package_id, version) = (package.id.as_str(), package.version.as_str());
                    ensure!(
                        settings.socket.is_absolute()
                            && settings.socket.as_os_str().as_bytes().len() <= 107,
                        "{path}: socket must be an absolute Unix path at most 107 bytes"
                    );
                    ensure!(
                        settings.socket.starts_with(&paths.runtime),
                        "{path}: socket must be under the IsOS runtime directory"
                    );
                    ensure!(
                        (1..=120_000).contains(&settings.start_timeout_ms),
                        "{path}: start_timeout_ms must be 1..=120000"
                    );
                    let resolved = backend::resolve(
                        paths,
                        package_id,
                        version,
                        &settings.scene,
                        &settings.env,
                    )
                    .map_err(|source| PackageLoadFailure {
                        id: package_id.to_owned(),
                        version: version.to_owned(),
                        stage: "backend",
                        node_path: Some(format!(
                            "/cores/{core_id}/layers/{layer_id}/groups/{group_id}/nodes/{key}"
                        )),
                        source,
                    })?;
                    ensure!(
                        backends.insert(path, (resolved, settings.socket)).is_none(),
                        "duplicate backend path"
                    );
                }
            }
        }
    }
    let mut adapter_libraries = BTreeMap::new();
    if let Some(hardware) = graph
        .cores
        .get("eos")
        .and_then(|core| core.layers.get("hardware"))
    {
        for (group_id, group) in &hardware.groups {
            for (node_id, node) in &group.nodes {
                let Some(adapter) = &node.adapter else {
                    continue;
                };
                let Some(family) = adapter.get("type").and_then(Value::as_str) else {
                    continue;
                };
                if matches!(family, "mock-cxx" | "mujoco-v2") {
                    continue;
                }
                let package = adapter
                    .get("package")
                    .and_then(Value::as_object)
                    .and_then(|package| {
                        Some((
                            package.get("id")?.as_str()?,
                            package.get("version")?.as_str()?,
                        ))
                    })
                    .or_else(|| {
                        node.package
                            .as_ref()
                            .map(|package| (package.id.as_str(), package.version.as_str()))
                    })
                    .with_context(|| format!("{}: loadable adapter requires a package", node.id))?;
                let paths = paths.context("loadable adapter requires IsOS package resolver")?;
                let (library, digest) =
                    packages::verified_adapter_library(paths, package.0, package.1, family)
                        .map_err(|source| PackageLoadFailure {
                            id: package.0.to_owned(),
                            version: package.1.to_owned(),
                            stage: "adapter",
                            node_path: Some(format!(
                                "/cores/eos/layers/hardware/groups/{group_id}/nodes/{node_id}"
                            )),
                            source,
                        })?;
                let argument = format!("{family}={}@{digest}", library.display());
                if let Some(previous) =
                    adapter_libraries.insert(family.to_owned(), argument.clone())
                {
                    ensure!(
                        previous == argument,
                        "adapter family {family} resolves to conflicting libraries"
                    );
                }
            }
        }
    }
    for (core_id, core) in &graph.cores {
        if core_id != "eos" {
            continue;
        }
        if let Some(hardware) = core.layers.get("hardware") {
            for (group_id, group) in &hardware.groups {
                for (key, node) in &group.nodes {
                    let Some(adapter) = &node.adapter else {
                        continue;
                    };
                    if adapter.get("type").and_then(Value::as_str) != Some("mujoco-v2") {
                        continue;
                    }
                    let settings = adapter
                        .get("settings")
                        .context("MuJoCo adapter lacks settings")?;
                    let backend_path = settings
                        .get("backend")
                        .and_then(Value::as_str)
                        .context("MuJoCo adapter lacks backend reference")?;
                    let backend_path = canonical_node_path(backend_path);
                    let (resolved, socket) = backends.get(&backend_path).with_context(|| {
                        format!("MuJoCo adapter {key} references missing backend {backend_path}")
                    })?;
                    ensure!(
                        node.references
                            .iter()
                            .any(|r| canonical_node_path(r) == backend_path),
                        "MuJoCo adapter {key} must reference its backend node"
                    );
                    ensure!(
                        settings.get("socket_path").and_then(Value::as_str) == socket.to_str(),
                        "MuJoCo adapter {key} socket differs from backend"
                    );
                    let kind = if group_id == "mujoco" {
                        if node.node_type.ends_with("mujoco_camera@v1.0") {
                            "camera"
                        } else if node.node_type.ends_with("mujoco_base@v1.0") {
                            "base"
                        } else if node.node_type.ends_with("mujoco_manipulator@v1.0") {
                            "manipulator"
                        } else {
                            anyhow::bail!("unknown MuJoCo hardware type {}", node.node_type)
                        }
                    } else {
                        group_id.as_str()
                    };
                    let expected = resolved
                        .descriptor
                        .devices
                        .get(kind)
                        .with_context(|| format!("backend lacks {kind} device"))?;
                    ensure!(
                        settings.get("simulator_node").and_then(Value::as_str)
                            == Some(expected.as_str()),
                        "MuJoCo adapter {key} device differs from package"
                    );
                    ensure!(
                        settings
                            .get("device_id")
                            .and_then(Value::as_str)
                            .is_none_or(|device| device == expected),
                        "MuJoCo adapter {key} device_id differs from package"
                    );
                }
            }
        }
    }
    for (core_id, core) in graph.cores {
        for (layer_id, layer) in core.layers {
            for (group_id, group) in layer.groups {
                for (key, node) in group.nodes {
                    let path = format!("{core_id}/{layer_id}/{group_id}/{key}");
                    if matches!(
                        node.node_type.as_str(),
                        "eos.process.mujoco@v1.0" | "eos.process.mujoco@v0.1"
                    ) {
                        continue;
                    }
                    if matches!(
                        node.node_type.as_str(),
                        "isos.process.component@v1.1" | "isos.process.component@v0.1"
                    ) {
                        ensure!(
                            core_id == "isos" && layer_id == "process" && group_id == "component",
                            "{path}: process type has invalid placement"
                        );
                        ensure!(key == node.id, "{path}: node id differs from map key");
                        let mut settings: ProcessSettings =
                            serde_json::from_value(node.settings)
                                .with_context(|| format!("{path}: invalid process settings"))?;
                        ensure!(
                            node.node_type == "isos.process.component@v0.1"
                                || settings.env_from.is_empty(),
                            "{path}: env_from requires the @v0.1 component type"
                        );
                        let binary = match key.as_str() {
                            "eos" => "aether-runtime",
                            "bridge" => "aether-http-bridge",
                            "config" => "aether-config-server",
                            _ => "",
                        };
                        if !binary.is_empty() && paths.is_some() {
                            ensure!(
                                settings.role.as_deref().is_none_or(|role| role == "eos"),
                                "{path}: role must be eos"
                            );
                            ensure!(
                                settings.command.is_empty(),
                                "{path}: command is obsolete; use role and args"
                            );
                            let args = settings.args.clone();
                            settings.command = std::iter::once(
                                core_bins::resolve(binary)?.to_string_lossy().into_owned(),
                            )
                            .chain(args)
                            .collect();
                            settings.role = Some("eos".into());
                        } else if key == "egia" && paths.is_some() {
                            ensure!(
                                settings.role.as_deref().is_none_or(|role| role == "egia"),
                                "{path}: role must be egia"
                            );
                            ensure!(
                                settings.command.is_empty(),
                                "{path}: command is obsolete; use role and args"
                            );
                            let args = settings.args.clone();
                            let executable =
                                crate::core_setup::adapter_path(paths.unwrap(), "egia")?
                                    .with_file_name("egia");
                            ensure!(
                                executable.is_file(),
                                "EGIA executable missing at {}",
                                executable.display()
                            );
                            settings.command =
                                std::iter::once(executable.to_string_lossy().into_owned())
                                    .chain(args)
                                    .collect();
                            settings.role = Some("egia".into());
                            settings
                                .env
                                .extend(crate::generated_v2::egia_launch_env(&value)?);
                        }
                        for dependency in &mut settings.depends_on {
                            *dependency = canonical_node_path(dependency);
                        }
                        let mut spec = ProcessSpec {
                            id: key.clone(),
                            path,
                            settings,
                            package: None,
                            fingerprint: None,
                        };
                        if key == "eos" || key == "config" {
                            for sidecar in &sidecars {
                                spec.settings.command.extend([
                                    "--node-types".into(),
                                    sidecar.to_string_lossy().into_owned(),
                                ]);
                            }
                        }
                        if key == "eos" {
                            for argument in adapter_libraries.values() {
                                spec.settings
                                    .command
                                    .extend(["--adapter-library".into(), argument.clone()]);
                            }
                            ensure!(backends.len() <= 1, "EOS currently supports one backend");
                            for (backend_path, (resolved, _socket)) in &backends {
                                spec.settings.command.extend([
                                    "--backend-node".into(),
                                    backend_path.clone(),
                                    "--backend-executable".into(),
                                    resolved.executable.to_string_lossy().into_owned(),
                                    "--backend-scene".into(),
                                    resolved.scene.to_string_lossy().into_owned(),
                                ]);
                                spec.settings.env.extend(resolved.environment.clone());
                                spec.package = Some(resolved.descriptor.package.clone());
                                spec.fingerprint = Some(resolved.fingerprint.clone());
                            }
                        }
                        if paths.is_some()
                            && matches!(key.as_str(), "eos" | "bridge" | "config" | "egia")
                        {
                            let mut digest = Sha256::new();
                            digest.update(fs::read(&spec.settings.command[0])?);
                            if let Some(backend) = &spec.fingerprint {
                                digest.update(backend.as_bytes());
                            }
                            spec.fingerprint = Some(hex::encode(digest.finalize()));
                        }
                        spec.validate()?;
                        ensure!(
                            specs.insert(key.clone(), spec).is_none(),
                            "duplicate process id {key}"
                        );
                    } else if core_id == "isos" {
                        if node.node_type == "isos.ui.vision@v1.0"
                            && layer_id == "ui"
                            && group_id == "panel"
                            && key == node.id
                        {
                            continue;
                        }
                        bail!("{path}: unregistered IsOS node type {}", node.node_type);
                    }
                }
            }
        }
    }
    ensure!(
        backends.is_empty() || specs.contains_key("eos"),
        "EOS backend requires an IsOS-managed EOS runtime process"
    );
    validate_listener_ports(&specs)?;
    let mut ordered = Vec::with_capacity(specs.len());
    let mut pending: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (id, spec) in &specs {
        let mut deps = BTreeSet::new();
        for dep in &spec.settings.depends_on {
            let dep_id = dep.rsplit('/').next().unwrap_or(dep);
            let provider = specs
                .get(dep_id)
                .ok_or_else(|| anyhow::anyhow!("{} depends on unknown process {dep}", id))?;
            ensure!(
                dep == &provider.path,
                "{}: noncanonical process dependency {dep}",
                id
            );
            ensure!(id != dep_id, "{id} cannot depend on itself");
            ensure!(
                deps.insert(dep_id.to_owned()),
                "{id}: duplicate dependency {dep}"
            );
        }
        pending.insert(id.clone(), deps);
    }
    while !pending.is_empty() {
        let Some(next) = pending
            .iter()
            .find(|(_, deps)| deps.is_empty())
            .map(|(id, _)| id.clone())
        else {
            bail!("IsOS process dependencies contain a cycle")
        };
        ordered.push(specs.remove(&next).unwrap());
        pending.remove(&next);
        for deps in pending.values_mut() {
            deps.remove(&next);
        }
    }
    Ok(ordered)
}

/// Reject obsolete graph shapes before delegating admission to the core registry.
fn reject_obsolete_nodes(graph: &Value) -> Result<()> {
    if let Some(nodes) = graph
        .pointer("/cores/isos/layers/process/groups/component/nodes")
        .and_then(Value::as_object)
    {
        for (id, node) in nodes {
            ensure!(
                node["node_type"] != "isos.process.component@v1.0"
                    && node["settings"].get("command").is_none(),
                "{id}: obsolete executable command; set node_type to isos.process.component@v1.1 and use settings.role plus settings.args, then save the graph"
            );
        }
    }
    if let Some(cores) = graph.get("cores").and_then(Value::as_object) {
        for (core_id, core) in cores {
            if let Some(layers) = core.get("layers").and_then(Value::as_object) {
                for (layer_id, layer) in layers {
                    if let Some(groups) = layer.get("groups").and_then(Value::as_object) {
                        for (group_id, group) in groups {
                            if let Some(nodes) = group.get("nodes").and_then(Value::as_object) {
                                for (id, node) in nodes {
                                    ensure!(
                                        node["node_type"] != "eos.process.backend@v1.0",
                                        "obsolete MuJoCo backend at {core_id}/{layer_id}/{group_id}/{id}; add the installed package node set to a current graph and reconnect its references"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_listener_ports(specs: &BTreeMap<String, ProcessSpec>) -> Result<()> {
    let api = crate::http::local_port()?;
    let ui = crate::http::ui_port()?;
    let mut claimed = BTreeMap::<u16, &str>::new();
    for (id, spec) in specs {
        let mut endpoints = Vec::new();
        if let Some(ready) = &spec.settings.ready_tcp {
            endpoints.push(ready.as_str());
        }
        for pair in spec.settings.command.windows(2) {
            if matches!(id.as_str(), "eos" | "bridge" | "config" | "egia") && pair[0] == "--listen"
            {
                endpoints.push(&pair[1]);
            }
        }
        for endpoint in endpoints {
            for address in endpoint
                .to_socket_addrs()
                .with_context(|| format!("{id}: invalid listener address {endpoint}"))?
            {
                if !address.ip().is_loopback() {
                    continue;
                }
                let port = address.port();
                ensure!(
                    port != api && port != ui,
                    "{id}: listener port {port} conflicts with IsOS workbench"
                );
                if let Some(owner) = claimed.insert(port, id) {
                    ensure!(
                        owner == id,
                        "{id}: listener port {port} conflicts with {owner}"
                    );
                }
            }
        }
    }
    Ok(())
}

pub fn normalize_source(source: &str) -> Result<Value> {
    ensure!(
        source.len() as u64 <= MAX_GRAPH_BYTES,
        "node graph exceeds 1 MiB"
    );
    let graph = if source.trim_start().starts_with('{') {
        serde_json::from_str(source)?
    } else {
        let value: toml::Value = toml::from_str(source)?;
        serde_json::to_value(value)?
    };
    if let Some(diagnostic) = graph_compat::version_diagnostic(&graph) {
        bail!(diagnostic);
    }
    Ok(graph)
}

pub fn validate_value(paths: &Paths, graph: &Value) -> Result<()> {
    packages::verify_graph_packages(paths, graph)?;
    mujoco_catalog::validate_graph(paths, graph)?;
    graph_compat::admit_legacy_mapped(graph, catalog().ok().as_ref())?;
    validate_value_with_profile(paths, graph, &core_bins::resolve("aether-profile")?)
}

pub fn validate_value_with_profile(paths: &Paths, graph: &Value, profile: &Path) -> Result<()> {
    packages::verify_graph_packages(paths, graph)?;
    mujoco_catalog::validate_graph(paths, graph)?;
    graph_compat::admit_legacy_mapped(graph, catalog().ok().as_ref())?;
    reject_obsolete_nodes(graph)?;
    let eos_graph = if graph["version"] == "0.1.0" {
        if let Some(diagnostic) = graph_migration::cross_core_diagnostic(graph) {
            bail!(diagnostic);
        }
        let catalog = catalog_for_graph(paths, graph)?;
        edge_admission::admit(graph, &catalog)
            .map_err(|diagnostic| anyhow::anyhow!("{diagnostic}"))?;
        let mut projection = graph.clone();
        projection["cores"]
            .as_object_mut()
            .context("graph cores must be an object")?
            .retain(|owner, _| owner == "eos");
        projection
    } else {
        graph.clone()
    };
    paths.prepare()?;
    let mut file = tempfile::Builder::new()
        .prefix("isos-graph-")
        .suffix(".json")
        .tempfile_in(&paths.runtime)?;
    serde_json::to_writer(&mut file, &eos_graph)?;
    use std::io::Write;
    file.flush()?;
    validate_with_program(file.path(), profile)?;
    project_with_packages(paths, &serde_json::to_string(graph)?)?;
    Ok(())
}

/// Admit the full owner graph, then validate only the EOS projection with its
/// registry. IsOS rechecks process nodes before supervising any child.
pub fn validate_full_graph(path: &Path) -> Result<()> {
    let graph = normalize_source(&fs::read_to_string(path)?)?;
    let paths = Paths::discover()?;
    validate_value(&paths, &graph)
}

pub fn catalog() -> Result<Value> {
    catalog_with_sidecars(packages::node_type_sidecars(&Paths::discover()?)?)
}

/// Built-in EOS registry without installed package contributions. Option
/// projection resolves package selectors from the draft's exact verified pin.
pub fn catalog_base() -> Result<Value> {
    catalog_with_sidecars(Vec::new())
}

/// Resolve only the exact MuJoCo process pin used by an editable draft. A
/// missing or changed pin is omitted so node-options can show a repair row.
/// Installed releases outside this graph cannot contribute selector metadata.
pub fn catalog_for_option_graph(paths: &Paths, graph: &Value) -> Result<Value> {
    if graph["version"] != "0.1.0" {
        // Historical graphs expose only the three read-only collections from
        // EOS's built-in registry. Unrelated installed releases cannot change
        // those collections or make inspection fail on duplicate type IDs.
        return catalog_base();
    }
    let mut sidecar = None;
    if let Some(nodes) = graph
        .pointer("/cores/eos/layers/process/groups/mujoco/nodes")
        .and_then(Value::as_object)
    {
        for node in nodes.values() {
            if node["node_type"] != "eos.process.mujoco@v0.1" {
                continue;
            }
            if let (Some(id), Some(version)) = (
                node["package"]["id"].as_str(),
                node["package"]["version"].as_str(),
            ) && let Ok(verified) = packages::verify(paths, id, version)
                && let Some(selected) = verified.sidecar
            {
                sidecar = Some(selected);
                break;
            }
        }
    }
    match sidecar {
        Some(sidecar) => catalog_with_sidecars(vec![sidecar]),
        None => catalog_base(),
    }
}

pub fn catalog_for_graph(paths: &Paths, graph: &Value) -> Result<Value> {
    catalog_with_sidecars(packages::node_type_sidecars_for_graph(paths, graph)?)
}

fn catalog_with_sidecars(sidecars: Vec<PathBuf>) -> Result<Value> {
    let program = core_bins::resolve("aether-profile")?;
    let mut command = Command::new(&program);
    command.arg("catalog");
    let output = command
        .output()
        .with_context(|| format!("run EOS type catalog {}", Path::new(&program).display()))?;
    ensure!(
        output.status.success(),
        "EOS catalog failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    ensure!(
        output.stdout.len() <= MAX_GRAPH_BYTES as usize,
        "EOS catalog exceeds 1 MiB"
    );
    let mut catalog: Value = serde_json::from_slice(&output.stdout)?;
    ensure!(
        catalog.get("types").and_then(Value::as_array).is_some(),
        "EOS catalog has no types array"
    );
    let mut contributed = BTreeMap::new();
    let mut contributed_data = BTreeMap::new();
    let mut contributed_adapters = BTreeMap::new();
    for sidecar in sidecars {
        let value: Value = serde_json::from_slice(&fs::read(&sidecar)?)?;
        let admitted = Command::new(&program)
            .arg("catalog")
            .arg("--node-types")
            .arg(&sidecar)
            .output()
            .with_context(|| format!("catalog package sidecar {}", sidecar.display()))?;
        ensure!(
            admitted.status.success(),
            "EOS rejected package sidecar {}: {}",
            sidecar.display(),
            String::from_utf8_lossy(&admitted.stderr).trim()
        );
        let admitted: Value = serde_json::from_slice(&admitted.stdout)?;
        if let Some(types) = value["node_types"].as_array() {
            for node_type in types {
                let id = node_type["id"]
                    .as_str()
                    .context("contributed node type lacks id")?;
                let mut node_type = node_type.clone();
                node_type["package"] = value["package"].clone();
                ensure!(
                    contributed.insert(id.to_owned(), node_type).is_none(),
                    "installed packages contribute duplicate node type {id}; remove one package version"
                );
            }
        }
        if let Some(data_types) = value["data_types"].as_array() {
            for data_type in data_types {
                let id = data_type["id"]
                    .as_str()
                    .context("contributed data type lacks id")?;
                let mut data_type = data_type.clone();
                data_type["package"] = value["package"].clone();
                ensure!(
                    contributed_data.insert(id.to_owned(), data_type).is_none(),
                    "installed packages contribute duplicate data type {id}; remove one package version"
                );
            }
        }
        if let Some(adapters) = admitted["adapter_families"].as_array() {
            for adapter in adapters {
                let id = adapter["id"]
                    .as_str()
                    .context("contributed adapter family lacks id")?;
                if !value["adapter_families"]
                    .as_array()
                    .is_some_and(|families| families.iter().any(|family| family["id"] == id))
                {
                    continue;
                }
                let mut adapter = adapter.clone();
                adapter["package"] = value["package"].clone();
                ensure!(
                    contributed_adapters
                        .insert(id.to_owned(), adapter)
                        .is_none(),
                    "installed packages contribute duplicate adapter family {id}"
                );
            }
        }
    }
    let types = catalog["types"].as_array_mut().unwrap();
    if !types
        .iter()
        .any(|item| item["id"] == "isos.process.component@v0.1")
    {
        let mut component = types
            .iter()
            .find(|item| item["id"] == "isos.process.component@v1.1")
            .context("EOS catalog lacks IsOS legacy component definition")?
            .clone();
        component["id"] = Value::String("isos.process.component@v0.1".into());
        component["description"] = Value::String(
            "IsOS-owned role-selected child with bounded arguments and readiness".into(),
        );
        if let Some(settings) = component["settings"].as_array_mut() {
            settings.push(serde_json::json!({
                "name":"env_from","data_type":"string_table","required":false,
                "description":"Child environment targets and manager environment source names"
            }));
        }
        types.push(component);
    }
    let builtin: BTreeSet<_> = types
        .iter()
        .filter_map(|value| value["id"].as_str().map(str::to_owned))
        .collect();
    for (id, node_type) in contributed {
        ensure!(
            !builtin.contains(&id),
            "package node type {id} conflicts with built-in type"
        );
        types.push(node_type);
    }
    if !catalog["data_types"].is_array() {
        catalog["data_types"] = serde_json::json!([]);
    }
    let data_types = catalog["data_types"].as_array_mut().unwrap();
    let builtin_data: BTreeSet<_> = data_types
        .iter()
        .filter_map(|value| value["id"].as_str().map(str::to_owned))
        .collect();
    for (id, data_type) in contributed_data {
        ensure!(
            !builtin_data.contains(&id),
            "package data type {id} conflicts with built-in type"
        );
        data_types.push(data_type);
    }
    if !catalog["adapter_families"].is_array() {
        catalog["adapter_families"] = serde_json::json!([]);
    }
    let families = catalog["adapter_families"].as_array_mut().unwrap();
    let builtin_families: BTreeSet<_> = families
        .iter()
        .filter_map(|value| value["id"].as_str().map(str::to_owned))
        .collect();
    for (id, family) in contributed_adapters {
        ensure!(
            !builtin_families.contains(&id),
            "package adapter family {id} conflicts with built-in family"
        );
        families.push(family);
    }
    Ok(catalog)
}

fn validate_with_program(path: &Path, program: &Path) -> Result<()> {
    let mut command = Command::new(program);
    command.arg("validate").arg("--file").arg(path);
    let graph = normalize_source(&fs::read_to_string(path)?)?;
    for sidecar in packages::node_type_sidecars_for_graph(&Paths::discover()?, &graph)? {
        command.arg("--node-types").arg(sidecar);
    }
    let output = command
        .output()
        .with_context(|| format!("run EOS node graph validator {}", program.display()))?;
    ensure!(
        output.status.success(),
        "EOS node graph validation failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

struct Running {
    spec: ProcessSpec,
    child: Child,
    restarts: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessStatus {
    pub id: String,
    pub pid: u32,
    pub restarts: u32,
    pub state: &'static str,
    pub package: Option<backend::PackageIdentity>,
    pub fingerprint: Option<String>,
}

/// The manager process is the parent of every child in this registry.
pub struct Supervisor {
    running: BTreeMap<String, Running>,
    order: Vec<String>,
    logs: log_store::LogStore,
    paths: Paths,
    run_id: String,
    admitted_nodes: Arc<BTreeSet<String>>,
}

impl Supervisor {
    pub fn specs(&self) -> Vec<ProcessSpec> {
        self.order
            .iter()
            .filter_map(|id| self.running.get(id).map(|running| running.spec.clone()))
            .collect()
    }

    pub fn active_packages(&self) -> Vec<backend::PackageIdentity> {
        self.running
            .values()
            .filter_map(|running| running.spec.package.clone())
            .collect()
    }
    pub fn verify_specs(&self, specs: &[ProcessSpec]) -> Result<()> {
        for spec in specs {
            if let Some(active) = self.running.get(&spec.id) {
                ensure!(
                    active.spec.fingerprint == spec.fingerprint,
                    "{}: linked package changed while active; restart after revalidation",
                    spec.id
                );
                ensure!(
                    active.spec.settings.command == spec.settings.command,
                    "{}: active command differs from saved graph; restart",
                    spec.id
                );
            }
        }
        Ok(())
    }
    pub fn new(paths: &Paths, deployment: &str) -> Result<Self> {
        validate_deployment_name(deployment)?;
        let mut admitted_nodes = BTreeSet::new();
        if let Ok(bytes) = fs::read(paths.node_graph_path(deployment))
            && let Ok(graph) = serde_json::from_slice::<Value>(&bytes)
        {
            for core in ["eos", "egia"] {
                if let Some(layers) = graph
                    .pointer(&format!("/cores/{core}/layers"))
                    .and_then(Value::as_object)
                {
                    for (layer, entry) in layers {
                        if let Some(groups) = entry.get("groups").and_then(Value::as_object) {
                            for (group, entry) in groups {
                                if let Some(nodes) = entry.get("nodes").and_then(Value::as_object) {
                                    for node in nodes.keys() {
                                        admitted_nodes.insert(format!("/cores/{core}/layers/{layer}/groups/{group}/nodes/{node}"));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(Self {
            running: BTreeMap::new(),
            order: Vec::new(),
            logs: log_store::LogStore::new(paths, deployment)?,
            paths: paths.clone(),
            run_id: crate::manager::next_run_id(),
            admitted_nodes: Arc::new(admitted_nodes),
        })
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn set_run_id(&mut self, run_id: String) {
        self.run_id = run_id;
    }

    fn spawn(
        &self,
        spec: &ProcessSpec,
        secrets: &BTreeMap<String, std::ffi::OsString>,
        secret_sources: &BTreeSet<String>,
    ) -> Result<Child> {
        let mut command = Command::new(&spec.settings.command[0]);
        command.args(&spec.settings.command[1..]);
        if matches!(spec.id.as_str(), "eos" | "bridge" | "config")
            && spec.settings.role.as_deref() == Some("eos")
        {
            command.arg("--run-id").arg(&self.run_id);
            if spec.id == "eos"
                && let Some(package) = &spec.package
            {
                let verified = packages::verify(&self.paths, &package.id, &package.version)?;
                let release = verified
                    .receipt
                    .release_sha256
                    .as_deref()
                    .context("verified release digest missing")?;
                let payload = verified
                    .receipt
                    .payload_sha256
                    .as_deref()
                    .context("verified payload digest missing")?;
                ensure!(
                    release.len() == 64
                        && release
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                    "invalid release digest"
                );
                ensure!(
                    payload.len() == 64
                        && payload
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                    "invalid payload digest"
                );
                let owner_dir = self.logs.root().join("owner/eos");
                fs::create_dir_all(&owner_dir)?;
                fs::set_permissions(
                    self.logs.root().join("owner"),
                    fs::Permissions::from_mode(0o700),
                )?;
                fs::set_permissions(&owner_dir, fs::Permissions::from_mode(0o700))?;
                command
                    .arg("--log-dir")
                    .arg(owner_dir)
                    .arg("--backend-release-sha256")
                    .arg(release)
                    .arg("--backend-payload-sha256")
                    .arg(payload);
            }
        }
        command.envs(&spec.settings.env);
        for source in secret_sources {
            command.env_remove(source);
        }
        command.envs(secrets);
        if spec.id == "egia" && spec.settings.role.as_deref() == Some("egia") {
            command.env("ISOS_RUN_ID", &self.run_id);
        }
        if let Some(cwd) = &spec.settings.cwd {
            command.current_dir(cwd);
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // A dedicated process group lets stop terminate descendants as well.
        let parent_pid = std::process::id();
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() as u32 != parent_pid {
                    return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
                }
                Ok(())
            });
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("spawn {}", spec.id))?;
        let pid = child.id();
        let source = if matches!(spec.settings.role.as_deref(), Some("eos" | "egia")) {
            spec.id.clone()
        } else {
            format!("unselected-{}", spec.id)
        };
        if let Some(stdout) = child.stdout.take() {
            log_store::capture(
                stdout,
                self.logs.clone(),
                source.clone(),
                "stdout",
                self.run_id.clone(),
                pid,
                Some(self.admitted_nodes.clone()),
            );
        }
        if let Some(stderr) = child.stderr.take() {
            log_store::capture(
                stderr,
                self.logs.clone(),
                source,
                "stderr",
                self.run_id.clone(),
                pid,
                Some(self.admitted_nodes.clone()),
            );
        }
        Ok(child)
    }

    fn unix_socket_ready(path: &Path) -> bool {
        let bytes = path.as_os_str().as_bytes();
        if bytes.is_empty() || bytes.len() > 107 || bytes.contains(&0) {
            return false;
        }
        // A nonblocking connect keeps a full listener backlog within the startup deadline.
        unsafe {
            let fd = libc::socket(
                libc::AF_UNIX,
                libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                0,
            );
            if fd < 0 {
                return false;
            }
            let mut address: libc::sockaddr_un = std::mem::zeroed();
            address.sun_family = libc::AF_UNIX as libc::sa_family_t;
            for (destination, byte) in address.sun_path.iter_mut().zip(bytes.iter()) {
                *destination = *byte as libc::c_char;
            }
            let length = (std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1)
                as libc::socklen_t;
            let connected =
                libc::connect(fd, (&raw const address).cast::<libc::sockaddr>(), length) == 0;
            libc::close(fd);
            connected
        }
    }

    fn wait_ready(&self, spec: &ProcessSpec, child: &mut Child) -> Result<()> {
        let deadline = Instant::now() + Duration::from_millis(spec.settings.start_timeout_ms);
        let no_probe_grace = Instant::now() + Duration::from_millis(50);
        loop {
            if let Some(status) = child.try_wait()? {
                bail!("{} exited before ready: {status}", spec.id);
            }
            let ready = if let Some(endpoint) = &spec.settings.ready_tcp {
                endpoint.to_socket_addrs().ok().is_some_and(|addrs| {
                    addrs.into_iter().any(|addr| {
                        TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok()
                    })
                })
            } else if let Some(path) = &spec.settings.ready_unix {
                Self::unix_socket_ready(path)
            } else {
                Instant::now() >= no_probe_grace
            };
            let owner_ready = if matches!(spec.settings.role.as_deref(), Some("eos" | "egia")) {
                let expected = format!("{}_ready", spec.id);
                self.logs
                    .tail_filtered(&spec.id, 100, None)
                    .ok()
                    .is_some_and(|events| {
                        events.iter().any(|event| {
                            event["run_id"].as_str() == Some(self.run_id.as_str())
                                && event["pid"].as_u64() == Some(child.id() as u64)
                                && event["event_code"].as_str() == Some(expected.as_str())
                        })
                    })
            } else {
                true
            };
            if ready && owner_ready {
                return Ok(());
            }
            ensure!(
                Instant::now() < deadline,
                "{} did not become ready",
                spec.id
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    #[cfg(test)]
    pub fn start(&mut self, specs: &[ProcessSpec]) -> Result<Vec<String>> {
        let plan = crate::generated_v2::secret_plan(specs)?;
        self.start_with_secrets(specs, &plan)
    }

    pub fn start_with_secrets(
        &mut self,
        specs: &[ProcessSpec],
        secrets: &crate::generated_v2::SecretPlan,
    ) -> Result<Vec<String>> {
        let secret_sources: BTreeSet<String> = self
            .running
            .values()
            .map(|running| &running.spec)
            .chain(specs.iter())
            .flat_map(|spec| spec.settings.env_from.values().cloned())
            .collect();
        for spec in specs {
            ensure!(
                spec.settings.env_from.keys().all(|target| secrets
                    .get(&spec.id)
                    .is_some_and(|child| child.contains_key(target))),
                "{}: child secret plan is incomplete",
                spec.id
            );
        }
        let mut added = Vec::new();
        for spec in specs {
            if self.running.contains_key(&spec.id) {
                continue;
            }
            for dependency in &spec.settings.depends_on {
                let id = dependency.rsplit('/').next().unwrap_or(dependency);
                ensure!(
                    self.running.contains_key(id),
                    "{} needs process {id}",
                    spec.id
                );
            }
            let empty = BTreeMap::new();
            let mut child = match self.spawn(
                spec,
                secrets.get(&spec.id).unwrap_or(&empty),
                &secret_sources,
            ) {
                Ok(child) => child,
                Err(error) => {
                    self.rollback(&added);
                    return Err(error);
                }
            };
            if let Err(error) = self.wait_ready(spec, &mut child) {
                terminate(&mut child);
                self.rollback(&added);
                return Err(error);
            }
            self.running.insert(
                spec.id.clone(),
                Running {
                    spec: spec.clone(),
                    child,
                    restarts: 0,
                },
            );
            self.order.push(spec.id.clone());
            added.push(spec.id.clone());
        }
        Ok(added)
    }

    fn rollback(&mut self, added: &[String]) {
        for id in added.iter().rev() {
            if let Some(mut process) = self.running.remove(id) {
                terminate(&mut process.child);
            }
            self.order.retain(|candidate| candidate != id);
        }
    }

    pub fn stop(&mut self) -> Vec<String> {
        let mut stopped = Vec::new();
        for id in self.order.drain(..).rev() {
            if let Some(mut process) = self.running.remove(&id) {
                terminate(&mut process.child);
                stopped.push(id);
            }
        }
        stopped
    }

    pub fn poll(&mut self) -> Result<()> {
        let ids: Vec<_> = self.running.keys().cloned().collect();
        for id in ids {
            let Some(current) = self.running.get_mut(&id) else {
                continue;
            };
            let exited = current.child.try_wait()?;
            let Some(status) = exited else { continue };
            let mut process = self.running.remove(&id).unwrap();
            // A provider's disappearance invalidates live dependents. Stop
            // them before attempting recovery, then restore them in order.
            let mut affected = BTreeSet::from([id.clone()]);
            loop {
                let previous = affected.len();
                for (candidate, running) in &self.running {
                    if running.spec.settings.depends_on.iter().any(|dependency| {
                        affected.contains(dependency.rsplit('/').next().unwrap_or(dependency))
                    }) {
                        affected.insert(candidate.clone());
                    }
                }
                if previous == affected.len() {
                    break;
                }
            }
            let dependent_specs: Vec<_> = self
                .order
                .iter()
                .filter(|candidate| *candidate != &id && affected.contains(*candidate))
                .filter_map(|candidate| {
                    self.running
                        .get(candidate)
                        .map(|running| running.spec.clone())
                })
                .collect();
            for spec in dependent_specs.iter().rev() {
                if let Some(mut dependent) = self.running.remove(&spec.id) {
                    terminate(&mut dependent.child);
                }
                self.order.retain(|candidate| candidate != &spec.id);
            }
            let restart = match process.spec.settings.restart {
                Restart::Never => false,
                Restart::OnFailure => !status.success(),
                Restart::Always => true,
            };
            if restart && process.restarts < process.spec.settings.max_restarts {
                let correlation = crate::manager::next_correlation_id();
                let _ = self.logs.append_event_context(
                    "isos",
                    "warn",
                    "child_restart_attempt",
                    "Managed child restart requested",
                    log_store::LogContext {
                        run_id: Some(&self.run_id),
                        correlation_id: Some(&correlation),
                        node_path: None,
                        fields: serde_json::json!({"process_id":id}),
                    },
                );
                thread::sleep(Duration::from_millis(100 * (1 << process.restarts.min(5))));
                let restart_specs: Vec<_> = std::iter::once(process.spec.clone())
                    .chain(dependent_specs.iter().cloned())
                    .collect();
                let plan = crate::generated_v2::secret_plan(&restart_specs)?;
                let secret_sources: BTreeSet<String> = self
                    .running
                    .values()
                    .map(|running| &running.spec)
                    .chain(restart_specs.iter())
                    .flat_map(|spec| spec.settings.env_from.values().cloned())
                    .collect();
                let empty = BTreeMap::new();
                let mut child = match self.spawn(
                    &process.spec,
                    plan.get(&process.spec.id).unwrap_or(&empty),
                    &secret_sources,
                ) {
                    Ok(child) => child,
                    Err(error) => {
                        self.order.retain(|candidate| candidate != &id);
                        return Err(error);
                    }
                };
                if let Err(error) = self.wait_ready(&process.spec, &mut child) {
                    terminate(&mut child);
                    self.order.retain(|candidate| candidate != &id);
                    return Err(error);
                }
                process.child = child;
                process.restarts += 1;
                self.running.insert(id.clone(), process);
                self.start_with_secrets(&dependent_specs, &plan)?;
                let _ = self.logs.append_event_context(
                    "isos",
                    "info",
                    "child_restart_ready",
                    "Managed child restart ready",
                    log_store::LogContext {
                        run_id: Some(&self.run_id),
                        correlation_id: Some(&correlation),
                        node_path: None,
                        fields: serde_json::json!({"process_id":id}),
                    },
                );
            } else {
                self.order.retain(|candidate| candidate != &id);
            }
        }
        Ok(())
    }

    pub fn status(&mut self) -> Result<Vec<ProcessStatus>> {
        self.poll()?;
        Ok(self.status_snapshot())
    }

    pub fn status_snapshot(&self) -> Vec<ProcessStatus> {
        self.running
            .iter()
            .map(|(id, running)| ProcessStatus {
                id: id.clone(),
                pid: running.child.id(),
                restarts: running.restarts,
                state: "running",
                package: running.spec.package.clone(),
                fingerprint: running.spec.fingerprint.clone(),
            })
            .collect()
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn terminate(child: &mut Child) {
    let pid = child.id() as i32;
    if child.try_wait().ok().flatten().is_none() {
        unsafe { libc::kill(-pid, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        unsafe { libc::kill(-pid, libc::SIGKILL) };
    }
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;

    fn wait_for_file(path: &Path, expected_lines: usize) -> String {
        for _ in 0..200 {
            if let Ok(bytes) = fs::read_to_string(path)
                && bytes.lines().count() >= expected_lines
            {
                return bytes;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("child did not write the expected record");
    }

    #[test]
    fn backend_resolution_failure_carries_exact_package_identity() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        let graph = serde_json::json!({"version":"1.0.0","robot":{"id":"r","name":"R"},"cores":{"eos":{"layers":{"process":{"groups":{"mujoco":{"nodes":{"sim":{"id":"sim","node_type":"eos.process.mujoco@v1.0","package":{"id":"island/mujoco-sim","version":"0.1.1"},"settings":{"scene":"default","socket":paths.runtime.join("mujoco.sock")}}}}}}}}}}).to_string();
        let error = project_with_packages(&paths, &graph).unwrap_err();
        let failure = error.downcast_ref::<packages::PackageFailure>().unwrap();
        assert_eq!(failure.id, "island/mujoco-sim");
        assert_eq!(failure.version, "0.1.1");
        assert_eq!(failure.stage, "manifest");
    }

    fn graph(settings: &str) -> String {
        let settings: Value = serde_json::from_str(settings).unwrap();
        serde_json::json!({
            "version": "1.0.0",
            "robot": {"id": "r", "name": "R"},
            "cores": {"isos": {"layers": {"process": {"groups": {"component": {
                "nodes": {"eos": {
                    "id": "eos", "node_type": "isos.process.component@v1.1",
                    "settings": settings
                }}
            }}}}}}
        })
        .to_string()
    }

    #[test]
    fn projection_rejects_wrong_type_placement_and_unbounded_launch() {
        let source = graph(r#"{"command":["/bin/sleep","1"]}"#);
        assert_eq!(project(&source).unwrap()[0].id, "eos");
        assert!(project(&source.replace("v1.1", "v2.0")).is_err());
        assert!(project(&source.replace("\"process\"", "\"hardware\"")).is_err());
        assert!(project(&graph(r#"{"command":["sleep","1"]}"#)).is_err());
        let toml = r#"
version = "1.0.0"
[robot]
id = "r"
name = "R"
[cores.isos.layers.process.groups.component.nodes.eos]
id = "eos"
node_type = "isos.process.component@v1.1"
label = "EOS"
description = "runtime"
[cores.isos.layers.process.groups.component.nodes.eos.settings]
command = ["/bin/sleep", "1"]
"#;
        assert_eq!(project(toml).unwrap()[0].id, "eos");
    }

    #[test]
    fn unix_readiness_settings_are_bounded_and_exclusive() {
        assert!(
            project(&graph(
                r#"{"command":["/bin/sleep","1"],"ready_unix":"relative.sock"}"#
            ))
            .is_err()
        );
        assert!(
            project(&graph(
                &serde_json::json!({
                    "command": ["/bin/sleep", "1"],
                    "ready_unix": format!("/{}", "x".repeat(107))
                })
                .to_string()
            ))
            .is_err()
        );
        assert!(project(&graph(r#"{"command":["/bin/sleep","1"],"ready_unix":"/tmp/eos.sock","ready_tcp":"127.0.0.1:1"}"#)).is_err());
    }

    #[test]
    fn rejects_workbench_and_duplicate_listener_ports() {
        let mut spec = project(&graph(
            r#"{"command":["/bin/sleep","1"],"ready_tcp":"127.0.0.1:16020"}"#,
        ))
        .unwrap()
        .remove(0);
        let mut specs = BTreeMap::new();
        specs.insert(spec.id.clone(), spec.clone());
        assert!(validate_listener_ports(&specs).is_ok());

        let mut second = spec.clone();
        second.id = "bridge".into();
        second.path = "isos/process/component/bridge".into();
        specs.insert(second.id.clone(), second);
        assert!(validate_listener_ports(&specs).is_err());

        specs.remove("bridge");
        spec.settings.ready_tcp = Some("127.0.0.1:16000".into());
        specs.insert(spec.id.clone(), spec);
        assert!(validate_listener_ports(&specs).is_err());
    }

    #[test]
    fn selected_real_child_binds_run_pid_and_stderr_only() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let script = temp.path().join("selected-eos");
        fs::write(&script, r#"#!/bin/sh
run=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '--run-id' ]; then run="$2"; shift 2; else shift; fi
done
printf '{"timestamp":"2026-11-01T10:00:01.000Z","level":"info","source":"eos","core":"eos","event_code":"eos_ready","run_id":"%s","message":"EOS runtime ready","fields":{"state":"ready","node_count":1}}\n' "$run" >&2
printf '{"timestamp":"2026-11-01T10:00:01.000Z","level":"info","source":"eos","core":"eos","event_code":"eos_ready","run_id":"%s","message":"EOS runtime ready","fields":{"state":"ready","node_count":1}}\n' "$run"
sleep 2
"#).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let mut spec = project(&graph(&serde_json::json!({"command":[script]}).to_string()))
            .unwrap()
            .remove(0);
        spec.settings.role = Some("eos".into());
        let mut supervisor = Supervisor::new(&paths, "default").unwrap();
        let run_id = supervisor.run_id().to_owned();
        supervisor.start(&[spec]).unwrap();
        let pid = supervisor.status_snapshot()[0].pid;
        let logs = log_store::LogStore::new(&paths, "default").unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let line = loop {
            let lines = logs.read("eos", None, 10).unwrap();
            if let Some(line) = lines["lines"].as_array().unwrap().first() {
                break line.clone();
            }
            assert!(
                Instant::now() < deadline,
                "selected child event did not arrive"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(line["run_id"], run_id);
        assert_eq!(line["pid"], pid);
        assert_eq!(
            logs.read("eos", None, 10).unwrap()["lines"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(logs.root().join("owner/eos/stdout.log").is_file());
        assert!(logs.root().join("owner/eos/stderr.log").is_file());
        fs::remove_file(logs.root().join("events.log")).unwrap();
        fs::create_dir(logs.root().join("events.log")).unwrap();
        let mut later = line.clone();
        later["event_code"] = serde_json::json!("eos_stopping");
        later["message"] = serde_json::json!("EOS eos stopping");
        later["fields"] = serde_json::json!({"state":"stopping"});
        assert!(logs.append_selected(&later).is_err());
        let stop_started = Instant::now();
        supervisor.stop();
        assert!(stop_started.elapsed() < Duration::from_secs(1));
        assert!(log_store::counters().1 > 0);
    }

    #[test]
    fn selected_egia_run_binding_overrides_authored_environment() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let script = temp.path().join("selected-egia");
        fs::write(&script, r#"#!/bin/sh
printf '{"timestamp":"2026-11-01T10:00:01.000Z","level":"info","source":"egia","core":"egia","event_code":"egia_ready","run_id":"%s","message":"EGIA ready","fields":{"generation":1}}\n' "$ISOS_RUN_ID" >&2
sleep 2
"#).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let mut spec = project(&graph(
            &serde_json::json!({"command":[script],"env":{"ISOS_RUN_ID":"r-spoof"}}).to_string(),
        ))
        .unwrap()
        .remove(0);
        spec.id = "egia".into();
        spec.path = "isos/process/component/egia".into();
        spec.settings.role = Some("egia".into());
        let mut supervisor = Supervisor::new(&paths, "default").unwrap();
        let run_id = supervisor.run_id().to_owned();
        supervisor.start(&[spec]).unwrap();
        let lines = log_store::LogStore::new(&paths, "default")
            .unwrap()
            .read("egia", None, 10)
            .unwrap();
        assert_eq!(lines["lines"][0]["run_id"], run_id);
        supervisor.stop();
    }

    #[test]
    fn waits_for_unix_listener_and_rolls_back_on_timeout() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let socket = temp.path().join("ready.sock");
        let settings = serde_json::json!({
            "command": ["/bin/sleep", "30"],
            "ready_unix": socket,
            "start_timeout_ms": 1000
        });
        let specs = project(&graph(&settings.to_string())).unwrap();
        let listener_path = socket.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let listener = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            let _listener = UnixListener::bind(listener_path).unwrap();
            let _ = done_rx.recv_timeout(Duration::from_secs(2));
        });
        let mut supervisor = Supervisor::new(&paths, "test").unwrap();
        let start = Instant::now();
        assert_eq!(supervisor.start(&specs).unwrap(), vec!["eos"]);
        assert!(start.elapsed() >= Duration::from_millis(100));
        assert_eq!(supervisor.stop(), vec!["eos"]);
        done_tx.send(()).unwrap();
        listener.join().unwrap();

        fs::remove_file(&socket).unwrap();
        let timeout = serde_json::json!({
            "command": ["/bin/sleep", "30"],
            "ready_unix": socket,
            "start_timeout_ms": 120
        });
        assert!(
            supervisor
                .start(&project(&graph(&timeout.to_string())).unwrap())
                .is_err()
        );
        assert!(supervisor.status().unwrap().is_empty());
    }

    #[test]
    fn direct_child_start_stop_and_failed_readiness() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let mut supervisor = Supervisor::new(&paths, "test").unwrap();
        let specs = project(&graph(r#"{"command":["/bin/sleep","30"]}"#)).unwrap();
        assert_eq!(supervisor.start(&specs).unwrap(), vec!["eos"]);
        assert_eq!(supervisor.status().unwrap().len(), 1);
        assert_eq!(supervisor.stop(), vec!["eos"]);
        assert!(supervisor.status().unwrap().is_empty());

        let bad = project(&graph(
            r#"{"command":["/bin/false"],"start_timeout_ms":20}"#,
        ))
        .unwrap();
        assert!(supervisor.start(&bad).is_err());
        assert!(supervisor.status().unwrap().is_empty());
    }

    #[test]
    fn full_graph_validator_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let graph_path = temp.path().join("graph.json");
        fs::write(&graph_path, graph(r#"{"command":["/bin/sleep","1"]}"#)).unwrap();
        validate_with_program(&graph_path, Path::new("/bin/true")).unwrap();
        assert!(validate_with_program(&graph_path, Path::new("/bin/false")).is_err());
        assert!(validate_with_program(&graph_path, Path::new("/missing/validator")).is_err());
    }

    #[test]
    fn target_validation_rejects_cross_owner_reference_before_eos_projection() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        let mut graph = crate::starter::draft(&paths, "default").unwrap();
        graph["cores"]["eos"]["layers"]["dispatcher"]["groups"]["control"]["nodes"]["command"]["references"] =
            serde_json::json!(["isos/process/component/eos"]);
        let error = validate_value_with_profile(&paths, &graph, Path::new("/bin/true"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("cross_core_link_unavailable"), "{error}");
        assert!(error.contains("/references/0"), "{error}");
    }

    #[test]
    fn restarts_failing_child_with_bounded_attempts() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let mut supervisor = Supervisor::new(&paths, "test").unwrap();
        let specs = project(&graph(
            r#"{"command":["/bin/sleep","0.1"],"restart":"always","max_restarts":1}"#,
        ))
        .unwrap();
        supervisor.start(&specs).unwrap();
        thread::sleep(Duration::from_millis(140));
        assert_eq!(supervisor.status().unwrap()[0].restarts, 1);
        let events = log_store::LogStore::new(&paths, "test")
            .unwrap()
            .read("all", None, 10)
            .unwrap();
        let events = events["lines"].as_array().unwrap();
        assert_eq!(events[0]["event_code"], "child_restart_attempt");
        assert_eq!(events[1]["event_code"], "child_restart_ready");
        assert_eq!(events[0]["correlation_id"], events[1]["correlation_id"]);
        assert_eq!(events[0]["run_id"], supervisor.run_id());
        thread::sleep(Duration::from_millis(140));
        assert!(supervisor.status().unwrap().is_empty());
    }

    #[test]
    fn provider_exit_stops_dependent_process() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let mut source: Value =
            serde_json::from_str(&graph(r#"{"command":["/bin/sleep","0.1"]}"#)).unwrap();
        source["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]["egia"] = serde_json::json!({
            "id": "egia",
            "node_type": "isos.process.component@v1.1",
            "settings": {
                "command": ["/bin/sleep", "30"],
                "depends_on": ["cores/isos/layers/process/groups/component/nodes/eos"]
            }
        });
        let specs = project(&source.to_string()).unwrap();
        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.id.as_str())
                .collect::<Vec<_>>(),
            ["eos", "egia"]
        );
        let mut supervisor = Supervisor::new(&paths, "test").unwrap();
        supervisor.start(&specs).unwrap();
        thread::sleep(Duration::from_millis(140));
        assert!(supervisor.status().unwrap().is_empty());
    }

    #[test]
    fn env_from_value_reaches_only_its_named_child() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let source = "ISOS_P04_ISOLATION_SOURCE_9A6D1";
        unsafe { std::env::set_var(source, "private-value") };
        let eos_file = temp.path().join("eos.txt");
        let egia_file = temp.path().join("egia.txt");
        let script = temp.path().join("record.sh");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s|%s\\n' \"${{{source}-unset}}\" \"${{EGIA_API_KEY-unset}}\" > \"$1\"\nsleep 30\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let make = |id: &str, output: &Path, secret: bool| {
            ProcessSpec {
            id: id.into(),
            path: format!("isos/process/component/{id}"),
            settings: serde_json::from_value(serde_json::json!({
                "command": [script, output],
                "env_from": if secret { serde_json::json!({"EGIA_API_KEY":source}) } else { serde_json::json!({}) },
            }))
            .unwrap(),
            package: None,
            fingerprint: None,
        }
        };
        let specs = [
            make("eos", &eos_file, false),
            make("egia", &egia_file, true),
        ];
        let mut supervisor = Supervisor::new(&paths, "test").unwrap();
        assert_eq!(supervisor.start(&specs).unwrap(), ["eos", "egia"]);
        assert_eq!(wait_for_file(&eos_file, 1), "unset|unset\n");
        assert_eq!(wait_for_file(&egia_file, 1), "unset|private-value\n");
        supervisor.stop();
        unsafe { std::env::remove_var(source) };
    }

    #[test]
    fn dependent_auto_restart_uses_its_secret_plan_without_inheriting_source() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let source = "ISOS_P04_RESTART_SOURCE_36B11";
        unsafe { std::env::set_var(source, "restart-value") };
        let record = temp.path().join("dependent.txt");
        let script = temp.path().join("dependent.sh");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s|%s\\n' \"${{{source}-unset}}\" \"${{EGIA_API_KEY-unset}}\" >> \"$1\"\nsleep 30\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let provider = ProcessSpec {
            id: "eos".into(),
            path: "isos/process/component/eos".into(),
            settings: serde_json::from_value(serde_json::json!({
                "command": ["/bin/sh", "-c", "sleep 0.35; exit 1"],
                "restart": "always",
                "max_restarts": 1
            }))
            .unwrap(),
            package: None,
            fingerprint: None,
        };
        let dependent = ProcessSpec {
            id: "egia".into(),
            path: "isos/process/component/egia".into(),
            settings: serde_json::from_value(serde_json::json!({
                "command": [script, record],
                "depends_on": ["cores/isos/layers/process/groups/component/nodes/eos"],
                "env_from": {"EGIA_API_KEY": source}
            }))
            .unwrap(),
            package: None,
            fingerprint: None,
        };
        let mut supervisor = Supervisor::new(&paths, "test").unwrap();
        supervisor.start(&[provider, dependent]).unwrap();
        thread::sleep(Duration::from_millis(420));
        let status = supervisor.status().unwrap();
        assert_eq!(status.len(), 2);
        assert_eq!(
            status
                .iter()
                .find(|item| item.id == "eos")
                .unwrap()
                .restarts,
            1
        );
        assert_eq!(
            wait_for_file(&record, 2),
            "unset|restart-value\nunset|restart-value\n"
        );
        supervisor.stop();
        unsafe { std::env::remove_var(source) };
    }
}
