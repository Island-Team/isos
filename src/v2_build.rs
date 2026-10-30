//! Build and stage one exact Rust Aether executable set for CLI and UI.

use crate::config::{self, Paths};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

pub const EOS_BINARIES: [&str; 5] = [
    "aether-runtime",
    "aether-profile",
    "aether-http-bridge",
    "aether-config-server",
    "aether-tui",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildReceipt {
    pub schema_version: u32,
    pub component: String,
    pub product_version: String,
    pub source_commit: String,
    pub binaries: BTreeMap<String, PathBuf>,
    pub sha256: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Capabilities {
    product_version: String,
    graph_version: String,
    node_type_version: String,
    binary: String,
    features: Vec<String>,
}

fn probe(path: &Path, binary: &str, features: &[&str]) -> Result<()> {
    let output = Command::new(path)
        .arg("--capabilities")
        .output()
        .with_context(|| format!("probe {}", path.display()))?;
    ensure!(
        output.status.success(),
        "{binary} at {} lacks Rust v2 capabilities; run `isos build eos`: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let info: Capabilities = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "{binary} at {} returned invalid capabilities; run `isos build eos`",
            path.display()
        )
    })?;
    ensure!(
        info.binary == binary
            && info.product_version == "2.0.0"
            && info.graph_version == "2.0.0"
            && info.node_type_version == "1.0",
        "{binary} at {} has incompatible Aether versions; run `isos build eos`",
        path.display()
    );
    for feature in features {
        ensure!(
            info.features.iter().any(|candidate| candidate == feature),
            "{binary} at {} lacks {feature}; run `isos build eos`",
            path.display()
        );
    }
    Ok(())
}

pub fn preflight_processes(paths: &Paths, specs: &[crate::v2_process::ProcessSpec]) -> Result<()> {
    let selected = selected(paths)?;
    for (id, binary, features) in [
        (
            "eos",
            "aether-runtime",
            &[
                "short-paths",
                "node-types",
                "data-idl",
                "primitive-binding",
                "log-dir",
                "vision-gate",
            ][..],
        ),
        (
            "bridge",
            "aether-http-bridge",
            &[
                "short-paths",
                "node-types",
                "data-idl",
                "primitive-binding",
                "vision-ws",
            ][..],
        ),
        (
            "config",
            "aether-config-server",
            &[
                "short-paths",
                "node-types",
                "data-idl",
                "primitive-binding",
                "catalog",
            ][..],
        ),
    ] {
        let spec = specs
            .iter()
            .find(|spec| spec.id == id)
            .with_context(|| format!("graph lacks {id} process"))?;
        let path = Path::new(&spec.settings.command[0]);
        if let Some(receipt) = &selected {
            let expected = &receipt.binaries[binary];
            ensure!(
                path.canonicalize()? == expected.canonicalize()?,
                "{id} graph command uses {}; staged Rust v2 build uses {}; apply the Build binaries suggested graph before Run",
                path.display(),
                expected.display()
            );
        }
        probe(path, binary, features)?;
    }
    let profile = crate::v2_bins::resolve("aether-profile")?;
    if let Some(receipt) = &selected {
        ensure!(
            profile.canonicalize()? == receipt.binaries["aether-profile"].canonicalize()?,
            "profile catalog uses {}; staged Rust v2 build uses {}; remove the executable override or rebuild EOS",
            profile.display(),
            receipt.binaries["aether-profile"].display()
        );
    }
    probe(
        &profile,
        "aether-profile",
        &[
            "short-paths",
            "node-types",
            "data-idl",
            "primitive-binding",
            "catalog",
        ],
    )?;
    Ok(())
}

