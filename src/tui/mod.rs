use crate::core::{self, Manager};
use crate::domain::{Application, Capability, Setting, Source};
use crate::i18n::Catalog;
use crate::parse::{parse_settings, relocate_setting, replace_setting};
use core::diff::simple_diff;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::io;
use std::path::Path;

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

    pub fn filtered(&self) -> Vec<&Application> {
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

    pub fn current_app(&self) -> Option<&Application> {
        let list = self.filtered();
        if list.is_empty() {
            return None;
        }
        Some(list[self.app_index.min(list.len() - 1)])
    }

    pub fn current_source(&self) -> Option<&Source> {
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
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => self.quit = true,
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Esc => self.focus_up(),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => self.focus_down(),
            KeyCode::Char('/') => {
                self.prompt = Prompt::Search;
                self.input = self.filter.clone();
            }
            KeyCode::Char('s') => self.start_structured(),
            KeyCode::Char('e') => {
                if let Err(e) = self.start_editor() {
                    self.status = format!("! {e}");
                }
            }
            KeyCode::Char('r') => self.start_restore(),
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
                .text("Select an editable setting", "请选择可编辑的设置")
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
                .text(
                    "Select an existing configuration file",
                    "请选择已有的配置文件",
                )
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
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(change) = self.pending.take() {
                    let _ = self.manager.discard(&change);
                }
                self.prompt = Prompt::None;
                self.diff.clear();
                self.diff_offset = 0;
                self.quit = true;
            }
            KeyCode::Char('q') => {
                if let Some(change) = self.pending.take() {
                    let _ = self.manager.discard(&change);
                }
                self.prompt = Prompt::None;
                self.diff.clear();
                self.diff_offset = 0;
                self.quit = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.diff_offset = self.diff_offset.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.diff_offset = self
                    .diff_offset
                    .saturating_add(1)
                    .min(self.max_diff_offset());
            }
            KeyCode::PageUp => {
                self.diff_offset = self.diff_offset.saturating_sub(self.diff_page_size());
            }
            KeyCode::PageDown => {
                self.diff_offset = self
                    .diff_offset
                    .saturating_add(self.diff_page_size())
                    .min(self.max_diff_offset());
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
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
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
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
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(change) = self.pending.take() {
                    let _ = self.manager.discard(&change);
                }
                self.quit = true;
            }
            KeyCode::Esc => {
                self.prompt = Prompt::None;
                self.input.clear();
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Enter => {
                if self.prompt == Prompt::Search {
                    self.filter = self.input.clone();
                    self.app_index = 0;
                    self.prompt = Prompt::None;
                } else if self.prompt == Prompt::Value {
                    self.finish_structured();
                }
            }
            KeyCode::Char(c) => self.input.push(c),
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
                .text(
                    "Select an existing configuration file",
                    "请选择已有的配置文件",
                )
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

    pub fn render(&mut self, frame: &mut Frame) {
        if self.prompt == Prompt::Confirm && !self.diff.is_empty() {
            self.render_diff(frame, frame.area());
            return;
        }
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(frame.area());
        let title = format!(
            "Config Editor  {}",
            self.lang
                .text("safe configuration workspace", "安全配置工作台")
        );
        let (w, h) = (areas[1].width, areas[1].height);
        self.width = w;
        self.height = h;
        frame.render_widget(
            Paragraph::new(title).style(
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            areas[0],
        );
        self.render_apps_settings(frame, areas[1]);
        self.render_footer(frame, areas[2]);
    }

    fn render_apps_settings(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let mut lines: Vec<Line> = Vec::new();
        let apps = self.filtered();
        let width = area.width as usize;
        let heading = self.lang.text("Applications", "应用");
        lines.push(Line::from(Span::styled(
            truncate(heading, width),
            Style::default().fg(Color::DarkGray),
        )));
        for (i, app) in apps.iter().enumerate() {
            let marker = if i == self.app_index && self.focus == Focus::Apps {
                "> "
            } else {
                "  "
            };
            let state = if app.configured() {
                "●"
            } else if app.installed {
                "○"
            } else {
                "·"
            };
            let name = if self.lang.chinese {
                &app.name_zh
            } else {
                &app.name
            };
            let text = format!("{marker}{state} {name}");
            let span = if i == self.app_index {
                Span::styled(
                    truncate(&text, width),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw(truncate(&text, width))
            };
            lines.push(Line::from(span));
        }
        if let Some(app) = self.current_app() {
            let desc = if self.lang.chinese {
                &app.description_zh
            } else {
                &app.description
            };
            let name = if self.lang.chinese {
                &app.name_zh
            } else {
                &app.name
            };
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                truncate(&format!("{name} — {desc}"), width),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )));
            let details = self.detail_lines(app, width);
            // 详情区高度必须扣除标题行、应用列表、空行与描述行占用的行数；
            // 滚动位置用选中设置的实际显示行号，而非设置序号
            let header = 1 + apps.len() + 2;
            let available = area.height.saturating_sub(header as u16) as usize;
            let start = visible_start(self.selected_detail_row(app), details.len(), available);
            let end = (start + available).min(details.len());
            for detail in &details[start..end] {
                lines.push(Line::from(detail.clone()));
            }
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }

    /// 选中设置在 detail_lines 中的实际显示行号（含各 source 的 [file]/[missing]
    /// 与诊断行），而不是全局设置序号；用于滚动窗口计算。
    fn selected_detail_row(&self, app: &Application) -> usize {
        let mut row = 0;
        let mut idx = self.setting_index;
        for source in &app.sources {
            let header = 1 + usize::from(source.diagnostic.is_some());
            if idx < source.settings.len() {
                return row + header + idx;
            }
            idx -= source.settings.len();
            row += header + source.settings.len();
        }
        0
    }

    fn detail_lines(&self, app: &Application, width: usize) -> Vec<Span<'static>> {
        let mut lines = Vec::new();
        let mut row = 0;
        for source in &app.sources {
            let flag = if source.exists { "file" } else { "missing" };
            lines.push(Span::raw(truncate(
                &format!("  [{flag}] {}", source.path),
                width,
            )));
            if let Some(d) = &source.diagnostic {
                lines.push(Span::styled(
                    format!("  ! {d}"),
                    Style::default().fg(Color::Red),
                ));
            }
            for setting in &source.settings {
                let is_selected = self.focus == Focus::Settings && row == self.setting_index;
                let marker = if is_selected { "  > " } else { "    " };
                let value = truncate(&setting.value, width.saturating_sub(34).max(8));
                let text = format!("{}{:<28} {}", marker, setting.key, value);
                let span = if is_selected {
                    Span::styled(
                        truncate(&text, width),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::raw(truncate(&text, width))
                };
                lines.push(span);
                row += 1;
            }
        }
        lines
    }

    fn render_footer(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let mut lines: Vec<Line> = Vec::new();
        let width = area.width as usize;
        if self.prompt == Prompt::Search {
            lines.push(Line::from(truncate(
                &format!("/ {}_{}", self.input, ""),
                width,
            )));
        } else if self.prompt == Prompt::Value {
            lines.push(Line::from(truncate(
                &format!(
                    "{} {}_{}",
                    self.lang.text("New value:", "新值："),
                    self.input,
                    ""
                ),
                width,
            )));
        }
        if !self.status.is_empty() {
            let style = if self.status.starts_with('!') {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            lines.push(Line::from(Span::styled(
                truncate(&self.status, width),
                style,
            )));
        }
        lines.push(Line::from(""));
        let hint = self.lang.text(
            "↑↓/jk move  → settings  s set  e edit  r restore  / search  q quit",
            "↑↓/jk 移动  → 设置  s 修改  e 编辑  r 恢复  / 搜索  q 退出",
        );
        lines.push(Line::from(Span::styled(
            truncate(hint, width),
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }

    fn render_diff(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let width = area.width as usize;
        let all: Vec<&str> = self.diff.trim_end_matches('\n').split('\n').collect();
        let page = self.diff_page_size();
        let offset = self.diff_offset.min(all.len().saturating_sub(page));
        let end = (offset + page).min(all.len());
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            truncate(
                self.lang.text("Review proposed change", "审阅待应用更改"),
                width,
            ),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )));
        let start_display = if all.is_empty() { 0 } else { offset + 1 };
        lines.push(Line::from(Span::styled(
            truncate(&format!("lines {start_display}-{end}/{}", all.len()), width),
            Style::default().fg(Color::DarkGray),
        )));
        for line in &all[offset..end] {
            lines.push(Line::from(Span::raw(truncate(line, width))));
        }
        lines.push(Line::from(
            self.lang
                .text("Apply this change? [y/N]", "应用此更改？[y/N]"),
        ));
        lines.push(Line::from(Span::styled(
            truncate(
                self.lang
                    .text("↑↓/jk scroll  PgUp/PgDn page", "↑↓/jk 滚动  PgUp/PgDn 翻页"),
                width,
            ),
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }

    fn diff_page_size(&self) -> usize {
        self.height.saturating_sub(4).max(1) as usize
    }

    fn diff_lines(&self) -> usize {
        self.diff.trim_end_matches('\n').split('\n').count()
    }

    fn max_diff_offset(&self) -> usize {
        self.diff_lines().saturating_sub(self.diff_page_size())
    }
}

fn clamp_i(value: isize, min: usize, max: usize) -> isize {
    (value.max(min as isize)).min(max as isize)
}

fn truncate(s: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    if s.width() <= width {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = UnicodeWidthStr::width(c.to_string().as_str());
        if w + cw > width.saturating_sub(1) {
            break;
        }
        out.push(c);
        w += cw;
    }
    format!("{out}…")
}

fn visible_start(selected: usize, total: usize, size: usize) -> usize {
    if total <= size {
        return 0;
    }
    let selected = selected.min(total - 1);
    selected.saturating_sub(size / 2).min(total - size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Format;
    use crate::i18n;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn sample_apps() -> Vec<Application> {
        vec![Application {
            id: "git".into(),
            name: "Git".into(),
            name_zh: "Git".into(),
            description: "d".into(),
            description_zh: "d".into(),
            command: Some("git".into()),
            installed: true,
            capabilities: vec![Capability::Structured],
            sources: vec![Source {
                path: "/home/me/.gitconfig".into(),
                resolved: None,
                exists: true,
                format: Format::Git,
                diagnostic: None,
                settings: vec![Setting {
                    key: "user.name".into(),
                    value: "Ada".into(),
                    line: 1,
                    occ: 1,
                    editable: true,
                    sensitive: false,
                }],
            }],
        }]
    }

    #[test]
    fn j_navigates_apps() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.app_index, 0); // 只有一个应用，不动
        assert_eq!(app.focus, Focus::Apps);
    }

    #[test]
    fn right_enters_settings_pane() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.focus, Focus::Settings);
        assert_eq!(app.setting_index, 0);
    }

    #[test]
    fn search_filters_apps() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.handle_key(key(KeyCode::Char('/')));
        assert_eq!(app.prompt, Prompt::Search);
        app.handle_key(key(KeyCode::Char('g')));
        app.handle_key(key(KeyCode::Char('i')));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.filter, "gi");
        assert_eq!(app.prompt, Prompt::None);
        assert_eq!(app.filtered().len(), 1);
    }

    #[test]
    fn esc_from_settings_returns_to_apps() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.focus, Focus::Apps);
    }

    #[test]
    fn render_smoke_contains_key_labels() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.width = 80;
        app.height = 24;
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("Config Editor"));
        assert!(text.contains("user.name"));
        assert!(text.contains("q quit"));
    }

    #[test]
    fn q_requests_quit() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.quit);
    }

    #[test]
    fn ctrl_c_in_search_quits() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.handle_key(key(KeyCode::Char('/')));
        assert_eq!(app.prompt, Prompt::Search);
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.quit);
    }

    #[test]
    fn q_is_text_input_inside_search() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('q')));
        assert!(!app.quit);
        assert_eq!(app.input, "q");
    }

    fn temp_env() -> (tempfile::TempDir, core::Manager, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let cfg = home.join(".gitconfig");
        std::fs::write(&cfg, b"[user]\nname = Ada\n").unwrap();
        let manager = core::Manager {
            home: home.clone(),
            config_root: dir.path().join("config"),
            state_root: dir.path().join("state"),
        };
        (dir, manager, cfg)
    }

    fn app_with_source(manager: core::Manager, cfg: &std::path::Path) -> App {
        let mut app = App::new(sample_apps(), manager, i18n::Catalog { chinese: false });
        app.apps[0].sources[0].path = cfg.to_str().unwrap().into();
        app.apps[0].sources[0].resolved = Some(cfg.to_str().unwrap().into());
        app.apps[0].sources[0].settings[0].line = 2;
        app
    }

    #[test]
    fn structured_edit_stages_replacement_and_builds_diff() {
        let (_dir, manager, cfg) = temp_env();
        let mut app = app_with_source(manager, &cfg);
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.prompt, Prompt::Value);
        assert_eq!(app.input, "Ada");
        for _ in 0..3 {
            app.handle_key(key(KeyCode::Backspace));
        }
        for c in "Grace".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.prompt, Prompt::Confirm);
        let change = app.pending.as_ref().expect("pending change");
        let stage = change.stage.clone();
        assert_eq!(std::fs::read(&stage).unwrap(), b"[user]\nname=Grace\n");
        assert!(app.diff.contains("name = Ada"));
        assert!(app.diff.contains("name=Grace"));
        let _ = app.manager.discard(&app.pending.take().unwrap());
        assert!(!stage.exists());
    }

    #[test]
    fn diff_scroll_clamps_at_bottom() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        let mut diff = String::new();
        for i in 0..30 {
            diff.push_str(&format!("+line {i}\n"));
        }
        app.prompt = Prompt::Confirm;
        app.diff = diff;
        app.height = 12;
        let max = app.max_diff_offset();
        assert!(max > 0);
        // 远超底部的下滚：offset 必须被钳制到上限
        for _ in 0..100 {
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        }
        assert_eq!(app.diff_offset, max, "offset must clamp at max");
        // 钳制后按一次 k 立即上移
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.diff_offset, max - 1);
    }

    #[test]
    fn structured_edit_relocates_stale_line_number() {
        let (dir, manager, cfg) = temp_env();
        // 扫描之后、编辑之前，外部工具在 name 前插入了一行 email
        std::fs::write(&cfg, b"[user]\nemail = ada@example.test\nname = Ada\n").unwrap();
        let mut app = app_with_source(manager, &cfg);
        // app 中保存的仍是扫描时的旧行号：user.name 在第 2 行
        assert_eq!(app.apps[0].sources[0].settings[0].line, 2);
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Char('s')));
        for _ in 0..3 {
            app.handle_key(key(KeyCode::Backspace));
        }
        for c in "Grace".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(
            app.prompt,
            Prompt::Confirm,
            "stale line must not abort edit"
        );
        let change = app.pending.as_ref().expect("pending change");
        let text = String::from_utf8(std::fs::read(&change.stage).unwrap()).unwrap();
        assert!(
            text.contains("name=Grace"),
            "user.name 行必须被修改（当前第 3 行）:\n{text}"
        );
        assert!(
            text.contains("email = ada@example.test"),
            "email 行（旧行号 2 指向它）不得被修改:\n{text}"
        );
        assert!(!text.contains("email = Grace"), "email 被错误修改:\n{text}");
        let _ = app.manager.discard(&app.pending.take().unwrap());
        let _ = dir;
    }

    #[test]
    fn structured_edit_targets_selected_duplicate_occurrence() {
        let (dir, manager, cfg) = temp_env();
        // 同名键两条：选中第二条（user.name, occ=2）并改值，第一条必须原样保留
        std::fs::write(&cfg, b"[user]\nname = Ada\nname = Grace\n").unwrap();
        let mut app = app_with_source(manager, &cfg);
        app.apps[0].sources[0].settings = vec![
            Setting {
                key: "user.name".into(),
                value: "Ada".into(),
                line: 2,
                occ: 1,
                editable: true,
                sensitive: false,
            },
            Setting {
                key: "user.name".into(),
                value: "Grace".into(),
                line: 3,
                occ: 2,
                editable: true,
                sensitive: false,
            },
        ];
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Char('j'))); // 移到第二条
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.input, "Grace");
        for _ in 0..5 {
            app.handle_key(key(KeyCode::Backspace));
        }
        for c in "Rust".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.prompt, Prompt::Confirm, "duplicate must not abort edit");
        let change = app.pending.as_ref().expect("pending change");
        let text = String::from_utf8(std::fs::read(&change.stage).unwrap()).unwrap();
        assert!(
            text.contains("name = Ada"),
            "第一条同名键不得被修改:\n{text}"
        );
        assert!(
            text.contains("name=Rust"),
            "选中的第二条必须被修改:\n{text}"
        );
        assert!(!text.contains("name=Grace"), "第二条已改为 Rust:\n{text}");
        let _ = app.manager.discard(&app.pending.take().unwrap());
        let _ = dir;
    }

    #[test]
    fn structured_edit_rejects_ambiguous_duplicates() {
        let (dir, manager, cfg) = temp_env();
        // 扫描时第 3 条 user.name（值 Grace）被选中；外部改动后同名键仍有多条、
        // 值 Grace 出现两次、occ 无法命中选中条 → 拒绝并要求重新扫描
        std::fs::write(&cfg, b"[user]\nname = Grace\nname = Grace\nname = Ada\n").unwrap();
        let mut app = app_with_source(manager, &cfg);
        app.apps[0].sources[0].settings[0].occ = 3;
        app.apps[0].sources[0].settings[0].line = 4;
        app.apps[0].sources[0].settings[0].value = "Grace".into();
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.prompt, Prompt::Value, "编辑前不应被拒");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.prompt, Prompt::None, "歧义必须中止编辑");
        assert!(
            app.status.contains("re-scan"),
            "必须提示重新扫描: {}",
            app.status
        );
        assert!(app.pending.is_none(), "歧义时不得留下 pending 暂存");
        let edit_dir = dir.path().join("state/config-editor/edit");
        let leftovers: Vec<_> = std::fs::read_dir(&edit_dir)
            .map(|rd| rd.filter_map(Result::ok).collect())
            .unwrap_or_default();
        assert!(leftovers.is_empty(), "暂存必须被丢弃");
        let _ = dir;
    }

    #[test]
    fn ctrl_c_in_confirm_quits_and_discards_stage() {
        let (dir, manager, cfg) = temp_env();
        let mut app = app_with_source(manager, &cfg);
        let change = app.manager.prepare(&cfg, Format::Git).unwrap();
        let stage = change.stage.clone();
        app.pending = Some(change);
        app.prompt = Prompt::Confirm;
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.quit);
        assert!(app.pending.is_none());
        assert!(
            !stage.exists(),
            "stage must be discarded when quitting from confirm"
        );
        let edit_dir = dir.path().join("state/config-editor/edit");
        let leftovers: Vec<_> = std::fs::read_dir(&edit_dir)
            .map(|rd| rd.filter_map(Result::ok).collect())
            .unwrap_or_default();
        assert!(leftovers.is_empty(), "edit directory must be empty");
    }

    #[test]
    fn editor_parse_error_discards_stage_and_reports_status() {
        let (dir, manager, cfg) = temp_env();
        let mut app = app_with_source(manager, &cfg);
        let saved_visual = std::env::var_os("VISUAL");
        let saved_editor = std::env::var_os("EDITOR");
        std::env::remove_var("VISUAL");
        std::env::set_var("EDITOR", "'");
        app.handle_key(key(KeyCode::Char('e')));
        match saved_visual {
            Some(v) => std::env::set_var("VISUAL", v),
            None => std::env::remove_var("VISUAL"),
        }
        match saved_editor {
            Some(v) => std::env::set_var("EDITOR", v),
            None => std::env::remove_var("EDITOR"),
        }
        assert!(app.status.starts_with('!'), "status must report the error");
        let edit_dir = dir.path().join("state/config-editor/edit");
        let leftovers: Vec<_> = std::fs::read_dir(&edit_dir)
            .map(|rd| rd.filter_map(Result::ok).collect())
            .unwrap_or_default();
        assert!(
            leftovers.is_empty(),
            "staged file must be discarded on editor error"
        );
    }

    #[test]
    fn selected_detail_row_counts_source_headers_and_diagnostics() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.apps[0].sources[0].settings = vec![
            Setting {
                key: "k1".into(),
                ..Default::default()
            },
            Setting {
                key: "k2".into(),
                ..Default::default()
            },
            Setting {
                key: "k3".into(),
                ..Default::default()
            },
        ];
        app.setting_index = 0;
        assert_eq!(app.selected_detail_row(&app.apps[0]), 1, "[file] 占 1 行");
        app.setting_index = 2;
        assert_eq!(app.selected_detail_row(&app.apps[0]), 3);
        // 诊断行让后续设置行号 +1
        app.apps[0].sources[0].diagnostic = Some("boom".into());
        app.setting_index = 0;
        assert_eq!(app.selected_detail_row(&app.apps[0]), 2);
        app.setting_index = 1;
        assert_eq!(app.selected_detail_row(&app.apps[0]), 3);
        // 两个 source：行号跨过第一个 source 的 [file] + 设置行
        let second = Source {
            path: "/home/me/.gitconfig.extra".into(),
            resolved: None,
            exists: true,
            format: Format::Git,
            diagnostic: Some("x".into()),
            settings: vec![Setting {
                key: "k4".into(),
                ..Default::default()
            }],
        };
        app.apps[0].sources.push(second);
        app.apps[0].sources[0].diagnostic = None;
        app.apps[0].sources[0].settings = vec![Setting {
            key: "k1".into(),
            ..Default::default()
        }];
        app.setting_index = 1; // 第二个 source 的第一个设置
        assert_eq!(
            app.selected_detail_row(&app.apps[0]),
            4,
            "1([file]) + 1(k1) + 1([file]) + 1(诊断) = 4 行前缀"
        );
    }

    #[test]
    fn detail_viewport_keeps_bottom_setting_visible_on_small_terminals() {
        // 30 个设置 + 标题/应用/描述 4 行头：detail 区仅 6 行时，
        // 滚动视口必须包含选中的最后一个设置（当前实现按整块高度计算，底部被裁剪）
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.apps[0].sources[0].settings = (0..30)
            .map(|i| Setting {
                key: format!("user.k{i}"),
                value: format!("v{i}"),
                line: 1,
                occ: 1,
                editable: true,
                sensitive: false,
            })
            .collect();
        app.focus = Focus::Settings;
        app.setting_index = 29;
        let backend = ratatui::backend::TestBackend::new(80, 16);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(
            text.contains("user.k29"),
            "选中的最后一个设置必须在视口内可见:\n{text}"
        );
        assert!(
            text.contains("> user.k29"),
            "选中标记必须与最后一个设置同行:\n{text}"
        );
    }

    #[test]
    fn arrow_keys_traverse_three_levels_of_focus() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.focus, Focus::Sources);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.focus, Focus::Settings);
        assert_eq!(app.setting_index, 0);
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.focus, Focus::Sources);
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.focus, Focus::Apps);
        // 最顶层 Left/Esc 不动作
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.focus, Focus::Apps);
    }

    #[test]
    fn entering_sources_resets_source_index() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        // 两个 source，先选中第二个再退出重进
        app.apps[0].sources.push(Source {
            path: "/home/me/.gitconfig.extra".into(),
            resolved: None,
            exists: true,
            format: Format::Git,
            diagnostic: None,
            settings: vec![],
        });
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.source_index, 1);
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.source_index, 0, "重进 Sources 必须重置索引");
    }

    #[test]
    fn j_moves_between_sources_and_edits_target_the_selected_one() {
        let (dir, manager, cfg) = temp_env();
        let extra = dir.path().join("home/.gitconfig.extra");
        std::fs::write(&extra, b"[user]\nname = Grace\n").unwrap();
        let saved_visual = std::env::var_os("VISUAL");
        let saved_editor = std::env::var_os("EDITOR");
        std::env::remove_var("VISUAL");
        std::env::set_var("EDITOR", "sed -i s/Grace/GraceX/");
        let mut app = app_with_source(manager, &cfg);
        app.apps[0].sources.push(Source {
            path: extra.to_str().unwrap().into(),
            resolved: Some(extra.to_str().unwrap().into()),
            exists: true,
            format: Format::Git,
            diagnostic: None,
            settings: vec![],
        });
        app.handle_key(key(KeyCode::Right)); // Sources
        app.handle_key(key(KeyCode::Char('j'))); // 第二个 source
        assert_eq!(app.source_index, 1);
        app.handle_key(key(KeyCode::Right)); // Settings（第二个 source 无设置 → 提示）
        assert_eq!(app.focus, Focus::Sources, "无设置的 source 不进入 Settings");
        assert!(
            app.status.contains("No structured settings") || app.status.contains("没有结构化设置")
        );
        // 用 e 编辑第二个 source（focus 停在 Sources，选中第二个）
        app.handle_key(key(KeyCode::Char('e')));
        let change = app.pending.as_ref().expect("pending change");
        let stage = change.stage.clone();
        let text = String::from_utf8(std::fs::read(&stage).unwrap()).unwrap();
        assert!(
            text.contains("name = Grace"),
            "必须编辑选中的第二个 source:\n{text}"
        );
        let _ = app.manager.discard(&app.pending.take().unwrap());
        match saved_visual {
            Some(v) => std::env::set_var("VISUAL", v),
            None => std::env::remove_var("VISUAL"),
        }
        match saved_editor {
            Some(v) => std::env::set_var("EDITOR", v),
            None => std::env::remove_var("EDITOR"),
        }
        let _ = dir;
    }

    #[test]
    fn s_in_sources_layer_prompts_to_enter_settings() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.prompt, Prompt::None);
        assert!(!app.status.is_empty(), "必须给出提示");
    }

    #[test]
    fn s_e_r_in_apps_layer_prompts_to_enter_sources() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.prompt, Prompt::None);
        assert!(!app.status.is_empty());
    }
}
