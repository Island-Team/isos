//! Verified local package store. Only explicit install changes the store.

use crate::{
    config::{self, Paths},
    lifecycle::Lock,
    release::{self, Release},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
};

pub const CATALOG: &str = "https://www.os-island.com";

#[derive(Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub source_commit: Option<String>,
    pub asset_sha256: Option<String>,
    pub local_link: Option<PathBuf>,
}

pub fn package_path(paths: &Paths, id: &str, version: &str) -> Result<PathBuf> {
    let (namespace, name) = id
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("package ID must be namespace/name"))?;
    let slug = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    };
    ensure!(slug(namespace) && slug(name), "invalid package ID");
    ensure!(
        !version.is_empty()
            && version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-' || c == b'+'),
        "invalid version"
    );
    Ok(paths
        .data
        .join("packages")
        .join(namespace)
        .join(name)
        .join(version))
}

fn curl(url: &str, output: Option<&Path>) -> Result<Vec<u8>> {
    ensure!(url.starts_with("https://"), "package URLs require HTTPS");
    let mut cmd = Command::new("curl");
    cmd.arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        .arg("--location")
        .arg("--proto")
        .arg("=https")
        .arg("--proto-redir")
        .arg("=https")
        .arg("--max-time")
        .arg("60")
        .arg("--max-redirs")
        .arg("5");
    if let Some(path) = output {
        cmd.arg("--output").arg(path);
    }
    let result = cmd
        .arg(url)
        .output()
        .with_context(|| "curl is required to fetch releases")?;
    ensure!(
        result.status.success(),
        "download failed: {}",
        String::from_utf8_lossy(&result.stderr).trim()
    );
    Ok(result.stdout)
}

pub fn catalog_release(id: &str, version: &str) -> Result<Release> {
    let (namespace, name) = id
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("package ID must be namespace/name"))?;
    let url = format!("{CATALOG}/api/packages/v1/packages/{namespace}/{name}/releases/{version}");
    let response: serde_json::Value = serde_json::from_slice(&curl(&url, None)?)?;
    ensure!(
        response["success"] == true,
        "catalog: {}",
        response["error"]["message"]
    );
    let release: Release = serde_json::from_value(response["data"].clone())
        .context("catalog release is not IsOS schema v4 yet; use a local reviewed v4 manifest")?;
    release.validate()?;
    Ok(release)
}

pub fn read_manifest(path: &Path) -> Result<Release> {
    let release: Release = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("read {}", path.display()))?,
    )?;
    release.validate()?;
    Ok(release)
}

fn unpack_archive(archive: &Path, destination: &Path) -> Result<()> {
    let file = fs::File::open(archive)?;
    let reader: Box<dyn Read> = if archive.to_string_lossy().ends_with(".gz") {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut tar = tar::Archive::new(reader);
    let mut total = 0_u64;
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        ensure!(
            !path.as_os_str().is_empty()
                && path.components().all(|c| matches!(c, Component::Normal(_))),
            "archive contains an unsafe path: {}",
            path.display()
        );
        ensure!(
            entry.header().entry_type().is_file() || entry.header().entry_type().is_dir(),
            "archive contains a link or unsupported entry: {}",
            path.display()
        );
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| anyhow::anyhow!("archive is too large"))?;
        ensure!(total <= 4_000_000_000, "archive expands beyond 4 GB limit");
        let target = destination.join(&path);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            fs::create_dir_all(target.parent().unwrap())?;
            let mut out = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            std::io::copy(&mut entry, &mut out)?;
            out.flush()?;
            let executable = entry.header().mode()? & 0o111 != 0;
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode(if executable { 0o700 } else { 0o600 }),
            )?;
        }
    }
    Ok(())
}

