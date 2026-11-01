//! Verified local package store. Only explicit install changes the store.

use crate::{
    auth,
    config::{self, Paths},
    lock::Lock,
    release::{self, Release},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    os::unix::net::UnixStream,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidecar_sha256: Option<String>,
}

/// A bounded package error suitable for the public API and event stream.
#[derive(Debug)]
pub struct PackageFailure {
    pub code: &'static str,
    pub stage: &'static str,
    pub id: String,
    pub version: String,
    pub path: String,
}

impl std::fmt::Display for PackageFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Reinstall or relink {}@{}", self.id, self.version)
    }
}
impl std::error::Error for PackageFailure {}

impl PackageFailure {
    pub fn diagnostic(&self) -> serde_json::Value {
        serde_json::json!({"code":self.code,"path":self.path,"message":self.to_string(),
            "id":self.id,"version":self.version,"stage":self.stage})
    }
    pub fn at(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }
}

fn failure(id: &str, version: &str, code: &'static str, stage: &'static str) -> anyhow::Error {
    PackageFailure {
        code,
        stage,
        id: id.into(),
        version: version.into(),
        path: String::new(),
    }
    .into()
}

fn descriptor_failure(id: &str, version: &str, error: &anyhow::Error) -> anyhow::Error {
    let text = error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let code = if text.contains("symlink") || text.contains("unsafe") {
        "package_unsafe_path"
    } else if text.contains("No such file") || text.contains("not found") {
        "package_missing_file"
    } else {
        "package_invalid_descriptor"
    };
    failure(id, version, code, "descriptor")
}

/// A handle returned only after the exact receipt and payload pass verification.
#[derive(Debug)]
pub struct VerifiedPackage {
    pub root: PathBuf,
    pub store: PathBuf,
    pub release: Release,
    pub receipt: Receipt,
    pub sidecar: Option<PathBuf>,
}

pub fn package_path(paths: &Paths, id: &str, version: &str) -> Result<PathBuf> {
    let (namespace, name) = validate_identity(id, version)?;
    Ok(paths
        .data
        .join("packages")
        .join(namespace)
        .join(name)
        .join(version))
}

fn validate_identity<'a>(id: &'a str, version: &str) -> Result<(&'a str, &'a str)> {
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
        version.bytes().next().is_some_and(|b| b.is_ascii_digit())
            && version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-' || c == b'+'),
        "invalid version"
    );
    Ok((namespace, name))
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(value)?)
}

fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= max,
        "package metadata is too large"
    );
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= max, "package metadata is too large");
    Ok(bytes)
}

fn sidecar_bytes(release: &Release) -> Result<Option<Vec<u8>>> {
    let bytes = release
        .node_contributions
        .as_ref()
        .map(|contributions| {
            canonical_json(&serde_json::json!({
                "package":{"id":release.id,"version":release.version},
                "node_types":contributions.node_types,
                "safety_adapters":contributions.safety_adapters,
                "adapter_families":contributions.adapter_families,
                "data_types":contributions.data_types,
            }))
        })
        .transpose()?;
    ensure!(
        bytes.as_ref().is_none_or(|bytes| bytes.len() <= 4_194_304),
        "package sidecar is too large"
    );
    Ok(bytes)
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Hash every payload file and directory with a stable length-delimited format.
fn payload_digest(root: &Path, installed: bool) -> Result<String> {
    const MAX_ENTRIES: usize = 16_384;
    const MAX_BYTES: u64 = 4_000_000_000;
    let mut entries = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    let mut total = 0_u64;
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(root)?;
            let name = relative.to_str().context("package path is not UTF-8")?;
            if installed
                && !name.contains('/')
                && matches!(name, "receipt.json" | "release.json" | "node-types.json")
            {
                continue;
            }
            ensure!(name.len() <= 4096, "package path is too long");
            let metadata = fs::symlink_metadata(&path)?;
            let kind = metadata.file_type();
            ensure!(!kind.is_symlink(), "package contains a symlink");
            ensure!(
                kind.is_dir() || kind.is_file(),
                "package contains a special file"
            );
            ensure!(
                entries
                    .insert(name.to_owned(), (path.clone(), metadata))
                    .is_none(),
                "duplicate package path"
            );
            ensure!(entries.len() <= MAX_ENTRIES, "package has too many entries");
            if kind.is_dir() {
                pending.push(path);
            }
        }
    }
    let mut tree = Sha256::new();
    tree.update(b"isos-package-tree-v1\0");
    for (name, (path, before)) in entries {
        let name = name.as_bytes();
        tree.update(u32::try_from(name.len())?.to_le_bytes());
        tree.update(name);
        let file = before.is_file();
        tree.update([if file { b'F' } else { b'D' }]);
        tree.update([if file && before.permissions().mode() & 0o111 != 0 {
            b'1'
        } else {
            b'0'
        }]);
        tree.update((if file { before.len() } else { 0 }).to_le_bytes());
        let mut content = Sha256::new();
        if file {
            total = total
                .checked_add(before.len())
                .context("package size overflow")?;
            ensure!(total <= MAX_BYTES, "package exceeds 4 GB");
            let mut input = fs::File::open(&path)?;
            let opened = input.metadata()?;
            ensure!(
                opened.dev() == before.dev()
                    && opened.ino() == before.ino()
                    && opened.len() == before.len(),
                "package changed while hashing"
            );
            let mut buffer = [0_u8; 1024 * 1024];
            loop {
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                content.update(&buffer[..count]);
            }
            let after = fs::symlink_metadata(&path)?;
            ensure!(
                after.dev() == before.dev()
                    && after.ino() == before.ino()
                    && after.len() == before.len()
                    && after.mtime_nsec() == before.mtime_nsec()
                    && after.mtime() == before.mtime(),
                "package changed while hashing"
            );
            tree.update(content.finalize());
        } else {
            tree.update([0_u8; 32]);
        }
    }
    Ok(hex::encode(tree.finalize()))
}

