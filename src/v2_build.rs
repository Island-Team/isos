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
pub struct IsosReceipt {
    pub schema_version: u32,
    pub product_version: String,
    pub source_commit: String,
    pub source_dirty: bool,
    pub source_fingerprint: String,
    #[serde(default)]
    pub source_root: Option<PathBuf>,
    pub executable: PathBuf,
    pub sha256: String,
}

pub fn verify_isos(receipt: &IsosReceipt) -> Result<()> {
    ensure!(
        receipt.schema_version == 1 && receipt.product_version == env!("CARGO_PKG_VERSION"),
        "invalid IsOS build receipt"
    );
    ensure!(
        receipt.executable.is_absolute() && receipt.executable.is_file(),
        "staged IsOS executable missing"
    );
    ensure!(
        fs::metadata(&receipt.executable)?.permissions().mode() & 0o111 != 0,
        "staged IsOS executable is not executable"
    );
    ensure!(
        digest(&receipt.executable)? == receipt.sha256,
        "staged IsOS executable digest changed"
    );
    Ok(())
}

pub fn build_isos(paths: &Paths) -> Result<IsosReceipt> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).canonicalize()?;
    let (commit, dirty, fingerprint) = source_identity(&root)?;
    ensure!(
        paths.mode()? != "product" || !dirty,
        "product IsOS build requires clean source"
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
        .args(["build", "--release", "--bin", "isos"])
        .env_remove("CARGO_TARGET_DIR")
        .current_dir(&root)
        .output()
        .context("build IsOS")?;
    ensure!(
        output.status.success(),
        "IsOS build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = root.join("target/release/isos");
    ensure!(source.is_file(), "IsOS release executable missing");
    let sha256 = digest(&source)?;
    let base = paths.data.join("builds/isos");
    fs::create_dir_all(&base)?;
    fs::set_permissions(&base, fs::Permissions::from_mode(0o700))?;
    let stage = base.join(format!(
        "{}-{}-{}",
        &commit[..12],
        &fingerprint[..12],
        &sha256[..12]
    ));
    let executable = stage.join("isos");
    let receipt = IsosReceipt {
        schema_version: 1,
        product_version: env!("CARGO_PKG_VERSION").into(),
        source_commit: commit,
        source_dirty: dirty,
        source_fingerprint: fingerprint,
        source_root: Some(root),
        executable,
        sha256,
    };
    if stage.exists() {
        verify_isos(&receipt)?;
    } else {
        let staging = tempfile::Builder::new()
            .prefix("stage-")
            .tempdir_in(&base)?;
        let dest = staging.path().join("isos");
        fs::copy(&source, &dest)?;
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o700))?;
        ensure!(
            digest(&dest)? == receipt.sha256,
            "IsOS staged copy differs from build"
        );
        fs::rename(staging.path(), &stage)?;
    }
    config::atomic_write(
        &stage.join("receipt.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}

pub fn install_isos(paths: &Paths, selector: &str) -> Result<IsosReceipt> {
    let mut matches = Vec::new();
    let base = paths.data.join("builds/isos");
    for entry in fs::read_dir(&base).with_context(|| format!("read {}", base.display()))? {
        let path = entry?.path().join("receipt.json");
        if !path.is_file() {
            continue;
        }
        let receipt: IsosReceipt = serde_json::from_slice(&fs::read(&path)?)?;
        if receipt.source_commit.starts_with(selector) || receipt.sha256.starts_with(selector) {
            matches.push(receipt);
        }
    }
    ensure!(
        matches.len() == 1,
        "expected one IsOS build matching {selector}; found {}",
        matches.len()
    );
    let receipt = matches.remove(0);
    verify_isos(&receipt)?;
    ensure!(
        paths.mode()? != "product"
            || (!receipt.source_dirty && !receipt.source_fingerprint.is_empty()),
        "product mode requires clean IsOS receipt"
    );
    ensure!(
        std::os::unix::net::UnixStream::connect(paths.socket()).is_err(),
        "stop the IsOS manager before activating a new executable"
    );
    let installed = std::env::current_exe()?.canonicalize()?;
    let parent = installed.parent().context("installed IsOS has no parent")?;
    let staging = tempfile::Builder::new()
        .prefix(".isos-install-")
        .tempfile_in(parent)?;
    fs::copy(&receipt.executable, staging.path())?;
    fs::set_permissions(staging.path(), fs::Permissions::from_mode(0o755))?;
    ensure!(
        digest(staging.path())? == receipt.sha256,
        "IsOS activation copy changed"
    );
    staging.persist(&installed)?;
    config::atomic_write(
        &paths.config.join("pins/isos.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}

pub fn verify_active_isos(paths: &Paths) -> Result<()> {
    let pin = paths.config.join("pins/isos.json");
    if !pin.exists() {
        ensure!(
            paths.mode()? != "product",
            "product mode requires a selected IsOS build; run `isos build isos`, then `isos version select isos <commit>` while the manager is stopped"
        );
        return Ok(());
    }
    let receipt: IsosReceipt = serde_json::from_slice(&fs::read(pin)?)?;
    verify_isos(&receipt)?;
    ensure!(
        digest(&std::env::current_exe()?)? == receipt.sha256,
        "this IsOS executable differs from the selected build; run `isos version select isos <commit>` while the manager is stopped"
    );
    if paths.mode()? == "product" {
        ensure!(
            !receipt.source_dirty && !receipt.source_fingerprint.is_empty(),
            "product mode requires a clean, fingerprinted IsOS receipt"
        );
    } else if let Some(root) = &receipt.source_root
        && root.is_dir()
    {
        let (commit, _, fingerprint) = source_identity(root)?;
        ensure!(
            commit == receipt.source_commit && fingerprint == receipt.source_fingerprint,
            "IsOS source changed since selected build; run `isos build isos`, then `isos version select isos <commit>`"
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildReceipt {
    pub schema_version: u32,
    pub component: String,
    pub product_version: String,
    pub source_commit: String,
    #[serde(default)]
    pub source_dirty: bool,
    #[serde(default)]
    pub source_fingerprint: String,
    #[serde(default)]
    pub source_root: Option<PathBuf>,
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
    ensure!(
        selected.is_some(),
        "EOS build is unselected; run `isos build eos`, then `isos version select eos <commit>`"
    );
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

pub fn source_identity(root: &Path) -> Result<(String, bool, String)> {
    let commit = git(root, &["rev-parse", "HEAD"])?;
    let dirty = !git(root, &["status", "--porcelain", "--untracked-files=all"])?.is_empty();
    let output = Command::new("git")
        .args(["ls-files", "-co", "--exclude-standard", "-z"])
        .current_dir(root)
        .output()?;
    ensure!(output.status.success(), "git ls-files failed");
    let mut names: Vec<_> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .collect();
    names.sort();
    names.dedup();
    let mut hash = Sha256::new();
    hash.update(commit.as_bytes());
    for name in names {
        let relative = std::str::from_utf8(name)?;
        let path = root.join(relative);
        hash.update(name);
        hash.update([0]);
        if path.is_file() {
            hash.update(Sha256::digest(fs::read(path)?));
        } else {
            hash.update(b"missing");
        }
    }
    Ok((commit, dirty, hex::encode(hash.finalize())))
}

pub fn build_eos(paths: &Paths) -> Result<BuildReceipt> {
    let root = source_root()?;
    let (commit, dirty, fingerprint) = source_identity(&root)?;
    ensure!(
        paths.mode()? != "product" || !dirty,
        "product EOS build requires clean source"
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
    let stage = base.join(format!(
        "{}-{}-{}",
        &commit[..12],
        &fingerprint[..12],
        &set_hash[..12]
    ));
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
        source_dirty: dirty,
        source_fingerprint: fingerprint,
        source_root: Some(root),
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
    Ok(receipt)
}

pub fn selected(paths: &Paths) -> Result<Option<BuildReceipt>> {
    let pin = paths.config.join("pins/eos.json");
    let path = if pin.exists() {
        pin
    } else {
        paths.data.join("builds/eos/current.json")
    };
    if !path.exists() {
        return Ok(None);
    }
    let receipt: BuildReceipt = serde_json::from_slice(&fs::read(path)?)?;
    verify(&receipt)?;
    verify_source(paths, &receipt)?;
    Ok(Some(receipt))
}

pub fn verify_source(paths: &Paths, receipt: &BuildReceipt) -> Result<()> {
    if paths.mode()? == "product" {
        ensure!(
            !receipt.source_dirty && !receipt.source_fingerprint.is_empty(),
            "product mode requires a clean, fingerprinted EOS receipt"
        );
    }
    if paths.mode()? == "dev"
        && !receipt.source_fingerprint.is_empty()
        && let Some(root) = &receipt.source_root
        && root.is_dir()
    {
        let (commit, _, fingerprint) = source_identity(root)?;
        ensure!(
            commit == receipt.source_commit && fingerprint == receipt.source_fingerprint,
            "EOS source changed since selected build; run `isos build eos`, then `isos version select eos <commit>`"
        );
    }
    Ok(())
}

pub fn candidates(paths: &Paths) -> Result<Vec<BuildReceipt>> {
    let base = paths.data.join("builds/eos");
    if !base.exists() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for entry in fs::read_dir(base)? {
        let path = entry?.path().join("receipt.json");
        if path.is_file() {
            found.push(serde_json::from_slice(&fs::read(path)?)?);
        }
    }
    Ok(found)
}

pub fn select_eos(
    paths: &Paths,
    name: &str,
    version: &str,
    expected_revision: &str,
) -> Result<BuildReceipt> {
    crate::v2_process::validate_deployment_name(name)?;
    let matches: Vec<_> = candidates(paths)?
        .into_iter()
        .filter(|r| {
            r.source_commit == version
                || r.source_commit.starts_with(version)
                || r.product_version == version
        })
        .collect();
    ensure!(
        matches.len() == 1,
        "expected one EOS build matching {version}; found {}",
        matches.len()
    );
    let receipt = matches.into_iter().next().unwrap();
    verify(&receipt)?;
    verify_source(paths, &receipt)?;
    let _lock = crate::lifecycle::Lock::take(paths)?;
    let graph_path = paths.v2_node_graph(name);
    let previous = fs::read(&graph_path)?;
    ensure!(
        hex::encode(Sha256::digest(&previous)) == expected_revision,
        "node graph revision changed; reload before selecting EOS"
    );
    let graph: serde_json::Value = serde_json::from_slice(&previous)?;
    let suggested = suggest_graph(&graph, &receipt)?;
    crate::v2_process::validate_value(paths, &suggested)?;
    let pin = paths.config.join("pins/eos.json");
    let old_pin = fs::read(&pin).ok();
    config::atomic_write(&pin, &serde_json::to_vec_pretty(&receipt)?)?;
    if let Err(error) = config::atomic_write(&graph_path, &serde_json::to_vec_pretty(&suggested)?) {
        if let Some(old) = old_pin {
            let _ = config::atomic_write(&pin, &old);
        } else {
            let _ = fs::remove_file(&pin);
        }
        return Err(error);
    }
    Ok(receipt)
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
    fn isos_receipt_rejects_artifact_damage() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("isos");
        fs::write(&executable, b"first").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let receipt = IsosReceipt {
            schema_version: 1,
            product_version: env!("CARGO_PKG_VERSION").into(),
            source_commit: "a".repeat(40),
            source_dirty: false,
            source_fingerprint: "b".repeat(64),
            source_root: None,
            executable: executable.clone(),
            sha256: digest(&executable).unwrap(),
        };
        verify_isos(&receipt).unwrap();
        fs::write(executable, b"second").unwrap();
        assert!(verify_isos(&receipt).is_err());
    }

    #[test]
    fn source_identity_changes_with_uncommitted_source() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for args in [
            vec!["init", "-q"],
            vec![
                "-c",
                "user.name=IsOS Test",
                "-c",
                "user.email=test@localhost",
                "commit",
                "-qm",
                "initial",
                "--allow-empty",
            ],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let (_, clean, before) = source_identity(root).unwrap();
        assert!(!clean);
        fs::write(root.join("source.rs"), "one").unwrap();
        let (_, dirty, after) = source_identity(root).unwrap();
        assert!(dirty);
        assert_ne!(before, after);
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
            source_dirty: false,
            source_fingerprint: String::new(),
            source_root: None,
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
