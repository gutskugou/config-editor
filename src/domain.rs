use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Discover,
    Structured,
    SyntaxCheck,
    StagedEditor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Git,
    Ssh,
    Ini,
    Properties,
    Toml,
    Jsonc,
    Bash,
    Zsh,
    Fish,
    Tmux,
    Vim,
    Lua,
}

impl Format {
    pub fn as_str(&self) -> &'static str {
        match self {
            Format::Git => "git",
            Format::Ssh => "ssh",
            Format::Ini => "ini",
            Format::Properties => "properties",
            Format::Toml => "toml",
            Format::Jsonc => "jsonc",
            Format::Bash => "bash",
            Format::Zsh => "zsh",
            Format::Fish => "fish",
            Format::Tmux => "tmux",
            Format::Vim => "vim",
            Format::Lua => "lua",
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub line: usize,
    pub editable: bool,
    pub sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Source {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
    pub exists: bool,
    pub format: Format,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<Setting>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Application {
    pub id: String,
    pub name: String,
    pub name_zh: String,
    pub description: String,
    pub description_zh: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub installed: bool,
    pub capabilities: Vec<Capability>,
    pub sources: Vec<Source>,
}

impl Application {
    pub fn configured(&self) -> bool {
        self.sources.iter().any(|s| s.exists)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_serialize_to_go_compatible_names() {
        let caps = vec![
            Capability::Discover,
            Capability::Structured,
            Capability::SyntaxCheck,
            Capability::StagedEditor,
        ];
        assert_eq!(
            serde_json::to_string(&caps).unwrap(),
            r#"["discover","structured","syntax-check","staged-editor"]"#
        );
    }

    #[test]
    fn formats_serialize_to_go_compatible_names() {
        let all = vec![
            Format::Git,
            Format::Ssh,
            Format::Ini,
            Format::Properties,
            Format::Toml,
            Format::Jsonc,
            Format::Bash,
            Format::Zsh,
            Format::Fish,
            Format::Tmux,
            Format::Vim,
            Format::Lua,
        ];
        assert_eq!(
            serde_json::to_string(&all).unwrap(),
            r#"["git","ssh","ini","properties","toml","jsonc","bash","zsh","fish","tmux","vim","lua"]"#
        );
    }

    #[test]
    fn application_json_matches_go_field_names() {
        let app = Application {
            id: "git".into(),
            name: "Git".into(),
            name_zh: "Git".into(),
            description: "d".into(),
            description_zh: "d".into(),
            command: Some("git".into()),
            installed: true,
            capabilities: vec![Capability::Structured],
            sources: vec![Source {
                path: "/home/x/.gitconfig".into(),
                resolved: None,
                exists: true,
                format: Format::Git,
                diagnostic: None,
                settings: vec![Setting {
                    key: "user.name".into(),
                    value: "Ada".into(),
                    line: 1,
                    editable: true,
                    sensitive: false,
                }],
            }],
        };
        let got: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&app).unwrap()).unwrap();
        assert!(got.get("name_zh").is_some());
        assert!(got.get("capabilities").is_some());
        assert!(
            got.get("resolved").is_none(),
            "empty resolved must be omitted"
        );
        assert!(got["sources"][0]["format"] == "git");
    }

    #[test]
    fn configured_is_true_when_any_source_exists() {
        let app = Application {
            sources: vec![Source {
                exists: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(app.configured());
    }
}
