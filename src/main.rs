use std::{collections::HashMap, io::IsTerminal, path::PathBuf};
use sweep::{
    analyze, config, parse,
    store::{Invocation, Store, now},
    tui,
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
    let env: HashMap<String, String> = std::env::vars().collect();
    let home = env
        .get("SWEEP_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".sweep")
        });
    std::fs::create_dir_all(&home)?;
    config::load(&home, &env).map_err(anyhow::Error::msg)?;
    let positional = std::env::args()
        .nth(1)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let mut store = Store::open(&home)?;
    if positional == "list" {
        let packages = store.list_installed_packages()?;
        if packages.is_empty() {
            println!("No packages installed.");
        } else {
            print!(
                "{}",
                sweep::app::format_list(
                    &packages,
                    tui::color_level(std::io::stdout().is_terminal()),
                    tui::resolve_appearance(&home)
                )
            );
        }
        return Ok(0);
    }
    let started = now();
    let start = if positional.is_empty() && std::io::stdout().is_terminal() {
        tui::SessionStart::Interactive
    } else {
        match parse::parse_install_command(&positional) {
            Ok(parsed) => tui::SessionStart::Direct {
                raw: positional.clone(),
                parsed,
            },
            Err(error) => {
                eprintln!("sweep: {}", error.message);
                store.insert_invocation(&Invocation {
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
    let provider = analyze::resolve_analysis_provider(&home, &env);
    let decision =
        match tui::run_session(start, provider, tui::resolve_appearance(&home), false).await {
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
