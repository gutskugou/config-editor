use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::core::secure;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub path: PathBuf,
    pub original_path: String,
    pub content_path: PathBuf,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    pub hash: String,
}

fn snapshots_root(state_root: &Path) -> PathBuf {
    state_root.join("config-editor/snapshots")
}

pub fn create_snapshot(
    state_root: &Path,
    original_path: &str,
    content: &[u8],
) -> Result<Snapshot, String> {
    let root = snapshots_root(state_root);
    secure::secure_dir(&root)?;
    let now = Utc::now();
    let name = format!(
        "{}-{}",
        now.format("%Y%m%dT%H%M%S%.9fZ"),
        &secure::digest(original_path.as_bytes())[..10]
    );
    let dir = root.join(name);
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    let complete = std::cell::Cell::new(false);
    let cleanup = || {
        if !complete.get() {
            let _ = fs::remove_dir_all(&dir);
        }
    };
    let content_path = dir.join("content");
    fs::write(&content_path, content).map_err(|e| {
        cleanup();
        e.to_string()
    })?;
    fs::set_permissions(&content_path, fs::Permissions::from_mode(0o600)).map_err(|e| {
        cleanup();
        e.to_string()
    })?;
    let snapshot = Snapshot {
        path: dir.clone(),
        original_path: original_path.to_string(),
        content_path,
        created_at: now,
        hash: secure::digest(content),
    };
    let meta = serde_json::to_vec_pretty(&snapshot).map_err(|e| e.to_string())?;
    fs::write(dir.join("metadata.json"), meta).map_err(|e| {
        cleanup();
        e.to_string()
    })?;
    complete.set(true);
    Ok(snapshot)
}

pub fn latest_snapshot(state_root: &Path, original_path: &str) -> Result<Snapshot, String> {
    let root = snapshots_root(state_root);
    let mut latest: Option<Snapshot> = None;
    for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let meta_path = entry.path().join("metadata.json");
        let Ok(data) = fs::read(&meta_path) else {
            continue;
        };
        let Ok(candidate) = serde_json::from_str::<Snapshot>(&String::from_utf8_lossy(&data))
        else {
            continue;
        };
        if candidate.original_path != original_path {
            continue;
        }
        if latest
            .as_ref()
            .map(|l| candidate.created_at > l.created_at)
            .unwrap_or(true)
        {
            latest = Some(candidate);
        }
    }
    latest.ok_or_else(|| "no snapshot found for this file".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_snapshot_with_metadata_and_hash() {
        let state = tempfile::tempdir().unwrap();
        let snap =
            create_snapshot(state.path(), "/home/x/.gitconfig", b"[user]\nname = Ada\n").unwrap();
        assert_eq!(
            std::fs::read(&snap.content_path).unwrap(),
            b"[user]\nname = Ada\n"
        );
        assert!(snap.path.starts_with(state.path()));
        assert_eq!(
            snap.hash,
            crate::core::secure::digest(b"[user]\nname = Ada\n")
        );
        let meta = std::fs::metadata(&snap.content_path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn latest_returns_most_recent_for_path() {
        let state = tempfile::tempdir().unwrap();
        create_snapshot(state.path(), "/a", b"one").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let newer = create_snapshot(state.path(), "/a", b"two").unwrap();
        create_snapshot(state.path(), "/b", b"other").unwrap();
        let latest = latest_snapshot(state.path(), "/a").unwrap();
        assert_eq!(latest.hash, newer.hash);
        assert_eq!(std::fs::read(latest.content_path).unwrap(), b"two");
    }

    #[test]
    fn latest_errors_when_missing() {
        let state = tempfile::tempdir().unwrap();
        assert!(latest_snapshot(state.path(), "/nope").is_err());
    }
}
