/**
 * Step 5 of the LLM-insights feature: the install SESSION controller. It owns ONE
 * alt-screen session that transitions paste → loading → resolved, runs
 * `fetchScript` + `analyzeScript` beneath that one spinner, and resolves to an
 * `InstallDecision` the orchestrator (step 6) acts on. It does NOT touch the DB,
 * the CAS, or exec — it only drives the TUI and the two async ops, then hands a
 * decision back.
 *
 * Why a session and not `mount.ts`'s `openDialog`: a single alt-screen must span
 * paste → fetch → analyze → approve. `openDialog` is open-await-one-close; here
 * we mount once and `rerender()` different elements in place over the same alt
 * buffer (the paste dialog literally BECOMES the insights dialog), tearing down
 * exactly once on settle.
 *
 * The four Step-4 contracts this controller honors:
 *   - Ctrl+C → onCancel: `renderDialog` mounts with `exitOnCtrlC:false`, so the
 *     component's Ctrl+C binding routes through `onCancel` instead of killing the
 *     process. (Esc / the Cancel button do the same.)
 *   - Focus reset across loading→resolved: the two `InsightDialog` renders carry
 *     DIFFERENT React `key`s ("loading" vs "resolved"), so the resolved phase
 *     REMOUNTS the component, resetting its `confirmValue`/`focusedIndex`
 *     (focus defaults back to Cancel).
 *   - No `onRun` double-fire: `onRun` runs `settle`, which `unmount()`s
 *     synchronously and is idempotent (first call wins), so a second Enter after
 *     run can't re-fire.
 *   - Inline parse error: a pasted command that fails to parse re-renders the
 *     paste dialog with `error=<message>` — it never advances and never prints a
 *     `sweep:` stderr line that the alt-screen would hide.
 *
 * Cancel-wins-over-abort-induced-failure: cancelling aborts the in-flight fetch
 * (which rejects) and `analyzeScript` (which maps the abort to `analysis.failed`
 * INSIDE itself — it never throws). A `settled` guard checked after every await
 * means a cancel that aborted the passes resolves to the CANCEL decision and the
 * abort-as-failed result is dropped on the floor — the resolved analysis-failed
 * view never flashes.
 */

import { createElement, type ReactElement } from "react";
import type { Color } from "wrap-core/ansi";
import type { CoreThemeTokens } from "wrap-core/theme";
import { type RenderedDialog, renderDialog } from "wrap-core/tui";
import { type AnalysisResult, analyzeScript as realAnalyzeScript } from "../installer/analyze.ts";
import {
  type FetchedScript,
  type FetchScriptError,
  fetchScript as realFetchScript,
} from "../installer/fetch.ts";
import type { InstallCommand } from "../installer/parse.ts";
import { parseInstallCommand } from "../installer/parse.ts";
import { InsightDialog } from "./insight-dialog.tsx";
import { deriveInsightView, deriveSource } from "./insight-view.ts";
import { InteractiveDialog } from "./interactive.tsx";

/** What the orchestrator (step 6) acts on. The fetched script flows out so the
 *  orchestrator can save it to the CAS and exec it without re-fetching. */
export type InstallDecision =
  | { kind: "run"; raw: string; parsed: InstallCommand; fetched: FetchedScript }
  | {
      kind: "cancel";
      raw: string | null;
      parsed: InstallCommand | null;
      fetched: FetchedScript | null;
    }
  | { kind: "fetch-failed"; raw: string; parsed: InstallCommand; error: FetchScriptError };

/** Where a session begins. Shared with the orchestrator (step 6): in direct
 *  mode the orchestrator parses the argument up front and hands the parsed
 *  command in; in interactive mode parse happens inside the session on paste. */
export type SessionStart =
  | { kind: "interactive" }
  | { kind: "direct"; raw: string; parsed: InstallCommand };

export type RunInstallSessionOpts = {
  start: SessionStart;
  // Caller owns appearance resolution: must `setTheme(theme)` first (severity
  // presets read the module-global theme via `getTheme()`) and keep
  // `gradientStops` consistent with `theme`.
  gradientStops: Color[];
  theme: CoreThemeTokens;
  nerdFonts: boolean;
  deps?: {
    mount?: (el: ReactElement) => RenderedDialog;
    fetchScript?: (url: string, o?: { signal?: AbortSignal }) => Promise<FetchedScript>;
    analyzeScript?: typeof realAnalyzeScript;
  };
};

