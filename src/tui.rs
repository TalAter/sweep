//! Sweep's approval UI: explicit consent, local chrome, and one terminal session.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use std::cell::Cell;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Dark,
    Light,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsightKind {
    Clear,
    Caution,
    Danger,
    Manipulation,
    NoLlm,
    AnalysisFailed,
}
#[derive(Clone, Debug)]
pub struct InsightView {
    pub kind: InsightKind,
    pub source: String,
    pub message: String,
    pub flags: Vec<String>,
    pub behaviors: Vec<(String, bool)>,
}
#[derive(Clone, Debug)]
pub enum Phase {
    Paste { error: Option<String> },
    Loading { source: String },
    Resolved(InsightView),
}
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Cancel,
    Run,
    Submit(String),
}
#[derive(Clone, Debug)]
pub struct Ui {
    pub phase: Phase,
    pub appearance: Appearance,
    pub nerd_fonts: bool,
    pub color_level: u8,
    input: String,
    cursor: usize,
    run_focused: bool,
    // Rendering updates viewport bounds, including after terminal resizes.
    scroll: Cell<u16>,
    max_scroll: Cell<u16>,
    tick: usize,
    killed: String,
}
impl Ui {
    pub fn new(phase: Phase, appearance: Appearance, nerd_fonts: bool) -> Self {
        Self {
            phase,
            appearance,
            nerd_fonts,
            color_level: 3,
            input: String::new(),
            cursor: 0,
            run_focused: false,
            scroll: Cell::new(0),
            max_scroll: Cell::new(0),
            tick: 0,
            killed: String::new(),
        }
    }
    pub fn transition(&mut self, phase: Phase) {
        self.phase = phase;
        self.input.clear();
        self.cursor = 0;
        self.run_focused = false;
        self.scroll.set(0);
        self.max_scroll.set(0);
    }
    fn typing(&self) -> bool {
        matches!(self.phase, Phase::Paste { .. })
            || matches!(&self.phase,Phase::Resolved(v) if matches!(v.kind,InsightKind::Danger|InsightKind::Manipulation))
    }
    pub fn key(&mut self, key: KeyEvent) -> Action {
        if key.kind == crossterm::event::KeyEventKind::Release {
            return Action::None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('c')) {
            return Action::Cancel;
        }
        match key.code {
            KeyCode::PageDown | KeyCode::Down => {
                let amount = if key.code == KeyCode::PageDown { 8 } else { 1 };
                self.scroll.set(
                    self.scroll
                        .get()
                        .saturating_add(amount)
                        .min(self.max_scroll.get()),
                );
                return Action::None;
            }
            KeyCode::PageUp | KeyCode::Up => {
                let amount = if key.code == KeyCode::PageUp { 8 } else { 1 };
                self.scroll.set(self.scroll.get().saturating_sub(amount));
                return Action::None;
            }
            _ => {}
        }
        if matches!(self.phase, Phase::Loading { .. }) {
            return Action::None;
        }
        if !self.typing() {
            match key.code {
                KeyCode::Left => self.run_focused = false,
                KeyCode::Right => self.run_focused = true,
                KeyCode::Enter => {
                    return if self.run_focused {
                        Action::Run
                    } else {
                        Action::Cancel
                    };
                }
                _ => {}
            }
            return Action::None;
        }
        match key.code {
            KeyCode::Enter => {
                let text = self.input.trim().to_string();
                if matches!(self.phase, Phase::Paste { .. }) {
                    if !text.is_empty() {
                        return Action::Submit(text);
                    }
                } else if text == "install" {
                    return Action::Run;
                } else {
                    self.input.clear();
                    self.cursor = 0;
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.len(),
            KeyCode::Left if alt => self.word_left(),
            KeyCode::Right if alt => self.word_right(),
            KeyCode::Left => self.cursor = self.previous(),
            KeyCode::Right => self.cursor = self.next(),
            KeyCode::Backspace => {
                let old = self.cursor;
                if alt {
                    self.word_left();
                } else {
                    self.cursor = self.previous();
                }
                if alt && self.cursor < old {
                    self.killed = self.input[self.cursor..old].to_string();
                }
                self.input.drain(self.cursor..old);
            }
            KeyCode::Delete => {
                let next = self.next();
                self.input.drain(self.cursor..next);
            }
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.input.len(),
            KeyCode::Char('u') if ctrl && self.cursor > 0 => {
                self.killed = self.input.drain(..self.cursor).collect();
                self.cursor = 0;
            }
            KeyCode::Char('k') if ctrl && self.cursor < self.input.len() => {
                self.killed = self.input.drain(self.cursor..).collect();
            }
            KeyCode::Char('y') if ctrl => {
                let killed = self.killed.clone();
                self.paste(&killed);
            }
            KeyCode::Char('b') if alt => self.word_left(),
            KeyCode::Char('f') if alt => self.word_right(),
            KeyCode::Char(c) if !ctrl && !alt => self.paste(&c.to_string()),
            _ => {}
        }
        Action::None
    }
    fn previous(&self) -> usize {
        self.input
            .grapheme_indices(true)
            .rfind(|(i, _)| *i < self.cursor)
            .map_or(0, |(i, _)| i)
    }
    fn next(&self) -> usize {
        self.input
            .grapheme_indices(true)
            .find(|(i, _)| *i > self.cursor)
            .map_or(self.input.len(), |(i, _)| i)
    }
    fn word_left(&mut self) {
        self.cursor = self
            .input
            .unicode_word_indices()
            .rfind(|(i, _)| *i < self.cursor)
            .map_or(0, |(i, _)| i);
    }
    fn word_right(&mut self) {
        self.cursor = self
            .input
            .unicode_word_indices()
            .find(|(i, word)| i + word.len() > self.cursor)
            .map_or(self.input.len(), |(i, word)| i + word.len());
    }
    pub fn paste(&mut self, text: &str) {
        if !self.typing() {
            return;
        }
        let text: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == '\t')
            .collect();
        let mut end = text
            .len()
            .min((256 * 1024_usize).saturating_sub(self.input.len()));
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.input.insert_str(self.cursor, &text[..end]);
        self.cursor += end;
    }
    fn colors(&self) -> (Color, Color, Color, Color, Color) {
        match self.appearance {
            Appearance::Dark => (
                Color::Rgb(210, 210, 225),
                Color::Rgb(170, 170, 195),
                Color::Rgb(35, 35, 50),
                Color::Rgb(55, 45, 80),
                Color::Rgb(255, 100, 100),
            ),
            Appearance::Light => (
                Color::Black,
                Color::Rgb(45, 45, 70),
                Color::Rgb(218, 232, 250),
                Color::Rgb(220, 215, 238),
                Color::Rgb(190, 25, 45),
            ),
        }
    }
    pub fn frame_colors(&self) -> (Color, Color) {
        let kind = match &self.phase {
            Phase::Resolved(v) => v.kind,
            _ => InsightKind::Clear,
        };
        match (self.appearance, kind) {
            (Appearance::Dark, InsightKind::Caution) => {
                (Color::Rgb(255, 100, 200), Color::Rgb(60, 60, 100))
            }
            (Appearance::Dark, InsightKind::Danger) => {
                (Color::Rgb(255, 60, 80), Color::Rgb(60, 60, 100))
            }
            (Appearance::Light, InsightKind::Caution) => {
                (Color::Rgb(175, 35, 115), Color::Rgb(170, 170, 195))
            }
            (Appearance::Light, InsightKind::Danger) => {
                (Color::Rgb(190, 25, 45), Color::Rgb(170, 170, 195))
            }
            (Appearance::Dark, _) => (Color::Rgb(80, 160, 255), Color::Rgb(40, 60, 100)),
            (Appearance::Light, _) => (Color::Rgb(25, 90, 190), Color::Rgb(170, 170, 195)),
        }
    }
    fn body(&self) -> Vec<Line<'static>> {
        let (body, supporting, _, _, danger) = self.colors();
        let base = Style::default().fg(body);
        let plain = |s: String| Line::styled(s, base);
        match &self.phase {
            Phase::Paste { .. } => vec![plain("Paste an install command".into())],
            Phase::Loading { source } => vec![plain(source.clone())],
            Phase::Resolved(view) => {
                let mut lines = Vec::new();
                if view.kind == InsightKind::Manipulation {
                    lines.push(Line::styled(
                        "⚠ analysis may be compromised",
                        Style::default().fg(danger).bold(),
                    ));
                }
                if matches!(view.kind, InsightKind::Caution | InsightKind::Danger) {
                    let caution = view.kind == InsightKind::Caution;
                    let (fg, bg) = match (self.appearance, caution) {
                        (Appearance::Dark, true) => {
                            (Color::Rgb(255, 200, 80), Color::Rgb(80, 50, 20))
                        }
                        (Appearance::Dark, false) => (danger, Color::Rgb(80, 25, 25)),
                        (Appearance::Light, true) => {
                            (Color::Rgb(160, 95, 0), Color::Rgb(248, 232, 200))
                        }
                        (Appearance::Light, false) => (danger, Color::Rgb(248, 218, 218)),
                    };
                    let mut spans = Vec::new();
                    if self.nerd_fonts {
                        spans.push(Span::styled("", Style::default().fg(bg)));
                    }
                    spans.push(Span::styled(
                        if caution {
                            " ⚠ caution "
                        } else {
                            " ✗ danger "
                        },
                        Style::default().fg(fg).bg(bg).bold(),
                    ));
                    if self.nerd_fonts {
                        spans.push(Span::styled("", Style::default().fg(bg)));
                    }
                    spans.push(Span::styled(format!("  {}", view.source), base));
                    lines.push(Line::from(spans));
                } else {
                    lines.push(plain(view.source.clone()));
                }
                lines.push(Line::default());
                lines.push(plain(view.message.clone()));
                if !view.flags.is_empty() {
                    lines.push(Line::default());
                    lines.push(Line::styled("Flags:", Style::default().fg(supporting)));
                    for flag in &view.flags {
                        lines.push(Line::styled(
                            format!(" ⚠ {flag}"),
                            Style::default().fg(supporting),
                        ));
                    }
                }
                if !view.behaviors.is_empty() {
                    lines.push(Line::default());
                    lines.push(Line::styled(
                        "Appears to do (not exhaustive):",
                        Style::default().fg(supporting),
                    ));
                    for (description, sudo) in &view.behaviors {
                        lines.push(Line::styled(
                            format!(" • {description}{}", if *sudo { "  (sudo)" } else { "" }),
                            Style::default().fg(supporting),
                        ));
                    }
                }
                lines
            }
        }
    }
    pub fn render(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        if area.width < 6 || area.height < 4 {
            return;
        }
        let (_, supporting, surface, selected, _) = self.colors();
        let lines = self.body();
        let paste = matches!(self.phase, Phase::Paste { .. });
        let indent = if paste { 0 } else { 1 };
        let natural = lines
            .iter()
            .map(Line::width)
            .max()
            .unwrap_or(0)
            .saturating_add(indent)
            .max(if paste { 50 } else { 44 });
        let width = (natural.saturating_add(4).min(u16::MAX as usize) as u16)
            .min(area.width.saturating_sub(4).max(6));
        let content_width = width.saturating_sub(4 + indent as u16).max(1);
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let body_rows = paragraph.line_count(content_width).min(u16::MAX as usize) as u16;
        let action_rows = match &self.phase {
            Phase::Paste { error: Some(error) } => {
                4 + Paragraph::new(error.as_str())
                    .wrap(Wrap { trim: false })
                    .line_count(content_width) as u16
            }
            Phase::Paste { .. } => 3,
            _ if self.typing() => 4,
            _ => 1,
        };
        let height = body_rows
            .saturating_add(action_rows)
            .saturating_add(5)
            .min(area.height);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        self.border(frame, rect);
        let inner = Rect::new(
            rect.x + 2 + indent as u16,
            rect.y + 2,
            content_width,
            rect.height.saturating_sub(4),
        );
        let actions_height = action_rows.min(inner.height);
        let body_height = inner.height.saturating_sub(actions_height + 1);
        self.max_scroll.set(body_rows.saturating_sub(body_height));
        let scroll = self.scroll.get().min(self.max_scroll.get());
        self.scroll.set(scroll);
        frame.render_widget(
            paragraph.scroll((scroll, 0)),
            Rect::new(inner.x, inner.y, inner.width, body_height),
        );
        let action_y = inner.y + inner.height.saturating_sub(actions_height);
        let base = Style::default().fg(supporting);
        let (label, primary, separator) = match self.appearance {
            Appearance::Dark => (
                Color::Rgb(115, 115, 140),
                Color::Rgb(245, 186, 74),
                Color::Rgb(65, 65, 80),
            ),
            Appearance::Light => (
                Color::Rgb(105, 105, 130),
                Color::Rgb(255, 165, 50),
                Color::Rgb(175, 175, 195),
            ),
        };
        let action =
            |glyph: &'static str, text: &'static str, primary_action: bool, focused: bool| {
                let background = if focused { selected } else { Color::Reset };
                vec![
                    Span::styled(
                        glyph,
                        Style::default()
                            .fg(if primary_action { primary } else { supporting })
                            .bg(background)
                            .bold(),
                    ),
                    Span::styled(text, Style::default().fg(label).bg(background)),
                ]
            };
        let focused = matches!(self.phase, Phase::Resolved(_)) && !self.typing();
        let cancel = action("Esc", " Cancel", false, focused && !self.run_focused);
        let divider = || Span::styled(" │ ", Style::default().fg(separator));
        match &self.phase {
            Phase::Paste { error } => {
                self.input_line(
                    frame,
                    Rect::new(inner.x, action_y, inner.width, 1),
                    "curl ... | sh",
                    surface,
                );
                if let Some(error) = error {
                    frame.render_widget(
                        Paragraph::new(error.as_str())
                            .wrap(Wrap { trim: false })
                            .style(Style::default().fg(self.colors().4)),
                        Rect::new(
                            inner.x,
                            action_y + 1,
                            inner.width,
                            actions_height.saturating_sub(2),
                        ),
                    );
                }
                frame.render_widget(
                    Paragraph::new(Line::from({
                        let mut spans = action("Enter", " Install", true, false);
                        spans.push(divider());
                        spans.extend(action("Esc", " Cancel", false, false));
                        spans
                    })),
                    Rect::new(
                        inner.x,
                        action_y + actions_height.saturating_sub(1),
                        inner.width,
                        1,
                    ),
                );
            }
            _ => {
                let spans = if self.typing() || matches!(self.phase, Phase::Loading { .. }) {
                    cancel
                } else {
                    let mut spans = cancel;
                    spans.push(divider());
                    spans.extend(action("Enter", " Run", true, self.run_focused));
                    spans
                };
                frame.render_widget(
                    Paragraph::new(Line::from(spans)),
                    Rect::new(inner.x + 2, action_y, inner.width.saturating_sub(2), 1),
                );
                if self.typing() && actions_height >= 4 {
                    frame.render_widget(
                        Paragraph::new("Type 'install' to run:").style(base),
                        Rect::new(inner.x, action_y + 2, inner.width, 1),
                    );
                    self.input_line(
                        frame,
                        Rect::new(inner.x, action_y + 3, inner.width, 1),
                        "",
                        surface,
                    );
                }
            }
        }
        if body_rows > body_height && rect.width > 24 {
            let hint = " ↑↓ PgUp/PgDn ";
            frame
                .buffer_mut()
                .set_string(rect.right() - 16, rect.bottom() - 1, hint, base);
        }
        for cell in &mut frame.buffer_mut().content {
            if self.color_level == 0 {
                if cell.bg == selected {
                    cell.modifier.insert(Modifier::REVERSED);
                }
                cell.fg = Color::Reset;
                cell.bg = Color::Reset;
            } else if self.color_level < 3 {
                cell.fg = quantize(cell.fg, self.color_level);
                cell.bg = quantize(cell.bg, self.color_level);
            }
        }
    }
    fn input_line(&self, frame: &mut Frame<'_>, area: Rect, placeholder: &str, surface: Color) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let (body, supporting, _, _, _) = self.colors();
        let mut start = self.cursor;
        while start > 0 {
            let prev = self.input[..start]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
            if self.input[prev..self.cursor].width() >= area.width.saturating_sub(1) as usize {
                break;
            }
            start = prev;
        }
        let shown = if self.input.is_empty() {
            placeholder
        } else {
            &self.input[start..]
        };
        frame.render_widget(
            Paragraph::new(shown).style(
                Style::default()
                    .fg(if self.input.is_empty() {
                        supporting
                    } else {
                        body
                    })
                    .bg(surface),
            ),
            area,
        );
        let offset = self.input[start..self.cursor]
            .width()
            .min(area.width.saturating_sub(1) as usize) as u16;
        frame.set_cursor_position((area.x + offset, area.y));
    }
    fn border(&self, frame: &mut Frame<'_>, r: Rect) {
        let (start, end) = if self.color_level < 3 {
            (self.colors().0, self.colors().0)
        } else {
            self.frame_colors()
        };
        let buf = frame.buffer_mut();
        for x in 0..r.width {
            let color = gradient(start, end, x, r.width.saturating_sub(1));
            buf[(r.x + x, r.y)]
                .set_symbol(if x == 0 {
                    "╭"
                } else if x + 1 == r.width {
                    "╮"
                } else {
                    "─"
                })
                .set_fg(color);
            buf[(r.x + x, r.bottom() - 1)]
                .set_symbol(if x == 0 {
                    "╰"
                } else if x + 1 == r.width {
                    "╯"
                } else {
                    "─"
                })
                .set_fg(end);
        }
        for y in 1..r.height.saturating_sub(1) {
            buf[(r.x, r.y + y)].set_symbol("│").set_fg(gradient(
                start,
                end,
                y - 1,
                r.height.saturating_sub(3),
            ));
            buf[(r.right() - 1, r.y + y)].set_symbol("│").set_fg(end);
        }
        if matches!(self.phase, Phase::Loading { .. }) && r.width > 20 {
            let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            buf.set_string(
                r.x + 2,
                r.bottom() - 1,
                format!(" {} Analyzing… ", spinner[(self.tick / 3) % spinner.len()]),
                Style::default().fg(self.colors().0),
            );
        }
    }
}

