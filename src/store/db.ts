import { Database } from "bun:sqlite";
import { ensureSweepHome, sweepFs } from "../fs.ts";

let db: Database | null = null;

/** Idempotent. Opens the DB (creating it if needed), enables WAL + FKs, runs migrations. */
export function ensureDb(): Database {
  if (db) return db;
  ensureSweepHome(); // bun:sqlite does not create parent dirs
  db = new Database(sweepFs.resolve("sweep.db"), { create: true });
  db.exec("PRAGMA journal_mode = WAL;");
  db.exec("PRAGMA foreign_keys = ON;");
  migrate(db);
  return db;
}

/**
 * Test-only: close and forget the cached handle so the next `ensureDb()` opens
 * a fresh connection. Needed because the preload's `beforeEach` wipes
 * `SWEEP_HOME`, leaving the cached handle pointing at a deleted file.
 */
export function __resetForTests(): void {
  db?.close();
  db = null;
}

function migrate(db: Database): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS schema_meta (
      version INTEGER PRIMARY KEY
    );

    CREATE TABLE IF NOT EXISTS packages (
      id                    INTEGER PRIMARY KEY,
      slug                  TEXT NOT NULL,
      source_url            TEXT NOT NULL UNIQUE,
      current_sha256        TEXT,
      status                TEXT NOT NULL,
      first_seen_at         TEXT NOT NULL,
      installed_at          TEXT,
      last_ran_at           TEXT
    );

    CREATE TABLE IF NOT EXISTS invocations (
      id                    TEXT PRIMARY KEY,
      package_id            INTEGER REFERENCES packages(id),
      ts_started            TEXT NOT NULL,
      ts_finished           TEXT,
      raw_input             TEXT NOT NULL,
      url                   TEXT,
      sha256                TEXT,
      install_command_json  TEXT,
      outcome               TEXT NOT NULL,
      exit_code             INTEGER,
      error_message         TEXT
    );

    CREATE INDEX IF NOT EXISTS packages_sha    ON packages(current_sha256);
    CREATE INDEX IF NOT EXISTS invocations_pkg ON invocations(package_id);
    CREATE INDEX IF NOT EXISTS invocations_ts  ON invocations(ts_started DESC);
  `);
  db.run("INSERT OR IGNORE INTO schema_meta (version) VALUES (1)");
}
