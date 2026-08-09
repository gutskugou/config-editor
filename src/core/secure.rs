use sha2::{Digest, Sha256};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Identity {
    pub dev: u64,
    pub ino: u64,
}

pub fn open_regular(path: &Path) -> Result<(File, Metadata), String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("stat {}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!(
            "only regular files are supported: {}",
            path.display()
        ));
    }
    Ok((file, meta))
}

pub fn validate_info(meta: &Metadata) -> Result<(), String> {
    let uid = unsafe { libc::getuid() };
    if meta.uid() != uid {
        return Err("configuration is not owned by the current user".into());
    }
    if meta.nlink() > 1 {
        return Err("files with multiple hard links are not edited safely".into());
    }
    Ok(())
}

pub fn identity(meta: &Metadata) -> Identity {
    Identity {
        dev: meta.dev(),
        ino: meta.ino(),
    }
}

pub fn allowed(roots: &[&Path], target: &Path) -> Result<(), String> {
    for root in roots {
        if let Ok(rel) = target.strip_prefix(root) {
            if !rel.as_os_str().is_empty() && !rel.starts_with("..") {
                return Ok(());
            }
        }
    }
    Err(format!(
        "refusing to edit outside the user configuration roots: {}",
        target.display()
    ))
}

pub fn secure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("mkdir {}: {e}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}

#[cfg(test)]
pub struct FailSync {
    flag: std::sync::atomic::AtomicBool,
    thread: std::sync::Mutex<Option<std::thread::ThreadId>>,
}

#[cfg(test)]
impl FailSync {
    const fn new() -> Self {
        Self {
            flag: std::sync::atomic::AtomicBool::new(false),
            thread: std::sync::Mutex::new(None),
        }
    }
    pub fn store(&self, value: bool, _order: std::sync::atomic::Ordering) {
        self.flag.store(value, std::sync::atomic::Ordering::SeqCst);
        *self.thread.lock().unwrap() = if value {
            Some(std::thread::current().id())
        } else {
            None
        };
    }
    pub fn load(&self, _order: std::sync::atomic::Ordering) -> bool {
        self.flag.load(std::sync::atomic::Ordering::SeqCst)
            && self.thread.lock().unwrap().as_ref() == Some(&std::thread::current().id())
    }
}

#[cfg(test)]
pub static FAIL_SYNC: FailSync = FailSync::new();

pub fn sync_dir(dir: &Path) -> Result<(), String> {
    #[cfg(test)]
    if FAIL_SYNC.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("injected directory sync failure".into());
    }
    let d = File::open(dir).map_err(|e| e.to_string())?;
    d.sync_all().map_err(|e| e.to_string())
}

pub fn atomic_write(
    path: &Path,
    content: &[u8],
    mode: u32,
    expected: &Identity,
) -> Result<Option<String>, String> {
    let dir = path.parent().ok_or("target has no parent directory")?;
    let mut temp = tempfile::Builder::new()
        .prefix(".config-editor-")
        .tempfile_in(dir)
        .map_err(|e| e.to_string())?;
    temp.as_file()
        .set_permissions(fs::Permissions::from_mode(mode & 0o777))
        .map_err(|e| e.to_string())?;
    temp.write_all(content).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    let (_, meta) = open_regular(path)?;
    validate_info(&meta)?;
    if identity(&meta) != *expected {
        return Err("configuration file was replaced before commit; nothing was written".into());
    }
    temp.persist(path).map_err(|e| e.error.to_string())?;
    match sync_dir(dir) {
        Ok(()) => Ok(None),
        Err(w) => Ok(Some(format!(
            "change was applied but directory sync failed: {w}"
        ))),
    }
}

pub fn digest(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex_lower(&hasher.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn open_regular_refuses_directory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(open_regular(dir.path()).is_err());
    }

    #[test]
    fn open_regular_follows_no_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, b"x").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(
            open_regular(&link).is_err(),
            "O_NOFOLLOW must reject symlink"
        );
        assert!(open_regular(&target).is_ok());
    }

    #[test]
    fn validate_info_rejects_multiple_hardlinks() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        std::fs::write(&a, b"x").unwrap();
        std::fs::hard_link(&a, dir.path().join("b")).unwrap();
        let (_f, meta) = open_regular(&a).unwrap();
        assert!(validate_info(&meta).is_err(), "nlink>1 must be rejected");
    }

    #[test]
    fn allowed_enforces_roots() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&home).unwrap();
        assert!(allowed(&[&home], &home.join("x/y")).is_ok());
        assert!(allowed(&[&home], &home.join("..").join("outside")).is_err());
        assert!(allowed(&[&home], &outside).is_err());
    }

    #[test]
    fn atomic_write_commits_and_syncs() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("cfg");
        std::fs::write(&f, b"old").unwrap();
        let (_f, meta) = open_regular(&f).unwrap();
        let id = identity(&meta);
        let result = atomic_write(&f, b"new", meta.permissions().mode() & 0o777, &id).unwrap();
        assert!(result.is_none());
        assert_eq!(std::fs::read(&f).unwrap(), b"new");
    }

    #[test]
    fn atomic_write_rejects_replaced_identity() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("cfg");
        std::fs::write(&f, b"old").unwrap();
        let other = Identity { dev: 1, ino: 999 };
        assert!(atomic_write(&f, b"new", 0o600, &other).is_err());
        assert_eq!(std::fs::read(&f).unwrap(), b"old");
    }

    #[test]
    fn digest_is_sha256_hex() {
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn directory_sync_failure_reports_warning_not_error() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("cfg");
        std::fs::write(&f, b"old").unwrap();
        let (_f, meta) = open_regular(&f).unwrap();
        let id = identity(&meta);
        FAIL_SYNC.store(true, std::sync::atomic::Ordering::SeqCst);
        let result = atomic_write(&f, b"new", 0o600, &id);
        FAIL_SYNC.store(false, std::sync::atomic::Ordering::SeqCst);
        match result {
            Ok(Some(_)) => {}
            other => panic!("expected applied-with-warning, got {other:?}"),
        }
        assert_eq!(std::fs::read(&f).unwrap(), b"new");
    }
}
