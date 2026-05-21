import { describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { sweepFs } from "../src/fs.ts";
import { readScript, saveScript, scriptPath } from "../src/store/scripts.ts";

const SHA_A = "a".repeat(64);
const SHA_B = "b".repeat(64);

describe("scriptPath", () => {
  test("returns <sweepFs.root>/cache/scripts/<sha>", () => {
    expect(scriptPath(SHA_A)).toBe(join(sweepFs.root, "cache", "scripts", SHA_A));
  });
});

describe("saveScript", () => {
  test("writes bytes to the content-addressed path and creates parent dirs", () => {
    const bytes = new TextEncoder().encode("#!/bin/sh\necho hi\n");
    const path = saveScript(SHA_A, bytes);

    expect(path).toBe(scriptPath(SHA_A));
    expect(existsSync(path)).toBe(true);
    expect(new Uint8Array(readFileSync(path))).toEqual(bytes);
  });

  test("is idempotent: existing slot is preserved, not overwritten", () => {
    const path = scriptPath(SHA_A);
    mkdirSync(dirname(path), { recursive: true });
    const sentinel = Uint8Array.from([0x11, 0x22, 0x33]);
    writeFileSync(path, sentinel);

    // Different bytes — saveScript should detect the slot exists and skip
    // the write entirely. (Real callers honor the CAS contract; this test
    // proves the skip happens regardless.)
    saveScript(SHA_A, Uint8Array.from([0x99, 0x99, 0x99]));

    expect(new Uint8Array(readFileSync(path))).toEqual(sentinel);
  });

  test("two different shas live in separate slots without cross-contamination", () => {
    const bytesA = Uint8Array.from([1, 2, 3]);
    const bytesB = Uint8Array.from([4, 5, 6]);
    saveScript(SHA_A, bytesA);
    saveScript(SHA_B, bytesB);

    expect(new Uint8Array(readFileSync(scriptPath(SHA_A)))).toEqual(bytesA);
    expect(new Uint8Array(readFileSync(scriptPath(SHA_B)))).toEqual(bytesB);
  });
});

describe("readScript", () => {
  test("round-trips arbitrary bytes (including non-UTF8 sequences)", () => {
    const bytes = Uint8Array.from([0x00, 0xff, 0x7f, 0x80, 0xc3, 0x28]);
    saveScript(SHA_A, bytes);
    expect(readScript(SHA_A)).toEqual(bytes);
  });

  test("returns null for an unknown sha", () => {
    expect(readScript(SHA_B)).toBeNull();
  });
});
