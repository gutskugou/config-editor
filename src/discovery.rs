use crate::domain::{Application, Capability, Format, Source};
use crate::parse::parse_settings;
use crate::paths::Paths;
use std::{env, path::PathBuf};

pub fn builtins() -> usize {
    definitions().len()
}

struct Candidate {
    format: Format,
    file: &'static str,
    under: Under,
}
enum Under {
    Home,
    Config,
}

struct AppDef {
    id: &'static str,
    name: &'static str,
    name_zh: &'static str,
    description: &'static str,
    description_zh: &'static str,
    command: &'static str,
    capabilities: &'static [Capability],
    candidates: Vec<Candidate>,
}

fn definitions() -> Vec<AppDef> {
    fn c(format: Format, under: Under, file: &'static str) -> Candidate {
        Candidate {
            format,
            under,
            file,
        }
    }
    fn structured() -> &'static [Capability] {
        &[
            Capability::Discover,
            Capability::Structured,
            Capability::StagedEditor,
        ]
    }
    fn script() -> &'static [Capability] {
        &[
            Capability::Discover,
            Capability::SyntaxCheck,
            Capability::StagedEditor,
        ]
    }
    fn raw() -> &'static [Capability] {
        &[Capability::Discover, Capability::StagedEditor]
    }
    vec![
        AppDef {
            id: "git",
            name: "Git",
            name_zh: "Git",
            description: "Identity, aliases and repository defaults",
            description_zh: "身份、别名和仓库默认值",
            command: "git",
            capabilities: structured(),
            candidates: vec![
                c(Format::Git, Under::Home, ".gitconfig"),
                c(Format::Git, Under::Config, "git/config"),
            ],
        },
        AppDef {
            id: "ssh",
            name: "SSH client",
            name_zh: "SSH 客户端",
            description: "Hosts, users, ports and keys",
            description_zh: "主机、用户、端口和密钥",
            command: "ssh",
            capabilities: structured(),
            candidates: vec![c(Format::Ssh, Under::Home, ".ssh/config")],
        },
        AppDef {
            id: "bash",
            name: "Bash",
            name_zh: "Bash",
            description: "Interactive shell startup",
            description_zh: "交互式 Shell 启动配置",
            command: "bash",
            capabilities: script(),
            candidates: vec![
                c(Format::Bash, Under::Home, ".bashrc"),
                c(Format::Bash, Under::Home, ".bash_profile"),
                c(Format::Bash, Under::Home, ".profile"),
            ],
        },
        AppDef {
            id: "zsh",
            name: "Zsh",
            name_zh: "Zsh",
            description: "Interactive shell startup",
            description_zh: "交互式 Shell 启动配置",
            command: "zsh",
            capabilities: script(),
            candidates: vec![
                c(Format::Zsh, Under::Home, ".zshrc"),
                c(Format::Zsh, Under::Config, "zsh/.zshrc"),
            ],
        },
        AppDef {
            id: "fish",
            name: "Fish",
            name_zh: "Fish",
            description: "Interactive shell startup",
            description_zh: "交互式 Shell 启动配置",
            command: "fish",
            capabilities: script(),
            candidates: vec![c(Format::Fish, Under::Config, "fish/config.fish")],
        },
        AppDef {
            id: "tmux",
            name: "tmux",
            name_zh: "tmux",
            description: "Terminal multiplexer preferences",
            description_zh: "终端复用器偏好",
            command: "tmux",
            capabilities: raw(),
            candidates: vec![
                c(Format::Tmux, Under::Home, ".tmux.conf"),
                c(Format::Tmux, Under::Config, "tmux/tmux.conf"),
            ],
        },
        AppDef {
            id: "vim",
            name: "Vim",
            name_zh: "Vim",
            description: "Editor preferences and plugins",
            description_zh: "编辑器偏好和插件",
            command: "vim",
            capabilities: raw(),
            candidates: vec![
                c(Format::Vim, Under::Home, ".vimrc"),
                c(Format::Vim, Under::Config, "vim/vimrc"),
            ],
        },
        AppDef {
            id: "nvim",
            name: "Neovim",
            name_zh: "Neovim",
            description: "Editor preferences and plugins",
            description_zh: "编辑器偏好和插件",
            command: "nvim",
            capabilities: raw(),
            candidates: vec![
                c(Format::Lua, Under::Config, "nvim/init.lua"),
                c(Format::Vim, Under::Config, "nvim/init.vim"),
            ],
        },
        AppDef {
            id: "vscode",
            name: "VS Code",
            name_zh: "VS Code",
            description: "User settings (local and Remote)",
            description_zh: "用户设置（本地与远程）",
            command: "code",
            capabilities: &[
                Capability::Discover,
                Capability::SyntaxCheck,
                Capability::StagedEditor,
            ],
            candidates: vec![
                c(
                    Format::Jsonc,
                    Under::Home,
                    ".vscode-server/data/Machine/settings.json",
                ),
                c(Format::Jsonc, Under::Config, "Code/User/settings.json"),
            ],
        },
        AppDef {
            id: "starship",
            name: "Starship",
            name_zh: "Starship",
            description: "Cross-shell prompt settings",
            description_zh: "跨 Shell 提示符设置",
            command: "starship",
            capabilities: &[
                Capability::Discover,
                Capability::Structured,
                Capability::SyntaxCheck,
                Capability::StagedEditor,
            ],
            candidates: vec![c(Format::Toml, Under::Config, "starship.toml")],
        },
        AppDef {
            id: "npm",
            name: "npm",
            name_zh: "npm",
            description: "Package manager defaults",
            description_zh: "包管理器默认值",
            command: "npm",
            capabilities: structured(),
            candidates: vec![
                c(Format::Properties, Under::Home, ".npmrc"),
                c(Format::Properties, Under::Config, "npm/npmrc"),
            ],
        },
        AppDef {
            id: "pip",
            name: "pip",
            name_zh: "pip",
            description: "Python package installer defaults",
            description_zh: "Python 包安装器默认值",
            command: "pip",
            capabilities: structured(),
            candidates: vec![
                c(Format::Ini, Under::Config, "pip/pip.conf"),
                c(Format::Ini, Under::Home, ".pip/pip.conf"),
            ],
        },
    ]
}

