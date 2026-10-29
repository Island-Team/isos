//! Versioned release metadata; downloading is deliberately separate from validation.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation: Option<Activation>,
    pub assets: Vec<Asset>,
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
                    && a.libc.as_ref().is_none_or(|v| {
                        let need = v.strip_prefix("glibc>=").and_then(numeric_version);
                        let have = host.libc.as_deref().and_then(numeric_version);
                        matches!((need,have), (Some(n),Some(h)) if h >= n)
                    })
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
    fn accepts_catalog_activation_compatibility_fields() {
        let release: Release = serde_json::from_str(&format!(
            r#"{{"schema_version":4,"id":"island/mujoco-sim","kind":"backend","version":"1.0.1","source_commit":"{}","activation":{{"files":[{{"source":"config/module.toml","destination":"module.toml"}}],"backend_template":"","entrypoint":"","eos_template":"","scene":"","simulator_template":""}},"assets":[{{"os":"linux","arch":"x86_64","url":"https://github.com/Island-Team/isos-mujoco-sim/releases/download/v1.0.1/sim.tar.gz","size_bytes":10,"sha256":"{}"}}]}}"#,
            "a".repeat(40),
            "b".repeat(64)
        ))
        .unwrap();

        release.validate().unwrap();
        assert_eq!(release.activation.as_ref().unwrap().files.len(), 1);
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
