//! Exact selected-reader admission for the authored graph and projected children.
use crate::{config::Paths, core_bins, core_setup, graph_compat::Diagnostic, process::ProcessSpec};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const LIMIT: u64 = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
const EGIA_TYPES: &[&str] = &[
    "egia.common.context@v1.0",
    "egia.common.guard@v1.0",
    "egia.common.planner@v1.0",
    "egia.common.source@v1.0",
];

#[derive(Clone)]
struct Claims {
    core: &'static str,
    binary: &'static str,
    hash: Option<String>,
    graph_versions: BTreeSet<String>,
    type_versions: BTreeSet<String>,
    node_types: BTreeSet<String>,
    features: BTreeSet<String>,
    scalar_fallback: bool,
}

impl Claims {
    fn empty(core: &'static str, binary: &'static str, path: &Path) -> Self {
        Self {
            core,
            binary,
            hash: fs::read(path)
                .ok()
                .map(|bytes| hex::encode(Sha256::digest(bytes))),
            graph_versions: BTreeSet::new(),
            type_versions: BTreeSet::new(),
            node_types: BTreeSet::new(),
            features: BTreeSet::new(),
            scalar_fallback: false,
        }
    }

    fn diagnostic(
        &self,
        code: &'static str,
        path: impl Into<String>,
        stage: &'static str,
        graph: Option<&str>,
        node_type: Option<&str>,
        feature: Option<&str>,
        package: Option<&Value>,
    ) -> Diagnostic {
        let mut context = BTreeMap::new();
        context.insert("stage".into(), json!(stage));
        context.insert(
            "reader".into(),
            json!({
                "core":self.core,
                "binary":self.binary,
                "binary_sha256":self.hash,
                "requested":{"graph_version":graph,"node_type":node_type,"feature":feature,"package":package},
                "supported":{
                    "graph_versions":self.graph_versions,
                    "node_type_versions":self.type_versions,
                    "node_types":self.node_types,
                    "features":self.features
                }
            }),
        );
        let repair = match code {
            "reader_binary_mismatch" | "reader_identity_invalid" => {
                "restore the selected build and Apply again"
            }
            "reader_package_pin_mismatch" => {
                "restore the exact verified package pin and Apply again"
            }
            _ => "select a verified compatible build and Apply again",
        };
        let subject = feature.or(node_type).or(graph).unwrap_or("identity");
        Diagnostic {
            code,
            path: path.into(),
            message: format!("{} lacks or rejects {subject}; {repair}", self.binary),
            context,
        }
    }
}

fn member_set(value: &Value, key: &str, nonempty: bool) -> Option<BTreeSet<String>> {
    let array = value.get(key)?.as_array()?;
    if nonempty && array.is_empty() {
        return None;
    }
    let mut values = BTreeSet::new();
    for item in array {
        let item = item.as_str()?;
        if item.is_empty() || item.len() > 256 || !item.is_ascii() || !values.insert(item.into()) {
            return None;
        }
    }
    Some(values)
}

fn parse_claims(
    mut claims: Claims,
    reply: &Value,
    graph_reader: bool,
    setup: bool,
) -> Result<Claims, Diagnostic> {
    let identity_path = if setup { "" } else { "/version" };
    if reply[if setup { "core" } else { "binary" }] != claims.core && setup
        || !setup && reply["binary"] != claims.binary
    {
        return Err(claims.diagnostic(
            "reader_binary_mismatch",
            identity_path,
            "reader",
            None,
            None,
            None,
            None,
        ));
    }
    if !reply["product_version"]
        .as_str()
        .is_some_and(|v| !v.is_empty())
        || setup && reply["setup_schema_version"] != 1
    {
        return Err(claims.diagnostic(
            "reader_identity_invalid",
            identity_path,
            "reader",
            None,
            None,
            None,
            None,
        ));
    }
    claims.features = if reply.get("features").is_none() {
        BTreeSet::new()
    } else {
        member_set(reply, "features", false).ok_or_else(|| {
            claims.diagnostic(
                "reader_identity_invalid",
                identity_path,
                "reader",
                None,
                None,
                None,
                None,
            )
        })?
    };
    if !graph_reader {
        if claims.core == "egia"
            && [
                "supported_graph_versions",
                "supported_node_type_versions",
                "supported_node_types",
            ]
            .iter()
            .any(|key| reply.get(*key).is_some())
        {
            return Err(claims.diagnostic(
                "reader_identity_invalid",
                identity_path,
                "reader",
                None,
                None,
                None,
                None,
            ));
        }
        return Ok(claims);
    }
    let keys = [
        "supported_graph_versions",
        "supported_node_type_versions",
        "supported_node_types",
    ];
    let present = keys.iter().filter(|key| reply.get(**key).is_some()).count();
    if present == 0 {
        if reply["graph_version"] != "1.0.0" || reply["node_type_version"] != "1.0" {
            return Err(claims.diagnostic(
                "reader_identity_invalid",
                identity_path,
                "reader",
                None,
                None,
                None,
                None,
            ));
        }
        claims.graph_versions.insert("1.0.0".into());
        claims.type_versions.insert("1.0".into());
        claims.scalar_fallback = true;
        return Ok(claims);
    }
    if present != keys.len() {
        return Err(claims.diagnostic(
            "reader_identity_invalid",
            identity_path,
            "reader",
            None,
            None,
            None,
            None,
        ));
    }
    claims.graph_versions = member_set(reply, keys[0], true).ok_or_else(|| {
        claims.diagnostic(
            "reader_identity_invalid",
            identity_path,
            "reader",
            None,
            None,
            None,
            None,
        )
    })?;
    claims.type_versions = member_set(reply, keys[1], true).ok_or_else(|| {
        claims.diagnostic(
            "reader_identity_invalid",
            identity_path,
            "reader",
            None,
            None,
            None,
            None,
        )
    })?;
    claims.node_types = member_set(reply, keys[2], false).ok_or_else(|| {
        claims.diagnostic(
            "reader_identity_invalid",
            identity_path,
            "reader",
            None,
            None,
            None,
            None,
        )
    })?;
    if claims.node_types.iter().any(|id| {
        id.rsplit_once("@v")
            .is_none_or(|(_, suffix)| !claims.type_versions.contains(suffix))
    }) {
        return Err(claims.diagnostic(
            "reader_identity_invalid",
            identity_path,
            "reader",
            None,
            None,
            None,
            None,
        ));
    }
    Ok(claims)
}

