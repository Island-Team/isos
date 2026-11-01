//! Durable current-run scope and authenticated, rotation-aware log cursors.

use super::{LogStore, event_policy, level_rank, source_valid, token_valid};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const STATE_MAX: u64 = 32_768;
const SCAN_MAX: usize = 65_536;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Anchor {
    event: Value,
    segment: String,
    after_offset: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    schema_version: u8,
    deployment: String,
    manager_session: String,
    never_run: bool,
    last_run_id: Option<String>,
    manager_anchor: Option<Anchor>,
    run_anchor: Option<Anchor>,
    run_anchor_state: String,
    loss_baseline: u64,
    loss_uncertain: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Origin {
    schema_version: u8,
    deployment: String,
    first_segment: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Token {
    d: String,
    k: String,
    r: String,
    f: String,
    s: String,
    o: u64,
    p: bool,
    l: u64,
}

#[derive(Debug)]
pub struct CurrentError {
    pub code: &'static str,
    pub path: &'static str,
    pub message: &'static str,
    pub status: u16,
}

impl CurrentError {
    fn new(code: &'static str, path: &'static str, message: &'static str, status: u16) -> Self {
        Self {
            code,
            path,
            message,
            status,
        }
    }

    pub fn diagnostic(&self) -> Value {
        json!({"path":self.path,"code":self.code,"message":self.message})
    }

    pub fn public_error(&self) -> &'static str {
        match self.code {
            "log_scope_unavailable" => "log scope unavailable",
            "log_cursor_invalid" | "log_cursor_filter_mismatch" => "log cursor rejected",
            _ => "log query rejected",
        }
    }
}

fn scope_error() -> CurrentError {
    CurrentError::new(
        "log_scope_unavailable",
        "/scope",
        "Current log scope unavailable",
        503,
    )
}

fn cursor_invalid() -> CurrentError {
    CurrentError::new(
        "log_cursor_invalid",
        "/query/cursor",
        "Log cursor invalid",
        400,
    )
}

fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("cursor state parent")?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let temporary = parent.join(format!(
        ".cursor-{}-{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok::<_, anyhow::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn private_read(path: &Path, max: u64) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(meta.file_type().is_file(), "cursor file is not regular");
    ensure!(
        meta.permissions().mode() & 0o077 == 0,
        "cursor file is not private"
    );
    ensure!(meta.len() <= max, "cursor file exceeds bound");
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= max, "cursor file changed while read");
    Ok(bytes)
}

fn identity(path: &Path) -> Result<String> {
    let meta = fs::metadata(path)?;
    let birth = meta
        .created()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    Ok(format!("{:x}-{:x}-{birth:x}", meta.dev(), meta.ino()))
}

fn deployment(store: &LogStore) -> Result<String> {
    Ok(store
        .root
        .file_name()
        .context("deployment log root")?
        .to_string_lossy()
        .into_owned())
}

fn state_path(store: &LogStore) -> PathBuf {
    store.root.join("cursor-state.json")
}

fn loss_total(store: &LogStore) -> u64 {
    store.losses.load(Ordering::Relaxed)
}

fn validate_state(state: &State, name: &str) -> Result<()> {
    ensure!(
        state.schema_version == 1 && state.deployment == name,
        "cursor state identity"
    );
    ensure!(
        token_valid(&state.manager_session, 64),
        "cursor manager identity"
    );
    ensure!(
        state
            .last_run_id
            .as_deref()
            .is_none_or(|id| id.starts_with('r') && token_valid(id, 64)),
        "cursor run identity"
    );
    ensure!(
        state.never_run == state.last_run_id.is_none(),
        "cursor run marker mismatch"
    );
    ensure!(
        matches!(
            state.run_anchor_state.as_str(),
            "pending" | "found" | "unavailable"
        ),
        "cursor anchor state"
    );
    if state.run_anchor_state == "found" {
        ensure!(state.run_anchor.is_some(), "cursor anchor missing");
    }
    for anchor in [&state.manager_anchor, &state.run_anchor]
        .into_iter()
        .flatten()
    {
        ensure!(
            anchor.event.is_object() && anchor.event["run_id"].is_string(),
            "cursor anchor event"
        );
        ensure!(
            anchor.segment.len() <= 96 && !anchor.segment.is_empty(),
            "cursor anchor segment"
        );
    }
    Ok(())
}

fn save_state(store: &LogStore, state: &State) -> Result<()> {
    #[cfg(test)]
    if state.deployment == "p07-anchor-fail" && state.run_anchor_state == "found" {
        anyhow::bail!("injected anchor publication failure");
    }
    private_write(&state_path(store), &serde_json::to_vec(state)?)
}

pub(super) fn initialize(store: &LogStore, fresh_root: bool) -> Result<()> {
    if !fresh_root {
        return Ok(());
    }
    let name = deployment(store)?;
    let origin = Origin {
        schema_version: 1,
        deployment: name.clone(),
        first_segment: identity(&store.root.join("events.log"))?,
    };
    private_write(
        &store.root.join("cursor-origin.json"),
        &serde_json::to_vec(&origin)?,
    )?;
    let state = State {
        schema_version: 1,
        deployment: name,
        manager_session: crate::manager::manager_session_id().to_owned(),
        never_run: true,
        last_run_id: None,
        manager_anchor: None,
        run_anchor: None,
        run_anchor_state: "pending".into(),
        loss_baseline: loss_total(store),
        loss_uncertain: false,
    };
    save_state(store, &state)?;
    let _ = key(store)?;
    Ok(())
}

fn segments(store: &LogStore) -> Result<Vec<Segment>> {
    let mut result = Vec::new();
    for backup in (1..=store.backups).rev() {
        let path = store.root.join(format!("events.log.{backup}"));
        if path.exists() {
            result.push(Segment::at(path)?);
        }
    }
    result.push(Segment::at(store.root.join("events.log"))?);
    Ok(result)
}

#[derive(Clone)]
struct Segment {
    path: PathBuf,
    id: String,
    len: u64,
}

impl Segment {
    fn at(path: PathBuf) -> Result<Self> {
        let meta = fs::symlink_metadata(&path)?;
        ensure!(meta.file_type().is_file(), "log segment is not regular");
        Ok(Self {
            id: identity(&path)?,
            len: meta.len(),
            path,
        })
    }
}

fn recover_state(store: &LogStore, name: &str) -> Result<State> {
    let origin: Origin =
        serde_json::from_slice(&private_read(&store.root.join("cursor-origin.json"), 1024)?)?;
    ensure!(
        origin.schema_version == 1 && origin.deployment == name,
        "history origin identity"
    );
    let segments = segments(store)?;
    ensure!(
        segments
            .first()
            .is_some_and(|segment| segment.id == origin.first_segment),
        "admission history is incomplete"
    );
    let mut latest = None;
    let mut scanned = 0_u64;
    for segment in segments {
        scanned = scanned.saturating_add(segment.len);
        ensure!(
            scanned <= 4_194_304,
            "admission history exceeds recovery bound"
        );
        for raw in BufReader::new(File::open(&segment.path)?).split(b'\n') {
            let raw = raw?;
            if raw.is_empty() {
                continue;
            }
            let event: Value = serde_json::from_slice(&raw)?;
            if event["source"] == "isos" && event["event_code"] == "deployment_starting" {
                let run = event["run_id"].as_str().context("admission marker run")?;
                ensure!(
                    run.starts_with('r') && token_valid(run, 64),
                    "admission marker run"
                );
                latest = Some(run.to_owned());
            }
        }
    }
    let last_run_id = latest.context("no proven last admitted run")?;
    let state = State {
        schema_version: 1,
        deployment: name.to_owned(),
        manager_session: crate::manager::manager_session_id().to_owned(),
        never_run: false,
        last_run_id: Some(last_run_id),
        manager_anchor: None,
        run_anchor: None,
        run_anchor_state: "unavailable".into(),
        loss_baseline: loss_total(store),
        loss_uncertain: true,
    };
    save_state(store, &state)?;
    Ok(state)
}

fn load_state(store: &LogStore) -> Result<State> {
    let name = deployment(store)?;
    match private_read(&state_path(store), STATE_MAX)
        .and_then(|bytes| serde_json::from_slice::<State>(&bytes).map_err(Into::into))
        .and_then(|state| {
            validate_state(&state, &name)?;
            Ok(state)
        }) {
        Ok(state) => Ok(state),
        Err(_) => recover_state(store, &name),
    }
}

pub(super) fn manager_scope_id(store: &LogStore) -> Option<String> {
    let _guard = store.lock.lock().unwrap();
    load_state(store).ok().map(|state| state.manager_session)
}

pub(super) fn begin_manager_session(store: &LogStore) -> Result<()> {
    let _guard = store.lock.lock().unwrap();
    let Ok(mut state) = load_state(store) else {
        // An upgraded P06 log directory has no proven prior identity. Keep
        // reads closed while allowing this manager to admit a new run.
        return Ok(());
    };
    if state.manager_session != crate::manager::manager_session_id() {
        state.manager_session = crate::manager::manager_session_id().to_owned();
        if state.last_run_id.is_none() {
            state.manager_anchor = None;
            state.loss_baseline = loss_total(store);
        } else if state.run_anchor_state == "pending" {
            // A prior manager published an admitted run but never durably
            // collected its first eligible result. A restart cannot keep
            // such a page pending forever.
            state.run_anchor_state = "unavailable".into();
        }
        if state.last_run_id.is_some() {
            // P06's process counters reset with the manager. Without an
            // accepted durable loss total, a zero after restart is unknown.
            state.loss_uncertain = true;
        }
        save_state(store, &state)?;
    }
    Ok(())
}

pub(super) fn status(store: &LogStore) -> (Option<String>, &'static str) {
    let _guard = store.lock.lock().unwrap();
    match load_state(store) {
        Ok(state) => match state.last_run_id {
            Some(run) => (Some(run), "known"),
            None => (None, "none"),
        },
        Err(_) => (None, "unavailable"),
    }
}

pub(super) fn publish_run(store: &LogStore, run_id: &str) -> Result<()> {
    ensure!(
        run_id.starts_with('r') && token_valid(run_id, 64),
        "invalid deployment run ID"
    );
    let _guard = store.lock.lock().unwrap();
    let mut state = match load_state(store) {
        Ok(state) => state,
        Err(_) => {
            // An older P06 log directory has no P07 never-run marker. It
            // cannot be read as manager scope, but a newly admitted run can
            // establish a durable identity and a new history proof epoch.
            let name = deployment(store)?;
            let origin = Origin {
                schema_version: 1,
                deployment: name.clone(),
                first_segment: identity(&store.root.join("events.log"))?,
            };
            private_write(
                &store.root.join("cursor-origin.json"),
                &serde_json::to_vec(&origin)?,
            )?;
            State {
                schema_version: 1,
                deployment: name,
                manager_session: crate::manager::manager_session_id().to_owned(),
                never_run: false,
                last_run_id: Some(run_id.to_owned()),
                manager_anchor: None,
                run_anchor: None,
                run_anchor_state: "pending".into(),
                loss_baseline: loss_total(store),
                loss_uncertain: false,
            }
        }
    };
    state.never_run = false;
    state.last_run_id = Some(run_id.to_owned());
    state.run_anchor = None;
    state.run_anchor_state = "pending".into();
    state.loss_baseline = loss_total(store);
    #[cfg(test)]
    if state.deployment == "p07-publish-fail" {
        anyhow::bail!("injected cursor-state publication failure");
    }
    let saved = save_state(store, &state);
    #[cfg(test)]
    let saved = if saved.is_ok() && state.deployment == "p07-publish-uncertain" {
        Err(anyhow::anyhow!(
            "injected post-rename publication uncertainty"
        ))
    } else {
        saved
    };
    if saved.is_err()
        && let Ok(bytes) = private_read(&state_path(store), STATE_MAX)
        && let Ok(mut survivor) = serde_json::from_slice::<State>(&bytes)
        && survivor.last_run_id.as_deref() == Some(run_id)
    {
        survivor.run_anchor_state = "unavailable".into();
        let _ = save_state(store, &survivor);
    }
    saved
}

pub(super) fn record_accept(store: &LogStore, event: &Value, apply_failure: bool) -> Result<()> {
    let run_id = event["run_id"].as_str().unwrap_or("");
    let code = event["event_code"].as_str().unwrap_or("");
    let source = event["source"].as_str().unwrap_or("");
    let level = event["level"].as_str().unwrap_or("");
    let ready = matches!(
        code,
        "deployment_ready" | "eos_ready" | "bridge_ready" | "config_ready" | "egia_ready"
    );
    if !(level == "error"
        && (source == "packages"
            || matches!(source, "isos" | "eos" | "bridge" | "config" | "egia")))
        && !ready
        && !apply_failure
    {
        return Ok(());
    }
    let mut state = load_state(store)?;
    let manager_eligible = state.last_run_id.is_none()
        && state.manager_anchor.is_none()
        && run_id == state.manager_session
        && level == "error"
        && ((source == "packages")
            || (source == "isos" && apply_failure && code == "deployment_preflight_failed"));
    let run_eligible = state.last_run_id.as_deref() == Some(run_id)
        && state.run_anchor_state == "pending"
        && ((ready && matches!(source, "isos" | "eos" | "bridge" | "config" | "egia"))
            || (level == "error"
                && matches!(source, "isos" | "eos" | "bridge" | "config" | "egia")
                && code != "legacy_external_event"));
    if !manager_eligible && !run_eligible {
        return Ok(());
    }
    let path = store.root.join("events.log");
    let anchor = Anchor {
        event: event.clone(),
        segment: identity(&path)?,
        after_offset: fs::metadata(&path)?.len(),
    };
    if manager_eligible {
        state.manager_anchor = Some(anchor);
    } else {
        state.run_anchor = Some(anchor);
        state.run_anchor_state = "found".into();
    }
    save_state(store, &state)
}

fn key(store: &LogStore) -> Result<[u8; 32]> {
    let path = store.root.join("cursor-key");
    if !path.exists() {
        let mut random = [0_u8; 32];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(&random)?;
        file.sync_all()?;
        File::open(&store.root)?.sync_all()?;
    }
    let bytes = private_read(&path, 32)?;
    ensure!(bytes.len() == 32, "cursor key length");
    let mut result = [0_u8; 32];
    result.copy_from_slice(&bytes);
    Ok(result)
}

fn mac(key: &[u8; 32], data: &[u8]) -> [u8; 32] {
    let mut inner_pad = [0x36_u8; 64];
    let mut outer_pad = [0x5c_u8; 64];
    for (index, byte) in key.iter().enumerate() {
        inner_pad[index] ^= byte;
        outer_pad[index] ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(data);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner);
    outer.finalize().into()
}

fn encode_token(store: &LogStore, token: &Token) -> Result<String> {
    let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(token)?);
    let signature = URL_SAFE_NO_PAD.encode(mac(&key(store)?, body.as_bytes()));
    let encoded = format!("lc1.{body}.{signature}");
    ensure!(encoded.len() <= 512, "log cursor exceeds bound");
    Ok(encoded)
}

