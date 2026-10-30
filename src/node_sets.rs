//! Package-contributed node sets are composed into the shared graph only on request.

use crate::{
    config::Paths,
    packages,
    release::{NodeSet, Release},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

pub fn installed_release(paths: &Paths, id: &str, version: &str) -> Result<Release> {
    let root = packages::package_path(paths, id, version)?;
    let receipt: packages::Receipt = serde_json::from_slice(&fs::read(root.join("receipt.json"))?)?;
    ensure!(
        receipt.id == id && receipt.version == version,
        "package receipt identity differs"
    );
    let release = Release::read_from_path(&root.join("release.json"))?;
    ensure!(
        release.id == id && release.version == version,
        "package release identity differs"
    );
    Ok(release)
}

pub fn list(paths: &Paths) -> Result<Value> {
    let mut sets = Vec::new();
    for receipt in packages::list(paths)? {
        let release_path =
            packages::package_path(paths, &receipt.id, &receipt.version)?.join("release.json");
        if !release_path.is_file() {
            continue;
        }
        let release = installed_release(paths, &receipt.id, &receipt.version)?;
        if let Some(contributions) = release.node_contributions {
            for set in contributions.node_sets {
                let mut value = serde_json::to_value(set)?;
                value["package"] = json!({"id":release.id,"version":release.version});
                sets.push(value);
            }
        }
    }
    Ok(json!({"sets":sets}))
}

fn substitute(value: &mut Value, parameters: &BTreeMap<String, String>) -> Result<()> {
    match value {
        Value::String(s) => {
            for (key, replacement) in parameters {
                *s = s.replace(&format!("${{{key}}}"), replacement);
            }
            ensure!(!s.contains("${"), "unresolved node set parameter in {s}");
        }
        Value::Array(items) => {
            for item in items {
                substitute(item, parameters)?;
            }
        }
        Value::Object(fields) => {
            for item in fields.values_mut() {
                substitute(item, parameters)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn resolve_refs(value: &mut Value, paths: &BTreeMap<String, String>) -> Result<()> {
    match value {
        Value::String(s) if s.starts_with('@') => {
            let reference = &s[1..];
            *s = paths
                .get(reference)
                .with_context(|| format!("unknown node ref {reference}"))?
                .clone();
        }
        Value::Array(items) => {
            for item in items {
                resolve_refs(item, paths)?;
            }
        }
        Value::Object(fields) => {
            for item in fields.values_mut() {
                resolve_refs(item, paths)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn references_any(value: &Value, paths: &BTreeSet<String>) -> bool {
    match value {
        Value::String(s) => paths.contains(s),
        Value::Array(items) => items.iter().any(|item| references_any(item, paths)),
        Value::Object(fields) => fields.values().any(|item| references_any(item, paths)),
        _ => false,
    }
}

fn graph_nodes(graph: &Value) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    if let Some(cores) = graph["cores"].as_object() {
        for (core_id, core) in cores {
            if let Some(layers) = core["layers"].as_object() {
                for (layer_id, layer) in layers {
                    if let Some(groups) = layer["groups"].as_object() {
                        for (group_id, group) in groups {
                            if let Some(nodes) = group["nodes"].as_object() {
                                for (node_id, profile) in nodes {
                                    out.push((format!("cores/{core_id}/layers/{layer_id}/groups/{group_id}/nodes/{node_id}"), profile.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

fn migrate_legacy_mujoco(graph: &mut Value) -> Result<()> {
    let legacy = [
        (
            "cores/eos/layers/process/groups/backend/nodes/simulator",
            "eos.process.backend@v1.0",
        ),
        (
            "cores/eos/layers/hardware/groups/camera/nodes/front",
            "eos.hardware.camera@v1.0",
        ),
        (
            "cores/eos/layers/hardware/groups/base/nodes/drive",
            "eos.hardware.base@v1.0",
        ),
        (
            "cores/eos/layers/hardware/groups/manipulator/nodes/arm",
            "eos.hardware.manipulator@v1.0",
        ),
    ];
    let mut removed = BTreeSet::new();
    for (path, kind) in legacy {
        if let Some(node) = graph.pointer(&format!("/{path}")) {
            ensure!(
                node["node_type"] == kind,
                "legacy path {path} has a different node type"
            );
            removed.insert(path.to_owned());
        }
    }
    loop {
        let before = removed.len();
        for (path, profile) in graph_nodes(graph) {
            if removed.contains(&path) {
                continue;
            }
            if references_any(&profile, &removed) {
                if path == "cores/eos/layers/dispatcher/groups/control/nodes/command" {
                    continue;
                }
                ensure!(
                    !path.starts_with("cores/isos/"),
                    "IsOS process {path} references legacy MuJoCo hardware"
                );
                removed.insert(path);
            }
        }
        if removed.len() == before {
            break;
        }
    }
    for path in &removed {
        let (parent, id) = path.rsplit_once('/').unwrap();
        graph
            .pointer_mut(&format!("/{parent}"))
            .and_then(Value::as_object_mut)
            .context("legacy node parent disappeared")?
            .remove(id);
    }
    if let Some(dispatcher) =
        graph.pointer_mut("/cores/eos/layers/dispatcher/groups/control/nodes/command")
        && let Some(references) = dispatcher["references"].as_array_mut()
    {
        references.retain(|reference| {
            reference
                .as_str()
                .is_none_or(|path| !removed.contains(path))
        });
    }
    Ok(())
}

#[cfg(test)]
pub fn compose(
    paths: &Paths,
    id: &str,
    version: &str,
    set_id: &str,
    graph: &Value,
    values: &BTreeMap<String, String>,
) -> Result<Value> {
    compose_with_migration(paths, id, version, set_id, graph, values, false)
}

pub fn compose_with_migration(
    paths: &Paths,
    id: &str,
    version: &str,
    set_id: &str,
    graph: &Value,
    values: &BTreeMap<String, String>,
    replace_legacy_mujoco: bool,
) -> Result<Value> {
    let release = installed_release(paths, id, version)?;
    let contributed_types: BTreeSet<_> = release
        .node_contributions
        .as_ref()
        .into_iter()
        .flat_map(|contributions| contributions.node_types.iter())
        .filter_map(|node_type| node_type["id"].as_str())
        .collect();
    let set: &NodeSet = release
        .node_contributions
        .as_ref()
        .and_then(|c| c.node_sets.iter().find(|set| set.id == set_id))
        .with_context(|| format!("node set {set_id} is not installed in {id}@{version}"))?;
    let mut parameters = BTreeMap::new();
    for (name, definition) in &set.parameters {
        let value = values
            .get(name)
            .or(definition.default.as_ref())
            .with_context(|| format!("missing node set parameter {name}"))?;
        parameters.insert(name.clone(), value.clone());
    }
    ensure!(
        values.keys().all(|name| set.parameters.contains_key(name)),
        "unknown node set parameter"
    );
    let mut paths_by_ref = BTreeMap::new();
    let default_group = id.split_once('/').unwrap().1;
    for node in &set.nodes {
        let group = node.group.as_deref().unwrap_or(default_group);
        let instance = node.profile["id"]
            .as_str()
            .context("node profile requires id")?;
        paths_by_ref.insert(
            node.reference.clone(),
            format!(
                "cores/{}/layers/{}/groups/{group}/nodes/{instance}",
                node.core, node.layer
            ),
        );
    }
    let mut draft = graph.clone();
    ensure!(
        draft["version"] == "2.0.0",
        "node graph must be version 2.0.0"
    );
    if replace_legacy_mujoco {
        ensure!(
            id == "island/mujoco-sim",
            "legacy migration is only for MuJoCo"
        );
        migrate_legacy_mujoco(&mut draft)?;
    }
    let mut added_paths = Vec::new();
    let mut seen = BTreeSet::new();
    for node in &set.nodes {
        let path = &paths_by_ref[&node.reference];
        ensure!(seen.insert(path), "node set duplicates path {path}");
        let pointer = format!("/{path}");
        ensure!(
            draft.pointer(&pointer).is_none(),
            "node path already exists: {path}"
        );
        let mut profile = node.profile.clone();
        substitute(&mut profile, &parameters)?;
        resolve_refs(&mut profile, &paths_by_ref)?;
        let package = json!({"id":id,"version":version});
        let node_type = profile["node_type"]
            .as_str()
            .context("node profile requires node_type")?;
        if contributed_types.contains(node_type) {
            ensure!(
                profile
                    .get("package")
                    .is_none_or(|current| current == &package),
                "node package pin differs"
            );
            profile["package"] = package;
        } else {
            ensure!(
                profile.get("package").is_none(),
                "built-in node type cannot carry package pin"
            );
        }
        let segments: Vec<_> = path.split('/').collect();
        let core = draft["cores"]
            .as_object_mut()
            .context("graph lacks cores object")?
            .entry(segments[1])
            .or_insert_with(|| json!({"layers":{}}));
        let layer = core["layers"]
            .as_object_mut()
            .context("core lacks layers object")?
            .entry(segments[3])
            .or_insert_with(|| json!({"groups":{}}));
        let group = layer["groups"]
            .as_object_mut()
            .context("layer lacks groups object")?
            .entry(segments[5])
            .or_insert_with(|| json!({"nodes":{}}));
        group["nodes"]
            .as_object_mut()
            .context("group lacks nodes object")?
            .insert(segments[7].to_owned(), profile);
        added_paths.push(path.clone());
    }
    Ok(json!({"graph":draft,"added_paths":added_paths}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture() -> (tempfile::TempDir, Paths) {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        let bundle = temp.path().join("bundle");
        fs::create_dir(&bundle).unwrap();
        let manifest = temp.path().join("release.json");
        let release = json!({
            "schema_version":4,"id":"test/sim","kind":"backend","version":"2.0.0",
            "source_commit":"a".repeat(40),
            "assets":[{"os":"linux","arch":"x86_64","url":"https://github.com/test/sim/releases/download/v2.0.0/sim.tar.gz","size_bytes":1,"sha256":"b".repeat(64)}],
            "node_contributions":{"node_types":[
                {"id":"eos.process.mujoco@v1.0","core":"eos","layer":"process","group":"mujoco","kind":"software","lifecycle":"process","capabilities":[],"settings":[],"inputs":[],"outputs":[]},
                {"id":"eos.hardware.mujoco_camera@v1.0","core":"eos","layer":"hardware","group":"mujoco","kind":"camera","lifecycle":"adapter","capabilities":[],"settings":[],"inputs":[],"outputs":[]}
            ],"safety_adapters":[],"data_types":[{"id":"test.vision.frame@v1.0","media_type":"application/json","codec":"json_v1","schema":{"type":"object"},"max_payload_bytes":65536}],"node_sets":[{
                "id":"hardware","parameters":{"socket":{"type":"string"}},
                "nodes":[
                    {"ref":"simulator","core":"eos","layer":"process","group":"mujoco","profile":{"id":"simulator","node_type":"eos.process.mujoco@v1.0","settings":{"socket":"${socket}"}}},
                    {"ref":"front","core":"eos","layer":"hardware","group":"mujoco","profile":{"id":"front","node_type":"eos.hardware.mujoco_camera@v1.0","references":["@simulator"],"adapter":{"settings":{"backend":"@simulator","socket_path":"${socket}"}}}},
                    {"ref":"gate","core":"eos","layer":"interface","group":"gate","profile":{"id":"gate","node_type":"eos.interface.camera_gate@v1.0","inputs":{"frame":{"node":"@front","output":"frame"}}}},
                    {"ref":"vision","core":"isosui","layer":"system","group":"ui","profile":{"id":"vision","node_type":"isosui.system.vision@v1.0","inputs":{"preview":{"node":"@gate","output":"preview"}}}}
                ]}]}
        });
        fs::write(&manifest, serde_json::to_vec(&release).unwrap()).unwrap();
        packages::link_with_manifest(
            &paths,
            "test/sim",
            "2.0.0",
            Path::new(&bundle),
            Some(&manifest),
        )
        .unwrap();
        (temp, paths)
    }

    #[test]
    fn compose_resolves_refs_parameters_and_package_pin() {
        let (_temp, paths) = fixture();
        let graph = json!({"version":"2.0.0","robot":{"id":"robot","name":"Robot"},"cores":{}});
        let values = BTreeMap::from([("socket".to_owned(), "/tmp/sim.sock".to_owned())]);
        let preview = compose(&paths, "test/sim", "2.0.0", "hardware", &graph, &values).unwrap();
        assert_eq!(preview["added_paths"].as_array().unwrap().len(), 4);
        assert_eq!(
            preview["graph"]["cores"]["eos"]["layers"]["hardware"]["groups"]["mujoco"]["nodes"]["front"]
                ["adapter"]["settings"]["backend"],
            "cores/eos/layers/process/groups/mujoco/nodes/simulator"
        );
        assert!(preview["graph"]["cores"]["eos"]["layers"]["interface"]["groups"]["gate"]["nodes"]["gate"].get("package").is_none());
        assert!(preview["graph"]["cores"]["isosui"]["layers"]["system"]["groups"]["ui"]["nodes"]["vision"].get("package").is_none());
        assert_eq!(
            preview["graph"]["cores"]["eos"]["layers"]["hardware"]["groups"]["mujoco"]["nodes"]["front"]
                ["package"]["version"],
            "2.0.0"
        );
        assert!(
            compose(
                &paths,
                "test/sim",
                "2.0.0",
                "hardware",
                &preview["graph"],
                &values
            )
            .is_err()
        );
        assert!(
            compose(
                &paths,
                "test/sim",
                "2.0.0",
                "hardware",
                &graph,
                &BTreeMap::new()
            )
            .is_err()
        );
    }

    #[test]
    fn legacy_migration_preserves_unrelated_nodes() {
        let mut graph = json!({"cores":{"eos":{"layers":{
            "hardware":{"groups":{"camera":{"nodes":{"front":{"node_type":"eos.hardware.camera@v1.0"},"other":{"node_type":"eos.hardware.camera@v1.0"}}}}},
            "control":{"groups":{"manual":{"nodes":{"old":{"references":["cores/eos/layers/hardware/groups/camera/nodes/front"]},"other":{"references":["cores/eos/layers/hardware/groups/camera/nodes/other"]}}}}},
            "dispatcher":{"groups":{"control":{"nodes":{"command":{"references":["cores/eos/layers/hardware/groups/camera/nodes/front","cores/eos/layers/hardware/groups/camera/nodes/other"]}}}}}
        }}}});
        migrate_legacy_mujoco(&mut graph).unwrap();
        assert!(
            graph
                .pointer("/cores/eos/layers/hardware/groups/camera/nodes/front")
                .is_none()
        );
        assert!(
            graph
                .pointer("/cores/eos/layers/control/groups/manual/nodes/old")
                .is_none()
        );
        assert!(
            graph
                .pointer("/cores/eos/layers/hardware/groups/camera/nodes/other")
                .is_some()
        );
        assert!(
            graph
                .pointer("/cores/eos/layers/control/groups/manual/nodes/other")
                .is_some()
        );
        assert_eq!(
            graph["cores"]["eos"]["layers"]["dispatcher"]["groups"]["control"]["nodes"]["command"]
                ["references"],
            json!(["cores/eos/layers/hardware/groups/camera/nodes/other"])
        );
    }
}
