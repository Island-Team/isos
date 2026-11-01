//! Private, per-run credentials for EOS managed admission.

use crate::{config::Paths, process};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const GRANT_NAMES: [&str; 3] = [
    "egia-control.grant",
    "operator-control.grant",
    "verifier.json",
];
pub const RESERVED_ENVS: [&str; 3] = [
    "EOS_CONTROL_GRANTS_FILE",
    "EGIA_EOS_CONTROL_GRANT_FILE",
    "AETHER_BRIDGE_CONTROL_GRANT_FILE",
];

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub stage: &'static str,
    pub path: String,
    pub message: &'static str,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: repair private control-grant state before launching",
            self.code
        )
    }
}
impl std::error::Error for Diagnostic {}

fn failure(code: &'static str) -> anyhow::Error {
    Diagnostic {
        code,
        stage: "spawn",
        path: String::new(),
        message: "Repair private control-grant state and retry launch",
    }
    .into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Verifier {
    schema_version: u32,
    run_id: String,
    active_revision: String,
    egia_control_sha256: String,
    operator_control_sha256: String,
}

#[derive(Debug, Clone)]
pub struct Bundle {
    root: PathBuf,
    run_id: String,
    revision: String,
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn inspect_dir(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|_| failure("control_grant_unsafe_path"))?;
    ensure!(
        meta.file_type().is_dir()
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.permissions().mode() & 0o7777 == 0o700,
        failure("control_grant_unsafe_path")
    );
    Ok(())
}

fn no_symlink_ancestors(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), failure("control_grant_unsafe_path"));
    for ancestor in path.ancestors() {
        let meta =
            fs::symlink_metadata(ancestor).map_err(|_| failure("control_grant_unsafe_path"))?;
        ensure!(
            meta.file_type().is_dir(),
            failure("control_grant_unsafe_path")
        );
    }
    Ok(())
}

fn create_dir(parent: &Path, name: &str) -> Result<PathBuf> {
    inspect_dir(parent)?;
    let path = parent.join(name);
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .map_err(|_| failure("control_grant_unsafe_path"))?;
    inspect_dir(&path)?;
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| failure("control_grant_issue_failed"))?;
    Ok(path)
}

fn require_dir(parent: &Path, name: &str) -> Result<PathBuf> {
    inspect_dir(parent)?;
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(_) => inspect_dir(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return create_dir(parent, name);
        }
        Err(_) => return Err(failure("control_grant_unsafe_path")),
    }
    Ok(path)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| failure("control_grant_issue_failed"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| failure("control_grant_issue_failed"))?;
    Ok(())
}

fn read_file(path: &Path, max: u64) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path).map_err(|_| failure("control_grant_invalid_file"))?;
    ensure!(
        meta.file_type().is_file()
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.nlink() == 1
            && meta.permissions().mode() & 0o7777 == 0o600,
        failure("control_grant_unsafe_path")
    );
    ensure!(meta.len() <= max, failure("control_grant_invalid_file"));
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| failure("control_grant_unsafe_path"))?;
    let descriptor_meta = file
        .metadata()
        .map_err(|_| failure("control_grant_unsafe_path"))?;
    ensure!(
        descriptor_meta.ino() == meta.ino() && descriptor_meta.dev() == meta.dev(),
        failure("control_grant_unsafe_path")
    );
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| failure("control_grant_invalid_file"))?;
    ensure!(
        bytes.len() as u64 == meta.len(),
        failure("control_grant_invalid_file")
    );
    Ok(bytes)
}

fn random_grant() -> Result<[u8; 32]> {
    let mut value = [0u8; 32];
    let mut offset = 0;
    while offset < value.len() {
        let count = unsafe {
            libc::getrandom(value[offset..].as_mut_ptr().cast(), value.len() - offset, 0)
        };
        if count < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(failure("control_grant_issue_failed"));
        }
        ensure!(count > 0, failure("control_grant_issue_failed"));
        offset += count as usize;
    }
    Ok(value)
}

fn decoded_grant(bytes: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        bytes.len() == 65 && bytes[64] == b'\n',
        failure("control_grant_invalid_file")
    );
    let text =
        std::str::from_utf8(&bytes[..64]).map_err(|_| failure("control_grant_invalid_file"))?;
    ensure!(valid_hash(text), failure("control_grant_invalid_file"));
    hex::decode(text).map_err(|_| failure("control_grant_invalid_file"))
}

impl Bundle {
    pub fn issue(paths: &Paths, deployment: &str, run_id: &str, revision: &str) -> Result<Self> {
        Self::issue_with(paths, deployment, run_id, revision, random_grant)
    }

