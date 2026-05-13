---
name: README
description: Sweep — high-level product spec. What we're building and why. Read at session start.
---

# Sweep

**Brew for what you cannot brew.**

Modern dev tools increasingly ship via `curl https://example.com/install.sh | sh`. The user loses the ergonomics of modern package managers — no `list`, no `update`, no `uninstall`, no sense of what just ran, easily losing track of what is installed on their machine.

Sweep is a package manager for the long tail of dev tools that ship via `curl | sh`. It empowers you to control what you install and what's already on your machine with the same fluency as brew, but without needing the publisher to ship a brew formula. Sweep also helps you make informed decisions about what you're about to install by reading the script and surfacing what's about to happen, as well as any potential red flags.

## The CLI

```
sweep "curl … | sh"     install (with LLM analysis if configured)
sweep                   interactive: paste, type, or pull from clipboard
sweep installed         list everything sweep installed
sweep update [name]     update one tool, or all
sweep away <name>       uninstall (best-effort)
sweep show <name>       saved script + last analysis
sweep track <url>       adopt and track an install you ran before sweep existed
sweep from-history      scan shell history, suggest tools to track
sweep --auto "…"        analyze + run unless analysis blocks (no TTY OK)
```

## First run

A short wizard: pick an LLM provider (or skip — sweep works without one) and drop in a key.

## Install

The paste-and-judge path. The user pastes the publisher's pipeline:

```
sweep "curl -fsSL https://ollama.com/install.sh | sh"
```

Sweep accepts any pipeline shape — sudo-wrapped, process-substitution, `-s -- args`, `bash -c "$(curl …)"`, all of it. There is no hand-rolled grammar; the LLM extracts URL, shell, and forwarded args.

Sweep fetches the script, saves it locally (keyed by content hash), and consults the registry. Popular installers are recognized — the registry returns cached info, including an LLM analysis so the user spends no tokens. Scripts the registry hasn't seen, or versions that have changed since its last record, get a local LLM pass anchored by whatever prior context the registry has. The registry also flags hashes it knows are problematic and can carry a registry-side report; both reach the user even with no local LLM configured.

Alongside the fetch, sweep collects deterministic signals: domain age, hash-DB lookups, sigstore for any binary the script will download. Scripts that fetch other scripts are followed recursively up to a configurable depth (default 5); each fetched script goes through the same registry-first funnel. Binaries the script downloads are fetched and hashed for known-bad lookups but never executed. All of these signals are passed to the LLM as inputs to a single analysis call — the LLM synthesizes them with the script content into one verdict, so it can notice combinations a per-channel display would miss (`olarna.com` + 3-day-old domain + unknown binary hash adds up to something different than any signal alone).

When the user approves, sweep tears down its TUI and `exec`s the locally saved shell file. Sweep records whether the install succeeded.

**`--auto` flag.** Analyze and run unless the analysis blocks. Intended for non-interactive contexts (Dockerfiles, CI).

### Naming

Each software installed has a canonical name as determined by the registry.

## Update

Many `curl | sh` installers double as updaters — re-running the script is the publisher's canonical upgrade path. Sweep makes this a first-class verb: `sweep update ollama`

`sweep update` (no arg) walks every installed tool sequentially.

When the publisher ships a *native* upgrade command (`rustup self update`, `bun upgrade`, `pnpm self-update`), sweep records it on first install and prefers it on subsequent updates — same path the publisher tested. Sweep learns the native command from the registry, from the install script, or by reading the binary's `--help`. Re-running the install script remains the universal fallback.

For installers the LLM classifies as "designed for fresh install only", sweep warns before re-running.

## Uninstall

`sweep away <name>`. Two paths.

**When a native uninstall exists** (`rustup self uninstall`, `omz uninstall`, `pnpm uninstall-self`), sweep detects it through the registry or parsing the help (see Update).

**When no native uninstall exists** (the dominant case: ollama, nvm, bun, deno, mise, starship), sweep falls back to commands derived from the install script's behavior — paths the script wrote, lines it appended to dotfiles, services it enabled. This path is **best-effort** and sweep is loud about it. The disciplines:

- **Dry-run by default.** `sweep away` enumerates what would be removed and waits for explicit confirmation.
- **Existence + mtime checks.** Paths that don't exist are silently skipped; paths with mtimes wildly newer than the install date prompt before removal.
- **Dotfile lines matched verbatim.** Appended shell-init lines are stored byte-for-byte and removed only if still present unmodified. Edited lines surface both versions and ask.

Sweep does not observe the filesystem during install. Tools that create runtime files (`~/.ollama/config`, `~/.wrap`) leave orphans on uninstall. The contract for tools that do not offer a native uninstaller is: best-effort, loud about incompleteness.

## Adopting existing installs

`sweep track <url>` registers a tool you installed before sweep existed — the registry resolves the URL to a canonical slug and the tool joins your `installed` list. `sweep from-history` reads your shell history to find past `curl … | sh` invocations and suggests candidates to track in bulk.

## The install dialog

Sweep's analysis surfaces in a TUI dialog. Sweep surfaces signals so the user can decide; it does not deliver verdicts on the user's behalf. There is no "all clear" green state — the dialog reads neutral when no concerning signals are present and warning when they are. Absence of red flags is not endorsement.

Two states:

