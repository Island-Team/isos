//! Selector options derived from the EOS registry and the current draft graph.
use crate::{config::Paths, graph_compat, mujoco_catalog, packages, process};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn device_kind<'a>(catalog: &'a Value, node_type: &str) -> Option<&'a str> {
    catalog["types"].as_array()?.iter().find(|entry| {
        entry["id"] == node_type && entry["core"] == "eos" && entry["layer"] == "hardware"
    })?["kind"]
        .as_str()
}

pub fn compose(graph: &Value, catalog: &Value) -> Result<Value> {
    ensure!(
        graph["version"] == "1.0.0" || graph["version"] == "0.1.0",
        "node graph must be version 1.0.0 or 0.1.0"
    );
    let families = catalog["adapter_families"]
        .as_array()
        .context("catalog lacks adapter_families")?;
    let specs = catalog["primitive_specs"]
        .as_array()
        .context("catalog lacks primitive_specs")?;
    let mut adapter_families = Vec::new();
    for family in families {
        let compiled = family["compiled"].as_bool().unwrap_or(false);
        let reason = if compiled {
            Value::Null
        } else {
            family.get("reason").cloned().unwrap_or(json!(
                "adapter is not compiled into the selected EOS runtime"
            ))
        };
        adapter_families.push(json!({
            "id":family["id"],"device_kinds":family["device_kinds"],"package":family.get("package"),
            "compiled":compiled,"transport":family["transport"],"description":family["description"],
            "supported_primitives":family.get("supported_primitives"),"available":compiled,"reason":reason
        }));
    }
    let mut primitive_bindings = Vec::new();
    for (kind, node_type) in [
        ("base", "eos.dispatcher.primitive_base@v1.0"),
        ("manipulator", "eos.dispatcher.primitive_manipulator@v1.0"),
    ] {
        let primitives: Vec<_> = specs.iter().filter(|spec| spec["device_class"] == kind).map(|spec| json!({
            "name":spec["name"],"version":spec["version"],"movement_type":spec["movement_type"],"description":spec["description"]
        })).collect();
        let available = catalog["types"]
            .as_array()
            .is_some_and(|types| types.iter().any(|entry| entry["id"] == node_type))
            && !primitives.is_empty();
        let movement_types: Vec<_> = primitives
            .iter()
            .map(|p| p["movement_type"].clone())
            .collect();
        primitive_bindings.push(json!({
            "node_type":node_type,"device_kind":kind,"movement_types":movement_types,
            "available":available,"reason":if available {Value::Null} else {json!("binding type or primitive implementation is unavailable")},"primitives":primitives
        }));
    }
    let mut targets = Vec::new();
    if let Some(cores) = graph["cores"].as_object() {
        for (core_id, core) in cores {
            if let Some(layers) = core["layers"].as_object() {
                for (layer_id, layer) in layers {
                    if let Some(groups) = layer["groups"].as_object() {
                        for (group_id, group) in groups {
                            if let Some(nodes) = group["nodes"].as_object() {
                                for (node_id, node) in nodes {
                                    let Some(kind) = node["node_type"]
                                        .as_str()
                                        .and_then(|id| device_kind(catalog, id))
                                    else {
                                        continue;
                                    };
                                    let adapter_type =
                                        node["adapter"]["type"].as_str().unwrap_or("");
                                    let family = adapter_families
                                        .iter()
                                        .find(|family| family["id"] == adapter_type);
                                    let available = family.is_some_and(|family| {
                                        family["available"] == true
                                            && family["device_kinds"].as_array().is_some_and(
                                                |kinds| kinds.iter().any(|entry| entry == kind),
                                            )
                                    });
                                    let movement_types: Vec<_> = specs
                                        .iter()
                                        .filter(|spec| {
                                            if spec["device_class"] != kind {
                                                return false;
                                            }
                                            family.is_none_or(|family| {
                                                family["supported_primitives"]
                                                    .as_array()
                                                    .is_none_or(|supported| {
                                                        let identity = format!(
                                                            "{}@{}",
                                                            spec["name"].as_str().unwrap_or(""),
                                                            spec["version"].as_str().unwrap_or("")
                                                        );
                                                        supported
                                                            .iter()
                                                            .any(|entry| entry == &identity)
                                                    })
                                            })
                                        })
                                        .map(|spec| spec["movement_type"].clone())
                                        .collect();
                                    targets.push(json!({
                                        "path":format!("{core_id}/{layer_id}/{group_id}/{node_id}"),
                                        "node_type":node["node_type"],"device_kind":kind,"adapter_type":adapter_type,
                                        "movement_types":movement_types,"available":available,
                                        "reason":if available {Value::Null} else {json!("selected adapter is unavailable or incompatible with device kind")}
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(
        json!({"adapter_families":adapter_families,"primitive_bindings":primitive_bindings,"targets":targets}),
    )
}

fn pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn stable_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn option_error(code: &'static str, path: String, node: &Value) -> anyhow::Error {
    let pin = if path.contains("/adapter/") {
        &node["adapter"]["package"]
    } else {
        &node["package"]
    };
    packages::PackageFailure {
        code,
        path,
        stage: "admission",
        id: pin["id"].as_str().unwrap_or("island/mujoco-sim").to_owned(),
        version: pin["version"].as_str().unwrap_or("0.1.1").to_owned(),
    }
    .into()
}

fn selected(node: &Value, setting: &str, path: &str) -> Result<Option<String>> {
    let value = match setting {
        "scene" => node.pointer("/settings/scene"),
        "adapter.settings.device_id" => node.pointer("/adapter/settings/device_id"),
        "adapter.settings.simulator_node" => node.pointer("/adapter/settings/simulator_node"),
        _ => None,
    };
    match value {
        None => Ok(None),
        Some(Value::String(id)) if stable_id(id) => Ok(Some(id.clone())),
        Some(_) => Err(option_error(
            "node_option_invalid_id",
            path.to_owned(),
            node,
        )),
    }
}

fn group(
    node_path: &str,
    setting: &str,
    selected: Option<String>,
    mut options: Vec<Value>,
    absent_code: &str,
) -> Value {
    if let Some(id) = &selected
        && !options.iter().any(|row| row["value"] == *id)
    {
        options.push(json!({"value":id,"label":id,"description":"Saved value is not in the verified catalog",
            "available":false,"reason_code":absent_code,"reason":"Select a verified available value"}));
    }
    options.sort_by(|left, right| left["value"].as_str().cmp(&right["value"].as_str()));
    options.dedup_by(|left, right| left["value"] == right["value"]);
    json!({"node_path":node_path,"setting":setting,"selected":selected,"options":options})
}

fn reason(code: &str) -> String {
    match code {
        "package_missing_file" => "Install and verify the selected package version",
        "package_changed" => "Relink the changed package release",
        "mujoco_library_unavailable" => "Install the required MuJoCo host library",
        "adapter_unavailable" => "Choose an EOS runtime with this adapter compiled",
        _ => "Select a verified available value",
    }
    .to_owned()
}

/// Project selectors from the current exact draft and exact verified package pins.
pub fn compose_with_paths(paths: &Paths, graph: &Value, catalog: &Value) -> Result<Value> {
    if let Some(diagnostic) = graph_compat::admission_diagnostic(graph, Some(catalog)) {
        anyhow::bail!(diagnostic);
    }
    let mut data = compose(graph, catalog)?;
    let mut groups: BTreeMap<(String, String), Value> = BTreeMap::new();
    if graph["version"] == "0.1.0" {
        let mut process_nodes = BTreeMap::new();
        let mut nodes = Vec::new();
        if let Some(cores) = graph["cores"].as_object() {
            for (core_id, core) in cores {
                if let Some(layers) = core["layers"].as_object() {
                    for (layer_id, layer) in layers {
                        if let Some(families) = layer["groups"].as_object() {
                            for (family_id, family) in families {
                                if let Some(instances) = family["nodes"].as_object() {
                                    for (node_id, node) in instances {
                                        let short =
                                            format!("{core_id}/{layer_id}/{family_id}/{node_id}");
                                        let pointer = format!(
                                            "/cores/{}/layers/{}/groups/{}/nodes/{}",
                                            pointer_segment(core_id),
                                            pointer_segment(layer_id),
                                            pointer_segment(family_id),
                                            pointer_segment(node_id)
                                        );
                                        if node["node_type"] == "eos.process.mujoco@v0.1" {
                                            process_nodes.insert(short.clone(), node);
                                        }
                                        nodes.push((short, pointer, family_id.as_str(), node));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // The process type is package-contributed. A damaged exact pin still
        // produces a repairable unavailable selected row.
        for (short, pointer, kind, node) in nodes {
            if let Some(definition) = catalog["types"]
                .as_array()
                .and_then(|types| types.iter().find(|entry| entry["id"] == node["node_type"]))
                && let Some(settings) = definition["settings"].as_array()
            {
                for setting in settings {
                    if let Some(selector) = setting.get("selector") {
                        let name = setting["name"].as_str().unwrap_or("");
                        let allowed = node["node_type"] == "eos.process.mujoco@v0.1"
                            && name == "scene"
                            && selector
                                == &json!({"provider":"package","resource":"scene","package_from":"node.package"});
                        if !allowed {
                            return Err(option_error(
                                "node_option_selector_invalid",
                                format!("{pointer}/settings/{}", pointer_segment(name)),
                                node,
                            ));
                        }
                    }
                }
            }
            if node["node_type"] == "eos.process.mujoco@v0.1" {
                let scene = selected(node, "scene", &format!("{pointer}/settings/scene"))?;
                let id = node["package"]["id"].as_str().unwrap_or("");
                let version = node["package"]["version"].as_str().unwrap_or("");
                let (rows, absent) = match packages::verify(paths, id, version) {
                    Ok(verified) => match mujoco_catalog::inspect_verified(&verified) {
                        Ok(found) if found.mode == mujoco_catalog::Mode::Target => {
                            let library =
                                mujoco_catalog::check_library(&found, node["settings"].get("env"))
                                    .err()
                                    .map(|issue| issue.code);
                            let rows = found.descriptor.scenes.iter().map(|entry| {
                                let code = library;
                                json!({"value":entry.id,"label":entry.label,"description":entry.description,
                                    "available":code.is_none(),"reason_code":code,"reason":code.map(reason)})
                            }).collect();
                            (rows, "mujoco_scene_unavailable")
                        }
                        Ok(_) => (Vec::new(), "mujoco_catalog_incomplete"),
                        Err(issue) => (Vec::new(), issue.code),
                    },
                    Err(error) => {
                        let code = error
                            .chain()
                            .find_map(|cause| cause.downcast_ref::<packages::PackageFailure>())
                            .map_or("package_missing_file", |failure| failure.code);
                        (Vec::new(), code)
                    }
                };
                groups.insert(
                    (short.clone(), "scene".into()),
                    group(&short, "scene", scene, rows, absent),
                );
            } else if matches!(kind, "camera" | "base" | "manipulator")
                && node["node_type"] == format!("eos.hardware.{kind}@v0.1")
                && node["adapter"]["type"] == "mujoco-v2"
            {
                let backend = node["adapter"]["settings"]["backend"]
                    .as_str()
                    .unwrap_or("");
                let process_node = process_nodes.get(backend).copied();
                let pin = &node["adapter"]["package"];
                let id = pin["id"].as_str().unwrap_or("");
                let version = pin["version"].as_str().unwrap_or("");
                let (rows, absent) = match packages::verify(paths, id, version) {
                    Ok(verified) => match mujoco_catalog::inspect_verified(&verified) {
                        Ok(found) if found.mode == mujoco_catalog::Mode::Target => {
                            let scene = process_node
                                .and_then(|process| process["settings"]["scene"].as_str())
                                .unwrap_or("");
                            let pin_matches =
                                process_node.is_some_and(|process| process["package"] == *pin);
                            let reference_matches =
                                node["references"].as_array().is_some_and(|refs| {
                                    refs.iter().any(|reference| reference == backend)
                                });
                            let compiled =
                                catalog["adapter_families"]
                                    .as_array()
                                    .is_some_and(|families| {
                                        families.iter().any(|family| {
                                            family["id"] == "mujoco-v2"
                                                && family["compiled"] == true
                                        })
                                    });
                            let library = mujoco_catalog::check_library(
                                &found,
                                process_node.and_then(|process| process["settings"].get("env")),
                            )
                            .err()
                            .map(|issue| issue.code);
                            let rows = found.devices.iter().filter(|entry| entry.kind == kind).map(|entry| {
                                let code = if !pin_matches || !reference_matches || !entry.scene_ids.iter().any(|item| item == scene) {Some("mujoco_device_unavailable")}
                                    else if let Some(code) = library {Some(code)}
                                    else if !compiled {Some("adapter_unavailable")} else {None};
                                json!({"value":entry.id,"label":entry.label,"description":entry.description,
                                    "available":code.is_none(),"reason_code":code,"reason":code.map(reason)})
                            }).collect();
                            (rows, "mujoco_device_unavailable")
                        }
                        Ok(_) => (Vec::new(), "mujoco_catalog_incomplete"),
                        Err(issue) => (Vec::new(), issue.code),
                    },
                    Err(error) => {
                        let code = error
                            .chain()
                            .find_map(|cause| cause.downcast_ref::<packages::PackageFailure>())
                            .map_or("package_missing_file", |failure| failure.code);
                        (Vec::new(), code)
                    }
                };
                for (setting, suffix) in [
                    ("adapter.settings.device_id", "device_id"),
                    ("adapter.settings.simulator_node", "simulator_node"),
                ] {
                    let value = selected(
                        node,
                        setting,
                        &format!("{pointer}/adapter/settings/{suffix}"),
                    )?;
                    groups.insert(
                        (short.clone(), setting.into()),
                        group(&short, setting, value, rows.clone(), absent),
                    );
                }
            }
        }
    }
    data["setting_options"] = json!(groups.into_values().collect::<Vec<_>>());
    Ok(data)
}

pub fn for_graph(paths: &Paths, graph: &Value) -> Result<Value> {
    compose_with_paths(paths, graph, &process::catalog()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selector_uses_short_paths_and_availability() {
        let graph = json!({"version":"1.0.0","cores":{"eos":{"layers":{"hardware":{"groups":{"base":{"nodes":{"drive":{"node_type":"eos.hardware.base@v1.0","adapter":{"type":"mujoco-v2"}}}}}}}}}});
        let catalog = json!({"types":[{"id":"eos.dispatcher.primitive_base@v1.0"},{"id":"eos.hardware.base@v1.0","core":"eos","layer":"hardware","kind":"base"}],"adapter_families":[{"id":"mujoco-v2","device_kinds":["base"],"transport":"unix_socket","description":"sim","compiled":true}],"primitive_specs":[{"name":"base.velocity","version":"1.0.0","device_class":"base","movement_type":"body_velocity","description":"velocity"}]});
        let options = compose(&graph, &catalog).unwrap();
        assert_eq!(options["targets"][0]["path"], "eos/hardware/base/drive");
        assert_eq!(options["targets"][0]["available"], true);
        assert_eq!(
            options["primitive_bindings"][0]["primitives"][0]["name"],
            "base.velocity"
        );
    }

    #[test]
    fn selector_declaration_and_ids_fail_at_exact_pointer() {
        let temporary = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temporary.path().join("config"),
            data: temporary.path().join("data"),
            runtime: temporary.path().join("run"),
        };
        let mut graph = json!({"version":"0.1.0","cores":{"eos":{"layers":{"process":{"groups":{"mujoco":{"nodes":{"simulator":{
            "node_type":"eos.process.mujoco@v0.1","package":{"id":"island/mujoco-sim","version":"0.1.1"},
            "settings":{"scene":"pick-place"}}}}}}}}}});
        let catalog = json!({"types":[{"id":"eos.process.mujoco@v0.1",
            "settings":[{"name":"scene","selector":{"provider":"package","resource":"unknown-resource","package_from":"node.package"}}]}],
            "adapter_families":[],"primitive_specs":[]});
        let error = compose_with_paths(&paths, &graph, &catalog).unwrap_err();
        let failure = error.downcast_ref::<packages::PackageFailure>().unwrap();
        assert_eq!(failure.code, "node_option_selector_invalid");
        assert_eq!(
            failure.path,
            "/cores/eos/layers/process/groups/mujoco/nodes/simulator/settings/scene"
        );
        graph["cores"]["eos"]["layers"]["process"]["groups"]["mujoco"]["nodes"]["simulator"]["settings"]
            ["scene"] = json!("/tmp/secret.xml");
        let valid_catalog = json!({"types":[{"id":"eos.process.mujoco@v0.1",
            "settings":[{"name":"scene","selector":{"provider":"package","resource":"scene","package_from":"node.package"}}]}],
            "adapter_families":[],"primitive_specs":[]});
        let error = compose_with_paths(&paths, &graph, &valid_catalog).unwrap_err();
        let failure = error.downcast_ref::<packages::PackageFailure>().unwrap();
        assert_eq!(failure.code, "node_option_invalid_id");
        assert_eq!(
            failure.path,
            "/cores/eos/layers/process/groups/mujoco/nodes/simulator/settings/scene"
        );
        graph["cores"]["eos"]["layers"]["process"]["groups"]["mujoco"]["nodes"]["simulator"]["settings"]
            ["scene"] = Value::Null;
        let error = compose_with_paths(&paths, &graph, &valid_catalog).unwrap_err();
        let failure = error.downcast_ref::<packages::PackageFailure>().unwrap();
        assert_eq!(failure.code, "node_option_invalid_id");
    }
}