fn gradient(start: Color, end: Color, n: u16, total: u16) -> Color {
    use palette::{FromColor, Mix, Oklab, Srgb};
    let (Color::Rgb(r, g, b), Color::Rgb(rr, gg, bb)) = (start, end) else {
        return end;
    };
    let a = Oklab::from_color(Srgb::new(r, g, b).into_format::<f32>());
    let b = Oklab::from_color(Srgb::new(rr, gg, bb).into_format::<f32>());
    let t = if total == 0 {
        0.0
    } else {
        f32::from(n) / f32::from(total)
    };
    let mixed: Srgb<u8> = Srgb::from_color(a.mix(b, t)).into_format();
    Color::Rgb(mixed.red, mixed.green, mixed.blue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn resolved(kind: InsightKind) -> Ui {
        Ui::new(
            Phase::Resolved(InsightView {
                kind,
                source: "example.com/install".into(),
                message: "Downloads the tool.".into(),
                flags: vec![],
                behaviors: vec![],
            }),
            Appearance::Dark,
            false,
        )
    }
    fn screen(ui: &Ui, width: u16, height: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        t.draw(|f| ui.render(f)).unwrap();
        t.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
    }
    #[test]
    fn approval_defaults_to_cancel_and_requires_explicit_run_selection() {
        let mut ui = resolved(InsightKind::Clear);
        assert_eq!(ui.key(key(KeyCode::Enter)), Action::Cancel);
        ui.key(key(KeyCode::Right));
        assert_eq!(ui.key(key(KeyCode::Enter)), Action::Run);
    }
    #[test]
    fn loading_never_accepts_approval_and_resolved_focus_resets() {
        let mut ui = resolved(InsightKind::Clear);
        ui.key(key(KeyCode::Right));
        ui.transition(Phase::Loading {
            source: "example.com".into(),
        });
        assert_eq!(ui.key(key(KeyCode::Enter)), Action::None);
        assert_eq!(ui.key(key(KeyCode::Esc)), Action::Cancel);
        ui.transition(resolved(InsightKind::Clear).phase);
        assert_eq!(ui.key(key(KeyCode::Enter)), Action::Cancel);
    }
    #[test]
    fn elevated_risk_requires_trimmed_case_sensitive_confirmation() {
        for kind in [InsightKind::Danger, InsightKind::Manipulation] {
            let mut ui = resolved(kind);
            ui.paste("INSTALL");
            assert_eq!(ui.key(key(KeyCode::Enter)), Action::None);
            assert_eq!(ui.input, "");
            ui.paste(" install ");
            assert_eq!(ui.key(key(KeyCode::Enter)), Action::Run);
        }
    }
    #[test]
    fn paste_preserves_command_and_inline_error_with_unicode_editing() {
        let mut ui = Ui::new(Phase::Paste { error: None }, Appearance::Dark, false);
        ui.paste("  curl https://example.com/é | sh  ");
        ui.key(key(KeyCode::Home));
        ui.key(key(KeyCode::Delete));
        assert_eq!(
            ui.key(key(KeyCode::Enter)),
            Action::Submit("curl https://example.com/é | sh".into())
        );
        ui.phase = Phase::Paste {
            error: Some("Only one pipe is supported".into()),
        };
        let text = screen(&ui, 90, 24);
        assert!(text.contains("Only one pipe is supported"));
        assert!(text.contains("curl https://example.com/é | sh"));
    }
    #[test]
    fn renderer_preserves_neutral_frame_and_risk_pills_with_bounded_layout() {
        let plain = resolved(InsightKind::Clear);
        let manipulation = resolved(InsightKind::Manipulation);
        assert_eq!(plain.frame_colors(), manipulation.frame_colors());
        assert_ne!(
            plain.frame_colors(),
            resolved(InsightKind::Danger).frame_colors()
        );
        let text = screen(&manipulation, 100, 30);
        assert!(text.contains("analysis may be compromised"));
        assert!(text.contains("Type 'install' to run:"));
        assert!(text.contains("╭"));
        let text = screen(&resolved(InsightKind::Caution), 100, 30);
        assert!(text.contains("⚠ caution"));
        for (w, h) in [(1, 1), (8, 3), (32, 10), (80, 24)] {
            screen(&plain, w, h);
        }
    }
    #[test]
    fn overflow_can_scroll_without_hiding_approval_controls() {
        let mut ui = resolved(InsightKind::Clear);
        if let Phase::Resolved(view) = &mut ui.phase {
            view.behaviors = (0..40).map(|n| (format!("Operation {n}"), false)).collect();
        }
        let before = screen(&ui, 50, 15);
        assert!(before.contains("Cancel"));
        for _ in 0..40 {
            ui.key(key(KeyCode::PageDown));
        }
        let after = screen(&ui, 50, 15);
        assert!(after.contains("Operation 39"));
        assert!(after.contains("Cancel"));
        assert_ne!(before, after);
    }

    fn overflowing(kind: InsightKind) -> Ui {
        let mut ui = resolved(kind);
        if let Phase::Resolved(view) = &mut ui.phase {
            view.behaviors = (0..40).map(|n| (format!("Operation {n}"), false)).collect();
        }
        ui
    }

    #[test]
    fn paging_back_moves_immediately_after_repeated_bottom_input() {
        let mut ui = overflowing(InsightKind::Clear);
        screen(&ui, 50, 15);
        for _ in 0..40 {
            ui.key(key(KeyCode::PageDown));
        }
        let bottom = screen(&ui, 50, 15);
        // Multiple events may arrive between frames.
        ui.key(key(KeyCode::PageDown));
        ui.key(key(KeyCode::PageDown));
        ui.key(key(KeyCode::PageUp));
        assert_ne!(bottom, screen(&ui, 50, 15));
    }

    #[test]
    fn growing_viewport_clamps_scroll_before_next_input() {
        let mut ui = overflowing(InsightKind::Clear);
        screen(&ui, 50, 15);
        for _ in 0..40 {
            ui.key(key(KeyCode::PageDown));
        }
        screen(&ui, 50, 15);
        let expanded = screen(&ui, 50, 30);
        assert!(expanded.contains("Operation 39"));
        ui.key(key(KeyCode::Up));
        assert_ne!(expanded, screen(&ui, 50, 30));
    }

    #[test]
    fn high_risk_arrow_scrolling_preserves_confirmation_editing() {
        for kind in [InsightKind::Danger, InsightKind::Manipulation] {
            let mut ui = overflowing(kind);
            ui.paste("instal");
            let before = screen(&ui, 50, 15);
            ui.key(key(KeyCode::Down));
            assert_ne!(before, screen(&ui, 50, 15));
            ui.key(key(KeyCode::Up));
            assert_eq!(before, screen(&ui, 50, 15));
            ui.key(key(KeyCode::Left));
            ui.paste("l");
            assert_eq!(ui.key(key(KeyCode::Enter)), Action::Run);
        }
    }
}

use crate::{
    analyze::AnalysisProvider,
    fetch::{FetchScriptError, FetchedScript},
    parse::InstallCommand,
};
use std::{io, path::Path};
#[derive(Clone, Debug)]
pub enum SessionStart {
    Interactive,
    Direct { raw: String, parsed: InstallCommand },
}
#[derive(Clone, Debug)]
pub enum InstallDecision {
    Run {
        raw: String,
        parsed: InstallCommand,
        fetched: FetchedScript,
    },
    Cancel {
        raw: Option<String>,
        parsed: Option<InstallCommand>,
        fetched: Option<FetchedScript>,
    },
    FetchFailed {
        raw: String,
        parsed: InstallCommand,
        error: FetchScriptError,
    },
}
pub async fn run_session(
    start: SessionStart,
    provider: AnalysisProvider,
    appearance: Appearance,
    nerd_fonts: bool,
) -> Result<InstallDecision, Box<SessionError>> {
    use std::io::IsTerminal;
    let mut session = Session::new(start, appearance, nerd_fonts);
    session.ui.color_level = color_level(io::stderr().is_terminal());
    run_session_inner(&mut session, provider)
        .await
        .map_err(|error| Box::new(session.failure(error)))
}
async fn run_session_inner(
    session: &mut Session,
    provider: AnalysisProvider,
) -> io::Result<InstallDecision> {
    use crossterm::event::{self, Event};
    #[cfg(unix)]
    let signals = SessionSignals::open()?;
    let mut terminal = TerminalSession::open()?;
    let mut pipeline = session
        .parsed
        .as_ref()
        .map(|parsed| Pipeline::start(parsed.clone(), provider.clone()));
    loop {
        #[cfg(unix)]
        if signals.received() {
            return Ok(session.cancel(pipeline.as_mut()));
        }
        if !TerminalSession::active() {
            return Err(io::Error::other(
                "terminal released after a background panic",
            ));
        }
        terminal.terminal.draw(|frame| session.ui.render(frame))?;
        // Input is processed before asynchronous results so cancellation wins over
        // errors caused by aborting an in-flight request.
        // Crossterm's /dev/tty backend skips reads with a zero timeout.
        while event::poll(std::time::Duration::from_millis(1))? {
            let action = match event::read()? {
                Event::Key(key) => session.ui.key(key),
                Event::Paste(text) => {
                    session.ui.paste(&text);
                    Action::None
                }
                _ => Action::None,
            };
            match action {
                Action::Cancel => return Ok(session.cancel(pipeline.as_mut())),
                Action::Run => return Ok(session.decision(true)),
                Action::Submit(raw) => {
                    if session.submit(raw) {
                        pipeline = session
                            .parsed
                            .as_ref()
                            .map(|parsed| Pipeline::start(parsed.clone(), provider.clone()));
                    }
                }
                Action::None => {}
            }
        }
        if let Some(work) = &mut pipeline {
            loop {
                match work.updates.try_recv() {
                    Ok(Update::Fetched(fetched)) => session.fetched = Some(fetched),
                    Ok(Update::Resolved(result)) => {
                        if let Some(parsed) = &session.parsed {
                            session
                                .ui
                                .transition(Phase::Resolved(derive_view(result, &parsed.url)));
                        }
                        work.complete = true;
                    }
                    Ok(Update::Failed(error)) => {
                        return Ok(InstallDecision::FetchFailed {
                            raw: session.raw.clone().expect("fetch requires command"),
                            parsed: session.parsed.clone().expect("fetch requires parse"),
                            error,
                        });
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) if !work.complete => {
                        return Err(io::Error::other("analysis task stopped unexpectedly"));
                    }
                    Err(_) => break,
                }
            }
        }
        session.ui.tick = session.ui.tick.wrapping_add(1);
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

// Signal dispositions belong to the dialog, not the installer that runs afterward.
// Tokio retains its handlers after listener drop, so use a scoped OS registration.
#[cfg(unix)]
static SESSION_SIGNAL: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
#[cfg(unix)]
static SIGNAL_SESSION_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn session_signal(signal: libc::c_int) {
    // Lock-free atomic store only: no allocation, locks, or terminal I/O in a handler.
    SESSION_SIGNAL.store(signal, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(unix)]
struct SessionSignals {
    previous: Vec<(libc::c_int, libc::sigaction)>,
}
#[cfg(unix)]
impl SessionSignals {
    fn open() -> io::Result<Self> {
        use std::sync::atomic::Ordering;
        SIGNAL_SESSION_ACTIVE
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| io::Error::other("a terminal session is already active"))?;
        let mut guard = Self {
            previous: Vec::with_capacity(3),
        };
        SESSION_SIGNAL.store(0, Ordering::Relaxed);
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            // SAFETY: zeroed sigaction values are initialized with a valid handler,
            // empty mask and flags before installation; the OS fills `previous`.
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            let mut previous: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = session_signal as *const () as libc::sighandler_t;
            action.sa_flags = libc::SA_RESTART;
            unsafe { libc::sigemptyset(&mut action.sa_mask) };
            if unsafe { libc::sigaction(signal, &action, &mut previous) } == -1 {
                return Err(io::Error::last_os_error());
            }
            guard.previous.push((signal, previous));
        }
        Ok(guard)
    }
    fn received(&self) -> bool {
        SESSION_SIGNAL.load(std::sync::atomic::Ordering::Relaxed) != 0
    }
}
#[cfg(unix)]
impl Drop for SessionSignals {
    fn drop(&mut self) {
        for (signal, previous) in self.previous.iter().rev() {
            // SAFETY: restore the exact disposition returned by successful registration.
            unsafe { libc::sigaction(*signal, previous, std::ptr::null_mut()) };
        }
        SIGNAL_SESSION_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
    }
}

struct TerminalSession {
    terminal: ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stderr>>,
}
static TERMINAL_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static PANIC_HOOK: std::sync::Once = std::sync::Once::new();
impl TerminalSession {
    fn open() -> io::Result<Self> {
        use crossterm::{
            event::EnableBracketedPaste,
            execute,
            terminal::{EnterAlternateScreen, enable_raw_mode},
        };
        use std::io::IsTerminal;
        if !io::stderr().is_terminal() {
            return Err(io::Error::other("stderr is not a terminal"));
        }
        // A panic message must land on a restored screen, not inside the alternate one.
        PANIC_HOOK.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                if TERMINAL_ACTIVE.swap(false, std::sync::atomic::Ordering::AcqRel) {
                    restore_terminal();
                }
                previous(info);
            }));
        });
        TERMINAL_ACTIVE.store(true, std::sync::atomic::Ordering::Release);
        if let Err(error) = enable_raw_mode() {
            TERMINAL_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
            return Err(error);
        }
        if let Err(error) = execute!(
            io::stderr(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            crossterm::cursor::Hide
        ) {
            Self::release();
            return Err(error);
        }
        match ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(io::stderr())) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                Self::release();
                Err(error)
            }
        }
    }
    fn active() -> bool {
        TERMINAL_ACTIVE.load(std::sync::atomic::Ordering::Acquire)
    }
    fn release() {
        if TERMINAL_ACTIVE.swap(false, std::sync::atomic::Ordering::AcqRel) {
            restore_terminal();
        }
    }
}
impl Drop for TerminalSession {
    fn drop(&mut self) {
        Self::release();
    }
}
fn restore_terminal() {
    use crossterm::{
        event::DisableBracketedPaste,
        execute,
        terminal::{LeaveAlternateScreen, disable_raw_mode},
    };
    let _ = execute!(
        io::stderr(),
        DisableBracketedPaste,
        LeaveAlternateScreen,
        crossterm::cursor::Show
    );
    let _ = disable_raw_mode();
}
enum Update {
    Fetched(FetchedScript),
    Resolved(crate::analyze::AnalysisResult),
    Failed(FetchScriptError),
}
struct Pipeline {
    updates: tokio::sync::mpsc::UnboundedReceiver<Update>,
    cancel: tokio_util::sync::CancellationToken,
    task: tokio::task::JoinHandle<()>,
    complete: bool,
}
impl Pipeline {
    fn start(parsed: InstallCommand, provider: AnalysisProvider) -> Self {
        let (send, updates) = tokio::sync::mpsc::unbounded_channel();
        let cancel = tokio_util::sync::CancellationToken::new();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            let fetched = match crate::fetch::fetch_script(&parsed.url, &token).await {
                Ok(fetched) => fetched,
                Err(error) => {
                    let _ = send.send(Update::Failed(error));
                    return;
                }
            };
            let input = crate::analyze::AnalysisInput {
                url: parsed.url.clone(),
                final_url: Some(fetched.final_url.clone()),
                script_bytes: fetched.bytes.clone(),
                redacted_command: Some(crate::redact::redact_command(&parsed)),
            };
            if send.send(Update::Fetched(fetched)).is_err() {
                return;
            }
            let result = crate::analyze::analyze_script(input, provider, token).await;
            let _ = send.send(Update::Resolved(result));
        });
        Self {
            updates,
            cancel,
            task,
            complete: false,
        }
    }
}
impl Drop for Pipeline {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.task.abort();
    }
}

