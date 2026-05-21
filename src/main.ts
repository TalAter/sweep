import { ensureConfig } from "./config.ts";
import { ensureSweepHome } from "./fs.ts";
import { runInstall } from "./installer/install.ts";
import { dispatch } from "./subcommands/dispatch.ts";
import { commands } from "./subcommands/registry.ts";

export async function main() {
  try {
    ensureSweepHome();
    ensureConfig();
    const positional = process.argv[2] ?? "";
    const verb = commands.find((c) => c.name === positional);
    // Assign to process.exitCode rather than calling process.exit() — the
    // latter terminates the test process mid-run.
    if (verb) {
      process.exitCode = await dispatch(verb.name, process.argv.slice(3));
    } else {
      process.exitCode = await runInstall(positional);
    }
  } catch (err) {
    console.error(`sweep: ${err instanceof Error ? err.message : String(err)}`);
    process.exitCode = 1;
  }
}