**Allow.** No concerning signals. Soft LLM-prose verdict at the top. Signal panel below — `✓` known-positive, `◯` neutral. Behavior list framed *"things this script appears to do (not exhaustive)"* — sweep does not claim totality. Buttons: `[Cancel] [Run] [Read source]`. Cancel is the default focus.

**Block.** Concerning signals present. Risk pill at the top. Concerning signals (`⚠`, `✗`) listed first; neutral signals below. Behavior list as before. **No Run button** — only `[Cancel]` and `[Read source]`. To run anyway, the user invokes the install themselves outside sweep.

When sweep can compare today's script against a known prior version, the dialog adds a changes panel — new URLs, replaced binaries, modified install paths since the last seen version, elevated permission requirements. This comes from whichever source has it: the local store (the user has sweep'd this script before) or the registry (someone else has, and the registry has been tracking the script's history). Local history covers re-encounters; registry history covers first-time installs of scripts with a public track record.

The verdict is LLM-generated prose, not a rule-derived label. The LLM is what notices that `ollarna.com` is trying to look like `ollama.com`. Local rules cannot reliably do that, and a deterministic rule engine is explicitly not part of sweep.

## Without an LLM

Sweep is useful with no LLM configured. The first-run wizard makes the choice explicit. In LLM-less mode sweep is plumbing — it fetches scripts, runs the deterministic signal lookups (domain age, hash databases), and runs the install when approved. Registry signals still surface: cached verdicts on popular installers, problematic-hash flags, and any registry-side report carry through, so users without a local LLM still get benefit on anything the registry already knows.

## The local store

Everything sweep does is backed by a local store. It holds every install script sweep has ever fetched (keyed by content hash, so identical scripts deduplicate), every recursively fetched script, every binary hash, the recorded uninstall steps for each install, and a log of every encounter — what was fetched, when, from where, the install outcome (ran, cancelled, errored), and how it was identified.

The log is the source of truth for `sweep installed`.

Versioning is best-effort: when the LLM extracts an explicit version from the script (`VERSION=1.4.2`), sweep records it. Otherwise the install is identified by content hash.

## The registry

`registry.sweep.tld` — a centralized service sweep consults to:

- **Canonicalize.** Map an install URL to a canonical slug, when one is known. Prevents two installs of the same tool from drifting to different handles.
- **Track URL evolution.** When publishers move install URLs, the registry knows the redirect; `sweep update foo` knows where to get the latest script from.
- **Cache LLM verdicts.** Async server-side LLM analysis on popular installers. Users hitting a known installer get the verdict instantly without spending their own LLM tokens. Run on installers above a popularity threshold;
- **Surface change history.** *"This script changed twice in the last 30 days; the change on 2026-04-12 added a new binary URL."* Cross-time signal sweep cannot build alone.
- Log native upgrade/uninstall commands

**Curated, not crowdsourced.** Verdicts, reports, and behavioral notes in the registry come from registry-side scanning and verification only. Local LLM analysis, signal results, and per-user decisions never flow back. The registry receives install URLs (and maybe content hashes) so it can canonicalize and track aggregate change history; it does not receive what individual users found.

**Privacy footprint:** each request sends install URL (and maybe content hash). No personally identifiable data is stored.

## What sweep catches

The signals available. Most signals are negative — they flag known-bad. Absence is not proof of safety.

- **Domain age.** WHOIS + Certificate Transparency (`crt.sh`). Free, keyless, typosquat signal.
- **Hash lookups.** CIRCL hashlookup, Team Cymru MHR, Sigstore — all keyless, all GET/DNS. Strong negative signal when they hit; neutral when they miss.
- **Diff vs prior.** a strong signal once a script has history. Catches Codecov-class compromises in the upgrade path.
- **Registry signals.** Canonical slug, URL evolution, change history when the installer is in the registry.
- **LLM behavioral analysis.** What the script does, where it fetches from, whether it runs sudo. Probabilistic. Catches most of what a careful developer would notice on read.
- **Two-pass injection defense.** A separate LLM pass asks "is this script trying to manipulate a reviewer?" The two passes run in parallel; sweep only trusts the analysis if the manipulation pass came back clean.
- **LLM verdict prose.** Soft natural-language assessment. Catches things rules cannot — typosquats, suspicious framing, oddly-formed code.

What sweep does **not** catch: novel attacks not in any DB and not LLM-obvious; runtime behavior after install completes; changes done by executables; sophisticated attackers who understand the LLM's blind spots.

## Future ideas

- **`sweep export` / `sweep import`.** A Brewfile equivalent for `curl | sh` installs. Export produces a JSON manifest; import re-runs each install through sweep on a new machine.
- **In-script header standard.** Open Graph for installers. Publishers embed a YAML/JSON block at the top of their install script in a comment block.
- **Sandbox detonation and fs-watch during install.** Run installers in a container, observe behavior and record filesystem writes. Most of the value flows through the registry running this on popular installers and caching the result.
- **source viewer** - Syntax highlighted and annotated source installer source viewer.
- **URL sanitization before registry submission.** Redact non-public URL shapes (presigned links, tokens in path or userinfo, internal hosts) client-side; fall back to hash-only when the URL looks non-public.
- **`--no-registry` flag and config.** Disable all registry calls for users who don't want any metadata flow.
- **Signed registry records.** Per-record signatures so registry tampering is detectable by clients.
