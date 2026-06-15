/**
 * Integration tests for `runInstall` — the install orchestrator after the flip
 * (step 6). `runInstall` no longer fetches or analyzes itself; it drives an
 * injectable SESSION (the step-5 controller) and maps the session's
 * `InstallDecision` to DB rows + CAS save + exec + exit codes.
 *
 * No network and no alt-screen: tests inject `runSession` to supply a canned
 * decision (the bytes/outcome they want), so the real fetch + Ink stack never
 * runs. The decision echoes back the `raw`/`parsed` that runInstall already
 * parsed (direct mode), injecting only the fetched bytes/outcome. We still
 * drive the real exec path (spawn) so DB writes, CAS save, package status
 * transitions, and the transactional commit are exercised for real.
 *
 * `console.error` is silenced per test; the spy lets error paths assert they
 * printed SOMETHING (we don't pin message format).
 *
 * `process.env.SWEEP_HOME` is pinned by `tests/sweep-home-preload.ts`, and the
 * DB handle + temp home are reset between tests there — so each test starts
 * with empty `packages` and `invocations` tables.
 */
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import type { FetchedScript } from "../src/installer/fetch.ts";
import { FetchScriptError } from "../src/installer/fetch.ts";
import { type RunInstallDeps, runInstall } from "../src/installer/install.ts";
import type { InstallCommand } from "../src/installer/parse.ts";
import { ensureDb } from "../src/store/db.ts";
import { readScript } from "../src/store/scripts.ts";
import type { SessionStart } from "../src/tui/install-session.ts";

// ---- console.error silencer ----------------------------------------------

const errSpy = mock(() => {});
const origConsoleError = console.error;

beforeEach(() => {
  console.error = errSpy;
  errSpy.mockClear();
});

afterEach(() => {
  console.error = origConsoleError;
});

// ---- helpers -------------------------------------------------------------

/** A direct install command targeting a stable host (slug "localhost"). */
const installCmd = (path: string, shell = "sh"): string =>
  `curl -fsSL https://localhost${path} | ${shell}`;

const sourceUrl = (path: string): string => `https://localhost${path}`;

const bytes = (s: string): Uint8Array => new TextEncoder().encode(s);

/** Independent sha256 of bytes — proves install stored the right CAS slot. */
const sha256Hex = (b: Uint8Array): string => {
  const h = new Bun.CryptoHasher("sha256");
  h.update(b);
  return h.digest("hex");
};

/** Build a `FetchedScript` from raw bytes (what a successful fetch would yield). */
const fetchedFor = (b: Uint8Array): FetchedScript => ({
  bytes: b,
  sha256: sha256Hex(b),
  finalUrl: "https://localhost/resolved",
  fetchedAt: new Date().toISOString(),
  status: 200,
});

/** Narrow a direct start so we can echo back its committed raw/parsed. */
function directStart(s: SessionStart): { raw: string; parsed: InstallCommand } {
  if (s.kind !== "direct") throw new Error("expected a direct session start");
  return { raw: s.raw, parsed: s.parsed };
}

/** A `runSession` that always approves a run with the given bytes. */
const runWith = (b: Uint8Array): RunInstallDeps => ({
  runSession: async (s: SessionStart) => {
    const { raw, parsed } = directStart(s);
    return { kind: "run", raw, parsed, fetched: fetchedFor(b) };
  },
});

/** All rows from `invocations`, newest first. */
function allInvocations(): Array<Record<string, unknown>> {
  return ensureDb()
    .query("SELECT * FROM invocations ORDER BY ts_started DESC, rowid DESC")
    .all() as Array<Record<string, unknown>>;
}

/** All rows from `packages`. */
function allPackages(): Array<Record<string, unknown>> {
  return ensureDb().query("SELECT * FROM packages").all() as Array<Record<string, unknown>>;
}

/**
 * Assert the table has exactly one row and return it. Lets tests drop both
 * `!` non-null assertions on `[0]` and the separate length check.
 */
