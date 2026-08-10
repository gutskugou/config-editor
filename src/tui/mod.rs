use crate::domain::Source;
use app::{App, Focus, Prompt};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

pub mod app;
mod keymap;

pub use app::run_tui;

impl App {
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
            // Sources 分区：标题 + source 行（高度 min(count,4)）+ 设置标题
            let source = self.current_source();
            let source_count = app.sources.len();
            let visible_sources = source_count.min(4);
            let src_start = visible_start(self.source_index, source_count, visible_sources);
            let heading1 = Span::styled(
                truncate(
                    &format!("── {} ──", self.lang.text("Sources", "配置文件")),
                    width,
                ),
                Style::default().fg(Color::DarkGray),
            );
            lines.push(Line::from(heading1));
            if let Some(source) = source {
                for (i, s) in app
                    .sources
                    .iter()
                    .enumerate()
                    .skip(src_start)
                    .take(visible_sources)
                {
                    let is_selected = self.focus == Focus::Sources && i == self.source_index;
                    let flag = if s.exists { "file" } else { "missing" };
                    let marker = if is_selected { "> " } else { "  " };
                    let text = format!("{marker}[{flag}] {}", s.path);
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
                    lines.push(Line::from(span));
                }
                let heading2 = Span::styled(
                    truncate(
                        &format!("── {} ──", self.lang.text("Settings", "设置")),
                        width,
                    ),
                    Style::default().fg(Color::DarkGray),
                );
                lines.push(Line::from(heading2));
                // 设置区可用高度：中部高度 - 头部(apps/desc) - sources 分区
                let details = self.detail_lines(source, width);
                let header = 1 + apps.len() + 2 + 2 + visible_sources;
                let available = area.height.saturating_sub(header as u16) as usize;
                let start =
                    visible_start(self.selected_detail_row(source), details.len(), available);
                let end = (start + available).min(details.len());
                for detail in &details[start..end] {
                    lines.push(Line::from(detail.clone()));
                }
            }
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }

    /// 选中设置在 detail_lines 中的实际显示行号（含 [file]/[missing] 与诊断行），
    /// 而不是设置序号；用于滚动窗口计算。
    fn selected_detail_row(&self, source: &Source) -> usize {
        1 + usize::from(source.diagnostic.is_some()) + self.setting_index
    }

    fn detail_lines(&self, source: &Source, width: usize) -> Vec<Span<'static>> {
        let mut lines = Vec::new();
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
        for (row, setting) in source.settings.iter().enumerate() {
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
    use crate::core;
    use crate::domain::{Application, Capability, Format, Setting};
    use crate::i18n;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    // 进程级环境变量 EDITOR/VISUAL 是共享全局；注入编辑器的测试必须串行执行
    static EDITOR_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    fn right_traverses_sources_then_settings() {
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
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.focus, Focus::Sources);
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
    fn render_shows_sources_section_and_only_selected_source_settings() {
        let mut app = App::new(
            sample_apps(),
            core::Manager::default(),
            i18n::Catalog { chinese: false },
        );
        app.apps[0].sources[0].settings = vec![Setting {
            key: "user.first".into(),
            value: "A".into(),
            line: 1,
            occ: 1,
            editable: true,
            sensitive: false,
        }];
        app.apps[0].sources.push(Source {
            path: "/home/me/.gitconfig.extra".into(),
            resolved: None,
            exists: true,
            format: Format::Git,
            diagnostic: None,
            settings: vec![Setting {
                key: "user.second".into(),
                value: "B".into(),
                line: 1,
                occ: 1,
                editable: true,
                sensitive: false,
            }],
        });
        app.focus = Focus::Sources;
        app.source_index = 1;
        app.width = 80;
        app.height = 24;
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains(".gitconfig"), "source 路径必须可见:\n{text}");
        assert!(text.contains(".gitconfig.extra"));
        assert!(
            text.contains("user.second"),
            "设置区必须显示选中 source 的设置:\n{text}"
        );
        assert!(
            !text.contains("user.first"),
            "设置区不得混入其他 source 的设置:\n{text}"
        );
        let style = row_starting_with(&buffer, "> ").expect("选中 source 行必须以 '> ' 开头");
        assert_eq!(style.fg, Some(Color::Green), "选中 source 行必须高亮为绿色");
        assert!(
            style.add_modifier.contains(Modifier::BOLD),
            "选中 source 行必须加粗"
        );
    }

    fn row_starting_with(buffer: &ratatui::buffer::Buffer, prefix: &str) -> Option<Style> {
        let width = buffer.area.width as usize;
        for y in 0..buffer.area.height {
            let row: String = (0..width)
                .map(|x| buffer.cell((x as u16, y)).map(|c| c.symbol()).unwrap_or(""))
                .collect();
            if row.starts_with(prefix) {
                return buffer.cell((0, y)).map(|c| c.style());
            }
        }
        None
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
        // 与 j_moves 测试串行化：两个测试共享进程环境变量 EDITOR/VISUAL
        let _env_guard = EDITOR_ENV_LOCK.lock().unwrap();
        let (dir, manager, cfg) = temp_env();
        let mut app = app_with_source(manager, &cfg);
        let saved_visual = std::env::var_os("VISUAL");
        let saved_editor = std::env::var_os("EDITOR");
        std::env::remove_var("VISUAL");
        std::env::set_var("EDITOR", "'");
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Right));
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
        assert_eq!(
            app.selected_detail_row(&app.apps[0].sources[0]),
            1,
            "[file] 占 1 行"
        );
        app.setting_index = 2;
        assert_eq!(app.selected_detail_row(&app.apps[0].sources[0]), 3);
        // 诊断行让设置行号 +1
        app.apps[0].sources[0].diagnostic = Some("boom".into());
        app.setting_index = 0;
        assert_eq!(app.selected_detail_row(&app.apps[0].sources[0]), 2);
        app.setting_index = 1;
        assert_eq!(app.selected_detail_row(&app.apps[0].sources[0]), 3);
        // 第二个 source 独立计算行号：1([file]) + 1(诊断) = 2 行前缀
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
        app.setting_index = 0;
        assert_eq!(
            app.selected_detail_row(&app.apps[0].sources[1]),
            2,
            "1([file]) + 1(诊断) = 2 行前缀"
        );
    }

    #[test]
    fn detail_viewport_keeps_bottom_setting_visible_on_small_terminals() {
        // 30 个设置 + 头 7 行（标题/应用/描述/Sources 分区，公式 1+apps+2+2+visible_sources）：
        // 80x16 终端中部 10 行中 detail 区仅 3 行时，
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
        // 与 editor_parse_error 测试串行化：两个测试共享进程环境变量 EDITOR/VISUAL
        let _env_guard = EDITOR_ENV_LOCK.lock().unwrap();
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
        for code in [KeyCode::Char('s'), KeyCode::Char('e'), KeyCode::Char('r')] {
            app.handle_key(key(code));
            assert_eq!(
                app.prompt,
                Prompt::None,
                "Apps 层按 {code:?} 不得进入编辑流程"
            );
            assert!(!app.status.is_empty(), "Apps 层按 {code:?} 必须给出提示");
            assert!(app.pending.is_none(), "Apps 层按 {code:?} 不得留下暂存");
        }
    }

    #[test]
    fn normal_keymap_maps_all_binding_keys() {
        use super::keymap::{normal_action, Action};
        let m = |c| key(KeyCode::Char(c));
        assert_eq!(normal_action(m('q')), Action::Quit);
        assert_eq!(normal_action(m('k')), Action::Move(-1));
        assert_eq!(normal_action(key(KeyCode::Up)), Action::Move(-1));
        assert_eq!(normal_action(m('j')), Action::Move(1));
        assert_eq!(normal_action(key(KeyCode::Down)), Action::Move(1));
        assert_eq!(normal_action(m('h')), Action::FocusUp);
        assert_eq!(normal_action(key(KeyCode::Left)), Action::FocusUp);
        assert_eq!(normal_action(key(KeyCode::Esc)), Action::FocusUp);
        assert_eq!(normal_action(m('l')), Action::FocusDown);
        assert_eq!(normal_action(key(KeyCode::Right)), Action::FocusDown);
        assert_eq!(normal_action(key(KeyCode::Enter)), Action::FocusDown);
        assert_eq!(normal_action(m('/')), Action::Search);
        assert_eq!(normal_action(m('s')), Action::Set);
        assert_eq!(normal_action(m('e')), Action::Edit);
        assert_eq!(normal_action(m('r')), Action::Restore);
        assert_eq!(normal_action(m('x')), Action::None);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(normal_action(ctrl_c), Action::Quit);
    }

    #[test]
    fn confirm_and_text_keymap_map_all_binding_keys() {
        use super::keymap::{confirm_action, text_action, Action};
        let m = |c| key(KeyCode::Char(c));
        assert_eq!(confirm_action(m('y')), Action::Apply);
        assert_eq!(confirm_action(m('Y')), Action::Apply);
        assert_eq!(confirm_action(m('n')), Action::Reject);
        assert_eq!(confirm_action(m('N')), Action::Reject);
        assert_eq!(confirm_action(key(KeyCode::Esc)), Action::Reject);
        assert_eq!(confirm_action(m('k')), Action::Move(-1));
        assert_eq!(confirm_action(m('j')), Action::Move(1));
        assert_eq!(confirm_action(key(KeyCode::PageUp)), Action::PgUp);
        assert_eq!(confirm_action(key(KeyCode::PageDown)), Action::PgDn);
        assert_eq!(confirm_action(m('q')), Action::Quit);
        assert_eq!(confirm_action(m('x')), Action::None);
        assert_eq!(text_action(key(KeyCode::Esc)), Action::Cancel);
        assert_eq!(text_action(key(KeyCode::Backspace)), Action::Backspace);
        assert_eq!(text_action(key(KeyCode::Enter)), Action::Submit);
        assert_eq!(text_action(m('a')), Action::Char('a'));
        assert_eq!(text_action(m('x')), Action::Char('x'));
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(text_action(ctrl_c), Action::Quit);
    }
}
