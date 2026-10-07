use crate::{
    exec,
    parse::slug_from_url,
    store::{Invocation, PackageRow, Store, now},
    tui::{Appearance, InstallDecision},
};
use anyhow::Result;
use unicode_width::UnicodeWidthStr;

/// Persist a completed session; terminal ownership has ended before this boundary.
pub fn finish_install(store: &mut Store, started: &str, decision: InstallDecision) -> Result<i32> {
    let mut invocation = Invocation {
        id: uuid::Uuid::new_v4().to_string(),
        ts_started: started.into(),
        ..Default::default()
    };
    match decision {
        InstallDecision::Cancel {
            raw,
            parsed,
            fetched,
        } => {
            let Some(raw) = raw else { return Ok(0) };
            invocation.raw_input = raw;
            if let Some(parsed) = parsed {
                invocation.url = Some(parsed.url.clone());
                invocation.install_command_json = Some(serde_json::to_string(&parsed)?);
            }
            if let Some(fetched) = fetched {
                invocation.final_url = Some(fetched.final_url);
                invocation.sha256 = Some(fetched.sha256);
            }
            invocation.outcome = "cancelled".into();
            invocation.ts_finished = Some(now());
            store.insert_invocation(&invocation)?;
            Ok(130)
        }
        InstallDecision::FetchFailed { raw, parsed, error } => {
            eprintln!("sweep: {}", error.message);
            invocation.raw_input = raw;
            invocation.url = Some(parsed.url.clone());
            invocation.install_command_json = Some(serde_json::to_string(&parsed)?);
            invocation.outcome = "fetch_failed".into();
            invocation.error_message = Some(error.message);
            invocation.ts_finished = Some(now());
            store.insert_invocation(&invocation)?;
            Ok(1)
        }
        InstallDecision::Run {
            raw,
            parsed,
            fetched,
        } => {
            invocation.raw_input = raw;
            invocation.url = Some(parsed.url.clone());
            invocation.final_url = Some(fetched.final_url);
            invocation.sha256 = Some(fetched.sha256.clone());
            invocation.install_command_json = Some(serde_json::to_string(&parsed)?);
            // Keep signal handling scoped through the final database transaction.
            let signals = crate::exec_signals::ExecutionSignals::open()?;
            let execution = (|| -> Result<i32> {
                store.save_script(&fetched.sha256, &fetched.bytes)?;
                invocation.outcome = "running".into();
                store.begin_exec(&mut invocation, &parsed.url, &slug_from_url(&parsed.url))?;
                Ok(exec::run_script_with_signals(
                    &parsed,
                    &fetched.bytes,
                    &signals,
                )?)
            })();
            let finished = now();
            invocation.ts_finished = Some(finished.clone());
            match execution {
                Ok(code) => {
                    invocation.exit_code = Some(code);
                    invocation.outcome = if code == 0 { "ran" } else { "errored" }.into();
                    store.record_exec(
                        &invocation,
                        invocation.package_id.expect("package precedes execution"),
                        &fetched.sha256,
                        code,
                        &finished,
                    )?;
                    Ok(code)
                }
                Err(error) => {
                    invocation.outcome = "errored".into();
                    invocation.error_message = Some(error.to_string());
                    if let Some(id) = invocation.package_id {
                        store.record_exec(&invocation, id, &fetched.sha256, 1, &finished)?;
                    } else {
                        store.insert_invocation(&invocation)?;
                    }
                    Err(error)
                }
            }
        }
    }
}

pub fn format_list(packages: &[PackageRow], color_level: u8, appearance: Appearance) -> String {
    let color = color_level > 0;
    let headers = ["PACKAGE", "SOURCE", "STATUS", "LAST RAN"];
    let rows: Vec<[String; 4]> = packages
        .iter()
        .map(|p| {
            [
                p.slug.clone(),
                url::Url::parse(&p.source_url)
                    .ok()
                    .and_then(|u| {
                        u.host_str()
                            .map(|h| h.trim_start_matches("www.").to_string())
                    })
                    .unwrap_or_else(|| p.source_url.clone()),
                p.status.clone(),
                p.last_ran_at
                    .as_deref()
                    .map(|s| s.chars().take(10).collect())
                    .unwrap_or_else(|| "never".into()),
            ]
        })
        .collect();
    let mut widths = headers.map(UnicodeWidthStr::width);
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }
    let colors = match appearance {
        Appearance::Dark => [
            (210, 210, 225),
            (120, 180, 255),
            (120, 230, 160),
            (170, 170, 195),
        ],
        Appearance::Light => [(0, 0, 0), (25, 90, 190), (15, 125, 55), (45, 45, 70)],
    };
    let mut out = String::new();
    for (index, row) in std::iter::once(headers.map(str::to_string))
        .chain(rows)
        .enumerate()
    {
        for (i, cell) in row.iter().enumerate() {
            if color {
                out.push_str(&format!(
                    "\x1b[{};{}m",
                    if index == 0 { 1 } else { 22 },
                    ansi_foreground(if index == 0 { colors[3] } else { colors[i] }, color_level)
                ));
            }
            out.push_str(cell);
            if color {
                out.push_str("\x1b[0m");
            }
            if i < 3 {
                out.push_str(&" ".repeat(widths[i] - UnicodeWidthStr::width(cell.as_str()) + 2));
            }
        }
        out.push('\n');
    }
    out
}

fn ansi_foreground(rgb: (u8, u8, u8), level: u8) -> String {
    use ratatui::style::Color;
    match crate::tui::quantize(Color::from(rgb), level) {
        Color::Rgb(r, g, b) => format!("38;2;{r};{g};{b}"),
        Color::Indexed(n) => format!("38;5;{n}"),
        color => crate::tui::BASIC_COLORS
            .iter()
            .position(|c| *c == color)
            .map(|n| if n < 8 { 30 + n } else { 90 + n - 8 })
            .unwrap_or(39)
            .to_string(),
    }
}
