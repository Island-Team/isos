//! Versioned release metadata; downloading is deliberately separate from validation.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::{fs::File, io::Read, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub schema_version: u32,
    pub id: String,
    pub kind: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    pub source_commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_url: Option<String>,
    #[serde(default)]
    pub contracts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_inputs: Vec<ConfigInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation: Option<Activation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_contributions: Option<NodeContributions>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapters: Vec<AdapterLibrary>,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterLibrary {
    pub family: String,
    pub library: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeContributions {
    #[serde(default)]
    pub node_types: Vec<Value>,
    #[serde(default)]
    pub safety_adapters: Vec<Value>,
    #[serde(default)]
    pub adapter_families: Vec<Value>,
    #[serde(default)]
    pub data_types: Vec<DataType>,
    #[serde(default)]
    pub node_sets: Vec<NodeSet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataType {
    pub id: String,
    pub version: String,
    pub purpose: String,
    pub media_type: String,
    pub codec: String,
    pub schema: Value,
    pub max_payload_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeSet {
    pub id: String,
    #[serde(default)]
    pub parameters: BTreeMap<String, NodeParameter>,
    pub nodes: Vec<NodeTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeParameter {
    #[serde(rename = "type")]
    pub value_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeTemplate {
    #[serde(rename = "ref")]
    pub reference: String,
    pub core: String,
    pub layer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub profile: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
// Island-port adds legacy template fields to activation in catalog responses;
// the schema v4 client only consumes the canonical files list.
pub struct Activation {
    pub files: Vec<ActivationFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationFile {
    pub source: String,
    pub destination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigInput {
    pub name: String,
    #[serde(rename = "type")]
    pub input_type: String,
    pub choices: Vec<String>,
    pub default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub os: String,
    pub arch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub libc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ros: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_libraries: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tested_ubuntu: Vec<String>,
    // Compatibility metadata projected by Island-port's catalog response.
    // `glibc_min` is used only when the canonical schema v4 `libc` is absent.
    #[serde(default, skip_serializing)]
    pub glibc_min: String,
    #[serde(default, skip_serializing, rename = "eos_contract")]
    _eos_contract: String,
    pub url: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct Platform {
    pub os: String,
    pub arch: String,
    pub libc: Option<String>,
    pub gpu: Option<String>,
    pub ros: Option<String>,
}

fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn valid_data_type_identity(data_type: &DataType) -> bool {
    let valid_version = data_type
        .version
        .split_once('.')
        .is_some_and(|(major, minor)| {
            !major.is_empty()
                && !minor.is_empty()
                && major.bytes().all(|b| b.is_ascii_digit())
                && minor.bytes().all(|b| b.is_ascii_digit())
        });
    valid_version
        && data_type.id.ends_with(&format!("@v{}", data_type.version))
        && data_type.id.split_once("@v").is_some_and(|(prefix, _)| {
            prefix.contains('.') && !prefix.starts_with('.') && !prefix.ends_with('.')
        })
}

fn numeric_version(s: &str) -> Option<Vec<u32>> {
    let values: Option<Vec<u32>> = s.split('.').map(|v| v.parse().ok()).collect();
    values.filter(|v| !v.is_empty())
}

impl Release {
    pub fn read_from_path(path: &Path) -> Result<Self> {
        let release: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        release.validate()?;
        Ok(release)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 4, "release schema_version must be 4");
        ensure!(
            self.id
                .split_once('/')
                .is_some_and(|(a, b)| valid_slug(a) && valid_slug(b)),
            "release id must be namespace/name"
        );
        ensure!(
            matches!(
                self.kind.as_str(),
                "component" | "service" | "backend" | "skill" | "extension" | "example"
            ),
            "unsupported release kind: {}",
            self.kind
        );
        ensure!(
            self.version.split('.').count() >= 3
                && self.version.starts_with(|c: char| c.is_ascii_digit()),
            "invalid release version"
        );
        ensure!(
            self.source_commit.len() == 40
                && self.source_commit.bytes().all(|b| b.is_ascii_hexdigit()),
            "source_commit must be a full Git commit SHA"
        );
        for url in [&self.source_url, &self.release_url].into_iter().flatten() {
            ensure!(
                url.starts_with("https://github.com/"),
                "release source URLs must use GitHub HTTPS"
            );
        }
        ensure!(
            !self.assets.is_empty(),
            "release requires at least one asset"
        );
        let mut adapter_families = std::collections::BTreeSet::new();
        for adapter in &self.adapters {
            ensure!(
                self.kind == "backend",
                "adapter libraries require a backend release"
            );
            ensure!(
                !adapter.family.is_empty() && adapter_families.insert(&adapter.family),
                "duplicate or empty adapter family"
            );
            ensure!(
                adapter.library.ends_with(".so")
                    && !adapter.library.starts_with('/')
                    && adapter
                        .library
                        .split('/')
                        .all(|part| !matches!(part, "" | "." | "..")),
                "adapter library must be a safe package-relative .so path"
            );
            ensure!(
                adapter.sha256.len() == 64
                    && adapter.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "adapter library sha256 must be 64 hex characters"
            );
        }
        if let Some(activation) = &self.activation {
            ensure!(!activation.files.is_empty(), "activation requires files");
            let mut destinations = std::collections::BTreeSet::new();
            for file in &activation.files {
                for path in [&file.source, &file.destination] {
                    ensure!(
                        !path.is_empty()
                            && path.split('/').all(|p| !matches!(p, "" | "." | ".."))
                            && !path.starts_with('/'),
                        "activation paths must be safe relative paths"
                    );
                }
                ensure!(
                    file.destination.ends_with(".toml"),
                    "activation destination must be TOML"
                );
                ensure!(
                    destinations.insert(&file.destination),
                    "duplicate activation destination"
                );
            }
        }
        if let Some(contributions) = &self.node_contributions {
            let mut data_ids = std::collections::BTreeSet::new();
            for data_type in &contributions.data_types {
                ensure!(
                    valid_data_type_identity(data_type) && data_ids.insert(&data_type.id),
                    "invalid or duplicate data type {}",
                    data_type.id
                );
                ensure!(
                    !data_type.purpose.trim().is_empty(),
                    "{}: purpose is required",
                    data_type.id
                );
                ensure!(
                    !data_type.media_type.trim().is_empty(),
                    "{}: media_type is required",
                    data_type.id
                );
                ensure!(
                    !data_type.codec.trim().is_empty(),
                    "{}: codec is required",
                    data_type.id
                );
                ensure!(
                    data_type.schema.is_object(),
                    "{}: schema must be an object",
                    data_type.id
                );
                ensure!(
                    (1..=100_000_000).contains(&data_type.max_payload_bytes),
                    "{}: max_payload_bytes must be 1..100000000",
                    data_type.id
                );
            }
            let mut type_ids = std::collections::BTreeSet::new();
            for node_type in &contributions.node_types {
                let id = node_type
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("contributed node type requires id"))?;
                ensure!(
                    id.contains("@v1.0") && type_ids.insert(id),
                    "invalid or duplicate contributed node type {id}"
                );
                for field in [
                    "core",
                    "layer",
                    "group",
                    "kind",
                    "lifecycle",
                    "capabilities",
                    "settings",
                    "inputs",
                    "outputs",
                ] {
                    ensure!(
                        node_type.get(field).is_some(),
                        "{id}: node type requires {field}"
                    );
                }
                for port in ["inputs", "outputs"]
                    .into_iter()
                    .flat_map(|key| node_type[key].as_array().into_iter().flatten())
                {
                    if let Some(rate) = port.get("expected_rate_hz") {
                        ensure!(
                            rate.as_f64()
                                .is_some_and(|value| value.is_finite() && value > 0.0),
                            "{id}: expected_rate_hz must be positive"
                        );
                    }
                    if let Some(transport) = port.get("transport").and_then(Value::as_str) {
                        ensure!(
                            matches!(
                                transport,
                                "in_process" | "unix_socket" | "websocket" | "ros2"
                            ),
                            "{id}: invalid port transport"
                        );
                    }
                    if let Some(class) = port.get("data_class").and_then(Value::as_str) {
                        ensure!(
                            matches!(
                                class,
                                "image" | "observation" | "command" | "event" | "state" | "opaque"
                            ),
                            "{id}: invalid port data_class"
                        );
                    }
                }
            }
            let mut adapter_ids = std::collections::BTreeSet::new();
            for adapter in &contributions.safety_adapters {
                let id = adapter
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("safety adapter requires id"))?;
                ensure!(adapter_ids.insert(id), "duplicate safety adapter {id}");
                ensure!(
                    adapter.get("supports_stop") == Some(&Value::Bool(true)),
                    "{id}: safety adapter must support stop"
                );
            }
            let mut family_ids = std::collections::BTreeSet::new();
            for family in &contributions.adapter_families {
                let id = family
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("adapter family requires id"))?;
                ensure!(
                    !id.trim().is_empty() && family_ids.insert(id),
                    "invalid or duplicate adapter family {id}"
                );
                ensure!(
                    family
                        .get("device_kinds")
                        .and_then(Value::as_array)
                        .is_some_and(|kinds| !kinds.is_empty()),
                    "{id}: device_kinds must be a nonempty array"
                );
                ensure!(
                    family
                        .get("transport")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.is_empty()),
                    "{id}: transport is required"
                );
                ensure!(
                    family
                        .get("description")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.trim().is_empty()),
                    "{id}: description is required"
                );
            }
            let mut set_ids = std::collections::BTreeSet::new();
            for set in &contributions.node_sets {
                ensure!(
                    valid_slug(&set.id) && set_ids.insert(&set.id),
                    "invalid or duplicate node set {}",
                    set.id
                );
                ensure!(!set.nodes.is_empty(), "node set {} requires nodes", set.id);
                for (name, parameter) in &set.parameters {
                    ensure!(
                        !name.is_empty()
                            && name
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.'),
                        "invalid node set parameter {name}"
                    );
                    ensure!(
                        parameter.value_type == "string",
                        "{name}: only string parameter is supported"
                    );
                }
                let mut refs = std::collections::BTreeSet::new();
                for node in &set.nodes {
                    ensure!(
                        valid_slug(&node.reference) && refs.insert(&node.reference),
                        "invalid or duplicate node ref {}",
                        node.reference
                    );
                    ensure!(
                        valid_slug(&node.core) && valid_slug(&node.layer),
                        "invalid node placement"
                    );
                    ensure!(
                        node.group.as_deref().is_none_or(valid_slug),
                        "invalid node group"
                    );
                    ensure!(node.profile.is_object(), "node profile must be an object");
                    ensure!(
                        node.profile
                            .get("id")
                            .and_then(Value::as_str)
                            .is_some_and(valid_slug),
                        "node profile requires slug id"
                    );
                    ensure!(
                        node.profile
                            .get("node_type")
                            .and_then(Value::as_str)
                            .is_some(),
                        "node profile requires node_type"
                    );
                }
            }
        }
        for a in &self.assets {
            ensure!(
                !a.os.is_empty() && !a.arch.is_empty(),
                "asset requires os and arch"
            );
            ensure!(
                a.url.starts_with("https://github.com/") && a.url.contains("/releases/download/"),
                "asset URL must be a GitHub Release HTTPS download URL"
            );
            ensure!(
                a.size_bytes > 0 && a.size_bytes <= 2_000_000_000,
                "asset size must be 1..2000000000 bytes"
            );
            let hash = a.sha256.strip_prefix("sha256:").unwrap_or(&a.sha256);
            ensure!(
                hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "asset sha256 must be 64 hexadecimal digits"
            );
            if let Some(libc) = &a.libc {
                ensure!(
                    libc.strip_prefix("glibc>=")
                        .and_then(numeric_version)
                        .is_some(),
                    "asset libc must use glibc>=major.minor"
                );
            }
            ensure!(
                a.glibc_min.is_empty() || numeric_version(&a.glibc_min).is_some(),
                "asset glibc_min must use major.minor"
            );
            ensure!(
                a.required_libraries
                    .iter()
                    .all(|name| !name.is_empty() && !name.contains('/')),
                "required_libraries must contain library names"
            );
        }
        Ok(())
    }

    pub fn select_asset<'a>(&'a self, host: &Platform) -> Result<&'a Asset> {
        self.validate()?;
        self.assets
            .iter()
            .find(|a| {
                a.os == host.os
                    && a.arch == host.arch
                    && a.gpu.as_ref().is_none_or(|v| host.gpu.as_ref() == Some(v))
                    && a.ros.as_ref().is_none_or(|v| host.ros.as_ref() == Some(v))
                    && {
                        let need = a
                            .libc
                            .as_deref()
                            .and_then(|v| v.strip_prefix("glibc>="))
                            .and_then(numeric_version)
                            .or_else(|| numeric_version(&a.glibc_min));
                        let have = host.libc.as_deref().and_then(numeric_version);
                        need.is_none_or(|n| matches!(have, Some(h) if h >= n))
                    }
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no asset for {} {} (libc {:?}, gpu {:?}, ros {:?})",
                    host.os,
                    host.arch,
                    host.libc,
                    host.gpu,
                    host.ros
                )
            })
    }
}