pub fn resolve_appearance(home: &Path) -> Appearance {
    use std::io::IsTerminal;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    appearance_with(
        home,
        std::env::var("SWEEP_THEME").ok().as_deref(),
        now,
        || {
            if !io::stderr().is_terminal() {
                return None;
            }
            let mut options = terminal_colorsaurus::QueryOptions::default();
            options.timeout = std::time::Duration::from_secs(2);
            let color = terminal_colorsaurus::background_color(options).ok()?;
            let (r, g, b) = color.scale_to_8bit();
            let luminance =
                (0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)) / 255.0;
            Some(if luminance > 0.5 {
                Appearance::Light
            } else {
                Appearance::Dark
            })
        },
    )
}
pub fn derive_source(source: &str) -> String {
    url::Url::parse(source)
        .map(|url| {
            let path = if url.path() == "/" { "" } else { url.path() };
            format!(
                "{}{path}",
                &url[url::Position::BeforeHost..url::Position::AfterPort]
            )
        })
        .unwrap_or_else(|_| source.into())
}
pub fn derive_view(result: crate::analyze::AnalysisResult, source: &str) -> InsightView {
    use crate::analyze::{AnalysisPass, AnalysisResult, ManipulationPass, Severity};
    let mut view = InsightView {
        kind: InsightKind::Clear,
        source: derive_source(source),
        message: String::new(),
        flags: vec![],
        behaviors: vec![],
    };
    match result {
        AnalysisResult::NoProvider => {
            view.kind = InsightKind::NoLlm;
            view.message = "No LLM provider configured — no analysis to show.".into();
        }
        AnalysisResult::Analyzed {
            analysis: AnalysisPass::Failed { reason },
            manipulation,
        } => {
            view.kind = if manipulation == ManipulationPass::Fired {
                InsightKind::Manipulation
            } else {
                InsightKind::AnalysisFailed
            };
            view.message = format!("Couldn't analyze: {reason}.");
        }
        AnalysisResult::Analyzed {
            analysis:
                AnalysisPass::Ok {
                    severity,
                    summary,
                    flags,
                    behaviors,
                },
            manipulation,
        } => {
            view.kind = if manipulation != ManipulationPass::Clean {
                InsightKind::Manipulation
            } else {
                match severity {
                    Severity::Clear => InsightKind::Clear,
                    Severity::Caution => InsightKind::Caution,
                    Severity::Danger => InsightKind::Danger,
                }
            };
            view.message = summary;
            view.flags = flags;
            view.behaviors = behaviors
                .into_iter()
                .map(|b| (b.description, b.sudo))
                .collect();
        }
    }
    view
}
struct Session {
    ui: Ui,
    raw: Option<String>,
    parsed: Option<InstallCommand>,
    fetched: Option<FetchedScript>,
}
impl Session {
    fn new(start: SessionStart, appearance: Appearance, nerd: bool) -> Self {
        let (phase, raw, parsed) = match start {
            SessionStart::Interactive => (Phase::Paste { error: None }, None, None),
            SessionStart::Direct { raw, parsed } => (
                Phase::Loading {
                    source: derive_source(&parsed.url),
                },
                Some(raw),
                Some(parsed),
            ),
        };
        Self {
            ui: Ui::new(phase, appearance, nerd),
            raw,
            parsed,
            fetched: None,
        }
    }
    fn submit(&mut self, raw: String) -> bool {
        let raw = raw.trim().to_string();
        match crate::parse::parse_install_command(&raw) {
            Ok(parsed) => {
                self.ui.transition(Phase::Loading {
                    source: derive_source(&parsed.url),
                });
                self.raw = Some(raw);
                self.parsed = Some(parsed);
                true
            }
            Err(error) => {
                self.ui.phase = Phase::Paste {
                    error: Some(error.message),
                };
                false
            }
        }
    }
    fn decision(&self, run: bool) -> InstallDecision {
        if run
            && matches!(self.ui.phase, Phase::Resolved(_))
            && let (Some(raw), Some(parsed), Some(fetched)) =
                (&self.raw, &self.parsed, &self.fetched)
        {
            return InstallDecision::Run {
                raw: raw.clone(),
                parsed: parsed.clone(),
                fetched: fetched.clone(),
            };
        }
        InstallDecision::Cancel {
            raw: self.raw.clone(),
            parsed: self.parsed.clone(),
            fetched: self.fetched.clone(),
        }
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use crate::analyze::{AnalysisPass, AnalysisResult, ManipulationPass, Severity};
    #[test]
    fn failed_manipulation_is_untrusted() {
        let view = derive_view(
            AnalysisResult::Analyzed {
                analysis: AnalysisPass::Ok {
                    severity: Severity::Clear,
                    summary: "Summary".into(),
                    flags: vec![],
                    behaviors: vec![],
                },
                manipulation: ManipulationPass::Failed {
                    reason: "Timeout".into(),
                },
            },
            "https://example.com/install?secret=yes#hash",
        );
        assert_eq!(view.kind, InsightKind::Manipulation);
        assert_eq!(view.source, "example.com/install");
    }
    #[test]
    fn fired_manipulation_requires_typed_approval_when_analysis_fails() {
        let view = derive_view(
            AnalysisResult::Analyzed {
                analysis: AnalysisPass::Failed {
                    reason: "Timeout".into(),
                },
                manipulation: ManipulationPass::Fired,
            },
            "https://example.com/",
        );
        assert_eq!(view.kind, InsightKind::Manipulation);
        assert_eq!(view.message, "Couldn't analyze: Timeout.");
        assert_eq!(view.source, "example.com");
        let mut ui = Ui::new(Phase::Resolved(view), Appearance::Dark, false);
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        assert_eq!(ui.key(key(KeyCode::Enter)), Action::None);
        ui.key(key(KeyCode::Right));
        assert_eq!(ui.key(key(KeyCode::Enter)), Action::None);
        ui.paste("INSTALL");
        assert_eq!(ui.key(key(KeyCode::Enter)), Action::None);
        ui.paste("install");
        assert_eq!(ui.key(key(KeyCode::Enter)), Action::Run);
    }
    #[test]
    fn invalid_paste_stays_inline_and_never_becomes_an_invocation() {
        let mut session = Session::new(SessionStart::Interactive, Appearance::Dark, false);
        assert!(!session.submit("not an install".into()));
        assert!(matches!(session.ui.phase, Phase::Paste { error: Some(_) }));
        assert!(matches!(
            session.decision(false),
            InstallDecision::Cancel { raw: None, .. }
        ));
        assert!(session.submit("  curl https://example.com/install | sh  ".into()));
        assert!(matches!(session.ui.phase, Phase::Loading { .. }));
        assert!(
            matches!(session.decision(false),InstallDecision::Cancel{raw:Some(raw),..} if raw=="curl https://example.com/install | sh")
        );
    }
    #[test]
    fn canceled_analysis_preserves_fetched_script_for_persistence() {
        let raw = "curl https://example.com/install | sh".to_string();
        let parsed = crate::parse::parse_install_command(&raw).unwrap();
        let mut session = Session::new(
            SessionStart::Direct {
                raw: raw.clone(),
                parsed,
            },
            Appearance::Dark,
            false,
        );
        session.fetched = Some(FetchedScript {
            bytes: b"echo safe".to_vec(),
            sha256: "hash".into(),
            final_url: "https://example.com/install".into(),
            fetched_at: "2026-10-06T00:00:00.000Z".into(),
            status: 200,
        });
        assert!(
            matches!(session.decision(false),InstallDecision::Cancel{raw:Some(r),parsed:Some(_),fetched:Some(f)} if r==raw && f.bytes==b"echo safe")
        );
        assert!(
            matches!(session.decision(true), InstallDecision::Cancel { .. }),
            "Loading cannot bypass approval"
        );
    }
}
fn appearance_with(
    home: &Path,
    env_override: Option<&str>,
    now: u64,
    probe: impl FnOnce() -> Option<Appearance>,
) -> Appearance {
    let parse = |value: &str| match value {
        "dark" => Some(Appearance::Dark),
        "light" => Some(Appearance::Light),
        _ => None,
    };
    if let Some(value) = env_override.and_then(parse) {
        return value;
    }
    let path = home.join("cache/appearance.json");
    if let Ok(bytes) = std::fs::read(&path)
        && let Ok(cache) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && let (Some(ts), Some(value)) = (
            cache["ts"].as_u64(),
            cache["appearance"].as_str().and_then(parse),
        )
        && ts.saturating_add(3_600_000) >= now
    {
        return value;
    }
    if let Some(value) = probe() {
        let _ = std::fs::create_dir_all(home.join("cache"));
        let _=std::fs::write(path,serde_json::json!({"appearance":if value==Appearance::Dark {"dark"} else {"light"},"ts":now}).to_string());
        return value;
    }
    Appearance::Dark
}
#[cfg(test)]
mod appearance_tests {
    use super::*;
    #[test]
    fn environment_and_fresh_cache_avoid_terminal_probe() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join("cache")).unwrap();
        std::fs::write(
            home.path().join("cache/appearance.json"),
            r#"{"appearance":"light","ts":1000}"#,
        )
        .unwrap();
        assert_eq!(
            appearance_with(home.path(), Some("dark"), 2000, || panic!("env must win")),
            Appearance::Dark
        );
        assert_eq!(
            appearance_with(home.path(), None, 2000, || panic!("cache must win")),
            Appearance::Light
        );
        assert_eq!(
            appearance_with(home.path(), None, 4_000_000, || Some(Appearance::Dark)),
            Appearance::Dark
        );
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct SessionError {
    pub error: io::Error,
    pub raw: Option<String>,
    pub parsed: Option<InstallCommand>,
    pub fetched: Option<FetchedScript>,
}
impl Session {
    fn failure(&self, error: io::Error) -> SessionError {
        SessionError {
            error,
            raw: self.raw.clone(),
            parsed: self.parsed.clone(),
            fetched: self.fetched.clone(),
        }
    }
}
#[cfg(test)]
mod failure_tests {
    use super::*;
    #[test]
    fn terminal_failures_keep_committed_command_but_not_rejected_paste() {
        let mut session = Session::new(SessionStart::Interactive, Appearance::Dark, false);
        session.submit("invalid".into());
        assert!(
            session
                .failure(io::Error::other("terminal disconnected"))
                .raw
                .is_none()
        );
        session.submit("curl https://example.com | sh".into());
        let error = session.failure(io::Error::other("terminal disconnected"));
        assert_eq!(error.raw.as_deref(), Some("curl https://example.com | sh"));
        assert!(error.parsed.is_some());
        assert_eq!(error.to_string(), "terminal disconnected");
    }
}

pub fn color_level(is_tty: bool) -> u8 {
    color_level_with(is_tty, &std::env::vars().collect())
}
fn color_level_with(is_tty: bool, env: &std::collections::HashMap<String, String>) -> u8 {
    if env.contains_key("NO_COLOR") {
        return 0;
    }
    if let Some(force) = env.get("FORCE_COLOR") {
        return force
            .parse::<i64>()
            .map(|n| n.clamp(0, 3) as u8)
            .unwrap_or(1);
    }
    if !is_tty {
        return 0;
    }
    let term = env.get("TERM").map(String::as_str).unwrap_or_default();
    if term == "dumb" {
        return 0;
    }
    if matches!(
        env.get("COLORTERM").map(String::as_str),
        Some("truecolor" | "24bit")
    ) {
        return 3;
    }
    if [
        "KITTY_WINDOW_ID",
        "WT_SESSION",
        "ALACRITTY_LOG",
        "ALACRITTY_SOCKET",
        "KONSOLE_VERSION",
        "WEZTERM_EXECUTABLE",
    ]
    .iter()
    .any(|key| env.contains_key(*key))
    {
        return 3;
    }
    if matches!(
        env.get("TERM_PROGRAM").map(String::as_str),
        Some("iTerm.app" | "vscode" | "ghostty" | "WezTerm" | "Hyper")
    ) {
        return 3;
    }
    if env
        .get("VTE_VERSION")
        .and_then(|s| s.parse::<u64>().ok())
        .is_some_and(|v| v >= 3600)
    {
        return 3;
    }
    if term.contains("-256") {
        return 2;
    }
    if matches!(
        term,
        "linux" | "vt100" | "vt220" | "vt320" | "ansi" | "cons25"
    ) {
        return 1;
    }
    2
}
pub fn quantize(color: Color, level: u8) -> Color {
    if level == 0 {
        return Color::Reset;
    }
    let Color::Rgb(r, g, b) = color else {
        return color;
    };
    if level >= 3 {
        return color;
    }
    if level == 2 {
        if r == g && g == b {
            return Color::Indexed(if r < 8 {
                16
            } else if r > 248 {
                231
            } else {
                (((f64::from(r) - 8.0) / 246.0 * 24.0).round() as u8).saturating_add(232)
            });
        }
        let levels = [0_i32, 95, 135, 175, 215, 255];
        let closest = |c: u8| {
            levels
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| (i32::from(c) - **v).abs())
                .map_or(0, |(i, _)| i as u8)
        };
        return Color::Indexed(16 + 36 * closest(r) + 6 * closest(g) + closest(b));
    }
    const PALETTE: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (170, 0, 0),
        (0, 170, 0),
        (170, 85, 0),
        (0, 0, 170),
        (170, 0, 170),
        (0, 170, 170),
        (170, 170, 170),
        (85, 85, 85),
        (255, 85, 85),
        (85, 255, 85),
        (255, 255, 85),
        (85, 85, 255),
        (255, 85, 255),
        (85, 255, 255),
        (255, 255, 255),
    ];
    let index = PALETTE
        .iter()
        .enumerate()
        .min_by_key(|(_, (rr, gg, bb))| {
            (i32::from(r) - i32::from(*rr)).pow(2)
                + (i32::from(g) - i32::from(*gg)).pow(2)
                + (i32::from(b) - i32::from(*bb)).pow(2)
        })
        .map_or(7, |(i, _)| i);
    BASIC_COLORS[index]
}
/// The 16 ANSI colors in SGR order: 30-37 then 90-97.
pub const BASIC_COLORS: [Color; 16] = [
    Color::Black,
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::Gray,
    Color::DarkGray,
    Color::LightRed,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightMagenta,
    Color::LightCyan,
    Color::White,
];

