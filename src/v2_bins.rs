//! Resolve the Aether executables used by the IsOS v2 graph.

use anyhow::{Result, ensure};
use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

fn runnable(path: &PathBuf) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn override_key(name: &str) -> String {
    format!("ISOS_{}_BIN", name.to_ascii_uppercase().replace('-', "_"))
}

fn required_in_dir(name: &str, dir: &Path) -> Result<PathBuf> {
    ensure!(dir.is_absolute(), "ISOS_AETHER_BIN_DIR must be absolute");
    let path = dir.join(name);
    ensure!(
        runnable(&path),
        "{} is missing or not executable; check ISOS_AETHER_BIN_DIR",
        path.display()
    );
    Ok(path)
}

pub fn resolve(name: &str) -> Result<PathBuf> {
    let key = override_key(name);
    if let Some(path) = env::var_os(&key) {
        let path = PathBuf::from(path);
        ensure!(
            path.is_absolute() && runnable(&path),
            "{key} must name an absolute executable file: {}",
            path.display()
        );
        return Ok(path);
    }
    if let Some(dir) = env::var_os("ISOS_AETHER_BIN_DIR") {
        return required_in_dir(name, &PathBuf::from(dir));
    }
    if let Ok(current) = env::current_exe()
        && let Some(dir) = current.parent()
    {
        let path = dir.join(name);
        if runnable(&path) {
            return Ok(path);
        }
    }
    if let Some(paths) = env::var_os("PATH") {
        for dir in env::split_paths(&paths) {
            let path = dir.join(name);
            if runnable(&path) {
                return Ok(path);
            }
        }
    }
    anyhow::bail!(
        "{name} is not installed; install the Aether v2 executables beside isos or set ISOS_AETHER_BIN_DIR to their absolute bin directory"
    )
}

pub fn draft_path(name: &str) -> String {
    if let Ok(path) = resolve(name) {
        return path.to_string_lossy().into_owned();
    }
    if let Some(path) = env::var_os(override_key(name)) {
        return PathBuf::from(path).to_string_lossy().into_owned();
    }
    if let Some(dir) = env::var_os("ISOS_AETHER_BIN_DIR") {
        return PathBuf::from(dir).join(name).to_string_lossy().into_owned();
    }
    // An editable graph can still be created before Aether is installed.
    let dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("/usr/bin"));
    dir.join(name).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn resolves_explicit_aether_directory_and_reports_missing_binary() {
        let temp = tempfile::tempdir().unwrap();
        let name = "aether-test-only-resolver";
        let path = temp.path().join(name);
        fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        let found = required_in_dir(name, temp.path()).unwrap();
        assert_eq!(found, path);
        assert!(required_in_dir("missing", temp.path()).is_err());
    }
}
