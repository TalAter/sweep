/**
 * Integration tests for `runInstall` — the install orchestrator.
 *
 * No network: a local `Bun.serve` fixture on an ephemeral port (`port: 0`)
 * supplies routes for happy path, non-zero exec, and 404. Tests register
 * routes per-test and drive the real fetch + spawn path end-to-end so we
 * exercise the actual integration (DB writes, CAS save, package status
 * transitions, transactional commit) rather than mocks.
 *
 * `console.error` is silenced for every test so error-path output doesn't
 * leak into the test report. We don't assert on the exact message format;
 * we only assert that error paths print SOMETHING and that the DB ends up
 * in the right state.
 *
 * `process.env.SWEEP_HOME` is pinned by `tests/sweep-home-preload.ts`, and
 * the DB handle + temp home are reset between tests there — so each test
 * starts with empty `packages` and `invocations` tables.
 */
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, mock, test } from "bun:test";
import { runInstall } from "../src/installer/install.ts";
import { ensureDb } from "../src/store/db.ts";
import { readScript } from "../src/store/scripts.ts";

// ---- fixture server -------------------------------------------------------
//
// Routes registered per-test, wiped in afterEach so handlers don't bleed.

type Handler = (req: Request) => Response | Promise<Response>;
const routes: Record<string, Handler> = {};

let server: ReturnType<typeof Bun.serve>;

beforeAll(() => {
  server = Bun.serve({
    port: 0,
    fetch(req) {
      const u = new URL(req.url);
      const handler = routes[u.pathname];
      return handler ? handler(req) : new Response("not found", { status: 404 });
    },
  });
});

afterAll(() => {
  server.stop(true);
});

// ---- console.error silencer ----------------------------------------------
//
// Keep test output clean; record the spy so we can assert error paths
// printed at least once.

const errSpy = mock(() => {});
const origConsoleError = console.error;

beforeEach(() => {
  // Wipe routes (defense vs. leftover handlers if a test threw mid-setup).
  for (const k of Object.keys(routes)) delete routes[k];
  // Reinstall spy each test; clear call history.
  console.error = errSpy;
  errSpy.mockClear();
});

afterEach(() => {
  console.error = origConsoleError;
});

// ---- helpers -------------------------------------------------------------

const url = (path: string): string => `http://localhost:${server.port}${path}`;

/** Build a `curl <url> | sh` install command targeting the fixture. */
const installCmd = (path: string, shell = "sh"): string => `curl -fsSL ${url(path)} | ${shell}`;

const bytes = (s: string): Uint8Array => new TextEncoder().encode(s);

/** Independent sha256 of bytes — proves install stored the right CAS slot. */
const sha256Hex = (b: Uint8Array): string => {
  const h = new Bun.CryptoHasher("sha256");
  h.update(b);
  return h.digest("hex");
};

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
  test("exits 0; persists installed package + ran invocation", async () => {
    const body = bytes('echo "ok"\nexit 0\n');
    const hash = sha256Hex(body);
    routes["/install.sh"] = () => new Response(body, { status: 200 });

    const exitCode = await runInstall(installCmd("/install.sh"));
    expect(exitCode).toBe(0);

    const pkg = onlyRow(allPackages());
    expect(pkg.status).toBe("installed");
    expect(pkg.slug).toBe("localhost"); // hostname stem of http://localhost:<port>/…
    expect(pkg.source_url).toBe(url("/install.sh"));
    expect(pkg.current_sha256).toBe(hash);
    expect(pkg.installed_at).toBeTruthy();
    expect(pkg.last_ran_at).toBeTruthy();

    const inv = onlyRow(allInvocations());
    expect(inv.outcome).toBe("ran");
    expect(inv.exit_code).toBe(0);
    expect(inv.package_id).toBe(pkg.id);
    expect(inv.url).toBe(url("/install.sh"));
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
  test("returns the exec exit code; package transitions to failed; invocation outcome=errored", async () => {
    const body = bytes("exit 7\n");
    routes["/bad.sh"] = () => new Response(body, { status: 200 });

    const exitCode = await runInstall(installCmd("/bad.sh"));
    expect(exitCode).toBe(7);

    const pkg = onlyRow(allPackages());
    expect(pkg.status).toBe("failed");
    // current_sha256 stays null on a fresh attempting→failed transition (per
    // updatePackageOnExec spec: only success refreshes the hash).
    expect(pkg.current_sha256).toBeNull();

    const inv = onlyRow(allInvocations());
    expect(inv.outcome).toBe("errored");
    expect(inv.exit_code).toBe(7);
  });
});