function onlyRow<T>(rows: T[]): T {
  if (rows.length !== 1) throw new Error(`expected exactly 1 row, got ${rows.length}`);
  return rows[0] as T;
}

// =========================================================================
// 1. Happy path
// =========================================================================

describe("happy path", () => {
  test("exits 0; persists installed package + ran invocation; CAS written", async () => {
    const body = bytes("echo ok\nexit 0\n");
    const hash = sha256Hex(body);

    const exitCode = await runInstall(
      { kind: "direct", raw: installCmd("/install.sh") },
      runWith(body),
    );
    expect(exitCode).toBe(0);

    const pkg = onlyRow(allPackages());
    expect(pkg.status).toBe("installed");
    expect(pkg.slug).toBe("localhost");
    expect(pkg.source_url).toBe(sourceUrl("/install.sh"));
    expect(pkg.current_sha256).toBe(hash);
    expect(pkg.installed_at).toBeTruthy();
    expect(pkg.last_ran_at).toBeTruthy();

    const inv = onlyRow(allInvocations());
    expect(inv.outcome).toBe("ran");
    expect(inv.exit_code).toBe(0);
    expect(inv.package_id).toBe(pkg.id);
    expect(inv.url).toBe(sourceUrl("/install.sh"));
    // The post-redirect origin the bytes were served from (from the fetch).
    expect(inv.final_url).toBe("https://localhost/resolved");
    expect(inv.sha256).toBe(hash);
    expect(inv.install_command_json).toBeTruthy();
    expect(inv.raw_input).toBe(installCmd("/install.sh"));
    expect(inv.ts_started).toBeTruthy();
    expect(inv.ts_finished).toBeTruthy();
    expect(inv.error_message).toBeNull();

    // CAS slot got written by saveScript.
    expect(readScript(hash)).not.toBeNull();
  });
});

// =========================================================================
// 2. Non-zero exit
// =========================================================================

describe("non-zero exit", () => {
  test("returns the exec exit code; package → failed; invocation outcome=errored", async () => {
    const body = bytes("exit 7\n");

    const exitCode = await runInstall(
      { kind: "direct", raw: installCmd("/bad.sh") },
      runWith(body),
    );
    expect(exitCode).toBe(7);

    const pkg = onlyRow(allPackages());
    expect(pkg.status).toBe("failed");
    // current_sha256 stays null on a fresh attempting→failed transition.
    expect(pkg.current_sha256).toBeNull();

    const inv = onlyRow(allInvocations());
    expect(inv.outcome).toBe("errored");
    expect(inv.exit_code).toBe(7);
  });
});

// =========================================================================
// 3. Parse failure (pre-session — no session is ever mounted)
// =========================================================================

describe("parse failure", () => {
  test("returns 2; one parse_failed invocation; no packages row; no session", async () => {
    let sessionCalled = false;
    const exitCode = await runInstall(
      { kind: "direct", raw: "not a real command" },
      {
        runSession: async () => {
          sessionCalled = true;
          throw new Error("session must not run on a pre-session parse failure");
        },
      },
    );
    expect(exitCode).toBe(2);
    expect(sessionCalled).toBe(false);

    const inv = onlyRow(allInvocations());
    expect(inv.outcome).toBe("parse_failed");
    expect(inv.package_id).toBeNull();
    expect(inv.url).toBeNull();
    expect(inv.sha256).toBeNull();
    expect(inv.install_command_json).toBeNull();
    expect(inv.exit_code).toBeNull();
    expect(inv.error_message).toBeTruthy();
    expect(inv.raw_input).toBe("not a real command");
    expect(inv.ts_started).toBeTruthy();
    expect(inv.ts_finished).toBeTruthy();

    expect(allPackages()).toHaveLength(0);
    expect(errSpy).toHaveBeenCalled();
  });
});

// =========================================================================
// 4. Fetch failure (session resolved fetch-failed)
// =========================================================================

