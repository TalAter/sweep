//! Draws the setup wizard's current screen into a ratatui frame, reusing the
//! session card chrome (border gradient, action-bar colors, color degradation).
use super::providers::{self, Kind, Provider};
use super::screens::{Screen, Wizard};
use crate::tui::{self, Appearance, Palette, SPINNER};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

/// Wizard-only color tokens; `(fg, bg)` pairs for pill segments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WizardPalette {
    pub wizard: (Color, Color),
    pub done: (Color, Color),
    pub active: (Color, Color),
    pub pending: (Color, Color),
    pub muted: Color,
    pub success: Color,
    pub focus_bg: Color,
    pub header: Color,
    pub logo: (Color, Color),
    pub gold: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub(super) fn wizard_palette(appearance: Appearance) -> WizardPalette {
    let gold = tui::action_colors(appearance).1;
    match appearance {
        Appearance::Dark => WizardPalette {
            wizard: (rgb(0xd2d2e1), rgb(0x1e325a)),
            done: (rgb(0x82dca0), rgb(0x285a3c)),
            active: (rgb(0xb4c8ff), rgb(0x3c50a0)),
            pending: (rgb(0x64648c), rgb(0x323250)),
            muted: rgb(0x73738c),
            success: rgb(0x78e6a0),
            focus_bg: rgb(0x1a2a4d),
            header: rgb(0x3c3c64),
            logo: (rgb(0xb3b3b3), rgb(0xffffff)),
            gold,
        },
        Appearance::Light => WizardPalette {
            wizard: (rgb(0x0a145a), rgb(0x96afdc)),
            done: (rgb(0x0f6e37), rgb(0xd2f0d7)),
            active: (rgb(0x1e4bbe), rgb(0xd2e1fa)),
            pending: (rgb(0x82829b), rgb(0xdcdce6)),
            muted: rgb(0x696982),
            success: rgb(0x0f7d37),
            focus_bg: rgb(0xd2dcf5),
            header: rgb(0xaaaac3),
            logo: (rgb(0xa079bf), rgb(0x4f5e85)),
            gold,
        },
    }
}

const LOGO: [&str; 6] = [
    "███████╗██╗    ██╗███████╗███████╗██████╗",
    "██╔════╝██║    ██║██╔════╝██╔════╝██╔══██╗",
    "███████╗██║ █╗ ██║█████╗  █████╗  ██████╔╝",
    "╚════██║██║███╗██║██╔══╝  ██╔══╝  ██╔═══╝",
    "███████║╚███╔███╔╝███████╗███████╗██║",
    "╚══════╝ ╚══╝╚══╝ ╚══════╝╚══════╝╚═╝",
];
const NERD_SAMPLE: &str = "\u{F08D9}  \u{F1D0}  \u{EDD6}  \u{F1D1}";
const DISCLAIMER_HEAD: &str = "Sweep will route your queries through the ";
const DISCLAIMER_TAIL: &str = " CLI instead of calling the Anthropic API directly. This is slower, and your prompts flow through Claude Code under its own terms — bring your own subscription and credentials.";
const MAX_CARD_WIDTH: u16 = 74;

/// Everything the card needs besides its border: body lines, the action bar, and
/// which body line (if any) carries the cursor or the key editor.
struct Content {
    lines: Vec<Line<'static>>,
    indent: u16,
    cursor_line: Option<usize>,
    input_line: Option<usize>,
    actions: Vec<(&'static str, &'static str)>,
}
impl Content {
    /// Appends a list row; the focused one becomes the line scrolling keeps on screen.
    fn push_row(&mut self, line: Line<'static>, focused: bool) {
        if focused {
            self.cursor_line = Some(self.lines.len());
        }
        self.lines.push(line);
    }
}

