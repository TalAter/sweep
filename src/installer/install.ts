/**
 * Install orchestrator (post-flip). `runInstall` no longer fetches or analyzes
 * itself: fetch + two-pass analysis + the approval gate all live INSIDE the
 * step-5 alt-screen session (`tui/install-session.ts`). This module drives that
 * session and maps the `InstallDecision` it resolves to DB rows + the CAS save
 * + exec + an exit code.
 *
 * Invariant: every run writes exactly ONE row to `invocations`, distinguished
 * by `outcome` — EXCEPT the interactive paste-cancel case, where no command was
 * ever committed (no command ⇒ no "run", consistent with invariant 3).
 *
 * Exit codes (returned, not thrown):
 *   - `2`   pre-session parse failure (direct mode only)
 *   - `1`   fetch failure
 *   - `130` cancel/decline of a committed command (non-zero, outside the fixed
 *           parse(2)/fetch(1) codes; exec passes arbitrary codes through, so a
 *           collision with those isn't worth chasing — 130 is the working default)
 *   - `0`   interactive paste cancelled before any command was committed
 *   - exec exit code passthrough on a run (0 on success, non-zero on failure)
 *
 * `tsStarted` is captured at function entry — BEFORE parse — so even a
 * parse_failed row reflects when the user actually invoked sweep.
 *
 * Session seam: `defaultRunSession` does the real I/O + alt-screen mount and is
 * the only place the TUI/Ink stack loads (via dynamic import), so subcommands
 * like `sweep list` never pay the Ink cost. Tests inject `deps.runSession` to
 * supply a canned decision and skip the real fetch + mount entirely.
 */

import { resolveAppearance, resolveTheme, setTheme } from "wrap-core/theme";
import { sweepFs } from "../fs.ts";
import { slugFromUrl } from "../identity/naming.ts";
import { ensureDb } from "../store/db.ts";
import { insertInvocation } from "../store/invocations.ts";
import { findOrCreatePackage, updatePackageOnExec } from "../store/packages.ts";
import { saveScript } from "../store/scripts.ts";
// TYPE-ONLY imports (erased at runtime — they do NOT pull the TUI/Ink). The
// real session module is loaded lazily inside `defaultRunSession`.
import type { InstallDecision, SessionStart } from "../tui/install-session.ts";
import { DARK_GRADIENT, LIGHT_GRADIENT } from "../tui/theme.ts";
import { runScript } from "./exec.ts";
import { parseInstallCommand } from "./parse.ts";

export type RunInstallInput = { kind: "interactive" } | { kind: "direct"; raw: string };
export type SessionRunner = (start: SessionStart) => Promise<InstallDecision>;
export type RunInstallDeps = { runSession?: SessionRunner };

export async function runInstall(
  input: RunInstallInput,
  deps: RunInstallDeps = {},
): Promise<number> {
  const tsStarted = new Date().toISOString();
  const runSession = deps.runSession ?? defaultRunSession;

  // ---- Build the session start --------------------------------------------
  let start: SessionStart;
  if (input.kind === "direct") {
    // Direct mode parses the argument up front. A parse failure is a
    // PRE-session failure: no alt-screen is mounted, so it keeps today's
    // stderr chrome + exit 2.
    const parsed = parseInstallCommand(input.raw);
    if ("kind" in parsed) {
      const tsFinished = new Date().toISOString();
      console.error(`sweep: ${parsed.message}`);
      insertInvocation({
        packageId: null,
        tsStarted,
        tsFinished,
        rawInput: input.raw,
        url: null,
        sha256: null,
        installCommandJson: null,
        outcome: "parse_failed",
        exitCode: null,
        errorMessage: parsed.message,
      });
      return 2;
    }
    start = { kind: "direct", raw: input.raw, parsed };
  } else {
    // Interactive mode: parse happens INSIDE the session on paste.
    start = { kind: "interactive" };
  }

  // ---- Drive the session --------------------------------------------------
  const decision = await runSession(start);

  // ---- Map the decision to rows + exec + exit code ------------------------
  switch (decision.kind) {
    case "run": {
      const { raw, parsed, fetched } = decision;
      saveScript(fetched.sha256, fetched.bytes);
      const pkg = findOrCreatePackage({ url: parsed.url, slug: slugFromUrl(parsed.url) });
      const { exitCode } = await runScript(parsed, fetched.bytes);

      // Invocation row + package status flip commit as one unit (unchanged).
      const tsFinished = new Date().toISOString();
      ensureDb().transaction(() => {
        insertInvocation({
          packageId: pkg.id,
          tsStarted,
          tsFinished,
          rawInput: raw,
          url: parsed.url,
          sha256: fetched.sha256,
          installCommandJson: JSON.stringify(parsed),
          outcome: exitCode === 0 ? "ran" : "errored",
          exitCode,
          errorMessage: null,
        });
        updatePackageOnExec({
          packageId: pkg.id,
          sha256: fetched.sha256,
          exitCode,
          ranAt: tsFinished,
        });
      })();

      return exitCode;
    }

    case "fetch-failed": {
      const { raw, parsed, error } = decision;
      const tsFinished = new Date().toISOString();
      console.error(`sweep: ${error.message}`);
      insertInvocation({
        packageId: null,
        tsStarted,
        tsFinished,
        rawInput: raw,
        url: parsed.url,
        sha256: null,
        installCommandJson: JSON.stringify(parsed),
        outcome: "fetch_failed",
        exitCode: null,
        errorMessage: error.message,
      });
      return 1;
    }

    case "cancel": {
      const { raw, parsed, fetched } = decision;
      // Interactive paste cancelled before any command was committed: no
      // command was ever produced, so no row and a clean exit (preserves the
      // old `if (!command) return;` behavior and invariant 3).
      if (raw === null) return 0;

      // A command WAS committed, then declined (during loading or once
      // resolved). Log the cancelled run only — package-state-after-cancel is
      // deferred (no CAS save, no package row, no lifecycle transition).
      const tsFinished = new Date().toISOString();
      insertInvocation({
        packageId: null,
        tsStarted,
        tsFinished,
        rawInput: raw,
        url: parsed?.url ?? null,
        sha256: fetched?.sha256 ?? null,
        installCommandJson: parsed ? JSON.stringify(parsed) : null,
        outcome: "cancelled",
        exitCode: null,
        errorMessage: null,
      });
      return 130;
    }
  }
}

/**
 * Real session wiring (used when no `deps.runSession` is injected). Resolves
 * appearance/theme (Ink-free) up front, then DYNAMICALLY imports the TUI stack
 * so the Ink dependency loads only on the install path — `sweep list` and other
 * subcommands never touch it.
 *
 * Honors the two step-5 caller contracts: `setTheme(theme)` before the session
 * (severity presets read the module-global theme) and a `gradientStops`
 * consistent with the resolved theme; `preloadDialogRuntime()` awaited before
 * `runInstallSession` (`renderDialog` throws otherwise).
 */
async function defaultRunSession(start: SessionStart): Promise<InstallDecision> {
  const appearance = await resolveAppearance({ envVarName: "SWEEP_THEME", fs: sweepFs });
  const theme = resolveTheme(appearance);
  setTheme(theme);
  const gradientStops = appearance === "light" ? LIGHT_GRADIENT : DARK_GRADIENT;

  const { runInstallSession } = await import("../tui/install-session.ts");
  const { preloadDialogRuntime } = await import("wrap-core/tui");
  await preloadDialogRuntime();

  return runInstallSession({ start, gradientStops, theme, nerdFonts: false });
}
