use crate::domain::{Format, Setting};
use regex::Regex;
use std::sync::LazyLock;

static SENSITIVE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(token|password|passwd|secret|private[_-]?key|api[_-]?key|access[_-]?key|credential|_auth)").unwrap()
});
static SENSITIVE_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:bearer|basic)\s+[a-z0-9._~+/=-]+|://[^/\s:@]+:[^@\s/]+@").unwrap()
});

pub fn parse_settings(format: Format, content: &[u8]) -> Vec<Setting> {
    let mut out = Vec::new();
    let mut section = String::new();
    let mut ssh_scope = String::new();
    let mut occ: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for (index, raw) in String::from_utf8_lossy(content).lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        let Some((mut key, mut value)) = split_setting(format, line) else {
            continue;
        };
        if format == Format::Toml {
            value = strip_inline_comment(&value).trim_end().to_string();
        }
        if format == Format::Ssh
            && (key.eq_ignore_ascii_case("Host") || key.eq_ignore_ascii_case("Match"))
        {
            ssh_scope = format!("{} {}", key, value);
            continue;
        }
        if format == Format::Ssh && !ssh_scope.is_empty() {
            key = format!("{}.{}", ssh_scope, key);
        } else if !section.is_empty() {
            key = format!("{}.{}", section, key);
        }
        let n = occ.entry(key.clone()).or_default();
        *n += 1;
        let secret = is_sensitive(&key, &value);
        if secret {
            value = "••••••".to_string();
        }
        out.push(Setting {
            key,
            value,
            line: index + 1,
            occ: *n,
            editable: !secret,
            sensitive: secret,
        });
    }
    out
}

fn is_sensitive(key: &str, value: &str) -> bool {
    if SENSITIVE_KEY.is_match(key) || SENSITIVE_VALUE.is_match(value) {
        return true;
    }
    let trimmed = value.trim().trim_matches(|c| c == '"' || c == '\'');
    match url::Url::parse(trimmed) {
        Ok(parsed) => parsed.password().is_some(),
        Err(_) => false,
    }
}

fn split_setting(format: Format, line: &str) -> Option<(String, String)> {
    if format == Format::Ssh {
        let mut parts = line.split_whitespace();
        let key = parts.next()?;
        let value = parts.collect::<Vec<_>>().join(" ");
        if value.is_empty() {
            return None;
        }
        return Some((key.to_string(), value));
    }
    if format == Format::Ini {
        if let Some(pos) = line.find(['=', ':']) {
            if pos > 0 {
                return Some((
                    line[..pos].trim().to_string(),
                    line[pos + 1..].trim().to_string(),
                ));
            }
        }
        return None;
    }
    if let Some(pos) = line.find('=') {
        if pos > 0 {
            return Some((
                line[..pos].trim().to_string(),
                line[pos + 1..].trim().to_string(),
            ));
        }
    }
    if format == Format::Git {
        let mut parts = line.split_whitespace();
        if let Some(key) = parts.next() {
            if parts.next().is_none() {
                return Some((key.to_string(), "true".to_string()));
            }
        }
    }
    None
}