fn probe(
    path: &Path,
    binary: &'static str,
    core: &'static str,
    graph_reader: bool,
) -> Result<Claims, Diagnostic> {
    let claims = Claims::empty(core, binary, path);
    if claims.hash.is_none() {
        return Err(claims.diagnostic(
            "reader_binary_mismatch",
            "",
            "reader",
            None,
            None,
            None,
            None,
        ));
    }
    let mut child = Command::new(path)
        .arg("--capabilities")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| {
            claims.diagnostic(
                "reader_binary_mismatch",
                "",
                "reader",
                None,
                None,
                None,
                None,
            )
        })?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break Some(status);
        }
        if started.elapsed() >= TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = reader.join().ok().and_then(Result::ok);
    if !status.is_some_and(|s| s.success())
        || output.as_ref().is_none_or(|v| v.len() as u64 > LIMIT)
    {
        return Err(claims.diagnostic(
            "reader_identity_invalid",
            "",
            "reader",
            None,
            None,
            None,
            None,
        ));
    }
    let value: Value = serde_json::from_slice(output.as_ref().unwrap()).map_err(|_| {
        claims.diagnostic(
            "reader_identity_invalid",
            "",
            "reader",
            None,
            None,
            None,
            None,
        )
    })?;
    parse_claims(claims, &value, graph_reader, false)
}

fn setup(paths: &Paths, core: &'static str) -> Result<Claims, Diagnostic> {
    let binary = if core == "eos" {
        "aether-profile"
    } else {
        "egia-config"
    };
    let path = if core == "eos" {
        core_bins::resolve(binary)
    } else {
        core_setup::adapter_path(paths, core)
    };
    let path = path.unwrap_or_else(|_| PathBuf::new());
    let claims = Claims::empty(core, binary, &path);
    let reply = core_setup::identity(paths, core).map_err(|_| {
        claims.diagnostic(
            "reader_identity_invalid",
            "",
            "reader",
            None,
            None,
            None,
            None,
        )
    })?;
    parse_claims(claims, &reply, true, true)
}