pub(crate) fn checked_file(root: &Path, relative: &str, executable: bool) -> Result<PathBuf> {
    ensure!(
        !relative.is_empty()
            && !relative.starts_with('/')
            && relative
                .split('/')
                .all(|piece| !matches!(piece, "" | "." | "..")),
        "unsafe relative path"
    );
    let mut path = root.to_path_buf();
    let pieces: Vec<_> = relative.split('/').collect();
    for (index, piece) in pieces.iter().enumerate() {
        path.push(piece);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "package reference is a symlink"
        );
        ensure!(
            if index + 1 == pieces.len() {
                metadata.is_file()
            } else {
                metadata.is_dir()
            },
            "package reference type mismatch"
        );
    }
    if executable {
        ensure!(
            fs::metadata(&path)?.permissions().mode() & 0o111 != 0,
            "entrypoint is not executable"
        );
    }
    Ok(path)
}

fn checked_root(root: &Path) -> Result<PathBuf> {
    ensure!(root.is_absolute(), "package root must be absolute");
    let mut checked = PathBuf::from("/");
    for component in root.components() {
        if let Component::Normal(part) = component {
            checked.push(part);
            ensure!(
                !fs::symlink_metadata(&checked)?.file_type().is_symlink(),
                "package root contains a symlink"
            );
        }
    }
    ensure!(checked.is_dir(), "package root is not a directory");
    Ok(checked)
}

fn checked_manifest_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute
        .parent()
        .context("package manifest has no parent")?;
    let parent = checked_root(parent)?;
    let name = absolute
        .file_name()
        .context("package manifest has no file name")?;
    let file = parent.join(name);
    ensure!(
        !fs::symlink_metadata(&file)?.file_type().is_symlink(),
        "package manifest is a symlink"
    );
    ensure!(
        fs::symlink_metadata(&file)?.file_type().is_file(),
        "package manifest is not a regular file"
    );
    Ok(file)
}

fn validate_payload(root: &Path, release: &Release) -> Result<()> {
    if release.kind == "backend"
        && release.adapters.is_empty()
        && !root.join("backend-v2.json").is_file()
    {
        anyhow::bail!("backend release has no descriptor or adapter library");
    }
    verify_declared_adapter_libraries(root, release)?;
    if let Some(activation) = &release.activation {
        for file in &activation.files {
            checked_file(root, &file.source, false)?;
        }
    }
    if release.id == "island/mujoco-sim" || root.join("backend-v2.json").exists() {
        let path = checked_file(root, "backend-v2.json", false)?;
        let bytes = read_bounded(&path, 65_536)?;
        ensure!(bytes.len() <= 65_536, "backend descriptor is too large");
        let descriptor: crate::backend::BackendDescriptor = serde_json::from_slice(&bytes)?;
        ensure!(
            descriptor.schema_version == "2.0.0"
                && descriptor.protocol == "mujoco-v2"
                && descriptor.package.id == release.id
                && descriptor.package.version == release.version,
            "backend descriptor identity or protocol mismatch"
        );
        checked_file(root, &descriptor.entrypoint, true)?;
        ensure!(
            !descriptor.scenes.is_empty() && descriptor.scenes.len() <= 32,
            "backend scene count invalid"
        );
        let mut ids = BTreeSet::new();
        for scene in &descriptor.scenes {
            ensure!(
                !scene.id.is_empty() && ids.insert(&scene.id),
                "duplicate or empty scene ID"
            );
            checked_file(root, &scene.path, false)?;
        }
    }
    Ok(())
}

/// Revalidate an exact schema-2 receipt and return only verified payload paths.
pub fn verify(paths: &Paths, id: &str, version: &str) -> Result<VerifiedPackage> {
    let store = package_path(paths, id, version)?;
    let bytes = read_bounded(&store.join("receipt.json"), 65_536)
        .map_err(|_| failure(id, version, "package_invalid_manifest", "manifest"))?;
    let receipt: Receipt = serde_json::from_slice(&bytes)
        .map_err(|_| failure(id, version, "package_invalid_manifest", "manifest"))?;
    if receipt.schema_version != 2
        || receipt.id != id
        || receipt.version != version
        || !matches!(
            receipt.source_kind.as_deref(),
            Some("approved_catalog" | "local_candidate" | "local_link")
        )
    {
        return Err(failure(id, version, "package_invalid_manifest", "manifest"));
    }
    let release_bytes = read_bounded(&store.join("release.json"), 4_194_304)
        .map_err(|_| failure(id, version, "package_changed", "manifest"))?;
    if receipt.release_sha256.as_deref() != Some(digest(&release_bytes).as_str()) {
        return Err(failure(id, version, "package_changed", "manifest"));
    }
    let release: Release = serde_json::from_slice(&release_bytes)
        .map_err(|_| failure(id, version, "package_invalid_manifest", "manifest"))?;
    release
        .validate()
        .map_err(|_| failure(id, version, "package_invalid_manifest", "manifest"))?;
    if release.id != id
        || release.version != version
        || receipt.source_commit.as_deref() != Some(release.source_commit.as_str())
        || canonical_json(&release)? != release_bytes
    {
        return Err(failure(id, version, "package_changed", "manifest"));
    }
    let asset = release
        .select_asset(&release::Platform::current())
        .map_err(|_| failure(id, version, "package_platform_mismatch", "asset"))?;
    if receipt.local_link.is_some() {
        if receipt.source_kind.as_deref() != Some("local_link") || receipt.asset_sha256.is_some() {
            return Err(failure(id, version, "package_invalid_manifest", "manifest"));
        }
    } else if receipt.asset_sha256.as_deref()
        != Some(
            asset
                .sha256
                .strip_prefix("sha256:")
                .unwrap_or(&asset.sha256)
                .to_ascii_lowercase()
                .as_str(),
        )
        || receipt.source_kind.as_deref() == Some("local_link")
    {
        return Err(failure(id, version, "package_changed", "asset"));
    }
    let root = match &receipt.local_link {
        Some(path) => path.clone(),
        None => store.clone(),
    };
    let root = checked_root(&root).map_err(|error| {
        if error.to_string().contains("symlink") {
            failure(id, version, "package_unsafe_path", "payload")
        } else {
            failure(id, version, "package_missing_file", "payload")
        }
    })?;
    let payload = payload_digest(&root, receipt.local_link.is_none()).map_err(|error| {
        if error.to_string().contains("changed while hashing") {
            failure(id, version, "package_changed", "payload")
        } else {
            failure(id, version, "package_unsafe_path", "payload")
        }
    })?;
    if receipt.payload_sha256.as_deref() != Some(payload.as_str()) {
        return Err(failure(id, version, "package_changed", "payload"));
    }
    validate_payload(&root, &release).map_err(|error| descriptor_failure(id, version, &error))?;
    let projection = sidecar_bytes(&release)?;
    let sidecar = store.join("node-types.json");
    match projection {
        Some(expected) => {
            let actual = read_bounded(&sidecar, 4_194_304)
                .map_err(|_| failure(id, version, "package_invalid_sidecar", "sidecar"))?;
            if actual != expected
                || receipt.sidecar_sha256.as_deref() != Some(digest(&actual).as_str())
            {
                return Err(failure(id, version, "package_invalid_sidecar", "sidecar"));
            }
        }
        None if sidecar.exists() || receipt.sidecar_sha256.is_some() => {
            return Err(failure(id, version, "package_invalid_sidecar", "sidecar"));
        }
        None => (),
    }
    Ok(VerifiedPackage {
        root,
        store,
        release,
        receipt,
        sidecar: sidecar.exists().then_some(sidecar),
    })
}

