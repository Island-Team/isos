use crate::{
    component,
    graph::{Deployment, EdgeKind, NodeKind},
    packages, release,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
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

impl Paths {
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

    pub fn graph(&self, name: &str) -> PathBuf {
        self.config.join("deployments").join(format!("{name}.json"))
    }
    pub fn v2_node_graph(&self, name: &str) -> PathBuf {
        self.config
            .join("deployments")
            .join(format!("{name}.nodes.json"))
    }
    pub fn generated(&self, name: &str) -> PathBuf {
        self.data.join("generated").join(name)
    }
    pub fn active(&self, name: &str) -> PathBuf {
        self.runtime.join(format!("{name}.active.json"))
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

pub fn load(path: &Path) -> Result<Deployment> {
    let graph: Deployment = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("read {}", path.display()))?,
    )
    .with_context(|| format!("parse {}", path.display()))?;
    graph.validate()?;
    Ok(graph)
}

pub fn load_selected(paths: &Paths, name: &str) -> Result<Deployment> {
    let graph = load(&paths.graph(name))?;
    ensure!(
        graph.name == name,
        "graph name {} differs from selected deployment {name}",
        graph.name
    );
    Ok(graph)
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

#[derive(Serialize)]
struct Stack<'a> {
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    eos_config: Option<&'a str>,
    modules: Vec<String>,
    service: Vec<StackService<'a>>,
}

#[derive(Serialize)]
struct StackService<'a> {
    name: &'a str,
    role: &'a str,
    command: &'a [String],
    after: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ready_tcp: Option<&'a str>,
    timeout_ms: u32,
    env: &'a BTreeMap<String, String>,
}

pub fn generated_stack(
    graph: &Deployment,
    activation_modules: &[(String, String)],
) -> Result<String> {
    graph.validate()?;
    let eos_config = graph
        .nodes
        .iter()
        .find(|n| n.role.as_deref() == Some("eos"))
        .and_then(|n| n.settings.get("eos_config"))
        .and_then(|v| v.as_str());
    let mut modules: Vec<String> = graph
        .nodes
        .iter()
        .filter_map(|n| n.settings.get("module_toml").and_then(|v| v.as_str()))
        .map(str::to_owned)
        .collect();
    modules.extend(activation_modules.iter().map(|(_, path)| path.clone()));
    let service = graph
        .start_order()?
        .into_iter()
        .filter_map(|n| {
            if !n.owned
                || matches!(n.kind, NodeKind::Skill)
                || activation_modules.iter().any(|(id, _)| id == &n.id)
            {
                return None;
            }
            let launch = n.launch.as_ref()?;
            let after = graph
                .edges
                .iter()
                .filter(|e| e.from == n.id && !matches!(e.kind, EdgeKind::SkillLoad))
                .map(|e| e.to.as_str())
                .collect();
            Some(StackService {
                name: &n.id,
                role: if matches!(n.kind, NodeKind::Backend) {
                    "driver"
                } else {
                    "managed"
                },
                command: &launch.command,
                after,
                ready_tcp: launch.ready_tcp.as_deref(),
                timeout_ms: launch.timeout_ms,
                env: &launch.env,
            })
        })
        .collect();
    Ok(toml::to_string_pretty(&Stack {
        version: 1,
        eos_config,
        modules,
        service,
    })?)
}

