//! The versioned deployment graph shared by IsOS tools and local clients.

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const GRAPH_VERSION: u32 = 2;

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
    Endpoint,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProfileRef {
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
    pub profile: Option<ProfileRef>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
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
            self.schema_version == 1 || self.schema_version == GRAPH_VERSION,
            "unsupported graph schema {}; expected 1 or {}",
            self.schema_version,
            GRAPH_VERSION
        );
        ensure!(
            slug(&self.name),
            "deployment name must be a lowercase slug of at most 48 bytes"
        );
        let mut ids = BTreeSet::new();
        let mut roles = BTreeSet::new();
        let mut listeners = BTreeSet::new();
        let mut sockets = BTreeSet::new();
        for node in &self.nodes {
            ensure!(slug(&node.id), "invalid node ID: {}", node.id);
            ensure!(ids.insert(&node.id), "duplicate node ID: {}", node.id);
            if node.enabled {
                if let Some(endpoint)=node.launch.as_ref().and_then(|l|l.ready_tcp.as_deref()) {
                    ensure!(listeners.insert(endpoint.to_owned()),"conflicting listener {endpoint}");
                }
                if matches!(node.kind,NodeKind::Endpoint) {
                    if let Some(path)=node.settings.get("socket_path").and_then(Value::as_str) {
                        ensure!(sockets.insert(path.to_owned()),"conflicting UDS path {path}");
                    }
                }
            }
            match node.kind {
                NodeKind::Component => {
                    let role = node.role.as_deref().unwrap_or("");
                    ensure!(
                        matches!(role, "eos" | "egia"),
                        "component {} requires role eos or egia",
                        node.id
                    );
                    ensure!(roles.insert(role), "duplicate component role: {role}");
                    if self.schema_version == GRAPH_VERSION {
                        let expected = if role == "eos" {
                            "eos.native"
                        } else {
                            "egia.agent"
                        };
                        let profile = node.profile.as_ref().ok_or_else(|| {
                            anyhow::anyhow!("{} requires profile {expected}", node.id)
                        })?;
                        ensure!(
                            profile.id == expected && profile.version == "1.0.0",
                            "{} has unsupported profile",
                            node.id
                        );
                    }
                }
                NodeKind::Endpoint => {
                    ensure!(
                        self.schema_version == GRAPH_VERSION,
                        "endpoint requires graph v2"
                    );
                    ensure!(
                        node.role.is_none() && !node.owned && node.launch.is_none(),
                        "{}: endpoint cannot own a process",
                        node.id
                    );
                    let profile = node
                        .profile
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("{} requires eos.uds profile", node.id))?;
                    ensure!(
                        profile.id == "eos.uds" && profile.version == "1.0.0",
                        "{} has unsupported endpoint profile",
                        node.id
                    );
                    if node.enabled {
                        let path = node
                            .settings
                            .get("socket_path")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                anyhow::anyhow!("{} requires settings.socket_path", node.id)
                            })?;
                        ensure!(
                            path.starts_with('/') && !path.contains("..") && path.len() < 100,
                            "{} has unsafe UDS path",
                            node.id
                        );
                    }
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
            if let Some(sidecar) = node.settings.get("sidecar_of") {
                ensure!(
                    matches!(node.kind, NodeKind::Service | NodeKind::Backend)
                        && matches!(sidecar.as_str(), Some("eos" | "egia")),
                    "{}: settings.sidecar_of requires eos or egia on a service/backend",
                    node.id
                );
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
                edge.input.is_some() == edge.output.is_some(),
                "{} -> {} must name both input and output ports",
                edge.from,
                edge.to
            );
            if edge.input.is_some() {
                ensure!(
                    self.schema_version == GRAPH_VERSION,
                    "typed edges require graph v2"
                );
                ensure!(
                    from.profile.as_ref().is_some_and(|p| p.id == "egia.agent")
                        && to.profile.as_ref().is_some_and(|p| p.id == "eos.uds")
                        && edge.input.as_deref() == Some("eos")
                        && edge.output.as_deref() == Some("grpc"),
                    "{} -> {} has incompatible typed ports",
                    edge.from,
                    edge.to
                );
            }
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
        if self.schema_version == GRAPH_VERSION {
            for endpoint in self
                .nodes
                .iter()
                .filter(|n| n.enabled && matches!(n.kind, NodeKind::Endpoint))
            {
                let parent = self
                    .nodes
                    .iter()
                    .find(|n| n.enabled && n.role.as_deref() == Some("eos"));
                ensure!(
                    parent.is_some_and(|eos| self
                        .edges
                        .iter()
                        .any(|edge| edge.from == endpoint.id && edge.to == eos.id)),
                    "{} must depend on its EOS process",
                    endpoint.id
                );
            }
        }
        for node in self.nodes.iter().filter(|node| node.enabled) {
            if let Some(role) = node.settings.get("sidecar_of").and_then(Value::as_str) {
                let component = self
                    .nodes
                    .iter()
                    .find(|candidate| candidate.enabled && candidate.role.as_deref() == Some(role))
                    .ok_or_else(|| {
                        anyhow::anyhow!("{}: sidecar component {role} is not enabled", node.id)
                    })?;
                ensure!(
                    self.edges
                        .iter()
                        .any(|edge| edge.from == node.id && edge.to == component.id),
                    "{}: sidecar must depend on its {role} component",
                    node.id
                );
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
            if self.schema_version == 1 {
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
            } else {
                ensure!(
                    self.edges.iter().any(|e| e.from == egia.id
                        && e.input.as_deref() == Some("eos")
                        && e.output.as_deref() == Some("grpc")
                        && self.nodes.iter().any(|n| n.id == e.to
                            && n.profile.as_ref().is_some_and(|p| p.id == "eos.uds"))),
                    "EGIA must connect to EOS UDS grpc port"
                );
            }
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
        for node in &self.nodes {
            if node.enabled && node.settings.get("sidecar_of").and_then(Value::as_str) == Some(role)
            {
                wanted.insert(node.id.as_str());
            }
        }
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
          {"id":"eos","kind":"component","role":"eos","contract":"island.eos.v1","launch":{"command":["/bin/true"]},"settings":{"north_http_url":"http://127.0.0.1:16005"}},
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
            input: None,
            output: None,
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

    #[test]
    fn eos_closure_includes_its_sidecar_without_egia() {
        let mut graph = sample();
        let sidecar: Node = serde_json::from_str(r#"{"id":"eos-north-http","kind":"service","launch":{"command":["/bin/true"]},"settings":{"sidecar_of":"eos"}}"#).unwrap();
        graph.nodes.push(sidecar);
        graph.edges.push(Edge {
            from: "eos-north-http".into(),
            to: "eos".into(),
            kind: EdgeKind::DependsOn,
            input: None,
            output: None,
        });
        graph.validate().unwrap();
        let ids: Vec<_> = graph
            .closure(Some("eos"))
            .unwrap()
            .into_iter()
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(ids, vec!["eos", "eos-north-http"]);
    }
    #[test]
    fn v2_uds_endpoint_orders_eos_before_egia() {
        let graph:Deployment=serde_json::from_value(serde_json::json!({"schema_version":2,"name":"default","nodes":[
            {"id":"eos","kind":"component","role":"eos","profile":{"id":"eos.native","version":"1.0.0"},"launch":{"command":["/bin/true"],"ready_tcp":"127.0.0.1:16200"},"settings":{"north_http_url":"http://127.0.0.1:16201"}},
            {"id":"eos-uds","kind":"endpoint","owned":false,"profile":{"id":"eos.uds","version":"1.0.0"},"settings":{"socket_path":"/tmp/isos/eos.sock"}},
            {"id":"egia","kind":"component","role":"egia","profile":{"id":"egia.agent","version":"1.0.0"},"launch":{"command":["/bin/true","--managed","--no-tui"],"ready_tcp":"127.0.0.1:16100"}}
        ],"edges":[{"from":"eos-uds","to":"eos","kind":"depends_on"},{"from":"egia","to":"eos-uds","kind":"depends_on","input":"eos","output":"grpc"}]})).unwrap();
        graph.validate().unwrap();
        assert_eq!(graph.start_order().unwrap().iter().map(|n|n.id.as_str()).collect::<Vec<_>>(),vec!["eos","eos-uds","egia"]);
        let mut broken=graph.clone(); broken.edges[1].output=Some("http".into());
        assert!(broken.validate().unwrap_err().to_string().contains("incompatible typed ports"));
        let mut unsafe_path=graph;unsafe_path.nodes[1].settings.insert("socket_path".into(),serde_json::json!("/tmp/../escape"));
        assert!(unsafe_path.validate().unwrap_err().to_string().contains("unsafe UDS"));
    }
}