fn decode_token(store: &LogStore, raw: &str) -> Result<Token, CurrentError> {
    if raw.len() > 512 || !raw.is_ascii() {
        return Err(cursor_invalid());
    }
    let parts: Vec<_> = raw.split('.').collect();
    if parts.len() != 3 || parts[0] != "lc1" {
        return Err(cursor_invalid());
    }
    let expected = mac(
        &key(store).map_err(|_| cursor_invalid())?,
        parts[1].as_bytes(),
    );
    let actual = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|_| cursor_invalid())?;
    if actual.len() != 32
        || URL_SAFE_NO_PAD.encode(&actual) != parts[2]
        || actual
            .iter()
            .zip(expected)
            .fold(0_u8, |acc, (a, b)| acc | (a ^ b))
            != 0
    {
        return Err(cursor_invalid());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| cursor_invalid())?;
    if URL_SAFE_NO_PAD.encode(&bytes) != parts[1] {
        return Err(cursor_invalid());
    }
    serde_json::from_slice(&bytes).map_err(|_| cursor_invalid())
}

fn filter_hash(source: &str, level: &str, node: Option<&str>, correlation: Option<&str>) -> String {
    let bytes = serde_json::to_vec(&json!([source, level, node, correlation])).unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

fn gap(code: &str, count: Option<u64>) -> Value {
    json!({"code":code,"count":count})
}

pub struct CurrentQuery<'a> {
    pub source: &'a str,
    pub level: &'a str,
    pub node_path: Option<&'a str>,
    pub correlation_id: Option<&'a str>,
    pub cursor: Option<&'a str>,
    pub limit: usize,
    pub active_revision: Option<&'a str>,
}

