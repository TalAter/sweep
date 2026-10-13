use std::io::{self, IsTerminal, Write};
use sweep::tui::Appearance;
use unicode_width::UnicodeWidthStr;

const LOGO: &[&str] = &[
    "███████╗██╗    ██╗███████╗███████╗██████╗ ",
    "██╔════╝██║    ██║██╔════╝██╔════╝██╔══██╗",
    "███████╗██║ █╗ ██║█████╗  █████╗  ██████╔╝",
    "╚════██║██║███╗██║██╔══╝  ██╔══╝  ██╔═══╝ ",
    "███████║╚███╔███╔╝███████╗███████╗██║     ",
    "╚══════╝ ╚══╝╚══╝ ╚══════╝╚══════╝╚═╝     ",
];
const FRAMES: usize = 16;
const DELAY: std::time::Duration = std::time::Duration::from_millis(60);

struct Line {
    text: String,
    style: &'static str,
}

fn lines() -> Vec<Line> {
    let mut lines = Vec::new();
    let mut push = |text: &str, style| {
        lines.push(Line {
            text: text.into(),
            style,
        })
    };
    push("", "");
    push("  brew for the things you cannot brew.", "2");
    push("", "");
    push("  Usage:", "1");
    push("    sweep [<INSTALL_COMMAND> | COMMAND]", "36");
    push("", "");
    push("  Examples:", "1");
    push(
        "    sweep 'curl -fsSL https://example.com/install.sh | sh'",
        "36",
    );
    push(
        "      Review the script, then choose whether to run it.",
        "2",
    );
    push("    sweep", "36");
    push("      Enter an install command interactively.", "2");
    push("    sweep list", "36");
    push(
        "      See successfully installed packages and their sources.",
        "2",
    );
    push("", "");
    push("  Commands:", "1");
    // Command labels and descriptions come from the dispatch registry.
    for (argument, description) in crate::cli::entries() {
        let usage = format!("sweep {argument}");
        push(
            &format!("    {:<25} {description}", usage.trim_end()),
            "command",
        );
    }
    push("", "");
    push(
        "  Quote the entire install command so Sweep receives it as one argument.",
        "2",
    );
    push("", "");
    lines
}

fn enabled(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| {
        !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no" | "off"
        )
    })
}

fn fits(size: Option<(u16, u16)>, lines: &[Line]) -> bool {
    let Some((width, height)) = size else {
        return false;
    };
    width > 0
        && height > 0
        && lines.len() + LOGO.len() + 2 < usize::from(height)
        && lines
            .iter()
            .all(|line| line.text.width() < usize::from(width))
        && LOGO
            .iter()
            .all(|line| line.width() + 2 < usize::from(width))
}

fn logo(shine: Option<f32>, level: u8, appearance: Appearance) -> Vec<Vec<String>> {
    LOGO.iter()
        .map(|row| {
            format!("  {row}")
                .chars()
                .enumerate()
                .map(|(column, ch)| {
                    let t = (column as f32 / 43.0).clamp(0.0, 1.0);
                    let (start, end) = match appearance {
                        Appearance::Dark => ([210.0, 245.0, 255.0], [85.0, 165.0, 235.0]),
                        Appearance::Light => ([35.0, 130.0, 175.0], [30.0, 65.0, 145.0]),
                    };
                    let base: [f32; 3] =
                        std::array::from_fn(|i| start[i] + (end[i] - start[i]) * t);
                    let light = shine.map_or(0.0, |pos| {
                        (1.0 - (column as f32 - pos).abs() / 4.0).max(0.0).powi(2)
                    });
                    let highlight = match appearance {
                        Appearance::Dark => [255.0, 255.0, 255.0],
                        Appearance::Light => [110.0, 170.0, 205.0],
                    };
                    let [r, g, b]: [u8; 3] =
                        std::array::from_fn(|i| (base[i] + (highlight[i] - base[i]) * light) as u8);
                    match level {
                        3 => format!("\x1b[38;2;{r};{g};{b}m{ch}"),
                        2 => {
                            let colors = match appearance {
                                Appearance::Dark => [195, 159, 117, 75],
                                Appearance::Light => [31, 32, 25, 24],
                            };
                            format!("\x1b[38;5;{}m{ch}", colors[(t * 3.0) as usize])
                        }
                        _ => format!("\x1b[{}m{ch}", accent(appearance)),
                    }
                })
                .collect()
        })
        .collect()
}

