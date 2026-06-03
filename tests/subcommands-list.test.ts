import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { stripAnsi } from "wrap-core/ansi";
import {
  findOrCreatePackage,
  type PackageRow,
  updatePackageOnExec,
} from "../src/store/packages.ts";
import { buildListRows, listCmd } from "../src/subcommands/list.ts";

const logSpy = mock(() => {});
const origConsoleLog = console.log;

/** Capture everything written to stdout (where `printInline` renders the table). */
function captureStdout() {
  const chunks: string[] = [];
  const orig = process.stdout.write.bind(process.stdout);
  process.stdout.write = ((chunk: unknown) => {
    chunks.push(String(chunk));
    return true;
  }) as typeof process.stdout.write;
  return {
    restore: () => {
      process.stdout.write = orig;
    },
    text: () => stripAnsi(chunks.join("")),
  };
}

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

describe("buildListRows", () => {
  const row = (over: Partial<PackageRow>): PackageRow => ({
    id: 1,
    slug: "x",
    sourceUrl: "https://example.com/install.sh",
    currentSha256: null,
    status: "installed",
    firstSeenAt: "2026-01-01T00:00:00.000Z",
    installedAt: "2026-01-01T00:00:00.000Z",
    lastRanAt: "2026-05-20T10:00:00.000Z",
    ...over,
  });

  test("truncates the source URL to its host", () => {
    expect(buildListRows([row({ sourceUrl: "https://github.com/foo/bar" })])[0]?.[1]).toBe(
      "github.com",
    );
  });

  test("strips a leading www. from the host", () => {
    expect(buildListRows([row({ sourceUrl: "https://www.mise.run/x" })])[0]?.[1]).toBe("mise.run");
  });

  test("falls back to the raw string when the URL won't parse", () => {
    expect(buildListRows([row({ sourceUrl: "not a url" })])[0]?.[1]).toBe("not a url");
  });

  test("formats lastRanAt to a date, and null as 'never'", () => {
    expect(buildListRows([row({ lastRanAt: "2026-05-20T10:00:00.000Z" })])[0]?.[3]).toBe(
      "2026-05-20",
    );
    expect(buildListRows([row({ lastRanAt: null })])[0]?.[3]).toBe("never");
  });

  test("emits cells parallel to the columns: slug, host, status, last ran", () => {
    expect(buildListRows([row({ slug: "bun", status: "installed" })])[0]).toEqual([
      "bun",
      "example.com",
      "installed",
      "2026-05-20",
    ]);
  });
});

describe("listCmd", () => {
  test("empty DB prints 'No packages installed.' and returns 0", async () => {
    const code = await listCmd.run([]);
    expect(code).toBe(0);
    expect(lines()).toEqual(["No packages installed."]);
  });

  test("renders an aligned table, one row per installed package, installed_at DESC", async () => {
    const ollama = findOrCreatePackage({ url: "https://ollama.com/install.sh", slug: "ollama" });
    const bun = findOrCreatePackage({ url: "https://bun.sh/install", slug: "bun" });
    const mise = findOrCreatePackage({ url: "https://mise.run", slug: "mise" });

    updatePackageOnExec({
      packageId: ollama.id,
      sha256: "a".repeat(64),
      exitCode: 0,
      ranAt: "2026-05-18T10:00:00.000Z",
    });
    updatePackageOnExec({
      packageId: bun.id,
      sha256: "b".repeat(64),
      exitCode: 0,
      ranAt: "2026-05-19T10:00:00.000Z",
    });
    updatePackageOnExec({
      packageId: mise.id,
      sha256: "c".repeat(64),
      exitCode: 0,
      ranAt: "2026-05-20T10:00:00.000Z",
    });

    const cap = captureStdout();
    let code: number;
    try {
      code = await listCmd.run([]);
    } finally {
      cap.restore();
    }
    expect(code).toBe(0);

    const out = cap.text();
    // Header + hosts (truncated source) + dates, all present.
    expect(out).toContain("PACKAGE");
    expect(out).toContain("SOURCE");
    expect(out).toContain("mise.run");
    expect(out).toContain("bun.sh");
    expect(out).toContain("ollama.com");
    expect(out).toContain("2026-05-20");
    // installed_at DESC: mise (newest) before bun before ollama.
    expect(out.indexOf("mise")).toBeLessThan(out.indexOf("bun"));
    expect(out.indexOf("bun")).toBeLessThan(out.indexOf("ollama"));
  });

  test("non-'installed' rows (attempting, failed) are not rendered", async () => {
    const ok = findOrCreatePackage({ url: "https://bun.sh/install", slug: "bun" });
    updatePackageOnExec({
      packageId: ok.id,
      sha256: "b".repeat(64),
      exitCode: 0,
      ranAt: "2026-05-20T10:00:00.000Z",
    });
    // 'attempting' (never exec'd) and 'failed' (non-zero exec) must not appear.
    findOrCreatePackage({ url: "https://mise.run", slug: "mise" });
    const bad = findOrCreatePackage({ url: "https://broken.example/install.sh", slug: "broken" });
    updatePackageOnExec({
      packageId: bad.id,
      sha256: "d".repeat(64),
      exitCode: 1,
      ranAt: "2026-05-20T11:00:00.000Z",
    });

    const cap = captureStdout();
    try {
      await listCmd.run([]);
    } finally {
      cap.restore();
    }

    const out = cap.text();
    expect(out).toContain("bun.sh");
    expect(out).not.toContain("mise");
    expect(out).not.toContain("broken");
  });
});
