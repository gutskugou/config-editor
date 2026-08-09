use std::io::Write;

use crate::discovery;
use crate::paths;

pub fn version_string() -> String {
    format!(
        "config-editor {} (commit {}, built {})",
        env!("CARGO_PKG_VERSION"),
        "unknown",
        "unknown"
    )
}

pub fn run(args: &[String]) -> Result<Option<()>, String> {
    if args.len() == 2 && (args[1] == "version" || args[1] == "--version") {
        println!("{}", version_string());
        return Ok(Some(()));
    }
    let p = paths::resolve()?;
    if args.len() > 1 {
        match args[1].as_str() {
            "scan" => {
                if args.len() != 3 || args[2] != "--json" {
                    return Err("usage: config-editor scan --json".into());
                }
                let apps = discovery::scan(&p)?;
                let out = serde_json::to_string_pretty(&apps).map_err(|e| e.to_string())?;
                println!("{out}");
                Ok(Some(()))
            }
            "doctor" => {
                let mut out = String::new();
                out.push_str("config-editor doctor\n");
                out.push_str(&format!(
                    "OS: {}/{}\n",
                    std::env::consts::OS,
                    std::env::consts::ARCH
                ));
                out.push_str(&format!("HOME: {}\n", p.home.display()));
                out.push_str(&format!("XDG_CONFIG_HOME: {}\n", p.config.display()));
                out.push_str(&format!("XDG_STATE_HOME: {}\n", p.state.display()));
                out.push_str(&format!("Adapters: {}\n", discovery::builtins()));
                for cmd in [
                    "git", "ssh", "bash", "zsh", "fish", "tmux", "vim", "nvim", "code", "starship",
                    "npm", "pip",
                ] {
                    let state = if discovery_installed(cmd) {
                        "ok"
                    } else {
                        "missing"
                    };
                    out.push_str(&format!("{:<10} {}\n", cmd, state));
                }
                print!("{out}");
                std::io::stdout().flush().map_err(|e| e.to_string())?;
                Ok(Some(()))
            }
            other => Err(format!(
                "unknown command {other:?}; use scan --json, doctor or version"
            )),
        }
    } else {
        Ok(None)
    }
}

fn discovery_installed(cmd: &str) -> bool {
    crate::discovery::find_in_path(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_output_contains_binary_name() {
        assert!(version_string().starts_with("config-editor "));
    }

    #[test]
    fn scan_json_produces_valid_array_with_go_fields() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitconfig"), b"[user]\nname = Ada\n").unwrap();
        let home = dir.path().to_path_buf();
        let apps = discovery::scan(&paths::Paths {
            home: home.clone(),
            config: home.join(".config"),
            state: home.join(".state"),
            cache: home.join(".cache"),
        })
        .unwrap();
        let json = serde_json::to_string_pretty(&apps).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value.is_array());
        let git = value
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "git")
            .unwrap();
        assert_eq!(git["name"], "Git");
        assert!(git["sources"][0]["settings"][0]["key"].as_str().is_some());
    }

    #[test]
    fn unknown_command_errors() {
        assert!(run(&["config-editor".to_string(), "frobnicate".to_string()]).is_err());
    }
}