/// Check every adapter payload against its release metadata before accepting a package.
pub fn verify_declared_adapter_libraries(root: &Path, release: &Release) -> Result<()> {
    let root = root
        .canonicalize()
        .with_context(|| format!("package root {} is missing", root.display()))?;
    ensure!(root.is_dir(), "adapter package root must be a directory");
    for adapter in &release.adapters {
        verify_adapter_file(&root, &adapter.library, &adapter.sha256).with_context(|| {
            format!(
                "adapter {} in {}@{}",
                adapter.family, release.id, release.version
            )
        })?;
    }
    Ok(())
}

fn verify_adapter_file(
    root: &Path,
    library: &str,
    expected_sha256: &str,
) -> Result<(PathBuf, String)> {
    let path = checked_file(root, library, false)?;
    let metadata = fs::metadata(&path)?;
    ensure!(
        metadata.len() <= 64 * 1024 * 1024,
        "adapter library exceeds 64 MiB"
    );
    let mut file = fs::File::open(&path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
    }
    let actual = hex::encode(hasher.finalize());
    ensure!(
        actual.eq_ignore_ascii_case(expected_sha256),
        "adapter library SHA-256 mismatch"
    );
    Ok((path, actual))
}

pub fn verified_adapter_library(
    paths: &Paths,
    id: &str,
    version: &str,
    family: &str,
) -> Result<(PathBuf, String)> {
    let verified = verify(paths, id, version)?;
    let adapter = verified
        .release
        .adapters
        .iter()
        .find(|adapter| adapter.family == family)
        .with_context(|| format!("package {id}@{version} has no {family} adapter library"))?;
    verify_adapter_file(&verified.root, &adapter.library, &adapter.sha256)
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
    } else {
        cmd.arg("--max-filesize").arg("4194304");
    }
    let result = cmd
        .arg(url)
        .output()
        .with_context(|| "curl is required to fetch releases")?;
    ensure!(result.status.success(), "package download failed");
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
    let release: Release = serde_json::from_slice(&read_bounded(path, 4_194_304)?)?;
    release.validate()?;
    Ok(release)
}

