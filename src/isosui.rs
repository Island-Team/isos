//! Explicit local IsosUI builds, pins, and integrity checks.
use crate::config::{self, Paths};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub version: String,
    pub source_commit: String,
    pub api_schema_version: u32,
    pub build_target: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub schema_version: u32,
    pub manifest: Manifest,
    pub directory: PathBuf,
    pub sha256: String,
}

fn build_root(paths: &Paths) -> PathBuf {
    paths.data.join("builds/isosui")
}

fn pin_path(paths: &Paths) -> PathBuf {
    paths.config.join("pins/isosui.json")
}

fn manifest(dir: &Path) -> Result<Manifest> {
    let value: Manifest = serde_json::from_slice(&fs::read(dir.join("manifest.json"))?)?;
    ensure!(value.schema_version == 1, "unknown IsosUI manifest schema");
    ensure!(
        value.api_schema_version == 2,
        "IsosUI requires unsupported API schema"
    );
    ensure!(
        value.build_target == "local",
        "IsosUI build must target local"
    );
    ensure!(
        value.source_commit.len() == 40
            && value.source_commit.bytes().all(|c| c.is_ascii_hexdigit()),
        "IsosUI source_commit must be a full Git SHA"
    );
    ensure!(
        dir.join("index.html").is_file(),
        "IsosUI index.html missing"
    );
    Ok(value)
}

fn files(dir: &Path, prefix: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "IsosUI bundle contains symlink"
        );
        if metadata.is_dir() {
            files(&path, prefix, found)?;
        } else {
            ensure!(metadata.is_file(), "IsosUI bundle contains special file");
            found.push(path.strip_prefix(prefix)?.to_path_buf());
        }
    }
    Ok(())
}

fn digest(dir: &Path) -> Result<String> {
    let mut list = Vec::new();
    files(dir, dir, &mut list)?;
    list.sort();
    let mut hasher = Sha256::new();
    for relative in list {
        let name = relative
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non-UTF-8 IsosUI path"))?;
        let bytes = fs::read(dir.join(&relative))?;
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let destination = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "IsosUI bundle contains symlink"
        );
        if metadata.is_dir() {
            copy_tree(&path, &destination)?;
        } else {
            ensure!(metadata.is_file(), "IsosUI bundle contains special file");
            fs::copy(&path, &destination)?;
        }
    }
    Ok(())
}

fn command(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("run {program}"))?;
    ensure!(
        output.status.success(),
        "{program} {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

pub fn build(paths: &Paths) -> Result<Receipt> {
    let root = std::env::var_os("ISOSUI_REPO_PATH")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("ISOSUI_REPO_PATH is required for isos build isosui"))?;
    ensure!(root.is_dir(), "ISOSUI_REPO_PATH is not a directory");
    let root = root.canonicalize()?;
    let commit = command(&root, "git", &["rev-parse", "HEAD"])?;
    ensure!(
        command(&root, "git", &["status", "--porcelain"])?.is_empty(),
        "IsosUI source has uncommitted changes; commit first"
    );
    command(&root, "npm", &["ci"])?;
    command(&root, "npm", &["run", "build:local"])?;
    let output = root.join("dist/local");
    let identity = manifest(&output)?;
    ensure!(
        identity.source_commit == commit,
        "IsosUI manifest source_commit differs from checkout"
    );
    let source_hash = digest(&output)?;
    let destination = build_root(paths).join(&commit);
    if destination.exists() {
        ensure!(
            digest(&destination)? == source_hash,
            "existing IsosUI build differs; remove it explicitly before rebuilding"
        );
    } else {
        let staging = build_root(paths).join(format!(".{commit}-{}", std::process::id()));
        ensure!(
            !staging.exists(),
            "IsosUI build staging directory already exists"
        );
        copy_tree(&output, &staging)?;
        ensure!(
            digest(&staging)? == source_hash,
            "IsosUI copy changed during build"
        );
        fs::rename(&staging, &destination)?;
    }
    let receipt = Receipt {
        schema_version: 1,
        manifest: identity,
        directory: destination,
        sha256: source_hash,
    };
    config::atomic_write(
        &build_root(paths).join(format!("{commit}.json")),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}

fn verify(receipt: &Receipt) -> Result<()> {
    ensure!(
        receipt.schema_version == 1,
        "unknown IsosUI build receipt schema"
    );
    ensure!(
        manifest(&receipt.directory)? == receipt.manifest,
        "IsosUI manifest changed"
    );
    ensure!(
        digest(&receipt.directory)? == receipt.sha256,
        "IsosUI build digest changed"
    );
    Ok(())
}

fn candidates(paths: &Paths) -> Result<Vec<Receipt>> {
    let root = build_root(paths);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut receipts = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            receipts.push(serde_json::from_slice(&fs::read(path)?)?);
        }
    }
    Ok(receipts)
}

fn read_pin(paths: &Paths) -> Result<Option<Receipt>> {
    let path = pin_path(paths);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}

pub fn pin(paths: &Paths, version: &str) -> Result<Receipt> {
    let matches: Vec<_> = candidates(paths)?
        .into_iter()
        .filter(|r| {
            r.manifest.version == version
                || r.manifest.source_commit == version
                || r.manifest.source_commit.starts_with(version)
        })
        .collect();
    ensure!(
        matches.len() == 1,
        "expected one local IsosUI build matching {version}; found {}",
        matches.len()
    );
    let receipt = matches.into_iter().next().unwrap();
    verify(&receipt)?;
    config::atomic_write(&pin_path(paths), &serde_json::to_vec_pretty(&receipt)?)?;
    Ok(receipt)
}

pub fn pinned_dir(paths: &Paths) -> Result<Option<PathBuf>> {
    let Some(receipt) = read_pin(paths)? else {
        return Ok(None);
    };
    verify(&receipt)?;
    Ok(Some(receipt.directory))
}

pub fn check(paths: &Paths) -> Result<Value> {
    let available: Vec<_> = candidates(paths)?
        .into_iter()
        .map(|r| json!({"version":r.manifest.version,"commit":r.manifest.source_commit}))
        .collect();
    if let Some(receipt) = read_pin(paths)? {
        verify(&receipt)?;
        Ok(
            json!({"status":"pinned","version":receipt.manifest.version,"commit":receipt.manifest.source_commit,"sha256":receipt.sha256,"available_local_builds":available}),
        )
    } else {
        Ok(json!({"status":"unpinned","available_local_builds":available}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_prefers_verified_pin_and_rejects_damage() {
        let temporary = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temporary.path().join("config"),
            data: temporary.path().join("data"),
            runtime: temporary.path().join("run"),
        };
        paths.prepare().unwrap();
        assert!(crate::http::open_url(&paths).is_err());
        let commit = "a".repeat(40);
        let directory = build_root(&paths).join(&commit);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("index.html"), "<html></html>").unwrap();
        let identity = Manifest {
            schema_version: 1,
            version: "0.1.0".into(),
            source_commit: commit.clone(),
            api_schema_version: 2,
            build_target: "local".into(),
        };
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(&identity).unwrap(),
        )
        .unwrap();
        let receipt = Receipt {
            schema_version: 1,
            manifest: identity,
            directory: directory.clone(),
            sha256: digest(&directory).unwrap(),
        };
        config::atomic_write(&pin_path(&paths), &serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert!(
            crate::http::open_url(&paths)
                .unwrap()
                .eq("http://127.0.0.1:16000/en/isos")
        );
        fs::write(directory.join("index.html"), "damaged").unwrap();
        assert!(crate::http::open_url(&paths).is_err());
    }
}