describe("fetch failure", () => {
  test("returns 1; one fetch_failed invocation; no packages row", async () => {
    const exitCode = await runInstall(
      { kind: "direct", raw: installCmd("/missing.sh") },
      {
        runSession: async (s) => {
          const { raw, parsed } = directStart(s);
          return {
            kind: "fetch-failed",
            raw,
            parsed,
            error: new FetchScriptError(
              "non-2xx",
              `HTTP 404 Not Found: ${sourceUrl("/missing.sh")}`,
            ),
          };
        },
      },
    );
    expect(exitCode).toBe(1);

    const inv = onlyRow(allInvocations());
    expect(inv.outcome).toBe("fetch_failed");
    expect(inv.package_id).toBeNull();
    expect(inv.url).toBe(sourceUrl("/missing.sh"));
    // No fetch completed → no served-from origin to record.
    expect(inv.final_url).toBeNull();
    expect(inv.sha256).toBeNull();
    expect(inv.install_command_json).toBeTruthy();
    expect(inv.exit_code).toBeNull();
    expect(inv.error_message).toBeTruthy();
    expect(inv.ts_started).toBeTruthy();
    expect(inv.ts_finished).toBeTruthy();

    expect(allPackages()).toHaveLength(0);
    expect(errSpy).toHaveBeenCalled();
  });
});

// =========================================================================
// 5. Cancel — committed command, before fetch completed
// =========================================================================

describe("cancel — committed, pre-fetch", () => {
  test("returns 130; one cancelled invocation with null sha/package; no packages row", async () => {
    const exitCode = await runInstall(
      { kind: "direct", raw: installCmd("/cancelme.sh") },
      {
        runSession: async (s) => {
          const { raw, parsed } = directStart(s);
          return { kind: "cancel", raw, parsed, fetched: null };
        },
      },
    );
    expect(exitCode).toBe(130);

    const inv = onlyRow(allInvocations());
    expect(inv.outcome).toBe("cancelled");
    expect(inv.package_id).toBeNull();
    expect(inv.sha256).toBeNull();
    expect(inv.url).toBe(sourceUrl("/cancelme.sh"));
    expect(inv.install_command_json).toBeTruthy();
    expect(inv.exit_code).toBeNull();
    expect(inv.error_message).toBeNull();
    expect(inv.raw_input).toBe(installCmd("/cancelme.sh"));

    expect(allPackages()).toHaveLength(0);
  });
});

// =========================================================================
// 6. Cancel — committed command, after fetch completed
// =========================================================================

describe("cancel — committed, post-fetch", () => {
  test("returns 130; cancelled row carries the fetched sha; no CAS write; no package", async () => {
    const body = bytes("echo never run\n");
    const hash = sha256Hex(body);

    const exitCode = await runInstall(
      { kind: "direct", raw: installCmd("/cancel-after.sh") },
      {
        runSession: async (s) => {
          const { raw, parsed } = directStart(s);
          return { kind: "cancel", raw, parsed, fetched: fetchedFor(body) };
        },
      },
    );
    expect(exitCode).toBe(130);

    const inv = onlyRow(allInvocations());
    expect(inv.outcome).toBe("cancelled");
    expect(inv.sha256).toBe(hash);
    // A fetch DID complete before the cancel → its served-from origin is recorded.
    expect(inv.final_url).toBe("https://localhost/resolved");
    expect(inv.package_id).toBeNull();
    expect(inv.exit_code).toBeNull();

    expect(allPackages()).toHaveLength(0);
    // Cancel does NOT save to the CAS — the slot for these bytes stays empty.
    expect(readScript(hash)).toBeNull();
  });
});

// =========================================================================
// 7. Cancel — interactive paste with no committed command
// =========================================================================

describe("cancel — interactive paste (no command)", () => {
  test("returns 0; zero invocation rows; zero packages", async () => {
    const exitCode = await runInstall(
      { kind: "interactive" },
      { runSession: async () => ({ kind: "cancel", raw: null, parsed: null, fetched: null }) },
    );
    expect(exitCode).toBe(0);

    expect(allInvocations()).toHaveLength(0);
    expect(allPackages()).toHaveLength(0);
  });
});

