//! Render real UI cells as JSON for visual review without entering a terminal.
use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier},
};
use serde_json::json;
use sweep::tui::{Appearance, InsightKind, InsightView, Phase, Ui};
fn color(value: Color) -> Option<String> {
    match value {
        Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        Color::Black => Some("#000000".into()),
        _ => None,
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let state = args.get(1).map(String::as_str).unwrap_or("caution");
    let appearance = if args.get(2).is_some_and(|s| s == "light") {
        Appearance::Light
    } else {
        Appearance::Dark
    };
    let width = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(90);
    let height = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(28);
    let phase = match state {
        "paste" => Phase::Paste { error: None },
        "loading" => Phase::Loading {
            source: "get.example.com/install.sh".into(),
        },
        _ => {
            let kind = match state {
                "clear" => InsightKind::Clear,
                "danger" => InsightKind::Danger,
                "manipulation" => InsightKind::Manipulation,
                "no-llm" => InsightKind::NoLlm,
                "failed" => InsightKind::AnalysisFailed,
                _ => InsightKind::Caution,
            };
            let message = match kind {
                InsightKind::NoLlm => "No LLM provider configured — no analysis to show.",
                InsightKind::AnalysisFailed => "Couldn't analyze: The provider timed out.",
                _ => "Downloads the latest release and installs the command-line tool.",
            };
            let analyzed = !matches!(kind, InsightKind::NoLlm | InsightKind::AnalysisFailed);
            Phase::Resolved(InsightView {
                kind,
                source: "get.example.com/install.sh".into(),
                message: message.into(),
                flags: if analyzed {
                    vec!["Executes a downloaded binary without verifying its checksum.".into()]
                } else {
                    vec![]
                },
                behaviors: if analyzed {
                    vec![
                        (
                            "Creates ~/.local/bin and updates the shell profile.".into(),
                            false,
                        ),
                        ("Copies the executable into /usr/local/bin.".into(), true),
                    ]
                } else {
                    vec![]
                },
            })
        }
    };
    let ui = Ui::new(phase, appearance, false);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui.render(frame)).unwrap();
    let cells:Vec<_>=terminal.backend().buffer().content.iter().map(|c|json!({"text":c.symbol(),"fg":color(c.fg),"bg":color(c.bg),"bold":c.modifier.contains(Modifier::BOLD)})).collect();
    println!(
        "{}",
        json!({"width":width,"height":height,"state":state,"appearance":if appearance==Appearance::Light{"light"}else{"dark"},"cells":cells})
    );
}