pub fn inspect_link_candidate(source: &Path, manifest: &Path) -> Result<Release> {
    let release = checked_manifest_path(manifest)
        .and_then(|path| read_manifest(&path))
        .map_err(|error| {
            failure(
                "unknown/package",
                "unknown",
                if error.to_string().contains("symlink") {
                    "package_unsafe_path"
                } else {
                    "package_invalid_manifest"
                },
                "manifest",
            )
        })?;
    release
        .select_asset(&release::Platform::current())
        .map_err(|_| {
            failure(
                &release.id,
                &release.version,
                "package_platform_mismatch",
                "asset",
            )
        })?;
    let source = if source.is_absolute() {
        source.to_path_buf()
    } else {
        std::env::current_dir()?.join(source)
    };
    let root = checked_root(&source).map_err(|error| {
        failure(
            &release.id,
            &release.version,
            if error.to_string().contains("symlink") {
                "package_unsafe_path"
            } else {
                "package_missing_file"
            },
            "payload",
        )
    })?;
    if !root.is_dir() {
        return Err(failure(
            &release.id,
            &release.version,
            "package_missing_file",
            "payload",
        ));
    }
    if release.kind == "backend"
        && release.adapters.is_empty()
        && !root.join("backend-v2.json").is_file()
    {
        return Err(failure(
            &release.id,
            &release.version,
            "package_invalid_descriptor",
            "descriptor",
        ));
    }
    validate_payload(&root, &release)
        .map_err(|error| descriptor_failure(&release.id, &release.version, &error))?;
    payload_digest(&root, false).map_err(|_| {
        failure(
            &release.id,
            &release.version,
            "package_unsafe_path",
            "payload",
        )
    })?;
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
    let mut seen = BTreeSet::new();
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
        ensure!(
            seen.insert(path.clone()),
            "archive contains a duplicate path"
        );
        ensure!(seen.len() <= 16_384, "archive contains too many entries");
        ensure!(
            !matches!(
                path.to_str(),
                Some("receipt.json" | "release.json" | "node-types.json")
            ),
            "archive contains a reserved metadata name"
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
    release.validate().map_err(|_| {
        failure(
            &release.id,
            &release.version,
            "package_invalid_manifest",
            "manifest",
        )
    })?;
    ensure!(
        allow_unapproved || source.is_none(),
        "local manifests require --allow-unapproved"
    );
    let _lock = Lock::take(paths)?;
    let asset = release
        .select_asset(&release::Platform::current())
        .map_err(|_| {
            failure(
                &release.id,
                &release.version,
                "package_platform_mismatch",
                "asset",
            )
        })?;
    let target = package_path(paths, &release.id, &release.version)?;
    if target.exists() {
        return Err(failure(
            &release.id,
            &release.version,
            "package_version_exists",
            "admission",
        ));
    }
    fs::create_dir_all(target.parent().unwrap())?;
    let staging_guard = tempfile::Builder::new()
        .prefix(".package-stage-")
        .tempdir_in(target.parent().unwrap())?;
    let staging = staging_guard.path();
    let result = (|| -> Result<()> {
        let archive = staging.join("asset.tar.gz");
        if let Some(local) = source {
            fs::copy(local, &archive)?;
        } else {
            curl(&asset.url, Some(&archive))?;
        }
        release::verify_file(asset, &archive).map_err(|_| {
            failure(
                &release.id,
                &release.version,
                "package_digest_mismatch",
                "asset",
            )
        })?;
        let payload = staging.join("payload");
        fs::create_dir(&payload)?;
        unpack_archive(&archive, &payload).map_err(|_| {
            failure(
                &release.id,
                &release.version,
                "package_unsafe_path",
                "extract",
            )
        })?;
        fs::remove_file(&archive)?;
        validate_payload(&payload, release)
            .map_err(|error| descriptor_failure(&release.id, &release.version, &error))?;
        let tree = payload_digest(&payload, false).map_err(|_| {
            failure(
                &release.id,
                &release.version,
                "package_unsafe_path",
                "payload",
            )
        })?;
        let release_bytes = canonical_json(release)?;
        let sidecar = sidecar_bytes(release)?;
        let receipt = Receipt {
            schema_version: 2,
            id: release.id.clone(),
            version: release.version.clone(),
            source_commit: Some(release.source_commit.clone()),
            asset_sha256: Some(
                asset
                    .sha256
                    .strip_prefix("sha256:")
                    .unwrap_or(&asset.sha256)
                    .to_ascii_lowercase(),
            ),
            local_link: None,
            source_kind: Some(
                if source.is_some() {
                    "local_candidate"
                } else {
                    "approved_catalog"
                }
                .into(),
            ),
            release_sha256: Some(digest(&release_bytes)),
            payload_sha256: Some(tree),
            sidecar_sha256: sidecar.as_ref().map(|bytes| digest(bytes)),
        };
        config::atomic_write(&payload.join("receipt.json"), &canonical_json(&receipt)?)?;
        config::atomic_write(&payload.join("release.json"), &release_bytes)?;
        if let Some(sidecar) = sidecar {
            config::atomic_write(&payload.join("node-types.json"), &sidecar)?;
        }
        for name in ["receipt.json", "release.json", "node-types.json"] {
            let path = payload.join(name);
            if path.exists() {
                fs::File::open(path)?.sync_all()?;
            }
        }
        fs::File::open(&payload)?.sync_all()?;
        fs::rename(&payload, &target)?;
        fs::File::open(target.parent().unwrap())?.sync_all()?;
        Ok(())
    })();
    result?;
    Ok(target)
}

#[cfg(test)]
pub fn link(paths: &Paths, id: &str, version: &str, source: &Path) -> Result<PathBuf> {
    link_with_manifest(paths, id, version, source, None)
}

pub fn link_with_manifest(
    paths: &Paths,
    id: &str,
    version: &str,
    source: &Path,
    manifest: Option<&Path>,
) -> Result<PathBuf> {
    let _lock = Lock::take(paths)?;
    let source = if source.is_absolute() {
        source.to_path_buf()
    } else {
        std::env::current_dir()?.join(source)
    };
    let source = checked_root(&source).map_err(|error| {
        failure(
            id,
            version,
            if error.to_string().contains("symlink") {
                "package_unsafe_path"
            } else {
                "package_missing_file"
            },
            "payload",
        )
    })?;
    let discovered = source.join("release.json");
    let manifest = manifest.or_else(|| {
        fs::symlink_metadata(&discovered)
            .is_ok()
            .then_some(discovered.as_path())
    });
    let release = manifest
        .map(|path| {
            checked_manifest_path(path)
                .and_then(|path| read_manifest(&path))
                .map_err(|error| {
                    failure(
                        id,
                        version,
                        if error.to_string().contains("symlink") {
                            "package_unsafe_path"
                        } else {
                            "package_invalid_manifest"
                        },
                        "manifest",
                    )
                })
        })
        .transpose()?;
    if let Some(release) = &release
        && (release.id != id || release.version != version)
    {
        return Err(failure(id, version, "package_invalid_manifest", "manifest"));
    }
    let target = package_path(paths, id, version)?;
    if target.exists() {
        return Err(failure(id, version, "package_version_exists", "admission"));
    }
    fs::create_dir_all(target.parent().unwrap())?;
    let staging_guard = tempfile::Builder::new()
        .prefix(".package-stage-")
        .tempdir_in(target.parent().unwrap())?;
    let staging = staging_guard.path();
    let result = (|| -> Result<()> {
        let (tree, release_bytes, sidecar) = if let Some(release) = &release {
            validate_payload(&source, release)
                .map_err(|error| descriptor_failure(id, version, &error))?;
            (
                Some(
                    payload_digest(&source, false)
                        .map_err(|_| failure(id, version, "package_unsafe_path", "payload"))?,
                ),
                Some(canonical_json(release)?),
                sidecar_bytes(release)?,
            )
        } else {
            (None, None, None)
        };
        let receipt = Receipt {
            schema_version: if release.is_some() { 2 } else { 1 },
            id: id.into(),
            version: version.into(),
            source_commit: release
                .as_ref()
                .map(|release| release.source_commit.clone()),
            asset_sha256: None,
            local_link: Some(source),
            source_kind: release.as_ref().map(|_| "local_link".into()),
            release_sha256: release_bytes.as_ref().map(|bytes| digest(bytes)),
            payload_sha256: tree,
            sidecar_sha256: sidecar.as_ref().map(|bytes| digest(bytes)),
        };
        config::atomic_write(&staging.join("receipt.json"), &canonical_json(&receipt)?)?;
        if let Some(bytes) = release_bytes {
            config::atomic_write(&staging.join("release.json"), &bytes)?;
        }
        if let Some(bytes) = sidecar {
            config::atomic_write(&staging.join("node-types.json"), &bytes)?;
        }
        for name in ["receipt.json", "release.json", "node-types.json"] {
            let path = staging.join(name);
            if path.exists() {
                fs::File::open(path)?.sync_all()?;
            }
        }
        fs::File::open(staging)?.sync_all()?;
        fs::rename(staging, &target)?;
        fs::File::open(target.parent().unwrap())?.sync_all()?;
        Ok(())
    })();
    result?;
    Ok(target)
}

pub fn node_type_sidecars(paths: &Paths) -> Result<Vec<PathBuf>> {
    let mut sidecars = Vec::new();
    for receipt in list(paths)? {
        if let Ok(verified) = verify(paths, &receipt.id, &receipt.version)
            && let Some(file) = verified.sidecar
        {
            sidecars.push(file);
        }
    }
    Ok(sidecars)
}

pub fn node_type_sidecars_for_graph(
    paths: &Paths,
    graph: &serde_json::Value,
) -> Result<Vec<PathBuf>> {
    Ok(verify_graph_packages(paths, graph)?
        .into_iter()
        .filter_map(|package| package.sidecar)
        .collect())
}

#[derive(Debug, Clone)]
pub struct PackagePin {
    pub id: String,
    pub version: String,
    pub path: String,
}

/// Collect only canonical, complete package pins declared by graph nodes.
pub fn collect_pins(graph: &serde_json::Value) -> Result<Vec<PackagePin>> {
    let mut pins = BTreeMap::new();
    let Some(cores) = graph.get("cores").and_then(serde_json::Value::as_object) else {
        return Ok(Vec::new());
    };
    for (core_id, core) in cores {
        let Some(layers) = core["layers"].as_object() else {
            continue;
        };
        for (layer_id, layer) in layers {
            let Some(groups) = layer["groups"].as_object() else {
                continue;
            };
            for (group_id, group) in groups {
                let Some(nodes) = group["nodes"].as_object() else {
                    continue;
                };
                for (node_id, node) in nodes {
                    let base = format!(
                        "/cores/{core_id}/layers/{layer_id}/groups/{group_id}/nodes/{node_id}"
                    );
                    for (path, package) in [
                        (format!("{base}/package"), node.get("package")),
                        (
                            format!("{base}/adapter/package"),
                            node.get("adapter")
                                .and_then(|adapter| adapter.get("package")),
                        ),
                    ] {
                        if let Some(package) = package {
                            let id = package
                                .get("id")
                                .and_then(serde_json::Value::as_str)
                                .ok_or_else(|| {
                                    anyhow::anyhow!("incomplete package pin at {path}")
                                })?;
                            let version = package
                                .get("version")
                                .and_then(serde_json::Value::as_str)
                                .ok_or_else(|| {
                                    anyhow::anyhow!("incomplete package pin at {path}")
                                })?;
                            validate_identity(id, version)
                                .with_context(|| format!("invalid package pin at {path}"))?;
                            pins.entry((id.to_owned(), version.to_owned()))
                                .or_insert(path);
                        }
                    }
                    let settings = &node["settings"];
                    if !settings["package"].is_null() || !settings["version"].is_null() {
                        let path = format!("{base}/settings/package");
                        let id = settings["package"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("incomplete package pin at {path}"))?;
                        let version = settings["version"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("incomplete package pin at {path}"))?;
                        validate_identity(id, version)
                            .with_context(|| format!("invalid package pin at {path}"))?;
                        pins.entry((id.to_owned(), version.to_owned()))
                            .or_insert(path);
                    }
                }
            }
        }
    }
    Ok(pins
        .into_iter()
        .map(|((id, version), path)| PackagePin { id, version, path })
        .collect())
}

pub fn verify_graph_packages(
    paths: &Paths,
    graph: &serde_json::Value,
) -> Result<Vec<VerifiedPackage>> {
    collect_pins(graph)?
        .into_iter()
        .map(|pin| {
            verify(paths, &pin.id, &pin.version).map_err(|error| {
                match error.downcast::<PackageFailure>() {
                    Ok(failure) => anyhow::Error::new(failure.at(pin.path)),
                    Err(error) => error,
                }
            })
        })
        .collect()
}

fn node_package_pins(node: &serde_json::Value) -> Vec<(String, String)> {
    let mut pins = Vec::new();
    for package in [&node["package"], &node["adapter"]["package"]] {
        if let (Some(id), Some(version)) = (package["id"].as_str(), package["version"].as_str()) {
            pins.push((id.to_owned(), version.to_owned()));
        }
    }
    if let (Some(id), Some(version)) = (
        node["settings"]["package"].as_str(),
        node["settings"]["version"].as_str(),
    ) {
        pins.push((id.to_owned(), version.to_owned()));
    }
    pins
}

#[cfg(test)]
mod pin_tests {
    use super::*;

    #[test]
    fn adapter_only_package_pin_selects_sidecar_and_blocks_removal() {
        let node = serde_json::json!({"node_type":"eos.hardware.base@v1.0","adapter":{"type":"vendor-base","package":{"id":"vendor/devices","version":"2.0.0"}}});
        assert_eq!(
            node_package_pins(&node),
            vec![("vendor/devices".into(), "2.0.0".into())]
        );
    }
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
                if path.is_file()
                    && let Ok(receipt) = read_bounded(&path, 65_536)
                        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(Into::into))
                {
                    receipts.push(receipt);
                }
            }
        }
    }
    receipts.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));
    Ok(receipts)
}