fn candidate_path(p: &Paths, c: &Candidate) -> PathBuf {
    match c.under {
        Under::Home => p.home.join(c.file),
        Under::Config => p.config.join(c.file),
    }
}

pub(crate) fn find_in_path(command: &str) -> bool {
    let Ok(path) = env::var("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| {
        let full = dir.join(command);
        full.is_file() && {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(&full)
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        }
    })
}

pub fn scan(p: &Paths) -> Result<Vec<Application>, String> {
    let mut apps = Vec::with_capacity(builtins());
    for d in definitions() {
        let mut a = Application {
            id: d.id.into(),
            name: d.name.into(),
            name_zh: d.name_zh.into(),
            description: d.description.into(),
            description_zh: d.description_zh.into(),
            command: Some(d.command.into()),
            installed: find_in_path(d.command),
            capabilities: d.capabilities.to_vec(),
            sources: Vec::new(),
        };
        for c in &d.candidates {
            let path = candidate_path(p, c);
            let mut source = Source {
                path: path.to_string_lossy().into_owned(),
                format: c.format,
                ..Default::default()
            };
            match std::fs::metadata(&path) {
                Ok(meta) if meta.is_file() => {
                    source.exists = true;
                    if let Ok(resolved) = std::fs::canonicalize(&path) {
                        source.resolved = Some(resolved.to_string_lossy().into_owned());
                    }
                    match std::fs::read(&path) {
                        Ok(content) => {
                            if d.capabilities.contains(&Capability::Structured) {
                                source.settings = parse_settings(c.format, &content);
                            }
                        }
                        Err(e) => source.diagnostic = Some(e.to_string()),
                    }
                }
                Ok(_) => source.diagnostic = Some("path exists but is not a regular file".into()),
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    source.diagnostic = Some(format!("inspect: {e}"));
                }
                Err(_) => {}
            }
            a.sources.push(source);
        }
        apps.push(a);
    }
    apps.sort_by(|x, y| {
        let xc = x.configured();
        let yc = y.configured();
        if xc != yc {
            return yc.cmp(&xc);
        }
        if x.installed != y.installed {
            return y.installed.cmp(&x.installed);
        }
        x.name.to_lowercase().cmp(&y.name.to_lowercase())
    });
    Ok(apps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;

    #[test]
    fn builtins_is_twelve() {
        assert_eq!(builtins(), 12);
    }

    #[test]
    fn shell_sources_are_not_structured() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join(".bashrc"),
            b"if [ \"$x\" = y ]; then\n  export A=b\nfi\n",
        )
        .unwrap();
        let p = Paths {
            home: home.path().to_path_buf(),
            config: home.path().join(".config"),
            state: home.path().join(".state"),
            cache: home.path().join(".cache"),
        };
        let apps = scan(&p).unwrap();
        let bash = apps.iter().find(|a| a.id == "bash").unwrap();
        assert!(bash.sources[0].settings.is_empty());
    }

    #[test]
    fn symlink_sources_report_resolved_target() {
        let home = tempfile::tempdir().unwrap();
        let target = home.path().join("real-config");
        std::fs::write(&target, b"[user]\nname = Ada\n").unwrap();
        std::os::unix::fs::symlink(&target, home.path().join(".gitconfig")).unwrap();
        let p = Paths {
            home: home.path().to_path_buf(),
            config: home.path().join(".config"),
            state: home.path().join(".state"),
            cache: home.path().join(".cache"),
        };
        let apps = scan(&p).unwrap();
        let git = apps.iter().find(|a| a.id == "git").unwrap();
        let src = &git.sources[0];
        assert!(src.exists);
        assert_eq!(src.resolved.as_deref(), Some(target.to_str().unwrap()));
        assert_eq!(src.settings[0].key, "user.name");
    }

    #[test]
    fn non_regular_source_reports_diagnostic() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join(".gitconfig")).unwrap();
        let p = Paths {
            home: home.path().to_path_buf(),
            config: home.path().join(".config"),
            state: home.path().join(".state"),
            cache: home.path().join(".cache"),
        };
        let apps = scan(&p).unwrap();
        let git = apps.iter().find(|a| a.id == "git").unwrap();
        assert!(git.sources[0].diagnostic.is_some());
    }

    #[test]
    fn scan_sorts_configured_first_then_installed_then_name() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".bashrc"), b"x=1\n").unwrap();
        let p = Paths {
            home: home.path().to_path_buf(),
            config: home.path().join(".config"),
            state: home.path().join(".state"),
            cache: home.path().join(".cache"),
        };
        let apps = scan(&p).unwrap();
        assert_eq!(apps[0].id, "bash", "configured app must sort first");
    }
}
