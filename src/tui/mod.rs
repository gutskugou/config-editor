use crate::core::{self, Manager};
use crate::domain::{Application, Capability, Setting, Source};
use crate::i18n::Catalog;
use crate::parse::{parse_settings, replace_setting};
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
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Esc => self.focus = Focus::Apps,
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => self.enter_settings(),
            KeyCode::Char('/') => {
                self.prompt = Prompt::Search;
                self.input = self.filter.clone();
            }
            KeyCode::Char('s') => self.start_structured(),
            KeyCode::Char('e') => {
                let _ = self.start_editor();
            }
            KeyCode::Char('r') => self.start_restore(),
            _ => {}
        }
    }

    fn enter_settings(&mut self) {
        if let Some(app) = self.current_app() {
            if setting_count(app) > 0 {
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
    }

    fn move_selection(&mut self, delta: isize) {
        if self.focus == Focus::Apps {
            let list = self.filtered();
            if list.is_empty() {
                return;
            }
            self.app_index = clamp_i(self.app_index as isize + delta, 0, list.len() - 1) as usize;
            self.setting_index = 0;
        } else {
            let Some(app) = self.current_app() else {
                return;
            };
            let count = app.sources.iter().map(|s| s.settings.len()).sum::<usize>();
            if count > 0 {
                self.setting_index =
                    clamp_i(self.setting_index as isize + delta, 0, count - 1) as usize;
            }
        }
    }

    fn selection(&self) -> Option<(&Application, &Source, Option<&Setting>)> {
        let app = self.current_app()?;
        let mut row = 0;
        for source in &app.sources {
            for setting in &source.settings {
                if row == self.setting_index {
                    return Some((app, source, Some(setting)));
                }
                row += 1;
            }
        }
        for source in &app.sources {
            if source.exists {
                return Some((app, source, None));
            }
        }
        None
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
        // replace_setting 需要原始内容；stage 在 Prepare 时已复制原内容，直接读 target 等价且简单
        let original = std::fs::read(&change.target).unwrap_or_default();
        let content = match replace_setting(source.format, &original, setting, &self.input) {
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
            KeyCode::Up | KeyCode::Char('k') => {
                self.diff_offset = self.diff_offset.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.diff_offset += 1;
            }
            KeyCode::PageUp => {
                self.diff_offset = self.diff_offset.saturating_sub(self.diff_page_size());
            }
            KeyCode::PageDown => {
                self.diff_offset += self.diff_page_size();
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
        let parts = shell_words::split(&editor).map_err(|e| e.to_string())?;
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
        let after = std::fs::read(&change.stage).map_err(|e| e.to_string())?;
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
            let available = area.height as usize;
            let start = visible_start(self.setting_index, details.len(), available);
            let end = (start + available).min(details.len());
            for detail in &details[start..end] {
                lines.push(Line::from(detail.clone()));
            }
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
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
}

fn setting_count(app: &Application) -> usize {
    app.sources.iter().map(|s| s.settings.len()).sum()
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
}
