use super::app::App;
use crate::domain::Source;
use crate::tui::app::{Focus, Prompt};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

impl App {
    pub(crate) fn render(&mut self, frame: &mut Frame) {
        if self.error_view {
            self.render_error(frame, frame.area());
            return;
        }
        let area = frame.area();
        if area.width < MIN_TERMINAL_WIDTH || area.height < MIN_TERMINAL_HEIGHT {
            self.terminal_small = true;
            self.render_small_terminal(frame, area);
            return;
        }
        self.terminal_small = false;
        if self.prompt == Prompt::Confirm && !self.diff.is_empty() {
            self.render_diff(frame, area);
            return;
        }
        if self.prompt == Prompt::Restore {
            self.render_restore(frame, area);
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

    pub(crate) fn render_apps_settings(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let mut lines: Vec<Line> = Vec::new();
        let apps = self.filtered();
        let width = area.width as usize;
        let heading = self.lang.text("Applications", "应用");
        lines.push(Line::from(Span::styled(
            truncate(heading, width),
            Style::default().fg(Color::DarkGray),
        )));
        // 应用列表独立滚动视口：中部高度扣掉 sources/settings 分区与固定开销
        // （1 标题 + 1 空行 + 1 描述 + 1 Sources 标题 + visible_sources + 1 Settings 标题 + 设置区最少 2 行）
        let source_count = self.current_app().map(|a| a.sources.len()).unwrap_or(0);
        let visible_sources = source_count.min(4);
        let apps_visible = if apps.is_empty() {
            0
        } else {
            apps.len().min(
                (area.height as usize)
                    .saturating_sub(7 + visible_sources)
                    .max(1),
            )
        };
        let app_start = visible_start(self.app_index, apps.len(), apps_visible);
        for (i, app) in apps.iter().enumerate().skip(app_start).take(apps_visible) {
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
                // 设置区可用高度：中部高度 - 应用标题 - apps 视口 - 固定行 - sources 分区
                let details = self.detail_lines(source, width);
                let header = 1 + apps_visible + 2 + 2 + visible_sources;
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
    pub(crate) fn selected_detail_row(&self, source: &Source) -> usize {
        1 + usize::from(source.diagnostic.is_some()) + self.setting_index
    }

    pub(crate) fn detail_lines(&self, source: &Source, width: usize) -> Vec<Span<'static>> {
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

    pub(crate) fn render_footer(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
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

    pub(crate) fn render_diff(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
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
            lines.push(Line::from(Span::styled(
                truncate(line, width),
                diff_style(line),
            )));
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
    pub(crate) fn render_restore(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let width = area.width as usize;
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            truncate(self.lang.text("Restore snapshot", "恢复快照"), width),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )));
        if let Some(snap) = &self.restore_snapshot {
            lines.push(Line::from(Span::raw(truncate(
                &format!(
                    "{}: {}",
                    self.lang.text("Source", "来源"),
                    snap.original_path
                ),
                width,
            ))));
            let local = snap.created_at.with_timezone(&chrono::Local);
            lines.push(Line::from(Span::raw(truncate(
                &format!(
                    "{}: {}",
                    self.lang.text("Created", "创建时间"),
                    local.format("%Y-%m-%d %H:%M:%S")
                ),
                width,
            ))));
            let short_hash = snap.hash.get(..12).unwrap_or(&snap.hash);
            lines.push(Line::from(Span::raw(truncate(
                &format!("SHA-256: {short_hash}…",),
                width,
            ))));
            let size = std::fs::metadata(&snap.content_path)
                .map(|m| m.len())
                .unwrap_or(0);
            lines.push(Line::from(Span::raw(truncate(
                &format!("{}: {size} bytes", self.lang.text("Size", "大小")),
                width,
            ))));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(
            self.lang
                .text("Restore this snapshot? [y/N]", "恢复此快照？[y/N]"),
        ));
        lines.push(Line::from(Span::styled(
            truncate(
                self.lang.text("y restore  n cancel", "y 恢复  n 取消"),
                width,
            ),
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }

    pub(crate) fn render_error(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let width = area.width as usize;
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            truncate(self.lang.text("Error details", "错误详情"), width),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        let text = self.status.trim_end_matches('\n');
        let lines_all: Vec<&str> = if text.is_empty() {
            vec![]
        } else {
            text.split('\n').collect()
        };
        let page = area.height.saturating_sub(3).max(1) as usize;
        let offset = self.error_offset.min(lines_all.len().saturating_sub(page));
        let end = (offset + page).min(lines_all.len());
        // 长行不预截断：交由 Paragraph wrap 折行，保证单行长错误完整可读
        for line in &lines_all[offset..end] {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Color::Red),
            )));
        }
        lines.push(Line::from(Span::styled(
            truncate(
                self.lang
                    .text("↑↓/jk scroll  Esc close", "↑↓/jk 滚动  Esc 关闭"),
                width,
            ),
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }

    pub(crate) fn render_small_terminal(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let width = area.width as usize;
        let lines = vec![
            Line::from(Span::styled(
                truncate(self.lang.text("Config Editor", "Config Editor"), width),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            // 拆成短行：窄终端下关键信息（最小尺寸）不得被截断
            Line::from(Span::raw(truncate(
                self.lang.text("Terminal too small", "终端窗口过小"),
                width,
            ))),
            Line::from(Span::raw(truncate(
                self.lang.text("min 40x12", "最小 40×12"),
                width,
            ))),
        ];
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }
}

/// unified diff 逐行样式：文件头 / hunk 定位 / 新增 / 删除 / 提示
fn diff_style(line: &str) -> Style {
    if line.starts_with("--- ") || line.starts_with("+++ ") {
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD)
    } else if line.starts_with("@@") {
        Style::default().fg(Color::Cyan)
    } else if line.starts_with('+') {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') {
        Style::default().fg(Color::Red)
    } else if line.starts_with('(') {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    }
}

const MIN_TERMINAL_WIDTH: u16 = 40;
const MIN_TERMINAL_HEIGHT: u16 = 12;

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