pub fn replace_setting(
    format: Format,
    content: &[u8],
    setting: &Setting,
    value: &str,
) -> Result<Vec<u8>, String> {
    if !setting.editable || setting.sensitive {
        return Err("sensitive settings cannot be edited here".into());
    }
    // 严格 UTF-8：无效字节不得被 from_utf8_lossy 改写后写回
    let text = std::str::from_utf8(content)
        .map_err(|_| "file is not valid UTF-8; inline editing is disabled".to_string())?;
    let mut lines: Vec<&str> = text.split('\n').collect();
    if setting.line < 1 || setting.line > lines.len() {
        return Err("setting line is no longer present".into());
    }
    let line = lines[setting.line - 1];
    let indent: String = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    if format == Format::Ssh {
        let key = line
            .split_whitespace()
            .next()
            .ok_or("setting is not a key/value line")?;
        let new_line = format!("{}{} {}", indent, key, value);
        lines[setting.line - 1] = &new_line;
        return Ok(lines.join("\n").into_bytes());
    }
    let mut pos = line.find('=');
    if format == Format::Ini && pos.is_none() {
        pos = line.find(':');
    }
    if format == Format::Git && pos.is_none() {
        let key = line.trim();
        let new_line = format!("{}{}={}", indent, key, value);
        lines[setting.line - 1] = &new_line;
        return Ok(lines.join("\n").into_bytes());
    }
    let pos = pos.ok_or("setting is not a key/value line")?;
    let prefix = line[..pos].trim_end();
    let mut new_value = value.to_string();
    if format == Format::Toml && is_quoted(line[pos + 1..].trim()) && !is_quoted(&new_value) {
        new_value = format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""));
    }
    let mut replaced;
    if matches!(format, Format::Properties | Format::Ini | Format::Git) {
        // 保留原始分隔符风格（" = " / "=" / ": " 等），不归一化为 "="。
        // 分隔符严格为「空白 + 单个 =/: + 空白」：值以 =/: 开头时不得被吞入分隔符
        let sep_start = line[..pos].trim_end().len();
        let after_sep = line[pos + 1..]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();
        let sep = &line[sep_start..pos + 1 + after_sep];
        replaced = format!("{}{}{}", prefix, sep, new_value);
    } else {
        let space = if line[pos + 1..].starts_with(' ') {
            " "
        } else {
            ""
        };
        replaced = format!("{} ={}{}", prefix, space, new_value);
    }
    if format == Format::Toml {
        // 行内注释（引号外的 # 及其后内容）必须保留
        let rest = line[pos + 1..].trim();
        let comment = rest[strip_inline_comment(rest).len()..].trim_end();
        if !comment.is_empty() {
            replaced = format!("{replaced} {comment}");
        }
    }
    lines[setting.line - 1] = &replaced;
    Ok(lines.join("\n").into_bytes())
}

/// 截断行内注释：返回引号字符串之外的第一个 `#` 之前的子串。
/// TOML 中 `#` 只出现在字符串值外时才是注释起始。
fn strip_inline_comment(value: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (i, c) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_double => escaped = true,
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            '#' if !in_single && !in_double => return &value[..i],
            _ => {}
        }
    }
    value
}

fn is_quoted(value: &str) -> bool {
    value.starts_with('"') || value.starts_with('\'')
}

