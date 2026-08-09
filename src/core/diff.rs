use similar::TextDiff;

pub fn simple_diff(before: &[u8], after: &[u8]) -> String {
    if before == after {
        return "--- current\n+++ proposed\n(no changes)\n".to_string();
    }
    let before_s = String::from_utf8_lossy(before).into_owned();
    let after_s = String::from_utf8_lossy(after).into_owned();
    let diff = TextDiff::from_lines(&before_s, &after_s);
    let unified = diff
        .unified_diff()
        .context_radius(3)
        .header("current", "proposed")
        .to_string();
    if unified.is_empty() {
        return "--- current\n+++ proposed\n(end-of-file newline changed)\n".to_string();
    }
    let before_nl = before.ends_with(b"\n");
    let after_nl = after.ends_with(b"\n");
    let mut result = unified;
    if before_nl != after_nl {
        result.push_str(&format!(
            "(end-of-file newline {})\n",
            if after_nl { "added" } else { "removed" }
        ));
    }
    sanitize(&result)
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c == '\n' || c == '\t' || c >= ' ' {
                c
            } else {
                '\u{FFFD}'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_input_reports_no_changes() {
        let out = simple_diff(b"a\nb\n", b"a\nb\n");
        assert!(out.contains("(no changes)"));
    }

    #[test]
    fn eof_newline_change_is_detected() {
        let out = simple_diff(b"a\nb\n", b"a\nb");
        assert!(out.contains("end-of-file newline"));
    }

    #[test]
    fn unified_diff_shows_hunks() {
        let out = simple_diff(b"x = 1\n", b"x = 2\n");
        assert!(out.contains("-x = 1"));
        assert!(out.contains("+x = 2"));
        assert!(out.contains("current"));
        assert!(out.contains("proposed"));
    }

    #[test]
    fn control_characters_are_sanitized() {
        let out = simple_diff(b"a\x00\x01b\n", b"c\n");
        assert!(!out.contains('\x00') && !out.contains('\x01'));
    }
}
