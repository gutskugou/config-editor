use std::{env, path::PathBuf};

pub struct Paths {
    pub home: PathBuf,
    pub config: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
}

pub fn resolve() -> Result<Paths, String> {
    let home = env::var_os("HOME").ok_or("HOME is not set")?;
    let home = PathBuf::from(home);
    Ok(Paths {
        home: home.clone(),
        config: base("XDG_CONFIG_HOME", home.join(".config")),
        state: base("XDG_STATE_HOME", home.join(".local/state")),
        cache: base("XDG_CACHE_HOME", home.join(".cache")),
    })
}

fn base(name: &str, fallback: PathBuf) -> PathBuf {
    match env::var_os(name) {
        Some(v) if PathBuf::from(&v).is_absolute() => v.into(),
        _ => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn fallbacks_use_home_when_env_is_relative() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved_home = std::env::var_os("HOME");
        let saved_xdg = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("HOME", "/home/me");
        std::env::set_var("XDG_CONFIG_HOME", "relative/config");
        let p = resolve().unwrap();
        assert_eq!(p.config, PathBuf::from("/home/me/.config"));
        restore(&saved_home, "HOME");
        restore(&saved_xdg, "XDG_CONFIG_HOME");
    }

    #[test]
    fn absolute_xdg_override_is_used() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved_home = std::env::var_os("HOME");
        let saved_xdg = std::env::var_os("XDG_STATE_HOME");
        std::env::set_var("HOME", "/home/me");
        std::env::set_var("XDG_STATE_HOME", "/var/state");
        let p = resolve().unwrap();
        assert_eq!(p.state, PathBuf::from("/var/state"));
        restore(&saved_home, "HOME");
        restore(&saved_xdg, "XDG_STATE_HOME");
    }

    fn restore(saved: &Option<std::ffi::OsString>, name: &str) {
        match saved {
            Some(v) => std::env::set_var(name, v),
            None => std::env::remove_var(name),
        }
    }
}
