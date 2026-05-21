import { ensureDb } from "./db.ts";

export type Outcome = "ran" | "cancelled" | "errored" | "parse_failed" | "fetch_failed";

export type Invocation = {
  id: string; // crypto.randomUUID()
  packageId: number | null;
  tsStarted: string; // ISO 8601
  tsFinished: string | null;
  rawInput: string;
  url: string | null;
  sha256: string | null;
  installCommandJson: string | null;
  outcome: Outcome;
  exitCode: number | null;
  errorMessage: string | null;
};

/**
 * Insert-only writer for the `invocations` table. Fire-and-forget: single
 * INSERT, no follow-up SELECT — we already know every field that hit the row.
 *
 * `id` defaults to a fresh v4 UUID; an explicit id is honored verbatim
 * (useful for tests and for callers that want to thread a known id around).
 *
 * Does not open its own transaction. `install.ts` wraps this together with
 * `updatePackageOnExec` in one `db.transaction()` so the invocation row and
 * the package status flip commit as a unit.
 */
export function insertInvocation(args: Omit<Invocation, "id"> & { id?: string }): Invocation {
  const inv: Invocation = {
    id: args.id ?? crypto.randomUUID(),
    packageId: args.packageId,
    tsStarted: args.tsStarted,
    tsFinished: args.tsFinished,
    rawInput: args.rawInput,
    url: args.url,
    sha256: args.sha256,
    installCommandJson: args.installCommandJson,
    outcome: args.outcome,
    exitCode: args.exitCode,
    errorMessage: args.errorMessage,
  };
  ensureDb().run(
    `INSERT INTO invocations
       (id, package_id, ts_started, ts_finished, raw_input, url, sha256,
        install_command_json, outcome, exit_code, error_message)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      inv.id,
      inv.packageId,
      inv.tsStarted,
      inv.tsFinished,
      inv.rawInput,
      inv.url,
      inv.sha256,
      inv.installCommandJson,
      inv.outcome,
      inv.exitCode,
      inv.errorMessage,
    ],
  );
  return inv;
}
