import { describe, expect, test } from "bun:test";
import { ensureDb } from "../src/store/db.ts";
import {
  findOrCreatePackage,
  listInstalledPackages,
  updatePackageOnExec,
} from "../src/store/packages.ts";

const URL_A = "https://ollama.com/install.sh";
const URL_B = "https://bun.sh/install";
const SLUG_A = "ollama";
const SLUG_B = "bun";
const SHA_A = "a".repeat(64);
const SHA_B = "b".repeat(64);

describe("findOrCreatePackage", () => {
  test("creates a new row with the expected defaults", () => {
    const pkg = findOrCreatePackage({ url: URL_A, slug: SLUG_A });

    expect(pkg.slug).toBe(SLUG_A);
    expect(pkg.sourceUrl).toBe(URL_A);
    expect(pkg.currentSha256).toBeNull();
    expect(pkg.status).toBe("attempting");
    expect(pkg.installedAt).toBeNull();
    expect(pkg.lastRanAt).toBeNull();
    // ISO 8601 — Date round-trip is the cheapest sanity check.
    expect(Number.isNaN(Date.parse(pkg.firstSeenAt))).toBe(false);
    expect(typeof pkg.id).toBe("number");
  });

  test("is idempotent: second call with same source_url returns the same row unchanged", () => {
    const first = findOrCreatePackage({ url: URL_A, slug: SLUG_A });
    // Different slug — findOrCreatePackage must NOT overwrite anything when
    // the row already exists.
    const second = findOrCreatePackage({ url: URL_A, slug: "different-slug" });

    expect(second).toEqual(first);

    const count = ensureDb().query("SELECT COUNT(*) AS n FROM packages").get() as { n: number };
    expect(count.n).toBe(1);
  });

  test("a different source_url produces a different row", () => {
    const a = findOrCreatePackage({ url: URL_A, slug: SLUG_A });
    const b = findOrCreatePackage({ url: URL_B, slug: SLUG_B });

    expect(b.id).not.toBe(a.id);
    expect(b.sourceUrl).toBe(URL_B);
    expect(b.slug).toBe(SLUG_B);
  });
});

describe("listInstalledPackages", () => {
  test("returns [] when there are no installed packages", () => {
    expect(listInstalledPackages()).toEqual([]);
  });

  test("returns only status='installed' rows, ordered by installedAt DESC", () => {
    const older = findOrCreatePackage({ url: URL_A, slug: SLUG_A });
    const newer = findOrCreatePackage({ url: URL_B, slug: SLUG_B });
    const pending = findOrCreatePackage({ url: "https://mise.run", slug: "mise" });

    updatePackageOnExec({
      packageId: older.id,
      sha256: SHA_A,
      exitCode: 0,
      ranAt: "2026-05-19T10:00:00.000Z",
    });
    updatePackageOnExec({
      packageId: newer.id,
      sha256: SHA_B,
      exitCode: 0,
      ranAt: "2026-05-20T10:00:00.000Z",
    });

    const rows = listInstalledPackages();
    expect(rows.map((r) => r.id)).toEqual([newer.id, older.id]);
    expect(rows.every((r) => r.status === "installed")).toBe(true);
    expect(rows.some((r) => r.id === pending.id)).toBe(false);
  });
});

describe("updatePackageOnExec", () => {
  test("exitCode=0 on a fresh attempting row promotes to installed", () => {
    const pkg = findOrCreatePackage({ url: URL_A, slug: SLUG_A });
    const ranAt = "2026-05-20T12:00:00.000Z";

    updatePackageOnExec({ packageId: pkg.id, sha256: SHA_A, exitCode: 0, ranAt });

    const row = readRawPackage(pkg.id);
    expect(row.status).toBe("installed");
    expect(row.current_sha256).toBe(SHA_A);
    expect(row.installed_at).toBe(ranAt);
    expect(row.last_ran_at).toBe(ranAt);
  });

  test("exitCode=0 on an already-installed row preserves installed_at but refreshes sha and last_ran_at", () => {
    const pkg = findOrCreatePackage({ url: URL_A, slug: SLUG_A });
    const firstRanAt = "2026-05-19T08:00:00.000Z";
    const secondRanAt = "2026-05-20T15:30:00.000Z";

    updatePackageOnExec({ packageId: pkg.id, sha256: SHA_A, exitCode: 0, ranAt: firstRanAt });
    updatePackageOnExec({ packageId: pkg.id, sha256: SHA_B, exitCode: 0, ranAt: secondRanAt });

    const row = readRawPackage(pkg.id);
    expect(row.status).toBe("installed");
    expect(row.current_sha256).toBe(SHA_B);
    expect(row.installed_at).toBe(firstRanAt); // preserved
    expect(row.last_ran_at).toBe(secondRanAt);
  });

  test("non-zero exit on an attempting row transitions to failed and leaves sha/installed_at null", () => {
    const pkg = findOrCreatePackage({ url: URL_A, slug: SLUG_A });
    const ranAt = "2026-05-20T12:00:00.000Z";

    updatePackageOnExec({ packageId: pkg.id, sha256: SHA_A, exitCode: 1, ranAt });

    const row = readRawPackage(pkg.id);
    expect(row.status).toBe("failed");
    expect(row.current_sha256).toBeNull();
    expect(row.installed_at).toBeNull();
    expect(row.last_ran_at).toBe(ranAt);
  });

  test("non-zero exit on an installed row does NOT downgrade status", () => {
    const pkg = findOrCreatePackage({ url: URL_A, slug: SLUG_A });
    const installedRanAt = "2026-05-19T08:00:00.000Z";
    const failedRanAt = "2026-05-20T19:00:00.000Z";

    updatePackageOnExec({
      packageId: pkg.id,
      sha256: SHA_A,
      exitCode: 0,
      ranAt: installedRanAt,
    });
    updatePackageOnExec({
      packageId: pkg.id,
      sha256: SHA_B,
      exitCode: 7,
      ranAt: failedRanAt,
    });

    const row = readRawPackage(pkg.id);
    expect(row.status).toBe("installed");
    expect(row.current_sha256).toBe(SHA_A); // unchanged on failure
    expect(row.installed_at).toBe(installedRanAt);
    expect(row.last_ran_at).toBe(failedRanAt); // still bumped
  });

  test("works inside a caller-owned db.transaction(): changes commit at tx end", () => {
    const pkg = findOrCreatePackage({ url: URL_A, slug: SLUG_A });
    const ranAt = "2026-05-20T12:00:00.000Z";

    ensureDb().transaction(() => {
      updatePackageOnExec({ packageId: pkg.id, sha256: SHA_A, exitCode: 0, ranAt });
    })();

    const row = readRawPackage(pkg.id);
    expect(row.status).toBe("installed");
    expect(row.current_sha256).toBe(SHA_A);
  });
});

// Independent oracle: read the raw snake_case row so a buggy rowToPackage
// can't mask itself in the assertions.
function readRawPackage(id: number) {
  return ensureDb().query("SELECT * FROM packages WHERE id = ?").get(id) as {
    id: number;
    slug: string;
    source_url: string;
    current_sha256: string | null;
    status: string;
    first_seen_at: string;
    installed_at: string | null;
    last_ran_at: string | null;
  };
}
