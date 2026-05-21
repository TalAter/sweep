import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { dispatch } from "../src/subcommands/dispatch.ts";

const logSpy = mock(() => {});
const origConsoleLog = console.log;

beforeEach(() => {
  console.log = logSpy;
  logSpy.mockClear();
});

afterEach(() => {
  console.log = origConsoleLog;
});

describe("dispatch", () => {
  test("routes 'list' to listCmd and returns its exit code", async () => {
    const code = await dispatch("list", []);
    expect(code).toBe(0);
    // Empty DB path: the one captured log is the "No packages installed." line.
    expect(logSpy).toHaveBeenCalled();
  });

  test("throws on unknown verb, mentioning the verb name", async () => {
    expect(dispatch("nope", [])).rejects.toThrow(/nope/);
  });
});
