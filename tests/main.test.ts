/**
 * End-to-end tests for the `main()` entrypoint. We invoke main() in-process
 * with synthesized `process.argv`, await it, then assert on `process.exitCode`
 * plus observable side effects (DB rows, console output).
 *
 * Installs that reach the alt-screen session are driven via an injected
 * `runSession` (the `RunInstallDeps` seam `main` forwards to `runInstall`), so
 * the real fetch + Ink stack never runs. The injected decision echoes the
 * `raw`/`parsed` that runInstall already parsed and supplies canned bytes.
 *
 * `process.argv` and `process.exitCode` are saved/restored per test to keep
 * leakage to zero — Bun runs test files serially in one process.
 *
 * `console.log` / `console.error` are mocked so command output doesn't leak
 * into the test report.
 */
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { stripAnsi } from "wrap-core/ansi";
import type { FetchedScript } from "../src/installer/fetch.ts";
import type { RunInstallDeps } from "../src/installer/install.ts";
import { main } from "../src/main.ts";
import { ensureDb } from "../src/store/db.ts";
import type { SessionStart } from "../src/tui/install-session.ts";

// ---- argv / exitCode save-restore ----------------------------------------

const origArgv = process.argv;
const origExitCode = process.exitCode;

afterEach(() => {
  process.argv = origArgv;
  process.exitCode = origExitCode;
});

// ---- console spies --------------------------------------------------------

const logSpy = mock(() => {});
const errSpy = mock(() => {});
const origConsoleLog = console.log;
const origConsoleError = console.error;

beforeEach(() => {
  console.log = logSpy;
  console.error = errSpy;
  logSpy.mockClear();
  errSpy.mockClear();
});

afterEach(() => {
  console.log = origConsoleLog;
  console.error = origConsoleError;
});

// ---- helpers -------------------------------------------------------------

const bytes = (s: string): Uint8Array => new TextEncoder().encode(s);

const sha256Hex = (b: Uint8Array): string => {
  const h = new Bun.CryptoHasher("sha256");
  h.update(b);
  return h.digest("hex");
};

/** Build a `FetchedScript` from raw bytes (a successful fetch result). */
const fetchedFor = (b: Uint8Array): FetchedScript => ({
  bytes: b,
  sha256: sha256Hex(b),
  finalUrl: "https://localhost/resolved",
  fetchedAt: new Date().toISOString(),
  status: 200,
});

/** A `runSession` that always approves a run with the given bytes. */
const runWith = (b: Uint8Array): RunInstallDeps => ({
  runSession: async (s: SessionStart) => {
    if (s.kind !== "direct") throw new Error("test drives direct mode only");
    return { kind: "run", raw: s.raw, parsed: s.parsed, fetched: fetchedFor(b) };
  },
});

function logLines(): string[] {
  return logSpy.mock.calls.map((call) => call.map((a) => String(a)).join(" "));
}

function allPackages(): Array<Record<string, unknown>> {
  return ensureDb().query("SELECT * FROM packages").all() as Array<Record<string, unknown>>;
}

function allInvocations(): Array<Record<string, unknown>> {
  return ensureDb().query("SELECT * FROM invocations").all() as Array<Record<string, unknown>>;
}

// Sanity: the test process is non-TTY, so a positional-less run falls through
// to parse rather than the interactive session.

// =========================================================================

describe("main()", () => {
  test("`sweep list` on empty DB prints 'No packages installed.' and exits 0", async () => {
    process.argv = ["bun", "/path/to/index.ts", "list"];
    await main();
    expect(process.exitCode).toBe(0);
    expect(logLines()).toEqual(["No packages installed."]);
  });

  test("`sweep list` after install renders the slug", async () => {
    // Seed via the install path (injected session) so we exercise the same
    // DB plumbing end-to-end. Slug derives from the URL host stem → "localhost".
    process.argv = ["bun", "/path/to/index.ts", "curl -fsSL https://localhost/seed.sh | sh"];
    await main(runWith(bytes("echo ok\n")));
    expect(process.exitCode).toBe(0);
    logSpy.mockClear();
    errSpy.mockClear();

    // The table renders inline via printInline (stdout), not console.log.
    const chunks: string[] = [];
    const origWrite = process.stdout.write.bind(process.stdout);
    process.stdout.write = ((chunk: unknown) => {
      chunks.push(String(chunk));
      return true;
    }) as typeof process.stdout.write;

    process.argv = ["bun", "/path/to/index.ts", "list"];
    try {
      await main();
    } finally {
      process.stdout.write = origWrite;
    }
    expect(process.exitCode).toBe(0);
    expect(stripAnsi(chunks.join(""))).toContain("localhost");
  });

  test("malformed install command exits 2 and prints to stderr", async () => {
    process.argv = ["bun", "/path/to/index.ts", "not a real curl"];
    await main();
    expect(process.exitCode).toBe(2);
    expect(errSpy).toHaveBeenCalled();
  });

  test("no positional arg in non-TTY falls through to parse and exits 2", async () => {
    process.argv = ["bun", "/path/to/index.ts"];
    await main();
    expect(process.exitCode).toBe(2);
    expect(errSpy).toHaveBeenCalled();
  });

  test("leading/trailing whitespace in the install command is trimmed before it is stored", async () => {
    const cmd = "curl -fsSL https://localhost/install.sh | sh";
    process.argv = ["bun", "/path/to/index.ts", `  \n\t${cmd}\t \n `];
    await main(runWith(bytes("echo ok\nexit 0\n")));
    expect(process.exitCode).toBe(0);

    const invs = allInvocations();
    expect(invs).toHaveLength(1);
    // The persisted raw_input is the canonical trimmed form, not the padded input.
    expect(invs[0]?.raw_input).toBe(cmd);
  });

  test("`sweep '<curl | sh>'` happy path exits 0 and persists a package row", async () => {
    process.argv = ["bun", "/path/to/index.ts", "curl -fsSL https://localhost/install.sh | sh"];
    await main(runWith(bytes('echo "ok"\nexit 0\n')));
    expect(process.exitCode).toBe(0);

    const pkgs = allPackages();
    expect(pkgs).toHaveLength(1);
    expect(pkgs[0]?.status).toBe("installed");
    expect(pkgs[0]?.source_url).toBe("https://localhost/install.sh");
  });
});
