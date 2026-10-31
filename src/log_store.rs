//! Bounded, owner-only per-deployment logs and cursor reads.

use crate::config::Paths;
use anyhow::{Context, Result, ensure};
use chrono::{SecondsFormat, Utc};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

const DEFAULT_MAX_BYTES: u64 = 1_048_576;
const DEFAULT_BACKUPS: u8 = 3;
const MAX_READ_BYTES: u64 = 65_536;

#[derive(Clone)]
pub struct LogStore {
    root: PathBuf,
    max_bytes: u64,
    backups: u8,
    level: u8,
    lock: Arc<Mutex<()>>,
}

fn source_valid(source: &str) -> bool {
    !source.is_empty()
        && source.len() <= 48
        && source
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn valid_source(source: &str) -> bool {
    source_valid(source) && !matches!(source, "all" | "services" | "events")
}

fn shared_lock() -> Arc<Mutex<()>> {
    static LOCK: OnceLock<Arc<Mutex<()>>> = OnceLock::new();
    LOCK.get_or_init(|| Arc::new(Mutex::new(()))).clone()
}

pub fn level_rank(level: &str) -> Option<u8> {
    match level {
        "trace" => Some(0),
        "debug" => Some(1),
        "info" => Some(2),
        "warn" => Some(3),
        "error" => Some(4),
        _ => None,
    }
}

fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

impl LogStore {
    pub fn new(paths: &Paths, deployment: &str) -> Result<Self> {
        ensure!(source_valid(deployment), "invalid deployment for logs");
        let max_bytes = std::env::var("ISOS_LOG_MAX_BYTES")
            .ok()
            .map(|v| v.parse())
            .transpose()?
            .unwrap_or(DEFAULT_MAX_BYTES);
        let backups = std::env::var("ISOS_LOG_BACKUPS")
            .ok()
            .map(|v| v.parse())
            .transpose()?
            .unwrap_or(DEFAULT_BACKUPS);
        let level = std::env::var("ISOS_LOG_LEVEL").unwrap_or_else(|_| "info".into());
        let level = level_rank(&level)
            .context("ISOS_LOG_LEVEL must be trace, debug, info, warn, or error")?;
        ensure!(
            (65_536..=100_000_000).contains(&max_bytes),
            "ISOS_LOG_MAX_BYTES must be 65536..100000000"
        );
        ensure!(
            (1..=10).contains(&backups),
            "ISOS_LOG_BACKUPS must be 1..10"
        );
        let root = paths.data.join("logs").join(deployment);
        private_dir(&root)?;
        for source in [
            "isos", "packages", "graph", "ui", "eos", "egia", "mujoco", "bridge", "config",
        ] {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(root.join(format!("{source}.log")))?;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self {
            root,
            max_bytes,
            backups,
            level,
            lock: shared_lock(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, source: &str) -> Result<PathBuf> {
        ensure!(source_valid(source), "invalid log source");
        Ok(self.root.join(format!(
            "{}.log",
            if matches!(source, "all" | "services") {
                "events"
            } else {
                source
            }
        )))
    }

    fn append_file(&self, path: &Path, line: &[u8]) -> Result<()> {
        let old_len = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if old_len.saturating_add(line.len() as u64) > self.max_bytes {
            for index in (1..=self.backups).rev() {
                let dest = path.with_extension(format!("log.{index}"));
                let src = if index == 1 {
                    path.to_path_buf()
                } else {
                    path.with_extension(format!("log.{}", index - 1))
                };
                if dest.exists() {
                    fs::remove_file(&dest)?;
                }
                if src.exists() {
                    fs::rename(src, dest)?;
                }
            }
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(line)?;
        Ok(())
    }

    pub fn append(&self, source: &str, level: &str, message: &str) -> Result<()> {
        self.append_event(source, level, "message", message)
    }

    pub fn append_event(
        &self,
        source: &str,
        level: &str,
        event_code: &str,
        message: &str,
    ) -> Result<()> {
        ensure!(valid_source(source), "invalid log source");
        ensure!(source_valid(event_code), "invalid log event code");
        let rank = level_rank(level).context("invalid log level")?;
        if rank < self.level {
            return Ok(());
        }
        let _guard = self.lock.lock().unwrap();
        let path = self.path(source)?;
        let message = if message.len() > 8192 {
            &message[..message.floor_char_boundary(8192)]
        } else {
            message
        };
        let mut line = serde_json::to_vec(
            &json!({"timestamp":Utc::now().to_rfc3339_opts(SecondsFormat::Millis,true),"level":level,"source":source,"event_code":event_code,"message":message,"pid":std::process::id()}),
        )?;
        line.push(b'\n');
        self.append_file(&path, &line)?;
        if source != "events" {
            self.append_file(&self.root.join("events.log"), &line)?;
        }
        Ok(())
    }

    pub fn read(&self, source: &str, cursor: Option<&str>, limit: usize) -> Result<Value> {
        self.read_filtered(source, cursor, limit, None)
    }

    pub fn read_filtered(
        &self,
        source: &str,
        cursor: Option<&str>,
        limit: usize,
        min_level: Option<&str>,
    ) -> Result<Value> {
        ensure!((1..=200).contains(&limit), "log limit must be 1..200");
        let min_rank = min_level
            .map(|level| level_rank(level).context("invalid log level"))
            .transpose()?;
        let path = self.path(source)?;
        let mut file =
            File::open(&path).with_context(|| format!("log source {source} is unavailable"))?;
        let meta = file.metadata()?;
        // Inodes can be reused during rapid rotation. Birth time is stable
        // while a file is appended and changes when a replacement is created.
        let birth_ns = meta
            .created()
            .ok()
            .and_then(|created| created.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_nanos());
        let identity = format!("{:x}-{:x}-{birth_ns:x}", meta.dev(), meta.ino());
        let mut offset = 0_u64;
        let mut rotated = false;
        if let Some(cursor) = cursor {
            let (id, position) = cursor.rsplit_once(':').context("invalid log cursor")?;
            let position = position
                .parse::<u64>()
                .context("invalid log cursor offset")?;
            if id == identity && position <= meta.len() {
                offset = position;
            } else {
                rotated = true;
            }
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut reader = BufReader::new(file.take(MAX_READ_BYTES));
        let mut lines = Vec::new();
        let mut consumed = 0_u64;
        while lines.len() < limit {
            let mut raw = Vec::new();
            let size = reader.read_until(b'\n', &mut raw)?;
            if size == 0 {
                break;
            }
            if raw.last() != Some(&b'\n') && raw.len() == 8193 {
                break;
            }
            consumed += size as u64;
            if let Ok(value) = serde_json::from_slice::<Value>(&raw)
                && (source != "services"
                    || value["source"]
                        .as_str()
                        .is_some_and(|name| name.starts_with("service-")))
                && min_rank.is_none_or(|rank| {
                    value["level"]
                        .as_str()
                        .and_then(level_rank)
                        .is_some_and(|level| level >= rank)
                })
            {
                lines.push(value);
            }
        }
        let next_cursor = format!("{identity}:{}", offset + consumed);
        Ok(json!({"lines":lines,"next_cursor":next_cursor,"rotated":rotated}))
    }

    pub fn tail_filtered(
        &self,
        source: &str,
        limit: usize,
        min_level: Option<&str>,
    ) -> Result<Vec<Value>> {
        ensure!((1..=1000).contains(&limit), "log limit must be 1..1000");
        let min_rank = min_level
            .map(|level| level_rank(level).context("invalid log level"))
            .transpose()?;
        let path = self.path(source)?;
        let file =
            File::open(&path).with_context(|| format!("log source {source} is unavailable"))?;
        let mut lines = VecDeque::with_capacity(limit);
        for raw in BufReader::new(file).split(b'\n') {
            let raw = raw?;
            if let Ok(value) = serde_json::from_slice::<Value>(&raw)
                && (source != "services"
                    || value["source"]
                        .as_str()
                        .is_some_and(|name| name.starts_with("service-")))
                && min_rank.is_none_or(|rank| {
                    value["level"]
                        .as_str()
                        .and_then(level_rank)
                        .is_some_and(|level| level >= rank)
                })
            {
                if lines.len() == limit {
                    lines.pop_front();
                }
                lines.push_back(value);
            }
        }
        Ok(lines.into_iter().collect())
    }
}

pub fn capture<R: std::io::Read + Send + 'static>(
    reader: R,
    store: LogStore,
    source: String,
    _level: &'static str,
) {
    std::thread::spawn(move || capture_stream(reader, store, source));
}

fn capture_stream<R: std::io::Read>(reader: R, store: LogStore, source: String) {
    let mut reader = BufReader::new(reader);
    loop {
        let mut raw = Vec::new();
        // Bound a child that writes a very long line without a newline.
        match Read::by_ref(&mut reader)
            .take(8193)
            .read_until(b'\n', &mut raw)
        {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if raw.last() != Some(&b'\n') {
                    // Drain the rest of an oversized line without buffering it.
                    loop {
                        let mut discard = Vec::new();
                        match Read::by_ref(&mut reader)
                            .take(8193)
                            .read_until(b'\n', &mut discard)
                        {
                            Ok(0) | Err(_) => break,
                            Ok(_) if discard.last() == Some(&b'\n') => break,
                            Ok(_) => {}
                        }
                    }
                    continue;
                }
                let Ok(message) = std::str::from_utf8(&raw) else {
                    continue;
                };
                if let Ok(record) = serde_json::from_str::<Value>(message)
                    && let (Some(severity), Some(body)) = (
                        record.get("level").and_then(Value::as_str),
                        record.get("message").and_then(Value::as_str),
                    )
                    && level_rank(severity).is_some()
                {
                    let event_code = record
                        .get("event_code")
                        .and_then(Value::as_str)
                        .filter(|code| source_valid(code))
                        .unwrap_or("message");
                    let _ = store.append_event(&source, severity, event_code, body);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn store() -> (tempfile::TempDir, LogStore) {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            runtime: temp.path().join("run"),
        };
        let store = LogStore::new(&paths, "default").unwrap();
        (temp, store)
    }

    #[test]
    fn cursor_is_bounded_and_reports_rotation() {
        let (_temp, mut store) = store();
        store.max_bytes = 500;
        store.append("eos", "info", "first").unwrap();
        store.append("eos", "error", "second").unwrap();
        let first = store.read("eos", None, 1).unwrap();
        assert_eq!(first["lines"].as_array().unwrap().len(), 1);
        assert_eq!(first["lines"][0]["message"], "first");
        let second = store.read("eos", first["next_cursor"].as_str(), 1).unwrap();
        assert_eq!(second["lines"][0]["message"], "second");
        for _ in 0..10 {
            store.append("eos", "info", &"x".repeat(150)).unwrap();
        }
        let rotated = store
            .read("eos", first["next_cursor"].as_str(), 10)
            .unwrap();
        assert_eq!(rotated["rotated"], true);
    }

    #[test]
    fn logs_are_private_and_sources_are_checked() {
        let (_temp, store) = store();
        store.append("config", "error", "failure").unwrap();
        assert_eq!(
            fs::metadata(&store.root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(store.root.join("config.log"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            store.read("config", None, 10).unwrap()["lines"][0]["message"],
            "failure"
        );
        assert!(store.read("../secret", None, 1).is_err());
        assert!(store.read("config", None, 201).is_err());
        assert!(store.read("config", Some("bad"), 1).is_err());
        assert!(
            !store
                .root
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("process-logs")
                .exists()
        );
    }

    #[test]
    fn severity_threshold_filters_child_events() {
        let (_temp, mut store) = store();
        store.level = level_rank("warn").unwrap();
        store.append("eos", "info", "routine stdout").unwrap();
        store.append("eos", "error", "important stderr").unwrap();
        let lines = store.read("eos", None, 10).unwrap();
        assert_eq!(lines["lines"].as_array().unwrap().len(), 1);
        assert_eq!(lines["lines"][0]["message"], "important stderr");
        assert!(store.append("isos", "verbose", "unknown").is_err());
        assert!(store.append("all", "warn", "bad alias").is_err());
    }

    #[test]
    fn unified_stream_keeps_isos_and_service_events() {
        let (_temp, store) = store();
        store
            .append_event("isos", "warn", "admission_failed", "stale receipt")
            .unwrap();
        store
            .append_event("service-model", "error", "child_exited", "exit 1")
            .unwrap();
        let all = store.read("all", None, 10).unwrap();
        assert_eq!(all["lines"].as_array().unwrap().len(), 2);
        assert_eq!(all["lines"][0]["event_code"], "admission_failed");
        assert_eq!(all["lines"][1]["source"], "service-model");
        assert!(all["lines"][0]["pid"].is_number());
        let services = store.read("services", None, 10).unwrap();
        assert_eq!(services["lines"].as_array().unwrap().len(), 1);
        assert_eq!(services["lines"][0]["event_code"], "child_exited");
    }

    #[test]
    fn capture_only_records_selected_structured_events() {
        let (_temp, store) = store();
        let mut input = b"routine progress\n{\"level\":\"warn\",\"message\":\"selected\",\"source\":\"forged\",\"event_code\":\"child_warning\"}\n".to_vec();
        input.extend_from_slice(&[b'x'; 9000]);
        input.extend_from_slice(b"\n{\"level\":\"error\",\"message\":\"second\"}\n");
        input.extend_from_slice(&[0xff, b'\n']);
        capture_stream(&input[..], store.clone(), "eos".into());
        let lines = store.read("eos", None, 10).unwrap();
        assert_eq!(lines["lines"].as_array().unwrap().len(), 2);
        assert_eq!(lines["lines"][0]["source"], "eos");
        assert_eq!(lines["lines"][0]["event_code"], "child_warning");
        assert_eq!(lines["lines"][1]["message"], "second");
    }

    #[test]
    fn filtered_reads_and_tail_keep_only_matching_severity() {
        let (_temp, store) = store();
        for (level, message) in [
            ("info", "one"),
            ("warn", "two"),
            ("info", "three"),
            ("error", "four"),
        ] {
            store.append("eos", level, message).unwrap();
        }
        let filtered = store.read_filtered("eos", None, 2, Some("warn")).unwrap();
        assert_eq!(filtered["lines"].as_array().unwrap().len(), 2);
        assert_eq!(filtered["lines"][0]["message"], "two");
        assert_eq!(filtered["lines"][1]["message"], "four");
        let tail = store.tail_filtered("eos", 1, Some("warn")).unwrap();
        assert_eq!(tail[0]["message"], "four");
        assert!(
            store
                .read_filtered("eos", None, 1, Some("verbose"))
                .is_err()
        );
        assert!(
            store
                .append_event("eos", "trace", "../bad", "invalid")
                .is_err()
        );
    }
}