// =========================================================================
// 8. Re-run on installed package preserves installed_at
// =========================================================================

describe("re-run preserves first installed_at", () => {
  test("second successful run does not bump installed_at; last_ran_at advances", async () => {
    const body = bytes("echo ok\n");

    expect(
      await runInstall({ kind: "direct", raw: installCmd("/idempotent.sh") }, runWith(body)),
    ).toBe(0);
    const firstPkg = onlyRow(allPackages());
    const firstInstalledAt = firstPkg.installed_at as string;
    const firstLastRanAt = firstPkg.last_ran_at as string;
    expect(firstInstalledAt).toBeTruthy();

    // Force a measurable wall-clock gap so ISO strings differ.
    await new Promise((r) => setTimeout(r, 10));

    expect(
      await runInstall({ kind: "direct", raw: installCmd("/idempotent.sh") }, runWith(body)),
    ).toBe(0);
    const secondPkg = onlyRow(allPackages());
    expect(secondPkg.installed_at).toBe(firstInstalledAt);
    expect(secondPkg.last_ran_at).not.toBe(firstLastRanAt);

    // Two invocations now; still one package row (same parsed.url → same row).
    expect(allInvocations()).toHaveLength(2);
    expect(allPackages()).toHaveLength(1);
  });
});

// =========================================================================
// 9. Re-run with new sha updates current_sha256
// =========================================================================

describe("re-run with new sha", () => {
  test("current_sha256 tracks the latest successful run's bytes", async () => {
    const v1 = bytes("echo v1\n");
    const v2 = bytes("echo v2-updated\n");

    expect(await runInstall({ kind: "direct", raw: installCmd("/upgrade.sh") }, runWith(v1))).toBe(
      0,
    );
    expect(onlyRow(allPackages()).current_sha256).toBe(sha256Hex(v1));

    expect(await runInstall({ kind: "direct", raw: installCmd("/upgrade.sh") }, runWith(v2))).toBe(
      0,
    );
    expect(onlyRow(allPackages()).current_sha256).toBe(sha256Hex(v2));
  });
});

// =========================================================================
// 10. Failure on installed does NOT downgrade
// =========================================================================

describe("installed → failed exec preserves installed status", () => {
  test("status stays installed; current_sha256 stays at the installed bytes", async () => {
    const okBody = bytes("echo good\n");
    const badBody = bytes("exit 3\n");

    expect(
      await runInstall({ kind: "direct", raw: installCmd("/sometimes.sh") }, runWith(okBody)),
    ).toBe(0);
    const installedPkg = onlyRow(allPackages());
    expect(installedPkg.status).toBe("installed");
    expect(installedPkg.current_sha256).toBe(sha256Hex(okBody));

    expect(
      await runInstall({ kind: "direct", raw: installCmd("/sometimes.sh") }, runWith(badBody)),
    ).toBe(3);

    const pkg = onlyRow(allPackages());
    expect(pkg.status).toBe("installed");
    // current_sha256 NOT bumped to the failing-run's bytes.
    expect(pkg.current_sha256).toBe(sha256Hex(okBody));

    const invs = allInvocations();
    expect(invs).toHaveLength(2);
    const outcomes = invs.map((i) => i.outcome).sort();
    expect(outcomes).toEqual(["errored", "ran"]);
  });
});

// =========================================================================
// 11. Transactional invariant
// =========================================================================

describe("transactional invariant", () => {
  test("after happy-path runInstall, package + invocation are observably committed", async () => {
    const body = bytes("echo tx\n");

    expect(await runInstall({ kind: "direct", raw: installCmd("/tx.sh") }, runWith(body))).toBe(0);

    const pkg = onlyRow(allPackages());
    const inv = onlyRow(allInvocations());
    expect(pkg.status).toBe("installed");
    expect(inv.outcome).toBe("ran");
    expect(inv.package_id).toBe(pkg.id);
    // Cross-check: invocation sha === package's current sha.
    expect(inv.sha256).toBe(pkg.current_sha256);
  });
});
