//! P02 MuJoCo scene and device catalog admission after exact P01 verification.

use crate::{
    backend::{BackendDescriptor, DeviceEntry, Scene},
    config::Paths,
    packages::{self, PackageFailure, VerifiedPackage},
    process,
};
use anyhow::{Context, Result};
use roxmltree::{Document, ParsingOptions};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const LEGACY_FIELDS: &[&str] = &[
    "schema_version",
    "package",
    "entrypoint",
    "protocol",
    "scenes",
    "devices",
    "runtime",
];
const TARGET_FIELDS: &[&str] = &[
    "schema_version",
    "catalog_version",
    "package",
    "entrypoint",
    "protocol",
    "scenes",
    "devices",
    "device_catalog",
    "runtime",
];
const KINDS: [&str; 3] = ["camera", "base", "manipulator"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Legacy,
    Target,
}

#[derive(Clone, Debug)]
pub struct Issue {
    pub code: &'static str,
    pub stage: &'static str,
    pub kind: Option<&'static str>,
}

impl Issue {
    fn descriptor(code: &'static str) -> Self {
        Self {
            code,
            stage: "descriptor",
            kind: None,
        }
    }
    fn device(code: &'static str, kind: &'static str) -> Self {
        Self {
            code,
            stage: "descriptor",
            kind: Some(kind),
        }
    }
    fn admission(code: &'static str) -> Self {
        Self {
            code,
            stage: "admission",
            kind: None,
        }
    }
    pub fn into_error(self, id: &str, version: &str, path: impl Into<String>) -> anyhow::Error {
        PackageFailure {
            code: self.code,
            stage: self.stage,
            id: id.to_owned(),
            version: version.to_owned(),
            path: path.into(),
        }
        .into()
    }
}

pub struct Catalog {
    pub descriptor: BackendDescriptor,
    pub mode: Mode,
    pub devices: Vec<DeviceEntry>,
}

fn shape(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|field| object.contains_key(*field))
    })
}