pub fn install(
    paths: &Paths,
    release: &Release,
    allow_unapproved: bool,
    source: Option<&Path>,
) -> Result<PathBuf> {
    release.validate()?;
    ensure!(
        allow_unapproved || source.is_none(),
        "local manifests require --allow-unapproved"
    );
    let _lock = Lock::take(paths)?;
    let asset = release.select_asset(&release::Platform::current())?;
    let target = package_path(paths, &release.id, &release.version)?;
    ensure!(
        !target.exists(),
        "{} {} is already installed",
        release.id,
        release.version
    );
    fs::create_dir_all(target.parent().unwrap())?;
    let staging = target.with_extension(format!("{}.staging", std::process::id()));
    fs::create_dir(&staging)?;
    let result = (|| -> Result<()> {
        let archive = staging.join("asset.tar.gz");
        if let Some(local) = source {
            fs::copy(local, &archive)?;
        } else {
            curl(&asset.url, Some(&archive))?;
        }
        release::verify_file(asset, &archive)?;
        let payload = staging.join("payload");
        fs::create_dir(&payload)?;
        unpack_archive(&archive, &payload)?;
        fs::remove_file(&archive)?;
        let receipt = Receipt {
            schema_version: 1,
            id: release.id.clone(),
            version: release.version.clone(),
            source_commit: Some(release.source_commit.clone()),
            asset_sha256: Some(asset.sha256.clone()),
            local_link: None,
        };
        config::atomic_write(
            &payload.join("receipt.json"),
            &serde_json::to_vec_pretty(&receipt)?,
        )?;
        config::atomic_write(
            &payload.join("release.json"),
            &serde_json::to_vec_pretty(release)?,
        )?;
        fs::rename(&payload, &target)?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    result?;
    Ok(target)
}

pub fn link(paths: &Paths, id: &str, version: &str, source: &Path) -> Result<PathBuf> {
    let _lock = Lock::take(paths)?;
    let source = source
        .canonicalize()
        .with_context(|| format!("source {} is missing", source.display()))?;
    ensure!(source.is_dir(), "linked source must be a directory");
    let target = package_path(paths, id, version)?;
    ensure!(!target.exists(), "package version is already installed");
    fs::create_dir_all(&target)?;
    let receipt = Receipt {
        schema_version: 1,
        id: id.into(),
        version: version.into(),
        source_commit: None,
        asset_sha256: None,
        local_link: Some(source),
    };
    config::atomic_write(
        &target.join("receipt.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(target)
}

pub fn list(paths: &Paths) -> Result<Vec<Receipt>> {
    let base = paths.data.join("packages");
    if !base.exists() {
        return Ok(Vec::new());
    }
    let mut receipts: Vec<Receipt> = Vec::new();
    for ns in fs::read_dir(base)? {
        for name in fs::read_dir(ns?.path())? {
            for version in fs::read_dir(name?.path())? {
                let path = version?.path().join("receipt.json");
                if path.is_file() {
                    receipts.push(serde_json::from_slice(&fs::read(path)?)?);
                }
            }
        }
    }
    receipts.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));
    Ok(receipts)
}

pub fn remove(paths: &Paths, id: &str, version: &str) -> Result<()> {
    let _lock = Lock::take(paths)?;
    let path = package_path(paths, id, version)?;
    ensure!(
        path.join("receipt.json").is_file(),
        "package version is not installed"
    );
    for deployment in fs::read_dir(paths.config.join("deployments"))
        .into_iter()
        .flatten()
        .flatten()
    {
        if let Ok(graph) = config::load(&deployment.path()) {
            ensure!(
                !graph.nodes.iter().any(|n| n
                    .package
                    .as_ref()
                    .is_some_and(|p| p.id == id && p.version == version)),
                "package is referenced by deployment {}; remove that reference first",
                graph.name
            );
        }
    }
    for entry in fs::read_dir(&paths.runtime).into_iter().flatten().flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .ends_with(".active.json")
        {
            continue;
        }
        let active: crate::lifecycle::ActiveState =
            serde_json::from_slice(&fs::read(entry.path())?)?;
        ensure!(
            !active.graph.nodes.iter().any(|node| node
                .package
                .as_ref()
                .is_some_and(|package| package.id == id && package.version == version)),
            "package is referenced by an active deployment; stop it before removal"
        );
    }
    fs::remove_dir_all(path)?;
    Ok(())
}

pub fn search(query: &str) -> Result<serde_json::Value> {
    ensure!(
        query
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b' '),
        "search query contains unsupported characters"
    );
    let encoded = query.replace(' ', "%20");
    let url = format!("{CATALOG}/api/packages/v1/packages?q={encoded}");
    let response: serde_json::Value = serde_json::from_slice(&curl(&url, None)?)?;
    ensure!(response["success"] == true, "catalog search failed");
    Ok(response["data"].clone())
}
