# First-run setup wizard

Sweep currently starts with no config and silently shows "No LLM provider
configured" in the insight panel. Add an interactive setup wizard that runs
when there is no config, writes `config.jsonc` plus a schema file, then
continues into whatever the user launched.

**Implement from this spec only.** Do not read sibling repositories under
`/Users/tal/mysite` (e.g. `wrap`, `wrap-core`); this is a clean
reimplementation, not a port.

## Trigger

- Runs when `<home>/config.jsonc` does not exist. Any existing file, even
  empty, skips it. A non-empty `SWEEP_CONFIG` also skips it.
- `sweep list` never triggers it.
- Runs before fetching or analyzing anything, for both bare `sweep` and
  `sweep '<install command>'`.
- Needs a TTY on stderr (same requirement as the session). Without one, print
  `sweep: no config found; run sweep in a terminal to set one up.` and exit 2.
- Ctrl-C, Esc, SIGINT/SIGTERM/SIGHUP cancel, as in the session (differs from
  wrap, which ignores Ctrl-C). Cancel: restore terminal, write nothing, print
  nothing, exit 0. Exception: Esc on the disclaimer screen (below).
- Appearance is not asked; `resolve_appearance` already auto-detects.

## After completion

1. Write `config.jsonc` and `config.schema.json` (formats below).
2. Print `🧠 Configuration saved` to stderr.
3. Continue: bare launch opens the interactive paste screen; a launch with an
   install command proceeds to fetch and analyze it. The freshly written config
   must take effect in that same run (provider and `nerdFonts`).

Today the session ignores `nerdFonts` entirely. `nerdFonts` (bool, default
false) from config must reach the session so pill caps render.

## Screens

Order: Welcome → Nerd fonts → Providers → Loading → per-provider loop →
Default picker (only if ≥2 providers remain). Back navigation is not
required.

Shared: same alt-screen card, border gradient, and padding as the session,
wide enough for the mockups below. Action bar: bold key glyph then a
lowercase label (`⏎ to continue`, `Esc to cancel`), primary key in gold,
` │ ` separator, indented 3 columns. This wording is deliberate and differs
from the session's `Enter Run` style.

### 1. Welcome

No pill. Logo rows get a top-to-bottom gradient, bold. No animation (wrap's
right-hand brain animation is dropped deliberately).

```
╭────────────────────────────────────────────────────────────────────────╮
│                                                                        │
│    ███████╗██╗    ██╗███████╗███████╗██████╗                           │  logo gradient
│    ██╔════╝██║    ██║██╔════╝██╔════╝██╔══██╗                          │
│    ███████╗██║ █╗ ██║█████╗  █████╗  ██████╔╝                          │
│    ╚════██║██║███╗██║██╔══╝  ██╔══╝  ██╔═══╝                           │
│    ███████║╚███╔███╔╝███████╗███████╗██║                               │
│    ╚══════╝ ╚══╝╚══╝ ╚══════╝╚══════╝╚═╝                               │
│    brew for the things you cannot brew.                                │  "brew" ×2 bold gold
│                                                                        │
│                                                                        │
│    Quick one-time setup — pick your AI provider and a couple of        │
│    preferences.                                                        │
│                                                                        │
│                                                                        │
│    ◷ Takes ~45 seconds.                                                │  "~45 seconds." success green
│                                                                        │
│                                                                        │
│    ⏎ to continue │ Esc to cancel                                       │
│                                                                        │
╰────────────────────────────────────────────────────────────────────────╯
```

Enter → Nerd fonts.

### 2. Nerd fonts

Pill: `🧙 Setup Wizard`.

```
╭─ 🧙 Setup Wizard ──────────────────────────────────────────────────────╮
│                                                                        │
│ Do you see four icons below?                                           │
│                                                                        │
│     󰣙                                                         │  U+F08D9 U+F1D0 U+EDD6 U+F1D1, 2 spaces apart
│                                                                        │
│   ❯ Yes — enable icons throughout Sweep                                │  ❯ gold, label body
│     No — they look like boxes or question marks                        │  muted
│                                                                        │
│    ↑↓ to move │ ⏎ to select                                            │
│                                                                        │
╰────────────────────────────────────────────────────────────────────────╯
```