fn slug(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|piece| {
            !piece.is_empty()
                && piece
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn nonblank(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

fn hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn scene_path(root: &Path, scene: &Scene) -> std::result::Result<PathBuf, Issue> {
    packages::checked_file(root, &scene.path, false)
        .map_err(|_| Issue::descriptor("mujoco_scene_invalid"))
}

fn model_names(
    xml: &str,
) -> std::result::Result<(String, BTreeMap<String, BTreeSet<String>>), Issue> {
    let document = Document::parse_with_options(
        xml,
        ParsingOptions {
            nodes_limit: 65_536,
            ..ParsingOptions::default()
        },
    )
    .map_err(|_| Issue::admission("mujoco_scene_profile_mismatch"))?;
    if document.root_element().tag_name().name() != "mujoco" {
        return Err(Issue::admission("mujoco_scene_profile_mismatch"));
    }
    let mut markers = Vec::new();
    let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for element in document.descendants().filter(|node| node.is_element()) {
        let tag = element.tag_name().name();
        if let Some(name) = element.attribute("name") {
            names
                .entry(tag.to_owned())
                .or_default()
                .insert(name.to_owned());
            if tag == "numeric" && name.starts_with("eos_benchmark_") {
                markers.push(name.to_owned());
            }
        }
    }
    let Some(marker) = markers.pop() else {
        return Err(Issue::admission("mujoco_scene_profile_mismatch"));
    };
    if !markers.is_empty()
        || !matches!(
            marker.as_str(),
            "eos_benchmark_pick_place_v1" | "eos_benchmark_navigation_v1"
        )
    {
        return Err(Issue::admission("mujoco_scene_profile_mismatch"));
    }
    let has = |tag: &str, name: &str| names.get(tag).is_some_and(|items| items.contains(name));
    let required = [
        ("site", "gr_pinch"),
        ("site", "attachment_site"),
        ("body", "benchmark_object"),
        ("freejoint", "object_free"),
        ("general", "gr_fingers_actuator"),
        ("geom", "gr_left_pad1"),
        ("geom", "gr_right_pad1"),
        ("camera", "robot_camera"),
        ("joint", "base_x"),
        ("joint", "base_y"),
        ("joint", "base_yaw"),
        ("velocity", "base_x_motor"),
        ("velocity", "base_y_motor"),
        ("velocity", "base_yaw_motor"),
    ];
    let joints = [
        "shoulder_pan_joint",
        "shoulder_lift_joint",
        "elbow_joint",
        "wrist_1_joint",
        "wrist_2_joint",
        "wrist_3_joint",
    ];
    let actuators = [
        "shoulder_pan",
        "shoulder_lift",
        "elbow",
        "wrist_1",
        "wrist_2",
        "wrist_3",
    ];
    if !required.iter().all(|(tag, name)| has(tag, name))
        || !joints.iter().all(|name| has("joint", name))
        || !actuators.iter().all(|name| has("general", name))
        || (marker == "eos_benchmark_navigation_v1" && !has("site", "navigation_goal"))
    {
        return Err(Issue::admission("mujoco_scene_profile_mismatch"));
    }
    Ok((marker, names))
}

fn validate_target_scene(root: &Path, scene: &Scene) -> std::result::Result<(), Issue> {
    if !nonblank(scene.label.as_deref())
        || !nonblank(scene.description.as_deref())
        || !matches!(
            scene.profile.as_deref(),
            Some("eos_benchmark_pick_place_v1" | "eos_benchmark_navigation_v1")
        )
        || !scene.sha256.as_deref().is_some_and(hex_digest)
    {
        return Err(Issue::descriptor("mujoco_scene_invalid"));
    }
    let path = scene_path(root, scene)?;
    let bytes = fs::read(path).map_err(|_| Issue::descriptor("mujoco_scene_invalid"))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(Issue::descriptor("mujoco_scene_invalid"));
    }
    if hex::encode(Sha256::digest(&bytes)) != scene.sha256.as_deref().unwrap_or_default() {
        return Err(Issue::descriptor("mujoco_scene_digest_mismatch"));
    }
    let xml = std::str::from_utf8(&bytes)
        .map_err(|_| Issue::descriptor("mujoco_scene_profile_mismatch"))?;
    let (marker, _) =
        model_names(xml).map_err(|_| Issue::descriptor("mujoco_scene_profile_mismatch"))?;
    if scene.profile.as_deref() != Some(marker.as_str()) {
        return Err(Issue::descriptor("mujoco_scene_profile_mismatch"));
    }
    Ok(())
}

fn validate_devices(
    descriptor: &BackendDescriptor,
    mode: Mode,
) -> std::result::Result<Vec<DeviceEntry>, Issue> {
    if descriptor.devices.len() != KINDS.len() {
        return Err(Issue::device("mujoco_device_invalid", "camera"));
    }
    let mut distinct = BTreeSet::new();
    for kind in KINDS {
        let Some(id) = descriptor.devices.get(kind) else {
            return Err(Issue::device("mujoco_device_invalid", kind));
        };
        if !slug(id) || !distinct.insert(id) {
            return Err(Issue::device("mujoco_device_invalid", kind));
        }
    }
    if mode == Mode::Legacy {
        return Ok(Vec::new());
    }
    let expected = [
        ("camera", "cam-0"),
        ("base", "base-0"),
        ("manipulator", "arm-0"),
    ];
    for (kind, id) in expected {
        if descriptor
            .devices
            .get(kind)
            .is_none_or(|actual| actual != id)
        {
            return Err(Issue::device("mujoco_device_invalid", kind));
        }
    }
    let Some(catalog) = &descriptor.device_catalog else {
        return Err(Issue::descriptor("mujoco_catalog_incomplete"));
    };
    let catalog: Vec<DeviceEntry> = serde_json::from_value(catalog.clone())
        .map_err(|_| Issue::descriptor("mujoco_catalog_incomplete"))?;
    if catalog.len() != KINDS.len() {
        return Err(Issue::descriptor("mujoco_catalog_incomplete"));
    }
    let scenes: BTreeSet<_> = descriptor
        .scenes
        .iter()
        .map(|scene| scene.id.as_str())
        .collect();
    let mut seen_kinds = BTreeSet::new();
    let mut seen_ids = BTreeSet::new();
    for device in &catalog {
        let Some(kind) = KINDS.into_iter().find(|kind| *kind == device.kind) else {
            return Err(Issue::device("mujoco_device_invalid", "camera"));
        };
        if !seen_kinds.insert(kind)
            || !seen_ids.insert(device.id.as_str())
            || !slug(&device.id)
            || descriptor.devices.get(kind) != Some(&device.id)
            || !nonblank(Some(&device.label))
            || !nonblank(Some(&device.description))
            || device.scene_ids.is_empty()
            || device
                .scene_ids
                .iter()
                .any(|scene| !scenes.contains(scene.as_str()))
            || device.scene_ids.iter().collect::<BTreeSet<_>>().len() != device.scene_ids.len()
        {
            return Err(Issue::device("mujoco_device_invalid", kind));
        }
    }
    Ok(catalog)
}

pub fn inspect(root: &Path, id: &str, version: &str) -> std::result::Result<Catalog, Issue> {
    let path = packages::checked_file(root, "backend-v2.json", false)
        .map_err(|_| Issue::descriptor("mujoco_catalog_incomplete"))?;
    let bytes = fs::read(path).map_err(|_| Issue::descriptor("mujoco_catalog_incomplete"))?;
    if bytes.len() > 65_536 {
        return Err(Issue::descriptor("mujoco_catalog_incomplete"));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| Issue::descriptor("mujoco_catalog_incomplete"))?;
    let mode = match value.get("catalog_version") {
        Some(Value::String(version)) if version == "0.1.0" => Mode::Target,
        Some(_) => return Err(Issue::descriptor("mujoco_catalog_unsupported")),
        None => Mode::Legacy,
    };
    if !shape(
        &value,
        if mode == Mode::Target {
            TARGET_FIELDS
        } else {
            LEGACY_FIELDS
        },
    ) {
        return Err(Issue::descriptor("mujoco_catalog_incomplete"));
    }
    let Some(scenes) = value["scenes"].as_array() else {
        return Err(Issue::descriptor("mujoco_catalog_incomplete"));
    };
    for scene in scenes {
        let fields = if mode == Mode::Target {
            &["id", "path", "label", "description", "profile", "sha256"][..]
        } else {
            &["id", "path"][..]
        };
        if !shape(scene, fields) {
            return Err(Issue::descriptor("mujoco_catalog_incomplete"));
        }
    }
    if mode == Mode::Target {
        let Some(catalog) = value["device_catalog"].as_array() else {
            return Err(Issue::descriptor("mujoco_catalog_incomplete"));
        };
        for device in catalog {
            if !shape(device, &["kind", "id", "label", "description", "scene_ids"]) {
                return Err(Issue::descriptor("mujoco_catalog_incomplete"));
            }
        }
    }
    let descriptor: BackendDescriptor = serde_json::from_value(value)
        .map_err(|_| Issue::descriptor("mujoco_catalog_incomplete"))?;
    if descriptor.schema_version != "2.0.0"
        || descriptor.protocol != "mujoco-v2"
        || descriptor.package.id != id
        || descriptor.package.version != version
    {
        return Err(Issue::descriptor("mujoco_catalog_incomplete"));
    }
    if descriptor.scenes.is_empty() || descriptor.scenes.len() > 32 {
        return Err(Issue::descriptor("mujoco_scene_invalid"));
    }
    let mut ids = BTreeSet::new();
    for scene in &descriptor.scenes {
        if !slug(&scene.id) || !ids.insert(scene.id.as_str()) {
            return Err(Issue::descriptor("mujoco_scene_invalid"));
        }
        scene_path(root, scene)?;
        if mode == Mode::Target {
            validate_target_scene(root, scene)?;
        }
    }
    if mode == Mode::Target && ids != BTreeSet::from(["pick-place", "navigation"]) {
        return Err(Issue::descriptor("mujoco_scene_invalid"));
    }
    let devices = validate_devices(&descriptor, mode)?;
    Ok(Catalog {
        descriptor,
        mode,
        devices,
    })
}

pub fn inspect_verified(verified: &VerifiedPackage) -> std::result::Result<Catalog, Issue> {
    inspect(
        &verified.root,
        &verified.release.id,
        &verified.release.version,
    )
}

pub fn selected_scene<'a>(catalog: &'a Catalog, id: &str) -> std::result::Result<&'a Scene, Issue> {
    catalog
        .descriptor
        .scenes
        .iter()
        .find(|scene| scene.id == id)
        .ok_or_else(|| Issue::admission("mujoco_scene_unavailable"))
}