pub fn listing(paths: &Paths) -> Result<Vec<serde_json::Value>> {
    let base = paths.data.join("packages");
    if !base.exists() {
        return Ok(Vec::new());
    }
    let mut rows = Vec::new();
    for namespace in fs::read_dir(base)? {
        let namespace = namespace?;
        for name in fs::read_dir(namespace.path())? {
            let name = name?;
            for version in fs::read_dir(name.path())? {
                let version = version?;
                let id = format!(
                    "{}/{}",
                    namespace.file_name().to_string_lossy(),
                    name.file_name().to_string_lossy()
                );
                let version = version.file_name().to_string_lossy().into_owned();
                if validate_identity(&id, &version).is_err() {
                    continue;
                }
                let receipt = read_bounded(
                    &package_path(paths, &id, &version)?.join("receipt.json"),
                    65_536,
                )
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Receipt>(&bytes).ok());
                let checked = verify(paths, &id, &version);
                let mut row = receipt
                    .as_ref()
                    .and_then(|receipt| serde_json::to_value(receipt).ok())
                    .unwrap_or_else(|| serde_json::json!({"id":id,"version":version}));
                row["available"] = serde_json::json!(checked.is_ok());
                row["reason"] = if checked.is_ok() {
                    serde_json::Value::Null
                } else {
                    serde_json::json!(format!("Reinstall or relink {id}@{version}"))
                };
                rows.push(row);
            }
        }
    }
    rows.sort_by(|a, b| {
        (a["id"].as_str(), a["version"].as_str()).cmp(&(b["id"].as_str(), b["version"].as_str()))
    });
    Ok(rows)
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
        if deployment
            .file_name()
            .to_string_lossy()
            .ends_with(".nodes.json")
        {
            let graph: serde_json::Value = serde_json::from_slice(&fs::read(deployment.path())?)?;
            if graph
                .pointer("/cores")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|cores| {
                    cores.values().any(|core| {
                        core["layers"].as_object().is_some_and(|layers| {
                            layers.values().any(|layer| {
                                layer["groups"].as_object().is_some_and(|groups| {
                                    groups.values().any(|group| {
                                        group["nodes"].as_object().is_some_and(|nodes| {
                                            nodes.values().any(|node| {
                                                node_package_pins(node).iter().any(
                                                    |(pin_id, pin_version)| {
                                                        pin_id == id && pin_version == version
                                                    },
                                                )
                                            })
                                        })
                                    })
                                })
                            })
                        })
                    })
                })
            {
                anyhow::bail!(
                    "package is referenced by an applied node graph; remove that reference first"
                );
            }
        }
    }
    if UnixStream::connect(paths.socket()).is_ok() {
        let active = crate::manager::manager_call(paths, "default", "deployment_active_packages")?;
        ensure!(
            !active["packages"]
                .as_array()
                .is_some_and(|packages| packages
                    .iter()
                    .any(|package| package["id"] == id && package["version"] == version)),
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

pub fn publish(paths: &Paths, manifest_path: &Path) -> Result<()> {
    let credentials = auth::read(paths)?;
    ensure!(
        credentials.user.role == "developer",
        "package publication requires an Island-port developer account; signed-in role is {}",
        credentials.user.role
    );
    let release = read_manifest(manifest_path)?;
    let source_url = release
        .source_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("manifest source_url is required for publication"))?;
    let release_url = release
        .release_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("manifest release_url is required for publication"))?;
    let body = serde_json::json!({
        "package_id": release.id,
        "version": release.version,
        "source_url": source_url,
        "release_url": release_url,
        "manifest": release,
    });
    let response = auth::authorized_post(
        paths,
        &format!("{CATALOG}/api/packages/v1/submissions"),
        &body,
    )?;
    let status = response.status();
    let envelope: serde_json::Value = response
        .json()
        .context("Island-port returned an invalid publication response")?;
    ensure!(
        status.is_success() && envelope["success"] == true,
        "Island-port publication failed: {} ({})",
        envelope["error"]["message"]
            .as_str()
            .unwrap_or("request failed"),
        envelope["error"]["code"].as_str().unwrap_or("HTTP_ERROR")
    );
    let data = &envelope["data"];
    if data["status"].as_str() == Some("approved") {
        println!(
            "published {} {} to the Island package catalog",
            release.id, release.version
        );
    } else {
        println!(
            "submitted {} {} for Island-port review (status: {})",
            release.id,
            release.version,
            data["status"].as_str().unwrap_or("unknown")
        );
    }
    Ok(())
}

