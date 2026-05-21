import { ensureDb } from "./db.ts";

export type PackageRow = {
  id: number;
  slug: string;
  sourceUrl: string;
  currentSha256: string | null;
  status: "attempting" | "installed" | "failed" | "uninstalled" | "tracked";
  firstSeenAt: string;
  installedAt: string | null;
  lastRanAt: string | null;
};

/**
 * Resolves the `packages` row keyed by `source_url`. Creates it (status
 * `attempting`, sha/installed_at/last_ran_at all null) on first sight and
 * returns whatever is in the table afterwards. Pre-existing rows are returned
 * untouched — slug and other fields are NOT overwritten.
 *
 * Uses `INSERT … ON CONFLICT(source_url) DO NOTHING` + `SELECT` so the
 * conflict path is a single round-trip instead of a SELECT-then-INSERT race.
 */
export function findOrCreatePackage(args: { url: string; slug: string }): PackageRow {
  const db = ensureDb();
  db.run(
    `INSERT INTO packages
       (slug, source_url, current_sha256, status, first_seen_at, installed_at, last_ran_at)
     VALUES (?, ?, NULL, 'attempting', ?, NULL, NULL)
     ON CONFLICT(source_url) DO NOTHING`,
    [args.slug, args.url, new Date().toISOString()],
  );
  const row = db.query("SELECT * FROM packages WHERE source_url = ?").get(args.url);
  if (!row) {
    // Unreachable: the INSERT either created it or the row pre-existed.
    throw new Error(`findOrCreatePackage: row missing after upsert for ${args.url}`);
  }
  return rowToPackage(row);
}

/** All installed packages, most-recently-installed first. */
export function listInstalledPackages(): PackageRow[] {
  const rows = ensureDb()
    .query("SELECT * FROM packages WHERE status = 'installed' ORDER BY installed_at DESC")
    .all();
  return rows.map(rowToPackage);
}

/**
 * Applies the post-exec status transition to a package row. Single atomic
 * UPDATE — no read-modify-write. The CASE expressions encode the spec's
 * lifecycle table:
 *
 *   exitCode === 0  → status='installed', refresh current_sha256, bump
 *                     installed_at iff it's still NULL (preserve first-install
 *                     timestamp on re-runs), bump last_ran_at.
 *   exitCode !== 0  → only flip status to 'failed' when prior status was
 *                     'attempting' (never downgrade 'installed'). Leave
 *                     current_sha256 and installed_at alone. Still bump
 *                     last_ran_at.
 *
 * Safe to call inside a caller-owned `db.transaction(() => { ... })` — does
 * not open its own. install.ts wraps this together with `insertInvocation`
 * so the pair commits as one unit.
 */
export function updatePackageOnExec(args: {
  packageId: number;
  sha256: string;
  exitCode: number;
  ranAt: string;
}): void {
  ensureDb().run(
    `UPDATE packages
        SET status         = CASE WHEN ?1 = 0          THEN 'installed'
                                  WHEN status = 'attempting' THEN 'failed'
                                  ELSE status
                             END,
            current_sha256 = CASE WHEN ?1 = 0 THEN ?2
                                  ELSE current_sha256
                             END,
            installed_at   = CASE WHEN ?1 = 0 AND installed_at IS NULL THEN ?3
                                  ELSE installed_at
                             END,
            last_ran_at    = ?3
      WHERE id = ?4`,
    [args.exitCode, args.sha256, args.ranAt, args.packageId],
  );
}

/**
 * Maps a raw snake_case `packages` row to the camelCase domain `PackageRow`.
 * Trusts the schema (we wrote the migration) but asserts object-ness so a
 * future drift fails loudly here rather than downstream.
 */
function rowToPackage(row: unknown): PackageRow {
  if (!row || typeof row !== "object") {
    throw new Error(`rowToPackage: expected object, got ${typeof row}`);
  }
  const r = row as Record<string, unknown>;
  return {
    id: r.id as number,
    slug: r.slug as string,
    sourceUrl: r.source_url as string,
    currentSha256: r.current_sha256 as string | null,
    status: r.status as PackageRow["status"],
    firstSeenAt: r.first_seen_at as string,
    installedAt: r.installed_at as string | null,
    lastRanAt: r.last_ran_at as string | null,
  };
}
