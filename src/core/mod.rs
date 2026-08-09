pub mod diff;
pub mod secure;
pub mod snapshot;

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::domain::Format;
use crate::validate::validate;
use secure::{
    allowed, atomic_write, digest, identity, open_regular, secure_dir, validate_info, Identity,
};

#[derive(Default)]
pub struct Manager {
    pub home: PathBuf,
    pub config_root: PathBuf,
    pub state_root: PathBuf,
}

#[derive(Debug)]
pub struct Change {
    pub target: PathBuf,
    pub stage: PathBuf,
    pub base_hash: String,
    pub identity: Identity,
    pub mode: u32,
    pub format: Format,
}

pub struct ApplyResult {
    pub snapshot: snapshot::Snapshot,
    pub warning: Option<String>,
}

fn edit_root(state_root: &Path) -> PathBuf {
    state_root.join("config-editor/edit")
}

impl Manager {
    pub fn prepare(&self, path: &Path, format: Format) -> Result<Change, String> {
        let canonical = fs::canonicalize(path).map_err(|e| format!("resolve target: {e}"))?;
        allowed(&[&self.home, &self.config_root], &canonical)?;
        let (mut file, meta) = open_regular(&canonical)?;
        validate_info(&meta)?;
        let mut before = Vec::new();
        file.read_to_end(&mut before).map_err(|e| e.to_string())?;
        let stage_dir = edit_root(&self.state_root);
        secure_dir(&stage_dir)?;
        let mut stage = tempfile::Builder::new()
            .prefix("stage-")
            .tempfile_in(&stage_dir)
            .map_err(|e| e.to_string())?;
        stage
            .as_file()
            .set_permissions(fs::Permissions::from_mode(
                meta.permissions().mode() & 0o777,
            ))
            .map_err(|e| e.to_string())?;
        stage.write_all(&before).map_err(|e| e.to_string())?;
        stage.as_file().sync_all().map_err(|e| e.to_string())?;
        let stage_path = stage.keep().map(|(_, p)| p).map_err(|e| e.to_string())?;
        Ok(Change {
            target: canonical,
            stage: stage_path,
            base_hash: digest(&before),
            identity: identity(&meta),
            mode: meta.permissions().mode() & 0o777,
            format,
        })
    }

    pub fn apply(&self, change: &Change) -> Result<ApplyResult, String> {
        let result = (|| {
            allowed(&[&self.home, &self.config_root], &change.target)?;
            let (mut current_file, meta) = open_regular(&change.target)?;
            validate_info(&meta)?;
            if identity(&meta) != change.identity {
                return Err(
                    "configuration file was replaced since editing began; nothing was written"
                        .into(),
                );
            }
            let mut current = Vec::new();
            current_file
                .read_to_end(&mut current)
                .map_err(|e| e.to_string())?;
            if digest(&current) != change.base_hash {
                return Err(
                    "configuration changed since editing began; nothing was written".into(),
                );
            }
            let stage_dir = edit_root(&self.state_root);
            let rel = change
                .stage
                .strip_prefix(&stage_dir)
                .map_err(|_| "staged file is outside the private edit directory".to_string())?;
            if rel.as_os_str().is_empty() || rel.starts_with("..") {
                return Err("staged file is outside the private edit directory".into());
            }
            let (mut stage_file, _) = open_regular(&change.stage)?;
            let mut after = Vec::new();
            stage_file
                .read_to_end(&mut after)
                .map_err(|e| e.to_string())?;
            validate(change.format, &change.stage, &after)?;
            let snap = snapshot::create_snapshot(
                &self.state_root,
                change.target.to_str().unwrap_or_default(),
                &current,
            )?;
            match atomic_write(&change.target, &after, change.mode, &change.identity) {
                Ok(warning) => Ok(ApplyResult {
                    snapshot: snap,
                    warning,
                }),
                Err(e) => {
                    let _ = fs::remove_dir_all(&snap.path);
                    Err(e)
                }
            }
        })();
        let _ = fs::remove_file(&change.stage);
        result
    }

    pub fn discard(&self, change: &Change) -> Result<(), String> {
        fs::remove_file(&change.stage).map_err(|e| e.to_string())
    }

    pub fn latest(&self, path: &str) -> Result<snapshot::Snapshot, String> {
        snapshot::latest_snapshot(&self.state_root, path)
    }

