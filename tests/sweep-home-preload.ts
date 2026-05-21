/**
 * SWEEP_HOME isolation preload — wired FIRST in `bunfig.toml` → `[test] preload`.
 *
 * `sweep/src/fs.ts` constructs `sweepFs = createAppFs({ app: "sweep" })` at
 * module load — `sweepFs.root` is captured once and never re-read. Any test
 * that imports a sweep module without SWEEP_HOME pre-set would point at the
 * developer's real `~/.sweep`. Setting `process.env.SWEEP_HOME` at the top of
 * a test file does NOT work: ES module imports are hoisted, so the sweep
 * import (and therefore sweepFs construction) runs BEFORE the env-var
 * assignment.
 *
 * Preload modules run before any test file's imports, so setting SWEEP_HOME
 * here pins it in time. Test files that need to read/write under SWEEP_HOME
 * import `TEST_HOME` from here. Bun runs test files serially in a shared
 * process, so TEST_HOME is one temp dir for the whole run — the global
 * `beforeEach` below wipes and recreates it between every test so files can
 * write freely without colliding with siblings.
 */
import { beforeEach } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const TEST_HOME = mkdtempSync(join(tmpdir(), "sweep-test-home-"));
process.env.SWEEP_HOME = TEST_HOME;

beforeEach(() => {
  rmSync(TEST_HOME, { recursive: true, force: true });
  mkdirSync(TEST_HOME, { recursive: true });
});
