//! Explicit, deterministic conversion of saved legacy graph values.

use crate::{
    config::{self, Paths},
    core_setup, edge_admission,
    graph_compat::{self, Diagnostic, SavedGraph},
    lock, preflight, process,
};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};

const TYPE_MAP: &[(&str, &str)] = &[
    ("eos.hardware.camera@v1.0", "eos.hardware.camera@v0.1"),
    ("eos.hardware.base@v1.0", "eos.hardware.base@v0.1"),
    (
        "eos.hardware.manipulator@v1.0",
        "eos.hardware.manipulator@v0.1",
    ),
    ("eos.dispatcher.control@v1.0", "eos.dispatcher.control@v0.1"),
    (
        "eos.dispatcher.primitive_base@v1.0",
        "eos.dispatcher.primitive_base@v0.1",
    ),
    (
        "eos.dispatcher.primitive_manipulator@v1.0",
        "eos.dispatcher.primitive_manipulator@v0.1",
    ),
    (
        "eos.interface.camera_gate@v1.0",
        "eos.interface.camera_gate@v0.1",
    ),
    ("isos.process.component@v1.1", "isos.process.component@v0.1"),
    ("eos.process.mujoco@v1.0", "eos.process.mujoco@v0.1"),
];
const PASS_THROUGH: &[&str] = &[
    "egia.common.source@v1.0",
    "egia.common.context@v1.0",
    "egia.common.planner@v1.0",
    "egia.common.guard@v1.0",
];
const ROOT_FIELDS: &[&str] = &["version", "robot", "cores"];
const ROBOT_FIELDS: &[&str] = &["id", "name"];
const NODE_FIELDS: &[&str] = &[
    "id",
    "node_type",
    "label",
    "description",
    "capabilities",
    "settings",
    "inputs",
    "references",
    "adapter",
    "package",
    "health_timing",
    "observation",
    "movement",
    "frame_id",
    "joint_names",
    "enabled",
];

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub from: String,
    pub to: String,
}

pub struct Proposal {
    pub graph: Value,
    pub bytes: Vec<u8>,
    pub revision: String,
    pub changes: Vec<Change>,
}

pub struct Prepared {
    pub specs: Vec<process::ProcessSpec>,
    pub files: Vec<(PathBuf, Vec<u8>)>,
    pub edges: Vec<edge_admission::Edge>,
}

pub enum MigrationError {
    Conflict(String),
    Diagnostic(Diagnostic),
    Filesystem(anyhow::Error),
    OutcomeUncertain(anyhow::Error),
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(message) => write!(formatter, "{message}"),
            Self::Diagnostic(diagnostic) => write!(formatter, "{diagnostic}"),
            Self::Filesystem(error) => write!(formatter, "{error:#}"),
            Self::OutcomeUncertain(error) => {
                write!(formatter, "commit_outcome_uncertain: {error:#}")
            }
        }
    }
}

fn diagnostic(
    path: impl Into<String>,
    code: &'static str,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        path: path.into(),
        code,
        message: message.into(),
        context: BTreeMap::new(),
    }
}

fn edge_diagnostic(edge: edge_admission::Diagnostic) -> Diagnostic {
    let mut context = BTreeMap::new();
    for (key, value) in [
        ("source", edge.source),
        ("target", edge.target),
        ("input", edge.input),
        ("source_data_type", edge.source_data_type),
        ("target_data_type", edge.target_data_type),
    ] {
        if let Some(value) = value {
            context.insert(key.into(), value);
        }
    }
    Diagnostic {
        path: edge.path,
        code: edge.code,
        message: edge.message,
        context,
    }
}

fn keys(value: &Value, path: &str, allowed: &[&str]) -> Result<(), Diagnostic> {
    let object = value.as_object().ok_or_else(|| {
        diagnostic(
            path,
            "unmapped_graph_field",
            "expected an object in the legacy graph",
        )
    })?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(diagnostic(
                format!("{path}/{key}"),
                "unmapped_graph_field",
                format!("legacy field {key} is not declared by the owner schema"),
            ));
        }
    }
    Ok(())
}

