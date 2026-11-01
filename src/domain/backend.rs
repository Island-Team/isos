//! Resolve an installed Aether backend bundle without executing package code.

use crate::{config::Paths, mujoco_catalog, packages};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BackendDescriptor {
    pub schema_version: String,
    #[serde(default)]
    pub catalog_version: Option<String>,
    pub package: PackageIdentity,
    pub entrypoint: String,
    pub protocol: String,
    pub scenes: Vec<Scene>,
    pub devices: BTreeMap<String, String>,
    #[serde(default)]
    pub device_catalog: Option<serde_json::Value>,
    pub runtime: RuntimeRequirement,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIdentity {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceEntry {
    pub kind: String,
    pub id: String,
    pub label: String,
    pub description: String,
    pub scene_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRequirement {
    pub mujoco_version: String,
    pub environment_variable: String,
    pub library: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedBackend {
    pub descriptor: BackendDescriptor,
    pub executable: PathBuf,
    pub scene: PathBuf,
    pub environment: BTreeMap<String, String>,
    pub fingerprint: String,
}

fn scene_asset_hash(root: &Path, scene: &Path, digest: &mut Sha256) -> Result<()> {
    let source = fs::read_to_string(scene).context("MuJoCo scene is not UTF-8 XML")?;
    ensure!(
        source.len() <= 4 * 1024 * 1024,
        "MuJoCo scene exceeds 4 MiB"
    );
    digest.update(source.as_bytes());
    for (index, (start, _)) in source.match_indices("file=").enumerate() {
        ensure!(index < 512, "MuJoCo scene references too many assets");
        let attribute = &source[start + 5..];
        let quote = attribute
            .chars()
            .next()
            .context("empty scene file attribute")?;
        ensure!(
            quote == '"' || quote == '\'',
            "scene file attribute must be quoted"
        );
        let reference = attribute[1..]
            .split(quote)
            .next()
            .context("invalid scene asset reference")?;
        let path = scene
            .parent()
            .context("scene has no parent")?
            .join(reference)
            .canonicalize()
            .with_context(|| format!("missing scene asset {reference}"))?;
        ensure!(
            path.starts_with(root) && path.is_file(),
            "scene asset escapes bundle: {reference}"
        );
        ensure!(
            fs::metadata(&path)?.len() <= 64 * 1024 * 1024,
            "scene asset is too large: {reference}"
        );
        digest.update(fs::read(path)?);
    }
    Ok(())
}

fn bundle_asset_hash(root: &Path, digest: &mut Sha256) -> Result<()> {
    const MAX_FILES: usize = 4096;
    const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
    let assets = root.join("assets");
    let mut pending = vec![assets];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        if !directory.exists() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            ensure!(
                !kind.is_symlink(),
                "bundle assets must not contain symlinks"
            );
            if kind.is_dir() {
                pending.push(entry.path());
            } else {
                ensure!(kind.is_file(), "bundle assets must be regular files");
                files.push(entry.path());
                ensure!(files.len() <= MAX_FILES, "bundle has too many asset files");
            }
        }
    }
    files.sort();
    let mut total = 0_u64;
    for path in files {
        let relative = path.strip_prefix(root)?;
        let size = fs::metadata(&path)?.len();
        ensure!(
            size <= 64 * 1024 * 1024,
            "bundle asset is too large: {}",
            relative.display()
        );
        total = total
            .checked_add(size)
            .context("bundle asset size overflow")?;
        ensure!(total <= MAX_TOTAL_BYTES, "bundle assets exceed 512 MiB");
        digest.update(relative.as_os_str().as_encoded_bytes());
        digest.update(size.to_le_bytes());
        digest.update(fs::read(&path)?);
    }
    Ok(())
}

fn relative_file(root: &Path, relative: &str, executable: bool) -> Result<PathBuf> {
    packages::checked_file(root, relative, executable)
}

pub fn resolve(
    paths: &Paths,
    id: &str,
    version: &str,
    scene_id: &str,
    env: &BTreeMap<String, String>,
) -> Result<ResolvedBackend> {
    let verified = packages::verify(paths, id, version)?;
    let root = verified.root;
    let descriptor_bytes =
        fs::read(root.join("backend-v2.json")).context("backend-v2.json is missing")?;
    ensure!(
        descriptor_bytes.len() <= 64 * 1024,
        "backend descriptor exceeds 64 KiB"
    );
    let descriptor: BackendDescriptor = serde_json::from_slice(&descriptor_bytes)?;
    mujoco_catalog::inspect(&root, id, version)
        .map_err(|issue| issue.into_error(id, version, ""))?;
    ensure!(
        descriptor.schema_version == "2.0.0"
            && descriptor.package.id == id
            && descriptor.package.version == version,
        "backend descriptor identity or version mismatch"
    );
    ensure!(
        descriptor.protocol == "mujoco-v2",
        "unsupported backend protocol {}",
        descriptor.protocol
    );
    ensure!(
        !descriptor.scenes.is_empty() && descriptor.scenes.len() <= 32,
        "backend needs 1..32 scenes"
    );
    let mut scene_ids = std::collections::BTreeSet::new();
    for scene in &descriptor.scenes {
        ensure!(scene_ids.insert(&scene.id), "duplicate scene {}", scene.id);
        relative_file(&root, &scene.path, false)?;
    }
    for kind in ["camera", "base", "manipulator"] {
        ensure!(
            descriptor
                .devices
                .get(kind)
                .is_some_and(|id| !id.is_empty()),
            "backend lacks {kind} device"
        );
    }
    let executable = relative_file(&root, &descriptor.entrypoint, true)?;
    let scene = descriptor
        .scenes
        .iter()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| anyhow::anyhow!("unknown backend scene {scene_id}"))?;
    let scene = relative_file(&root, &scene.path, false)?;
    ensure!(
        descriptor.runtime.environment_variable == "MUJOCO_ROOT",
        "unsupported runtime environment key"
    );
    let mujoco_root = env
        .get("MUJOCO_ROOT")
        .cloned()
        .or_else(|| std::env::var("MUJOCO_ROOT").ok())
        .context("MUJOCO_ROOT is required for MuJoCo backend")?;
    let library_root = PathBuf::from(&mujoco_root)
        .canonicalize()
        .context("MUJOCO_ROOT is missing")?;
    let library = relative_file(&library_root, &descriptor.runtime.library, false)?;
    let mut environment = env.clone();
    environment.insert(
        "MUJOCO_ROOT".into(),
        library_root.to_string_lossy().into_owned(),
    );
    let mut loader_paths = vec![
        library
            .parent()
            .context("MuJoCo library has no parent directory")?
            .to_path_buf(),
    ];
    if let Some(existing) = env
        .get("LD_LIBRARY_PATH")
        .cloned()
        .or_else(|| std::env::var("LD_LIBRARY_PATH").ok())
    {
        loader_paths.extend(std::env::split_paths(&existing));
    }
    environment.insert(
        "LD_LIBRARY_PATH".into(),
        std::env::join_paths(loader_paths)?
            .to_string_lossy()
            .into_owned(),
    );
    let mut digest = Sha256::new();
    digest.update(&descriptor_bytes);
    digest.update(fs::read(&executable)?);
    scene_asset_hash(&root, &scene, &mut digest)?;
    bundle_asset_hash(&root, &mut digest)?;
    Ok(ResolvedBackend {
        descriptor,
        executable,
        scene,
        environment,
        fingerprint: hex::encode(digest.finalize()),
    })
}

pub fn installed(paths: &Paths) -> Vec<serde_json::Value> {
    let mut rows = Vec::new();
    if let Ok(receipts) = packages::list(paths) {
        for receipt in receipts
            .into_iter()
            .filter(|r| r.id == "island/mujoco-sim" && r.version == "0.1.1")
        {
            let verified = packages::verify(paths, &receipt.id, &receipt.version);
            let catalog_result = verified.as_ref().ok().map(mujoco_catalog::inspect_verified);
            let catalog = catalog_result
                .as_ref()
                .and_then(|result| result.as_ref().ok());
            let scene_catalog: Vec<_> = catalog.as_ref().filter(|catalog| catalog.mode == mujoco_catalog::Mode::Target)
                .map(|catalog| catalog.descriptor.scenes.iter().map(|scene| serde_json::json!({
                    "id":scene.id,"label":scene.label,"description":scene.description,"profile":scene.profile
                })).collect()).unwrap_or_default();
            let device_catalog = catalog
                .as_ref()
                .and_then(|catalog| mujoco_catalog::device_entries(catalog))
                .map(|entries| serde_json::to_value(entries).unwrap_or_default())
                .unwrap_or_default();
            let scenes: Vec<_> = catalog
                .map(|catalog| {
                    catalog
                        .descriptor
                        .scenes
                        .iter()
                        .map(|scene| scene.id.clone())
                        .collect()
                })
                .unwrap_or_default();
            let devices: Vec<_> = catalog
                .map(|catalog| catalog.descriptor.devices.values().cloned().collect())
                .unwrap_or_default();
            let issues: Vec<String> = if let Err(error) = &verified {
                vec![error.to_string()]
            } else if let Some(Err(issue)) = &catalog_result {
                vec![issue.code.to_owned()]
            } else if let Some(catalog) = catalog {
                mujoco_catalog::check_library(catalog, None)
                    .err()
                    .map(|issue| vec![issue.code.to_owned()])
                    .unwrap_or_default()
            } else {
                vec!["mujoco_catalog_incomplete".into()]
            };
            let linked = verified
                .as_ref()
                .map_or(receipt.local_link.is_some(), |package| {
                    package.receipt.local_link.is_some()
                });
            rows.push(
                serde_json::json!({"id":receipt.id,"version":receipt.version,"linked":linked,
                "scenes":scenes,"devices":devices,"scene_catalog":scene_catalog,
                "device_catalog":device_catalog,"ready":issues.is_empty(),"issues":issues}),
            );
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn linked_bundle_is_revalidated_and_scene_is_pinned() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        let bundle = temp.path().join("bundle");
        fs::create_dir_all(bundle.join("bin")).unwrap();
        fs::create_dir_all(bundle.join("assets")).unwrap();
        fs::write(bundle.join("bin/sim"), "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(bundle.join("bin/sim"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(bundle.join("assets/scene.xml"), "<mesh file=\"mesh.stl\"/>").unwrap();
        fs::write(bundle.join("assets/mesh.stl"), "mesh-a").unwrap();
        let descriptor = serde_json::json!({"schema_version":"2.0.0","package":{"id":"island/mujoco-sim","version":"0.1.1"},"entrypoint":"bin/sim","protocol":"mujoco-v2","scenes":[{"id":"default","path":"assets/scene.xml"}],"devices":{"camera":"cam-0","base":"base-0","manipulator":"arm-0"},"runtime":{"mujoco_version":"3.16.0","environment_variable":"MUJOCO_ROOT","library":"lib/libmujoco.so.3.16.0"}});
        fs::write(
            bundle.join("backend-v2.json"),
            serde_json::to_vec(&descriptor).unwrap(),
        )
        .unwrap();
        let mj = temp.path().join("mujoco");
        fs::create_dir_all(mj.join("lib")).unwrap();
        fs::write(mj.join("lib/libmujoco.so.3.16.0"), "stub").unwrap();
        let platform = crate::release::Platform::current();
        let linked_release = serde_json::json!({"schema_version":4,"id":"island/mujoco-sim","kind":"backend","version":"0.1.1",
            "source_commit":"0123456789012345678901234567890123456789",
            "assets":[{"os":platform.os,"arch":platform.arch,"url":"https://github.com/Island-Team/isos-mujoco/releases/download/v0.1.1/bundle.tar.gz","size_bytes":1,"sha256":"a".repeat(64)}]});
        fs::write(
            bundle.join("release.json"),
            serde_json::to_vec(&linked_release).unwrap(),
        )
        .unwrap();
        packages::link(&paths, "island/mujoco-sim", "0.1.1", &bundle).unwrap();
        let env = BTreeMap::from([("MUJOCO_ROOT".into(), mj.to_string_lossy().into_owned())]);
        let first = resolve(&paths, "island/mujoco-sim", "0.1.1", "default", &env).unwrap();
        assert_eq!(first.descriptor.devices["camera"], "cam-0");
        assert!(first.environment["LD_LIBRARY_PATH"].starts_with(mj.join("lib").to_str().unwrap()));
        assert!(resolve(&paths, "island/mujoco-sim", "0.1.1", "unknown", &env).is_err());
        fs::write(bundle.join("assets/mesh.stl"), "mesh-b").unwrap();
        assert!(resolve(&paths, "island/mujoco-sim", "0.1.1", "default", &env).is_err());
        fs::create_dir(bundle.join("assets/nested")).unwrap();
        fs::write(
            bundle.join("assets/nested/child.xml"),
            "<mesh file=\"part.stl\"/>",
        )
        .unwrap();
        fs::write(bundle.join("assets/nested/part.stl"), "nested-a").unwrap();
        fs::write(
            bundle.join("assets/scene.xml"),
            "<include file=\"nested/child.xml\"/>",
        )
        .unwrap();
        assert!(resolve(&paths, "island/mujoco-sim", "0.1.1", "default", &env).is_err());
        fs::write(bundle.join("assets/nested/part.stl"), "nested-b").unwrap();
        assert!(resolve(&paths, "island/mujoco-sim", "0.1.1", "default", &env).is_err());
        fs::write(bundle.join("assets/scene.xml"), "<mesh file=\"mesh.stl\"/>").unwrap();
        fs::remove_file(bundle.join("assets/mesh.stl")).unwrap();
        assert!(resolve(&paths, "island/mujoco-sim", "0.1.1", "default", &env).is_err());
        fs::remove_file(bundle.join("bin/sim")).unwrap();
        assert!(resolve(&paths, "island/mujoco-sim", "0.1.1", "default", &env).is_err());
        let graph = serde_json::json!({"cores":{"eos":{"layers":{"process":{"groups":{"backend":{"nodes":{"simulator":{"settings":{"package":"island/mujoco-sim","version":"0.1.1"}}}}}}}}}});
        crate::config::atomic_write(
            &paths.node_graph_path("default"),
            &serde_json::to_vec(&graph).unwrap(),
        )
        .unwrap();
        assert!(packages::remove(&paths, "island/mujoco-sim", "0.1.1").is_err());
        fs::remove_file(paths.node_graph_path("default")).unwrap();
        assert!(packages::remove(&paths, "island/mujoco-sim", "0.1.1").is_ok());

        fs::write(bundle.join("bin/sim"), "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(bundle.join("bin/sim"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(bundle.join("assets/mesh.stl"), "mesh-c").unwrap();
        fs::remove_file(bundle.join("release.json")).unwrap();
        let archive = temp.path().join("bundle.tar.gz");
        let encoder = flate2::write::GzEncoder::new(
            fs::File::create(&archive).unwrap(),
            flate2::Compression::default(),
        );
        let mut tar = tar::Builder::new(encoder);
        tar.append_dir_all("", &bundle).unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        let bytes = fs::read(&archive).unwrap();
        let platform = crate::release::Platform::current();
        let release: crate::release::Release = serde_json::from_value(serde_json::json!({
            "schema_version":4,"id":"island/mujoco-sim","kind":"backend","version":"0.1.1",
            "source_commit":"0123456789012345678901234567890123456789",
            "assets":[{"os":platform.os,"arch":platform.arch,"url":"https://github.com/Island-Team/isos-mujoco/releases/download/v0.1.1/bundle.tar.gz","size_bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(&bytes))}]
        })).unwrap();
        let mut altered = bytes.clone();
        *altered.last_mut().unwrap() ^= 1;
        fs::write(&archive, &altered).unwrap();
        assert!(packages::install(&paths, &release, true, Some(&archive)).is_err());
        fs::write(&archive, &bytes).unwrap();
        packages::install(&paths, &release, true, Some(&archive)).unwrap();
        assert!(resolve(&paths, "island/mujoco-sim", "0.1.1", "default", &env).is_ok());
    }
}
