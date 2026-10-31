//! IsOS-owned process projection of the shared v2 node graph.
//!
//! The graph remains the authored source. This module admits only registered
//! IsOS process nodes and keeps direct child handles in the manager process.

use crate::{config::Paths, log_store, packages, v2_backend, v2_bins};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    net::{TcpStream, ToSocketAddrs},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_GRAPH_BYTES: u64 = 1024 * 1024;
const MAX_COMMAND_ARGS: usize = 64;
const MAX_RESTARTS: u32 = 10;

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
    package: Option<v2_backend::PackageIdentity>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackendSettings {
    #[serde(default)]
    package: Option<String>,
    #[serde(default)]
    version: Option<String>,
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
    pub command: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
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
    pub package: Option<v2_backend::PackageIdentity>,
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
    ensure!(valid_segment(name), "invalid v2 deployment name");
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

fn project_inner(paths: Option<&Paths>, source: &str) -> Result<Vec<ProcessSpec>> {
    ensure!(
        source.len() as u64 <= MAX_GRAPH_BYTES,
        "v2 graph exceeds 1 MiB"
    );
    let value = normalize_source(source)?;
    let sidecars = paths
        .map(|paths| packages::node_type_sidecars_for_graph(paths, &value))
        .transpose()?
        .unwrap_or_default();
    let graph: Graph = serde_json::from_value(value)?;
    ensure!(graph.version == "2.0.0", "graph version must be 2.0.0");
    let mut specs = BTreeMap::new();
    let mut backends = BTreeMap::new();
    for (core_id, core) in &graph.cores {
        for (layer_id, layer) in &core.layers {
            for (group_id, group) in &layer.groups {
                for (key, node) in &group.nodes {
                    if node.node_type != "eos.process.backend@v1.0"
                        && node.node_type != "eos.process.mujoco@v1.0"
                    {
                        continue;
                    }
                    let path = format!("{core_id}/{layer_id}/{group_id}/{key}");
                    ensure!(
                        core_id == "eos"
                            && layer_id == "process"
                            && group_id
                                == if node.node_type == "eos.process.mujoco@v1.0" {
                                    "mujoco"
                                } else {
                                    "backend"
                                }
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
                        .map(|p| p.id.as_str())
                        .or(settings.package.as_deref())
                        .context("backend requires package identity")?;
                    let version = node
                        .package
                        .as_ref()
                        .map(|p| p.version.as_str())
                        .or(settings.version.as_deref())
                        .context("backend requires package version")?;
                    ensure!(
                        settings.package.as_deref().is_none_or(|id| id == package)
                            && settings.version.as_deref().is_none_or(|v| v == version),
                        "backend package pin differs"
                    );
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
                    let resolved = v2_backend::resolve(
                        paths,
                        package,
                        version,
                        &settings.scene,
                        &settings.env,
                    )?;
                    ensure!(
                        backends.insert(path, (resolved, settings.socket)).is_none(),
                        "duplicate backend path"
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
                    if node.node_type == "eos.process.backend@v1.0"
                        || node.node_type == "eos.process.mujoco@v1.0"
                    {
                        continue;
                    }
                    if node.node_type == "isos.process.component@v1.0" {
                        ensure!(
                            core_id == "isos" && layer_id == "process" && group_id == "component",
                            "{path}: process type has invalid placement"
                        );
                        ensure!(key == node.id, "{path}: node id differs from map key");
                        let mut settings: ProcessSettings =
                            serde_json::from_value(node.settings)
                                .with_context(|| format!("{path}: invalid process settings"))?;
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

pub fn normalize_source(source: &str) -> Result<Value> {
    ensure!(
        source.len() as u64 <= MAX_GRAPH_BYTES,
        "v2 graph exceeds 1 MiB"
    );
    if source.trim_start().starts_with('{') {
        Ok(serde_json::from_str(source)?)
    } else {
        let value: toml::Value = toml::from_str(source)?;
        Ok(serde_json::to_value(value)?)
    }
}

pub fn validate_value(paths: &Paths, graph: &Value) -> Result<()> {
    validate_value_with_profile(paths, graph, &v2_bins::resolve("aether-profile")?)
}

pub fn validate_value_with_profile(paths: &Paths, graph: &Value, profile: &Path) -> Result<()> {
    paths.prepare()?;
    let mut file = tempfile::Builder::new()
        .prefix("isos-v2-graph-")
        .suffix(".json")
        .tempfile_in(&paths.runtime)?;
    serde_json::to_writer(&mut file, graph)?;
    use std::io::Write;
    file.flush()?;
    validate_with_program(file.path(), profile)?;
    project_with_packages(paths, &serde_json::to_string(graph)?)?;
    Ok(())
}

/// Full graph admission is delegated to EOS's node-type registry. IsOS then
/// validates the process projection again before supervising any child.
pub fn validate_full_graph(path: &Path) -> Result<()> {
    validate_with_program(path, &v2_bins::resolve("aether-profile")?)
}

pub fn catalog() -> Result<Value> {
    let program = v2_bins::resolve("aether-profile")?;
    let mut command = Command::new(&program);
    command.arg("catalog");
    let output = command
        .output()
        .with_context(|| format!("run EOS v2 type catalog {}", Path::new(&program).display()))?;
    ensure!(
        output.status.success(),
        "EOS v2 catalog failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    ensure!(
        output.stdout.len() <= MAX_GRAPH_BYTES as usize,
        "EOS v2 catalog exceeds 1 MiB"
    );
    let mut catalog: Value = serde_json::from_slice(&output.stdout)?;
    ensure!(
        catalog.get("types").and_then(Value::as_array).is_some(),
        "EOS v2 catalog has no types array"
    );
    let mut contributed = BTreeMap::new();
    let mut contributed_data = BTreeMap::new();
    let mut contributed_adapters = BTreeMap::new();
    for sidecar in packages::node_type_sidecars(&Paths::discover()?)? {
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
        .with_context(|| format!("run EOS v2 graph validator {}", program.display()))?;
    ensure!(
        output.status.success(),
        "EOS v2 graph validation failed: {}",
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
    pub package: Option<v2_backend::PackageIdentity>,
    pub fingerprint: Option<String>,
}

/// The manager process is the parent of every child in this registry.
pub struct Supervisor {
    running: BTreeMap<String, Running>,
    order: Vec<String>,
    logs: log_store::LogStore,
}

impl Supervisor {
    pub fn specs(&self) -> Vec<ProcessSpec> {
        self.order
            .iter()
            .filter_map(|id| self.running.get(id).map(|running| running.spec.clone()))
            .collect()
    }

    pub fn active_packages(&self) -> Vec<v2_backend::PackageIdentity> {
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
        Ok(Self {
            running: BTreeMap::new(),
            order: Vec::new(),
            logs: log_store::LogStore::new(paths, deployment)?,
        })
    }

    fn spawn(&self, spec: &ProcessSpec) -> Result<Child> {
        let mut command = Command::new(&spec.settings.command[0]);
        command.args(&spec.settings.command[1..]);
        if spec.id == "eos" && spec.package.is_some() {
            command.arg("--log-dir").arg(self.logs.root());
        }
        command.envs(&spec.settings.env);
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
        if let Some(stdout) = child.stdout.take() {
            log_store::capture(stdout, self.logs.clone(), spec.id.clone(), "info");
        }
        if let Some(stderr) = child.stderr.take() {
            log_store::capture(stderr, self.logs.clone(), spec.id.clone(), "warn");
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

    fn wait_ready(spec: &ProcessSpec, child: &mut Child) -> Result<()> {
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
            if ready {
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

    pub fn start(&mut self, specs: &[ProcessSpec]) -> Result<Vec<String>> {
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
            let mut child = match self.spawn(spec) {
                Ok(child) => child,
                Err(error) => {
                    self.rollback(&added);
                    return Err(error);
                }
            };
            if let Err(error) = Self::wait_ready(spec, &mut child) {
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
                thread::sleep(Duration::from_millis(100 * (1 << process.restarts.min(5))));
                let mut child = match self.spawn(&process.spec) {
                    Ok(child) => child,
                    Err(error) => {
                        self.order.retain(|candidate| candidate != &id);
                        return Err(error);
                    }
                };
                if let Err(error) = Self::wait_ready(&process.spec, &mut child) {
                    terminate(&mut child);
                    self.order.retain(|candidate| candidate != &id);
                    return Err(error);
                }
                process.child = child;
                process.restarts += 1;
                self.running.insert(id, process);
                self.start(&dependent_specs)?;
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

    fn graph(settings: &str) -> String {
        let settings: Value = serde_json::from_str(settings).unwrap();
        serde_json::json!({
            "version": "2.0.0",
            "robot": {"id": "r", "name": "R"},
            "cores": {"isos": {"layers": {"process": {"groups": {"component": {
                "nodes": {"eos": {
                    "id": "eos", "node_type": "isos.process.component@v1.0",
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
        assert!(project(&source.replace("v1.0", "v2.0")).is_err());
        assert!(project(&source.replace("\"process\"", "\"hardware\"")).is_err());
        assert!(project(&graph(r#"{"command":["sleep","1"]}"#)).is_err());
        let toml = r#"
version = "2.0.0"
[robot]
id = "r"
name = "R"
[cores.isos.layers.process.groups.component.nodes.eos]
id = "eos"
node_type = "isos.process.component@v1.0"
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
            "node_type": "isos.process.component@v1.0",
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
}
