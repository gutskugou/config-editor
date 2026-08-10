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

    pub(crate) fn render_apps_settings(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
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