pub(super) fn read_current(
    store: &LogStore,
    query: CurrentQuery<'_>,
) -> Result<Value, CurrentError> {
    if query.source != "all" && !source_valid(query.source) {
        return Err(CurrentError::new(
            "log_source_invalid",
            "/query/source",
            "Log source invalid",
            400,
        ));
    }
    let rank = level_rank(query.level).ok_or_else(|| {
        CurrentError::new(
            "log_level_invalid",
            "/query/level",
            "Log level invalid",
            400,
        )
    })?;
    if !(1..=200).contains(&query.limit) {
        return Err(CurrentError::new(
            "log_limit_invalid",
            "/query/limit",
            "Log limit invalid",
            400,
        ));
    }
    if let Some(path) = query.node_path
        && !["isos", "eos", "egia"]
            .into_iter()
            .any(|core| event_policy::node_pointer(path, core))
    {
        return Err(CurrentError::new(
            "log_node_path_invalid",
            "/query/node_path",
            "Log node path invalid",
            400,
        ));
    }
    if let Some(id) = query.correlation_id
        && !token_valid(id, 64)
    {
        return Err(CurrentError::new(
            "log_correlation_id_invalid",
            "/query/correlation_id",
            "Log correlation ID invalid",
            400,
        ));
    }
    let filters = filter_hash(
        query.source,
        query.level,
        query.node_path,
        query.correlation_id,
    );
    let _guard = store.lock.lock().unwrap();
    let state = load_state(store).map_err(|_| scope_error())?;
    let (kind, run_id, anchor_kind, anchor) = if let Some(run) = state.last_run_id.as_deref() {
        (
            "deployment_run",
            run,
            "first_ready_or_failure",
            state.run_anchor.as_ref(),
        )
    } else {
        (
            "manager_session",
            state.manager_session.as_str(),
            "first_prelaunch_failure",
            state.manager_anchor.as_ref(),
        )
    };
    let name = deployment(store).map_err(|_| scope_error())?;
    let segments = segments(store).map_err(|_| scope_error())?;
    let current = segments.last().ok_or_else(scope_error)?;
    let mut run_changed = false;
    let mut expired = false;
    let mut rotated = false;
    let mut old = None;
    if let Some(raw) = query.cursor {
        let parsed = decode_token(store, raw)?;
        if parsed.d != name {
            return Err(cursor_invalid());
        }
        if parsed.f != filters {
            return Err(CurrentError::new(
                "log_cursor_filter_mismatch",
                "/query/cursor",
                "Log cursor filters changed",
                400,
            ));
        }
        if parsed.k != kind || parsed.r != run_id {
            run_changed = true;
        } else {
            old = Some(parsed);
        }
    }
    let loss_now = loss_total(store);
    let gap_count = if state.loss_uncertain {
        None
    } else {
        old.as_ref().map_or_else(
            || loss_now.checked_sub(state.loss_baseline),
            |token| loss_now.checked_sub(token.l),
        )
    };
    let collector_gap = gap_count.is_none() || gap_count.is_some_and(|count| count > 0);
    let mut anchor_state = if anchor.is_some() {
        "found"
    } else if state.last_run_id.is_some() {
        state.run_anchor_state.as_str()
    } else {
        "pending"
    };
    if anchor_state == "pending" && collector_gap {
        anchor_state = "unavailable";
    }
    let mut lines = Vec::new();
    let mut position = (current.id.clone(), current.len);
    if anchor_state == "found" {
        let pinned = anchor.expect("found anchor exists");
        let (start_segment, start_offset) =
            if let Some(token) = old.as_ref().filter(|token| !token.p) {
                (token.s.as_str(), token.o)
            } else {
                (pinned.segment.as_str(), pinned.after_offset)
            };
        let (start_index, mut offset) = if let Some(index) = segments
            .iter()
            .position(|segment| segment.id == start_segment && start_offset <= segment.len)
        {
            (index, start_offset)
        } else {
            if old.is_some() {
                expired = true;
                rotated = true;
            }
            (0, 0)
        };
        let mut remaining = SCAN_MAX;
        position = (segments[start_index].id.clone(), offset);
        for (index, segment) in segments.iter().enumerate().skip(start_index) {
            if index > start_index {
                offset = 0;
                rotated = true;
            }
            if remaining == 0 || lines.len() >= query.limit {
                break;
            }
            let mut file = File::open(&segment.path).map_err(|_| scope_error())?;
            file.seek(SeekFrom::Start(offset))
                .map_err(|_| scope_error())?;
            let mut reader = BufReader::new(file.take(remaining as u64));
            loop {
                if remaining == 0 || lines.len() >= query.limit {
                    break;
                }
                let mut raw = Vec::new();
                let read = reader
                    .read_until(b'\n', &mut raw)
                    .map_err(|_| scope_error())?;
                if read == 0 {
                    break;
                }
                remaining -= read;
                if raw.last() != Some(&b'\n') {
                    break;
                }
                offset += read as u64;
                position = (segment.id.clone(), offset);
                let Ok(value) = serde_json::from_slice::<Value>(&raw) else {
                    continue;
                };
                if value["run_id"].as_str() != Some(run_id) {
                    continue;
                }
                if query.source != "all" && value["source"].as_str() != Some(query.source) {
                    continue;
                }
                if !value["level"]
                    .as_str()
                    .and_then(level_rank)
                    .is_some_and(|level| level >= rank)
                {
                    continue;
                }
                if query.node_path.is_some() && value["node_path"].as_str() != query.node_path {
                    continue;
                }
                if query.correlation_id.is_some()
                    && value["correlation_id"].as_str() != query.correlation_id
                {
                    continue;
                }
                lines.push(value);
            }
        }
    } else if anchor_state == "pending" {
        position = (current.id.clone(), current.len);
    }
    let token = Token {
        d: name,
        k: kind.into(),
        r: run_id.into(),
        f: filters,
        s: position.0,
        o: position.1,
        p: anchor_state == "pending",
        l: loss_now,
    };
    let next_cursor = encode_token(store, &token).map_err(|_| scope_error())?;
    let mut diagnostics = Vec::new();
    if run_changed {
        diagnostics.push(gap("log_run_changed", None));
    }
    if expired {
        diagnostics.push(gap("log_cursor_expired", None));
    }
    if collector_gap {
        diagnostics.push(gap("log_collector_gap", gap_count));
    }
    if anchor_state == "unavailable" {
        diagnostics.push(gap("log_anchor_unavailable", None));
    }
    Ok(json!({
        "scope":{"kind":kind,"run_id":run_id,"active_revision":query.active_revision},
        "anchor":{"kind":anchor_kind,"state":anchor_state,"event":anchor.map(|pinned| &pinned.event)},
        "lines":lines,
        "next_cursor":next_cursor,
        "rotated":rotated,
        "diagnostics":diagnostics,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Paths, log_store::LogContext};
    use std::os::unix::fs::PermissionsExt;

    fn store(name: &str) -> (tempfile::TempDir, LogStore) {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let store = LogStore::new(&paths, name).unwrap();
        (temp, store)
    }

    fn page(store: &LogStore, cursor: Option<&str>, source: &str) -> Value {
        store
            .read_current(CurrentQuery {
                source,
                level: "info",
                node_path: None,
                correlation_id: None,
                cursor,
                limit: 100,
                active_revision: None,
            })
            .unwrap()
    }

    fn append(store: &LogStore, run: &str, code: &str, level: &str) {
        store
            .append_event_context(
                "isos",
                level,
                code,
                "test",
                LogContext {
                    run_id: Some(run),
                    correlation_id: None,
                    node_path: None,
                    fields: json!({}),
                },
            )
            .unwrap();
    }

    #[test]
    fn manager_anchor_distinguishes_validate_from_apply_and_bypasses_filters() {
        let (_temp, store) = store("p07-manager-anchor");
        let manager = crate::manager::manager_session_id();
        store
            .append_event_context(
                "isos",
                "error",
                "deployment_preflight_failed",
                "validate",
                LogContext {
                    run_id: Some(manager),
                    correlation_id: None,
                    node_path: None,
                    fields: json!({}),
                },
            )
            .unwrap();
        assert_eq!(page(&store, None, "eos")["anchor"]["state"], "pending");
        store
            .append_apply_failure_context(
                "isos",
                "error",
                "deployment_preflight_failed",
                "apply",
                LogContext {
                    run_id: Some(manager),
                    correlation_id: None,
                    node_path: None,
                    fields: json!({}),
                },
            )
            .unwrap();
        let current = page(&store, None, "eos");
        assert_eq!(current["scope"]["kind"], "manager_session");
        assert_eq!(current["anchor"]["event"]["message"], "apply");
        assert_eq!(current["lines"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn cursor_identity_and_key_are_private_files() {
        let (_temp, store) = store("p07-private");
        for name in ["cursor-state.json", "cursor-origin.json", "cursor-key"] {
            let metadata = fs::symlink_metadata(store.root.join(name)).unwrap();
            assert!(metadata.file_type().is_file());
            assert_eq!(metadata.permissions().mode() & 0o077, 0);
        }
        assert_eq!(
            fs::metadata(&store.root).unwrap().permissions().mode() & 0o077,
            0
        );
    }

    #[test]
    fn run_anchor_is_first_accepted_event_and_same_code_later_is_a_line() {
        let (_temp, store) = store("p07-run-anchor");
        let run = "r101";
        store.publish_run(run).unwrap();
        assert_eq!(page(&store, None, "all")["anchor"]["state"], "pending");
        append(&store, run, "deployment_ready", "info");
        append(&store, run, "deployment_ready", "info");
        let current = page(&store, None, "all");
        assert_eq!(current["anchor"]["event"]["event_code"], "deployment_ready");
        assert_eq!(current["lines"].as_array().unwrap().len(), 1);
        assert_eq!(current["lines"][0]["event_code"], "deployment_ready");
        let next = current["next_cursor"].as_str().unwrap();
        assert_eq!(
            page(&store, Some(next), "all")["lines"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn stopped_run_keeps_identity_anchor_and_terminal_line() {
        let (_temp, store) = store("p07-stopped");
        let run = "r115";
        store.publish_run(run).unwrap();
        append(&store, run, "deployment_ready", "info");
        append(&store, run, "deployment_stopped", "info");
        assert_eq!(store.run_identity(), (Some(run.into()), "known"));
        let current = page(&store, None, "all");
        assert_eq!(current["anchor"]["event"]["event_code"], "deployment_ready");
        assert_eq!(current["lines"][0]["event_code"], "deployment_stopped");
    }

    #[test]
    fn cursor_authentication_filter_binding_and_run_change() {
        let (_temp, store) = store("p07-token");
        store.publish_run("r201").unwrap();
        append(&store, "r201", "deployment_ready", "info");
        let token = page(&store, None, "all")["next_cursor"]
            .as_str()
            .unwrap()
            .to_owned();
        let invalid = store
            .read_current(CurrentQuery {
                source: "all",
                level: "info",
                node_path: None,
                correlation_id: None,
                cursor: Some("123:456:789"),
                limit: 100,
                active_revision: None,
            })
            .unwrap_err();
        assert_eq!(invalid.code, "log_cursor_invalid");
        let mismatch = store
            .read_current(CurrentQuery {
                source: "eos",
                level: "info",
                node_path: None,
                correlation_id: None,
                cursor: Some(&token),
                limit: 100,
                active_revision: None,
            })
            .unwrap_err();
        assert_eq!(mismatch.code, "log_cursor_filter_mismatch");
        let mut tampered = token.clone().into_bytes();
        let signature_start = token.rfind('.').unwrap() + 1;
        tampered[signature_start] = if tampered[signature_start] == b'A' {
            b'B'
        } else {
            b'A'
        };
        let tampered = String::from_utf8(tampered).unwrap();
        assert_eq!(
            store
                .read_current(CurrentQuery {
                    source: "all",
                    level: "info",
                    node_path: None,
                    correlation_id: None,
                    cursor: Some(&tampered),
                    limit: 100,
                    active_revision: None,
                })
                .unwrap_err()
                .code,
            "log_cursor_invalid"
        );
        store.publish_run("r202").unwrap();
        let changed = page(&store, Some(&token), "all");
        assert_eq!(changed["scope"]["run_id"], "r202");
        assert_eq!(changed["diagnostics"][0]["code"], "log_run_changed");
    }

    #[test]
    fn durable_two_run_recovery_and_missing_history_fail_closed() {
        let (_temp, store) = store("p07-recovery");
        for run in ["r301", "r302"] {
            store.publish_run(run).unwrap();
            append(&store, run, "deployment_starting", "info");
            append(&store, run, "deployment_ready", "info");
        }
        fs::write(state_path(&store), b"broken").unwrap();
        assert_eq!(store.run_identity(), (Some("r302".into()), "known"));
        let recovered = page(&store, None, "all");
        assert_eq!(recovered["anchor"]["state"], "unavailable");
        assert_eq!(recovered["diagnostics"][0]["code"], "log_collector_gap");
        fs::write(state_path(&store), b"broken").unwrap();
        fs::remove_file(store.root.join("cursor-origin.json")).unwrap();
        assert_eq!(store.run_identity(), (None, "unavailable"));
        assert_eq!(
            store
                .read_current(CurrentQuery {
                    source: "all",
                    level: "info",
                    node_path: None,
                    correlation_id: None,
                    cursor: None,
                    limit: 100,
                    active_revision: None,
                })
                .unwrap_err()
                .code,
            "log_scope_unavailable"
        );
    }

    #[test]
    fn publication_failure_keeps_never_run_scope() {
        let (_temp, store) = store("p07-publish-fail");
        assert!(store.publish_run("r401").is_err());
        assert_eq!(store.run_identity(), (None, "none"));
    }

    #[test]
    fn older_log_directory_is_unavailable_until_a_new_run_is_published() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let root = paths.data.join("logs").join("p07-upgrade");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("events.log"), b"").unwrap();
        let store = LogStore::new(&paths, "p07-upgrade").unwrap();
        assert_eq!(store.run_identity(), (None, "unavailable"));
        store.publish_run("r451").unwrap();
        assert_eq!(store.run_identity(), (Some("r451".into()), "known"));
        assert_eq!(page(&store, None, "all")["anchor"]["state"], "pending");
    }

    #[test]
    fn uncertain_publication_keeps_surviving_new_run_and_unavailable_anchor() {
        let (_temp, store) = store("p07-publish-uncertain");
        assert!(store.publish_run("r475").is_err());
        assert_eq!(store.run_identity(), (Some("r475".into()), "known"));
        let current = page(&store, None, "all");
        assert_eq!(current["anchor"]["state"], "unavailable");
        assert_eq!(current["diagnostics"][0]["code"], "log_anchor_unavailable");
    }

    #[test]
    fn manager_restart_after_publication_does_not_leave_anchor_pending() {
        let (_temp, store) = store("p07-restart-pending");
        store.publish_run("r476").unwrap();
        let mut state: State =
            serde_json::from_slice(&private_read(&state_path(&store), STATE_MAX).unwrap()).unwrap();
        state.manager_session = "m-old-manager".into();
        save_state(&store, &state).unwrap();
        begin_manager_session(&store).unwrap();
        let current = page(&store, None, "all");
        assert_eq!(current["scope"]["run_id"], "r476");
        assert_eq!(current["anchor"]["state"], "unavailable");
        assert_eq!(current["diagnostics"][0]["code"], "log_collector_gap");
        assert_eq!(current["diagnostics"][1]["code"], "log_anchor_unavailable");
    }

    #[test]
    fn anchor_write_failure_reports_gap_and_does_not_invent_ready() {
        let (_temp, store) = store("p07-anchor-fail");
        store.publish_run("r501").unwrap();
        append(&store, "r501", "deployment_ready", "info");
        let current = page(&store, None, "all");
        assert_eq!(current["anchor"]["state"], "unavailable");
        assert!(current["anchor"]["event"].is_null());
        assert_eq!(current["lines"].as_array().unwrap().len(), 0);
        assert_eq!(current["diagnostics"][0]["code"], "log_collector_gap");
        assert_eq!(current["diagnostics"][1]["code"], "log_anchor_unavailable");
    }

    #[test]
    fn retained_rotation_continues_and_expired_position_reports_gap() {
        let (_temp, mut store) = store("p07-rotation");
        store.max_bytes = 65_536;
        store.backups = 1;
        let run = "r601";
        store.publish_run(run).unwrap();
        append(&store, run, "deployment_ready", "info");
        let first = page(&store, None, "all");
        let old = first["next_cursor"].as_str().unwrap().to_owned();
        for _ in 0..17 {
            store
                .append_event_context(
                    "isos",
                    "info",
                    "deployment_starting",
                    &"x".repeat(5_000),
                    LogContext {
                        run_id: Some(run),
                        correlation_id: None,
                        node_path: None,
                        fields: json!({}),
                    },
                )
                .unwrap();
        }
        let retained = page(&store, Some(&old), "all");
        assert_eq!(retained["rotated"], true);
        assert!(!retained["lines"].as_array().unwrap().is_empty());
        assert!(retained["lines"].as_array().unwrap().len() < 17);
        assert!(
            !retained["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["code"] == "log_cursor_expired")
        );
        for _ in 0..32 {
            store
                .append_event_context(
                    "isos",
                    "info",
                    "deployment_starting",
                    &"y".repeat(5_000),
                    LogContext {
                        run_id: Some(run),
                        correlation_id: None,
                        node_path: None,
                        fields: json!({}),
                    },
                )
                .unwrap();
        }
        let expired = page(&store, Some(&old), "all");
        assert_eq!(expired["rotated"], true);
        assert_eq!(expired["diagnostics"][0]["code"], "log_cursor_expired");
    }

    #[test]
    fn filtered_empty_page_advances_after_anchor_within_scan_bound() {
        let (_temp, store) = store("p07-filter-progress");
        let run = "r701";
        store.publish_run(run).unwrap();
        append(&store, run, "deployment_ready", "info");
        for _ in 0..20 {
            store
                .append_event_context(
                    "isos",
                    "info",
                    "deployment_starting",
                    &"z".repeat(5_000),
                    LogContext {
                        run_id: Some(run),
                        correlation_id: None,
                        node_path: None,
                        fields: json!({}),
                    },
                )
                .unwrap();
        }
        let first = page(&store, None, "eos");
        assert_eq!(first["anchor"]["event"]["event_code"], "deployment_ready");
        assert!(first["lines"].as_array().unwrap().is_empty());
        let first_token = first["next_cursor"].as_str().unwrap();
        let second = page(&store, Some(first_token), "eos");
        assert!(second["lines"].as_array().unwrap().is_empty());
        assert_ne!(second["next_cursor"].as_str().unwrap(), first_token);
    }
}
