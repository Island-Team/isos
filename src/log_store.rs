//! Bounded, owner-only per-deployment logs and cursor reads.

use crate::config::Paths;
use anyhow::{Context, Result, ensure};
use chrono::{SecondsFormat, Utc};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const DEFAULT_MAX_BYTES: u64 = 1_048_576;
const DEFAULT_BACKUPS: u8 = 3;
const MAX_READ_BYTES: u64 = 65_536;

#[derive(Clone)]
pub struct LogStore {
    root: PathBuf,
    legacy: PathBuf,
    max_bytes: u64,
    backups: u8,
    lock: Arc<Mutex<()>>,
}

fn source_valid(source: &str) -> bool {
    !source.is_empty()
        && source.len() <= 48
        && source
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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
        ensure!(
            (65_536..=100_000_000).contains(&max_bytes),
            "ISOS_LOG_MAX_BYTES must be 65536..100000000"
        );
        ensure!(
            (1..=10).contains(&backups),
            "ISOS_LOG_BACKUPS must be 1..10"
        );
        let root = paths.data.join("logs").join(deployment);
        let legacy = paths.data.join("process-logs").join(deployment);
        private_dir(&root)?;
        private_dir(&legacy)?;
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
            legacy,
            max_bytes,
            backups,
            lock: Arc::new(Mutex::new(())),
        })
    }

    fn path(&self, source: &str) -> Result<PathBuf> {
        ensure!(source_valid(source), "invalid log source");
        Ok(self.root.join(format!("{source}.log")))
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
        let _guard = self.lock.lock().unwrap();
        let path = self.path(source)?;
        let message = if message.len() > 8192 {
            &message[..message.floor_char_boundary(8192)]
        } else {
            message
        };
        let mut line = serde_json::to_vec(
            &json!({"timestamp":Utc::now().to_rfc3339_opts(SecondsFormat::Millis,true),"level":level,"source":source,"message":message}),
        )?;
        line.push(b'\n');
        self.append_file(&path, &line)
    }

    pub fn append_child(&self, source: &str, level: &str, message: &str) -> Result<()> {
        self.append(source, level, message)?;
        let _guard = self.lock.lock().unwrap();
        let path = self.legacy.join(format!("{source}.log"));
        let mut line = message.as_bytes().to_vec();
        if line.len() > 8192 {
            line.truncate(8192);
        }
        line.push(b'\n');
        self.append_file(&path, &line)
    }

    pub fn read(&self, source: &str, cursor: Option<&str>, limit: usize) -> Result<Value> {
        ensure!((1..=200).contains(&limit), "log limit must be 1..200");
        let path = self.path(source)?;
        let mut file =
            File::open(&path).with_context(|| format!("log source {source} is unavailable"))?;
        let meta = file.metadata()?;
        let identity = format!("{:x}-{:x}", meta.dev(), meta.ino());
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
            if raw.last() != Some(&b'\n') {
                break;
            }
            consumed += size as u64;
            if let Ok(value) = serde_json::from_slice::<Value>(&raw) {
                lines.push(value);
            }
        }
        let next_cursor = format!("{identity}:{}", offset + consumed);
        Ok(json!({"lines":lines,"next_cursor":next_cursor,"rotated":rotated}))
    }
}

pub fn capture<R: std::io::Read + Send + 'static>(
    reader: R,
    store: LogStore,
    source: String,
    level: &'static str,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        loop {
            let mut raw = Vec::new();
            // Bound a child that writes a very long line without a newline.
            match Read::by_ref(&mut reader)
                .take(8192)
                .read_until(b'\n', &mut raw)
            {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let message = String::from_utf8_lossy(&raw);
                    let _ =
                        store.append_child(&source, level, message.trim_end_matches(['\r', '\n']));
                }
            }
        }
    });
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
        store.append_child("config", "error", "failure").unwrap();
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
        assert_eq!(
            fs::read_to_string(store.legacy.join("config.log")).unwrap(),
            "failure\n"
        );
    }
}
