import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { sweepFs } from "../src/fs.ts";
import { __resetForTests, ensureDb } from "../src/store/db.ts";

describe("ensureDb", () => {
  test("provisions DB file, schema, indexes, and schema_meta row", () => {
    const db = ensureDb();

    expect(existsSync(sweepFs.resolve("sweep.db"))).toBe(true);

    const tables = (
      db.query("SELECT name FROM sqlite_master WHERE type='table'").all() as { name: string }[]
    ).map((r) => r.name);
    expect(tables).toEqual(expect.arrayContaining(["packages", "invocations", "schema_meta"]));

    const indexes = (
      db
        .query("SELECT name FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%'")
        .all() as { name: string }[]
    ).map((r) => r.name);
    expect(indexes).toEqual(
      expect.arrayContaining(["packages_sha", "invocations_pkg", "invocations_ts"]),
    );

    const meta = db.query("SELECT version FROM schema_meta").all() as { version: number }[];
    expect(meta).toEqual([{ version: 1 }]);
  });

  test("returns the same singleton on repeated calls", () => {
    expect(ensureDb()).toBe(ensureDb());
  });

  test("WAL journal mode is on", () => {
    const row = ensureDb().query("PRAGMA journal_mode").get() as { journal_mode: string };
    expect(row.journal_mode).toBe("wal");
  });

  test("foreign keys are on", () => {
    const row = ensureDb().query("PRAGMA foreign_keys").get() as { foreign_keys: number };
    expect(row.foreign_keys).toBe(1);
  });

  test("re-opening an existing DB does not duplicate schema_meta", () => {
    ensureDb();
    __resetForTests();
    const meta = ensureDb().query("SELECT version FROM schema_meta").all() as {
      version: number;
    }[];
    expect(meta).toEqual([{ version: 1 }]);
  });
});