    fn issue_with(
        paths: &Paths,
        deployment: &str,
        run_id: &str,
        revision: &str,
        mut draw: impl FnMut() -> Result<[u8; 32]>,
    ) -> Result<Self> {
        process::validate_deployment_name(deployment)?;
        ensure!(
            run_id.len() <= 64
                && run_id.starts_with('r')
                && run_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            failure("control_grant_issue_failed")
        );
        ensure!(valid_hash(revision), failure("control_grant_mismatch"));
        no_symlink_ancestors(&paths.runtime)?;
        inspect_dir(&paths.runtime)?;
        let control_root = require_dir(&paths.runtime, "control-grants")?;
        let deployment_root = require_dir(&control_root, deployment)?;
        let root = create_dir(&deployment_root, run_id)?;
        let issue = (|| {
            let egia = draw()?;
            let operator = draw()?;
            ensure!(egia != operator, failure("control_grant_issue_failed"));
            write_new(
                &root.join(GRANT_NAMES[0]),
                format!("{}\n", hex::encode(egia)).as_bytes(),
            )?;
            write_new(
                &root.join(GRANT_NAMES[1]),
                format!("{}\n", hex::encode(operator)).as_bytes(),
            )?;
            let verifier = Verifier {
                schema_version: 1,
                run_id: run_id.to_owned(),
                active_revision: revision.to_owned(),
                egia_control_sha256: hex::encode(Sha256::digest(egia)),
                operator_control_sha256: hex::encode(Sha256::digest(operator)),
            };
            let mut bytes =
                serde_json::to_vec(&verifier).map_err(|_| failure("control_grant_issue_failed"))?;
            bytes.push(b'\n');
            write_new(&root.join(GRANT_NAMES[2]), &bytes)?;
            File::open(&root)
                .and_then(|f| f.sync_all())
                .map_err(|_| failure("control_grant_issue_failed"))?;
            Ok::<(), anyhow::Error>(())
        })();
        if let Err(error) = issue {
            for name in GRANT_NAMES {
                let _ = fs::remove_file(root.join(name));
            }
            let _ = fs::remove_dir(&root);
            return Err(error);
        }
        let bundle = Self {
            root,
            run_id: run_id.to_owned(),
            revision: revision.to_owned(),
        };
        if let Err(error) = bundle.verify() {
            for name in GRANT_NAMES {
                let _ = fs::remove_file(bundle.root.join(name));
            }
            let _ = fs::remove_dir(&bundle.root);
            return Err(error);
        }
        Ok(bundle)
    }

    pub fn verify(&self) -> Result<()> {
        let run_dir = &self.root;
        no_symlink_ancestors(run_dir)?;
        inspect_dir(run_dir)?;
        inspect_dir(
            run_dir
                .parent()
                .ok_or_else(|| failure("control_grant_unsafe_path"))?,
        )?;
        inspect_dir(
            run_dir
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| failure("control_grant_unsafe_path"))?,
        )?;
        inspect_dir(
            run_dir
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .ok_or_else(|| failure("control_grant_unsafe_path"))?,
        )?;
        let actual = fs::read_dir(run_dir)
            .map_err(|_| failure("control_grant_unsafe_path"))?
            .map(|entry| {
                entry
                    .map(|item| item.file_name())
                    .map_err(|_| failure("control_grant_unsafe_path"))
            })
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        let expected = GRANT_NAMES
            .iter()
            .map(std::ffi::OsStr::new)
            .map(std::ffi::OsStr::to_os_string)
            .collect();
        ensure!(actual == expected, failure("control_grant_unsafe_path"));
        let egia = decoded_grant(&read_file(&run_dir.join(GRANT_NAMES[0]), 65)?)?;
        let operator = decoded_grant(&read_file(&run_dir.join(GRANT_NAMES[1]), 65)?)?;
        let bytes = read_file(&run_dir.join(GRANT_NAMES[2]), 1024)?;
        let verifier: Verifier =
            serde_json::from_slice(&bytes).map_err(|_| failure("control_grant_invalid_file"))?;
        ensure!(
            verifier.schema_version == 1
                && verifier.run_id == self.run_id
                && verifier.active_revision == self.revision
                && verifier.egia_control_sha256 == hex::encode(Sha256::digest(egia))
                && verifier.operator_control_sha256 == hex::encode(Sha256::digest(operator)),
            failure("control_grant_mismatch")
        );
        Ok(())
    }

    pub fn launch_env(&self, id: &str) -> Option<(&'static str, PathBuf)> {
        match id {
            "eos" => Some((RESERVED_ENVS[0], self.root.join(GRANT_NAMES[2]))),
            "egia" => Some((RESERVED_ENVS[1], self.root.join(GRANT_NAMES[0]))),
            "bridge" => Some((RESERVED_ENVS[2], self.root.join(GRANT_NAMES[1]))),
            _ => None,
        }
    }

    pub fn retire(&self) -> Result<()> {
        self.verify()?;
        for name in GRANT_NAMES {
            fs::remove_file(self.root.join(name))
                .map_err(|_| failure("control_grant_unsafe_path"))?;
        }
        fs::remove_dir(&self.root).map_err(|_| failure("control_grant_unsafe_path"))?;
        File::open(
            self.root
                .parent()
                .ok_or_else(|| failure("control_grant_unsafe_path"))?,
        )?
        .sync_all()
        .map_err(|_| failure("control_grant_unsafe_path"))?;
        Ok(())
    }
}

