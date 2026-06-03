import { resolveAppearance, resolveTheme, setTheme } from "wrap-core/theme";
import type { TableColumn } from "wrap-core/tui";
import { sweepFs } from "../fs.ts";
import { listInstalledPackages, type PackageRow } from "../store/packages.ts";
import type { Subcommand } from "./types.ts";

/** The source URL reduced to its host (e.g. `https://github.com/x/y` → `github.com`). */
function sourceHost(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

/** ISO timestamp → `YYYY-MM-DD`, or `never` when the package has never run. */
function formatLastRan(iso: string | null): string {
  return iso ? iso.slice(0, 10) : "never";
}

/** Table columns for `sweep list`: slug · source host · status · last ran. */
const LIST_COLUMN_HEADERS = ["PACKAGE", "SOURCE", "STATUS", "LAST RAN"] as const;

/** Shape package rows into aligned table cells, parallel to {@link LIST_COLUMN_HEADERS}. */
export function buildListRows(packages: PackageRow[]): string[][] {
  return packages.map((r) => [
    r.slug,
    sourceHost(r.sourceUrl),
    r.status,
    formatLastRan(r.lastRanAt),
  ]);
}

export const listCmd: Subcommand = {
  name: "list",
  description: "List installed packages.",
  run: async (_argv: string[]): Promise<number> => {
    const rows = listInstalledPackages();
    if (rows.length === 0) {
      console.log("No packages installed.");
      return 0;
    }

    const appearance = await resolveAppearance({ envVarName: "SWEEP_THEME", fs: sweepFs });
    const theme = resolveTheme(appearance);
    setTheme(theme);

    const [react, { Table, printInline }] = await Promise.all([
      import("react"),
      import("wrap-core/tui"),
    ]);

    const colors = [theme.copy.body, theme.copy.link, theme.copy.success, theme.copy.supporting];
    const columns: TableColumn[] = LIST_COLUMN_HEADERS.map((header, i) => ({
      header,
      color: colors[i],
    }));

    await printInline(react.createElement(Table, { columns, rows: buildListRows(rows) }), {
      theme,
      nerdFonts: false,
    });
    return 0;
  },
};
