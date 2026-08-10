use crate::core::{self, Manager};
use crate::domain::{Application, Capability, Setting, Source};
use crate::i18n::Catalog;
use crate::parse::{parse_settings, relocate_setting, replace_setting};
use crate::tui::keymap::{self, Action};
use core::diff::simple_diff;
use crossterm::event::{self, Event, KeyEvent};
use std::io;
use std::path::Path;

use super::clamp_i;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Focus {
    Apps,
    Sources,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Prompt {
    None,
    Search,
    Value,
    Confirm,
}

pub struct App {
    pub apps: Vec<Application>,
    pub manager: Manager,
    pub lang: Catalog,
    pub app_index: usize,
    pub setting_index: usize,
    pub source_index: usize,
    pub focus: Focus,
    pub prompt: Prompt,
    pub input: String,
    pub filter: String,
    pub status: String,
    pub diff: String,
    pub pending: Option<core::Change>,
    pub diff_offset: usize,
    pub width: u16,
    pub height: u16,
    pub quit: bool,
}

pub fn run_tui(apps: Vec<Application>, manager: Manager, lang: Catalog) -> Result<(), String> {
    let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
    let mut terminal = ratatui::Terminal::new(backend).map_err(|e| e.to_string())?;
    crossterm::terminal::enable_raw_mode().map_err(|e| e.to_string())?;
    crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)
        .map_err(|e| e.to_string())?;
    let mut app = App::new(apps, manager, lang);
    let result = run_loop(&mut terminal, &mut app);
    crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen).ok();
    crossterm::terminal::disable_raw_mode().ok();
    result
}

fn run_loop(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), String> {
    loop {
        terminal
            .draw(|frame| app.render(frame))
            .map_err(|e| e.to_string())?;
        match event::read().map_err(|e| e.to_string())? {
            Event::Key(k) => {
                if app.prompt == Prompt::Confirm {
                    app.handle_confirm(k);
                } else {
                    app.handle_key(k);
                }
            }
            Event::Resize(w, h) => {
                app.width = w;
                app.height = h;
            }
            _ => {}
        }
        if app.quit {
            return Ok(());
        }
    }
}

impl App {
    pub fn new(apps: Vec<Application>, manager: Manager, lang: Catalog) -> App {
        App {
            apps,
            manager,
            lang,
            app_index: 0,
            setting_index: 0,
            source_index: 0,
            focus: Focus::Apps,
            prompt: Prompt::None,
            input: String::new(),
            filter: String::new(),
            status: String::new(),
            diff: String::new(),
            pending: None,
            diff_offset: 0,
            width: 80,
            height: 24,
            quit: false,
        }
    }

    pub(crate) fn filtered(&self) -> Vec<&Application> {
        if self.filter.is_empty() {
            return self.apps.iter().collect();
        }
        let needle = self.filter.to_lowercase();
        self.apps
            .iter()
            .filter(|a| {
                let hay = format!("{} {} {}", a.name, a.name_zh, a.id).to_lowercase();
                hay.contains(&needle)
            })
            .collect()
    }

    pub(crate) fn current_app(&self) -> Option<&Application> {
        let list = self.filtered();
        if list.is_empty() {
            return None;
        }
        Some(list[self.app_index.min(list.len() - 1)])
    }

