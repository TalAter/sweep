# Sweep — To Do

All remaining implementation tasks and future ideas. Completed features are omitted — see `vault/` notes for architecture reference.

---

## Analysis Dialog

- [ ] "Configure a provider" button on the no-LLM dialog state. The analysis dialog now ships, and its no-LLM state shows "No LLM provider configured — no analysis to show." with a plain `[Run]` — a dead end for a user who'd actually like analysis. Add a button to the bottom action bar that jumps to the config / first-run wizard so a provider can be set up without leaving the install flow. Now blocked only on the config wizard (see [[product-spec]] "First run"); the dialog half is built.

## Config

- [ ] Wire `nerdFonts` from config into the install dialog (hardcoded `false` today).