pub(super) fn render(w: &Wizard, frame: &mut Frame<'_>) {
    let area = frame.area();
    if area.width < 10 || area.height < 4 {
        return;
    }
    let Palette {
        body,
        supporting,
        surface,
        ..
    } = tui::palette(w.appearance);
    let wp = wizard_palette(w.appearance);
    let width = MAX_CARD_WIDTH.min(area.width - 4);
    let content_width = width - 4;
    let mut content = content(w, content_width, &wp);
    let body_width = content_width.saturating_sub(content.indent).max(1);
    // Scrolling and the editor row work in wrapped visual rows, not logical lines;
    // `first_row[i]` is the visual row where logical line `i` starts.
    let mut first_row = Vec::with_capacity(content.lines.len() + 1);
    let mut body_rows = 0usize;
    for line in &content.lines {
        first_row.push(body_rows);
        body_rows += Paragraph::new(line.clone())
            .wrap(Wrap { trim: false })
            .line_count(body_width);
    }
    first_row.push(body_rows);
    let cursor_row = content
        .cursor_line
        .or(content.input_line)
        .map(|l| first_row[l]);
    let input_row = content.input_line.map(|l| first_row[l]);
    let body_rows = body_rows.min(u16::MAX as usize) as u16;
    // Padding rows (below the top border, around the action bar) are dropped when the
    // terminal cannot fit the padded body, keeping the action bar and cursor row on screen.
    let pad = u16::from(body_rows.saturating_add(6) <= area.height);
    let height = body_rows.saturating_add(3 + 3 * pad).min(area.height);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    let (start, end) = if w.color_level < 3 {
        (body, body)
    } else {
        tui::frame_gradient(w.appearance)
    };
    let status = matches!(w.screen, Screen::Loading).then(|| {
        truncate(
            "Loading models list…",
            rect.width.saturating_sub(6) as usize,
        )
    });
    tui::draw_card_border(
        frame.buffer_mut(),
        rect,
        start,
        end,
        status.as_deref().map(|text| (text, body)),
    );
    draw_pill(w, frame.buffer_mut(), rect, &wp);

    let inner = Rect::new(
        rect.x + 2,
        rect.y + 1 + pad,
        content_width,
        rect.height - 2 - 2 * pad,
    );
    let body_height = inner.height.saturating_sub(1 + pad);
    let max_scroll = body_rows.saturating_sub(body_height) as usize;
    let mut scroll = w.scroll.get();
    if let Some(cursor) = cursor_row {
        if cursor < scroll {
            scroll = cursor;
        }
        if cursor >= scroll + body_height as usize {
            scroll = cursor + 1 - body_height as usize;
        }
    }
    scroll = scroll.min(max_scroll);
    w.scroll.set(scroll);
    let scrollable = matches!(w.screen, Screen::Welcome | Screen::Disclaimer { .. });
    if scrollable && max_scroll > 0 {
        content.actions.insert(0, ("↑↓", "to scroll"));
    }
    let body_area = Rect::new(inner.x + content.indent, inner.y, body_width, body_height);
    frame.render_widget(
        Paragraph::new(content.lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll as u16, 0)),
        body_area,
    );
    if let Some(line) = input_row
        && line >= scroll
        && line - scroll < body_height as usize
    {
        let y = inner.y + (line - scroll) as u16;
        let row = Rect::new(inner.x, y, content_width, 1);
        frame
            .buffer_mut()
            .set_style(row, Style::default().bg(surface));
        let placeholder = match w.screen {
            Screen::ApiKey { provider } => provider.placeholder.unwrap_or(""),
            _ => "",
        };
        w.editor.render(
            frame,
            Rect::new(inner.x + 1, y, content_width.saturating_sub(2).max(1), 1),
            placeholder,
            Style::default().fg(body).bg(surface),
            Style::default().fg(wp.muted).bg(surface),
        );
    }
    if inner.height >= 1 {
        let action_y = inner.bottom() - 1;
        frame.render_widget(
            Paragraph::new(action_bar(&content.actions, w.appearance, supporting)),
            Rect::new(inner.x + 3, action_y, content_width.saturating_sub(3), 1),
        );
    }
    tui::apply_color_level(frame.buffer_mut(), w.color_level, wp.focus_bg);
}

fn truncate(text: &str, max: usize) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        if out.width() + c.to_string().width() + 1 > max {
            break;
        }
        out.push(c);
    }
    if max > 0 {
        out.push('…');
    }
    out
}

/// Key hints: bold glyph then a lowercase label, joined by ` │ `. The primary key
/// (`⏎` when present, otherwise the first key) is gold; the rest use the supporting color.
fn action_bar(
    actions: &[(&'static str, &'static str)],
    appearance: Appearance,
    supporting: Color,
) -> Line<'static> {
    let (label, primary, separator) = tui::action_colors(appearance);
    let primary_index = actions
        .iter()
        .position(|(glyph, _)| *glyph == "⏎")
        .unwrap_or(0);
    let mut spans = Vec::new();
    for (i, (glyph, text)) in actions.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(separator)));
        }
        let fg = if i == primary_index {
            primary
        } else {
            supporting
        };
        spans.push(Span::styled(*glyph, Style::default().fg(fg).bold()));
        spans.push(Span::styled(format!(" {text}"), Style::default().fg(label)));
    }
    Line::from(spans)
}

