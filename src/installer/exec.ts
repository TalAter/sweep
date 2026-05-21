/**
 * Spawn the parsed install command and feed the fetched script bytes to its
 * stdin. The semantics are deliberately identical to `cat /local/script | <argv>`
 * — and therefore identical to `curl … | sh` — so installers that grab
 * `/dev/tty` for prompts (ollama, nvm, …) keep working.
 *
 * Contract (per spec `vault/impl-specs/v0.md` → `src/installer/exec.ts`):
 *
 *   - `argv = [...(cmd.sudo ? ["sudo"] : []), cmd.shell, "-s", "--", ...cmd.scriptArgs]`.
 *     Always include `-s --`: harmless when scriptArgs is empty, required when
 *     not (so the shell treats trailing tokens as `$1..$N` rather than as a
 *     script filename).
 *   - `env = { ...process.env, ...cmd.envVars }`: later spread wins, i.e.
 *     user-supplied env vars OVERRIDE the parent process's. The orchestrator
 *     spec depends on this ordering.
 *   - Bytes go to the child's fd 0 literally (Bun's spawn accepts `Uint8Array`
 *     for `stdin`).
 *   - stdout/stderr inherited so the child writes directly to the user's
 *     terminal — tests silence this via `tests/spawn-inherit-preload.ts`
 *     rewriting inherit→ignore.
 *   - Non-zero exit codes are RETURNED, not thrown. The installer records them
 *     as `outcome: "errored"`. The caller passes the exit code through to its
 *     own process exit.
 *   - No retry, no timeout, no signal interception. This is a thin wrapper.
 *   - Missing shell (ENOENT from `Bun.spawn`) propagates as a throw. The
 *     installer's top-level try/catch path is for this kind of unexpected
 *     failure; we don't synthesize a fake exit code here.
 */

import type { InstallCommand } from "./parse.ts";

// Re-export so tests (and future callers) can import the type from this module
// without reaching into `./parse.ts` — `exec.ts` is the public surface for the
// "run a parsed command" operation.
export type { InstallCommand };

export async function runScript(
  cmd: InstallCommand,
  scriptBytes: Uint8Array,
): Promise<{ exitCode: number }> {
  const argv: string[] = [...(cmd.sudo ? ["sudo"] : []), cmd.shell, "-s", "--", ...cmd.scriptArgs];

  // Spread order matters: parent env first, user env second — so user-supplied
  // vars override colliding parent vars. Spec requirement.
  const env = { ...process.env, ...cmd.envVars };

  const child = Bun.spawn({
    cmd: argv,
    env,
    stdin: scriptBytes,
    stdout: "inherit",
    stderr: "inherit",
  });

  await child.exited;
  // `exitCode` is `null` only for signal-killed processes (spec says we don't
  // intercept signals — pass -1 through; the installer surfaces it verbatim).
  return { exitCode: child.exitCode ?? -1 };
}