pub fn validate_plan(specs: &[process::ProcessSpec]) -> Result<()> {
    for (id, role) in [("eos", "eos"), ("config", "eos")] {
        ensure!(
            specs
                .iter()
                .any(|spec| spec.id == id && spec.settings.role.as_deref() == Some(role)),
            failure("control_grant_delivery_failed")
        );
    }
    for spec in specs {
        if matches!(spec.id.as_str(), "egia" | "bridge") {
            let role = if spec.id == "egia" { "egia" } else { "eos" };
            ensure!(
                spec.settings.role.as_deref() == Some(role),
                failure("control_grant_delivery_failed")
            );
        }
        ensure!(
            spec.settings
                .env_from
                .iter()
                .all(|(target, source)| !RESERVED_ENVS.contains(&target.as_str())
                    && !RESERVED_ENVS.contains(&source.as_str())),
            failure("control_grant_delivery_failed")
        );
        if matches!(spec.id.as_str(), "eos" | "bridge" | "config") {
            for (index, arg) in spec.settings.args.iter().enumerate() {
                if [
                    "--run-id",
                    "--backend-release-sha256",
                    "--backend-payload-sha256",
                    "--log-dir",
                ]
                .iter()
                .any(|flag| arg == flag || arg.starts_with(&format!("{flag}=")))
                {
                    return Err(Diagnostic { code: "control_grant_reserved_arg", stage: "graph",
                        path: format!("/cores/isos/layers/process/groups/component/nodes/{}/settings/args/{index}", spec.id),
                        message: "Remove the owner-reserved launch argument and retry" }.into());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::symlink;

    fn paths() -> (tempfile::TempDir, Paths) {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        paths.prepare().unwrap();
        (temp, paths)
    }

    fn revision() -> String {
        "a".repeat(64)
    }

    fn spec(id: &str, role: &str, args: Vec<&str>) -> process::ProcessSpec {
        process::ProcessSpec {
            id: id.into(),
            path: format!("isos/process/component/{id}"),
            settings: serde_json::from_value(json!({
                "command": ["/bin/true"], "role": role, "args": args
            }))
            .unwrap(),
            package: None,
            fingerprint: None,
        }
    }

    fn two_specs() -> Vec<process::ProcessSpec> {
        vec![spec("eos", "eos", vec![]), spec("config", "eos", vec![])]
    }

    fn code(error: &anyhow::Error) -> &'static str {
        error.downcast_ref::<Diagnostic>().unwrap().code
    }

    #[test]
    fn issues_exact_private_files_and_distinct_grants_without_generated_state() {
        let (_temp, paths) = paths();
        let bundle = Bundle::issue_with(&paths, "test", "r-test-1", &revision(), {
            let mut values = [[0x11; 32], [0x22; 32]].into_iter();
            move || Ok(values.next().unwrap())
        })
        .unwrap();
        bundle.verify().unwrap();
        assert_eq!(
            fs::symlink_metadata(&bundle.root)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        for name in GRANT_NAMES {
            assert_eq!(
                fs::symlink_metadata(bundle.root.join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let egia = fs::read(bundle.root.join(GRANT_NAMES[0])).unwrap();
        let operator = fs::read(bundle.root.join(GRANT_NAMES[1])).unwrap();
        assert_eq!(egia.len(), 65);
        assert_eq!(operator.len(), 65);
        assert_ne!(egia, operator);
        let verifier: Verifier =
            serde_json::from_slice(&fs::read(bundle.root.join(GRANT_NAMES[2])).unwrap()).unwrap();
        assert_eq!(verifier.run_id, "r-test-1");
        assert_eq!(verifier.active_revision, revision());
        assert_eq!(
            verifier.egia_control_sha256,
            hex::encode(Sha256::digest([0x11; 32]))
        );
        assert_eq!(
            verifier.operator_control_sha256,
            hex::encode(Sha256::digest([0x22; 32]))
        );
        assert!(!paths.generated("test").exists());
        assert_eq!(bundle.launch_env("config"), None);
        assert_eq!(bundle.launch_env("other"), None);
        bundle.retire().unwrap();
        assert!(!bundle.root.exists());
    }

    #[test]
    fn rotation_rejects_old_grant_and_duplicate_run_directory() {
        let (_temp, paths) = paths();
        let first = Bundle::issue(&paths, "test", "r-test-1", &revision()).unwrap();
        assert_eq!(
            code(&Bundle::issue(&paths, "test", "r-test-1", &revision()).unwrap_err()),
            "control_grant_unsafe_path"
        );
        let old = fs::read(first.root.join(GRANT_NAMES[0])).unwrap();
        let second = Bundle::issue(&paths, "test", "r-test-2", &revision()).unwrap();
        let new = fs::read(second.root.join(GRANT_NAMES[0])).unwrap();
        assert_ne!(old, new);
        first.retire().unwrap();
        second.retire().unwrap();
    }

    #[test]
    fn rejects_tamper_shape_mode_symlink_and_verifier_mismatch() {
        let (_temp, paths) = paths();
        let bundle = Bundle::issue(&paths, "test", "r-test-1", &revision()).unwrap();
        let grant = bundle.root.join(GRANT_NAMES[0]);
        let original = fs::read(&grant).unwrap();
        fs::write(&grant, &original[..64]).unwrap();
        assert_eq!(
            code(&bundle.verify().unwrap_err()),
            "control_grant_invalid_file"
        );
        fs::write(&grant, &original).unwrap();
        fs::set_permissions(&grant, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            code(&bundle.verify().unwrap_err()),
            "control_grant_unsafe_path"
        );
        fs::set_permissions(&grant, fs::Permissions::from_mode(0o600)).unwrap();
        let verifier = bundle.root.join(GRANT_NAMES[2]);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&verifier).unwrap()).unwrap();
        value["active_revision"] = json!("b".repeat(64));
        fs::write(&verifier, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            code(&bundle.verify().unwrap_err()),
            "control_grant_mismatch"
        );
        fs::remove_file(&verifier).unwrap();
        symlink(&grant, &verifier).unwrap();
        assert_eq!(
            code(&bundle.verify().unwrap_err()),
            "control_grant_unsafe_path"
        );
    }

    #[test]
    fn injected_random_failure_does_not_leave_a_run_directory() {
        let (_temp, paths) = paths();
        let error = Bundle::issue_with(&paths, "test", "r-fail", &revision(), || {
            Err(failure("control_grant_issue_failed"))
        })
        .unwrap_err();
        assert_eq!(code(&error), "control_grant_issue_failed");
        assert!(!paths.runtime.join("control-grants/test/r-fail").exists());
    }

    #[test]
    fn plan_requires_eos_and_config_and_rejects_reserved_author_args() {
        let mut specs = two_specs();
        validate_plan(&specs).unwrap();
        validate_plan(&[specs.clone(), vec![spec("egia", "egia", vec![])]].concat()).unwrap();
        specs.remove(0);
        assert_eq!(
            code(&validate_plan(&specs).unwrap_err()),
            "control_grant_delivery_failed"
        );
        let mut specs = two_specs();
        specs[0].settings.args = vec![
            "--listen".into(),
            "127.0.0.1:16020".into(),
            "--run-id=spoof".into(),
        ];
        let error = validate_plan(&specs).unwrap_err();
        let diagnostic = error.downcast_ref::<Diagnostic>().unwrap();
        assert_eq!(diagnostic.code, "control_grant_reserved_arg");
        assert_eq!(diagnostic.stage, "graph");
        assert_eq!(
            diagnostic.path,
            "/cores/isos/layers/process/groups/component/nodes/eos/settings/args/2"
        );
        specs[0].settings.args = vec!["--run-id".into(), "spoof".into()];
        assert_eq!(
            validate_plan(&specs)
                .unwrap_err()
                .downcast_ref::<Diagnostic>()
                .unwrap()
                .path,
            "/cores/isos/layers/process/groups/component/nodes/eos/settings/args/0"
        );
        specs[0].settings.args.clear();
        specs[0]
            .settings
            .env_from
            .insert("EOS_CONTROL_GRANTS_FILE".into(), "MALICIOUS".into());
        assert_eq!(
            code(&validate_plan(&specs).unwrap_err()),
            "control_grant_delivery_failed"
        );
    }
}