export async function runInstallSession(opts: RunInstallSessionOpts): Promise<InstallDecision> {
  const { start, gradientStops, theme, nerdFonts } = opts;
  const fetchScript = opts.deps?.fetchScript ?? realFetchScript;
  const analyzeScript = opts.deps?.analyzeScript ?? realAnalyzeScript;
  // `renderDialog` throws unless `preloadDialogRuntime()` has resolved, so the
  // (step 6) caller must `await preloadDialogRuntime()` before `runInstallSession`
  // (tests inject `deps.mount`, so they don't need it).
  const mount: (el: ReactElement) => RenderedDialog =
    opts.deps?.mount ?? ((el) => renderDialog(el, { theme, nerdFonts }));

  return new Promise<InstallDecision>((resolve) => {
    // The async chain's cancellation handle. Created when loading begins.
    let abort: AbortController | null = null;
    // Whatever has been fetched so far (for the cancel-with-fetched-present case).
    let fetched: FetchedScript | null = null;
    // The committed command + parse once a paste succeeds (or the direct start).
    let raw: string | null = start.kind === "direct" ? start.raw : null;
    let parsed: InstallCommand | null = start.kind === "direct" ? start.parsed : null;

    let settled = false;
    let session: RenderedDialog;

    /** Terminal path. First call wins; unmounts exactly once, synchronously. */
    const settle = (decision: InstallDecision) => {
      if (settled) return;
      settled = true;
      session.unmount();
      resolve(decision);
    };

    const onCancel = () => {
      // Abort first so the in-flight fetch/analyze stop; the chain's `settled`
      // guard then drops their (abort-induced) results.
      abort?.abort();
      settle({ kind: "cancel", raw, parsed, fetched });
    };

    const onRun = () => {
      // Only reachable in the resolved phase; raw+parsed+fetched are all set.
      if (raw === null || parsed === null || fetched === null) return;
      settle({ kind: "run", raw, parsed, fetched });
    };

    const renderPaste = (error?: string) =>
      session.rerender(
        createElement(InteractiveDialog, {
          gradientStops,
          onSubmit: onPasteSubmit,
          onCancel,
          error,
        }),
      );

    const onPasteSubmit = (command: string) => {
      const result = parseInstallCommand(command);
      if ("kind" in result) {
        // Parse failure: stay on the paste input, surface the message inline.
        renderPaste(result.message);
        return;
      }
      raw = command;
      parsed = result;
      // Coming from paste: swap the live paste dialog to the spinner, then run.
      session.rerender(loadingElement(deriveSource(result.url)));
      runFetchAndAnalyze(result, command);
    };

    /** The loading-phase element. `key:"loading"` (vs "resolved") forces a
     *  remount on the phase change so the resolved view's focus resets. `source`
     *  is derived by the caller, which holds the committed (non-null) command. */
    const loadingElement = (source: string): ReactElement =>
      createElement(InsightDialog, {
        key: "loading",
        state: { phase: "loading", source },
        neutralGradient: gradientStops,
        onRun,
        onCancel,
      });

    /** Create the abort handle and run fetch → analyze beneath the spinner. Takes
     *  the committed (non-null) command + raw so it never has to re-derive them
     *  through casts. The caller must already have rendered (or mounted) loading. */
    const runFetchAndAnalyze = (cmd: InstallCommand, rawCmd: string) => {
      const url = cmd.url;
      abort = new AbortController();
      const signal = abort.signal;

      void (async () => {
        let result: FetchedScript;
        try {
          result = await fetchScript(url, { signal });
        } catch (err) {
          // A cancel aborted the fetch → drop the (timeout-classified) error.
          if (settled || signal.aborted) return;
          settle({
            kind: "fetch-failed",
            raw: rawCmd,
            parsed: cmd,
            error: err as FetchScriptError,
          });
          return;
        }
        if (settled || signal.aborted) return;
        fetched = result;

        const analysis: AnalysisResult = await analyzeScript({
          url,
          finalUrl: result.finalUrl,
          scriptBytes: result.bytes,
          signal,
        });
        // Cancel wins: a cancel that aborted analyze maps to analysis-failed
        // INSIDE analyzeScript; the guard drops it so we never flash that view.
        if (settled || signal.aborted) return;

        const view = deriveInsightView(analysis, url);
        session.rerender(
          createElement(InsightDialog, {
            key: "resolved",
            state: { phase: "resolved", view },
            neutralGradient: gradientStops,
            onRun,
            onCancel,
          }),
        );
      })();
    };

    // Mount once with the appropriate first element, then drive by rerender.
    //   interactive → the paste dialog (rerenders into loading on submit)
    //   direct      → straight to the loading spinner (parse already happened)
    const firstElement: ReactElement =
      start.kind === "interactive"
        ? createElement(InteractiveDialog, {
            gradientStops,
            onSubmit: onPasteSubmit,
            onCancel,
          })
        : loadingElement(deriveSource(start.parsed.url));

    session = mount(firstElement);

    // Direct mode skips paste: the loading element is already mounted, so just
    // start the async chain (one render, no churn).
    if (start.kind === "direct") runFetchAndAnalyze(start.parsed, start.raw);
  });
}
