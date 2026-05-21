/**
 * Tests for `runScript` — actually spawn `/bin/sh` (and `/bin/bash`) and assert
 * the recipe wires correctly: exit code passthrough, stdin-fed script body,
 * scriptArgs, env vars (including parent-env override), and shell selection.
 *
 * Why real shells, not mocks: `runScript` is a thin `Bun.spawn` wrapper — the
 * only thing worth testing is whether the recipe matches the spec. Mocking
 * `Bun.spawn` would tautologically re-state the recipe; spawning a real shell
 * exercises the actual end-to-end behavior (stdin bytes really arriving at fd 0,
 * env really overriding, etc).
 *
 * Sentinel-file pattern: stdout/stderr are inherited (and silenced for tests
 * via `tests/spawn-inherit-preload.ts` rewriting inherit→ignore). To assert
 * *what* the script did, we have scripts write a sentinel string to a temp file.
 * Each test gets its own temp dir created/torn down via beforeEach/afterEach.
 *
 * `sudo: true` is intentionally not exercised — would prompt for a password in
 * a dev shell or fail noisily in CI. The recipe wires identically; trust it.
 */
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type InstallCommand, runScript } from "../src/installer/exec.ts";

// ---- fixture helpers ------------------------------------------------------

let dir: string;
let sentinel: string;

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "sweep-exec-test-"));
  sentinel = join(dir, "out");
});
afterEach(() => {
  rmSync(dir, { recursive: true, force: true });
});

function cmdFor(opts: Partial<InstallCommand> = {}): InstallCommand {
  return {
    envVars: {},
    sudo: false,
    shell: "sh",
    scriptArgs: [],
    url: "https://example.com/x",
    raw: "curl https://example.com/x | sh",
    ...opts,
  };
}

const bytes = (s: string): Uint8Array => new TextEncoder().encode(s);

// =========================================================================
// Exit code passthrough
// =========================================================================

describe("exit code", () => {
  test("0 on a no-op script", async () => {
    const { exitCode } = await runScript(cmdFor(), bytes('echo "ok"\n'));
    expect(exitCode).toBe(0);
  });

  test("non-zero exit is returned, not thrown", async () => {
    const { exitCode } = await runScript(cmdFor(), bytes("exit 7\n"));
    expect(exitCode).toBe(7);
  });
});

// =========================================================================
// Script body arrives via stdin
// =========================================================================

describe("stdin = script body", () => {
  test("commands inside the script actually run", async () => {
    // If the shell didn't receive the script bytes on fd 0, this file
    // wouldn't get written.
    const script = `echo ran > ${sentinel}\n`;
    const { exitCode } = await runScript(cmdFor(), bytes(script));
    expect(exitCode).toBe(0);
    expect(readFileSync(sentinel, "utf8")).toBe("ran\n");
  });
});

// =========================================================================
// scriptArgs reach the script as positional params
// =========================================================================

describe("scriptArgs", () => {
  test("$1 $2 see the args via -s --", async () => {
    const script = `printf '%s\\n%s\\n' "$1" "$2" > ${sentinel}\n`;
    const { exitCode } = await runScript(cmdFor({ scriptArgs: ["alpha", "beta"] }), bytes(script));
    expect(exitCode).toBe(0);
    expect(readFileSync(sentinel, "utf8")).toBe("alpha\nbeta\n");
  });

  test("empty scriptArgs is harmless (-s -- still passed)", async () => {
    // If `-s --` weren't always included, `bash` would treat the script body
    // arg-mode badly. The spec promises this is always safe.
    const script = `printf 'no-args\\n' > ${sentinel}\n`;
    const { exitCode } = await runScript(cmdFor(), bytes(script));
    expect(exitCode).toBe(0);
    expect(readFileSync(sentinel, "utf8")).toBe("no-args\n");
  });
});

// =========================================================================
// envVars
// =========================================================================

describe("envVars", () => {
  test("user envVars reach the script", async () => {
    const script = `printf '%s\\n' "$FOO" > ${sentinel}\n`;
    const { exitCode } = await runScript(cmdFor({ envVars: { FOO: "hello" } }), bytes(script));
    expect(exitCode).toBe(0);
    expect(readFileSync(sentinel, "utf8")).toBe("hello\n");
  });

  test("user envVars OVERRIDE parent process env", async () => {
    // Set a parent env var, then pass a colliding one — child must see the
    // user-supplied value (later-spread-wins per spec).
    const parentKey = "SWEEP_EXEC_TEST_OVERRIDE";
    const prev = process.env[parentKey];
    process.env[parentKey] = "parent";
    try {
      const script = `printf '%s\\n' "$${parentKey}" > ${sentinel}\n`;
      const { exitCode } = await runScript(
        cmdFor({ envVars: { [parentKey]: "child" } }),
        bytes(script),
      );
      expect(exitCode).toBe(0);
      expect(readFileSync(sentinel, "utf8")).toBe("child\n");
    } finally {
      if (prev === undefined) delete process.env[parentKey];
      else process.env[parentKey] = prev;
    }
  });

  test("parent env still visible when not overridden", async () => {
    const parentKey = "SWEEP_EXEC_TEST_INHERIT";
    const prev = process.env[parentKey];
    process.env[parentKey] = "from-parent";
    try {
      const script = `printf '%s\\n' "$${parentKey}" > ${sentinel}\n`;
      const { exitCode } = await runScript(cmdFor(), bytes(script));
      expect(exitCode).toBe(0);
      expect(readFileSync(sentinel, "utf8")).toBe("from-parent\n");
    } finally {
      if (prev === undefined) delete process.env[parentKey];
      else process.env[parentKey] = prev;
    }
  });
});

// =========================================================================
// Shell selection
// =========================================================================

describe("shell selection", () => {
  // We can't reliably distinguish bash from sh by feature flags across
  // macOS (`/bin/sh` is bash-in-POSIX-mode) and Linux (`/bin/sh` is dash).
  // Instead, assert that `$0` inside the script equals the argv[0] the
  // shell was launched with — proves `cmd.shell` actually drives argv.
  test("shell='bash' makes the child see $0 = bash", async () => {
    const script = `printf '%s\\n' "$0" > ${sentinel}\n`;
    const { exitCode } = await runScript(cmdFor({ shell: "bash" }), bytes(script));
    expect(exitCode).toBe(0);
    // Path may resolve to `/bin/bash` or stay as `bash` depending on PATH;
    // either way it ends with `bash`.
    expect(readFileSync(sentinel, "utf8").trim()).toMatch(/(^|\/)bash$/);
  });

  test("shell='sh' makes the child see $0 = sh", async () => {
    const script = `printf '%s\\n' "$0" > ${sentinel}\n`;
    const { exitCode } = await runScript(cmdFor({ shell: "sh" }), bytes(script));
    expect(exitCode).toBe(0);
    expect(readFileSync(sentinel, "utf8").trim()).toMatch(/(^|\/)sh$/);
  });
});