Cursor starts on Yes. Enter records `nerdFonts`. The wizard's
own glyph mode (pill caps, breadcrumb icons, provider icons) switches
immediately to the answer for the remaining screens (wrap keeps the mode fixed
for the whole wizard; we don't).

### 3. Providers

Pill: breadcrumb (see Breadcrumb). A CLI provider whose command is not
available is hidden; the CLI section header is hidden if none are.

```
╭─ 🧙 Setup Wizard  Providers ───────────────────────────────────────────╮
│                                                                        │
│ Sweep needs at least one LLM provider configured.                      │
│                                                                        │
│ ⠶⠶ SELECT API PROVIDER(S) ⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶ │  ⠶ dim, label bold supporting
│    [✓] Anthropic                                                       │  checked: success green
│  ❯ [ ] OpenAI                                                          │  focused: body text on focus bg, full width
│    [ ] OpenRouter                                                      │  muted
│    [ ] Groq                                                            │
│    [ ] Mistral                                                         │
│    [ ] Ollama (local)                                                  │
│                                                                        │
│ ⠶⠶ USE YOUR CODING AGENT'S SUBSCRIPTION ⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶⠶ │
│    [ ] Claude Code                                                     │
│                                                                        │
│    Space to toggle │ ⏎ to continue                                     │  nothing checked: "Space to toggle │ ⏎ to continue without an LLM"
│                                                                        │
╰────────────────────────────────────────────────────────────────────────╯
```

- Section header: `⠶⠶ LABEL ⠶⠶…` filling the width; label only if width <
  label+8. Blank line before every header except the first.
- Row: pointer (` ❯` focused, else 2 spaces), space, `[✓]`/`[ ]`, space,
  nerd icon + space in nerd mode, display name.
- ↑↓ move over rows only, start on Anthropic. Space toggles.
  Selection order is normalized to table order regardless of tick order.
- **No-LLM exit hatch.** Enter with nothing checked finishes the wizard
  immediately: no models fetch, no loop, no default picker. The config is
  written with `"providers": {}` and no `defaultProvider`, and the session
  shows its existing "No LLM provider configured" insight. The user can add a
  provider later by editing the file. (Wrap has no such hatch; Enter there is
  a no-op.)

### 4. Loading

Fetch models.dev (below). Body: `<spinner> Loading models…` with the
session's spinner. Bottom border status `Loading models
list…`. Esc cancels. On success → first provider's first screen.

Failure (no usable cache or invalid JSON): restore terminal, print
`sweep: could not load model list from https://models.dev/api.json. Check
your connection and try again.`, exit 1, write nothing. (Wrap's behavior here
is accidental; this is the intended one.)

### 5. Per-provider loop

For each selected provider in table order:

| type | screens |
|---|---|
| CLI (claude-code) | Disclaimer |
| API with key URL (anthropic, openai, openrouter, groq, mistral) | API key → Model |
| API without key URL (ollama) | Model |

**API key**

```
╭─ 🧙 Setup Wizard  Anthropic  API Key ──────────────────────────────────╮
│                                                                        │
│ Anthropic API key                                                      │  bold
│ Get one: https://console.anthropic.com/settings/keys                   │  URL muted
│                                                                        │
│  •••••••••█                                                            │  surface bg; █ = cursor
│                                                                        │
│    ⏎ to continue                                                       │
│                                                                        │
╰────────────────────────────────────────────────────────────────────────╯
```

Single-line masked input (`•` per char), full width, 1 col padding, same
editing keys and paste handling as the session's paste input, newlines
stripped. Empty field shows cursor then
placeholder in muted color. Enter submits trimmed value; empty/whitespace is
a silent no-op. No format check. Esc cancels the wizard.

**Model**

```
╭─ 🧙 Setup Wizard  Anthropic  ✓ API Key  Model ─────────────────────────╮
│                                                                        │
│ Anthropic model                                                        │  bold
│                                                                        │
│ ❯ claude-sonnet-4-6  ✦ Recommended                                     │  body
│   claude-opus-4-1                                                      │  supporting
│   claude-haiku-4-5                                                     │
│                                                                        │
│    ↑↓ to move │ ⏎ to continue                                          │
│                                                                        │
╰────────────────────────────────────────────────────────────────────────╯
```

Rows show model id only; recommended row appends `  ✦ Recommended` and is
first. Scrolls when it overflows. Enter → next provider or default
picker. A configured API provider always gets a model; there is no skip here.
When models.dev has no models for the provider, show `No models found for
<name>.` with `Esc to skip this provider`, using the disclaimer's skip
semantics.

**Disclaimer** (claude-code; breadcrumb shows `Claude Code  API Key`)

> Sweep will route your queries through the **claude** CLI instead of calling
> the Anthropic API directly. This is slower, and your prompts flow through
> Claude Code under its own terms — bring your own subscription and
> credentials.

Action bar: `⏎ to accept │ Esc to skip this provider`. Enter records `{}` and
advances. Esc removes the provider: if it was the only selection, return to
Providers with all boxes cleared; otherwise continue the loop (or end it).

### 6. Default picker

Only when ≥2 providers remain; with one, it becomes the default silently.

```
╭─ 🧙 Setup Wizard  ✓ Providers  Set Default ────────────────────────────╮
│                                                                        │
│ Which provider should be the default?                                  │  bold
│                                                                        │
│ ❯ Anthropic                                                            │
│   OpenAI                                                               │
│   Claude Code                                                          │
│                                                                        │
│    ↑↓ to move │ ⏎ to select                                            │
│                                                                        │
╰────────────────────────────────────────────────────────────────────────╯
```

Scrolls when it overflows. Enter finishes.

## Breadcrumb pills

Segments are ` label ` in their own fg/bg; the wizard segment and the active
step are bold. Nerd mode wraps them in powerline caps: left U+E0B6 (fg = first
bg), separator U+E0C0 between segments (fg = previous bg, bg = next bg), right
U+E0B4 (fg = last bg). ASCII mode: segments abut with no caps.

Steps: Providers (icon U+F03A, abbr P), API Key (U+F084, K), Model (U+F1B2,
M), Set Default (U+F005, D). Done mark: U+F00C nerd, `✓` ASCII.

| screen | wide breadcrumb |
|---|---|
| Providers, Loading | `[🧙 Setup Wizard][Providers*]` |
| API key / Disclaimer for X | `[🧙 Setup Wizard][X][API Key*]` |
| Model for X | `[🧙 Setup Wizard][X][✓ API Key][Model*]` (Ollama too) |
| Default picker | `[🧙 Setup Wizard][✓ Providers][Set Default*]` |

`*` = active (bold). Provider-name segment uses the provider's glyph in nerd
mode. If the label doesn't fit: narrow breadcrumb (no wizard segment, done
steps `✓`, active full label, future steps abbreviation/icon), then narrow with
abbreviations, then no label. Bottom status is `╰─ status ──…╯`, truncated
with `…`.

## Colors

Reuse the session palette (body, supporting, surface, selected, danger, frame
gradient, action bar gold). New tokens, dark / light:

| token | dark | light |
|---|---|---|
| wizard pill fg / bg | #d2d2e1 / #1e325a | #0a145a / #96afdc |
| done step fg / bg | #82dca0 / #285a3c | #0f6e37 / #d2f0d7 |
| active step fg / bg | #b4c8ff / #3c50a0 | #1e4bbe / #d2e1fa |
| pending step fg / bg | #64648c / #323250 | #82829b / #dcdce6 |
| muted (hints, URL, unfocused rows, placeholder) | #73738c | #696982 |
| success (checked row, "~45 seconds.", chosen) | #78e6a0 | #0f7d37 |
| checklist focused row bg | #1a2a4d | #d2dcf5 |
| section header `⠶` | #3c3c64 | #aaaac3 |
| logo gradient top→bottom | #b3b3b3 → #ffffff | #a079bf → #4f5e85 |
| gold (❯ marker, "brew", primary key) | existing #f5ba4a | existing #ffa532 |

Below truecolor, degrade the same way the session does.

Glyphs that never change with mode: `❯ ⠶ ✦ ◷ ↑↓ 🧙 🧠 ✓ [✓]`.

## Providers table

| key | display | key URL | placeholder | baseURL written | recommended regex | nerd glyph |
|---|---|---|---|---|---|---|
| anthropic | Anthropic | https://console.anthropic.com/settings/keys | `sk-ant-api03-` | — | `^claude-sonnet(-\d+)+$` | U+E754 |
| openai | OpenAI | https://platform.openai.com/api-keys | `sk-proj-` | — | `^gpt-5(\.\d+)?$` | U+F0104 |
| openrouter | OpenRouter | https://openrouter.ai/keys | `sk-or-v1-` | https://openrouter.ai/api/v1 | — | U+EA63 |
| groq | Groq | https://console.groq.com/keys | `gsk_` | https://api.groq.com/openai/v1 | — | U+F140B |
| mistral | Mistral | https://console.mistral.ai/api-keys | — | https://api.mistral.ai/v1 | — | U+EF16 |
| ollama | Ollama (local) | — | — | http://localhost:11434/v1 | — | U+F019A |
| claude-code (CLI, binary `claude`) | Claude Code | — | — | — | — | U+F0BC9 |

Groq and Mistral baseURLs are new: sweep's resolver requires them, and wrap
omits them (a wrap bug that makes those providers fail at write time).

## models.dev

- URL `https://models.dev/api.json`, fetched only after Providers is
  submitted. Cache `<home>/cache/models.dev.json`, good for 24 h. Fresh
  cache → no network. Stale/missing → fetch and overwrite; if the fetch fails
  and any cache exists, use it silently.
- Models for a provider are indexed by sweep's provider key. Keep models where
  `tool_call` is exactly true, `status` ≠ `"deprecated"`, and
  `modalities.input` and `.output` both include `"text"`. Sort by
  `release_date` descending (string compare), undated last. First regex match
  is marked recommended and moved to the top; others keep order.

## Written files

`config.jsonc`: plain JSON, 2-space indent. Key order `$schema`, `providers`,
`defaultProvider`, `nerdFonts`. Provider keys in table order. Entry fields in
order `baseURL` (if any), `apiKey` (trimmed), `model`; CLI entries are `{}`.
Before writing, reject any entry the session itself would reject at
startup, printing `sweep: <message>` and exiting 1 without writing. An
empty `providers` map with no `defaultProvider` is valid.

```json
{
  "$schema": "./config.schema.json",
  "providers": {
    "anthropic": { "apiKey": "sk-ant-api03-XXXX", "model": "claude-sonnet-4-6" },
    "openai": { "apiKey": "sk-proj-XXXX", "model": "gpt-5" },
    "claude-code": {}
  },
  "defaultProvider": "openai",
  "nerdFonts": true
}
```

`config.schema.json`: draft-07, title "Sweep configuration file", properties
`$schema` (string), `providers` (map of name → object with optional string
`apiKey`, `baseURL`, `model`, no extra fields), `defaultProvider` (string),
`nerdFonts` (bool, default false). One-line descriptions each, including that
`apiKey` may be `"$ENV_VAR"`. Top level allows unknown keys (the loader
preserves them and older binaries must tolerate newer configs); provider
entries do not. Written every time the wizard completes.

## Acceptance scenarios

- No config, TTY, Esc on Welcome: exit 0, home contains no config or schema.
- No config, stderr not a TTY: the message above, exit 2.
- `SWEEP_CONFIG` set, no file: wizard skipped.
- Existing empty `config.jsonc`: wizard skipped.
- Anthropic only, key, Enter on model: file has sonnet (recommended first),
  default anthropic, nerdFonts per answer; schema file exists; run continues
  into paste screen (bare) or into fetch (with command) using the new config.
- Anthropic + OpenAI, Down on default picker: default openai.
- Nothing checked, Enter: no network request, config written with empty
  providers and no default, schema written, session continues and shows the
  no-LLM insight.
- Provider absent from models.dev: model screen shows the not-found message;
  Esc skips that provider.
- Claude Code alone, Esc on disclaimer: back on Providers, nothing checked.
- Anthropic + Claude Code, Esc on disclaimer: finishes with anthropic default,
  no picker.
- `claude` not on PATH: no Claude Code row and no CLI header.
- Fresh cache: no network request. Stale cache + fetch failure: stale used.
  No cache + failure: error message, exit 1.
- Filtering/sorting/recommendation rules each covered with a small fixture.
- Breadcrumb wide/narrow/none fallback at decreasing widths.
- `nerdFonts: true` in config makes the session render pill caps.
