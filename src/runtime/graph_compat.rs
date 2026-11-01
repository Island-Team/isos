//! Exact-version inspection of saved deployment graph bytes.

use anyhow::{Result, bail, ensure};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

pub const MAX_GRAPH_BYTES: usize = 1024 * 1024;
pub const SUPPORTED_VERSIONS: [&str; 2] = ["1.0.0", "0.1.0"];

const KNOWN_LEGACY_TYPES: &[&str] = &[
    "eos.hardware.camera@v1.0",
    "eos.hardware.base@v1.0",
    "eos.hardware.manipulator@v1.0",
    "eos.dispatcher.control@v1.0",
    "eos.dispatcher.primitive_base@v1.0",
    "eos.dispatcher.primitive_manipulator@v1.0",
    "eos.control.manual@v1.0",
    "eos.control.primitive@v1.0",
    "eos.control.skill@v1.0",
    "eos.interface.camera_gate@v1.0",
    "egia.common.source@v1.0",
    "egia.common.context@v1.0",
    "egia.common.planner@v1.0",
    "egia.common.guard@v1.0",
    "isos.process.component@v1.1",
    "isos.ui.vision@v1.0",
    "isosui.system.vision@v1.0",
    "eos.process.mujoco@v1.0",
    "eos.hardware.mujoco_camera@v1.0",
    "eos.hardware.mujoco_base@v1.0",
    "eos.hardware.mujoco_manipulator@v1.0",
];
const KNOWN_NEW_TYPES: &[&str] = &[
    "eos.hardware.camera@v0.1",
    "eos.hardware.base@v0.1",
    "eos.hardware.manipulator@v0.1",
    "eos.dispatcher.control@v0.1",
    "eos.dispatcher.primitive_base@v0.1",
    "eos.dispatcher.primitive_manipulator@v0.1",
    "eos.interface.camera_gate@v0.1",
    "eos.process.mujoco@v0.1",
    "isos.process.component@v0.1",
];
const EGIA_TARGET_PASS_THROUGH: &[&str] = &[
    "egia.common.source@v1.0",
    "egia.common.context@v1.0",
    "egia.common.planner@v1.0",
    "egia.common.guard@v1.0",
];

fn target_placement(node_type: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match node_type {
        "eos.hardware.camera@v0.1" => Some(("eos", "hardware", "camera")),
        "eos.hardware.base@v0.1" => Some(("eos", "hardware", "base")),
        "eos.hardware.manipulator@v0.1" => Some(("eos", "hardware", "manipulator")),
        "eos.dispatcher.control@v0.1" => Some(("eos", "dispatcher", "control")),
        "eos.dispatcher.primitive_base@v0.1" => Some(("eos", "dispatcher", "base")),
        "eos.dispatcher.primitive_manipulator@v0.1" => Some(("eos", "dispatcher", "manipulator")),
        "eos.interface.camera_gate@v0.1" => Some(("eos", "interface", "gate")),
        "eos.process.mujoco@v0.1" => Some(("eos", "process", "mujoco")),
        "isos.process.component@v0.1" => Some(("isos", "process", "component")),
        "egia.common.source@v1.0" | "egia.common.context@v1.0" => Some(("egia", "common", "data")),
        "egia.common.planner@v1.0" | "egia.common.guard@v1.0" => {
            Some(("egia", "common", "process"))
        }
        _ => None,
    }
}

pub fn is_isos_target_type(node_type: &str) -> bool {
    node_type == "isos.process.component@v0.1"
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: String,
    pub code: &'static str,
    pub message: String,
    #[serde(flatten)]
    pub context: BTreeMap<String, String>,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.context.is_empty() {
            write!(formatter, "{}: {}: {}", self.path, self.code, self.message)
        } else {
            write!(
                formatter,
                "{}",
                serde_json::to_string(self).map_err(|_| std::fmt::Error)?
            )
        }
    }
}

