/**
 * Spawn-inherit suppression preload. Wired via `bunfig.toml` → `[test] preload`,
 * AFTER `sweep-home-preload.ts`.
 *
 * Why: `installer/exec.ts` spawns the install script with
 * `stdout: "inherit", stderr: "inherit"` so install scripts can write to the
 * user's terminal as if they were running under `curl | sh`. In a test run
 * that wires straight to the test runner's fd 1/2, leaking child output
 * (e.g. `echo "ok"`) into the test report.
 *
 * Solution (borrowed from wrap): monkey-patch `Bun.spawn` / `Bun.spawnSync`
 * at preload time to rewrite `"inherit"` → `"ignore"`. Opt-out per test with
 * `SWEEP_TEST_ALLOW_INHERIT=1` for the rare case where a test wants to see
 * the real child stdio.
 *
 * This is a test-only concern; production stdio behavior is untouched.
 */

/**
 * Bun.spawn / Bun.spawnSync can be called in two shapes:
 *
 *   Bun.spawn(argv, options?)                 // 2-arg: opts in arg[1]
 *   Bun.spawn({ cmd: argv, ...options })      // 1-arg: opts merged with cmd
 *
 * Our `installer/exec.ts` uses the 1-arg form, so patching only the 2nd
 * argument (as wrap does) silently misses it. Detect the shape and rewrite
 * the right place.
 */

function shouldPatch(opts: { stdout?: unknown; stderr?: unknown } | null | undefined): boolean {
  if (!opts || process.env.SWEEP_TEST_ALLOW_INHERIT) return false;
  return opts.stdout === "inherit" || opts.stderr === "inherit";
}

function patchOpts<T extends { stdout?: unknown; stderr?: unknown }>(opts: T): T {
  if (!shouldPatch(opts)) return opts;
  const out = { ...opts };
  if (out.stdout === "inherit") out.stdout = "ignore";
  if (out.stderr === "inherit") out.stderr = "ignore";
  return out;
}

function patchArgs(args: unknown[]): unknown[] {
  if (args.length === 0) return args;
  const first = args[0];
  // 1-arg object form: { cmd, stdout, stderr, ... }
  if (
    args.length === 1 &&
    first &&
    typeof first === "object" &&
    !Array.isArray(first) &&
    "cmd" in first
  ) {
    return [patchOpts(first as { stdout?: unknown; stderr?: unknown })];
  }
  // 2-arg form: (argv, opts?)
  const opts = args[1];
  if (opts && typeof opts === "object") {
    return [first, patchOpts(opts as { stdout?: unknown; stderr?: unknown })];
  }
  return args;
}

const realSpawn = Bun.spawn;
Bun.spawn = ((...args: unknown[]) => {
  return (realSpawn as (...a: unknown[]) => unknown)(...patchArgs(args));
}) as typeof Bun.spawn;

const realSpawnSync = Bun.spawnSync;
Bun.spawnSync = ((...args: unknown[]) => {
  return (realSpawnSync as (...a: unknown[]) => unknown)(...patchArgs(args));
}) as typeof Bun.spawnSync;
