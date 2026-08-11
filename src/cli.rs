use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::discovery;
use crate::domain::Application;
use crate::paths;

pub fn version_string() -> String {
    format!(
        "config-editor {} (commit {}, built {})",
        env!("CARGO_PKG_VERSION"),
        env!("CONFIG_EDITOR_COMMIT"),
        env!("CONFIG_EDITOR_DATE")
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
                if args.len() == 3 && args[2] == "--json" {
                    println!("{}", doctor_json(&p));
                    return Ok(Some(()));
                }
                if args.len() > 2 {
                    return Err("usage: config-editor doctor [--json]".into());
                }
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
            "report" => {
                let file = if args.len() == 4 && args[2] == "--output" {
                    args[3].as_str()
                } else {
                    return Err("usage: config-editor report --output FILE".into());
                };
                // 报告只写入新文件：拒绝覆盖已有文件（包括配置文件），
                // 与"永不静默改写配置"的安全边界一致
                let target = Path::new(file);
                if target.exists() {
                    return Err(format!(
                        "refusing to overwrite existing file: {file}; choose a new path"
                    ));
                }
                let apps = discovery::scan(&p)?;
                let out = build_report(&apps)?;
                write_report(target, out.as_bytes()).map_err(|e| format!("write {file}: {e}"))?;
                println!("report written to {file}");
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

/// doctor --json：机器可读环境诊断；路径归一化到 ~ 下，不暴露用户名
fn doctor_json(p: &paths::Paths) -> String {
    let mut tools = serde_json::Map::new();
    for cmd in [
        "git", "ssh", "bash", "zsh", "fish", "tmux", "vim", "nvim", "code", "starship", "npm",
        "pip",
    ] {
        tools.insert(
            cmd.to_string(),
            serde_json::Value::String(if discovery_installed(cmd) {
                "ok".into()
            } else {
                "missing".into()
            }),
        );
    }
    serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "home": normalize_home(&p.home, &p.home),
        "config_home": normalize_home(&p.config, &p.home),
        "state_home": normalize_home(&p.state, &p.home),
        "adapters": discovery::builtins(),
        "tools": tools,
    })
    .to_string()
}

/// report --output：本地诊断导出；不含配置内容、设置值、路径、用户名或 token
fn build_report(apps: &[Application]) -> Result<String, String> {
    let applications: Vec<serde_json::Value> = apps
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "installed": a.installed,
                "configured": a.configured(),
                "capabilities": a.capabilities,
                "sources": a.sources.iter().map(|s| serde_json::json!({
                    "format": s.format.as_str(),
                    "exists": s.exists,
                    "settings": s.settings.len(),
                    "sensitive": s.settings.iter().filter(|x| x.sensitive).count(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let out = serde_json::json!({
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "applications": applications,
    });
    serde_json::to_string_pretty(&out).map_err(|e| e.to_string())
}

/// 路径归一化：home 下 → ~/…；home 外只保留 basename，避免泄露用户名与私人路径
fn normalize_home(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rel) if rel.as_os_str().is_empty() => "~".into(),
        Ok(rel) => format!("~/{}", rel.display()),
        // home 外路径只保留 basename；注意 basename 本身可能恰为用户名（如 /home2/alice），
        // 因此 doctor --json 适合本机诊断；对外分享请用 report（完全不包含路径）
        Err(_) => path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "external".into()),
    }
}

/// 报告写入：目标目录内临时文件（创建即 0600）→ fsync → 原子 rename；
/// 不继承 umask，也不留半成品
fn write_report(target: &Path, content: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::Builder::new()
        .prefix(".config-editor-report-")
        .tempfile_in(dir)
        .map_err(|e| e.to_string())?;
    temp.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    temp.write_all(content).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(target).map_err(|e| e.error.to_string())?;
    Ok(())
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

    #[test]
    fn doctor_json_normalizes_paths_without_usernames() {
        let p = paths::Paths {
            home: "/home/me".into(),
            config: "/home/me/.config".into(),
            state: "/home/me/.local/state".into(),
            cache: "/home/me/.cache".into(),
        };
        let value: serde_json::Value = serde_json::from_str(&doctor_json(&p)).unwrap();
        assert_eq!(value["os"], std::env::consts::OS);
        assert_eq!(value["home"], "~");
        assert_eq!(value["config_home"], "~/.config");
        assert_eq!(value["state_home"], "~/.local/state");
        assert_eq!(value["adapters"], 12);
        assert!(value["tools"]["git"].is_string());
    }

    #[test]
    fn doctor_json_redacts_paths_outside_home() {
        // XDG_STATE_HOME 在 home 外且路径含用户名：只保留 basename
        let p = paths::Paths {
            home: "/home/me".into(),
            config: "/home/me/.config".into(),
            state: "/home/me-state".into(),
            cache: "/home/me/.cache".into(),
        };
        let value: serde_json::Value = serde_json::from_str(&doctor_json(&p)).unwrap();
        assert_eq!(value["state_home"], "me-state");
        assert!(
            !doctor_json(&p).contains("/home/me-state"),
            "home 外路径不得原样输出"
        );
    }

    #[test]
    fn report_output_is_redacted_and_structured() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".gitconfig"),
            b"[user]\nname = Ada\npassword = swordfish\n",
        )
        .unwrap();
        let home = dir.path().to_path_buf();
        let p = paths::Paths {
            home: home.clone(),
            config: home.join(".config"),
            state: home.join(".state"),
            cache: home.join(".cache"),
        };
        let apps = discovery::scan(&p).unwrap();
        let out = build_report(&apps).unwrap();
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        assert!(value["generated_at"].is_string());
        let git = value["applications"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "git")
            .unwrap();
        assert!(git["installed"].is_boolean());
        let source = &git["sources"][0];
        assert_eq!(source["format"], "git");
        assert_eq!(source["settings"], 2);
        assert_eq!(source["sensitive"], 1);
        let raw = out.to_lowercase();
        assert!(!raw.contains("password"), "report 不得包含设置键或值");
        assert!(!raw.contains("/home/"), "report 不得包含完整私人路径");
        assert!(!raw.contains("swordfish"), "report 不得包含设置值");
    }

    #[test]
    fn report_requires_output_flag() {
        assert!(run(&["config-editor".into(), "report".into()]).is_err());
        assert!(run(&["config-editor".into(), "report".into(), "--output".into()]).is_err());
    }

    #[test]
    fn report_refuses_to_overwrite_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("existing.json");
        std::fs::write(&target, b"precious").unwrap();
        // 通过 run 走完整 CLI 路径
        let args = [
            "config-editor".to_string(),
            "report".to_string(),
            "--output".to_string(),
            target.to_str().unwrap().to_string(),
        ];
        assert!(run(&args).is_err(), "已存在文件必须被拒绝覆盖");
        assert_eq!(std::fs::read(&target).unwrap(), b"precious");
    }

    #[test]
    fn report_writes_private_atomic_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("report.json");
        let args = [
            "config-editor".to_string(),
            "report".to_string(),
            "--output".to_string(),
            target.to_str().unwrap().to_string(),
        ];
        assert!(run(&args).is_ok());
        let meta = std::fs::metadata(&target).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        // 目录里不得残留临时文件
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(leftovers.len(), 1, "只应有 report.json，无临时文件残留");
    }

    #[test]
    fn doctor_json_accepts_flag() {
        assert!(run(&["config-editor".into(), "doctor".into(), "--json".into()]).is_ok());
        assert!(
            run(&[
                "config-editor".into(),
                "doctor".into(),
                "--json".into(),
                "extra".into()
            ])
            .is_err(),
            "doctor 不接受多余参数"
        );
    }
}