/// Upload without replacing an existing GitHub asset, then submit metadata for review.
pub fn upload(paths: &Paths, manifest_path: &Path, asset_path: &Path) -> Result<()> {
    let release = read_manifest(manifest_path)?;
    let gh = std::env::var("ISOS_GH").unwrap_or_else(|_| "gh".into());
    github_upload(&release, manifest_path, asset_path, &gh)?;
    publish(paths, manifest_path)
}

fn github_upload(
    release: &Release,
    manifest_path: &Path,
    asset_path: &Path,
    gh: &str,
) -> Result<()> {
    let asset_name = asset_path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| anyhow::anyhow!("asset filename must be UTF-8"))?;
    let selected = release
        .assets
        .iter()
        .find(|a| {
            a.name.as_deref() == Some(asset_name) || a.url.ends_with(&format!("/{asset_name}"))
        })
        .ok_or_else(|| anyhow::anyhow!("asset is not listed in manifest"))?;
    release::verify_file(selected, asset_path)?;
    let source = release
        .source_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("manifest source_url is required"))?;
    let repo = source
        .strip_prefix("https://github.com/")
        .unwrap_or("")
        .trim_end_matches(".git")
        .trim_end_matches('/');
    ensure!(
        repo.split('/').count() == 2 && repo.split('/').all(|s| !s.is_empty()),
        "source_url must identify one GitHub repository"
    );
    let release_url = release
        .release_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("manifest release_url is required"))?;
    let tag = release_url
        .rsplit('/')
        .next()
        .ok_or_else(|| anyhow::anyhow!("release_url has no tag"))?;
    ensure!(
        release_url == format!("https://github.com/{repo}/releases/tag/{tag}"),
        "release_url must match source repository"
    );
    let output = Command::new(gh)
        .args(["api", &format!("repos/{repo}/git/ref/tags/{tag}")])
        .output()?;
    ensure!(
        output.status.success(),
        "Git tag {tag} does not exist in {repo}"
    );
    let reference: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let mut ref_sha = reference["object"]["sha"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Git tag has no target SHA"))?
        .to_owned();
    if reference["object"]["type"] == "tag" {
        let peeled = Command::new(gh)
            .args(["api", &format!("repos/{repo}/git/tags/{ref_sha}")])
            .output()?;
        ensure!(
            peeled.status.success(),
            "cannot resolve annotated tag {tag}"
        );
        let value: serde_json::Value = serde_json::from_slice(&peeled.stdout)?;
        ensure!(
            value["object"]["type"] == "commit",
            "tag {tag} does not resolve to a commit"
        );
        ref_sha = value["object"]["sha"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("annotated tag has no commit SHA"))?
            .to_owned();
    }
    ensure!(
        ref_sha == release.source_commit,
        "tag {tag} does not point to source_commit {}",
        release.source_commit
    );
    let release_view = Command::new(gh)
        .args([
            "release",
            "view",
            tag,
            "--repo",
            repo,
            "--json",
            "isDraft,assets",
        ])
        .output()?;
    if !release_view.status.success() {
        let stderr = String::from_utf8_lossy(&release_view.stderr).to_ascii_lowercase();
        ensure!(
            stderr.contains("release not found") || stderr.contains("http 404"),
            "cannot inspect GitHub Release: {stderr}"
        );
        let create = Command::new(gh)
            .args([
                "release",
                "create",
                tag,
                "--repo",
                repo,
                "--draft",
                "--verify-tag",
                "--title",
                &format!("{} {}", release.id, release.version),
            ])
            .status()?;
        ensure!(create.success(), "cannot create draft GitHub Release");
    } else {
        let view: serde_json::Value = serde_json::from_slice(&release_view.stdout)?;
        ensure!(
            view["isDraft"] == true,
            "existing GitHub Release is published; upload requires a draft"
        );
        let names: Vec<&str> = view["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["name"].as_str())
            .collect();
        ensure!(
            !names.contains(&asset_name),
            "GitHub Release already contains {asset_name}; refusing overwrite"
        );
        let manifest_name = manifest_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        ensure!(
            !names.contains(&manifest_name),
            "GitHub Release already contains {manifest_name}; refusing overwrite"
        );
    }
    let uploaded = Command::new(gh)
        .args(["release", "upload", tag, "--repo", repo])
        .arg(asset_path)
        .arg(manifest_path)
        .status()?;
    ensure!(uploaded.success(), "GitHub Release upload failed");
    println!(
        "uploaded {} and {} to draft release {release_url}",
        asset_name,
        manifest_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod upload_tests {
    use super::*;
    use sha2::Digest;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn github_upload_checks_tag_and_never_overwrites_assets() {
        let root = std::env::temp_dir().join(format!("isos-upload-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let asset = root.join("asset.tgz");
        fs::write(&asset, b"abc").unwrap();
        let hash = hex::encode(sha2::Sha256::digest(b"abc"));
        let manifest = root.join("release.json");
        let sha = "a".repeat(40);
        let release:Release=serde_json::from_value(serde_json::json!({
            "schema_version":4,"id":"island/test","kind":"skill","version":"1.0.0","source_commit":sha,
            "source_url":"https://github.com/Island-Team/test","release_url":"https://github.com/Island-Team/test/releases/tag/v1.0.0",
            "assets":[{"name":"asset.tgz","os":"linux","arch":"x86_64","url":"https://github.com/Island-Team/test/releases/download/v1.0.0/asset.tgz","size_bytes":3,"sha256":hash}]
        })).unwrap();
        fs::write(&manifest, serde_json::to_vec(&release).unwrap()).unwrap();
        let mock = root.join("gh");
        let script = format!(
            "#!/bin/sh\ncase \"$1 $2\" in\n  'api repos/Island-Team/test/git/ref/tags/v1.0.0') echo '{{\"object\":{{\"type\":\"commit\",\"sha\":\"{}\"}}}}';;\n  'release view') echo '{{\"isDraft\":true,\"assets\":[]}}';;\n  'release upload') exit 0;;\n  *) exit 2;;\nesac\n",
            release.source_commit
        );
        fs::write(&mock, &script).unwrap();
        fs::set_permissions(&mock, fs::Permissions::from_mode(0o700)).unwrap();
        github_upload(&release, &manifest, &asset, mock.to_str().unwrap()).unwrap();
        let blocked = script.replace("\"assets\":[]", "\"assets\":[{\"name\":\"asset.tgz\"}]");
        fs::write(&mock, blocked).unwrap();
        assert!(
            github_upload(&release, &manifest, &asset, mock.to_str().unwrap())
                .unwrap_err()
                .to_string()
                .contains("refusing overwrite")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod adapter_library_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn release(hash: &str) -> Release {
        serde_json::from_value(serde_json::json!({
            "schema_version": 4, "id": "vendor/devices", "kind": "backend", "version": "2.0.0",
            "source_commit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "adapters": [{"family": "vendor-base", "library": "lib/base.so", "sha256": hash}],
            "assets": [{"os": "linux", "arch": "x86_64", "url": "https://github.com/vendor/devices/releases/download/v2.0.0/devices.tar.gz", "size_bytes": 1, "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]
        })).unwrap()
    }

    #[test]
    fn link_verifies_adapter_library_before_writing_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(source.join("lib")).unwrap();
        fs::write(source.join("lib/base.so"), b"adapter-v1").unwrap();
        let hash = hex::encode(Sha256::digest(b"adapter-v1"));
        let release = release(&hash);
        fs::write(
            source.join("release.json"),
            serde_json::to_vec(&release).unwrap(),
        )
        .unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("runtime"),
        };
        let target = link_with_manifest(&paths, "vendor/devices", "2.0.0", &source, None).unwrap();
        assert!(target.join("receipt.json").is_file());
        let (path, actual) =
            verified_adapter_library(&paths, "vendor/devices", "2.0.0", "vendor-base").unwrap();
        assert_eq!(path, source.join("lib/base.so"));
        assert_eq!(actual, hash);
        fs::write(source.join("lib/base.so"), b"changed").unwrap();
        assert!(
            verified_adapter_library(&paths, "vendor/devices", "2.0.0", "vendor-base").is_err()
        );
    }

    #[test]
    fn link_rejects_missing_changed_and_escaped_libraries() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(source.join("lib")).unwrap();
        let hash = hex::encode(Sha256::digest(b"adapter-v1"));
        let release = release(&hash);
        fs::write(
            source.join("release.json"),
            serde_json::to_vec(&release).unwrap(),
        )
        .unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("runtime"),
        };
        assert!(link_with_manifest(&paths, "vendor/devices", "2.0.0", &source, None).is_err());
        assert!(
            !package_path(&paths, "vendor/devices", "2.0.0")
                .unwrap()
                .exists()
        );
        fs::write(source.join("lib/base.so"), b"wrong").unwrap();
        assert!(link_with_manifest(&paths, "vendor/devices", "2.0.0", &source, None).is_err());
        fs::remove_file(source.join("lib/base.so")).unwrap();
        fs::write(temp.path().join("outside.so"), b"adapter-v1").unwrap();
        symlink(temp.path().join("outside.so"), source.join("lib/base.so")).unwrap();
        assert!(link_with_manifest(&paths, "vendor/devices", "2.0.0", &source, None).is_err());
        assert!(
            !package_path(&paths, "vendor/devices", "2.0.0")
                .unwrap()
                .exists()
        );
    }
}

#[cfg(test)]
mod verification_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn fixture() -> (tempfile::TempDir, Paths, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        let source = temp.path().join("bundle");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("payload.bin"), b"original").unwrap();
        let platform = release::Platform::current();
        let release = serde_json::json!({"schema_version":4,"id":"test/verified","kind":"skill","version":"0.1.1",
            "source_commit":"a".repeat(40),"node_contributions":{"node_types":[],"safety_adapters":[],"adapter_families":[],"data_types":[],"node_sets":[]},
            "assets":[{"os":platform.os,"arch":platform.arch,"url":"https://github.com/test/verified/releases/download/v0.1.1/bundle.tar.gz","size_bytes":1,"sha256":"b".repeat(64)}]});
        fs::write(
            source.join("release.json"),
            serde_json::to_vec(&release).unwrap(),
        )
        .unwrap();
        (temp, paths, source)
    }

    #[test]
    fn verified_link_detects_payload_manifest_sidecar_and_symlink_mutation() {
        for mutation in ["payload", "manifest", "sidecar", "symlink"] {
            let (temp, paths, source) = fixture();
            let store =
                link_with_manifest(&paths, "test/verified", "0.1.1", &source, None).unwrap();
            let checked = verify(&paths, "test/verified", "0.1.1").unwrap();
            assert_eq!(checked.receipt.schema_version, 2);
            assert_eq!(checked.receipt.source_kind.as_deref(), Some("local_link"));
            match mutation {
                "payload" => fs::write(source.join("payload.bin"), b"changed").unwrap(),
                "manifest" => fs::write(store.join("release.json"), b"{}").unwrap(),
                "sidecar" => fs::write(store.join("node-types.json"), b"{}").unwrap(),
                _ => {
                    symlink(temp.path().join("outside"), source.join("escape")).unwrap();
                }
            }
            let error = verify(&paths, "test/verified", "0.1.1").unwrap_err();
            let failure = error.downcast_ref::<PackageFailure>().unwrap();
            assert_eq!(
                (failure.code, failure.stage),
                match mutation {
                    "payload" => ("package_changed", "payload"),
                    "manifest" => ("package_changed", "manifest"),
                    "sidecar" => ("package_invalid_sidecar", "sidecar"),
                    _ => ("package_unsafe_path", "payload"),
                }
            );
        }
    }

    #[test]
    fn link_rejects_symlinked_manifest_before_publication() {
        let (temp, paths, source) = fixture();
        fs::rename(
            source.join("release.json"),
            temp.path().join("real-release.json"),
        )
        .unwrap();
        symlink(
            temp.path().join("real-release.json"),
            source.join("release.json"),
        )
        .unwrap();
        let error =
            link_with_manifest(&paths, "test/verified", "0.1.1", &source, None).unwrap_err();
        let failure = error.downcast_ref::<PackageFailure>().unwrap();
        assert_eq!(
            (failure.code, failure.stage),
            ("package_unsafe_path", "manifest")
        );
        assert!(
            !package_path(&paths, "test/verified", "0.1.1")
                .unwrap()
                .exists()
        );
    }

    #[test]
    fn graph_pin_collection_rejects_incomplete_pin_and_only_verifies_referenced_release() {
        let (_temp, paths, source) = fixture();
        link_with_manifest(&paths, "test/verified", "0.1.1", &source, None).unwrap();
        let mut graph = serde_json::json!({"cores":{"eos":{"layers":{"process":{"groups":{"demo":{"nodes":{"one":{"package":{"id":"test/verified","version":"0.1.1"}}}}}}}}}});
        let packages = verify_graph_packages(&paths, &graph).unwrap();
        assert_eq!(packages.len(), 1);
        graph["cores"]["eos"]["layers"]["process"]["groups"]["demo"]["nodes"]["one"]["adapter"] =
            serde_json::json!({"package":{"id":"test/verified"}});
        assert!(
            collect_pins(&graph)
                .unwrap_err()
                .to_string()
                .contains("/adapter/package")
        );
    }

    #[test]
    fn tree_digest_is_order_independent_and_tracks_executable_bit() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("a")).unwrap();
        fs::write(root.join("z"), b"z").unwrap();
        fs::write(root.join("a/x"), b"x").unwrap();
        let first = payload_digest(root, false).unwrap();
        assert_eq!(first, payload_digest(root, false).unwrap());
        fs::set_permissions(root.join("a/x"), fs::Permissions::from_mode(0o700)).unwrap();
        assert_ne!(first, payload_digest(root, false).unwrap());
    }
}
