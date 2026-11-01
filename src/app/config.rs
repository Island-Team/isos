use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub runtime: PathBuf,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeConfig {
    #[serde(default)]
    server: ServerConfig,
    #[serde(default)]
    paths: RuntimePaths,
    #[serde(default)]
    deployment: DeploymentConfig,
    #[serde(default)]
    logging: LoggingConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerConfig {
    http_port: Option<u16>,
    ui_port: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimePaths {
    ui_repo: Option<PathBuf>,
    ui_dist: Option<PathBuf>,
    aether_bin_dir: Option<PathBuf>,
    egia_bin_dir: Option<PathBuf>,
    egia_defaults_dir: Option<PathBuf>,
    egia_factory_defaults_dir: Option<PathBuf>,
    mujoco_root: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeploymentConfig {
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoggingConfig {
    default_source: Option<String>,
    max_bytes: Option<u64>,
    backups: Option<u8>,
    max_sources: Option<u16>,
    level: Option<String>,
}

fn config_values(source: &str, repo_root: &Path) -> Result<BTreeMap<&'static str, String>> {
    let config: RuntimeConfig = toml::from_str(source)?;
    let mut values = BTreeMap::new();
    for (port, key) in [
        (config.server.http_port, "ISOS_HTTP_PORT"),
        (config.server.ui_port, "ISOSUI_PORT"),
    ] {
        if let Some(port) = port {
            ensure!(port >= 1024, "{key} must be 1024..65535");
            values.insert(key, port.to_string());
        }
    }
    for (path, key) in [
        (config.paths.ui_repo, "ISOSUI_REPO_PATH"),
        (config.paths.ui_dist, "ISOSUI_DIST_DIR"),
        (config.paths.aether_bin_dir, "ISOS_AETHER_BIN_DIR"),
        (config.paths.egia_bin_dir, "ISOS_EGIA_BIN_DIR"),
        (config.paths.egia_defaults_dir, "ISOS_EGIA_DEFAULTS_DIR"),
        (
            config.paths.egia_factory_defaults_dir,
            "ISOS_EGIA_FACTORY_DEFAULTS_DIR",
        ),
        (config.paths.mujoco_root, "MUJOCO_ROOT"),
    ] {
        if let Some(path) = path {
            let path = if path.is_absolute() {
                path
            } else {
                repo_root.join(path)
            };
            values.insert(key, path.to_string_lossy().into_owned());
        }
    }
    if let Some(name) = config.deployment.name {
        crate::process::validate_deployment_name(&name)?;
        values.insert("ISOS_DEPLOYMENT", name);
    }
    if let Some(source) = config.logging.default_source {
        ensure!(
            source == "all" || crate::log_store::valid_source(&source),
            "logging.default_source must be all or a valid log source"
        );
        values.insert("ISOS_LOG_DEFAULT_SOURCE", source);
    }
    if let Some(max_bytes) = config.logging.max_bytes {
        ensure!(
            (65_536..=100_000_000).contains(&max_bytes),
            "logging.max_bytes must be 65536..100000000"
        );
        values.insert("ISOS_LOG_MAX_BYTES", max_bytes.to_string());
    }
    if let Some(backups) = config.logging.backups {
        ensure!((1..=10).contains(&backups), "logging.backups must be 1..10");
        values.insert("ISOS_LOG_BACKUPS", backups.to_string());
    }
    if let Some(max_sources) = config.logging.max_sources {
        ensure!(
            (10..=256).contains(&max_sources),
            "logging.max_sources must be 10..256"
        );
        values.insert("ISOS_LOG_MAX_SOURCES", max_sources.to_string());
    }
    if let Some(level) = config.logging.level {
        ensure!(
            crate::log_store::level_rank(&level).is_some(),
            "logging.level must be trace, debug, info, warn, or error"
        );
        values.insert("ISOS_LOG_LEVEL", level);
    }
    Ok(values)
}

fn config_without_env_overrides(
    values: BTreeMap<&'static str, String>,
    has_env: impl Fn(&str) -> bool,
) -> BTreeMap<&'static str, String> {
    values
        .into_iter()
        .filter(|(key, _)| !has_env(key))
        .collect()
}

fn runtime_config_path(
    repo_root: &Path,
    override_path: Option<&std::ffi::OsStr>,
) -> Result<PathBuf> {
    if let Some(path) = override_path {
        let path = PathBuf::from(path);
        ensure!(path.is_absolute(), "ISOS_CONFIG must be an absolute path");
        Ok(path)
    } else {
        Ok(repo_root.join("config/isos.toml"))
    }
}

pub fn load_runtime_config() -> Result<()> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let override_path = std::env::var_os("ISOS_CONFIG");
    let path = runtime_config_path(repo_root, override_path.as_deref())?;
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && override_path.is_none() => {
            return Ok(());
        }
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    for (key, value) in config_without_env_overrides(config_values(&source, repo_root)?, |key| {
        std::env::var_os(key).is_some()
    }) {
        // This runs once before CLI parsing, threads, or child processes exist.
        unsafe {
            std::env::set_var(key, value);
        }
    }
    Ok(())
}

impl Paths {
    pub fn mode(&self) -> Result<&'static str> {
        let selected = std::env::var("ISOS_MODE").ok().or_else(|| {
            fs::read_to_string(self.config.join("mode"))
                .ok()
                .map(|value| value.trim().to_owned())
        });
        match selected.as_deref().unwrap_or("dev") {
            "dev" => Ok("dev"),
            "product" => Ok("product"),
            _ => anyhow::bail!("IsOS mode must be dev or product"),
        }
    }

    pub fn set_mode(&self, mode: &str) -> Result<()> {
        ensure!(
            matches!(mode, "dev" | "product"),
            "IsOS mode must be dev or product"
        );
        atomic_write(&self.config.join("mode"), mode.as_bytes())
    }

    pub fn discover() -> Result<Self> {
        if let Ok(base) = std::env::var("ISOS_HOME") {
            let base = PathBuf::from(base);
            ensure!(base.is_absolute(), "ISOS_HOME must be an absolute path");
            return Ok(Self {
                config: base.join("config"),
                data: base.join("data"),
                runtime: base.join("run"),
            });
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("HOME is required"))?;
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("isos");
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("isos");
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })))
            .join("isos");
        Ok(Self {
            config,
            data,
            runtime,
        })
    }

    pub fn node_graph_path(&self, name: &str) -> PathBuf {
        self.config
            .join("deployments")
            .join(format!("{name}.nodes.json"))
    }
    pub fn generated(&self, name: &str) -> PathBuf {
        self.data.join("generated").join(name)
    }
    pub fn socket(&self) -> PathBuf {
        self.runtime.join("manager.sock")
    }

    pub fn prepare(&self) -> Result<()> {
        for dir in [&self.config, &self.data, &self.runtime] {
            fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
}

pub fn deployment_name(cli: Option<String>) -> String {
    cli.or_else(|| std::env::var("ISOS_DEPLOYMENT").ok())
        .unwrap_or_else(|| "default".into())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path has no parent"))?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .with_context(|| format!("create {}", temporary.display()))?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok::<_, anyhow::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_toml_maps_settings_and_rejects_invalid_values() {
        let root = Path::new("/tmp/isos-source");
        let values = config_values("[server]\nhttp_port=16000\n[paths]\nui_repo='../IsosUI'\n[logging]\ndefault_source='all'\nlevel='warn'\nmax_bytes=262144\nbackups=2\nmax_sources=32\n", root).unwrap();
        assert_eq!(values["ISOS_HTTP_PORT"], "16000");
        assert_eq!(values["ISOSUI_REPO_PATH"], "/tmp/isos-source/../IsosUI");
        assert_eq!(values["ISOS_LOG_DEFAULT_SOURCE"], "all");
        assert_eq!(values["ISOS_LOG_BACKUPS"], "2");
        assert_eq!(values["ISOS_LOG_MAX_SOURCES"], "32");
        assert_eq!(values["ISOS_LOG_LEVEL"], "warn");
        let selected = config_without_env_overrides(values, |key| key == "ISOS_LOG_LEVEL");
        assert!(!selected.contains_key("ISOS_LOG_LEVEL"));
        assert_eq!(selected["ISOS_HTTP_PORT"], "16000");
        assert!(config_values("[server]\nhttp_port=80\n", root).is_err());
        assert!(config_values("[unknown]\nvalue=1\n", root).is_err());
        assert!(config_values("[logging]\nlevel='verbose'\n", root).is_err());
        assert!(config_values("[logging]\ndefault_source='../bad'\n", root).is_err());
        assert!(config_values("[logging]\nmax_sources=9\n", root).is_err());
        assert_eq!(
            runtime_config_path(root, None).unwrap(),
            root.join("config/isos.toml")
        );
        assert_eq!(
            runtime_config_path(root, Some(std::ffi::OsStr::new("/tmp/custom.toml"))).unwrap(),
            Path::new("/tmp/custom.toml")
        );
        assert!(runtime_config_path(root, Some(std::ffi::OsStr::new("custom.toml"))).is_err());
        for source in [
            include_str!("../../config/isos.toml"),
            include_str!("../../config/isos.example.toml"),
        ] {
            let parsed = config_values(source, root).unwrap();
            for key in ["ISOSUI_REPO_PATH", "ISOS_AETHER_BIN_DIR", "MUJOCO_ROOT"] {
                if let Some(path) = parsed.get(key) {
                    assert!(Path::new(path).is_absolute());
                }
            }
        }
    }
}