fn content(w: &Wizard, width: u16, wp: &WizardPalette) -> Content {
    let Palette {
        body, supporting, ..
    } = tui::palette(w.appearance);
    let plain = |s: &str, fg: Color| Line::styled(s.to_string(), Style::default().fg(fg));
    let bold = |s: String| Line::styled(s, Style::default().fg(body).bold());
    let blank = Line::default;
    let pointer = |focused: bool| {
        Span::styled(
            if focused { "❯ " } else { "  " },
            Style::default().fg(wp.gold),
        )
    };
    let mut content = Content {
        lines: Vec::new(),
        indent: 0,
        cursor_line: None,
        input_line: None,
        actions: Vec::new(),
    };
    match w.screen {
        Screen::Welcome => {
            content.indent = 3;
            for (i, row) in LOGO.iter().enumerate() {
                let fg = tui::gradient(wp.logo.0, wp.logo.1, i as u16, LOGO.len() as u16 - 1);
                content
                    .lines
                    .push(Line::styled(*row, Style::default().fg(fg).bold()));
            }
            let brew = Span::styled("brew", Style::default().fg(wp.gold).bold());
            let text = |s: &'static str| Span::styled(s, Style::default().fg(body));
            content.lines.push(Line::from(vec![
                brew.clone(),
                text(" for the things you cannot "),
                brew,
                text("."),
            ]));
            content.lines.extend([blank(), blank()]);
            content.lines.push(plain(
                "Quick one-time setup — pick your AI provider and a couple of preferences.",
                body,
            ));
            content.lines.extend([blank(), blank()]);
            content.lines.push(Line::from(vec![
                text("◷ Takes "),
                Span::styled("~45 seconds.", Style::default().fg(wp.success)),
            ]));
            content.lines.push(blank());
            content.actions = vec![("⏎", "to continue"), ("Esc", "to cancel")];
        }
        Screen::NerdFonts => {
            content
                .lines
                .push(plain("Do you see four icons below?", body));
            content.lines.push(blank());
            content
                .lines
                .push(plain(&format!("    {NERD_SAMPLE}"), body));
            content.lines.push(blank());
            for (i, label) in [
                "Yes — enable icons throughout Sweep",
                "No — they look like boxes or question marks",
            ]
            .into_iter()
            .enumerate()
            {
                let focused = i == w.cursor;
                content.push_row(
                    Line::from(vec![
                        Span::raw("  "),
                        pointer(focused),
                        Span::styled(
                            label,
                            Style::default().fg(if focused { body } else { wp.muted }),
                        ),
                    ]),
                    focused,
                );
            }
            content.actions = vec![("↑↓", "to move"), ("⏎", "to select")];
        }
        Screen::Providers => {
            content.lines.push(plain(
                "Sweep needs at least one LLM provider configured.",
                body,
            ));
            let header = |label: &str| section_header(label, width, wp, supporting);
            let row = |content: &mut Content, i: usize, provider: &Provider| {
                let focused = i == w.cursor;
                let checked = w.checked.get(i).copied().unwrap_or(false);
                let fg = if checked {
                    wp.success
                } else if focused {
                    body
                } else {
                    wp.muted
                };
                let mut spans = vec![
                    Span::styled(
                        if focused { " ❯ " } else { "   " },
                        Style::default().fg(wp.gold),
                    ),
                    Span::styled(
                        if checked { "[✓] " } else { "[ ] " },
                        Style::default().fg(fg),
                    ),
                ];
                if w.nerd_fonts {
                    spans.push(Span::styled(
                        format!("{} ", provider.glyph),
                        Style::default().fg(fg),
                    ));
                }
                spans.push(Span::styled(provider.display, Style::default().fg(fg)));
                let mut line = Line::from(spans);
                if focused {
                    let pad = (width as usize).saturating_sub(line.width());
                    line.push_span(Span::raw(" ".repeat(pad)));
                    line = line.style(Style::default().bg(wp.focus_bg));
                }
                content.push_row(line, focused);
            };
            content.lines.push(blank());
            content.lines.push(header("SELECT API PROVIDER(S)"));
            let api = w
                .available
                .iter()
                .enumerate()
                .filter(|(_, p)| p.kind == Kind::Api);
            for (i, provider) in api {
                row(&mut content, i, provider);
            }
            let cli: Vec<_> = w
                .available
                .iter()
                .enumerate()
                .filter(|(_, p)| matches!(p.kind, Kind::Cli { .. }))
                .collect();
            if !cli.is_empty() {
                content.lines.push(blank());
                content
                    .lines
                    .push(header("USE YOUR CODING AGENT'S SUBSCRIPTION"));
                for (i, provider) in cli {
                    row(&mut content, i, provider);
                }
            }
            let any = w.checked.iter().any(|c| *c);
            content.actions = vec![
                ("Space", "to toggle"),
                (
                    "⏎",
                    if any {
                        "to continue"
                    } else {
                        "to continue without an LLM"
                    },
                ),
            ];
        }
        Screen::Loading => {
            let spinner = SPINNER[(w.tick / 3) % SPINNER.len()];
            content
                .lines
                .push(plain(&format!("{spinner} Loading models…"), body));
            content.actions = vec![("Esc", "to cancel")];
        }
        Screen::ApiKey { provider } => {
            content
                .lines
                .push(bold(format!("{} API key", provider.display)));
            if let Some(url) = provider.key_url {
                content
                    .lines
                    .push(plain(&format!("Get one: {url}"), wp.muted));
            }
            content.lines.push(blank());
            content.input_line = Some(content.lines.len());
            content.lines.push(blank());
            content.actions = vec![("⏎", "to continue")];
        }
        Screen::Model { provider } => {
            let models = w.models_for(provider);
            if models.is_empty() {
                content.lines.push(plain(
                    &format!("No models found for {}.", provider.display),
                    body,
                ));
                content.actions = vec![("Esc", "to skip this provider")];
            } else {
                content
                    .lines
                    .push(bold(format!("{} model", provider.display)));
                content.lines.push(blank());
                for (i, model) in models.iter().enumerate() {
                    let focused = i == w.cursor;
                    let fg = if focused { body } else { supporting };
                    let mut spans = vec![
                        pointer(focused),
                        Span::styled(model.id.clone(), Style::default().fg(fg)),
                    ];
                    if model.recommended {
                        spans.push(Span::styled(
                            "  ✦ Recommended",
                            Style::default().fg(wp.success),
                        ));
                    }
                    content.push_row(Line::from(spans), focused);
                }
                content.actions = vec![("↑↓", "to move"), ("⏎", "to continue")];
            }
        }
        Screen::Disclaimer { .. } => {
            content.lines.push(Line::from(vec![
                Span::styled(DISCLAIMER_HEAD, Style::default().fg(body)),
                Span::styled("claude", Style::default().fg(body).bold()),
                Span::styled(DISCLAIMER_TAIL, Style::default().fg(body)),
            ]));
            content.actions = vec![("⏎", "to accept"), ("Esc", "to skip this provider")];
        }
        Screen::Default => {
            content
                .lines
                .push(bold("Which provider should be the default?".into()));
            content.lines.push(blank());
            for (i, entry) in w.configured.iter().enumerate() {
                let focused = i == w.cursor;
                let display = providers::find(&entry.key).map_or(entry.key.as_str(), |p| p.display);
                content.push_row(
                    Line::from(vec![
                        pointer(focused),
                        Span::styled(
                            display.to_string(),
                            Style::default().fg(if focused { body } else { supporting }),
                        ),
                    ]),
                    focused,
                );
            }
            content.actions = vec![("↑↓", "to move"), ("⏎", "to select")];
        }
    }
    content
}