/// 编辑前在当前内容中重新定位设置，返回该设置的当前状态。
///
/// 定位依据是稳定标识（完整 key + 出现序号 occ + 原值），而不是可能过期的行号：
/// - occ 与值都匹配 → 外部未增删同名键，精确命中；
/// - 仅值匹配且唯一 → occ 因外部增删同名键而漂移，值仍唯一确定；
/// - 同 key 仅剩一条 → 唯一确定；
/// - 其余情况（同名多条且无法可靠区分）→ 歧义，返回 Err，要求重新扫描；
/// - 同 key 不存在 → Ok(None)。
pub fn relocate_setting(
    format: Format,
    content: &[u8],
    setting: &Setting,
) -> Result<Option<Setting>, String> {
    // 严格 UTF-8：无效字节文件不做行内编辑（与 replace_setting 一致）
    std::str::from_utf8(content)
        .map_err(|_| "file is not valid UTF-8; inline editing is disabled".to_string())?;
    let candidates: Vec<Setting> = parse_settings(format, content)
        .into_iter()
        .filter(|s| s.key == setting.key)
        .collect();
    if candidates.is_empty() {
        return Ok(None);
    }
    if let Some(matched) = candidates
        .iter()
        .find(|s| s.occ == setting.occ && s.value == setting.value)
    {
        return Ok(Some(matched.clone()));
    }
    let value_matches: Vec<&Setting> = candidates
        .iter()
        .filter(|s| s.value == setting.value)
        .collect();
    if value_matches.len() == 1 {
        return Ok(Some(value_matches[0].clone()));
    }
    match candidates.len() {
        1 => Ok(candidates.into_iter().next()),
        _ => Err("ambiguous: multiple same-name settings; re-scan and try again".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Format;

    #[test]
    fn parses_sections_and_redacts_secrets() {
        let s = parse_settings(Format::Git, b"[user]\nname = Ada\npassword = swordfish\n");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].key, "user.name");
        assert_eq!(s[0].value, "Ada");
        assert!(s[1].sensitive && !s[1].editable && s[1].value == "••••••");
    }

    #[test]
    fn redacts_credentials_in_values() {
        let s = parse_settings(Format::Ini, b"[global]\nindex-url=https://user:swordfish@example.test/simple\nheader=Bearer abc.def.ghi\n");
        assert_eq!(s.len(), 2);
        assert!(s
            .iter()
            .all(|x| x.sensitive && !x.editable && x.value == "••••••"));
    }

    #[test]
    fn parses_supported_line_formats() {
        let pip = parse_settings(Format::Ini, b"[global]\ntimeout: 30\n");
        assert_eq!(pip[0].key, "global.timeout");
        let git = parse_settings(Format::Git, b"[core]\nbare\n");
        assert_eq!(git[0].key, "core.bare");
        assert_eq!(git[0].value, "true");
        let ssh = parse_settings(
            Format::Ssh,
            b"Host example\n  User ada\nHost other\n  User grace\n",
        );
        assert_eq!(ssh[0].key, "Host example.User");
        assert_eq!(ssh[1].key, "Host other.User");
    }

    #[test]
    fn assigns_occurrence_numbers_to_duplicate_keys() {
        let git = parse_settings(
            Format::Git,
            b"[user]\nname = Ada\nname = Grace\n[core]\nbare\nname = Grace\n",
        );
        let dups: Vec<_> = git.iter().filter(|s| s.key == "user.name").collect();
        assert_eq!(dups.len(), 2);
        assert_eq!((dups[0].occ, dups[0].value.as_str()), (1, "Ada"));
        assert_eq!((dups[1].occ, dups[1].value.as_str()), (2, "Grace"));
        let single = git.iter().find(|s| s.key == "core.bare").unwrap();
        assert_eq!(single.occ, 1, "first occurrence starts at 1");
        let ssh = parse_settings(
            Format::Ssh,
            b"Host example\n  User ada\nHost example\n  User grace\n",
        );
        assert_eq!(ssh[0].occ, 1);
        assert_eq!(ssh[1].occ, 2);
        assert_eq!(ssh[0].key, ssh[1].key, "duplicate Host must share the key");
    }

    #[test]
    fn replace_preserves_other_lines() {
        let before = b"# keep\n[user]\n\tname = Ada\n\temail = ada@example.test\n";
        let setting = Setting {
            key: "user.name".into(),
            value: "Ada".into(),
            line: 3,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let after =
            String::from_utf8(replace_setting(Format::Git, before, &setting, "Grace").unwrap())
                .unwrap();
        assert!(after.contains("# keep"));
        assert!(after.contains("email = ada@example.test"));
        assert!(after.contains("name = Grace"), "等号两侧空格风格必须保留");
    }

    #[test]
    fn replace_bare_git_boolean() {
        let before = b"[core]\n\tbare\n";
        let setting = Setting {
            key: "core.bare".into(),
            value: "true".into(),
            line: 2,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let after =
            String::from_utf8(replace_setting(Format::Git, before, &setting, "false").unwrap())
                .unwrap();
        assert!(after.contains("\tbare=false"));
    }

    #[test]
    fn replace_toml_requotes_strings() {
        let before = b"format = \"$all\"\n";
        let setting = Setting {
            key: "format".into(),
            value: "\"$all\"".into(),
            line: 1,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let after =
            String::from_utf8(replace_setting(Format::Toml, before, &setting, "$all").unwrap())
                .unwrap();
        assert!(after.contains("format = \"$all\""));
    }

    #[test]
    fn replace_toml_requotes_escapes_backslashes() {
        let before = b"format = \"$all\"\n";
        let setting = Setting {
            key: "format".into(),
            value: "\"$all\"".into(),
            line: 1,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let after = String::from_utf8(
            replace_setting(Format::Toml, before, &setting, r"C:\Users\ada").unwrap(),
        )
        .unwrap();
        assert!(after.contains("\"C:\\\\Users\\\\ada\""));
    }

    #[test]
    fn replace_toml_keeps_bare_literals() {
        let before = b"scan_timeout = 5000\n";
        let setting = Setting {
            key: "scan_timeout".into(),
            value: "5000".into(),
            line: 1,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let after =
            String::from_utf8(replace_setting(Format::Toml, before, &setting, "6000").unwrap())
                .unwrap();
        assert!(after.contains("scan_timeout = 6000"));
    }

    #[test]
    fn ssh_bare_token_lines_produce_no_settings() {
        let bare_host = parse_settings(Format::Ssh, b"Host example\n  StrictHostKeyChecking\n");
        assert!(
            bare_host.is_empty(),
            "bare token inside Host block must be skipped"
        );
        let bare_line = parse_settings(Format::Ssh, b"Host\n");
        assert!(bare_line.is_empty(), "Host with no value must be skipped");
    }

    #[test]
    fn replace_ssh_keeps_indent_and_key() {
        let before = b"Host example\n  User ada\n";
        let setting = Setting {
            key: "Host example.User".into(),
            value: "ada".into(),
            line: 2,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let after =
            String::from_utf8(replace_setting(Format::Ssh, before, &setting, "grace").unwrap())
                .unwrap();
        assert!(after.contains("  User grace"));
    }

    #[test]
    fn url_username_without_password_is_not_sensitive() {
        let s = parse_settings(
            Format::Ini,
            b"[global]\nindex-url=https://user@example.test/simple\n",
        );
        assert_eq!(s.len(), 1);
        assert!(!s[0].sensitive && s[0].editable);
    }

    #[test]
    fn refuses_sensitive_or_out_of_range() {
        let s = Setting {
            key: "x".into(),
            value: "y".into(),
            line: 1,
            occ: 1,
            editable: false,
            sensitive: true,
        };
        assert!(replace_setting(Format::Git, b"x = y\n", &s, "z").is_err());
        let s = Setting {
            key: "x".into(),
            value: "y".into(),
            line: 99,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        assert!(replace_setting(Format::Git, b"x = y\n", &s, "z").is_err());
    }

    #[test]
    fn replace_keeps_git_delimiter_spacing() {
        let before = b"[user]\n\tname = Ada\n";
        let setting = Setting {
            key: "user.name".into(),
            value: "Ada".into(),
            line: 2,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let after =
            String::from_utf8(replace_setting(Format::Git, before, &setting, "Grace").unwrap())
                .unwrap();
        assert_eq!(after, "[user]\n\tname = Grace\n");
        // 紧凑风格同样保留
        let compact = b"[user]\nname=Ada\n";
        let setting = Setting {
            key: "user.name".into(),
            value: "Ada".into(),
            line: 2,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let after =
            String::from_utf8(replace_setting(Format::Git, compact, &setting, "Grace").unwrap())
                .unwrap();
        assert_eq!(after, "[user]\nname=Grace\n");
    }

    #[test]
    fn replace_keeps_ini_colon_separator() {
        let before = b"[global]\ntimeout: 30\n";
        let setting = Setting {
            key: "global.timeout".into(),
            value: "30".into(),
            line: 2,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let after =
            String::from_utf8(replace_setting(Format::Ini, before, &setting, "60").unwrap())
                .unwrap();
        assert_eq!(after, "[global]\ntimeout: 60\n");
    }

    #[test]
    fn replace_does_not_swallow_value_starting_with_separator_char() {
        // 值以 = 开头：分隔符只能取第一个 =，不得吞入值
        let before = b"[user]\nname==Ada\n";
        let settings = parse_settings(Format::Git, before);
        assert_eq!(settings[0].value, "=Ada");
        let after =
            String::from_utf8(replace_setting(Format::Git, before, &settings[0], "Grace").unwrap())
                .unwrap();
        assert_eq!(after, "[user]\nname=Grace\n");
        // INI 值以 : 开头同理
        let ini = b"[global]\nurl :://host/x\n";
        let settings = parse_settings(Format::Ini, ini);
        assert_eq!(settings[0].value, "://host/x");
        let after =
            String::from_utf8(replace_setting(Format::Ini, ini, &settings[0], "://new/y").unwrap())
                .unwrap();
        assert_eq!(after, "[global]\nurl :://new/y\n");
    }

    #[test]
    fn replace_toml_preserves_inline_comment() {
        let before = b"format = \"$all\" # keep me\n";
        let setting = Setting {
            key: "format".into(),
            value: "\"$all\"".into(),
            line: 1,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let after =
            String::from_utf8(replace_setting(Format::Toml, before, &setting, "$all").unwrap())
                .unwrap();
        assert_eq!(after, "format = \"$all\" # keep me\n");
    }

    #[test]
    fn parse_toml_strips_inline_comment_but_not_hash_in_string() {
        let s = parse_settings(
            Format::Toml,
            b"format = \"$all\" # keep me\nkey = \"a#b\"\n",
        );
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].value, "\"$all\"", "行内注释不得并入值");
        assert_eq!(s[1].value, "\"a#b\"", "字符串内的 # 不得被剥离");
    }

    #[test]
    fn replace_rejects_non_utf8_content_without_corruption() {
        let content: Vec<u8> = b"# \xfe\xff comment\n[user]\nname = Ada\n".to_vec();
        let setting = Setting {
            key: "user.name".into(),
            value: "Ada".into(),
            line: 3,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let err = replace_setting(Format::Git, &content, &setting, "Grace")
            .expect_err("非 UTF-8 文件必须拒绝行内编辑");
        assert!(err.contains("UTF-8"), "错误信息应说明 UTF-8 原因: {err}");
        assert_eq!(content[2..4], [0xfe, 0xff], "原始字节不得被改写");
        assert!(
            relocate_setting(Format::Git, &content, &setting).is_err(),
            "relocate 也必须拒绝非 UTF-8 内容"
        );
    }

    #[test]
    fn relocate_finds_sole_occurrence_when_occ_changed() {
        // 扫描时有两条 user.name；外部删除第一条后只剩一条
        let content = b"[user]\nname = Grace\n";
        let stale = Setting {
            key: "user.name".into(),
            value: "Grace".into(),
            line: 3,
            occ: 2,
            editable: true,
            sensitive: false,
        };
        let found = relocate_setting(Format::Git, content, &stale)
            .unwrap()
            .expect("sole remaining duplicate must still be locatable");
        assert_eq!(found.occ, 1);
    }

    #[test]
    fn relocate_matches_occurrence_after_external_insert() {
        // 扫描时 user.name 在第 2 行（occ 1）；外部在其前插入同名键
        let content = b"[user]\nname = Newcomer\nname = Ada\n";
        let stale = Setting {
            key: "user.name".into(),
            value: "Ada".into(),
            line: 2,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let found = relocate_setting(Format::Git, content, &stale)
            .unwrap()
            .expect("must resolve by occurrence number");
        assert_eq!(
            found.line, 3,
            "must resolve to the originally selected entry"
        );
        assert_eq!(found.value, "Ada");
    }

    #[test]
    fn relocate_rejects_ambiguous_duplicates() {
        // 两条同名键且值相同：occ 与值都无法区分 → 歧义
        let same_value = b"[user]\nname = Grace\nname = Grace\n";
        let stale = Setting {
            key: "user.name".into(),
            value: "Grace".into(),
            line: 2,
            occ: 7,
            editable: true,
            sensitive: false,
        };
        assert!(
            relocate_setting(Format::Git, same_value, &stale).is_err(),
            "indistinguishable duplicates must be rejected"
        );
        // 两条同名键值都不同，但选中的值已不存在（外部改过值）→ 歧义
        let changed_values = b"[user]\nname = Newcomer\nname = Ada\n";
        let stale = Setting {
            key: "user.name".into(),
            value: "Grace".into(),
            line: 2,
            occ: 7,
            editable: true,
            sensitive: false,
        };
        assert!(relocate_setting(Format::Git, changed_values, &stale).is_err());
        // 消失：同 key 完全不存在
        let gone = Setting {
            key: "user.email".into(),
            value: "a@b.c".into(),
            line: 5,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        assert!(relocate_setting(Format::Git, changed_values, &gone)
            .unwrap()
            .is_none());
    }

    #[test]
    fn relocate_matches_by_value_when_occ_drifted() {
        // 外部在选中条目前插入了同名键：occ 已漂移，值仍唯一 → 定位到原条目
        let content = b"[user]\nname = Newcomer\nname = Ada\n";
        let stale = Setting {
            key: "user.name".into(),
            value: "Ada".into(),
            line: 2,
            occ: 1,
            editable: true,
            sensitive: false,
        };
        let found = relocate_setting(Format::Git, content, &stale)
            .unwrap()
            .expect("must resolve by value");
        assert_eq!(
            found.line, 3,
            "must not fall back to the inserted duplicate"
        );
        assert_eq!(found.occ, 2);
    }

    #[test]
    fn relocate_duplicate_ssh_hosts_by_occurrence() {
        let content = b"Host example\n  User ada\nHost example\n  User grace\n";
        let stale = Setting {
            key: "Host example.User".into(),
            value: "grace".into(),
            line: 4,
            occ: 2,
            editable: true,
            sensitive: false,
        };
        let found = relocate_setting(Format::Ssh, content, &stale)
            .unwrap()
            .expect("duplicate Host must be resolved by occurrence");
        assert_eq!(found.line, 4);
        assert_eq!(found.value, "grace");
    }
}
