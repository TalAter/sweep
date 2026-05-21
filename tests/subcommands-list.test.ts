import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { findOrCreatePackage, updatePackageOnExec } from "../src/store/packages.ts";
import { listCmd } from "../src/subcommands/list.ts";

const logSpy = mock(() => {});
const origConsoleLog = console.log;

beforeEach(() => {
  console.log = logSpy;
  logSpy.mockClear();
});

afterEach(() => {
  console.log = origConsoleLog;
});

/** Concatenate all console.log call args into the lines they printed. */
function lines(): string[] {
  return logSpy.mock.calls.map((call) => call.map((a) => String(a)).join(" "));
}

describe("listCmd", () => {
  test("empty DB prints 'No packages installed.' and returns 0", async () => {
    const code = await listCmd.run([]);
    expect(code).toBe(0);
    expect(lines()).toEqual(["No packages installed."]);
  });

  test("prints one tab-separated line per installed package, installed_at DESC", async () => {
    const ollama = findOrCreatePackage({
      url: "https://ollama.com/install.sh",
      slug: "ollama",
    });
    const bun = findOrCreatePackage({ url: "https://bun.sh/install", slug: "bun" });
    const mise = findOrCreatePackage({ url: "https://mise.run", slug: "mise" });

    const olderTs = "2026-05-18T10:00:00.000Z";
    const middleTs = "2026-05-19T10:00:00.000Z";
    const newerTs = "2026-05-20T10:00:00.000Z";

    updatePackageOnExec({
      packageId: ollama.id,
      sha256: "a".repeat(64),
      exitCode: 0,
      ranAt: olderTs,
    });
    updatePackageOnExec({
      packageId: bun.id,
      sha256: "b".repeat(64),
      exitCode: 0,
      ranAt: middleTs,
    });
    updatePackageOnExec({
      packageId: mise.id,
      sha256: "c".repeat(64),
      exitCode: 0,
      ranAt: newerTs,
    });

    const code = await listCmd.run([]);
    expect(code).toBe(0);

    expect(lines()).toEqual([
      `mise\thttps://mise.run\t${newerTs}`,
      `bun\thttps://bun.sh/install\t${middleTs}`,
      `ollama\thttps://ollama.com/install.sh\t${olderTs}`,
    ]);
  });

  test("non-'installed' rows (attempting, failed) are not printed", async () => {
    // 'installed' — should appear.
    const ok = findOrCreatePackage({ url: "https://bun.sh/install", slug: "bun" });
    updatePackageOnExec({
      packageId: ok.id,
      sha256: "b".repeat(64),
      exitCode: 0,
      ranAt: "2026-05-20T10:00:00.000Z",
    });

    // 'attempting' (never exec'd) — should NOT appear.
    findOrCreatePackage({ url: "https://mise.run", slug: "mise" });

    // 'failed' (non-zero exec) — should NOT appear.
    const bad = findOrCreatePackage({ url: "https://broken.example/install.sh", slug: "broken" });
    updatePackageOnExec({
      packageId: bad.id,
      sha256: "d".repeat(64),
      exitCode: 1,
      ranAt: "2026-05-20T11:00:00.000Z",
    });

    const code = await listCmd.run([]);
    expect(code).toBe(0);
    expect(lines()).toEqual(["bun\thttps://bun.sh/install\t2026-05-20T10:00:00.000Z"]);
  });
});