pub fn propose(saved: &SavedGraph) -> Result<Proposal, Diagnostic> {
    if saved.value["version"] != "1.0.0" {
        return Err(diagnostic(
            "/version",
            "migration_required",
            "only a saved 1.0.0 graph can be migrated",
        ));
    }
    keys(&saved.value, "", ROOT_FIELDS)?;
    keys(&saved.value["robot"], "/robot", ROBOT_FIELDS)?;
    let mut graph = saved.value.clone();
    graph["version"] = Value::String("0.1.0".into());
    let mut changes = vec![Change {
        path: "/version".into(),
        from: "1.0.0".into(),
        to: "0.1.0".into(),
    }];
    let cores = graph["cores"]
        .as_object_mut()
        .ok_or_else(|| diagnostic("/cores", "unmapped_graph_field", "cores must be an object"))?;
    for (core_id, core) in cores {
        let core_path = format!("/cores/{core_id}");
        keys(core, &core_path, &["layers"])?;
        let layers = core["layers"].as_object_mut().ok_or_else(|| {
            diagnostic(
                format!("{core_path}/layers"),
                "unmapped_graph_field",
                "layers must be an object",
            )
        })?;
        for (layer_id, layer) in layers {
            let layer_path = format!("{core_path}/layers/{layer_id}");
            keys(layer, &layer_path, &["groups"])?;
            let groups = layer["groups"].as_object_mut().ok_or_else(|| {
                diagnostic(
                    format!("{layer_path}/groups"),
                    "unmapped_graph_field",
                    "groups must be an object",
                )
            })?;
            for (group_id, group) in groups {
                let group_path = format!("{layer_path}/groups/{group_id}");
                keys(group, &group_path, &["nodes"])?;
                let nodes = group["nodes"].as_object_mut().ok_or_else(|| {
                    diagnostic(
                        format!("{group_path}/nodes"),
                        "unmapped_graph_field",
                        "nodes must be an object",
                    )
                })?;
                for (node_id, node) in nodes {
                    let node_path = format!("{group_path}/nodes/{node_id}");
                    keys(node, &node_path, NODE_FIELDS)?;
                    let node_type = node["node_type"].as_str().unwrap_or("");
                    if node_type == "isos.process.component@v1.0"
                        || (core_id == "isos" && node.pointer("/settings/command").is_some())
                    {
                        let path = if node_type == "isos.process.component@v1.0" {
                            format!("{node_path}/node_type")
                        } else {
                            format!("{node_path}/settings/command")
                        };
                        return Err(diagnostic(
                            path,
                            "unsafe_process_node",
                            "arbitrary-command process nodes cannot be migrated",
                        ));
                    }
                    let Some((_, target)) = TYPE_MAP.iter().find(|(old, _)| *old == node_type)
                    else {
                        if PASS_THROUGH.contains(&node_type) {
                            continue;
                        }
                        return Err(diagnostic(
                            format!("{node_path}/node_type"),
                            "unmapped_node_type",
                            format!("{node_type} has no approved 0.1.0 mapping"),
                        ));
                    };
                    if node_type == "eos.process.mujoco@v1.0" && node["package"].is_null() {
                        return Err(diagnostic(
                            format!("{node_path}/package"),
                            "unmapped_node_type",
                            "MuJoCo process mapping requires the same exact package pin",
                        ));
                    }
                    let from = node_type.to_owned();
                    node["node_type"] = Value::String((*target).into());
                    changes.push(Change {
                        path: format!("{node_path}/node_type"),
                        from,
                        to: (*target).into(),
                    });
                    if changes.len() > 256 {
                        return Err(diagnostic(
                            "/changes",
                            "migration_change_limit",
                            "migration changes exceed 256 entries",
                        ));
                    }
                }
            }
        }
    }
    let bytes = serde_json::to_vec_pretty(&graph)
        .map_err(|error| diagnostic("/", "migration_serialization_failed", error.to_string()))?;
    Ok(Proposal {
        revision: graph_compat::revision(&bytes),
        graph,
        bytes,
        changes,
    })
}

fn core_of(path: &str) -> Option<&str> {
    if path.starts_with("cores/") {
        path.split('/').nth(1)
    } else {
        path.split('/').next()
    }
}