fn write_logo(out: &mut impl Write, cells: &[Vec<String>]) -> io::Result<()> {
    writeln!(out)?;
    for row in cells {
        writeln!(out, "{}\x1b[0m", row.join(""))?;
    }
    writeln!(out)
}

fn accent(appearance: Appearance) -> &'static str {
    match appearance {
        Appearance::Dark => "96",
        Appearance::Light => "34",
    }
}

fn write_body(
    out: &mut impl Write,
    lines: &[Line],
    color: bool,
    appearance: Appearance,
) -> io::Result<()> {
    for line in lines {
        if !color || line.style.is_empty() {
            writeln!(out, "{}", line.text)?;
        } else if line.style == "command" {
            // Registry labels are ASCII; descriptions remain in the default foreground.
            let (label, description) = line.text.split_at(29);
            writeln!(
                out,
                "\x1b[{}m{label}\x1b[0m{description}",
                accent(appearance)
            )?;
        } else {
            let style = if line.style == "36" {
                accent(appearance)
            } else if line.style == "2" && appearance == Appearance::Light {
                "0"
            } else {
                line.style
            };
            writeln!(out, "\x1b[{}m{}\x1b[0m", style, line.text)?;
        }
    }
    out.flush()
}

async fn render(out: &mut impl Write, tty: bool, home: &std::path::Path) -> io::Result<()> {
    let level = sweep::tui::color_level(tty);
    let body = lines();
    if level == 0 {
        writeln!(out, "Sweep — {}", env!("CARGO_PKG_DESCRIPTION"))?;
        return write_body(out, &body, false, Appearance::Dark);
    }
    let appearance = if tty {
        sweep::tui::detect_appearance_guarded(home)?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Interrupted,
                    "appearance detection interrupted",
                )
            })?
            .appearance
    } else if std::env::var("SWEEP_THEME").as_deref() == Ok("light") {
        Appearance::Light
    } else {
        Appearance::Dark
    };
    let size = crossterm::terminal::size().ok();
    let animate = tty
        && level == 3
        && !enabled("CI")
        && !enabled("SWEEP_NO_ANIMATION")
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").as_deref() != Ok("dumb")
        && fits(size, &body);
    let mut previous = logo(None, level, appearance);
    write_logo(out, &previous)?;
    write_body(out, &body, true, appearance)?;
    if !animate {
        return Ok(());
    }
    // Each frame returns to the end of help before yielding. No terminal modes
    // or cursor visibility are changed, so interruption needs no restoration.
    let distance = body.len() + LOGO.len() + 1;
    for frame in 0..FRAMES {
        tokio::time::sleep(DELAY).await;
        if crossterm::terminal::size().ok() != size {
            break;
        }
        let t = frame as f32 / (FRAMES - 1) as f32;
        let position = (t * t * (3.0 - 2.0 * t)) * 51.0 - 4.0;
        let current = logo(Some(position), level, appearance);
        let mut escape = String::new();
        for (row, (before, after)) in previous.iter().zip(&current).enumerate() {
            if let Some(start) = before.iter().zip(after).position(|(a, b)| a != b) {
                let end = before.iter().zip(after).rposition(|(a, b)| a != b).unwrap();
                let up = distance - row;
                escape.push_str(&format!("\x1b[{up}A\r"));
                if start > 0 {
                    escape.push_str(&format!("\x1b[{start}C"));
                }
                escape.push_str(&after[start..=end].join(""));
                escape.push_str(&format!("\x1b[0m\r\x1b[{up}B"));
            }
        }
        out.write_all(escape.as_bytes())?;
        out.flush()?;
        previous = current;
    }
    Ok(())
}

pub async fn print(home: &std::path::Path) -> anyhow::Result<i32> {
    let stdout = io::stdout();
    let result = render(&mut stdout.lock(), stdout.is_terminal(), home).await;
    match result {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(0),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(130),
        result => result.map(|()| 0).map_err(Into::into),
    }
}
