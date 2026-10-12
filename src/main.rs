use std::{
    collections::HashMap,
    io::{IsTerminal, Write},
    path::PathBuf,
};
use sweep::{
    analyze, config, parse,
    store::{Invocation, Store, now},
    tui, wizard,
};

#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("sweep: {error}");
            1
        }
    };
    std::process::exit(code);
}

async fn run() -> anyhow::Result<i32> {
    let positional = std::env::args_os()
        .nth(1)
        .map(|arg| arg.to_string_lossy().trim().to_owned())
        .unwrap_or_default();
    if positional == "--version" {
        return print_stdout(concat!("sweep ", env!("CARGO_PKG_VERSION"), "\n")).map(|()| 0);
    }
    let env: HashMap<String, String> = std::env::vars_os()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            )
        })
        .collect();
    let home = std::env::var_os("SWEEP_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".sweep")
        });
    let mut config = config::load(&home, &env).map_err(anyhow::Error::msg)?;
    if positional == "list" {
        let packages = Store::open(&home)?.list_installed_packages()?;
        let text = if packages.is_empty() {
            "No packages installed.\n".to_owned()
        } else {
            let level = tui::color_level(std::io::stdout().is_terminal());
            let appearance = if level > 0 {
                tui::resolve_appearance(&home)
            } else {
                tui::Appearance::Dark
            };
            sweep::app::format_list(&packages, level, appearance)
        };
        return print_stdout(&text).map(|()| 0);
    }
    let started = now();
    let start = if positional.is_empty() {
        if !std::io::stdout().is_terminal() {
            eprintln!("sweep: usage: sweep 'curl https://example.com/install.sh | sh'");
            return Ok(2);
        }
        tui::SessionStart::Interactive
    } else {
        match parse::parse_install_command(&positional) {
            Ok(parsed) => tui::SessionStart::Direct {
                raw: positional.clone(),
                parsed,
            },
            Err(error) => {
                eprintln!("sweep: {}", error.message);
                Store::open(&home)?.insert_invocation(&Invocation {
                    id: uuid::Uuid::new_v4().to_string(),
                    ts_started: started,
                    ts_finished: Some(now()),
                    raw_input: positional,
                    outcome: "parse_failed".into(),
                    error_message: Some(error.message),
                    ..Default::default()
                })?;
                return Ok(2);
            }
        }
    };
    let mut appearance = None;
    if wizard::needed(&home, &env) {
        if !std::io::stderr().is_terminal() {
            eprintln!("sweep: no config found; run sweep in a terminal to set one up.");
            return Ok(2);
        }
        match wizard::run(&home, &env).await? {
            wizard::Outcome::Completed(probed) => {
                eprintln!("🧠 Configuration saved");
                appearance = Some(probed);
            }
            wizard::Outcome::Cancelled => return Ok(0),
        }
        config = config::load(&home, &env).map_err(anyhow::Error::msg)?;
    }
    // Opened after the setup gate so a cancelled or refused setup leaves no database.
    let mut store = Store::open(&home)?;
    let provider = analyze::resolve_analysis_provider(&home, &env);
    let nerd_fonts = config::nerd_fonts(&config);
    let appearance = appearance.unwrap_or_else(|| tui::resolve_appearance(&home));
    let decision = match tui::run_session(start, provider, appearance, nerd_fonts).await {
        Ok(decision) => decision,
        Err(failure) => {
            if let Some(raw) = failure.raw {
                store.insert_invocation(&Invocation {
                    id: uuid::Uuid::new_v4().to_string(),
                    ts_started: started,
                    ts_finished: Some(now()),
                    raw_input: raw,
                    url: failure.parsed.as_ref().map(|p| p.url.clone()),
                    install_command_json: failure
                        .parsed
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?,
                    final_url: failure.fetched.as_ref().map(|f| f.final_url.clone()),
                    sha256: failure.fetched.as_ref().map(|f| f.sha256.clone()),
                    outcome: "errored".into(),
                    error_message: Some(failure.error.to_string()),
                    ..Default::default()
                })?;
            }
            return Err(failure.error.into());
        }
    };
    sweep::app::finish_install(&mut store, &started, decision)
}
/// A closed pipe (`sweep list | head`) is not an error for the caller.
fn print_stdout(text: &str) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        result => Ok(result?),
    }
}