// =========================================================================
// 3. Parse failure
// =========================================================================

describe("parse failure", () => {
  test("returns 2; one parse_failed invocation; no packages row", async () => {
    const exitCode = await runInstall("not a real command");
    expect(exitCode).toBe(2);

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

    // Stderr was hit at least once.
    expect(errSpy).toHaveBeenCalled();
  });
});

// =========================================================================
// 4. Fetch failure
// =========================================================================

describe("fetch failure", () => {
  test("404 returns 1; one fetch_failed invocation; no packages row", async () => {
    routes["/missing.sh"] = () => new Response("nope", { status: 404 });

    const exitCode = await runInstall(installCmd("/missing.sh"));
    expect(exitCode).toBe(1);

    const inv = onlyRow(allInvocations());
    expect(inv.outcome).toBe("fetch_failed");
    expect(inv.package_id).toBeNull();
    expect(inv.url).toBe(url("/missing.sh"));
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
// 5. Re-run on installed package preserves installed_at
// =========================================================================

describe("re-run preserves first installed_at", () => {
  test("second successful run does not bump installed_at; last_ran_at advances", async () => {
    const body = bytes("echo ok\n");
    routes["/idempotent.sh"] = () => new Response(body, { status: 200 });

    expect(await runInstall(installCmd("/idempotent.sh"))).toBe(0);
    const firstPkg = onlyRow(allPackages());
    const firstInstalledAt = firstPkg.installed_at as string;
    const firstLastRanAt = firstPkg.last_ran_at as string;
    expect(firstInstalledAt).toBeTruthy();

    // Force a measurable wall-clock gap so ISO strings differ (we want to
    // observe that installed_at is pinned and last_ran_at moves).
    await new Promise((r) => setTimeout(r, 10));

    expect(await runInstall(installCmd("/idempotent.sh"))).toBe(0);
    const secondPkg = onlyRow(allPackages());
    expect(secondPkg.installed_at).toBe(firstInstalledAt);
    // last_ran_at strictly advances.
    expect(secondPkg.last_ran_at).not.toBe(firstLastRanAt);

    // Two invocations now; still one package row.
    expect(allInvocations()).toHaveLength(2);
    expect(allPackages()).toHaveLength(1);
  });
});

// =========================================================================
// 6. Re-run with new sha updates current_sha256
// =========================================================================

describe("re-run with new sha", () => {
  test("current_sha256 tracks the latest successful run's bytes", async () => {
    const v1 = bytes("echo v1\n");
    const v2 = bytes("echo v2-updated\n");

    routes["/upgrade.sh"] = () => new Response(v1, { status: 200 });
    expect(await runInstall(installCmd("/upgrade.sh"))).toBe(0);
    expect(onlyRow(allPackages()).current_sha256).toBe(sha256Hex(v1));

    // Swap the fixture's bytes — same URL, new content.
    routes["/upgrade.sh"] = () => new Response(v2, { status: 200 });
    expect(await runInstall(installCmd("/upgrade.sh"))).toBe(0);
    expect(onlyRow(allPackages()).current_sha256).toBe(sha256Hex(v2));
  });
});

// =========================================================================
// 7. Failure on installed does NOT downgrade
// =========================================================================

describe("installed → failed exec preserves installed status", () => {
  test("status stays installed; current_sha256 stays at the installed bytes", async () => {
    const okBody = bytes("echo good\n");
    const badBody = bytes("exit 3\n");

    routes["/sometimes.sh"] = () => new Response(okBody, { status: 200 });
    expect(await runInstall(installCmd("/sometimes.sh"))).toBe(0);
    const installedPkg = onlyRow(allPackages());
    expect(installedPkg.status).toBe("installed");
    expect(installedPkg.current_sha256).toBe(sha256Hex(okBody));

    // Swap to a failing script — exec returns non-zero.
    routes["/sometimes.sh"] = () => new Response(badBody, { status: 200 });
    expect(await runInstall(installCmd("/sometimes.sh"))).toBe(3);

    const pkg = onlyRow(allPackages());
    expect(pkg.status).toBe("installed");
    // current_sha256 NOT bumped to the failing-run's bytes (per spec: only
    // success refreshes the hash).
    expect(pkg.current_sha256).toBe(sha256Hex(okBody));

    // Both invocations recorded.
    const invs = allInvocations();
    expect(invs).toHaveLength(2);
    const outcomes = invs.map((i) => i.outcome).sort();
    expect(outcomes).toEqual(["errored", "ran"]);
  });
});

// =========================================================================
// 8. Step-5 analysis seam (env-gated LLM analysis)
// =========================================================================
//
// `SWEEP_TEST_RESPONSES` is sweep's own test-provider contract (wrap-core
// reads no env vars — test-provider selection is consumer policy). Set →
// step 5 runs a canned analysis conversation end to end; absent → step 5
// stays a no-op. The preload deletes the var before every test, so every
// other test in this file pins the no-op path for free.
//
// The invariant under test throughout: analysis NEVER changes install
// semantics — exit codes, invocation rows, and package status belong to
// the exec path alone, whatever the analysis does.

describe("step-5 analysis (env-gated)", () => {
  /** Joined console.error lines, mirroring main.test.ts's logLines(). */
  const errLines = (): string[] =>
    errSpy.mock.calls.map((call) => (call as unknown[]).map((a) => String(a)).join(" "));

  test("canned summary: install succeeds and a sweep: line carries the summary", async () => {
    routes["/analyzed.sh"] = () => new Response(bytes("echo ok\n"), { status: 200 });
    process.env.SWEEP_TEST_RESPONSES = JSON.stringify({
      summary: "Downloads a binary to /usr/local/bin.",
    });

    expect(await runInstall(installCmd("/analyzed.sh"))).toBe(0);

    const line = errLines().find((l) => l.includes("Downloads a binary to /usr/local/bin."));
    expect(line).toBeDefined();
    expect(line).toStartWith("sweep:");

    // Install semantics unchanged by the analysis.
    expect(onlyRow(allPackages()).status).toBe("installed");
    const inv = onlyRow(allInvocations());
    expect(inv.outcome).toBe("ran");
    expect(inv.exit_code).toBe(0);
  });

  test("provider error in playback: install still succeeds; sweep: failure line surfaced", async () => {
    routes["/analyzed.sh"] = () => new Response(bytes("echo ok\n"), { status: 200 });
    // JSON-array form — also proves the list shape of the env value parses.
    process.env.SWEEP_TEST_RESPONSES = JSON.stringify(["ERROR: model exploded"]);

    expect(await runInstall(installCmd("/analyzed.sh"))).toBe(0);

    const line = errLines().find((l) => l.includes("model exploded"));
    expect(line).toBeDefined();
    expect(line).toStartWith("sweep:");

    expect(onlyRow(allPackages()).status).toBe("installed");
    expect(onlyRow(allInvocations()).outcome).toBe("ran");
  });

  test("unparseable canned response: parse retry exhausts; install still succeeds", async () => {
    routes["/analyzed.sh"] = () => new Response(bytes("echo ok\n"), { status: 200 });
    // Not JSON → taken verbatim as a single repeating response → both send
    // attempts fail to parse → typed parse error → caught and surfaced.
    process.env.SWEEP_TEST_RESPONSES = "this is not json";

    expect(await runInstall(installCmd("/analyzed.sh"))).toBe(0);

    const line = errLines().find((l) => l.includes("script analysis failed"));
    expect(line).toBeDefined();
    expect(line).toStartWith("sweep:");

    expect(onlyRow(allPackages()).status).toBe("installed");
    expect(onlyRow(allInvocations()).outcome).toBe("ran");
  });

  test("without SWEEP_TEST_RESPONSES, a happy-path install makes zero console.error calls (no analysis lines)", async () => {
    // The preload wiped the var; this pins the gate: step 5 is a strict
    // no-op for real users (createLlm never runs, no analysis line is
    // printed). Scope: console.error calls only — the spawned script's own
    // stderr is out of frame.
    routes["/plain.sh"] = () => new Response(bytes("echo ok\n"), { status: 200 });

    expect(await runInstall(installCmd("/plain.sh"))).toBe(0);

    expect(errSpy).not.toHaveBeenCalled();
    expect(onlyRow(allPackages()).status).toBe("installed");
  });
});

// =========================================================================
// 9. Transactional invariant
// =========================================================================

describe("transactional invariant", () => {
  test("after happy-path runInstall, package + invocation are observably committed", async () => {
    const body = bytes("echo tx\n");
    routes["/tx.sh"] = () => new Response(body, { status: 200 });

    expect(await runInstall(installCmd("/tx.sh"))).toBe(0);

    // Both rows must be queryable post-call. (If the transaction were never
    // committed, the SELECTs below would come back empty.)
    const pkg = onlyRow(allPackages());
    const inv = onlyRow(allInvocations());
    expect(pkg.status).toBe("installed");
    expect(inv.outcome).toBe("ran");
    expect(inv.package_id).toBe(pkg.id);
    // Cross-check: invocation sha === package's current sha.
    expect(inv.sha256).toBe(pkg.current_sha256);
  });
});
