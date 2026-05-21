import { listInstalledPackages } from "../store/packages.ts";
import type { Subcommand } from "./types.ts";

export const listCmd: Subcommand = {
  name: "list",
  description: "List installed packages.",
  run: async (_argv: string[]): Promise<number> => {
    const rows = listInstalledPackages();
    if (rows.length === 0) {
      console.log("No packages installed.");
      return 0;
    }
    for (const r of rows) {
      // installedAt is non-null in practice on status='installed' rows, but
      // render "" rather than "null" if upstream invariants ever drift.
      console.log(`${r.slug}\t${r.sourceUrl}\t${r.installedAt ?? ""}`);
    }
    return 0;
  },
};
