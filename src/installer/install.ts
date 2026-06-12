/**
 * Install orchestrator. Invariant: every code path — including parse and
 * fetch failures — writes exactly one row to `invocations`, distinguished
 * by `outcome`.
 *
 * Exit codes (returned, not thrown):
 *   - `2` parse failure
 *   - `1` fetch failure
 *   - exec exit code passthrough otherwise (0 on success, non-zero on
 *     script failure; -1 if the child never reported one).
 *
 * `tsStarted` is captured at function entry — BEFORE parse — so that even
 * a parse_failed row reflects when the user actually invoked sweep, not
 * when we got around to writing the row.
 */

import { slugFromUrl } from "../identity/naming.ts";
import { ensureDb } from "../store/db.ts";
import { insertInvocation } from "../store/invocations.ts";
import { findOrCreatePackage, updatePackageOnExec } from "../store/packages.ts";
import { saveScript } from "../store/scripts.ts";
import { maybeAnalyzeScript } from "./analyze.ts";
import { runScript } from "./exec.ts";
import { FetchScriptError, fetchScript } from "./fetch.ts";
import { parseInstallCommand } from "./parse.ts";

export async function runInstall(raw: string): Promise<number> {
  const tsStarted = new Date().toISOString();

  // ---- 1. Parse -----------------------------------------------------------
  const parsed = parseInstallCommand(raw);
  if ("kind" in parsed) {
    const tsFinished = new Date().toISOString();
    console.error(`sweep: ${parsed.message}`);
    insertInvocation({
      packageId: null,
      tsStarted,
      tsFinished,
      rawInput: raw,
      url: null,
      sha256: null,
      installCommandJson: null,
      outcome: "parse_failed",
      exitCode: null,
      errorMessage: parsed.message,
    });
    return 2;
  }

  const installCommandJson = JSON.stringify(parsed);

  // ---- 2. Fetch -----------------------------------------------------------
  let fetched: Awaited<ReturnType<typeof fetchScript>>;
  try {
    fetched = await fetchScript(parsed.url);
  } catch (err) {
    const tsFinished = new Date().toISOString();
    const message = err instanceof FetchScriptError ? err.message : String(err);
    console.error(`sweep: ${message}`);
    insertInvocation({
      packageId: null,
      tsStarted,
      tsFinished,
      rawInput: raw,
      url: parsed.url,
      sha256: null,
      installCommandJson,
      outcome: "fetch_failed",
      exitCode: null,
      errorMessage: message,
    });
    return 1;
  }

  // ---- 3. Save CAS --------------------------------------------------------
  saveScript(fetched.sha256, fetched.bytes);

  // ---- 4. Resolve package -------------------------------------------------
  const pkg = findOrCreatePackage({ url: parsed.url, slug: slugFromUrl(parsed.url) });

  // ---- 5. Analysis (env-gated; no-op without sweep's test contract) -------
  await maybeAnalyzeScript({ url: parsed.url, scriptBytes: fetched.bytes });

  // ---- 6. Exec ------------------------------------------------------------
  const { exitCode } = await runScript(parsed, fetched.bytes);

  // ---- 7. Persist outcome (single transaction) ----------------------------
  const tsFinished = new Date().toISOString();
  ensureDb().transaction(() => {
    insertInvocation({
      packageId: pkg.id,
      tsStarted,
      tsFinished,
      rawInput: raw,
      url: parsed.url,
      sha256: fetched.sha256,
      installCommandJson,
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

  // ---- 8. Return exec exit code ------------------------------------------
  return exitCode;
}