impl Platform {
    pub fn current() -> Self {
        let arch = match std::env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            other => other,
        }
        .to_string();
        let libc = if cfg!(target_os = "linux") {
            // glibc exposes this as a static NUL-terminated string.
            unsafe {
                let ptr = libc::gnu_get_libc_version();
                if ptr.is_null() {
                    None
                } else {
                    Some(std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned())
                }
            }
        } else {
            None
        };
        Self {
            os: std::env::consts::OS.into(),
            arch,
            libc,
            gpu: std::env::var("ISOS_GPU").ok(),
            ros: std::env::var("ISOS_ROS").ok(),
        }
    }
}

pub fn verify_file(asset: &Asset, path: &Path) -> Result<()> {
    let mut file = File::open(path).with_context(|| format!("cannot read {}", path.display()))?;
    ensure!(
        file.metadata()?.len() == asset.size_bytes,
        "asset size mismatch: {}",
        path.display()
    );
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
    }
    let expected = asset
        .sha256
        .strip_prefix("sha256:")
        .unwrap_or(&asset.sha256);
    ensure!(
        hex::encode(hasher.finalize()).eq_ignore_ascii_case(expected),
        "asset SHA-256 mismatch: {}",
        path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_only_backend_metadata_is_valid_and_rejects_unsafe_libraries() {
        let mut release: Release = serde_json::from_value(serde_json::json!({
            "schema_version": 4, "id": "vendor/devices", "kind": "backend", "version": "2.0.0",
            "source_commit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "adapters": [{"family": "vendor-base", "library": "lib/base.so", "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}],
            "assets": [{"os": "linux", "arch": "x86_64", "url": "https://github.com/vendor/devices/releases/download/v2.0.0/devices.tar.gz", "size_bytes": 1, "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}]
        })).unwrap();
        release.validate().unwrap();
        release.adapters[0].library = "../base.so".into();
        assert!(release.validate().is_err());
        release.adapters[0].library = "lib/base.so".into();
        release.adapters.push(release.adapters[0].clone());
        assert!(release.validate().is_err());
    }

    #[test]
    fn adapter_family_requires_device_class_and_transport() {
        let mut release: Release = serde_json::from_value(serde_json::json!({
            "schema_version":4,"id":"island/test","kind":"backend","version":"2.0.0",
            "source_commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "assets":[{"os":"linux","arch":"x86_64","url":"https://github.com/Island-Team/test/releases/download/v2.0.0/test.tar.gz","size_bytes":1,"sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}],
            "node_contributions":{"adapter_families":[{"id":"test-v2","device_kinds":["base"],"transport":"unix_socket","description":"Test base adapter"}]}
        })).unwrap();
        release.validate().unwrap();
        release
            .node_contributions
            .as_mut()
            .unwrap()
            .adapter_families[0]["device_kinds"] = serde_json::json!([]);
        assert!(release.validate().is_err());
    }

    #[test]
    fn custom_data_idl_requires_explicit_version_and_purpose() {
        let valid = serde_json::json!({"id":"vendor.camera.frame@v1.0","version":"1.0","purpose":"Camera preview","media_type":"application/json","codec":"json_v1","schema":{"type":"object"},"max_payload_bytes":65536});
        let descriptor: DataType = serde_json::from_value(valid.clone()).unwrap();
        assert!(valid_data_type_identity(&descriptor));
        let mut mismatched = descriptor.clone();
        mismatched.version = "1.1".into();
        assert!(!valid_data_type_identity(&mismatched));
        let mut missing_version = valid.clone();
        missing_version.as_object_mut().unwrap().remove("version");
        assert!(serde_json::from_value::<DataType>(missing_version).is_err());
        let mut missing_purpose = valid;
        missing_purpose.as_object_mut().unwrap().remove("purpose");
        assert!(serde_json::from_value::<DataType>(missing_purpose).is_err());
    }

    #[test]
    fn accepts_catalog_activation_compatibility_fields() {
        let release: Release = serde_json::from_str(&format!(
            r#"{{"schema_version":4,"id":"island/mujoco-sim","kind":"backend","version":"1.0.1","source_commit":"{}","config_inputs":[],"activation":{{"files":[{{"source":"config/module.toml","destination":"module.toml"}}],"backend_template":"","entrypoint":"","eos_template":"","scene":"","simulator_template":""}},"assets":[{{"os":"linux","arch":"x86_64","glibc_min":"2.35","eos_contract":"","url":"https://github.com/Island-Team/isos-mujoco-sim/releases/download/v1.0.1/sim.tar.gz","size_bytes":10,"sha256":"{}"}}]}}"#,
            "a".repeat(40),
            "b".repeat(64)
        ))
        .unwrap();

        release.validate().unwrap();
        assert_eq!(release.activation.as_ref().unwrap().files.len(), 1);
        assert!(
            release
                .select_asset(&Platform {
                    os: "linux".into(),
                    arch: "x86_64".into(),
                    libc: Some("2.39".into()),
                    gpu: None,
                    ros: None,
                })
                .is_ok()
        );
        assert!(
            release
                .select_asset(&Platform {
                    os: "linux".into(),
                    arch: "x86_64".into(),
                    libc: Some("2.31".into()),
                    gpu: None,
                    ros: None,
                })
                .is_err()
        );
        let encoded = serde_json::to_value(release).unwrap();
        let activation = encoded["activation"].as_object().unwrap();
        assert_eq!(activation.len(), 1);
        assert!(activation.contains_key("files"));
    }

    #[test]
    fn selects_matching_asset() {
        let release: Release = serde_json::from_str(&format!(r#"{{"schema_version":4,"id":"island/eos","kind":"component","version":"2.0.0","source_commit":"{}","assets":[{{"os":"linux","arch":"x86_64","libc":"glibc>=2.35","url":"https://github.com/Island-Team/Aether-EOS-R/releases/download/v2.0.0/eos.tar.gz","size_bytes":10,"sha256":"{}"}}]}}"#, "a".repeat(40), "b".repeat(64))).unwrap();
        let host = Platform {
            os: "linux".into(),
            arch: "x86_64".into(),
            libc: Some("2.39".into()),
            gpu: None,
            ros: None,
        };
        assert!(release.select_asset(&host).is_ok());
        assert!(
            release
                .select_asset(&Platform {
                    libc: Some("2.31".into()),
                    ..host
                })
                .is_err()
        );
    }
}
