//! Human-facing readiness for configured core executables.

use crate::{config::Paths, core_bins, core_setup, isosui, process};
use serde_json::{Value, json};
use std::fs;

fn process_present(graph: &Value, id: &str) -> bool {
    graph
        .pointer(&format!(
            "/cores/isos/layers/process/groups/component/nodes/{id}"
        ))
        .is_some()
}

pub fn report(paths: &Paths, name: &str, active: Option<&Value>) -> Value {
    let graph_path = paths.node_graph_path(name);
    let graph: Value = fs::read(&graph_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null);
    let graph_version_issue = graph
        .is_object()
        .then(|| {
            (graph["version"] != "1.0.0").then(|| {
                format!(
                    "saved graph version {} is incompatible with graph format 1.0.0",
                    graph["version"]
                )
            })
        })
        .flatten();
    let projected = if graph.is_object() {
        process::project_with_packages(paths, &graph.to_string()).ok()
    } else {
        None
    };
    let endpoints = graph
        .is_object()
        .then(|| core_setup::endpoints(paths, name, &graph))
        .transpose();
    let cores: Vec<Value> = ["eos", "egia"]
        .into_iter()
        .map(|core| {
            let explicit = match core {
                "eos" => std::env::var_os("ISOS_AETHER_BIN_DIR").is_some(),
                _ => std::env::var_os("ISOS_EGIA_BIN_DIR").is_some(),
            };
            let path = if core == "eos" {
                core_bins::resolve("aether-profile")
            } else {
                core_setup::adapter_path(paths, core)
            };
            let enabled = process_present(&graph, core);
            let graph_configured = enabled
                && paths
                    .generated(name)
                    .join("core-setup/launch-inputs.json")
                    .is_file();
            let running = active
                .and_then(|value| value.get("processes"))
                .and_then(Value::as_array)
                .is_some_and(|processes| {
                    processes
                        .iter()
                        .any(|process| process["id"] == core && process["state"] == "running")
                });
            let running_changed = running
                && projected.as_ref().is_some_and(|specs| {
                    specs
                        .iter()
                        .find(|spec| spec.id == core)
                        .is_some_and(|spec| {
                            active
                                .and_then(|value| value.get("processes"))
                                .and_then(Value::as_array)
                                .and_then(|processes| {
                                    processes.iter().find(|process| process["id"] == core)
                                })
                                .is_some_and(|process| {
                                    process["fingerprint"] != json!(spec.fingerprint)
                                })
                        })
                });
            let (path, version, compatible, reason) = match path {
                Ok(path) => match core_setup::identity(paths, core) {
                    Ok(identity) => (
                        Some(path.to_string_lossy().into_owned()),
                        identity["product_version"].as_str().map(str::to_owned),
                        true,
                        None,
                    ),
                    Err(error) => (
                        Some(path.to_string_lossy().into_owned()),
                        None,
                        false,
                        Some(error.to_string()),
                    ),
                },
                Err(error) => (None, None, false, Some(error.to_string())),
            };
            let inputs_issue = if graph_configured {
                core_setup::verify_launch_inputs(paths, name)
                    .err()
                    .map(|e| e.to_string())
            } else {
                None
            };
            let endpoint_issue = endpoints.as_ref().err().map(ToString::to_string);
            let ready = compatible
                && graph_configured
                && graph_version_issue.is_none()
                && inputs_issue.is_none()
                && endpoint_issue.is_none()
                && !running_changed;
            let reason = if enabled {
                graph_version_issue
                    .clone()
                    .or(reason)
                    .or(inputs_issue)
                    .or(endpoint_issue)
                    .or_else(|| {
                        running_changed.then(|| {
                            "running from an earlier configured build; restart to use the saved build"
                                .into()
                        })
                    })
            } else {
                None
            };
            let configured_endpoint = endpoints.as_ref().ok().and_then(|endpoints| {
                endpoints.as_ref().and_then(|value| match core {
                    "eos" => value.get("eos_control"),
                    "egia" => value.get("egia_northbound"),
                    _ => None,
                })
            });
            let action = if !enabled {
                "Core is not enabled in the saved graph".into()
            } else if graph_version_issue.is_some() {
                "Back up the old graph, run `isos init` to create a v1 starter, then add nodes and Save graph".into()
            } else if !compatible {
                format!("Build {core} in its repository and configure its binary directory")
            } else if !graph_configured {
                format!("Add the {core} process to the graph, then save it")
            } else if running_changed {
                "Restart the deployment to use the saved build".into()
            } else if reason.is_some() {
                "Run `isos init` to refresh the configured build, then restart".into()
            } else if running {
                "Running from the configured build".into()
            } else {
                "Run the saved graph".into()
            };
            json!({"core":core,"path":path,"version":version,"compatible":compatible,
                "configured":explicit || path.is_some(),"enabled":enabled,"graph_configured":graph_configured,
                "running":running,"ready":ready,"reason":reason,"action":action,
                "configured_endpoint":configured_endpoint})
        })
        .collect();
    let ui = match isosui::bundle_for_serve(paths) {
        Ok(path) => json!({"path":path,"ready":true}),
        Err(error) => json!({"path":null,"ready":false,"reason":error.to_string(),
            "action":"Build IsosUI in its repository and configure ISOSUI_DIST_DIR"}),
    };
    json!({"cores":cores,"graph":{"path":graph_path,"configured":graph.is_object()},
        "isos":{"version":env!("CARGO_PKG_VERSION"),"path":std::env::current_exe().ok()},
        "isosui":ui})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_graph_version_takes_priority_over_changed_binary_guidance() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        crate::config::atomic_write(
            &paths.node_graph_path("default"),
            br#"{"version":"2.0.0","robot":{},"cores":{"isos":{"layers":{"process":{"groups":{"component":{"nodes":{"eos":{"id":"eos","node_type":"isos.process.component@v1.1","settings":{"role":"eos","args":[]}}}}}}}}}}"#,
        )
        .unwrap();
        let status = report(&paths, "default", None);
        let eos = &status["cores"][0];
        assert!(eos["reason"].as_str().unwrap().contains("incompatible"));
        assert!(
            eos["action"]
                .as_str()
                .unwrap()
                .contains("Back up the old graph")
        );
        assert!(status["cores"][1]["reason"].is_null());
    }
}