fn source_root() -> Result<PathBuf> {
    let path = std::env::var_os("EOS_REPO_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("Island-eos")
        });
    ensure!(
        path.is_absolute() && path.join("Cargo.toml").is_file(),
        "EOS_REPO_PATH must name the absolute Rust workspace checkout"
    );
    Ok(path.canonicalize()?)
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn digest(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn build_eos(paths: &Paths) -> Result<BuildReceipt> {
    let root = source_root()?;
    let commit = git(&root, &["rev-parse", "HEAD"])?;
    ensure!(
        git(&root, &["status", "--porcelain"])?.is_empty(),
        "EOS source has uncommitted changes; commit before building a reproducible set"
    );
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".cargo/bin/cargo"))
            .filter(|path| path.is_file())
            .map(Into::into)
            .unwrap_or_else(|| "cargo".into())
    });
    let output = Command::new(cargo)
        .args(["build", "--workspace", "--bins"])
        .env_remove("CARGO_TARGET_DIR")
        .current_dir(&root)
        .output()
        .context("run Rust Aether cargo build")?;
    ensure!(
        output.status.success(),
        "Rust Aether build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source_dir = root.join("target/debug");
    let base = paths.data.join("builds/eos");
    fs::create_dir_all(&base)?;
    fs::set_permissions(&base, fs::Permissions::from_mode(0o700))?;
    let mut source_hashes = BTreeMap::new();
    for name in EOS_BINARIES {
        let path = source_dir.join(name);
        ensure!(path.is_file(), "Rust Aether build omitted {name}");
        source_hashes.insert(name.to_owned(), digest(&path)?);
    }
    let set_hash = hex::encode(Sha256::digest(serde_json::to_vec(&source_hashes)?));
    let stage = base.join(format!("{}-{}", &commit[..12], &set_hash[..12]));
    let bin_dir = stage.join("bin");
    if !stage.exists() {
        let staging = tempfile::Builder::new()
            .prefix("stage-")
            .tempdir_in(&base)?;
        let staging_bin = staging.path().join("bin");
        fs::create_dir(&staging_bin)?;
        fs::set_permissions(&staging_bin, fs::Permissions::from_mode(0o700))?;
        for name in EOS_BINARIES {
            let destination = staging_bin.join(name);
            fs::copy(source_dir.join(name), &destination)?;
            fs::set_permissions(destination, fs::Permissions::from_mode(0o700))?;
        }
        if !stage.exists() {
            fs::rename(staging.path(), &stage)?;
        }
    }
    let binaries = EOS_BINARIES
        .into_iter()
        .map(|name| (name.to_owned(), bin_dir.join(name)))
        .collect();
    let receipt = BuildReceipt {
        schema_version: 2,
        component: "eos".into(),
        product_version: "2.0.0".into(),
        source_commit: commit,
        binaries,
        sha256: source_hashes,
    };
    verify(&receipt)?;
    for (binary, features) in [
        (
            "aether-runtime",
            &[
                "short-paths",
                "node-types",
                "data-idl",
                "primitive-binding",
                "log-dir",
                "vision-gate",
            ][..],
        ),
        (
            "aether-profile",
            &[
                "short-paths",
                "node-types",
                "data-idl",
                "primitive-binding",
                "catalog",
            ][..],
        ),
        (
            "aether-config-server",
            &[
                "short-paths",
                "node-types",
                "data-idl",
                "primitive-binding",
                "catalog",
            ][..],
        ),
        (
            "aether-http-bridge",
            &[
                "short-paths",
                "node-types",
                "data-idl",
                "primitive-binding",
                "vision-ws",
            ][..],
        ),
    ] {
        probe(&receipt.binaries[binary], binary, features)?;
    }
    config::atomic_write(
        &stage.join("receipt.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    config::atomic_write(
        &base.join("current.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}

pub fn selected(paths: &Paths) -> Result<Option<BuildReceipt>> {
    let path = paths.data.join("builds/eos/current.json");
    if !path.exists() {
        return Ok(None);
    }
    let receipt: BuildReceipt = serde_json::from_slice(&fs::read(path)?)?;
    verify(&receipt)?;
    Ok(Some(receipt))
}

pub fn verify(receipt: &BuildReceipt) -> Result<()> {
    ensure!(
        receipt.schema_version == 2
            && receipt.component == "eos"
            && receipt.product_version == "2.0.0",
        "invalid Rust Aether build receipt"
    );
    for name in EOS_BINARIES {
        let path = receipt
            .binaries
            .get(name)
            .with_context(|| format!("missing {name} in build receipt"))?;
        ensure!(
            path.is_absolute() && path.is_file(),
            "missing staged {name}: {}",
            path.display()
        );
        ensure!(
            fs::metadata(path)?.permissions().mode() & 0o111 != 0,
            "staged {name} is not executable"
        );
        ensure!(
            receipt
                .sha256
                .get(name)
                .is_some_and(|expected| digest(path).ok().as_ref() == Some(expected)),
            "staged {name} changed after build"
        );
    }
    Ok(())
}

pub fn suggest_graph(
    graph: &serde_json::Value,
    receipt: &BuildReceipt,
) -> Result<serde_json::Value> {
    let mut graph = graph.clone();
    for (node, binary) in [
        ("eos", "aether-runtime"),
        ("bridge", "aether-http-bridge"),
        ("config", "aether-config-server"),
    ] {
        let pointer =
            format!("/cores/isos/layers/process/groups/component/nodes/{node}/settings/command/0");
        let command = graph
            .pointer_mut(&pointer)
            .with_context(|| format!("graph lacks {node} command"))?;
        *command = serde_json::json!(receipt.binaries[binary]);
    }
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_probe_rejects_stale_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("aether-runtime");
        fs::write(&binary, "#!/bin/sh\nprintf '%s\\n' '{\"product_version\":\"2.0.0\",\"graph_version\":\"2.0.0\",\"node_type_version\":\"1.0\",\"binary\":\"aether-runtime\",\"features\":[\"log-dir\",\"node-types\"]}'\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        probe(&binary, "aether-runtime", &["log-dir", "node-types"]).unwrap();
        assert!(probe(&binary, "aether-runtime", &["primitive-binding"]).is_err());
        assert!(probe(&binary, "aether-profile", &["log-dir"]).is_err());
    }

    #[test]
    fn suggested_graph_changes_only_process_executable_paths() {
        let mut binaries = BTreeMap::new();
        for name in EOS_BINARIES {
            binaries.insert(name.into(), PathBuf::from(format!("/staged/{name}")));
        }
        let receipt = BuildReceipt {
            schema_version: 2,
            component: "eos".into(),
            product_version: "2.0.0".into(),
            source_commit: "a".repeat(40),
            binaries,
            sha256: BTreeMap::new(),
        };
        let graph = serde_json::json!({"version":"2.0.0","cores":{"isos":{"layers":{"process":{"groups":{"component":{"nodes":{"eos":{"settings":{"command":["/old/eos","--profile","graph"]}},"bridge":{"settings":{"command":["/old/bridge"]}},"config":{"settings":{"command":["/old/config"]}}}}}}}}}});
        let suggested = suggest_graph(&graph, &receipt).unwrap();
        assert_eq!(
            suggested["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]["eos"]
                ["settings"]["command"][0],
            "/staged/aether-runtime"
        );
        assert_eq!(
            suggested["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]["eos"]
                ["settings"]["command"][1],
            "--profile"
        );
        assert_eq!(
            graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]["eos"]["settings"]
                ["command"][0],
            "/old/eos"
        );
    }
}