/// `⠶⠶ LABEL ⠶⠶…` filled to `width`; just the label when there is no room for
/// eight columns of dots around it.
fn section_header(label: &str, width: u16, wp: &WizardPalette, supporting: Color) -> Line<'static> {
    let width = width as usize;
    let dots = Style::default().fg(wp.header);
    let label_style = Style::default().fg(supporting).bold();
    if width < label.width() + 8 {
        return Line::styled(label.to_string(), label_style);
    }
    let rest = width - label.width() - 4;
    Line::from(vec![
        Span::styled("⠶⠶ ", dots),
        Span::styled(label.to_string(), label_style),
        Span::styled(format!(" {}", "⠶".repeat(rest)), dots),
    ])
}

#[derive(Clone, Copy)]
enum StepId {
    Providers,
    ApiKey,
    Model,
    SetDefault,
}
impl StepId {
    fn label(self) -> &'static str {
        match self {
            StepId::Providers => "Providers",
            StepId::ApiKey => "API Key",
            StepId::Model => "Model",
            StepId::SetDefault => "Set Default",
        }
    }
    fn abbr(self) -> &'static str {
        match self {
            StepId::Providers => "P",
            StepId::ApiKey => "K",
            StepId::Model => "M",
            StepId::SetDefault => "D",
        }
    }
    fn icon(self) -> char {
        match self {
            StepId::Providers => '\u{F03A}',
            StepId::ApiKey => '\u{F084}',
            StepId::Model => '\u{F1B2}',
            StepId::SetDefault => '\u{F005}',
        }
    }
}