fn render_template(template: &str, package_root: &Path) -> Result<String> {
    let root = package_root.canonicalize()?;
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{package_file:") {
        out.push_str(&rest[..start]);
        let tail = &rest[start + "{{package_file:".len()..];
        let end = tail
            .find("}}")
            .ok_or_else(|| anyhow::anyhow!("unterminated package_file placeholder"))?;
        let relative = &tail[..end];
        ensure!(
            !relative.is_empty()
                && relative.split('/').all(|p| !matches!(p, "" | "." | ".."))
                && !relative.starts_with('/'),
            "unsafe package_file path"
        );
        let path = root
            .join(relative)
            .canonicalize()
            .with_context(|| format!("missing package file {relative}"))?;
        ensure!(
            path.starts_with(&root) && path.is_file(),
            "package_file must resolve to a file inside its package"
        );
        let encoded = serde_json::to_string(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("package_file path must be UTF-8"))?,
        )?;
        if out.ends_with('"') && tail[end + 2..].starts_with('"') {
            out.push_str(&encoded[1..encoded.len() - 1]);
        } else {
            out.push_str(&encoded);
        }
        rest = &tail[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

fn render_activations(
    paths: &Paths,
    graph: &Deployment,
    dir: &Path,
    final_dir: &Path,
) -> Result<Vec<(String, String)>> {
    let mut modules = Vec::new();
    for node in &graph.nodes {
        if !node.enabled {
            continue;
        }
        let Some(package) = &node.package else {
            continue;
        };
        let installed = packages::package_path(paths, &package.id, &package.version)?;
        let release_path = installed.join("release.json");
        if !release_path.exists() {
            continue;
        } // Local development links use explicit module_toml.
        let manifest = release::Release::read_from_path(&release_path)?;
        if let Some(activation) = manifest.activation {
            for file in activation.files {
                ensure!(
                    file.destination.ends_with(".toml"),
                    "activation destination must be TOML"
                );
                let source = installed.join(&file.source);
                let rendered = render_template(
                    &fs::read_to_string(&source)
                        .with_context(|| format!("read activation {}", source.display()))?,
                    &installed,
                )?;
                let _: toml::Value = toml::from_str(&rendered).with_context(|| {
                    format!("rendered activation {} is not TOML", file.destination)
                })?;
                let destination = dir
                    .join("activation")
                    .join(&node.id)
                    .join(&file.destination);
                atomic_write(&destination, rendered.as_bytes())?;
                if file.destination.ends_with("module.toml") {
                    modules.push((
                        node.id.clone(),
                        final_dir
                            .join(destination.strip_prefix(dir)?)
                            .to_string_lossy()
                            .into_owned(),
                    ));
                }
            }
        }
    }
    Ok(modules)
}

pub fn apply(paths: &Paths, graph: &Deployment) -> Result<()> {
    graph.validate()?;
    component::validate_profiles(graph)?;
    paths.prepare()?;
    let graph_bytes = serde_json::to_vec_pretty(graph)?;
    let dir = paths.generated(&graph.name);
    let staging = dir.with_extension(format!("{}.staging", std::process::id()));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    let result = (|| -> Result<()> {
        component::render(paths, graph, &staging)?;
        let modules = render_activations(paths, graph, &staging, &dir)?;
        let stack = generated_stack(graph, &modules)?;
        atomic_write(&staging.join("stack.toml"), stack.as_bytes())?;
        atomic_write(&staging.join("graph.json"), &graph_bytes)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    let backup = dir.with_extension(format!("{}.old", std::process::id()));
    if dir.exists() {
        fs::rename(&dir, &backup)?;
    }
    fs::rename(&staging, &dir)?;
    if backup.exists() {
        fs::remove_dir_all(backup)?;
    }
    let mut formatted = graph_bytes;
    formatted.push(b'\n');
    atomic_write(&paths.graph(&graph.name), &formatted)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    #[test]
    fn generates_readable_stack() {
        let graph: Deployment = serde_json::from_str(r#"{"schema_version":1,"name":"default","nodes":[{"id":"eos","kind":"component","role":"eos","launch":{"command":["/bin/true"],"ready_tcp":"127.0.0.1:16003"},"settings":{"north_http_url":"http://127.0.0.1:16005"}}]}"#).unwrap();
        let stack = generated_stack(&graph, &[]).unwrap();
        let parsed: toml::Value = toml::from_str(&stack).unwrap();
        assert_eq!(parsed["service"][0]["name"].as_str(), Some("eos"));
    }

    #[test]
    fn renders_package_file_before_validating_toml() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("isos-config-test-{}-{unique}", std::process::id()));
        let paths = Paths {
            config: root.join("config"),
            data: root.join("data"),
            runtime: root.join("run"),
        };
        let package = paths.data.join("packages/island/test/1.0.0");
        fs::create_dir_all(package.join("config")).unwrap();
        fs::write(package.join("resource.dat"), "payload").unwrap();
        fs::write(
            package.join("config/module.toml"),
            "file = {{package_file:resource.dat}}\nquoted = \"{{package_file:resource.dat}}\"\n",
        )
        .unwrap();
        let manifest = serde_json::json!({
            "schema_version":4,"id":"island/test","kind":"backend","version":"1.0.0",
            "source_commit":"0000000000000000000000000000000000000000",
            "assets":[{"os":"linux","arch":"x86_64",
                "url":"https://github.com/Island-Team/test/releases/download/v1.0.0/test.tar.gz",
                "size_bytes":1,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}],
            "activation":{"files":[{"source":"config/module.toml","destination":"module.toml"}]}
        });
        fs::write(
            package.join("release.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let graph: Deployment = serde_json::from_value(serde_json::json!({
            "schema_version":1,"name":"default","nodes":[{"id":"test","kind":"backend",
                "package":{"id":"island/test","version":"1.0.0"},
                "launch":{"command":["/bin/true"]}}]
        }))
        .unwrap();
        apply(&paths, &graph).unwrap();
        let generated = paths.generated("default");
        let rendered = fs::read_to_string(generated.join("activation/test/module.toml")).unwrap();
        assert!(rendered.contains(package.join("resource.dat").to_str().unwrap()));
        let parsed: toml::Value = toml::from_str(&rendered).unwrap();
        assert_eq!(parsed["file"], parsed["quoted"]);
        let stack = fs::read_to_string(generated.join("stack.toml")).unwrap();
        assert!(
            stack.contains(
                generated
                    .join("activation/test/module.toml")
                    .to_str()
                    .unwrap()
            )
        );
        let stack_value: toml::Value = toml::from_str(&stack).unwrap();
        assert!(
            stack_value
                .get("service")
                .is_none_or(|value| value.as_array().is_some_and(Vec::is_empty))
        );
        fs::remove_dir_all(root).unwrap();
    }
}
