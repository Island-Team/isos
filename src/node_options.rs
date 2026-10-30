//! Selector options derived from the EOS registry and the current draft graph.
use crate::v2_process;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

fn device_kind<'a>(catalog: &'a Value, node_type: &str) -> Option<&'a str> {
    catalog["types"].as_array()?.iter().find(|entry| {
        entry["id"] == node_type && entry["core"] == "eos" && entry["layer"] == "hardware"
    })?["kind"]
        .as_str()
}

pub fn compose(graph: &Value, catalog: &Value) -> Result<Value> {
    ensure!(
        graph["version"] == "2.0.0",
        "node graph must be version 2.0.0"
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

pub fn for_graph(graph: &Value) -> Result<Value> {
    compose(graph, &v2_process::catalog()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selector_uses_short_paths_and_availability() {
        let graph = json!({"version":"2.0.0","cores":{"eos":{"layers":{"hardware":{"groups":{"base":{"nodes":{"drive":{"node_type":"eos.hardware.base@v1.0","adapter":{"type":"mujoco-v2"}}}}}}}}}});
        let catalog = json!({"types":[{"id":"eos.dispatcher.primitive_base@v1.0"},{"id":"eos.hardware.base@v1.0","core":"eos","layer":"hardware","kind":"base"}],"adapter_families":[{"id":"mujoco-v2","device_kinds":["base"],"transport":"unix_socket","description":"sim","compiled":true}],"primitive_specs":[{"name":"base.velocity","version":"1.0.0","device_class":"base","movement_type":"body_velocity","description":"velocity"}]});
        let options = compose(&graph, &catalog).unwrap();
        assert_eq!(options["targets"][0]["path"], "eos/hardware/base/drive");
        assert_eq!(options["targets"][0]["available"], true);
        assert_eq!(
            options["primitive_bindings"][0]["primitives"][0]["name"],
            "base.velocity"
        );
    }
}
