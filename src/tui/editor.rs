use super::app::App;
use crate::core::diff::simple_diff;
use crate::tui::app::Prompt;
use std::io;
use std::path::Path;

impl App {
    pub(crate) fn start_editor(&mut self) -> Result<(), String> {
        let Some((_app, source, _)) = self.selection() else {
            self.status = self
                .lang
                .text("Enter a configuration source first", "请先进入配置源")
                .into();
            return Ok(());
        };
        if !source.exists {
            self.status = self
                .lang
                .text(
                    "Select an existing configuration file",
                    "请选择已有的配置文件",
                )
                .into();
            return Ok(());
        }
        let change = self
            .manager
            .prepare(Path::new(&source.path), source.format)?;
        let editor = std::env::var("VISUAL")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("EDITOR").ok().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "vi".to_string());
        let parts = match shell_words::split(&editor) {
            Ok(p) => p,
            Err(e) => {
                let _ = self.manager.discard(&change);
                return Err(e.to_string());
            }
        };
        if parts.is_empty() {
            let _ = self.manager.discard(&change);
            return Err("editor command is empty".into());
        }
        // 挂起 TUI 会话：退出 raw mode 与备用屏
        crossterm::terminal::disable_raw_mode().ok();
        crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen).ok();
        let status = std::process::Command::new(&parts[0])
            .args(&parts[1..])
            .arg(&change.stage)
            .status();
        crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen).ok();
        crossterm::terminal::enable_raw_mode().ok();
        match status {
            Err(e) => {
                let _ = self.manager.discard(&change);
                return Err(e.to_string());
            }
            Ok(st) if !st.success() => {
                let _ = self.manager.discard(&change);
                return Err(format!("editor exited with {st}"));
            }
            Ok(_) => {}
        }
        let after = match std::fs::read(&change.stage) {
            Ok(a) => a,
            Err(e) => {
                let _ = self.manager.discard(&change);
                return Err(e.to_string());
            }
        };
        let before = std::fs::read(&change.target).unwrap_or_default();
        if after == before {
            let _ = self.manager.discard(&change);
            self.status = self.lang.text("No changes", "没有更改").into();
            return Ok(());
        }
        self.pending = Some(change);
        self.diff = simple_diff(&before, &after);
        self.prompt = Prompt::Confirm;
        self.diff_offset = 0;
        Ok(())
    }
}
