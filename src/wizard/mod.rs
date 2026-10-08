//! First-run setup wizard: writes `config.jsonc` and its schema before a session starts.
mod config_file;
mod providers;
mod render;
mod screens;

use std::{collections::HashMap, io::IsTerminal, path::Path};

use crate::{
    models_dev::{MODELS_DEV_URL, ModelCatalog, ModelListError},
    tui::{self, Appearance, Dialog, Progress},
};
use screens::{Screen, Step, Wizard};

/// The wizard runs only when `config.jsonc` is absent and no `SWEEP_CONFIG` overlay
/// is set. Anything else at that path (a file, a dangling symlink, an unreadable
/// entry) is left for the config loader to report.
pub fn needed(home: &Path, env: &HashMap<String, String>) -> bool {
    let absent = matches!(
        std::fs::symlink_metadata(home.join("config.jsonc")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    absent
        && env
            .get("SWEEP_CONFIG")
            .is_none_or(|raw| raw.trim().is_empty())
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Carries the terminal appearance already probed, so the session need not ask again.
    Completed(Appearance),
    Cancelled,
}

/// Debug builds let tests point the models.dev fetch at a local or unroutable URL.
#[cfg(debug_assertions)]
fn catalog_url(env: &HashMap<String, String>) -> String {
    env.get("SWEEP_MODELS_URL")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map_or_else(|| MODELS_DEV_URL.to_string(), str::to_string)
}
#[cfg(not(debug_assertions))]
fn catalog_url(_env: &HashMap<String, String>) -> String {
    MODELS_DEV_URL.to_string()
}

type CatalogResult = Result<ModelCatalog, ModelListError>;

/// The in-flight models.dev load: its result channel and a handle to abort it.
struct Loading {
    result: tokio::sync::oneshot::Receiver<CatalogResult>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Loading {
    fn drop(&mut self) {
        self.task.abort();
    }
}

enum Exit {
    Cancelled,
    Finish(config_file::WizardConfig),
}

struct WizardDialog {
    wizard: Wizard,
    home: std::path::PathBuf,
    url: String,
    loading: Option<Loading>,
}
impl Dialog for WizardDialog {
    type Exit = Exit;
    fn render(&self, frame: &mut ratatui::Frame<'_>) {
        self.wizard.render(frame);
    }
    fn animating(&self) -> bool {
        self.wizard.screen == Screen::Loading
    }
    fn set_tick(&mut self, tick: usize) {
        self.wizard.tick = tick;
    }
    fn key(&mut self, key: crossterm::event::KeyEvent) -> Option<Exit> {
        match self.wizard.key(key) {
            Step::None => None,
            Step::Cancel => Some(self.cancelled()),
            Step::LoadModels => {
                let (send, result) = tokio::sync::oneshot::channel();
                let (home, url) = (self.home.clone(), self.url.clone());
                let task = tokio::spawn(async move {
                    let _ = send.send(ModelCatalog::load(&home, &url).await);
                });
                self.loading = Some(Loading { result, task });
                None
            }
            Step::Finish(config) => Some(Exit::Finish(config)),
        }
    }
    fn paste(&mut self, text: &str) {
        self.wizard.paste(text);
    }
    fn poll(&mut self) -> std::io::Result<Progress<Exit>> {
        use tokio::sync::oneshot::error::TryRecvError;
        let Some(loading) = &mut self.loading else {
            return Ok(Progress::Idle);
        };
        match loading.result.try_recv() {
            Err(TryRecvError::Empty) => Ok(Progress::Idle),
            Err(TryRecvError::Closed) => Err(std::io::Error::other(
                "model list task stopped unexpectedly",
            )),
            Ok(Err(error)) => Err(std::io::Error::other(error)),
            Ok(Ok(catalog)) => {
                self.loading = None;
                self.wizard.models_loaded(catalog);
                Ok(Progress::Changed)
            }
        }
    }
    fn cancelled(&mut self) -> Exit {
        self.loading = None;
        Exit::Cancelled
    }
}

/// Drives the wizard on the alternate screen. Cancellation (a key or a signal, even
/// one arriving while the last answer is queued) restores the terminal and writes
/// nothing: no config, schema, appearance cache, or models.dev cache. A model-list
/// failure restores the terminal and returns the error for `main` to print. The
/// config is written only after the terminal is released so a validation message
/// lands on a restored screen; the caches are published after the config. A signal
/// arriving during those writes keeps what was written but still cancels, so the
/// session does not start.
pub async fn run(home: &Path, env: &HashMap<String, String>) -> anyhow::Result<Outcome> {
    let signals = tui::SessionSignals::open()?;
    let appearance = tui::detect_appearance(home);
    let mut wizard = Wizard::new(appearance.appearance, providers::available(env));
    wizard.color_level = tui::color_level(std::io::stderr().is_terminal());
    let mut dialog = WizardDialog {
        wizard,
        home: home.to_path_buf(),
        url: catalog_url(env),
        loading: None,
    };
    let mut terminal = tui::TerminalSession::open()?;
    let exit = tui::drive(&mut dialog, &mut terminal, || signals.received()).await?;
    drop(terminal);
    let Exit::Finish(config) = exit else {
        return Ok(Outcome::Cancelled);
    };
    publish(
        home,
        &config,
        dialog.wizard.catalog(),
        appearance,
        env,
        || signals.received(),
    )
}

/// Writes the config and caches unless `cancelled` says a signal already arrived.
/// The check is repeated after the writes: a signal during them cancels the
/// session while leaving the files in place.
fn publish(
    home: &Path,
    config: &config_file::WizardConfig,
    catalog: Option<&ModelCatalog>,
    appearance: tui::DetectedAppearance,
    env: &HashMap<String, String>,
    cancelled: impl Fn() -> bool,
) -> anyhow::Result<Outcome> {
    if cancelled() {
        return Ok(Outcome::Cancelled);
    }
    config_file::write(home, config, env).map_err(anyhow::Error::msg)?;
    if let Some(catalog) = catalog {
        let _ = catalog.publish(home);
    }
    appearance.persist(home);
    if cancelled() {
        return Ok(Outcome::Cancelled);
    }
    Ok(Outcome::Completed(appearance.appearance))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needed_only_when_config_is_absent_and_no_overlay_is_set() {
        let home = tempfile::tempdir().unwrap();
        let env = HashMap::new();
        assert!(needed(home.path(), &env));
        let overlay = HashMap::from([("SWEEP_CONFIG".to_string(), "{}".to_string())]);
        assert!(!needed(home.path(), &overlay));
        let blank = HashMap::from([("SWEEP_CONFIG".to_string(), " ".to_string())]);
        assert!(needed(home.path(), &blank));
        std::fs::write(home.path().join("config.jsonc"), "").unwrap();
        assert!(!needed(home.path(), &env), "an empty file skips setup");
    }

    #[cfg(unix)]
    #[test]
    fn dangling_symlink_is_left_for_the_loader() {
        let home = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(
            home.path().join("missing"),
            home.path().join("config.jsonc"),
        )
        .unwrap();
        assert!(!needed(home.path(), &HashMap::new()));
    }

    fn openai_config() -> config_file::WizardConfig {
        config_file::WizardConfig {
            providers: vec![config_file::ProviderEntry {
                key: "openai".into(),
                api_key: Some("sk-test".into()),
                model: Some("gpt-5".into()),
            }],
            default_provider: Some("openai".into()),
            nerd_fonts: false,
        }
    }

    #[test]
    fn signal_before_writes_cancels_without_writing() {
        let home = tempfile::tempdir().unwrap();
        let appearance = tui::detect_appearance(home.path());
        let outcome = publish(
            home.path(),
            &openai_config(),
            None,
            appearance,
            &HashMap::new(),
            || true,
        )
        .unwrap();
        assert_eq!(outcome, Outcome::Cancelled);
        assert!(!home.path().join("config.jsonc").exists());
    }

    #[test]
    fn signal_during_writes_cancels_but_keeps_the_config() {
        let home = tempfile::tempdir().unwrap();
        let appearance = tui::detect_appearance(home.path());
        let polls = std::cell::Cell::new(0);
        let cancelled = || {
            polls.set(polls.get() + 1);
            polls.get() > 1
        };
        let outcome = publish(
            home.path(),
            &openai_config(),
            None,
            appearance,
            &HashMap::new(),
            cancelled,
        )
        .unwrap();
        assert_eq!(outcome, Outcome::Cancelled);
        assert!(home.path().join("config.jsonc").exists());
        assert!(home.path().join("config.schema.json").exists());
    }

    #[test]
    fn no_signal_completes_with_the_probed_appearance() {
        let home = tempfile::tempdir().unwrap();
        let appearance = tui::detect_appearance(home.path());
        let outcome = publish(
            home.path(),
            &openai_config(),
            None,
            appearance,
            &HashMap::new(),
            || false,
        )
        .unwrap();
        assert_eq!(outcome, Outcome::Completed(appearance.appearance));
        assert!(home.path().join("config.jsonc").exists());
    }

    #[test]
    fn catalog_url_defaults_to_models_dev() {
        assert_eq!(catalog_url(&HashMap::new()), MODELS_DEV_URL);
        let blank = HashMap::from([("SWEEP_MODELS_URL".to_string(), "  ".to_string())]);
        assert_eq!(catalog_url(&blank), MODELS_DEV_URL);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_builds_honor_the_models_url_override() {
        let env = HashMap::from([(
            "SWEEP_MODELS_URL".to_string(),
            " http://127.0.0.1:1/ ".into(),
        )]);
        assert_eq!(catalog_url(&env), "http://127.0.0.1:1/");
    }
}