pub fn selected_device(
    catalog: &Catalog,
    scene: &str,
    kind: &str,
    id: &str,
) -> std::result::Result<(), Issue> {
    let expected = catalog.descriptor.devices.get(kind);
    let listed = catalog
        .devices
        .iter()
        .find(|entry| entry.kind == kind && entry.id == id);
    if expected.is_none_or(|expected| expected != id)
        || (catalog.mode == Mode::Target
            && listed.is_none_or(|entry| !entry.scene_ids.iter().any(|item| item == scene)))
    {
        return Err(Issue::admission("mujoco_device_unavailable"));
    }
    Ok(())
}

pub fn check_library(catalog: &Catalog, env: Option<&Value>) -> std::result::Result<(), Issue> {
    let runtime = &catalog.descriptor.runtime;
    if runtime.mujoco_version != "3.16.0"
        || runtime.environment_variable != "MUJOCO_ROOT"
        || runtime.library != "lib/libmujoco.so.3.16.0"
    {
        return Err(Issue::admission("mujoco_library_unavailable"));
    }
    let root = env
        .and_then(|env| env.get("MUJOCO_ROOT"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| std::env::var("MUJOCO_ROOT").ok())
        .ok_or_else(|| Issue::admission("mujoco_library_unavailable"))?;
    let root = Path::new(&root);
    if !root.is_absolute() || packages::checked_file(root, &runtime.library, false).is_err() {
        return Err(Issue::admission("mujoco_library_unavailable"));
    }
    Ok(())
}

fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn option_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn nodes(graph: &Value) -> Vec<(String, String, String, &Value)> {
    let mut result = Vec::new();
    if let Some(cores) = graph.get("cores").and_then(Value::as_object) {
        for (core_id, core) in cores {
            if let Some(layers) = core.get("layers").and_then(Value::as_object) {
                for (layer_id, layer) in layers {
                    if let Some(groups) = layer.get("groups").and_then(Value::as_object) {
                        for (group_id, group) in groups {
                            if let Some(instances) = group.get("nodes").and_then(Value::as_object) {
                                for (node_id, node) in instances {
                                    let pointer = format!(
                                        "/cores/{}/layers/{}/groups/{}/nodes/{}",
                                        escape(core_id),
                                        escape(layer_id),
                                        escape(group_id),
                                        escape(node_id)
                                    );
                                    result.push((pointer, group_id.clone(), node_id.clone(), node));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    result
}

fn process_error(
    mut issue: Issue,
    id: &str,
    version: &str,
    base: &str,
    all: &[(String, String, String, &Value)],
    key: &str,
) -> anyhow::Error {
    if matches!(
        issue.code,
        "mujoco_scene_profile_mismatch" | "mujoco_scene_digest_mismatch"
    ) {
        issue.stage = "admission";
    }
    let path = if issue.code.starts_with("mujoco_scene_") {
        format!("{base}/settings/scene")
    } else if issue.code == "mujoco_device_invalid" {
        let canonical_backend = format!("eos/process/mujoco/{key}");
        issue
            .kind
            .and_then(|kind| {
                all.iter()
                    .find(|(_, group, _, node)| {
                        group == kind
                            && node["adapter"]["settings"]["backend"].as_str()
                                == Some(canonical_backend.as_str())
                    })
                    .map(|(path, _, _, _)| format!("{path}/adapter/settings/device_id"))
            })
            .unwrap_or_else(|| format!("{base}/package"))
    } else {
        format!("{base}/package")
    };
    issue.into_error(id, version, path)
}

/// Repeat P01 first, then admit the exact graph's MuJoCo catalog and selections.
pub fn validate_graph(paths: &Paths, graph: &Value) -> Result<()> {
    packages::verify_graph_packages(paths, graph)?;
    let all = nodes(graph);
    let mut backends = BTreeMap::new();
    for (base, _, key, node) in &all {
        if !matches!(
            node["node_type"].as_str(),
            Some("eos.process.mujoco@v0.1" | "eos.process.mujoco@v1.0")
        ) {
            continue;
        }
        let id = node["package"]["id"]
            .as_str()
            .context("MuJoCo process requires package ID")?;
        let version = node["package"]["version"]
            .as_str()
            .context("MuJoCo process requires package version")?;
        let verified = packages::verify(paths, id, version)?;
        let catalog = inspect_verified(&verified)
            .map_err(|issue| process_error(issue, id, version, base, &all, key))?;
        if graph["version"] == "0.1.0" && catalog.mode != Mode::Target {
            return Err(Issue::descriptor("mujoco_catalog_incomplete").into_error(
                id,
                version,
                format!("{base}/package"),
            ));
        }
        let scene = node["settings"]["scene"].as_str().unwrap_or("");
        if graph["version"] == "0.1.0" && !scene.is_empty() && !option_id(scene) {
            return Err(Issue::admission("node_option_invalid_id").into_error(
                id,
                version,
                format!("{base}/settings/scene"),
            ));
        }
        selected_scene(&catalog, scene)
            .map_err(|issue| issue.into_error(id, version, format!("{base}/settings/scene")))?;
        check_library(&catalog, node["settings"].get("env"))
            .map_err(|issue| issue.into_error(id, version, format!("{base}/package")))?;
        let canonical = base
            .strip_prefix("/cores/")
            .unwrap_or(base)
            .replace("/layers/", "/")
            .replace("/groups/", "/")
            .replace("/nodes/", "/");
        backends.insert(
            canonical,
            (catalog, scene.to_owned(), id.to_owned(), version.to_owned()),
        );
    }
    for (path, group, _, node) in &all {
        if node["adapter"]["type"] != "mujoco-v2" {
            continue;
        }
        let backend = node["adapter"]["settings"]["backend"]
            .as_str()
            .unwrap_or("");
        let Some((catalog, scene, id, version)) = backends.get(backend) else {
            if graph["version"] == "0.1.0" {
                let package = &node["adapter"]["package"];
                let id = package["id"].as_str().unwrap_or("island/mujoco-sim");
                let version = package["version"].as_str().unwrap_or("0.1.1");
                return Err(Issue::admission("mujoco_device_unavailable").into_error(
                    id,
                    version,
                    format!("{path}/adapter/settings/device_id"),
                ));
            }
            continue;
        };
        if catalog.mode == Mode::Legacy {
            continue;
        }
        let kind = if KINDS.contains(&group.as_str()) {
            group.as_str()
        } else {
            ""
        };
        let device_id = node["adapter"]["settings"]["device_id"]
            .as_str()
            .unwrap_or("");
        let simulator_node = node["adapter"]["settings"]["simulator_node"]
            .as_str()
            .unwrap_or("");
        for (value, key) in [(device_id, "device_id"), (simulator_node, "simulator_node")] {
            if !value.is_empty() && !option_id(value) {
                return Err(Issue::admission("node_option_invalid_id").into_error(
                    id,
                    version,
                    format!("{path}/adapter/settings/{key}"),
                ));
            }
        }
        if node["adapter"]["package"]["id"] != id.as_str()
            || node["adapter"]["package"]["version"] != version.as_str()
            || device_id != simulator_node
            || !node["references"]
                .as_array()
                .is_some_and(|refs| refs.iter().any(|reference| reference == backend))
            || selected_device(catalog, scene, kind, device_id).is_err()
        {
            return Err(Issue::admission("mujoco_device_unavailable").into_error(
                id,
                version,
                format!("{path}/adapter/settings/device_id"),
            ));
        }
        if graph["version"] == "0.1.0" {
            let adapter_compiled = process::catalog()?["adapter_families"]
                .as_array()
                .is_some_and(|families| {
                    families
                        .iter()
                        .any(|family| family["id"] == "mujoco-v2" && family["compiled"] == true)
                });
            if !adapter_compiled {
                return Err(Issue::admission("adapter_unavailable").into_error(
                    id,
                    version,
                    format!("{path}/adapter/type"),
                ));
            }
        }
    }
    Ok(())
}

pub fn is_target_set(set_id: &str, id: &str) -> bool {
    id == "island/mujoco-sim" && set_id == "sim-hardware"
}

pub fn device_entries(catalog: &Catalog) -> Option<&[DeviceEntry]> {
    (catalog.mode == Mode::Target).then_some(catalog.devices.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scene_xml(marker: &str) -> String {
        let mut xml = format!("<mujoco><custom><numeric name=\"{marker}\"/></custom><worldbody>");
        for name in ["gr_pinch", "attachment_site", "navigation_goal"] {
            xml.push_str(&format!("<site name=\"{name}\"/>"));
        }
        xml.push_str("<body name=\"benchmark_object\"><freejoint name=\"object_free\"/></body>");
        for name in ["gr_left_pad1", "gr_right_pad1"] {
            xml.push_str(&format!("<geom name=\"{name}\"/>"));
        }
        xml.push_str("<camera name=\"robot_camera\"/>");
        for name in [
            "base_x",
            "base_y",
            "base_yaw",
            "shoulder_pan_joint",
            "shoulder_lift_joint",
            "elbow_joint",
            "wrist_1_joint",
            "wrist_2_joint",
            "wrist_3_joint",
        ] {
            xml.push_str(&format!("<joint name=\"{name}\"/>"));
        }
        xml.push_str("</worldbody><actuator>");
        for name in ["base_x_motor", "base_y_motor", "base_yaw_motor"] {
            xml.push_str(&format!("<velocity name=\"{name}\"/>"));
        }
        for name in [
            "gr_fingers_actuator",
            "shoulder_pan",
            "shoulder_lift",
            "elbow",
            "wrist_1",
            "wrist_2",
            "wrist_3",
        ] {
            xml.push_str(&format!("<general name=\"{name}\"/>"));
        }
        xml.push_str("</actuator></mujoco>");
        xml
    }

    fn fixture() -> (tempfile::TempDir, Value) {
        let temp = tempfile::tempdir().unwrap();
        let mut descriptor: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/scene_catalog/target-descriptor.json"
        ))
        .unwrap();
        for (index, marker) in ["eos_benchmark_pick_place_v1", "eos_benchmark_navigation_v1"]
            .into_iter()
            .enumerate()
        {
            let xml = scene_xml(marker);
            let path = format!("scene-{index}.xml");
            fs::write(temp.path().join(&path), &xml).unwrap();
            descriptor["scenes"][index]["path"] = json!(path);
            descriptor["scenes"][index]["sha256"] =
                json!(hex::encode(Sha256::digest(xml.as_bytes())));
        }
        (temp, descriptor)
    }

    fn inspect_fixture(
        temp: &tempfile::TempDir,
        descriptor: &Value,
    ) -> std::result::Result<Catalog, Issue> {
        fs::write(
            temp.path().join("backend-v2.json"),
            serde_json::to_vec(descriptor).unwrap(),
        )
        .unwrap();
        inspect(temp.path(), "island/mujoco-sim", "0.1.1")
    }

    #[test]
    fn target_catalog_requires_exact_shape_and_scene_digest() {
        let (temp, mut descriptor) = fixture();
        let catalog = inspect_fixture(&temp, &descriptor).unwrap();
        assert_eq!(catalog.mode, Mode::Target);
        assert_eq!(catalog.devices.len(), 3);
        descriptor.as_object_mut().unwrap().remove("device_catalog");
        assert_eq!(
            inspect_fixture(&temp, &descriptor).err().unwrap().code,
            "mujoco_catalog_incomplete"
        );
        descriptor["device_catalog"] = serde_json::from_str::<Value>(include_str!(
            "../../tests/fixtures/scene_catalog/target-descriptor.json"
        ))
        .unwrap()["device_catalog"]
            .clone();
        descriptor["catalog_version"] = json!("0.2.0");
        assert_eq!(
            inspect_fixture(&temp, &descriptor).err().unwrap().code,
            "mujoco_catalog_unsupported"
        );
        descriptor["catalog_version"] = json!("0.1.0");
        descriptor["scenes"][0]["sha256"] = json!("0".repeat(64));
        assert_eq!(
            inspect_fixture(&temp, &descriptor).err().unwrap().code,
            "mujoco_scene_digest_mismatch"
        );
    }

    #[test]
    fn target_catalog_rejects_profile_and_device_errors() {
        let (temp, mut descriptor) = fixture();
        descriptor["scenes"][0]["profile"] = json!("eos_benchmark_navigation_v1");
        assert_eq!(
            inspect_fixture(&temp, &descriptor).err().unwrap().code,
            "mujoco_scene_profile_mismatch"
        );
        descriptor["scenes"][0]["profile"] = json!("eos_benchmark_pick_place_v1");
        descriptor["device_catalog"][2]["scene_ids"] = json!(["missing"]);
        assert_eq!(
            inspect_fixture(&temp, &descriptor).err().unwrap().code,
            "mujoco_device_invalid"
        );
        descriptor["device_catalog"][2]["scene_ids"] = json!(["navigation"]);
        let catalog = inspect_fixture(&temp, &descriptor).unwrap();
        assert_eq!(
            selected_device(&catalog, "pick-place", "manipulator", "arm-0")
                .err()
                .unwrap()
                .code,
            "mujoco_device_unavailable"
        );
        assert!(selected_device(&catalog, "navigation", "manipulator", "arm-0").is_ok());
    }

    #[test]
    fn xml_parser_rejects_dtd_and_missing_model_elements() {
        assert_eq!(
            model_names("<!DOCTYPE mujoco [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><mujoco/>")
                .err()
                .unwrap()
                .code,
            "mujoco_scene_profile_mismatch"
        );
        assert_eq!(
            model_names(
                "<mujoco><custom><numeric name='eos_benchmark_pick_place_v1'/></custom></mujoco>"
            )
            .err()
            .unwrap()
            .code,
            "mujoco_scene_profile_mismatch"
        );
    }

    #[test]
    fn device_descriptor_error_uses_exact_backend_reference() {
        let other = json!({"adapter":{"settings":{"backend":"eos/process/mujoco/othersim"}}});
        let selected = json!({"adapter":{"settings":{"backend":"eos/process/mujoco/sim"}}});
        let nodes = vec![
            (
                "/cores/eos/layers/hardware/groups/base/nodes/other".to_owned(),
                "base".to_owned(),
                "other".to_owned(),
                &other,
            ),
            (
                "/cores/eos/layers/hardware/groups/base/nodes/drive".to_owned(),
                "base".to_owned(),
                "drive".to_owned(),
                &selected,
            ),
        ];
        let error = process_error(
            Issue::device("mujoco_device_invalid", "base"),
            "island/mujoco-sim",
            "0.1.1",
            "/cores/eos/layers/process/groups/mujoco/nodes/sim",
            &nodes,
            "sim",
        );
        let failure = error.downcast_ref::<PackageFailure>().unwrap();
        assert_eq!(
            failure.path,
            "/cores/eos/layers/hardware/groups/base/nodes/drive/adapter/settings/device_id"
        );
    }
}
