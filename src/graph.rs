//! The versioned deployment graph shared by the CLI and the future WebUI.

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const GRAPH_VERSION: u32 = 1;

fn enabled_by_default() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deployment {
    pub schema_version: u32,
    pub name: String,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Component,
    Service,
    Backend,
    Skill,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRef {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launch {
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_tcp: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u32,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

fn default_timeout() -> u32 {
    30_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default = "enabled_by_default")]
    pub owned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<Launch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    DependsOn,
    ModelBinding,
    BackendAttachment,
    SkillLoad,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    /// Consumer node ID. A consumer starts after its provider.
    pub from: String,
    /// Provider node ID.
    pub to: String,
    pub kind: EdgeKind,
}

fn slug(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('a'..='z'))
        && value.len() <= 48
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Deployment {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == GRAPH_VERSION,
            "unsupported graph schema {}; expected {}",
            self.schema_version,
            GRAPH_VERSION
        );
        ensure!(
            slug(&self.name),
            "deployment name must be a lowercase slug of at most 48 bytes"
        );
        let mut ids = BTreeSet::new();
        let mut roles = BTreeSet::new();
        for node in &self.nodes {
            ensure!(slug(&node.id), "invalid node ID: {}", node.id);
            ensure!(ids.insert(&node.id), "duplicate node ID: {}", node.id);
            match node.kind {
                NodeKind::Component => {
                    let role = node.role.as_deref().unwrap_or("");
                    ensure!(
                        matches!(role, "eos" | "egia"),
                        "component {} requires role eos or egia",
                        node.id
                    );
                    ensure!(roles.insert(role), "duplicate component role: {role}");
                }
                _ => ensure!(node.role.is_none(), "only component nodes may have a role"),
            }
            if let Some(package) = &node.package {
                ensure!(
                    package
                        .id
                        .split_once('/')
                        .is_some_and(|(a, b)| slug(a) && slug(b)),
                    "{}: package ID must be namespace/name",
                    node.id
                );
                ensure!(
                    !package.version.is_empty()
                        && package.version.chars().next().unwrap().is_ascii_digit(),
                    "{}: package version is invalid",
                    node.id
                );
            }
            if node.owned && node.enabled && !matches!(node.kind, NodeKind::Skill) {
                let launch = node.launch.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("{}: enabled owned node requires launch", node.id)
                })?;
                ensure!(
                    !launch.command.is_empty() && launch.command.iter().all(|v| !v.is_empty()),
                    "{}: launch command must contain nonempty arguments",
                    node.id
                );
                if node.role.as_deref() == Some("egia") {
                    ensure!(
                        launch.command.iter().any(|arg| arg == "--managed")
                            && launch.command.iter().any(|arg| arg == "--no-tui"),
                        "{}: IsOS-managed EGIA requires --managed --no-tui",
                        node.id
                    );
                }
                if node.role.as_deref() == Some("eos") {
                    let url = node
                        .settings
                        .get("north_http_url")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "{}: settings.north_http_url is required for isos eos enter",
                                node.id
                            )
                        })?;
                    let authority = url.strip_prefix("http://").ok_or_else(|| {
                        anyhow::anyhow!("{}: north_http_url must be http://host:port", node.id)
                    })?;
                    let (host, port) = authority.rsplit_once(':').ok_or_else(|| {
                        anyhow::anyhow!("{}: north_http_url must be http://host:port", node.id)
                    })?;
                    ensure!(
                        !host.is_empty() && port.parse::<u16>().is_ok_and(|p| p > 0),
                        "{}: north_http_url must be http://host:port",
                        node.id
                    );
                }
                ensure!(
                    (1..=300_000).contains(&launch.timeout_ms),
                    "{}: launch timeout_ms must be 1..300000",
                    node.id
                );
                if let Some(endpoint) = &launch.ready_tcp {
                    let (_, port) = endpoint.rsplit_once(':').ok_or_else(|| {
                        anyhow::anyhow!("{}: ready_tcp must be host:port", node.id)
                    })?;
                    ensure!(
                        port.parse::<u16>().is_ok_and(|p| p > 0),
                        "{}: ready_tcp has invalid port",
                        node.id
                    );
                }
            }
        }
        let by_id: BTreeMap<_, _> = self.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut edges = BTreeSet::new();
        for edge in &self.edges {
            let from = by_id
                .get(edge.from.as_str())
                .ok_or_else(|| anyhow::anyhow!("edge consumer {} does not exist", edge.from))?;
            let to = by_id
                .get(edge.to.as_str())
                .ok_or_else(|| anyhow::anyhow!("edge provider {} does not exist", edge.to))?;
            ensure!(
                edge.from != edge.to,
                "{} cannot depend on itself",
                edge.from
            );
            ensure!(
                edges.insert((&edge.from, &edge.to)),
                "duplicate edge {} -> {}",
                edge.from,
                edge.to
            );
            ensure!(
                !from.enabled || to.enabled,
                "{} depends on disabled {}",
                edge.from,
                edge.to
            );
            match edge.kind {
                EdgeKind::DependsOn => {}
                EdgeKind::ModelBinding => ensure!(
                    matches!(to.kind, NodeKind::Service),
                    "model_binding provider {} must be a service",
                    edge.to
                ),
                EdgeKind::BackendAttachment => ensure!(
                    matches!(to.kind, NodeKind::Backend),
                    "backend_attachment provider {} must be a backend",
                    edge.to
                ),
                EdgeKind::SkillLoad => ensure!(
                    matches!(to.kind, NodeKind::Skill),
                    "skill_load provider {} must be a skill",
                    edge.to
                ),
            }
        }
        let eos = self
            .nodes
            .iter()
            .find(|n| n.enabled && n.role.as_deref() == Some("eos"));
        let egia = self
            .nodes
            .iter()
            .find(|n| n.enabled && n.role.as_deref() == Some("egia"));
        if let (Some(eos), Some(egia)) = (eos, egia) {
            ensure!(
                eos.contract.is_some() && eos.contract == egia.contract,
                "EOS and EGIA require the same explicit protocol contract"
            );
            ensure!(
                self.edges
                    .iter()
                    .any(|e| e.from == egia.id && e.to == eos.id),
                "EGIA must depend on EOS"
            );
        }
        self.start_order()?;
        Ok(())
    }

    pub fn start_order(&self) -> Result<Vec<&Node>> {
        let enabled: BTreeMap<_, _> = self
            .nodes
            .iter()
            .filter(|n| n.enabled)
            .map(|n| (n.id.as_str(), n))
            .collect();
        let mut pending: BTreeMap<&str, BTreeSet<&str>> =
            enabled.keys().map(|id| (*id, BTreeSet::new())).collect();
        for edge in &self.edges {
            if enabled.contains_key(edge.from.as_str()) && enabled.contains_key(edge.to.as_str()) {
                pending
                    .get_mut(edge.from.as_str())
                    .unwrap()
                    .insert(edge.to.as_str());
            }
        }
        let mut order = Vec::new();
        while !pending.is_empty() {
            let ready = pending
                .iter()
                .find(|(_, deps)| deps.is_empty())
                .map(|(id, _)| *id);
            let Some(id) = ready else {
                bail!("deployment graph has a dependency cycle")
            };
            order.push(enabled[id]);
            pending.remove(id);
            for deps in pending.values_mut() {
                deps.remove(id);
            }
        }
        Ok(order)
    }

    pub fn closure(&self, role: Option<&str>) -> Result<Vec<&Node>> {
        let order = self.start_order()?;
        let Some(role) = role else { return Ok(order) };
        let root = self
            .nodes
            .iter()
            .find(|n| n.enabled && n.role.as_deref() == Some(role))
            .ok_or_else(|| anyhow::anyhow!("{role} is not enabled in this deployment"))?;
        let mut wanted = BTreeSet::from([root.id.as_str()]);
        loop {
            let before = wanted.len();
            for edge in &self.edges {
                if wanted.contains(edge.from.as_str()) {
                    wanted.insert(&edge.to);
                }
            }
            if wanted.len() == before {
                break;
            }
        }
        Ok(order
            .into_iter()
            .filter(|n| wanted.contains(n.id.as_str()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Deployment {
        serde_json::from_str(r#"{"schema_version":1,"name":"default","nodes":[
          {"id":"eos","kind":"component","role":"eos","contract":"island.eos.v1","launch":{"command":["/bin/true"]},"settings":{"north_http_url":"http://127.0.0.1:16004"}},
          {"id":"egia","kind":"component","role":"egia","contract":"island.eos.v1","launch":{"command":["/bin/true","--managed","--no-tui"]}}
        ],"edges":[{"from":"egia","to":"eos","kind":"depends_on"}]}"#).unwrap()
    }
    #[test]
    fn sorts_dependencies() {
        let graph = sample();
        graph.validate().unwrap();
        assert_eq!(
            graph
                .start_order()
                .unwrap()
                .iter()
                .map(|n| n.id.as_str())
                .collect::<Vec<_>>(),
            vec!["eos", "egia"]
        );
    }
    #[test]
    fn rejects_cycles() {
        let mut graph = sample();
        graph.edges.push(Edge {
            from: "eos".into(),
            to: "egia".into(),
            kind: EdgeKind::DependsOn,
        });
        assert!(graph.validate().unwrap_err().to_string().contains("cycle"));
    }
    #[test]
    fn rejects_contract_mismatch() {
        let mut graph = sample();
        graph.nodes[1].contract = Some("old".into());
        assert!(
            graph
                .validate()
                .unwrap_err()
                .to_string()
                .contains("contract")
        );
    }
}
