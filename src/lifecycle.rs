//! Deployment-owned transient systemd units. Active state contains the graph
//! snapshot so stopping never follows a subsequently edited deployment.

use crate::{
    config::{self, Paths},
    graph::{Deployment, Node},
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    net::{TcpStream, ToSocketAddrs},
    os::fd::AsRawFd,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveState {
    pub schema_version: u32,
    pub graph: Deployment,
    pub started: BTreeSet<String>,
}

pub struct Lock(fs::File);
impl Lock {
    pub fn take(paths: &Paths) -> Result<Self> {
        paths.prepare()?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(paths.runtime.join("manager.lock"))?;
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        ensure!(result == 0, "cannot lock IsOS manager state");
        Ok(Self(file))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

fn unit(name: &str, node: &str) -> String {
    format!("isos-{name}-{node}.service")
}

fn run(tool: &str, args: &[String]) -> Result<String> {
    let output = Command::new(tool)
        .args(args)
        .output()
        .with_context(|| format!("run {tool}; a user systemd session is required"))?;
    ensure!(
        output.status.success(),
        "{} {}: {}",
        tool,
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

fn systemctl(args: &[String]) -> Result<String> {
    run(
        &std::env::var("ISOS_SYSTEMCTL").unwrap_or_else(|_| "systemctl".into()),
        args,
    )
}

fn is_active(unit: &str) -> bool {
    systemctl(&[
        "--user".into(),
        "is-active".into(),
        "--quiet".into(),
        unit.into(),
    ])
    .is_ok()
}

fn wait_ready(node: &Node, unit_name: &str) -> Result<()> {
    let launch = node.launch.as_ref().unwrap();
    let deadline = Instant::now() + Duration::from_millis(launch.timeout_ms as u64);
    loop {
        if !is_active(unit_name) {
            bail!(
                "{} exited before it was ready; inspect `isos logs {}`",
                node.id,
                node.id
            );
        }
        let ready = if let Some(endpoint) = &launch.ready_tcp {
            endpoint.to_socket_addrs().ok().is_some_and(|mut addrs| {
                addrs.any(|addr| {
                    TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok()
                })
            })
        } else {
            true
        };
        if ready {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!(
                "{} did not become ready within {} ms",
                node.id,
                launch.timeout_ms
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn start_unit(name: &str, node: &Node) -> Result<()> {
    let launch = node.launch.as_ref().unwrap();
    let unit_name = unit(name, &node.id);
    if is_active(&unit_name) {
        return Ok(());
    }
    let mut args = vec![
        "--user".into(),
        "--collect".into(),
        format!("--unit={unit_name}"),
        "--property=KillMode=control-group".into(),
        "--property=Restart=no".into(),
    ];
    for (key, value) in &launch.env {
        args.push(format!("--setenv={key}={value}"));
    }
    args.push("--".into());
    args.extend(launch.command.iter().cloned());
    run(
        &std::env::var("ISOS_SYSTEMD_RUN").unwrap_or_else(|_| "systemd-run".into()),
        &args,
    )?;
    wait_ready(node, &unit_name)
}

fn stop_unit(name: &str, node: &str) -> Result<()> {
    let unit_name = unit(name, node);
    if !is_active(&unit_name) {
        return Ok(());
    }
    systemctl(&["--user".into(), "stop".into(), unit_name])?;
    Ok(())
}

pub fn read_active(paths: &Paths, name: &str) -> Result<Option<ActiveState>> {
    let path = paths.active(name);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}

fn save_active(paths: &Paths, name: &str, active: &ActiveState) -> Result<()> {
    config::atomic_write(&paths.active(name), &serde_json::to_vec_pretty(active)?)
}

fn same_graph(a: &Deployment, b: &Deployment) -> Result<bool> {
    Ok(serde_json::to_value(a)? == serde_json::to_value(b)?)
}

fn check_packages(paths: &Paths, nodes: &[&Node]) -> Result<()> {
    for node in nodes {
        if let Some(package) = &node.package {
            let release_url = match node.role.as_deref() {
                Some("eos") => "https://github.com/Island-Team/Aether-EOS-R/releases",
                Some("egia") => "https://github.com/Island-Team/Aether-EGIA-R/releases",
                _ => "https://www.os-island.com/isos/packages",
            };
            let receipt = paths
                .data
                .join("packages")
                .join(&package.id)
                .join(&package.version)
                .join("receipt.json");
            ensure!(
                receipt.is_file(),
                "{} {} is not installed; use isos packages install {} or isos packages link {}; see {}",
                package.id,
                package.version,
                package.id,
                package.id,
                release_url
            );
        }
    }
    Ok(())
}

pub fn start(paths: &Paths, name: &str, role: Option<&str>) -> Result<Vec<String>> {
    let _lock = Lock::take(paths)?;
    let selected = config::load_selected(paths, name)?;
    let mut active = read_active(paths, name)?.unwrap_or(ActiveState {
        schema_version: 1,
        graph: selected.clone(),
        started: BTreeSet::new(),
    });
    ensure!(
        same_graph(&selected, &active.graph)?,
        "deployment config changed while running; stop this deployment before starting more nodes"
    );
    let closure = selected.closure(role)?;
    ensure!(
        !closure.is_empty(),
        "deployment has no enabled nodes; edit the graph created by isos config init"
    );
    check_packages(paths, &closure)?;
    let mut added: Vec<String> = Vec::new();
    for node in closure {
        if !node.owned || matches!(node.kind, crate::graph::NodeKind::Skill) {
            continue;
        }
        if active.started.contains(&node.id) && is_active(&unit(name, &node.id)) {
            continue;
        }
        ensure!(
            active.started.contains(&node.id) || !is_active(&unit(name, &node.id)),
            "{} is active outside IsOS state; stop that unit manually before IsOS can own it",
            unit(name, &node.id)
        );
        if let Err(error) = start_unit(name, node) {
            let mut rollback_errors = Vec::new();
            if let Err(e) = stop_unit(name, &node.id) {
                rollback_errors.push(e.to_string());
                active.started.insert(node.id.clone());
            }
            for id in added.iter().rev() {
                if let Err(e) = stop_unit(name, id) {
                    rollback_errors.push(e.to_string());
                } else {
                    active.started.remove(id);
                }
            }
            if active.started.is_empty() {
                let _ = fs::remove_file(paths.active(name));
            } else {
                save_active(paths, name, &active)?;
            }
            if rollback_errors.is_empty() {
                return Err(error);
            }
            bail!("{error}; rollback errors: {}", rollback_errors.join("; "));
        }
        active.started.insert(node.id.clone());
        added.push(node.id.clone());
        save_active(paths, name, &active)?;
    }
    Ok(added)
}

fn stop_set(active: &ActiveState, role: Option<&str>) -> Result<BTreeSet<String>> {
    let mut remove: BTreeSet<String> = BTreeSet::new();
    if let Some(role) = role {
        let root = active
            .graph
            .nodes
            .iter()
            .find(|n| n.role.as_deref() == Some(role))
            .ok_or_else(|| anyhow::anyhow!("{role} is not in active deployment"))?;
        remove.insert(root.id.clone());
        // A dependent component cannot remain after its provider is stopped.
        loop {
            let before = remove.len();
            for edge in &active.graph.edges {
                if remove.contains(&edge.to) {
                    remove.insert(edge.from.clone());
                }
            }
            if remove.len() == before {
                break;
            }
        }
        // Stop prerequisites only when no remaining active consumer needs them.
        loop {
            let before = remove.len();
            for edge in &active.graph.edges {
                if !remove.contains(&edge.from) {
                    continue;
                }
                if active.graph.edges.iter().any(|other| {
                    other.to == edge.to
                        && !remove.contains(&other.from)
                        && active.started.contains(&other.from)
                }) {
                    continue;
                }
                remove.insert(edge.to.clone());
            }
            if remove.len() == before {
                break;
            }
        }
    } else {
        remove.extend(active.started.iter().cloned());
    }
    Ok(remove)
}

pub fn stop(paths: &Paths, name: &str, role: Option<&str>) -> Result<Vec<String>> {
    let _lock = Lock::take(paths)?;
    let Some(mut active) = read_active(paths, name)? else {
        return Ok(Vec::new());
    };
    let order = active.graph.start_order()?;
    let remove = stop_set(&active, role)?;
    let mut stopped = Vec::new();
    for node in order.into_iter().rev() {
        if !remove.contains(&node.id) || !active.started.contains(&node.id) {
            continue;
        }
        stop_unit(name, &node.id)?;
        active.started.remove(&node.id);
        stopped.push(node.id.clone());
        save_active(paths, name, &active)?;
    }
    if active.started.is_empty() {
        let _ = fs::remove_file(paths.active(name));
    }
    Ok(stopped)
}

#[derive(Serialize)]
pub struct Status {
    pub schema_version: u32,
    pub active: Option<String>,
    pub north_http_url: String,
    pub running: bool,
    pub nodes: Vec<NodeStatus>,
}
#[derive(Serialize)]
pub struct NodeStatus {
    pub id: String,
    pub running: bool,
}

pub fn status(paths: &Paths, name: &str, role: Option<&str>) -> Result<Status> {
    let _lock = Lock::take(paths)?;
    let Some(active) = read_active(paths, name)? else {
        return Ok(Status {
            schema_version: 1,
            active: None,
            north_http_url: String::new(),
            running: false,
            nodes: Vec::new(),
        });
    };
    let nodes: Vec<NodeStatus> = active
        .started
        .iter()
        .map(|id| NodeStatus {
            id: id.clone(),
            running: is_active(&unit(name, id)),
        })
        .collect();
    let role_node = role.and_then(|r| {
        active
            .graph
            .nodes
            .iter()
            .find(|n| n.role.as_deref() == Some(r))
    });
    let running = if let Some(node) = role_node {
        nodes.iter().any(|n| n.id == node.id && n.running)
    } else {
        nodes.iter().any(|n| n.running)
    };
    let north_http_url = active
        .graph
        .nodes
        .iter()
        .find(|n| n.role.as_deref() == Some("eos"))
        .and_then(|n| n.settings.get("north_http_url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let eos_running = nodes.iter().any(|status| {
        active.graph.nodes.iter().any(|node| {
            node.role.as_deref() == Some("eos") && node.id == status.id && status.running
        })
    });
    let north_http_url = if eos_running {
        north_http_url.to_string()
    } else {
        String::new()
    };
    Ok(Status {
        schema_version: 1,
        active: running.then(|| name.into()),
        north_http_url,
        running,
        nodes,
    })
}

pub fn logs(name: &str, node: &str, lines: u32) -> Result<String> {
    ensure!(lines <= 1000, "lines must be at most 1000");
    run(
        "journalctl",
        &[
            "--user".into(),
            "--no-pager".into(),
            format!("--lines={lines}"),
            "--unit".into(),
            unit(name, node),
        ],
    )
}

pub fn ensure_config_exists(path: &Path) -> Result<()> {
    ensure!(
        path.is_file(),
        "no deployment at {}; run `isos config init` then `isos config apply <file>`",
        path.display()
    );
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Session {
    pid: i32,
    start_ticks: u64,
    token: String,
}

fn session_path(paths: &Paths, name: &str) -> std::path::PathBuf {
    paths.runtime.join(format!("{name}.sessions.json"))
}

fn process_start_ticks(pid: i32) -> Result<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let (_, tail) = stat
        .rsplit_once(") ")
        .ok_or_else(|| anyhow::anyhow!("malformed process stat"))?;
    // The tail begins with field 3 (state); starttime is field 22.
    Ok(tail
        .split_whitespace()
        .nth(19)
        .ok_or_else(|| anyhow::anyhow!("missing process starttime"))?
        .parse()?)
}

fn sessions(paths: &Paths, name: &str) -> Result<Vec<Session>> {
    let path = session_path(paths, name);
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn register_session(paths: &Paths, name: &str, pid: i32, token: &str) -> Result<()> {
    let _lock = Lock::take(paths)?;
    let mut entries = sessions(paths, name)?;
    entries.retain(|entry| process_start_ticks(entry.pid).ok() == Some(entry.start_ticks));
    entries.push(Session {
        pid,
        start_ticks: process_start_ticks(pid)?,
        token: token.into(),
    });
    config::atomic_write(&session_path(paths, name), &serde_json::to_vec(&entries)?)
}

pub fn unregister_session(paths: &Paths, name: &str, token: &str) -> Result<()> {
    let _lock = Lock::take(paths)?;
    let mut entries = sessions(paths, name)?;
    entries.retain(|entry| entry.token != token);
    config::atomic_write(&session_path(paths, name), &serde_json::to_vec(&entries)?)
}

pub fn close_other_sessions(
    paths: &Paths,
    name: &str,
    caller_token: Option<&str>,
) -> Result<usize> {
    let _lock = Lock::take(paths)?;
    let entries = sessions(paths, name)?;
    let mut kept = Vec::new();
    let mut closed = 0;
    for entry in entries {
        if caller_token == Some(entry.token.as_str()) {
            kept.push(entry);
            continue;
        }
        // pidfd pins the process identity. Compare starttime after opening it
        // so a recycled numeric PID cannot receive our signal.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, entry.pid, 0) as i32 };
        if fd < 0 {
            continue;
        }
        if process_start_ticks(entry.pid).ok() == Some(entry.start_ticks) {
            let sent =
                unsafe { libc::syscall(libc::SYS_pidfd_send_signal, fd, libc::SIGTERM, 0, 0) };
            if sent == 0 {
                closed += 1;
            }
        }
        unsafe {
            libc::close(fd);
        }
    }
    config::atomic_write(&session_path(paths, name), &serde_json::to_vec(&kept)?)?;
    Ok(closed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_current_process_identity() {
        assert!(process_start_ticks(std::process::id() as i32).unwrap() > 0);
    }

    #[test]
    fn stopping_eos_preserves_a_service_used_by_another_active_node() {
        let graph: Deployment = serde_json::from_str(
            r#"{
          "schema_version":1,"name":"default",
          "nodes":[
            {"id":"eos","kind":"component","role":"eos","launch":{"command":["/bin/true"]},
             "settings":{"north_http_url":"http://127.0.0.1:16005"}},
            {"id":"model","kind":"service","launch":{"command":["/bin/true"]}},
            {"id":"monitor","kind":"service","launch":{"command":["/bin/true"]}}
          ],
          "edges":[
            {"from":"eos","to":"model","kind":"model_binding"},
            {"from":"monitor","to":"model","kind":"depends_on"}
          ]
        }"#,
        )
        .unwrap();
        graph.validate().unwrap();
        let active = ActiveState {
            schema_version: 1,
            graph,
            started: ["eos", "model", "monitor"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        };
        let stop = stop_set(&active, Some("eos")).unwrap();
        assert_eq!(stop, BTreeSet::from(["eos".into()]));
    }
}