    pub fn prepare_restore(&self, path: &Path, format: Format) -> Result<Change, String> {
        let change = self.prepare(path, format)?;
        let snapshot = match self.latest(change.target.to_str().unwrap_or_default()) {
            Ok(s) => s,
            Err(e) => {
                let _ = self.discard(&change);
                return Err(e);
            }
        };
        let content = match fs::read(&snapshot.content_path) {
            Ok(c) => c,
            Err(e) => {
                let _ = self.discard(&change);
                return Err(e.to_string());
            }
        };
        if snapshot.hash.is_empty() || digest(&content) != snapshot.hash {
            let _ = self.discard(&change);
            return Err("snapshot content failed its SHA-256 integrity check".into());
        }
        fs::write(&change.stage, content).map_err(|e| {
            let _ = self.discard(&change);
            e.to_string()
        })?;
        Ok(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn manager(home: &Path, state: &Path) -> Manager {
        Manager {
            home: home.to_path_buf(),
            config_root: home.join(".config"),
            state_root: state.to_path_buf(),
        }
    }

    #[test]
    fn prepare_stages_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let state = dir.path().join("state");
        let cfg = home.join(".gitconfig");
        std::fs::write(&cfg, b"[user]\nname = Ada\n").unwrap();
        let m = manager(&home, &state);
        let change = m.prepare(&cfg, Format::Git).unwrap();
        assert!(change.stage.starts_with(&state.join("config-editor/edit")));
        assert_eq!(
            std::fs::read(&change.stage).unwrap(),
            b"[user]\nname = Ada\n"
        );
    }

    #[test]
    fn prepare_rejects_outside_roots() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let outside = dir.path().join("etc-file");
        std::fs::write(&outside, b"x").unwrap();
        let m = manager(&home, &dir.path().join("state"));
        assert!(m.prepare(&outside, Format::Git).is_err());
    }

    #[test]
    fn apply_commits_and_creates_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let state = dir.path().join("state");
        let cfg = home.join(".gitconfig");
        std::fs::write(&cfg, b"[user]\nname = Ada\n").unwrap();
        let m = manager(&home, &state);
        let change = m.prepare(&cfg, Format::Git).unwrap();
        std::fs::write(&change.stage, b"[user]\nname = Grace\n").unwrap();
        let result = m.apply(&change).unwrap();
        assert!(result.warning.is_none());
        assert_eq!(std::fs::read(&cfg).unwrap(), b"[user]\nname = Grace\n");
        assert!(result.snapshot.content_path.exists());
    }

    #[test]
    fn apply_rejects_modified_original() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let state = dir.path().join("state");
        let cfg = home.join(".gitconfig");
        std::fs::write(&cfg, b"[user]\nname = Ada\n").unwrap();
        let m = manager(&home, &state);
        let change = m.prepare(&cfg, Format::Git).unwrap();
        std::fs::write(&cfg, b"[user]\nname = Someone Else\n").unwrap();
        std::fs::write(&change.stage, b"[user]\nname = Grace\n").unwrap();
        assert!(m.apply(&change).is_err(), "hash drift must be rejected");
    }

    #[test]
    fn apply_rejects_replaced_identity() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let state = dir.path().join("state");
        let cfg = home.join(".gitconfig");
        std::fs::write(&cfg, b"a\n").unwrap();
        let m = manager(&home, &state);
        let change = m.prepare(&cfg, Format::Git).unwrap();
        std::fs::rename(&cfg, home.join("old")).unwrap();
        std::fs::write(&cfg, b"a\n").unwrap();
        assert!(m.apply(&change).is_err(), "identity drift must be rejected");
    }

    #[test]
    fn apply_runs_validation_before_write() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let state = dir.path().join("state");
        let cfg = home.join("starship.toml");
        std::fs::write(&cfg, b"format = \"ok\"\n").unwrap();
        let m = manager(&home, &state);
        let change = m.prepare(&cfg, Format::Toml).unwrap();
        std::fs::write(&change.stage, b"format = not quoted\n").unwrap();
        assert!(m.apply(&change).is_err(), "invalid TOML must block apply");
        assert_eq!(std::fs::read(&cfg).unwrap(), b"format = \"ok\"\n");
    }

    #[test]
    fn restore_uses_latest_snapshot_through_change() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let state = dir.path().join("state");
        let cfg = home.join(".gitconfig");
        std::fs::write(&cfg, b"[user]\nname = Grace\n").unwrap();
        let m = manager(&home, &state);
        let change = m.prepare(&cfg, Format::Git).unwrap();
        m.apply(&change).unwrap();
        // 模拟一次损坏的恢复目标
        let change2 = m.prepare_restore(&cfg, Format::Git).unwrap();
        assert_eq!(
            std::fs::read(&change2.stage).unwrap(),
            b"[user]\nname = Grace\n"
        );
    }

    #[test]
    fn restore_rejects_corrupt_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let state = dir.path().join("state");
        let cfg = home.join(".gitconfig");
        std::fs::write(&cfg, b"x = 1\n").unwrap();
        let m = manager(&home, &state);
        let change = m.prepare(&cfg, Format::Git).unwrap();
        m.apply(&change).unwrap();
        let snaps = state.join("config-editor/snapshots");
        for entry in std::fs::read_dir(&snaps).unwrap() {
            let e = entry.unwrap();
            if e.file_type().unwrap().is_dir() {
                std::fs::write(e.path().join("content"), b"corrupted").unwrap();
            }
        }
        assert!(m.prepare_restore(&cfg, Format::Git).is_err());
    }
}
