#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Catalog {
    pub chinese: bool,
}

pub fn detect() -> Catalog {
    let lang = std::env::var("LANG").unwrap_or_default().to_lowercase();
    Catalog {
        chinese: lang.starts_with("zh"),
    }
}

impl Catalog {
    pub fn text<'a>(&self, en: &'a str, zh: &'a str) -> &'a str {
        if self.chinese {
            zh
        } else {
            en
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn zh_locale_selects_chinese() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = std::env::var_os("LANG");
        std::env::set_var("LANG", "zh_CN.UTF-8");
        assert!(detect().chinese);
        assert_eq!(detect().text("No changes", "没有更改"), "没有更改");
        restore(&saved);
    }

    #[test]
    fn non_zh_locale_selects_english() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = std::env::var_os("LANG");
        std::env::set_var("LANG", "en_US.UTF-8");
        assert!(!detect().chinese);
        assert_eq!(detect().text("No changes", "没有更改"), "No changes");
        restore(&saved);
    }

    fn restore(saved: &Option<std::ffi::OsString>) {
        match saved {
            Some(v) => std::env::set_var("LANG", v),
            None => std::env::remove_var("LANG"),
        }
    }
}
