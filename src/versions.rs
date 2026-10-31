//! Legacy C++ deployment version checks retained for lifecycle admission.
use crate::{component, config::Paths, graph::Deployment, isosui};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub schema_version: u32,
    pub component: String,
    pub product_version: String,
    pub source_commit: String,
    pub artifact: PathBuf,
    #[serde(default)]
    pub config_binary: Option<PathBuf>,
    #[serde(default)]
    pub source_root: Option<PathBuf>,
    pub sha256: String,
    pub descriptor: Value,
    #[serde(default)]
    pub profiles: Value,
}
fn sha(path: &Path) -> Result<String> {
    Ok(hex::encode(Sha256::digest(fs::read(path)?)))
}
fn pin_path(paths: &Paths, role: &str) -> PathBuf {
    paths.config.join("pins").join(format!("{role}.json"))
}
fn candidates(paths: &Paths, role: &str) -> Result<Vec<Receipt>> {
    let dir = paths.data.join("builds").join(role);
    if !dir.exists() {
        return Ok(vec![]);
    }
    fs::read_dir(dir)?
        .map(|entry| {
            let path = entry?.path();
            Ok(serde_json::from_slice(&fs::read(path)?)?)
        })
        .collect()
}
fn read_pin(paths: &Paths, role: &str) -> Result<Option<Receipt>> {
    let path = pin_path(paths, role);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}
fn verify(receipt: &Receipt) -> Result<()> {
    ensure!(receipt.schema_version == 1, "unknown build receipt schema");
    ensure!(
        receipt.artifact.is_file(),
        "{} artifact missing",
        receipt.component
    );
    ensure!(
        sha(&receipt.artifact)? == receipt.sha256,
        "{} artifact digest changed",
        receipt.component
    );
    Ok(())
}
pub fn check(paths: &Paths, graph: Option<&Deployment>) -> Result<Value> {
    let own_pin = paths.config.join("isos-version");
    if own_pin.exists() {
        ensure!(
            fs::read_to_string(&own_pin)?.trim() == env!("CARGO_PKG_VERSION"),
            "IsOS binary version differs from initialized version; run install.sh explicitly, then reinitialize"
        );
    }
    let mut components = serde_json::Map::new();
    let mut selected = std::collections::BTreeMap::new();
    for role in ["eos", "egia"] {
        let available: Vec<Value> = candidates(paths, role)?
            .into_iter()
            .map(|r| serde_json::json!({"version":r.product_version,"commit":r.source_commit}))
            .collect();
        if let Some(receipt) = read_pin(paths, role)? {
            verify(&receipt)?;
            let live = component::call(role, "describe", None).ok();
            ensure!(
                live.as_ref() == Some(&receipt.descriptor),
                "{role} descriptor changed after build"
            );
            if !receipt.profiles.is_null() {
                ensure!(
                    component::call(role, "profiles", None).ok().as_ref()
                        == Some(&receipt.profiles),
                    "{role} profiles changed after build"
                );
            }
            components.insert(role.into(),serde_json::json!({"status":"pinned","version":receipt.product_version,"commit":receipt.source_commit,"sha256":receipt.sha256,"available_local_builds":available}));
            selected.insert(role, receipt.descriptor);
        } else {
            if graph.is_some_and(|g| {
                g.nodes
                    .iter()
                    .any(|n| n.enabled && n.role.as_deref() == Some(role))
            }) {
                bail!("enabled {role} has no pinned version");
            }
            components.insert(
                role.into(),
                serde_json::json!({"status":"unpinned","available_local_builds":available}),
            );
        }
    }
    if let (Some(eos), Some(egia)) = (selected.get("eos"), selected.get("egia")) {
        let provides = eos["provides"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("EOS descriptor lacks provides"))?;
        for required in egia["requires"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("EGIA descriptor lacks requires"))?
        {
            ensure!(
                provides
                    .iter()
                    .any(|p| p["name"] == required["name"] && p["version"] == required["version"]),
                "EGIA requires unsupported EOS contract {} {}",
                required["name"],
                required["version"]
            );
        }
    }
    Ok(
        serde_json::json!({"isos":env!("CARGO_PKG_VERSION"),"components":components,"isosui":isosui::bundle_status(paths)}),
    )
}
