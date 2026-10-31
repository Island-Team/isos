//! Verified local package store. Only explicit install changes the store.

use crate::{
    auth,
    config::{self, Paths},
    lifecycle::Lock,
    release::{self, Release},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
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
    let path = root
        .join(library)
        .canonicalize()
        .with_context(|| format!("adapter library {library} is missing"))?;
    ensure!(
        path.starts_with(root) && path.is_file(),
        "adapter library escapes package or is not a file"
    );
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
    let installed = package_path(paths, id, version)?;
    let receipt: Receipt = serde_json::from_slice(&fs::read(installed.join("receipt.json"))?)?;
    ensure!(
        receipt.id == id && receipt.version == version,
        "adapter package identity mismatch"
    );
    let release = read_manifest(&installed.join("release.json"))?;
    ensure!(
        release.id == id && release.version == version,
        "adapter release identity mismatch"
    );
    let adapter = release
        .adapters
        .iter()
        .find(|adapter| adapter.family == family)
        .with_context(|| format!("package {id}@{version} has no {family} adapter library"))?;
    let root = receipt
        .local_link
        .as_deref()
        .unwrap_or(&installed)
        .canonicalize()?;
    verify_adapter_file(&root, &adapter.library, &adapter.sha256)
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
        verify_declared_adapter_libraries(&payload, release)?;
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
        write_node_types(&payload, release)?;
        fs::rename(&payload, &target)?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    result?;
    Ok(target)
}

#[cfg(test)]
pub fn link(paths: &Paths, id: &str, version: &str, source: &Path) -> Result<PathBuf> {
    link_with_manifest(paths, id, version, source, None)
}

fn write_node_types(target: &Path, release: &Release) -> Result<()> {
    if let Some(contributions) = &release.node_contributions {
        config::atomic_write(
            &target.join("node-types.json"),
            &serde_json::to_vec_pretty(&serde_json::json!({
                "package":{"id":release.id,"version":release.version},
                "node_types":contributions.node_types,
                "safety_adapters":contributions.safety_adapters,
                "adapter_families":contributions.adapter_families,
                "data_types":contributions.data_types,
            }))?,
        )?;
    }
    Ok(())
}

pub fn link_with_manifest(
    paths: &Paths,
    id: &str,
    version: &str,
    source: &Path,
    manifest: Option<&Path>,
) -> Result<PathBuf> {
    let _lock = Lock::take(paths)?;
    let discovered = source.join("release.json");
    let manifest = manifest.or_else(|| discovered.is_file().then_some(discovered.as_path()));
    let release = manifest.map(Release::read_from_path).transpose()?;
    if let Some(release) = &release {
        ensure!(
            release.id == id && release.version == version,
            "linked manifest identity differs"
        );
    }
    let source = source
        .canonicalize()
        .with_context(|| format!("source {} is missing", source.display()))?;
    ensure!(source.is_dir(), "linked source must be a directory");
    if let Some(release) = &release {
        verify_declared_adapter_libraries(&source, release)?;
    }
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
    if let Some(release) = &release {
        config::atomic_write(
            &target.join("release.json"),
            &serde_json::to_vec_pretty(release)?,
        )?;
        write_node_types(&target, release)?;
    }
    Ok(target)
}

pub fn node_type_sidecars(paths: &Paths) -> Result<Vec<PathBuf>> {
    let mut sidecars = Vec::new();
    for receipt in list(paths)? {
        let file = package_path(paths, &receipt.id, &receipt.version)?.join("node-types.json");
        if file.is_file() {
            sidecars.push(file);
        }
    }
    Ok(sidecars)
}

pub fn node_type_sidecars_for_graph(
    paths: &Paths,
    graph: &serde_json::Value,
) -> Result<Vec<PathBuf>> {
    let mut pins = std::collections::BTreeSet::new();
    if let Some(cores) = graph.get("cores").and_then(serde_json::Value::as_object) {
        for core in cores.values() {
            if let Some(layers) = core["layers"].as_object() {
                for layer in layers.values() {
                    if let Some(groups) = layer["groups"].as_object() {
                        for group in groups.values() {
                            if let Some(nodes) = group["nodes"].as_object() {
                                for node in nodes.values() {
                                    pins.extend(node_package_pins(node));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let mut sidecars = Vec::new();
    for (id, version) in pins {
        let path = package_path(paths, &id, &version)?.join("node-types.json");
        if path.is_file() {
            sidecars.push(path);
        }
    }
    Ok(sidecars)
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
                    "package is referenced by an applied v2 graph; remove that reference first"
                );
            }
        }
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
    if UnixStream::connect(paths.socket()).is_ok() {
        let active = crate::manager::v2_call(paths, "default", "v2_active_packages")?;
        ensure!(
            !active["packages"]
                .as_array()
                .is_some_and(|packages| packages
                    .iter()
                    .any(|package| package["id"] == id && package["version"] == version)),
            "package is referenced by an active v2 deployment; stop it before removal"
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