#[cfg(test)]
mod color_tests {
    use super::*;
    use std::collections::HashMap;
    #[test]
    fn explicit_no_color_wins_and_terminal_capabilities_are_honored() {
        let mut env = HashMap::from([
            ("NO_COLOR".into(), "".into()),
            ("FORCE_COLOR".into(), "3".into()),
        ]);
        assert_eq!(color_level_with(true, &env), 0);
        env.remove("NO_COLOR");
        assert_eq!(color_level_with(false, &env), 3);
        env.clear();
        assert_eq!(color_level_with(false, &env), 0);
        env.insert("TERM".into(), "dumb".into());
        assert_eq!(color_level_with(true, &env), 0);
        env.insert("TERM".into(), "linux".into());
        assert_eq!(color_level_with(true, &env), 1);
        env.insert("TERM".into(), "xterm-256color".into());
        assert_eq!(color_level_with(true, &env), 2);
        env.insert("COLORTERM".into(), "truecolor".into());
        assert_eq!(color_level_with(true, &env), 3);
    }
    #[test]
    fn no_color_buffer_keeps_risk_warning_and_confirmation() {
        let mut ui = Ui::new(
            Phase::Resolved(InsightView {
                kind: InsightKind::Danger,
                source: "example.com".into(),
                message: "Risk".into(),
                flags: vec![],
                behaviors: vec![],
            }),
            Appearance::Dark,
            false,
        );
        ui.color_level = 0;
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| ui.render(f)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(
            buffer
                .content
                .iter()
                .all(|c| c.fg == Color::Reset && c.bg == Color::Reset)
        );
        let text = buffer
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("danger"));
        assert!(text.contains("Type 'install' to run:"));
    }
}
#[cfg(test)]
mod gradient_tests {
    use super::*;
    #[test]
    fn gradient_preserves_perceptual_midpoint() {
        assert_eq!(
            gradient(Color::Rgb(80, 160, 255), Color::Rgb(40, 60, 100), 1, 2),
            Color::Rgb(60, 108, 174)
        );
    }
}
#[cfg(test)]
mod action_tests {
    use super::*;
    #[test]
    fn primary_keyboard_hint_keeps_gold_accent_and_compact_action_spacing() {
        let ui = Ui::new(
            Phase::Resolved(InsightView {
                kind: InsightKind::Clear,
                source: "example.com".into(),
                message: "Summary".into(),
                flags: vec![],
                behaviors: vec![],
            }),
            Appearance::Dark,
            false,
        );
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| ui.render(f)).unwrap();
        let buffer = terminal.backend().buffer();
        let text = buffer
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("Esc Cancel │ Enter Run"));
        assert!(buffer.content.iter().any(|c| c.symbol() == "E"
            && c.fg == Color::Rgb(245, 186, 74)
            && c.modifier.contains(Modifier::BOLD)));
    }
}
impl Session {
    fn cancel(&mut self, pipeline: Option<&mut Pipeline>) -> InstallDecision {
        if let Some(pipeline) = pipeline {
            pipeline.cancel.cancel();
            while let Ok(update) = pipeline.updates.try_recv() {
                if let Update::Fetched(fetched) = update {
                    self.fetched = Some(fetched);
                }
            }
        }
        self.decision(false)
    }
}
#[cfg(test)]
mod cancellation_tests {
    use super::*;
    #[tokio::test]
    async fn cancel_keeps_completed_fetch_even_before_next_render_tick() {
        let raw = "curl https://example.com/install | sh".to_string();
        let parsed = crate::parse::parse_install_command(&raw).unwrap();
        let mut session = Session::new(
            SessionStart::Direct { raw, parsed },
            Appearance::Dark,
            false,
        );
        let (send, updates) = tokio::sync::mpsc::unbounded_channel();
        send.send(Update::Fetched(FetchedScript {
            bytes: b"echo fixture".to_vec(),
            sha256: "hash".into(),
            final_url: "https://example.com/install".into(),
            fetched_at: "2026-10-06T00:00:00Z".into(),
            status: 200,
        }))
        .unwrap();
        let cancel = tokio_util::sync::CancellationToken::new();
        let mut pipeline = Pipeline {
            updates,
            cancel: cancel.clone(),
            task: tokio::spawn(async {}),
            complete: false,
        };
        assert!(matches!(
            session.cancel(Some(&mut pipeline)),
            InstallDecision::Cancel {
                fetched: Some(_),
                ..
            }
        ));
        assert!(cancel.is_cancelled());
        assert!(matches!(session.ui.phase, Phase::Loading { .. }));
    }
}
#[cfg(test)]
mod word_navigation_tests {
    use super::*;
    #[test]
    fn alt_arrows_move_across_words_without_deleting_command_text() {
        let mut ui = Ui::new(Phase::Paste { error: None }, Appearance::Dark, false);
        ui.paste("curl https://example.com | sh");
        ui.key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
        assert_eq!(&ui.input[ui.cursor..], "sh");
        ui.key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
        assert_eq!(&ui.input[ui.cursor..], "example.com | sh");
        ui.key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        assert_eq!(&ui.input[ui.cursor..], " | sh");
        assert_eq!(ui.input, "curl https://example.com | sh");
    }
}

#[cfg(test)]
mod grapheme_tests {
    use super::*;
    #[test]
    fn backspace_removes_one_visible_character_including_emoji_and_accents() {
        let mut ui = Ui::new(Phase::Paste { error: None }, Appearance::Dark, false);
        ui.paste("a👨‍👩‍👧‍👦e\u{301}");
        ui.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(ui.input, "a👨‍👩‍👧‍👦");
        ui.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(ui.input, "a");
    }
}
#[cfg(test)]
mod kill_buffer_tests {
    use super::*;
    #[test]
    fn deleted_words_can_be_yanked_and_no_op_kills_preserve_previous_buffer() {
        let mut ui = Ui::new(Phase::Paste { error: None }, Appearance::Dark, false);
        ui.paste("curl example");
        ui.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT));
        assert_eq!(ui.input, "curl ");
        ui.key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
        assert_eq!(ui.input, "curl example");
        ui.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(ui.input, "");
        ui.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        ui.key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
        ui.key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
        assert_eq!(ui.input, "curl example");
    }
}