fn nodes(graph: &Value, core: &str) -> Vec<(String, String, Option<Value>)> {
    let mut found = Vec::new();
    let Some(layers) = graph
        .pointer(&format!("/cores/{core}/layers"))
        .and_then(Value::as_object)
    else {
        return found;
    };
    for (layer_id, layer) in layers {
        let Some(groups) = layer["groups"].as_object() else {
            continue;
        };
        for (group_id, group) in groups {
            let Some(entries) = group["nodes"].as_object() else {
                continue;
            };
            for (key, node) in entries {
                if node["enabled"] == false {
                    continue;
                }
                if let Some(id) = node["node_type"].as_str() {
                    found.push((
                        format!("/cores/{core}/layers/{layer_id}/groups/{group_id}/nodes/{key}"),
                        id.into(),
                        node.get("package").cloned(),
                    ));
                }
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

fn check_reader(
    claims: &Claims,
    graph: &Value,
    core: &str,
    catalog: &Value,
) -> Result<(), Diagnostic> {
    let version = graph["version"].as_str().unwrap_or_default();
    if !claims.graph_versions.contains(version) {
        return Err(claims.diagnostic(
            "reader_graph_unsupported",
            "/version",
            "graph",
            Some(version),
            None,
            None,
            None,
        ));
    }
    let builtins = &claims.node_types;
    let registered: BTreeMap<&str, &Value> = catalog["types"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["id"].as_str().map(|id| (id, item)))
        .collect();
    for (path, id, pin) in nodes(graph, core) {
        let Some((_, suffix)) = id.rsplit_once("@v") else {
            continue;
        };
        let contributed = registered
            .get(id.as_str())
            .and_then(|item| item.get("package"));
        if contributed.is_some() && pin.as_ref() != contributed {
            return Err(claims.diagnostic(
                "reader_package_pin_mismatch",
                format!("{path}/package"),
                "package",
                Some(version),
                Some(&id),
                None,
                pin.as_ref(),
            ));
        }
        if !claims.type_versions.contains(suffix)
            || (!builtins.contains(&id) && !claims.scalar_fallback && contributed.is_none())
            || (claims.scalar_fallback
                && !(version == "1.0.0"
                    && id.ends_with("@v1.0")
                    && registered.contains_key(id.as_str())))
        {
            return Err(claims.diagnostic(
                "reader_type_unsupported",
                format!("{path}/node_type"),
                "reader",
                Some(version),
                Some(&id),
                None,
                pin.as_ref(),
            ));
        }
    }
    Ok(())
}

fn feature(
    claims: &Claims,
    graph: &Value,
    path: &str,
    id: Option<&str>,
    token: &str,
) -> Result<(), Diagnostic> {
    if claims.features.contains(token) {
        return Ok(());
    }
    Err(claims.diagnostic(
        "reader_feature_unsupported",
        path,
        "feature",
        graph["version"].as_str(),
        id,
        Some(token),
        None,
    ))
}

pub fn check(
    paths: &Paths,
    graph: &Value,
    specs: &[ProcessSpec],
    catalog: &Value,
) -> Result<(), Diagnostic> {
    let version = graph["version"].as_str().unwrap_or_default();
    let profile = setup(paths, "eos")?;
    check_reader(&profile, graph, "eos", catalog)?;
    if version == "0.1.0" {
        for token in [
            "graph-0.1-reader",
            "node-types",
            "data-idl",
            "setup-v1",
            "catalog",
        ] {
            feature(&profile, graph, "/version", None, token)?;
        }
    }
    let egia_enabled = specs.iter().any(|spec| spec.id == "egia");
    if egia_enabled {
        let egia = setup(paths, "egia")?;
        let egia_catalog =
            json!({"types":EGIA_TYPES.iter().map(|id| json!({"id":id})).collect::<Vec<_>>()});
        check_reader(&egia, graph, "egia", &egia_catalog)?;
        if version == "0.1.0" {
            for token in ["graph-0.1-reader", "setup-v1", "catalog"] {
                feature(&egia, graph, "/version", None, token)?;
            }
        }
    }
    for (id, binary, graph_reader) in [
        ("eos", "aether-runtime", true),
        ("config", "aether-config-server", true),
        ("bridge", "aether-http-bridge", false),
        ("egia", "egia", false),
    ] {
        let Some(spec) = specs.iter().find(|spec| spec.id == id) else {
            continue;
        };
        let path = Path::new(&spec.settings.command[0]);
        let mut claims = Claims::empty(if id == "egia" { "egia" } else { "eos" }, binary, path);
        let pointer = format!("/cores/isos/layers/process/groups/component/nodes/{id}");
        let role_path = if graph.pointer(&format!("{pointer}/settings/role")).is_some() {
            format!("{pointer}/settings/role")
        } else {
            format!("{pointer}/node_type")
        };
        if claims.hash.is_none() {
            return Err(claims.diagnostic(
                "reader_binary_mismatch",
                role_path,
                "reader",
                None,
                None,
                None,
                None,
            ));
        }
        claims = probe(path, binary, claims.core, graph_reader).map_err(|mut error| {
            error.path = role_path.clone();
            error
        })?;
        if graph_reader {
            check_reader(&claims, graph, "eos", catalog)?;
        }
        if graph_reader && version == "0.1.0" {
            for token in ["graph-0.1-reader", "node-types", "data-idl"] {
                feature(&claims, graph, "/version", None, token)?;
            }
        }
        if id == "bridge" {
            feature(
                &claims,
                graph,
                &format!("{pointer}/node_type"),
                None,
                "vision-ws",
            )?;
        }
        if id == "egia" {
            for token in ["managed-config-v1", "eos-gateway-client-v2"] {
                feature(&claims, graph, &format!("{pointer}/node_type"), None, token)?;
            }
        }
        if id == "eos" {
            if spec.settings.command.iter().any(|arg| arg == "--log-dir") {
                feature(&claims, graph, "/version", None, "log-dir")?;
            }
            for (node_path, node_type, _) in nodes(graph, "eos") {
                let required = if node_type.starts_with("eos.dispatcher.primitive_") {
                    Some("primitive-binding")
                } else if node_type.starts_with("eos.interface.camera_gate") {
                    Some("vision-gate")
                } else if node_type.starts_with("eos.process.mujoco") {
                    Some("mujoco-child-v2")
                } else {
                    None
                };
                if let Some(token) = required {
                    feature(
                        &claims,
                        graph,
                        &format!("{node_path}/node_type"),
                        Some(&node_type),
                        token,
                    )?;
                }
            }
        }
    }
    Ok(())
}