#[derive(Debug)]
pub struct SavedGraph {
    pub value: Value,
    pub revision: String,
    pub read_only: bool,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn revision(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn read(path: &Path) -> Result<SavedGraph> {
    let bytes = fs::read(path)?;
    parse(&bytes)
}

pub fn parse(bytes: &[u8]) -> Result<SavedGraph> {
    ensure!(bytes.len() <= MAX_GRAPH_BYTES, "node graph exceeds 1 MiB");
    let value: Value = serde_json::from_slice(bytes)?;
    ensure!(value.is_object(), "node graph must be a JSON object");
    let diagnostics = version_diagnostic(&value).into_iter().collect();
    Ok(SavedGraph {
        revision: revision(bytes),
        read_only: true,
        value,
        diagnostics,
    })
}

pub fn version_diagnostic(graph: &Value) -> Option<Diagnostic> {
    let version = graph.get("version").and_then(Value::as_str);
    if version.is_some_and(|version| SUPPORTED_VERSIONS.contains(&version)) {
        return None;
    }
    Some(Diagnostic {
        path: "/version".into(),
        code: "unsupported_graph_version",
        message: format!(
            "graph version {} is unsupported; supported graph versions: {}",
            version.unwrap_or("<missing or invalid>"),
            SUPPORTED_VERSIONS.join(", ")
        ),
        context: BTreeMap::new(),
    })
}

pub fn admission_diagnostic(graph: &Value, catalog: Option<&Value>) -> Option<Diagnostic> {
    if let Some(diagnostic) = version_diagnostic(graph) {
        return Some(diagnostic);
    }
    let target = graph["version"] == "0.1.0";
    let cores = match graph.get("cores").and_then(Value::as_object) {
        Some(cores) => cores,
        None => {
            return Some(Diagnostic {
                path: "/cores".into(),
                code: "invalid_graph",
                message: "cores must be an object".into(),
                context: BTreeMap::new(),
            });
        }
    };
    for (core_id, core) in cores {
        if target && !matches!(core_id.as_str(), "eos" | "isos" | "egia") {
            return Some(Diagnostic {
                path: format!("/cores/{core_id}"),
                code: "unsupported_core",
                message: format!("core {core_id} has no 0.1.0 owner reader"),
                context: BTreeMap::new(),
            });
        }
        let Some(layers) = core.get("layers").and_then(Value::as_object) else {
            continue;
        };
        for (layer_id, layer) in layers {
            let Some(groups) = layer.get("groups").and_then(Value::as_object) else {
                continue;
            };
            for (group_id, group) in groups {
                let Some(nodes) = group.get("nodes").and_then(Value::as_object) else {
                    continue;
                };
                for (node_id, node) in nodes {
                    let path = format!(
                        "/cores/{core_id}/layers/{layer_id}/groups/{group_id}/nodes/{node_id}"
                    );
                    let node_type = node.get("node_type").and_then(Value::as_str).unwrap_or("");
                    if target {
                        let Some((expected_core, expected_layer, expected_group)) =
                            target_placement(node_type)
                        else {
                            return Some(Diagnostic {
                                path: format!("{path}/node_type"),
                                code: "unsupported_node_type",
                                message: format!(
                                    "node type {node_type} is not approved for graph 0.1.0"
                                ),
                                context: BTreeMap::new(),
                            });
                        };
                        if (core_id.as_str(), layer_id.as_str(), group_id.as_str())
                            != (expected_core, expected_layer, expected_group)
                        {
                            return Some(Diagnostic {
                                path: format!("{path}/node_type"),
                                code: "invalid_node_placement",
                                message: format!(
                                    "{node_type} belongs at {expected_core}/{expected_layer}/{expected_group}"
                                ),
                                context: BTreeMap::new(),
                            });
                        }
                    }
                    if node_type == "isos.process.component@v1.0" {
                        return Some(Diagnostic {
                            path: format!("{path}/node_type"),
                            code: "unsafe_process_node",
                            message: "arbitrary-command process nodes cannot be admitted; use a role-selected component".into(),
                            context: BTreeMap::new(),
                        });
                    }
                    if core_id == "isos"
                        && layer_id == "process"
                        && group_id == "component"
                        && node.pointer("/settings/command").is_some()
                    {
                        return Some(Diagnostic {
                            path: format!("{path}/settings/command"),
                            code: "unsafe_process_node",
                            message:
                                "settings.command cannot be admitted; use role and bounded args"
                                    .into(),
                            context: BTreeMap::new(),
                        });
                    }
                    if node_type == "eos.process.backend@v1.0" {
                        return Some(Diagnostic {
                            path: format!("{path}/node_type"),
                            code: "migration_required",
                            message:
                                "legacy backend needs an exact verified replacement before Apply"
                                    .into(),
                            context: BTreeMap::new(),
                        });
                    }
                    let registered = catalog.map_or_else(
                        || {
                            KNOWN_LEGACY_TYPES.contains(&node_type)
                                || KNOWN_NEW_TYPES.contains(&node_type)
                        },
                        |catalog| {
                            is_isos_target_type(node_type)
                                || (graph["version"] == "0.1.0"
                                    && (KNOWN_NEW_TYPES.contains(&node_type)
                                        || EGIA_TARGET_PASS_THROUGH.contains(&node_type)))
                                || catalog["types"].as_array().is_some_and(|types| {
                                    types.iter().any(|item| item["id"] == node_type)
                                })
                        },
                    );
                    if !registered {
                        return Some(Diagnostic {
                            path: format!("{path}/node_type"),
                            code: "unsupported_node_type",
                            message: format!("node type {node_type} is not registered"),
                            context: BTreeMap::new(),
                        });
                    }
                }
            }
        }
    }
    None
}

pub fn admit(graph: &Value, catalog: Option<&Value>) -> Result<()> {
    if let Some(diagnostic) = admission_diagnostic(graph, catalog) {
        bail!(diagnostic);
    }
    Ok(())
}

pub fn changed_legacy_diagnostic(saved: &SavedGraph, incoming: &Value) -> Option<Diagnostic> {
    (saved.value["version"] == "1.0.0" && saved.value != *incoming).then(|| Diagnostic {
        path: "/version".into(),
        code: "migration_required",
        message: "editing a saved 1.0.0 graph requires explicit migration".into(),
        context: BTreeMap::new(),
    })
}

pub fn requires_migration(saved: &SavedGraph, incoming: &Value, applied: bool) -> bool {
    applied && changed_legacy_diagnostic(saved, incoming).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: &[u8] = include_bytes!("../../tests/fixtures/graph_compat/legacy_1_0_0.json");
    const WHITESPACE: &[u8] =
        include_bytes!("../../tests/fixtures/graph_compat/legacy_1_0_0_whitespace.json");
    const FUTURE: &[u8] = include_bytes!("../../tests/fixtures/graph_compat/future_2_0_0.json");
    const UNKNOWN_TYPE: &[u8] =
        include_bytes!("../../tests/fixtures/graph_compat/unknown_type_1_0_0.json");
    const UNSAFE: &[u8] =
        include_bytes!("../../tests/fixtures/graph_compat/unsafe_component_1_0_0.json");

    #[test]
    fn frozen_bytes_have_distinct_original_revisions() {
        let legacy = parse(LEGACY).unwrap();
        let whitespace = parse(WHITESPACE).unwrap();
        assert_eq!(legacy.value, whitespace.value);
        assert_ne!(LEGACY, WHITESPACE);
        assert_eq!(
            legacy.revision,
            "7a8daee6b7db9105f5483506ffc1743625b63089710dfbf936622329c471016b"
        );
        assert_eq!(
            whitespace.revision,
            "27e137a1387393d7fb2e1abc1b735315cbdefd47a02ae2b265fa3596720158e9"
        );
        assert!(legacy.read_only);
    }

    #[test]
    fn future_and_unsafe_inputs_have_exact_paths() {
        let future = parse(FUTURE).unwrap();
        assert_eq!(future.diagnostics[0].path, "/version");
        assert_eq!(future.diagnostics[0].code, "unsupported_graph_version");
        let unsafe_graph = parse(UNSAFE).unwrap();
        let unsafe_diagnostic = admission_diagnostic(&unsafe_graph.value, None).unwrap();
        assert_eq!(
            unsafe_diagnostic.path,
            "/cores/isos/layers/process/groups/component/nodes/eos/node_type"
        );
        assert_eq!(unsafe_diagnostic.code, "unsafe_process_node");
        let catalog = serde_json::json!({"types":[]});
        let unknown = parse(UNKNOWN_TYPE).unwrap();
        let type_diagnostic = admission_diagnostic(&unknown.value, Some(&catalog)).unwrap();
        assert_eq!(
            type_diagnostic.path,
            "/cores/eos/layers/dispatcher/groups/control/nodes/command/node_type"
        );
        assert_eq!(type_diagnostic.code, "unsupported_node_type");
        let mut invented = parse(LEGACY).unwrap().value;
        invented["cores"]["eos"]["layers"]["dispatcher"]["groups"]["control"]["nodes"]["command"]
            ["node_type"] = Value::String("vendor.foo@v1.0".into());
        let invented_diagnostic = admission_diagnostic(&invented, None).unwrap();
        assert_eq!(
            invented_diagnostic.path,
            "/cores/eos/layers/dispatcher/groups/control/nodes/command/node_type"
        );
        assert_eq!(invented_diagnostic.code, "unsupported_node_type");
    }

    #[test]
    fn malformed_and_oversized_inputs_fail_closed() {
        assert!(parse(b"not JSON").is_err());
        assert!(parse(br#"{"version":"1.0.0"}"#).is_ok());
        assert!(parse(&vec![b' '; MAX_GRAPH_BYTES + 1]).is_err());
        assert_eq!(
            version_diagnostic(&serde_json::json!({})).unwrap().path,
            "/version"
        );
    }

    #[test]
    fn target_graph_rejects_unknown_core_legacy_ui_and_misplaced_type() {
        let source =
            include_bytes!("../../tests/fixtures/graph_compat/migration_expected_0_1_0.json");
        let mut graph = parse(source).unwrap().value;
        graph["cores"]["evil"] = serde_json::json!({"layers":{}});
        let diagnostic = admission_diagnostic(&graph, None).unwrap();
        assert_eq!(
            (diagnostic.path.as_str(), diagnostic.code),
            ("/cores/evil", "unsupported_core")
        );
        graph["cores"].as_object_mut().unwrap().remove("evil");
        let node = &mut graph["cores"]["eos"]["layers"]["dispatcher"]["groups"]["control"]["nodes"]
            ["command"];
        node["node_type"] = Value::String("isos.ui.vision@v1.0".into());
        let diagnostic = admission_diagnostic(&graph, None).unwrap();
        assert_eq!(diagnostic.code, "unsupported_node_type");
        assert_eq!(
            diagnostic.path,
            "/cores/eos/layers/dispatcher/groups/control/nodes/command/node_type"
        );
        graph["cores"]["eos"]["layers"]["dispatcher"]["groups"]["control"]["nodes"]["command"]["node_type"] =
            Value::String("eos.hardware.camera@v0.1".into());
        let diagnostic = admission_diagnostic(&graph, None).unwrap();
        assert_eq!(diagnostic.code, "invalid_node_placement");
        assert_eq!(
            diagnostic.path,
            "/cores/eos/layers/dispatcher/groups/control/nodes/command/node_type"
        );
    }

    #[test]
    fn legacy_edit_comparison_uses_structure_without_rewriting_bytes() {
        let saved = parse(WHITESPACE).unwrap();
        let same = parse(LEGACY).unwrap();
        assert!(changed_legacy_diagnostic(&saved, &same.value).is_none());
        let mut changed = same.value;
        changed["robot"]["name"] = Value::String("Changed".into());
        assert_eq!(
            changed_legacy_diagnostic(&saved, &changed).unwrap().code,
            "migration_required"
        );
        assert_eq!(saved.revision, revision(WHITESPACE));
        assert!(!requires_migration(&saved, &changed, false));
        assert!(requires_migration(&saved, &changed, true));
    }
}