pub fn cross_core_diagnostic(graph: &Value) -> Option<Diagnostic> {
    let cores = graph["cores"].as_object()?;
    for (core_id, core) in cores {
        for (layer_id, layer) in core["layers"].as_object()? {
            for (group_id, group) in layer["groups"].as_object()? {
                for (node_id, node) in group["nodes"].as_object()? {
                    let base = format!(
                        "/cores/{core_id}/layers/{layer_id}/groups/{group_id}/nodes/{node_id}"
                    );
                    if let Some(references) = node["references"].as_array() {
                        for (index, value) in references.iter().enumerate() {
                            if value
                                .as_str()
                                .and_then(core_of)
                                .is_some_and(|source| source != core_id)
                            {
                                return Some(diagnostic(
                                    format!("{base}/references/{index}"),
                                    "cross_core_link_unavailable",
                                    "cross-core references require G06 admission",
                                ));
                            }
                        }
                    }
                    if let Some(dependencies) = node
                        .pointer("/settings/depends_on")
                        .and_then(Value::as_array)
                    {
                        for (index, value) in dependencies.iter().enumerate() {
                            if value
                                .as_str()
                                .and_then(core_of)
                                .is_some_and(|source| source != core_id)
                            {
                                return Some(diagnostic(
                                    format!("{base}/settings/depends_on/{index}"),
                                    "cross_core_link_unavailable",
                                    "cross-core process dependencies require G06 admission",
                                ));
                            }
                        }
                    }
                    if let Some(inputs) = node["inputs"].as_object() {
                        for (input, sources) in inputs {
                            let source_list: Vec<(String, &Value)> =
                                sources.as_array().map_or_else(
                                    || vec![(format!("{base}/inputs/{input}"), sources)],
                                    |items| {
                                        items
                                            .iter()
                                            .enumerate()
                                            .map(|(index, source)| {
                                                (format!("{base}/inputs/{input}/{index}"), source)
                                            })
                                            .collect()
                                    },
                                );
                            for (path, source) in source_list {
                                let reference = source
                                    .get("node")
                                    .and_then(Value::as_str)
                                    .or_else(|| source.as_str());
                                if reference
                                    .and_then(core_of)
                                    .is_some_and(|source| source != core_id)
                                {
                                    return Some(diagnostic(
                                        path,
                                        "cross_core_link_unavailable",
                                        "cross-core edges require G06 admission",
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn prepare(paths: &Paths, name: &str, proposal: &Proposal) -> Result<Prepared, Diagnostic> {
    if let Some(diagnostic) = graph_compat::admission_diagnostic(&proposal.graph, None) {
        return Err(diagnostic);
    }
    if proposal.graph.pointer("/cores/eos").is_none() {
        return Err(diagnostic(
            "/cores/eos",
            "owner_reader_unavailable",
            "0.1.0 deployment requires an EOS core",
        ));
    }
    for role in ["eos", "egia"] {
        if role == "egia" && proposal.graph.pointer("/cores/egia").is_none() {
            continue;
        }
        let identity = core_setup::identity(paths, role).map_err(|error| {
            diagnostic(
                format!("/cores/{role}"),
                "owner_reader_unavailable",
                error.to_string(),
            )
        })?;
        let supports = identity["supported_graph_versions"]
            .as_array()
            .is_some_and(|versions| versions.iter().any(|version| version == "0.1.0"));
        if !supports {
            return Err(diagnostic(
                format!("/cores/{role}"),
                "owner_reader_unavailable",
                format!("{role} setup does not advertise graph 0.1.0"),
            ));
        }
    }
    let catalog = process::catalog()
        .map_err(|error| diagnostic("/cores/eos", "owner_reader_unavailable", error.to_string()))?;
    if let Some(diagnostic) = graph_compat::admission_diagnostic(&proposal.graph, Some(&catalog)) {
        return Err(diagnostic);
    }
    let edges = edge_admission::admit(&proposal.graph, &catalog)
        .map_err(|error| edge_diagnostic(*error))?;
    if let Some(diagnostic) = cross_core_diagnostic(&proposal.graph) {
        return Err(diagnostic);
    }
    let source = serde_json::to_string(&proposal.graph)
        .map_err(|error| diagnostic("/", "invalid_graph", error.to_string()))?;
    let specs = process::project_with_packages(paths, &source)
        .map_err(|error| diagnostic("/", "invalid_graph", error.to_string()))?;
    preflight::verify_processes(paths, &specs).map_err(|error| {
        diagnostic("/cores/isos", "owner_reader_unavailable", error.to_string())
    })?;
    preflight::verify_new_graph_readers(&specs)
        .map_err(|error| diagnostic("/cores/eos", "owner_reader_unavailable", error.to_string()))?;
    for role in ["eos", "egia"] {
        if role == "egia" && proposal.graph.pointer("/cores/egia").is_none() {
            continue;
        }
        let result = core_setup::call(
            paths,
            name,
            role,
            "validate",
            &proposal.graph,
            &proposal.revision,
        )
        .map_err(|error| {
            diagnostic(
                format!("/cores/{role}"),
                "owner_validation_failed",
                error.to_string(),
            )
        })?;
        if result["ok"] != true {
            let first = &result["diagnostics"][0];
            return Err(diagnostic(
                first["path"].as_str().unwrap_or("/"),
                "owner_validation_failed",
                first["message"].as_str().unwrap_or("owner rejected graph"),
            ));
        }
    }
    let files = core_setup::render_graph(paths, name, &proposal.graph, &proposal.revision)
        .map_err(|error| diagnostic("/", "owner_render_failed", error.to_string()))?;
    Ok(Prepared {
        specs,
        files,
        edges,
    })
}

fn check_digest(value: &str) -> Result<(), MigrationError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(MigrationError::Diagnostic(diagnostic(
            "/revision",
            "invalid_revision",
            "revision must be a lowercase 64-character SHA-256 hex digest",
        )));
    }
    Ok(())
}

pub struct Preview {
    pub proposal: Proposal,
    pub diagnostic: Option<Diagnostic>,
}

pub fn preview(
    paths: &Paths,
    name: &str,
    expected_revision: &str,
) -> Result<Preview, MigrationError> {
    check_digest(expected_revision)?;
    let saved =
        graph_compat::read(&paths.node_graph_path(name)).map_err(MigrationError::Filesystem)?;
    if saved.revision != expected_revision {
        return Err(MigrationError::Conflict(
            "node graph revision changed; reload before migration".into(),
        ));
    }
    let proposal = propose(&saved).map_err(MigrationError::Diagnostic)?;
    let diagnostic = prepare(paths, name, &proposal).err();
    Ok(Preview {
        proposal,
        diagnostic,
    })
}

pub fn commit(
    paths: &Paths,
    name: &str,
    expected_revision: &str,
    expected_proposal: &str,
) -> Result<Proposal, MigrationError> {
    check_digest(expected_revision)?;
    check_digest(expected_proposal)?;
    let _lock = lock::Lock::take(paths).map_err(MigrationError::Filesystem)?;
    let path = paths.node_graph_path(name);
    let saved = graph_compat::read(&path).map_err(MigrationError::Filesystem)?;
    if saved.revision != expected_revision {
        return Err(MigrationError::Conflict(
            "node graph revision changed; reload before migration".into(),
        ));
    }
    let proposal = propose(&saved).map_err(MigrationError::Diagnostic)?;
    if proposal.revision != expected_proposal {
        return Err(MigrationError::Conflict(
            "migration proposal changed; preview again".into(),
        ));
    }
    let prepared = prepare(paths, name, &proposal).map_err(MigrationError::Diagnostic)?;
    core_setup::write_revisioned(
        paths,
        name,
        &proposal.graph,
        &proposal.revision,
        prepared.files,
    )
    .map_err(MigrationError::Filesystem)?;
    config::atomic_write(&path, &proposal.bytes).map_err(MigrationError::Filesystem)?;
    fs::File::open(path.parent().unwrap())
        .and_then(|file| file.sync_all())
        .map_err(|error| MigrationError::OutcomeUncertain(error.into()))?;
    Ok(proposal)
}

pub fn apply_new(
    paths: &Paths,
    name: &str,
    graph: Value,
    expected_revision: &str,
) -> Result<(Proposal, Vec<process::ProcessSpec>), MigrationError> {
    if !expected_revision.is_empty() {
        check_digest(expected_revision)?;
    }
    if graph["version"] != "0.1.0" {
        return Err(MigrationError::Diagnostic(diagnostic(
            "/version",
            "unsupported_graph_version",
            "new writer requires graph 0.1.0",
        )));
    }
    let bytes = serde_json::to_vec_pretty(&graph).map_err(|error| {
        MigrationError::Diagnostic(diagnostic("/", "invalid_graph", error.to_string()))
    })?;
    let revision = graph_compat::revision(&bytes);
    let proposal = Proposal {
        graph,
        bytes,
        revision,
        changes: Vec::new(),
    };
    let _lock = lock::Lock::take(paths).map_err(MigrationError::Filesystem)?;
    let path = paths.node_graph_path(name);
    let previous = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(MigrationError::Filesystem(error.into())),
    };
    if previous
        .as_deref()
        .map(graph_compat::revision)
        .as_deref()
        .unwrap_or("")
        != expected_revision
    {
        return Err(MigrationError::Conflict(
            "node graph revision changed; reload before applying".into(),
        ));
    }
    if let Some(previous) = &previous {
        let saved = graph_compat::parse(previous).map_err(MigrationError::Filesystem)?;
        if saved.value["version"] != "0.1.0" {
            return Err(MigrationError::Diagnostic(diagnostic(
                "/version",
                "migration_required",
                "saved legacy graph requires explicit migration",
            )));
        }
    }
    let prepared = prepare(paths, name, &proposal).map_err(MigrationError::Diagnostic)?;
    if let Some(previous) = &previous {
        let saved = graph_compat::parse(previous).map_err(MigrationError::Filesystem)?;
        if saved.value == proposal.graph {
            core_setup::verify_rendered_snapshot(
                paths,
                name,
                &saved.value,
                &saved.revision,
                &prepared.files,
            )
            .map_err(MigrationError::Filesystem)?;
            core_setup::verify_launch_inputs(paths, name).map_err(MigrationError::Filesystem)?;
            return Ok((
                Proposal {
                    revision: saved.revision,
                    bytes: previous.clone(),
                    ..proposal
                },
                prepared.specs,
            ));
        }
    }
    core_setup::write_revisioned(
        paths,
        name,
        &proposal.graph,
        &proposal.revision,
        prepared.files,
    )
    .map_err(MigrationError::Filesystem)?;
    config::atomic_write(&path, &proposal.bytes).map_err(MigrationError::Filesystem)?;
    fs::File::open(path.parent().unwrap())
        .and_then(|file| file.sync_all())
        .map_err(|error| MigrationError::OutcomeUncertain(error.into()))?;
    Ok((proposal, prepared.specs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_diagnostic_keeps_structured_cli_and_http_context() {
        let graph: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/edge_admission/valid_0_1_0.json"
        ))
        .unwrap();
        let mut catalog: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/edge_admission/catalog.json"
        ))
        .unwrap();
        catalog["types"][0]["outputs"][0]["data_type"] = Value::String("observation_ref".into());
        let error = edge_admission::admit(&graph, &catalog).unwrap_err();
        let diagnostic = edge_diagnostic(*error);
        let value = serde_json::to_value(&diagnostic).unwrap();
        assert_eq!(value["code"], "edge_type_mismatch");
        assert_eq!(value["source"], "eos/hardware/camera/front");
        assert_eq!(value["target"], "eos/interface/gate/preview");
        assert_eq!(value["input"], "frame");
        assert_eq!(value["source_data_type"], "observation_ref");
        assert_eq!(value["target_data_type"], "camera_frame");
        assert_eq!(
            serde_json::from_str::<Value>(&diagnostic.to_string()).unwrap(),
            value
        );
    }

    #[test]
    fn fixture_proposal_is_exact_and_preserves_source_bytes() {
        let source = include_bytes!("../../tests/fixtures/graph_compat/legacy_1_0_0.json");
        let saved = graph_compat::parse(source).unwrap();
        let proposal = propose(&saved).unwrap();
        let expected: Value = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/graph_compat/migration_expected_0_1_0.json"
        ))
        .unwrap();
        assert_eq!(proposal.graph, expected);
        assert_eq!(
            proposal.revision,
            "825d667d31c455844be7f22b565e0ab6b14b44aeb7ce4861a704ffd028081df1"
        );
        assert_eq!(proposal.changes.len(), 5);
        assert_eq!(saved.revision, graph_compat::revision(source));
    }

    #[test]
    fn proposal_preserves_pins_adapter_settings_references_and_edge_metadata() {
        let mut saved = graph_compat::parse(include_bytes!(
            "../../tests/fixtures/graph_compat/legacy_1_0_0.json"
        ))
        .unwrap();
        let path = "/cores/eos/layers/hardware/groups/camera/nodes/front";
        saved.value["cores"]["eos"]["layers"]["hardware"] = serde_json::json!({
            "groups": {"camera": {"nodes": {"front": {
                "id":"front", "node_type":"eos.hardware.camera@v1.0",
                "package":{"id":"island/mujoco-sim","version":"0.1.1"},
                "adapter":{"type":"mujoco-v2","package":{"id":"island/mujoco-sim","version":"0.1.1"}},
                "settings":{"device":"front","frame_id":"camera/front"},
                "references":["cores/eos/layers/process/groups/mujoco/nodes/simulator"],
                "inputs":{"trigger":[{"node":"cores/eos/layers/dispatcher/groups/control/nodes/command",
                    "port":"status","priority":4,"frequency_hz":10.0}]}
            }}}}
        });
        let original = saved.value.pointer(path).unwrap().clone();
        let proposal = propose(&saved).unwrap();
        let converted = proposal.graph.pointer(path).unwrap();
        for field in ["package", "adapter", "settings", "references", "inputs"] {
            assert_eq!(converted[field], original[field], "{field}");
        }
        assert_eq!(converted["node_type"], "eos.hardware.camera@v0.1");
    }

    #[test]
    fn unsafe_unmapped_and_unknown_fields_fail_at_paths() {
        let source = include_bytes!("../../tests/fixtures/graph_compat/legacy_1_0_0.json");
        let mut saved = graph_compat::parse(source).unwrap();
        saved.value["extra"] = Value::Bool(true);
        let diagnostic = propose(&saved).err().unwrap();
        assert_eq!(
            (diagnostic.path.as_str(), diagnostic.code),
            ("/extra", "unmapped_graph_field")
        );
        saved.value.as_object_mut().unwrap().remove("extra");
        saved.value["cores"]["eos"]["layers"]["dispatcher"]["groups"]["control"]["nodes"]["command"]
            ["node_type"] = Value::String("vendor.foo@v1.0".into());
        let diagnostic = propose(&saved).err().unwrap();
        assert_eq!(diagnostic.code, "unmapped_node_type");
        let unsafe_saved = graph_compat::parse(include_bytes!(
            "../../tests/fixtures/graph_compat/unsafe_component_1_0_0.json"
        ))
        .unwrap();
        let diagnostic = propose(&unsafe_saved).err().unwrap();
        assert_eq!(diagnostic.code, "unsafe_process_node");
    }

    #[test]
    fn cross_core_input_reports_the_actual_source_pointer() {
        let base = "/cores/eos/layers/vision/groups/camera/nodes/cam/inputs/frame";
        let mut graph = serde_json::json!({
            "cores": {
                "eos": { "layers": { "vision": { "groups": { "camera": {
                    "nodes": { "cam": { "inputs": {
                        "frame": { "node": "cores/egia/layers/vision/groups/camera/nodes/source" }
                    } } }
                } } } } }
            }
        });
        assert_eq!(cross_core_diagnostic(&graph).unwrap().path, base);
        graph["cores"]["eos"]["layers"]["vision"]["groups"]["camera"]["nodes"]["cam"]["inputs"]["frame"] = serde_json::json!([
            { "node": "cores/eos/layers/vision/groups/camera/nodes/local" },
            { "node": "cores/egia/layers/vision/groups/camera/nodes/source" }
        ]);
        assert_eq!(
            cross_core_diagnostic(&graph).unwrap().path,
            format!("{base}/1")
        );
    }

    #[test]
    fn new_apply_rejects_unowned_core_before_any_write() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        let mut graph: Value = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/graph_compat/migration_expected_0_1_0.json"
        ))
        .unwrap();
        graph["cores"]["evil"] = serde_json::json!({"layers":{}});
        let error = apply_new(&paths, "default", graph, "").err().unwrap();
        let MigrationError::Diagnostic(diagnostic) = error else {
            panic!("unexpected error type");
        };
        assert_eq!(diagnostic.path, "/cores/evil");
        assert_eq!(diagnostic.code, "unsupported_core");
        assert!(!paths.node_graph_path("default").exists());
        assert!(!paths.generated("default").exists());
    }
}
