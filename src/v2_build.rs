//! Verify configured Rust Aether executables before launching the v2 graph.
use crate::config::Paths;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{path::Path, process::Command};

pub fn verify_active_isos(paths: &Paths) -> Result<()> {
    let _ = paths;
    let executable = std::env::current_exe()?;
    ensure!(executable.is_file(), "IsOS executable is missing");
    Ok(())
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
        "{binary} at {} lacks Rust v2 capabilities; build EOS and configure ISOS_AETHER_BIN_DIR: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let info: Capabilities = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "{binary} at {} returned invalid capabilities; build EOS and configure ISOS_AETHER_BIN_DIR",
            path.display()
        )
    })?;
    ensure!(
        info.binary == binary
            && info.product_version == "2.0.0"
            && info.graph_version == "2.0.0"
            && info.node_type_version == "1.0",
        "{binary} at {} has incompatible Aether versions; build EOS and configure ISOS_AETHER_BIN_DIR",
        path.display()
    );
    for feature in features {
        ensure!(
            info.features.iter().any(|candidate| candidate == feature),
            "{binary} at {} lacks {feature}; build EOS and configure ISOS_AETHER_BIN_DIR",
            path.display()
        );
    }
    Ok(())
}

pub fn preflight_processes(paths: &Paths, specs: &[crate::v2_process::ProcessSpec]) -> Result<()> {
    crate::core_setup::identity(paths, "eos")?;
    if specs.iter().any(|spec| spec.id == "egia") {
        crate::core_setup::identity(paths, "egia")?;
    }
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
        let expected = crate::v2_bins::resolve(binary)?;
        ensure!(
            path.canonicalize()? == expected.canonicalize()?,
            "{id} launch path differs from configured EOS binary {}; reload the configured graph",
            expected.display()
        );
        probe(path, binary, features)?;
    }
    let profile = crate::v2_bins::resolve("aether-profile")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn capability_probe_rejects_stale_runtime() {
        let temporary = tempfile::tempdir().unwrap();
        let binary = temporary.path().join("aether-runtime");
        fs::write(&binary, "#!/bin/sh\nprintf '%s\\n' '{\"product_version\":\"2.0.0\",\"graph_version\":\"2.0.0\",\"node_type_version\":\"1.0\",\"binary\":\"aether-runtime\",\"features\":[\"log-dir\",\"node-types\"]}'\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        probe(&binary, "aether-runtime", &["log-dir", "node-types"]).unwrap();
        assert!(probe(&binary, "aether-runtime", &["primitive-binding"]).is_err());
        assert!(probe(&binary, "aether-profile", &["log-dir"]).is_err());
    }

    #[test]
    fn active_isos_checks_current_executable() {
        let temporary = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temporary.path().join("config"),
            data: temporary.path().join("data"),
            runtime: temporary.path().join("run"),
        };
        assert!(verify_active_isos(&paths).is_ok());
    }
}
