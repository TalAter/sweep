/**
 * End-to-end tests for the `main()` entrypoint. We invoke main() in-process
 * with synthesized `process.argv`, await it, then assert on `process.exitCode`
 * plus observable side effects (DB rows, console output).
 *
 * `process.argv` and `process.exitCode` are saved/restored per test to keep
 * leakage to zero — Bun runs test files serially in one process.
 *
 * `console.log` / `console.error` are mocked so command output doesn't leak
 * into the test report.
 */
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, mock, test } from "bun:test";
import { main } from "../src/main.ts";
import { ensureDb } from "../src/store/db.ts";

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

// ---- fixture server (only test 5 needs it) -------------------------------

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

beforeEach(() => {
  for (const k of Object.keys(routes)) delete routes[k];
});

// ---- helpers -------------------------------------------------------------

const url = (path: string): string => `http://localhost:${server.port}${path}`;

function logLines(): string[] {
  return logSpy.mock.calls.map((call) => call.map((a) => String(a)).join(" "));
}

function allPackages(): Array<Record<string, unknown>> {
  return ensureDb().query("SELECT * FROM packages").all() as Array<Record<string, unknown>>;
}

// =========================================================================

describe("main()", () => {
  test("`sweep list` on empty DB prints 'No packages installed.' and exits 0", async () => {
    process.argv = ["bun", "/path/to/index.ts", "list"];
    await main();
    expect(process.exitCode).toBe(0);
    expect(logLines()).toEqual(["No packages installed."]);
  });

  test("`sweep list` after install renders the slug", async () => {
    // Seed via the install path so we exercise the same plumbing end-to-end.
    routes["/seed.sh"] = () => new Response(new TextEncoder().encode("echo ok\n"), { status: 200 });
    process.argv = ["bun", "/path/to/index.ts", `curl -fsSL ${url("/seed.sh")} | sh`];
    await main();
    expect(process.exitCode).toBe(0);
    logSpy.mockClear();
    errSpy.mockClear();

    process.argv = ["bun", "/path/to/index.ts", "list"];
    await main();
    expect(process.exitCode).toBe(0);
    const line = logLines()[0] ?? "";
    expect(line).toContain("localhost"); // slug from http://localhost:<port>
    expect(line).toContain(url("/seed.sh"));
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

  test("`sweep '<curl | sh>'` happy path exits 0 and persists a package row", async () => {
    routes["/install.sh"] = () =>
      new Response(new TextEncoder().encode('echo "ok"\nexit 0\n'), { status: 200 });
    process.argv = ["bun", "/path/to/index.ts", `curl -fsSL ${url("/install.sh")} | sh`];
    await main();
    expect(process.exitCode).toBe(0);

    const pkgs = allPackages();
    expect(pkgs).toHaveLength(1);
    expect(pkgs[0]?.status).toBe("installed");
    expect(pkgs[0]?.source_url).toBe(url("/install.sh"));
  });
});
