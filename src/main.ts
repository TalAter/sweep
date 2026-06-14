import { isTTY } from "wrap-core/ansi";
import { ensureConfig } from "./config.ts";
import { ensureSweepHome } from "./fs.ts";
import { type RunInstallDeps, runInstall } from "./installer/install.ts";
import { dispatch } from "./subcommands/dispatch.ts";
import { commands } from "./subcommands/registry.ts";

export async function main(deps: RunInstallDeps = {}) {
  try {
    ensureSweepHome();
    ensureConfig();
    const positional = process.argv[2] ?? "";

    // Interactive mode: no args + TTY → run the install session for a pasted
    // command (parse happens inside the session, on paste).
    if (!positional && isTTY()) {
      process.exitCode = await runInstall({ kind: "interactive" }, deps);
      return;
    }

    const verb = commands.find((c) => c.name === positional);
    // Assign to process.exitCode rather than calling process.exit() — the
    // latter terminates the test process mid-run.
    if (verb) {
      process.exitCode = await dispatch(verb.name, process.argv.slice(3));
    } else {
      process.exitCode = await runInstall({ kind: "direct", raw: positional }, deps);
    }
  } catch (err) {
    console.error(`sweep: ${err instanceof Error ? err.message : String(err)}`);
    process.exitCode = 1;
  }
}
