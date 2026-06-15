import { describe, expect, test } from "bun:test";
import { ensureDb } from "../src/store/db.ts";
import { insertInvocation } from "../src/store/invocations.ts";
import { findOrCreatePackage } from "../src/store/packages.ts";

const URL_A = "https://ollama.com/install.sh";
const SLUG_A = "ollama";
const SHA_A = "a".repeat(64);
const FINAL_URL_A = "https://cdn.ollama.com/install.sh"; // post-redirect origin
const TS_STARTED = "2026-05-20T12:00:00.000Z";
const TS_FINISHED = "2026-05-20T12:00:05.000Z";
const INSTALL_CMD_JSON = JSON.stringify({
  envVars: {},
  sudo: false,
  shell: "sh",
  scriptArgs: [],
  url: URL_A,
  raw: `curl -fsSL ${URL_A} | sh`,
});

const RAN_ARGS = {
  tsStarted: TS_STARTED,
  tsFinished: TS_FINISHED,
  rawInput: `curl -fsSL ${URL_A} | sh`,
  url: URL_A,
  finalUrl: FINAL_URL_A,
  sha256: SHA_A,
  installCommandJson: INSTALL_CMD_JSON,
  outcome: "ran" as const,
  exitCode: 0,
  errorMessage: null,
};

const PARSE_FAILED_ARGS = {
  packageId: null,
  tsStarted: TS_STARTED,
  tsFinished: TS_STARTED,
  rawInput: "curl … && rm -rf",
  url: null,
  finalUrl: null,
  sha256: null,
  installCommandJson: null,
  outcome: "parse_failed" as const,
  exitCode: null,
  errorMessage: "chained command refused",
};

describe("insertInvocation", () => {
  test("round-trips a fully-populated happy-path invocation", () => {
    const pkg = findOrCreatePackage({ url: URL_A, slug: SLUG_A });

    const inserted = insertInvocation({ packageId: pkg.id, ...RAN_ARGS });

    const row = readRawInvocation(inserted.id);
    expect(row.id).toBe(inserted.id);
    expect(row.package_id).toBe(pkg.id);
    expect(row.ts_started).toBe(TS_STARTED);
    expect(row.ts_finished).toBe(TS_FINISHED);
    expect(row.raw_input).toBe(RAN_ARGS.rawInput);
    expect(row.url).toBe(URL_A);
    expect(row.final_url).toBe(FINAL_URL_A);
    expect(row.sha256).toBe(SHA_A);
    expect(row.install_command_json).toBe(INSTALL_CMD_JSON);
    expect(row.outcome).toBe("ran");
    expect(row.exit_code).toBe(0);
    expect(row.error_message).toBeNull();
  });

  test("generates a UUID when id is not supplied", () => {
    const inv = insertInvocation(PARSE_FAILED_ARGS);

    // RFC 4122 v4 UUID shape — version nibble 4, variant nibble 8/9/a/b.
    expect(inv.id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  });

  test("honors an explicit id when supplied", () => {
    const explicit = "11111111-2222-4333-8444-555555555555";
    const inv = insertInvocation({ id: explicit, ...PARSE_FAILED_ARGS });

    expect(inv.id).toBe(explicit);
    expect(readRawInvocation(explicit).id).toBe(explicit);
  });

  test("returns the materialized camelCase Invocation matching the input plus the id", () => {
    const inv = insertInvocation(PARSE_FAILED_ARGS);
    expect(inv).toEqual({ id: inv.id, ...PARSE_FAILED_ARGS });
  });

  test("persists a parse_failed shape (package_id and most columns null)", () => {
    const inv = insertInvocation(PARSE_FAILED_ARGS);

    const row = readRawInvocation(inv.id);
    expect(row.outcome).toBe("parse_failed");
    expect(row.package_id).toBeNull();
    expect(row.url).toBeNull();
    expect(row.final_url).toBeNull();
    expect(row.sha256).toBeNull();
    expect(row.install_command_json).toBeNull();
    expect(row.exit_code).toBeNull();
    expect(row.error_message).toBe("chained command refused");
    expect(row.raw_input).toBe("curl … && rm -rf");
  });

  test("two inserts produce two distinct rows", () => {
    const a = insertInvocation(PARSE_FAILED_ARGS);
    const b = insertInvocation(PARSE_FAILED_ARGS);

    expect(a.id).not.toBe(b.id);
    const { n } = ensureDb().query("SELECT COUNT(*) AS n FROM invocations").get() as {
      n: number;
    };
    expect(n).toBe(2);
  });

  test("packageId referencing a real package row is persisted; bogus id is rejected by FK", () => {
    const pkg = findOrCreatePackage({ url: URL_A, slug: SLUG_A });

    const ok = insertInvocation({ packageId: pkg.id, ...RAN_ARGS });
    expect(readRawInvocation(ok.id).package_id).toBe(pkg.id);

    expect(() => insertInvocation({ packageId: 999999, ...RAN_ARGS })).toThrow();
  });

  test("works inside a caller-owned db.transaction(): does not open its own", () => {
    const pkg = findOrCreatePackage({ url: URL_A, slug: SLUG_A });

    let inv!: ReturnType<typeof insertInvocation>;
    ensureDb().transaction(() => {
      inv = insertInvocation({ packageId: pkg.id, ...RAN_ARGS });
    })();

    expect(readRawInvocation(inv.id).id).toBe(inv.id);
  });
});

// Independent oracle: SELECT the raw snake_case row so a buggy
// camelCase→snake_case mapping in insertInvocation can't mask itself.
function readRawInvocation(id: string) {
  return ensureDb().query("SELECT * FROM invocations WHERE id = ?").get(id) as {
    id: string;
    package_id: number | null;
    ts_started: string;
    ts_finished: string | null;
    raw_input: string;
    url: string | null;
    final_url: string | null;
    sha256: string | null;
    install_command_json: string | null;
    outcome: string;
    exit_code: number | null;
    error_message: string | null;
  };
}