    pub(crate) fn current_source(&self) -> Option<&Source> {
        self.current_app()?.sources.get(self.source_index)
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.quit {
            return;
        }
        match self.prompt {
            Prompt::None => self.handle_normal(key),
            Prompt::Search | Prompt::Value => self.handle_text_input(key),
            Prompt::Confirm => self.handle_confirm(key),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) {
        match keymap::normal_action(key) {
            Action::Quit => self.quit = true,
            Action::Move(delta) => self.move_selection(delta),
            Action::FocusUp => self.focus_up(),
            Action::FocusDown => self.focus_down(),
            Action::Search => {
                self.prompt = Prompt::Search;
                self.input = self.filter.clone();
            }
            Action::Set => self.start_structured(),
            Action::Edit => {
                if let Err(e) = self.start_editor() {
                    self.status = format!("! {e}");
                }
            }
            Action::Restore => self.start_restore(),
            _ => {}
        }
    }

    fn focus_up(&mut self) {
        match self.focus {
            Focus::Settings => self.focus = Focus::Sources,
            Focus::Sources => self.focus = Focus::Apps,
            Focus::Apps => {}
        }
    }

    fn focus_down(&mut self) {
        match self.focus {
            Focus::Apps => self.enter_sources(),
            Focus::Sources => self.enter_settings(),
            Focus::Settings => {}
        }
    }

    fn enter_sources(&mut self) {
        if self.current_app().is_some() {
            self.focus = Focus::Sources;
            self.source_index = 0;
        }
    }

    fn enter_settings(&mut self) {
        let Some(source) = self.current_source() else {
            return;
        };
        if !source.settings.is_empty() {
            self.focus = Focus::Settings;
            self.setting_index = 0;
        } else {
            self.status = self
                .lang
                .text(
                    "No structured settings; press e to edit a staged copy",
                    "没有结构化设置；按 e 编辑暂存副本",
                )
                .into();
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match self.focus {
            Focus::Apps => {
                let list = self.filtered();
                if list.is_empty() {
                    return;
                }
                self.app_index =
                    clamp_i(self.app_index as isize + delta, 0, list.len() - 1) as usize;
                self.source_index = 0;
                self.setting_index = 0;
            }
            Focus::Sources => {
                let Some(app) = self.current_app() else {
                    return;
                };
                if app.sources.is_empty() {
                    return;
                }
                self.source_index =
                    clamp_i(self.source_index as isize + delta, 0, app.sources.len() - 1) as usize;
                self.setting_index = 0;
            }
            Focus::Settings => {
                let Some(source) = self.current_source() else {
                    return;
                };
                if source.settings.is_empty() {
                    return;
                }
                self.setting_index = clamp_i(
                    self.setting_index as isize + delta,
                    0,
                    source.settings.len() - 1,
                ) as usize;
            }
        }
    }

    fn selection(&self) -> Option<(&Application, &Source, Option<&Setting>)> {
        if self.focus == Focus::Apps {
            return None;
        }
        let app = self.current_app()?;
        let source = app.sources.get(self.source_index)?;
        let setting = if self.focus == Focus::Settings {
            source.settings.get(self.setting_index)
        } else {
            None
        };
        Some((app, source, setting))
    }

    fn start_structured(&mut self) {
        let Some((_app, _source, setting)) = self.selection() else {
            self.status = self
                .lang
                .text("Enter a configuration source first", "请先进入配置源")
                .into();
            return;
        };
        let Some(setting) = setting else {
            self.status = self
                .lang
                .text("Select an editable setting", "请选择可编辑的设置")
                .into();
            return;
        };
        if !setting.editable {
            self.status = self
                .lang
                .text(
                    "Sensitive values are redacted and not edited inline",
                    "敏感值已隐藏，不能行内编辑",
                )
                .into();
            return;
        }
        let value = setting.value.clone();
        self.prompt = Prompt::Value;
        self.input = value;
    }

    fn finish_structured(&mut self) {
        let Some((_app, source, setting)) = self.selection() else {
            self.prompt = Prompt::None;
            return;
        };
        let Some(setting) = setting else {
            self.prompt = Prompt::None;
            return;
        };
        let change = match self.manager.prepare(Path::new(&source.path), source.format) {
            Ok(c) => c,
            Err(e) => {
                self.prompt = Prompt::None;
                self.status = format!("! {e}");
                return;
            }
        };
        // 以 prepare 时暂存的副本为基准做替换与 diff，避免目标文件在准备期间被修改
        let original = std::fs::read(&change.stage).unwrap_or_default();
        // 扫描时保存的行号可能已过期（文件被外部修改过），按稳定标识
        // （key + 出现序号 + 原值）在当前内容中重新定位；歧义时拒绝并提示重新扫描
        let located = relocate_setting(source.format, &original, setting);
        let current = match located {
            Ok(Some(s)) => s,
            Ok(None) => {
                let _ = self.manager.discard(&change);
                self.prompt = Prompt::None;
                self.status = self
                    .lang
                    .text(
                        "Setting no longer present in file; re-scan and try again",
                        "该设置已不在文件中；请重新扫描后重试",
                    )
                    .into();
                return;
            }
            Err(_) => {
                let _ = self.manager.discard(&change);
                self.prompt = Prompt::None;
                self.status = self
                    .lang
                    .text(
                        "Ambiguous duplicate setting; re-scan and try again",
                        "存在无法区分的同名设置；请重新扫描后重试",
                    )
                    .into();
                return;
            }
        };
        let content = match replace_setting(source.format, &original, &current, &self.input) {
            Ok(c) => c,
            Err(e) => {
                let _ = self.manager.discard(&change);
                self.prompt = Prompt::None;
                self.status = format!("! {e}");
                return;
            }
        };
        if let Err(e) = std::fs::write(&change.stage, &content) {
            let _ = self.manager.discard(&change);
            self.prompt = Prompt::None;
            self.status = format!("! {e}");
            return;
        }
        self.pending = Some(change);
        self.diff = simple_diff(&original, &content);
        self.prompt = Prompt::Confirm;
        self.input.clear();
        self.diff_offset = 0;
    }

    fn start_restore(&mut self) {
        let Some((_app, source, _)) = self.selection() else {
            self.status = self
                .lang
                .text("Enter a configuration source first", "请先进入配置源")
                .into();
            return;
        };
        if !source.exists {
            self.status = self
                .lang
                .text(
                    "Select an existing configuration file",
                    "请选择已有的配置文件",
                )
                .into();
            return;
        }
        let path = source.resolved.as_ref().unwrap_or(&source.path);
        let change = match self.manager.prepare_restore(Path::new(path), source.format) {
            Ok(c) => c,
            Err(e) => {
                self.status = format!("! {e}");
                return;
            }
        };
        let after = std::fs::read(&change.stage).unwrap_or_default();
        let before = std::fs::read(&change.target).unwrap_or_default();
        self.pending = Some(change);
        self.diff = simple_diff(&before, &after);
        self.prompt = Prompt::Confirm;
        self.diff_offset = 0;
    }

    fn handle_confirm(&mut self, key: KeyEvent) {
        match keymap::confirm_action(key) {
            Action::Quit => {
                if let Some(change) = self.pending.take() {
                    let _ = self.manager.discard(&change);
                }
                self.prompt = Prompt::None;
                self.diff.clear();
                self.diff_offset = 0;
                self.quit = true;
            }
            Action::Move(delta) => {
                self.diff_offset = if delta < 0 {
                    self.diff_offset.saturating_sub(1)
                } else {
                    self.diff_offset
                        .saturating_add(1)
                        .min(self.max_diff_offset())
                };
            }
            Action::PgUp => {
                self.diff_offset = self.diff_offset.saturating_sub(self.diff_page_size());
            }
            Action::PgDn => {
                self.diff_offset = self
                    .diff_offset
                    .saturating_add(self.diff_page_size())
                    .min(self.max_diff_offset());
            }
            Action::Apply => {
                let pending = self.pending.take();
                if let Some(change) = pending {
                    match self.manager.apply(&change) {
                        Ok(result) => {
                            self.status = if let Some(w) = result.warning {
                                format!("! {w}")
                            } else {
                                self.lang
                                    .text(
                                        "Applied safely; snapshot created",
                                        "已安全应用并创建快照",
                                    )
                                    .into()
                            };
                            self.refresh();
                        }
                        Err(e) => self.status = format!("! {e}"),
                    }
                }
                self.prompt = Prompt::None;
                self.diff.clear();
                self.diff_offset = 0;
            }
            Action::Reject => {
                if let Some(change) = self.pending.take() {
                    let _ = self.manager.discard(&change);
                }
                self.prompt = Prompt::None;
                self.diff.clear();
                self.diff_offset = 0;
                self.status = self.lang.text("Discarded", "已放弃").into();
            }
            _ => {}
        }
    }

    fn handle_text_input(&mut self, key: KeyEvent) {
        match keymap::text_action(key) {
            Action::Quit => {
                if let Some(change) = self.pending.take() {
                    let _ = self.manager.discard(&change);
                }
                self.quit = true;
            }
            Action::Cancel => {
                self.prompt = Prompt::None;
                self.input.clear();
            }
            Action::Backspace => {
                self.input.pop();
            }
            Action::Submit => {
                if self.prompt == Prompt::Search {
                    self.filter = self.input.clone();
                    self.app_index = 0;
                    self.prompt = Prompt::None;
                } else if self.prompt == Prompt::Value {
                    self.finish_structured();
                }
            }
            Action::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    fn refresh(&mut self) {
        for app in self.apps.iter_mut() {
            let structured = app.capabilities.contains(&Capability::Structured);
            if !structured {
                continue;
            }
            for source in app.sources.iter_mut() {
                if !source.exists {
                    continue;
                }
                let path = source.resolved.as_ref().unwrap_or(&source.path);
                if let Ok(content) = std::fs::read(path) {
                    source.settings = parse_settings(source.format, &content);
                }
            }
        }
    }

    fn start_editor(&mut self) -> Result<(), String> {
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

    pub(crate) fn diff_page_size(&self) -> usize {
        self.height.saturating_sub(4).max(1) as usize
    }

    pub(crate) fn diff_lines(&self) -> usize {
        self.diff.trim_end_matches('\n').split('\n').count()
    }

    pub(crate) fn max_diff_offset(&self) -> usize {
        self.diff_lines().saturating_sub(self.diff_page_size())
    }
}