enum Crumb {
    Wizard,
    Provider(&'static Provider),
    Done(StepId),
    Active(StepId),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    Wide,
    Narrow,
    Abbr,
}

struct Segment {
    text: String,
    fg: Color,
    bg: Color,
    bold: bool,
}

fn crumbs(screen: &Screen) -> Vec<Crumb> {
    match *screen {
        Screen::Welcome => vec![],
        Screen::NerdFonts => vec![Crumb::Wizard],
        Screen::Providers | Screen::Loading => {
            vec![Crumb::Wizard, Crumb::Active(StepId::Providers)]
        }
        Screen::ApiKey { provider } | Screen::Disclaimer { provider } => vec![
            Crumb::Wizard,
            Crumb::Provider(provider),
            Crumb::Active(StepId::ApiKey),
        ],
        Screen::Model { provider } => vec![
            Crumb::Wizard,
            Crumb::Provider(provider),
            Crumb::Done(StepId::ApiKey),
            Crumb::Active(StepId::Model),
        ],
        Screen::Default => vec![
            Crumb::Wizard,
            Crumb::Done(StepId::Providers),
            Crumb::Active(StepId::SetDefault),
        ],
    }
}

fn segments(w: &Wizard, variant: Variant, wp: &WizardPalette) -> Vec<Segment> {
    let nerd = w.nerd_fonts;
    let done_mark = if nerd { "\u{F00C}" } else { "✓" };
    let wide = variant == Variant::Wide;
    let seg = |text: String, (fg, bg): (Color, Color), bold: bool| Segment {
        text: format!(" {text} "),
        fg,
        bg,
        bold,
    };
    crumbs(&w.screen)
        .into_iter()
        .filter_map(|crumb| match crumb {
            Crumb::Wizard => wide.then(|| seg("🧙 Setup Wizard".into(), wp.wizard, true)),
            Crumb::Provider(p) => match (variant, nerd) {
                (Variant::Abbr, false) => None,
                (Variant::Abbr, true) => Some(seg(p.glyph.to_string(), wp.pending, false)),
                (_, true) => Some(seg(format!("{} {}", p.glyph, p.display), wp.pending, false)),
                (_, false) => Some(seg(p.display.into(), wp.pending, false)),
            },
            Crumb::Done(step) => Some(seg(
                if wide {
                    format!("{done_mark} {}", step.label())
                } else {
                    done_mark.into()
                },
                wp.done,
                false,
            )),
            Crumb::Active(step) => Some(seg(
                match (variant, nerd) {
                    (Variant::Abbr, true) => step.icon().to_string(),
                    (Variant::Abbr, false) => step.abbr().into(),
                    (_, true) => format!("{} {}", step.icon(), step.label()),
                    (_, false) => step.label().into(),
                },
                wp.active,
                true,
            )),
        })
        .collect()
}

/// Writes the breadcrumb pill over the top border, picking the widest variant that fits.
fn draw_pill(w: &Wizard, buf: &mut Buffer, rect: Rect, wp: &WizardPalette) {
    let nerd = w.nerd_fonts;
    let room = rect.width.saturating_sub(5) as usize;
    let fits = |segs: &[Segment]| {
        let text: usize = segs.iter().map(|s| s.text.width()).sum();
        let caps = if nerd {
            2 + segs.len().saturating_sub(1)
        } else {
            0
        };
        text + caps <= room
    };
    let Some(segs) = [Variant::Wide, Variant::Narrow, Variant::Abbr]
        .into_iter()
        .map(|v| segments(w, v, wp))
        .find(|segs| !segs.is_empty() && fits(segs))
    else {
        return;
    };
    let mut x = rect.x + 2;
    let y = rect.y;
    let mut put = |x: &mut u16, text: &str, style: Style| {
        buf.set_string(*x, y, text, style);
        *x += text.width() as u16;
    };
    if nerd {
        put(&mut x, "\u{E0B6}", Style::default().fg(segs[0].bg));
    }
    for (i, seg) in segs.iter().enumerate() {
        if nerd && i > 0 {
            put(
                &mut x,
                "\u{E0C0}",
                Style::default().fg(segs[i - 1].bg).bg(seg.bg),
            );
        }
        let mut style = Style::default().fg(seg.fg).bg(seg.bg);
        if seg.bold {
            style = style.bold();
        }
        put(&mut x, &seg.text, style);
    }
    if nerd {
        put(
            &mut x,
            "\u{E0B4}",
            Style::default().fg(segs[segs.len() - 1].bg),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models_dev::ModelCatalog;
    use crate::tui::Appearance;
    use crate::wizard::providers::PROVIDERS;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};
    use serde_json::json;
    use unicode_width::UnicodeWidthStr;

    fn buffer(w: &Wizard, width: u16, height: u16) -> Buffer {
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        t.draw(|f| w.render(f)).unwrap();
        t.backend().buffer().clone()
    }
    /// Cells after a wide glyph are blank continuation cells; drop them so text reads as typed.
    fn row(buf: &Buffer, y: u16) -> String {
        let mut out = String::new();
        let mut skip = false;
        for x in 0..buf.area.width {
            if skip {
                skip = false;
                continue;
            }
            let symbol = buf[(x, y)].symbol();
            skip = symbol.width() == 2;
            out.push_str(symbol);
        }
        out
    }
    fn rows(buf: &Buffer) -> Vec<String> {
        (0..buf.area.height).map(|y| row(buf, y)).collect()
    }
    fn text(buf: &Buffer) -> String {
        rows(buf).join("\n")
    }
    fn screen(w: &Wizard, width: u16, height: u16) -> String {
        text(&buffer(w, width, height))
    }
    /// The card's top edge, wherever vertical centering put it.
    fn border_row(buf: &Buffer) -> String {
        rows(buf)
            .into_iter()
            .find(|r| r.contains('╭'))
            .expect("card drawn")
    }
    fn top_border(w: &Wizard, width: u16) -> String {
        border_row(&buffer(w, width, 30))
    }
    fn press(w: &mut Wizard, code: KeyCode) {
        w.key(KeyEvent::new(code, KeyModifiers::NONE));
    }
    fn all() -> Wizard {
        Wizard::new(Appearance::Dark, PROVIDERS.iter().collect())
    }
    fn at_providers(nerd: bool) -> Wizard {
        let mut w = all();
        press(&mut w, KeyCode::Enter);
        if !nerd {
            press(&mut w, KeyCode::Down);
        }
        press(&mut w, KeyCode::Enter);
        w
    }
    fn model(date: &str) -> serde_json::Value {
        json!({
            "tool_call": true,
            "release_date": date,
            "modalities": { "input": ["text"], "output": ["text"] }
        })
    }
    fn catalog(count: usize) -> ModelCatalog {
        let mut models = serde_json::Map::new();
        models.insert("claude-sonnet-4-6".into(), model("2026-02-17"));
        for i in 0..count {
            models.insert(
                format!("claude-other-{i:02}"),
                model(&format!("2025-01-{:02}", 28 - i)),
            );
        }
        ModelCatalog::from_value(json!({ "anthropic": { "models": models } }))
    }
    /// Anthropic checked and submitted; `catalog` loaded; on the API-key screen.
    fn at_api_key(nerd: bool, catalog: &ModelCatalog) -> Wizard {
        let mut w = at_providers(nerd);
        press(&mut w, KeyCode::Char(' '));
        press(&mut w, KeyCode::Enter);
        w.models_loaded(catalog.clone());
        w
    }
    fn at_model(nerd: bool, catalog: &ModelCatalog) -> Wizard {
        let mut w = at_api_key(nerd, catalog);
        w.paste("k");
        press(&mut w, KeyCode::Enter);
        w
    }
    /// Foreground colors of the cells spelling `needle` (first occurrence).
    fn cells_with(buf: &Buffer, needle: &str) -> Vec<Color> {
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let tail: String = (x..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
                if tail.starts_with(needle) {
                    return (x..x + needle.chars().count() as u16)
                        .map(|x| buf[(x, y)].fg)
                        .collect();
                }
            }
        }
        panic!("{needle:?} not on screen");
    }

    #[test]
    fn welcome_shows_logo_tagline_timing_and_actions() {
        let w = all();
        let buf = buffer(&w, 80, 24);
        let s = text(&buf);
        assert!(s.contains("███████╗██╗    ██╗███████╗███████╗██████╗"));
        assert!(s.contains("brew for the things you cannot brew."));
        assert!(s.contains("◷ Takes ~45 seconds."));
        assert!(s.contains("⏎ to continue │ Esc to cancel"));
        assert!(
            !border_row(&buf).contains("Setup Wizard"),
            "welcome has no pill"
        );
        let green = Color::Rgb(0x78, 0xe6, 0xa0);
        assert!(cells_with(&buf, "~45 seconds.").iter().all(|c| *c == green));
        assert!(cells_with(&buf, "Takes").iter().all(|c| *c != green));
    }

    #[test]
    fn nerd_fonts_screen_has_wizard_pill_and_rows() {
        let mut w = all();
        press(&mut w, KeyCode::Enter);
        let buf = buffer(&w, 80, 24);
        assert!(
            border_row(&buf)
                .trim_start()
                .starts_with("╭─ 🧙 Setup Wizard ─")
        );
        let s = text(&buf);
        assert!(s.contains("Do you see four icons below?"));
        assert!(s.contains("❯ Yes — enable icons throughout Sweep"));
        assert!(s.contains("  No — they look like boxes or question marks"));
        assert!(s.contains("↑↓ to move │ ⏎ to select"));
    }

    #[test]
    fn providers_screen_lists_sections_focus_and_toggle() {
        let mut w = at_providers(false);
        let buf = buffer(&w, 80, 30);
        let s = text(&buf);
        assert!(s.contains("Sweep needs at least one LLM provider configured."));
        assert!(s.contains("⠶⠶ SELECT API PROVIDER(S) ⠶⠶⠶"));
        assert!(s.contains("⠶⠶ USE YOUR CODING AGENT'S SUBSCRIPTION ⠶⠶"));
        assert!(s.contains("❯ [ ] Anthropic"));
        assert!(s.contains("  [ ] Claude Code"));
        assert!(s.contains("Space to toggle │ ⏎ to continue without an LLM"));
        let focus_bg = Color::Rgb(0x1a, 0x2a, 0x4d);
        let row = rows(&buf)
            .iter()
            .position(|r| r.contains("❯ [ ] Anthropic"))
            .unwrap() as u16;
        assert_eq!(buf[(5, row)].bg, focus_bg, "first content column");
        assert_eq!(buf[(74, row)].bg, focus_bg, "focus bg spans the full row");
        press(&mut w, KeyCode::Char(' '));
        press(&mut w, KeyCode::Down);
        let s = screen(&w, 80, 30);
        assert!(s.contains("  [✓] Anthropic"));
        assert!(s.contains("❯ [ ] OpenAI"));
        assert!(s.contains("Space to toggle │ ⏎ to continue "));
        assert!(!s.contains("without an LLM"));
    }

    #[test]
    fn providers_without_cli_hide_cli_header_and_show_glyphs_in_nerd_mode() {
        let available = PROVIDERS
            .iter()
            .filter(|p| p.key != "claude-code")
            .collect();
        let mut w = Wizard::new(Appearance::Dark, available);
        press(&mut w, KeyCode::Enter);
        press(&mut w, KeyCode::Enter);
        let s = screen(&w, 80, 30);
        assert!(!s.contains("CODING AGENT"));
        assert!(!s.contains("Claude Code"));
        assert!(s.contains("❯ [ ] \u{E754} Anthropic"));
    }

    #[test]
    fn breadcrumb_wide_nerd_and_ascii_modes() {
        let cat = catalog(2);
        let ascii = top_border(&at_api_key(false, &cat), 100);
        assert!(
            ascii.contains(" 🧙 Setup Wizard  Anthropic  API Key "),
            "{ascii}"
        );
        for cap in ['\u{E0B6}', '\u{E0C0}', '\u{E0B4}'] {
            assert!(!ascii.contains(cap));
        }
        let nerd = top_border(&at_api_key(true, &cat), 100);
        for cap in ['\u{E0B6}', '\u{E0C0}', '\u{E0B4}', '\u{F084}', '\u{E754}'] {
            assert!(nerd.contains(cap), "{nerd}");
        }
        let model = top_border(&at_model(false, &cat), 100);
        assert!(model.contains("Anthropic  ✓ API Key  Model "), "{model}");
        let nerd_model = top_border(&at_model(true, &cat), 100);
        assert!(nerd_model.contains('\u{F00C}'));
        let mut w = at_providers(false);
        let providers = top_border(&w, 100);
        assert!(
            providers.contains("Setup Wizard  Providers "),
            "{providers}"
        );
        press(&mut w, KeyCode::Char(' '));
        press(&mut w, KeyCode::Enter);
        let loading = buffer(&w, 100, 24);
        assert!(text(&loading).contains("Loading models…"));
        let bottom = rows(&loading)
            .into_iter()
            .find(|r| r.contains('╰'))
            .unwrap();
        assert!(bottom.contains("╰─ Loading models list… ─"), "{bottom}");
    }

    #[test]
    fn breadcrumb_falls_back_at_narrow_widths() {
        let cat = catalog(2);
        let w = at_model(false, &cat);
        let narrow = top_border(&w, 40);
        assert!(!narrow.contains("Setup Wizard"), "{narrow}");
        assert!(narrow.contains(" Anthropic  ✓  Model "), "{narrow}");
        let abbr = top_border(&w, 24);
        assert!(!abbr.contains("Anthropic"), "{abbr}");
        assert!(abbr.contains(" ✓  M "), "{abbr}");
        let none = top_border(&w, 12);
        assert!(!none.contains('M'), "{none}");
        assert!(none.trim_start().starts_with("╭──"));
    }

    #[test]
    fn api_key_screen_masks_input_and_shows_placeholder() {
        let cat = catalog(2);
        let mut w = at_api_key(false, &cat);
        let s = screen(&w, 80, 24);
        assert!(s.contains("Anthropic API key"));
        assert!(s.contains("Get one: https://console.anthropic.com/settings/keys"));
        assert!(s.contains("sk-ant-api03-"));
        assert!(s.contains("⏎ to continue"));
        w.paste("secret");
        let s = screen(&w, 80, 24);
        assert!(s.contains("••••••"));
        assert!(!s.contains("secret"));
        assert!(!s.contains("sk-ant-api03-"));
    }

    #[test]
    fn model_screen_marks_recommended_first_and_scrolls_to_cursor() {
        let cat = catalog(12);
        let mut w = at_model(false, &cat);
        let s = screen(&w, 80, 30);
        assert!(s.contains("Anthropic model"));
        assert!(s.contains("❯ claude-sonnet-4-6  ✦ Recommended"));
        assert!(s.contains("  claude-other-00"));
        for _ in 0..12 {
            press(&mut w, KeyCode::Down);
        }
        let s = screen(&w, 80, 12);
        assert!(s.contains("❯ claude-other-11"), "{s}");
        assert!(s.contains("↑↓ to move │ ⏎ to continue"));
    }

    #[test]
    fn model_screen_without_models_offers_skip() {
        let mut w = all();
        press(&mut w, KeyCode::Enter);
        press(&mut w, KeyCode::Enter);
        for _ in 0..3 {
            press(&mut w, KeyCode::Down);
        }
        press(&mut w, KeyCode::Char(' '));
        press(&mut w, KeyCode::Enter);
        w.models_loaded(catalog(0));
        w.paste("gsk");
        press(&mut w, KeyCode::Enter);
        let s = screen(&w, 80, 24);
        assert!(s.contains("No models found for Groq."));
        assert!(s.contains("Esc to skip this provider"));
    }

    #[test]
    fn disclaimer_and_default_picker_render() {
        let mut w = all();
        press(&mut w, KeyCode::Enter);
        press(&mut w, KeyCode::Down);
        press(&mut w, KeyCode::Enter);
        press(&mut w, KeyCode::Char(' '));
        for _ in 0..6 {
            press(&mut w, KeyCode::Down);
        }
        press(&mut w, KeyCode::Char(' '));
        press(&mut w, KeyCode::Enter);
        w.models_loaded(catalog(1));
        w.paste("k");
        press(&mut w, KeyCode::Enter);
        press(&mut w, KeyCode::Enter);
        let buf = buffer(&w, 80, 24);
        let s = text(&buf);
        assert!(s.contains("Sweep will route your queries through the claude CLI"));
        assert!(s.contains("⏎ to accept │ Esc to skip this provider"));
        assert!(border_row(&buf).contains("Claude Code  API Key"));
        press(&mut w, KeyCode::Enter);
        let buf = buffer(&w, 80, 24);
        let s = text(&buf);
        assert!(s.contains("Which provider should be the default?"));
        assert!(s.contains("❯ Anthropic"));
        assert!(s.contains("  Claude Code"));
        assert!(s.contains("↑↓ to move │ ⏎ to select"));
        assert!(border_row(&buf).contains("✓ Providers  Set Default"));
    }

    #[test]
    fn narrow_section_header_keeps_the_label_and_drops_the_dots() {
        let w = at_providers(false);
        let s = screen(&w, 32, 30);
        assert!(s.contains("SELECT API PROVIDER(S)"), "{s}");
        assert!(!s.contains("⠶ SELECT"), "{s}");
        assert!(s.contains("USE YOUR CODING AGENT'S"), "{s}");
        assert!(!s.contains('⠶'), "{s}");
        let wide = screen(&w, 80, 30);
        assert!(wide.contains("⠶⠶ SELECT API PROVIDER(S) ⠶⠶"), "{wide}");
    }

    #[test]
    fn primary_key_is_gold_even_when_it_is_esc() {
        let gold = Color::Rgb(0xf5, 0xba, 0x4a);
        let welcome = buffer(&all(), 80, 24);
        assert_eq!(cells_with(&welcome, "⏎")[0], gold);
        assert_ne!(cells_with(&welcome, "Esc")[0], gold);
        let mut loading = at_providers(false);
        press(&mut loading, KeyCode::Char(' '));
        press(&mut loading, KeyCode::Enter);
        assert_eq!(cells_with(&buffer(&loading, 80, 24), "Esc")[0], gold);
        let mut no_models = at_providers(false);
        for _ in 0..3 {
            press(&mut no_models, KeyCode::Down);
        }
        press(&mut no_models, KeyCode::Char(' '));
        press(&mut no_models, KeyCode::Enter);
        no_models.models_loaded(catalog(0));
        no_models.paste("gsk");
        press(&mut no_models, KeyCode::Enter);
        assert_eq!(cells_with(&buffer(&no_models, 80, 24), "Esc")[0], gold);
        let disclaimer = {
            let mut w = at_providers(false);
            for _ in 0..6 {
                press(&mut w, KeyCode::Down);
            }
            press(&mut w, KeyCode::Char(' '));
            press(&mut w, KeyCode::Enter);
            buffer(&w, 80, 24)
        };
        assert_eq!(cells_with(&disclaimer, "⏎")[0], gold);
        assert_ne!(cells_with(&disclaimer, "Esc")[0], gold);
    }

    #[test]
    fn no_color_level_strips_rgb_but_keeps_text() {
        let mut w = at_providers(false);
        w.color_level = 0;
        let buf = buffer(&w, 80, 30);
        assert!(
            buf.content
                .iter()
                .all(|c| c.fg == Color::Reset && c.bg == Color::Reset)
        );
        assert!(text(&buf).contains("❯ [ ] Anthropic"));
    }

    #[test]
    fn tiny_and_odd_sizes_never_panic() {
        let cat = catalog(3);
        let mut screens = vec![
            all(),
            at_providers(true),
            at_api_key(true, &cat),
            at_model(false, &cat),
        ];
        let mut loading = at_providers(false);
        press(&mut loading, KeyCode::Char(' '));
        press(&mut loading, KeyCode::Enter);
        screens.push(loading);
        let mut nerd = all();
        press(&mut nerd, KeyCode::Enter);
        screens.push(nerd);
        for w in &screens {
            for (width, height) in [
                (1, 1),
                (8, 3),
                (12, 5),
                (32, 10),
                (74, 20),
                (80, 24),
                (120, 40),
            ] {
                let _ = buffer(w, width, height);
            }
        }
    }

    #[test]
    fn narrow_providers_screen_keeps_focused_row_visible_after_wrapping() {
        let mut w = at_providers(false);
        for _ in 0..6 {
            press(&mut w, KeyCode::Down);
        }
        let s = screen(&w, 32, 12);
        assert!(s.contains("❯ [ ] Claude Code"), "{s}");
        let api = at_api_key(false, &catalog(1));
        let s = screen(&api, 32, 24);
        assert!(s.contains("settings/keys"), "{s}");
        assert!(s.contains("  sk-ant-api03-"), "{s}");
    }

    #[test]
    fn short_terminals_keep_input_row_and_action_bar_visible() {
        let cat = catalog(3);
        let api = at_api_key(false, &cat);
        for height in [10u16, 8, 6, 4] {
            let rows = rows(&buffer(&api, 60, height));
            let input = rows.iter().position(|r| r.contains("sk-ant-api03-"));
            let actions = rows.iter().position(|r| r.contains("⏎ to continue"));
            assert!(
                input.is_some(),
                "input hidden at {height}:\n{}",
                rows.join("\n")
            );
            assert!(
                actions.is_some_and(|a| a + 2 == rows.len() || a + 3 == rows.len()),
                "action bar not at the bottom at {height}:\n{}",
                rows.join("\n")
            );
            assert!(input < actions);
        }
        let mut providers = at_providers(false);
        for _ in 0..6 {
            press(&mut providers, KeyCode::Down);
        }
        let s = screen(&providers, 60, 8);
        assert!(s.contains("❯ [ ] Claude Code"), "{s}");
        assert!(s.contains("Space to toggle"), "{s}");
        let model = at_model(false, &cat);
        let s = screen(&model, 60, 6);
        assert!(s.contains("❯ claude-sonnet-4-6"), "{s}");
        assert!(s.contains("↑↓ to move"), "{s}");
    }

    #[test]
    fn disclaimer_wraps_fully_into_a_short_card() {
        let mut w = at_providers(false);
        for _ in 0..6 {
            press(&mut w, KeyCode::Down);
        }
        press(&mut w, KeyCode::Char(' '));
        press(&mut w, KeyCode::Enter);
        let s = screen(&w, 60, 10);
        for needle in ["route your queries", "own subscription", "and credentials."] {
            assert!(s.contains(needle), "{needle}: {s}");
        }
        assert!(s.contains("⏎ to accept │ Esc to skip this provider"), "{s}");
        let roomy = rows(&buffer(&w, 60, 24));
        let top = roomy.iter().position(|r| r.contains('╭')).unwrap();
        assert!(
            roomy[top + 1].trim().trim_matches('│').trim().is_empty(),
            "padding returns when there is room:\n{}",
            roomy.join("\n")
        );
    }

    #[test]
    fn disclaimer_scrolls_with_arrows_only_when_it_overflows() {
        let mut w = at_providers(false);
        for _ in 0..6 {
            press(&mut w, KeyCode::Down);
        }
        press(&mut w, KeyCode::Char(' '));
        press(&mut w, KeyCode::Enter);
        let top = screen(&w, 40, 6);
        assert!(top.contains("Sweep will route"), "{top}");
        assert!(!top.contains("credentials."), "{top}");
        assert!(top.contains("↑↓ to scroll"), "{top}");
        for _ in 0..20 {
            press(&mut w, KeyCode::Down);
        }
        let bottom = screen(&w, 40, 6);
        assert!(bottom.contains("credentials."), "{bottom}");
        assert!(!bottom.contains("Sweep will route"), "{bottom}");
        press(&mut w, KeyCode::Up);
        assert_ne!(
            screen(&w, 40, 6),
            bottom,
            "clamped scroll moves back at once"
        );
        let roomy = screen(&w, 60, 24);
        assert!(!roomy.contains("to scroll"), "{roomy}");
        assert!(
            roomy.contains("⏎ to accept │ Esc to skip this provider"),
            "{roomy}"
        );
        let welcome = screen(&all(), 80, 8);
        assert!(
            welcome.contains("↑↓ to scroll │ ⏎ to continue"),
            "{welcome}"
        );
        assert!(!screen(&all(), 80, 24).contains("to scroll"));
    }

    #[test]
    fn api_key_input_is_one_blank_row_above_action_bar() {
        let cat = catalog(2);
        let w = at_api_key(false, &cat);
        let rows = rows(&buffer(&w, 80, 24));
        let input = rows
            .iter()
            .position(|r| r.contains("sk-ant-api03-"))
            .unwrap();
        let actions = rows
            .iter()
            .position(|r| r.contains("⏎ to continue"))
            .unwrap();
        assert_eq!(actions - input, 2, "{}", rows.join("\n"));
    }
}
