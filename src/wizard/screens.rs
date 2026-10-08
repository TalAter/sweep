//! Setup-wizard state machine: pure key handling with no terminal, I/O, or network.
use super::config_file::{ProviderEntry, WizardConfig};
use super::providers::{Kind, Provider};
use crate::models_dev::{ModelCatalog, ModelChoice};
use crate::tui::{Appearance, LineEditor};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::{cell::Cell, collections::HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    NerdFonts,
    Providers,
    Loading,
    ApiKey { provider: &'static Provider },
    Model { provider: &'static Provider },
    Disclaimer { provider: &'static Provider },
    Default,
}

/// What the runner must do after a key: nothing, abort, fetch the model catalog
/// (then call [`Wizard::models_loaded`]), or write the finished config. The catalog
/// is requested only when a selected provider needs a model list and none has been
/// loaded yet; later submissions reuse it.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    None,
    Cancel,
    LoadModels,
    Finish(WizardConfig),
}

pub struct Wizard {
    pub(super) appearance: Appearance,
    pub(super) color_level: u8,
    pub(super) tick: usize,
    /// The user's answer; `false` until the nerd-fonts screen is answered.
    pub(super) nerd_fonts: bool,
    pub(super) available: Vec<&'static Provider>,
    pub(super) checked: Vec<bool>,
    /// Row index on the current screen's list.
    pub(super) cursor: usize,
    /// First visible body row; the renderer moves it to keep the cursor on screen.
    pub(super) scroll: Cell<usize>,
    pub(super) screen: Screen,
    pub(super) editor: LineEditor,
    pub(super) configured: Vec<ProviderEntry>,
    /// Loaded once per run and reused when the provider list is submitted again.
    catalog: Option<ModelCatalog>,
    queue: Vec<&'static Provider>,
    position: usize,
    pending_key: Option<String>,
    models: HashMap<&'static str, Vec<ModelChoice>>,
}

impl Wizard {
    pub fn new(appearance: Appearance, available: Vec<&'static Provider>) -> Self {
        Self {
            appearance,
            color_level: 3,
            tick: 0,
            nerd_fonts: false,
            checked: vec![false; available.len()],
            available,
            cursor: 0,
            scroll: Cell::new(0),
            screen: Screen::Welcome,
            editor: LineEditor::masked('•'),
            configured: Vec::new(),
            catalog: None,
            queue: Vec::new(),
            position: 0,
            pending_key: None,
            models: HashMap::new(),
        }
    }
    pub fn render(&self, frame: &mut ratatui::Frame<'_>) {
        super::render::render(self, frame);
    }
    /// The catalog to publish once setup completes.
    pub fn catalog(&self) -> Option<&ModelCatalog> {
        self.catalog.as_ref()
    }
    pub(super) fn models_for(&self, provider: &Provider) -> &[ModelChoice] {
        self.models.get(provider.key).map_or(&[], Vec::as_slice)
    }

    pub fn key(&mut self, key: KeyEvent) -> Step {
        if key.kind == KeyEventKind::Release {
            return Step::None;
        }
        let ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl_c {
            return Step::Cancel;
        }
        let esc = key.code == KeyCode::Esc;
        match self.screen {
            Screen::Welcome => match key.code {
                KeyCode::Up | KeyCode::Down => self.scroll_text(key.code),
                KeyCode::Enter => self.go(Screen::NerdFonts),
                _ if esc => return Step::Cancel,
                _ => {}
            },
            Screen::NerdFonts => match key.code {
                KeyCode::Up | KeyCode::Down => self.step_cursor(key.code, 2),
                KeyCode::Enter => {
                    self.nerd_fonts = self.cursor == 0;
                    self.go(Screen::Providers);
                }
                _ if esc => return Step::Cancel,
                _ => {}
            },
            Screen::Providers => match key.code {
                KeyCode::Up | KeyCode::Down => self.step_cursor(key.code, self.available.len()),
                KeyCode::Char(' ') => {
                    if let Some(slot) = self.checked.get_mut(self.cursor) {
                        *slot = !*slot;
                    }
                }
                KeyCode::Enter => {
                    self.queue = self
                        .available
                        .iter()
                        .zip(&self.checked)
                        .filter(|(_, checked)| **checked)
                        .map(|(provider, _)| *provider)
                        .collect();
                    if self.queue.is_empty() {
                        return self.finish_without_llm();
                    }
                    let needs_catalog = self.queue.iter().any(|p| p.kind == Kind::Api);
                    if needs_catalog && self.catalog.is_none() {
                        self.go(Screen::Loading);
                        return Step::LoadModels;
                    }
                    return self.start_loop();
                }
                _ if esc => return Step::Cancel,
                _ => {}
            },
            Screen::Loading => {
                if esc {
                    return Step::Cancel;
                }
            }
            Screen::ApiKey { provider } => match key.code {
                KeyCode::Enter => {
                    let trimmed = self.editor.text().trim();
                    if !trimmed.is_empty() {
                        self.pending_key = Some(trimmed.to_string());
                        self.go(Screen::Model { provider });
                    }
                }
                _ if esc => return Step::Cancel,
                _ => {
                    self.editor.key(key);
                }
            },
            Screen::Model { provider } => {
                let models = self.models_for(provider);
                if models.is_empty() {
                    if esc {
                        return self.skip_provider();
                    }
                    return Step::None;
                }
                match key.code {
                    KeyCode::Up | KeyCode::Down => {
                        let len = models.len();
                        self.step_cursor(key.code, len);
                    }
                    KeyCode::Enter => {
                        let model = models[self.cursor.min(models.len() - 1)].id.clone();
                        let api_key = self.pending_key.take();
                        return self.record(ProviderEntry {
                            key: provider.key.to_string(),
                            api_key,
                            model: Some(model),
                        });
                    }
                    _ if esc => return Step::Cancel,
                    _ => {}
                }
            }
            Screen::Disclaimer { provider } => match key.code {
                KeyCode::Up | KeyCode::Down => self.scroll_text(key.code),
                KeyCode::Enter => {
                    return self.record(ProviderEntry {
                        key: provider.key.to_string(),
                        api_key: None,
                        model: None,
                    });
                }
                _ if esc => return self.skip_provider(),
                _ => {}
            },
            Screen::Default => match key.code {
                KeyCode::Up | KeyCode::Down => self.step_cursor(key.code, self.configured.len()),
                KeyCode::Enter => {
                    let default = self.configured.get(self.cursor).map(|e| e.key.clone());
                    return self.finish(default);
                }
                _ if esc => return Step::Cancel,
                _ => {}
            },
        }
        Step::None
    }

    /// Pasted text only matters on the API-key screen; elsewhere it is dropped.
    pub fn paste(&mut self, text: &str) {
        if matches!(self.screen, Screen::ApiKey { .. }) {
            self.editor.paste(text);
        }
    }

    /// Keeps the catalog for the rest of the run and opens the first provider's screen.
    pub fn models_loaded(&mut self, catalog: ModelCatalog) {
        self.catalog = Some(catalog);
        self.start_loop();
    }
    /// Walks the selected providers from the top, with each API provider's usable
    /// models taken from the catalog (none when it was never needed).
    fn start_loop(&mut self) -> Step {
        self.models = match &self.catalog {
            Some(catalog) => self
                .queue
                .iter()
                .filter(|p| p.kind == Kind::Api)
                .map(|p| (p.key, catalog.models(p.key, p.recommended)))
                .collect(),
            None => HashMap::new(),
        };
        self.position = 0;
        self.enter_provider()
    }

    fn go(&mut self, screen: Screen) {
        self.screen = screen;
        self.cursor = 0;
        self.scroll.set(0);
        self.editor.clear();
    }
    /// Text-only screens scroll by row; the renderer clamps to what overflows.
    fn scroll_text(&mut self, code: KeyCode) {
        let scroll = self.scroll.get();
        self.scroll.set(match code {
            KeyCode::Up => scroll.saturating_sub(1),
            _ => scroll + 1,
        });
    }
    fn step_cursor(&mut self, code: KeyCode, len: usize) {
        self.cursor = match code {
            KeyCode::Up => self.cursor.saturating_sub(1),
            _ => (self.cursor + 1).min(len.saturating_sub(1)),
        };
    }
    fn record(&mut self, entry: ProviderEntry) -> Step {
        self.configured.push(entry);
        self.position += 1;
        self.enter_provider()
    }
    fn skip_provider(&mut self) -> Step {
        self.pending_key = None;
        self.position += 1;
        self.enter_provider()
    }
    /// Opens the current queue entry's first screen, or wraps up once the queue is done.
    fn enter_provider(&mut self) -> Step {
        let Some(provider) = self.queue.get(self.position).copied() else {
            return self.after_loop();
        };
        self.go(match provider.kind {
            Kind::Cli { .. } => Screen::Disclaimer { provider },
            Kind::Api if provider.key_url.is_some() => Screen::ApiKey { provider },
            Kind::Api => Screen::Model { provider },
        });
        Step::None
    }
    fn after_loop(&mut self) -> Step {
        match self.configured.len() {
            0 => {
                self.checked.iter_mut().for_each(|c| *c = false);
                self.queue.clear();
                self.go(Screen::Providers);
                Step::None
            }
            1 => {
                let default = self.configured[0].key.clone();
                self.finish(Some(default))
            }
            _ => {
                self.go(Screen::Default);
                Step::None
            }
        }
    }
    /// The no-LLM hatch: an empty `providers` map the user can fill in by hand.
    fn finish_without_llm(&mut self) -> Step {
        self.finish(None)
    }
    fn finish(&mut self, default_provider: Option<String>) -> Step {
        Step::Finish(WizardConfig {
            providers: std::mem::take(&mut self.configured),
            default_provider,
            nerd_fonts: self.nerd_fonts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wizard::providers;
    use serde_json::json;

    fn wizard(keys: &[&str]) -> Wizard {
        let available = keys.iter().map(|k| providers::find(k).unwrap()).collect();
        Wizard::new(Appearance::Dark, available)
    }
    fn press(w: &mut Wizard, code: KeyCode) -> Step {
        w.key(KeyEvent::new(code, KeyModifiers::NONE))
    }
    fn enter(w: &mut Wizard) -> Step {
        press(w, KeyCode::Enter)
    }
    fn model(date: &str) -> serde_json::Value {
        json!({
            "tool_call": true,
            "release_date": date,
            "modalities": { "input": ["text"], "output": ["text"] }
        })
    }
    fn catalog() -> ModelCatalog {
        ModelCatalog::from_value(json!({
            "anthropic": { "models": {
                "claude-opus-4-1": model("2025-08-05"),
                "claude-sonnet-4-6": model("2026-02-17"),
                "claude-haiku-4-5": model("2025-10-15"),
            }},
            "openai": { "models": {
                "gpt-5": model("2025-08-07"),
                "gpt-4.1": model("2025-04-14"),
            }},
            "ollama": { "models": { "llama3": model("2024-04-18") } },
        }))
    }
    /// Welcome → nerd-fonts answer → Providers.
    fn to_providers(w: &mut Wizard, nerd: bool) {
        assert_eq!(enter(w), Step::None);
        assert_eq!(w.screen, Screen::NerdFonts);
        if !nerd {
            press(w, KeyCode::Down);
        }
        assert_eq!(enter(w), Step::None);
        assert_eq!(w.screen, Screen::Providers);
    }
    /// Checks the rows at `indexes` (in that order) and submits.
    fn submit_providers(w: &mut Wizard, indexes: &[usize]) -> Step {
        for &i in indexes {
            while w.cursor < i {
                press(w, KeyCode::Down);
            }
            while w.cursor > i {
                press(w, KeyCode::Up);
            }
            press(w, KeyCode::Char(' '));
        }
        enter(w)
    }
    fn entry(key: &str, api_key: Option<&str>, model: Option<&str>) -> ProviderEntry {
        ProviderEntry {
            key: key.into(),
            api_key: api_key.map(Into::into),
            model: model.map(Into::into),
        }
    }
    fn anthropic() -> &'static Provider {
        providers::find("anthropic").unwrap()
    }

    #[test]
    fn esc_on_welcome_cancels() {
        let mut w = wizard(&["anthropic"]);
        assert_eq!(press(&mut w, KeyCode::Esc), Step::Cancel);
    }

    #[test]
    fn anthropic_only_happy_path_records_key_recommended_model_and_default() {
        let mut w = wizard(&["anthropic", "openai"]);
        to_providers(&mut w, true);
        assert!(w.nerd_fonts);
        assert_eq!(submit_providers(&mut w, &[0]), Step::LoadModels);
        assert_eq!(w.screen, Screen::Loading);
        w.models_loaded(catalog());
        assert_eq!(
            w.screen,
            Screen::ApiKey {
                provider: anthropic()
            }
        );
        w.paste("  sk-ant-x  ");
        assert_eq!(enter(&mut w), Step::None);
        assert_eq!(
            w.screen,
            Screen::Model {
                provider: anthropic()
            }
        );
        assert_eq!(
            enter(&mut w),
            Step::Finish(WizardConfig {
                providers: vec![entry(
                    "anthropic",
                    Some("sk-ant-x"),
                    Some("claude-sonnet-4-6")
                )],
                default_provider: Some("anthropic".into()),
                nerd_fonts: true,
            })
        );
    }

    #[test]
    fn down_on_model_screen_picks_second_newest_model() {
        let mut w = wizard(&["anthropic"]);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[0]);
        w.models_loaded(catalog());
        w.paste("k");
        enter(&mut w);
        press(&mut w, KeyCode::Down);
        let Step::Finish(config) = enter(&mut w) else {
            panic!("expected finish");
        };
        assert_eq!(
            config.providers[0].model.as_deref(),
            Some("claude-haiku-4-5")
        );
    }

    #[test]
    fn two_providers_reach_default_picker_and_down_selects_openai() {
        let mut w = wizard(&["anthropic", "openai"]);
        to_providers(&mut w, false);
        assert_eq!(submit_providers(&mut w, &[1, 0]), Step::LoadModels);
        w.models_loaded(catalog());
        assert_eq!(
            w.screen,
            Screen::ApiKey {
                provider: anthropic()
            },
            "selection order is normalized to table order"
        );
        w.paste("a");
        enter(&mut w);
        enter(&mut w);
        assert_eq!(
            w.screen,
            Screen::ApiKey {
                provider: providers::find("openai").unwrap()
            }
        );
        w.paste("o");
        enter(&mut w);
        assert_eq!(enter(&mut w), Step::None);
        assert_eq!(w.screen, Screen::Default);
        press(&mut w, KeyCode::Down);
        assert_eq!(
            enter(&mut w),
            Step::Finish(WizardConfig {
                providers: vec![
                    entry("anthropic", Some("a"), Some("claude-sonnet-4-6")),
                    entry("openai", Some("o"), Some("gpt-5")),
                ],
                default_provider: Some("openai".into()),
                nerd_fonts: false,
            })
        );
    }

    #[test]
    fn nothing_checked_finishes_with_empty_providers() {
        let mut w = wizard(&["anthropic", "claude-code"]);
        to_providers(&mut w, false);
        assert_eq!(
            enter(&mut w),
            Step::Finish(WizardConfig {
                providers: vec![],
                default_provider: None,
                nerd_fonts: false,
            })
        );
    }

    #[test]
    fn empty_or_whitespace_key_is_a_silent_no_op() {
        let mut w = wizard(&["anthropic"]);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[0]);
        w.models_loaded(catalog());
        assert_eq!(enter(&mut w), Step::None);
        w.paste("   ");
        assert_eq!(enter(&mut w), Step::None);
        assert_eq!(
            w.screen,
            Screen::ApiKey {
                provider: anthropic()
            }
        );
        assert_eq!(press(&mut w, KeyCode::Esc), Step::Cancel);
    }

    #[test]
    fn ollama_skips_api_key_and_writes_model_only() {
        let mut w = wizard(&["ollama"]);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[0]);
        w.models_loaded(catalog());
        assert_eq!(
            w.screen,
            Screen::Model {
                provider: providers::find("ollama").unwrap()
            }
        );
        let Step::Finish(config) = enter(&mut w) else {
            panic!("expected finish");
        };
        assert_eq!(
            config.providers,
            vec![entry("ollama", None, Some("llama3"))]
        );
    }

    #[test]
    fn claude_code_alone_esc_on_disclaimer_returns_to_cleared_providers() {
        let mut w = wizard(&["anthropic", "claude-code"]);
        to_providers(&mut w, true);
        assert_eq!(
            submit_providers(&mut w, &[1]),
            Step::None,
            "a CLI-only selection needs no model list"
        );
        assert_eq!(
            w.screen,
            Screen::Disclaimer {
                provider: providers::find("claude-code").unwrap()
            }
        );
        assert_eq!(press(&mut w, KeyCode::Esc), Step::None);
        assert_eq!(w.screen, Screen::Providers);
        // The cursor is back on the first row and the earlier selection is gone:
        // toggling once selects Anthropic alone, which finishes without a disclaimer.
        press(&mut w, KeyCode::Char(' '));
        assert_eq!(enter(&mut w), Step::LoadModels);
        w.models_loaded(catalog());
        w.paste("k");
        enter(&mut w);
        let Step::Finish(config) = enter(&mut w) else {
            panic!("expected finish");
        };
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.default_provider.as_deref(), Some("anthropic"));
    }

    #[test]
    fn anthropic_plus_claude_code_esc_on_disclaimer_finishes_without_picker() {
        let mut w = wizard(&["anthropic", "claude-code"]);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[0, 1]);
        w.models_loaded(catalog());
        w.paste("k");
        enter(&mut w);
        enter(&mut w);
        assert!(matches!(w.screen, Screen::Disclaimer { .. }));
        let Step::Finish(config) = press(&mut w, KeyCode::Esc) else {
            panic!("expected finish");
        };
        assert_eq!(config.default_provider.as_deref(), Some("anthropic"));
        assert_eq!(config.providers.len(), 1);
    }

    #[test]
    fn enter_on_disclaimer_records_empty_cli_entry() {
        let mut w = wizard(&["claude-code"]);
        to_providers(&mut w, true);
        assert_eq!(submit_providers(&mut w, &[0]), Step::None);
        let Step::Finish(config) = enter(&mut w) else {
            panic!("expected finish");
        };
        assert_eq!(config.providers, vec![entry("claude-code", None, None)]);
        assert_eq!(config.default_provider.as_deref(), Some("claude-code"));
    }

    #[test]
    fn reselecting_an_api_provider_after_a_cli_only_round_loads_models() {
        let mut w = wizard(&["anthropic", "claude-code"]);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[1]);
        press(&mut w, KeyCode::Esc);
        assert_eq!(w.screen, Screen::Providers);
        assert_eq!(submit_providers(&mut w, &[0, 1]), Step::LoadModels);
        w.models_loaded(catalog());
        assert_eq!(
            w.screen,
            Screen::ApiKey {
                provider: anthropic()
            }
        );
    }

    #[test]
    fn provider_absent_from_catalog_ignores_enter_and_esc_skips_it() {
        let mut w = wizard(&["groq"]);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[0]);
        w.models_loaded(catalog());
        w.paste("gsk");
        enter(&mut w);
        assert!(matches!(w.screen, Screen::Model { .. }));
        assert_eq!(enter(&mut w), Step::None);
        assert!(matches!(w.screen, Screen::Model { .. }));
        assert_eq!(press(&mut w, KeyCode::Esc), Step::None);
        assert_eq!(w.screen, Screen::Providers);
        assert_eq!(
            enter(&mut w),
            Step::Finish(WizardConfig {
                providers: vec![],
                default_provider: None,
                nerd_fonts: true,
            }),
            "the selection was cleared"
        );
    }

    #[test]
    fn skipping_a_provider_absent_from_catalog_finishes_with_the_other() {
        let mut w = wizard(&["anthropic", "groq"]);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[0, 1]);
        w.models_loaded(catalog());
        w.paste("k");
        enter(&mut w);
        enter(&mut w);
        w.paste("gsk");
        enter(&mut w);
        let Step::Finish(config) = press(&mut w, KeyCode::Esc) else {
            panic!("expected finish");
        };
        assert_eq!(config.default_provider.as_deref(), Some("anthropic"));
    }

    #[test]
    fn esc_cancels_on_model_list() {
        let mut w = wizard(&["anthropic"]);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[0]);
        w.models_loaded(catalog());
        w.paste("a");
        enter(&mut w);
        assert!(matches!(w.screen, Screen::Model { .. }));
        assert_eq!(press(&mut w, KeyCode::Esc), Step::Cancel);
    }

    #[test]
    fn esc_cancels_on_default_picker() {
        let mut w = wizard(&["anthropic", "openai"]);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[0, 1]);
        w.models_loaded(catalog());
        w.paste("a");
        enter(&mut w);
        enter(&mut w);
        w.paste("o");
        enter(&mut w);
        enter(&mut w);
        assert_eq!(w.screen, Screen::Default);
        assert_eq!(press(&mut w, KeyCode::Esc), Step::Cancel);
    }

    #[test]
    fn resubmitting_providers_reuses_the_loaded_catalog_without_loading() {
        let mut w = wizard(&["anthropic", "groq"]);
        to_providers(&mut w, true);
        assert_eq!(submit_providers(&mut w, &[1]), Step::LoadModels);
        w.models_loaded(catalog());
        w.paste("gsk");
        enter(&mut w);
        assert_eq!(
            press(&mut w, KeyCode::Esc),
            Step::None,
            "groq has no models"
        );
        assert_eq!(w.screen, Screen::Providers);
        assert_eq!(
            submit_providers(&mut w, &[0]),
            Step::None,
            "the catalog is already loaded"
        );
        assert_eq!(
            w.screen,
            Screen::ApiKey {
                provider: anthropic()
            }
        );
        w.paste("k");
        enter(&mut w);
        let Step::Finish(config) = enter(&mut w) else {
            panic!("expected finish");
        };
        assert_eq!(
            config.providers,
            vec![entry("anthropic", Some("k"), Some("claude-sonnet-4-6"))]
        );
    }

    #[test]
    fn welcome_and_disclaimer_scroll_without_leaving_the_screen() {
        let mut w = wizard(&["claude-code"]);
        assert_eq!(press(&mut w, KeyCode::Down), Step::None);
        assert_eq!(press(&mut w, KeyCode::Down), Step::None);
        assert_eq!(w.scroll.get(), 2);
        assert_eq!(press(&mut w, KeyCode::Up), Step::None);
        assert_eq!(w.scroll.get(), 1);
        assert_eq!(w.screen, Screen::Welcome);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[0]);
        assert!(matches!(w.screen, Screen::Disclaimer { .. }));
        assert_eq!(w.scroll.get(), 0, "a new screen starts at the top");
        assert_eq!(press(&mut w, KeyCode::Down), Step::None);
        assert_eq!(w.scroll.get(), 1);
        assert_eq!(press(&mut w, KeyCode::Up), Step::None);
        assert_eq!(press(&mut w, KeyCode::Up), Step::None);
        assert_eq!(w.scroll.get(), 0);
        assert!(matches!(w.screen, Screen::Disclaimer { .. }));
    }

    #[test]
    fn ctrl_c_cancels_model_screen_without_models() {
        let mut w = wizard(&["groq"]);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[0]);
        w.models_loaded(catalog());
        w.paste("gsk");
        enter(&mut w);
        assert!(matches!(w.screen, Screen::Model { .. }));
        assert_eq!(
            w.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Step::Cancel
        );
    }

    #[test]
    fn ctrl_c_cancels_everywhere_including_disclaimer() {
        let mut w = wizard(&["claude-code"]);
        to_providers(&mut w, true);
        submit_providers(&mut w, &[0]);
        assert!(matches!(w.screen, Screen::Disclaimer { .. }));
        assert_eq!(
            w.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Step::Cancel
        );
    }

    #[test]
    fn nerd_answer_no_keeps_nerd_fonts_false_and_esc_cancels_on_loading() {
        let mut w = wizard(&["anthropic"]);
        to_providers(&mut w, false);
        assert!(!w.nerd_fonts);
        submit_providers(&mut w, &[0]);
        assert_eq!(press(&mut w, KeyCode::Esc), Step::Cancel);
    }

    #[test]
    fn key_releases_are_ignored() {
        let mut w = wizard(&["anthropic"]);
        let mut release = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        assert_eq!(w.key(release), Step::None);
        assert_eq!(w.screen, Screen::Welcome);
    }
}
