import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { ensureSweepHome, sweepFs } from "../src/fs.ts";
import { TEST_HOME } from "./sweep-home-preload.ts";

describe("sweepFs", () => {
  test("root matches SWEEP_HOME from preload", () => {
    expect(sweepFs.root).toBe(TEST_HOME);
  });

  test("ensureSweepHome creates the dir and is idempotent", () => {
    ensureSweepHome();
    expect(existsSync(sweepFs.root)).toBe(true);
    ensureSweepHome();
    expect(existsSync(sweepFs.root)).toBe(true);
  });

  test("write/read round-trip under sweepFs.root", () => {
    ensureSweepHome();
    sweepFs.write("x", "hi");
    expect(sweepFs.read("x")).toBe("hi");
  });
});
