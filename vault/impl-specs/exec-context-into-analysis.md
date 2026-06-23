---
name: exec-context-into-analysis
description: Feed the literal install command (secrets redacted in place) into LLM analysis, so it sees the sudo/shell/args/env the user typed.
---

# Exec-context-aware analysis

Today the analysis prompt sees only the script bytes + provenance (url, finalUrl), not the command wrapping the fetch. Feed it the **literal command** the user typed or pasted — secrets redacted in place — with a short label, alongside the existing provenance line so both the analysis and manipulation passes get it:

> Command the user is about to run: `curl -fsSL https://…/install.sh | API_TOKEN=<redacted> sudo -E sh`

Don't paraphrase it into prose — the model reads shell natively; a parsed restatement only adds a lossy layer that can disagree with the command. This is command-visibility, a partial proxy for the run, **not** the machine's runtime env; claim no more than the command shows. Weak models miss that piping into `sudo` runs the *whole* script as root — keep a one-line note in the system prompt to that effect (so per-behavior severity reflects it).

## Redaction

Blank secret values in the command before it goes in the prompt — they carry ~zero verdict signal and would ship the user's own secret to the configured provider.

> A value is secret when its env-var **name** or arg **flag** contains (case-insensitive) `KEY`, `TOKEN`, `SECRET`, `PASS`, `AUTH`, or `CRED`. Replace the value with `<redacted>`; leave the name/flag and everything else intact.

Use the parsed `InstallCommand` to locate them (`envVars` is a `name → value` map; `scriptArgs` a flat, whitespace-split token array), then blank each secret value where it appears in the literal command — match on the name/flag, never the value, and mind quoting. Build the redacted string once; raw secret values never reach the prompt.

The name list misses short flags (`-t`), odd names (`GITHUB_PAT`), and URL/positional secrets — accepted: it's the user's own secret going to a provider they chose. Value-shape/entropy detection deferred.

## Don't

Machine-tailoring — probing the user's OS/arch/libc/etc. and feeding it to suppress "non-applicable" warnings — was rejected: it inverts the threat model (hands an attacker a branch-gating lever the manipulation pass can't see), breaks the shareable cache, and pushes the tool from *showing* toward *vouching*. Exec-context from the *command* is the safe version that survived.

## Build

TDD, failing tests first. Lots of coverage for various ways to do redaction. A whole file just for redaction maybe. Analysis still never throws, never fails the install, never asserts safe. The matching cache-key change (sha + raw command) lives in [[script-inspection]], built later with the registry.
