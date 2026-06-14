/**
 * Tests for `fetchScript` — HTTP GET + sha256 + typed errors.
 *
 * No network. A local `Bun.serve` fixture on an ephemeral port (`port: 0`)
 * provides routes for happy-path, redirects, errors, oversize bodies, and
 * hanging responses. Each test installs its routes in a shared `routes` map
 * keyed by pathname.
 *
 * The 30s production timeout is too slow for tests — we override it via
 * `__setTimeoutMs` (a test-only seam mirroring `db.ts`'s `__resetForTests`
 * convention) and reset it in `afterEach` to avoid bleed between tests.
 */
import { afterAll, afterEach, beforeAll, describe, expect, test } from "bun:test";
import {
  __setTimeoutMs,
  type FetchedScript,
  FetchScriptError,
  fetchScript,
} from "../src/installer/fetch.ts";

// ---- fixture server -------------------------------------------------------

type Handler = (req: Request) => Response | Promise<Response>;
const routes: Record<string, Handler> = {};

let server: ReturnType<typeof Bun.serve>;

beforeAll(() => {
  server = Bun.serve({
    port: 0,
    fetch(req) {
      const u = new URL(req.url);
      const handler = routes[u.pathname];
      return handler ? handler(req) : new Response("not found", { status: 404 });
    },
  });
});

afterAll(() => {
  server.stop(true);
});

afterEach(() => {
  // Wipe registered routes so tests don't leak handlers into each other.
  for (const k of Object.keys(routes)) delete routes[k];
  // Reset timeout to default. A test that sets a tiny timeout must not
  // contaminate the next test, which might depend on the 30s default.
  __setTimeoutMs(30_000);
});

const url = (path: string): string => `http://localhost:${server.port}${path}`;

/** Independent sha256 of the same bytes — proves fetch's hash is correct. */
const sha256Hex = (bytes: Uint8Array): string => {
  const h = new Bun.CryptoHasher("sha256");
  h.update(bytes);
  return h.digest("hex");
};

// =========================================================================
// Happy path
// =========================================================================

describe("happy path", () => {
  test("200 OK returns bytes, sha, finalUrl, status, fetchedAt", async () => {
    const body = new TextEncoder().encode("#!/bin/sh\necho hi\n");
    routes["/install.sh"] = () => new Response(body, { status: 200 });

    const before = Date.now();
    const result: FetchedScript = await fetchScript(url("/install.sh"));
    const after = Date.now();

    expect(new Uint8Array(result.bytes)).toEqual(body);
    expect(result.sha256).toBe(sha256Hex(body));
    expect(result.status).toBe(200);
    expect(result.finalUrl).toBe(url("/install.sh"));

    // fetchedAt is a parseable ISO 8601 (UTC, ending in Z) timestamp, captured
    // around the time of the call.
    expect(result.fetchedAt).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/);
    const ts = Date.parse(result.fetchedAt);
    expect(ts).toBeGreaterThanOrEqual(before);
    expect(ts).toBeLessThanOrEqual(after);
  });

  test("handles non-UTF8 raw bytes correctly", async () => {
    // Bytes that aren't valid UTF-8 — proves we round-trip raw bytes, not text.
    const bytes = Uint8Array.from([0x00, 0xff, 0x7f, 0x80, 0xc3, 0x28]);
    routes["/raw"] = () => new Response(bytes, { status: 200 });

    const r = await fetchScript(url("/raw"));
    expect(new Uint8Array(r.bytes)).toEqual(bytes);
    expect(r.sha256).toBe(sha256Hex(bytes));
  });

  test("sha is deterministic across two fetches of the same body", async () => {
    const body = new TextEncoder().encode("set -e\necho ok\n");
    routes["/twice"] = () => new Response(body, { status: 200 });

    const a = await fetchScript(url("/twice"));
    const b = await fetchScript(url("/twice"));
    expect(a.sha256).toBe(b.sha256);
  });
});

// =========================================================================
// Non-2xx → FetchScriptError("non-2xx")
// =========================================================================

describe("non-2xx", () => {
  test("404 throws FetchScriptError with reason=non-2xx", async () => {
    routes["/missing"] = () => new Response("nope", { status: 404, statusText: "Not Found" });

    try {
      await fetchScript(url("/missing"));
      throw new Error("expected throw");
    } catch (e) {
      expect(e).toBeInstanceOf(FetchScriptError);
      const err = e as FetchScriptError;
      expect(err.reason).toBe("non-2xx");
      expect(err.message).toContain("404");
    }
  });

  test("500 throws FetchScriptError with reason=non-2xx", async () => {
    routes["/boom"] = () =>
      new Response("server error", { status: 500, statusText: "Internal Server Error" });

    try {
      await fetchScript(url("/boom"));
      throw new Error("expected throw");
    } catch (e) {
      expect(e).toBeInstanceOf(FetchScriptError);
      expect((e as FetchScriptError).reason).toBe("non-2xx");
      expect((e as FetchScriptError).message).toContain("500");
    }
  });
});

// =========================================================================
// Redirects
// =========================================================================

describe("redirects", () => {
  test("302 redirect resolves finalUrl to the post-redirect path", async () => {
    const body = new TextEncoder().encode("# final\n");
    routes["/redirect"] = () =>
      new Response(null, { status: 302, headers: { Location: url("/final") } });
    routes["/final"] = () => new Response(body, { status: 200 });

    const r = await fetchScript(url("/redirect"));
    expect(r.status).toBe(200);
    expect(r.finalUrl).toBe(url("/final"));
    expect(new Uint8Array(r.bytes)).toEqual(body);
  });
});

// =========================================================================
// Size limit → FetchScriptError("too-large")
// =========================================================================

describe("body size cap", () => {
  test("6 MiB body throws FetchScriptError with reason=too-large", async () => {
    // Honest 6 MiB body — exercises both the Content-Length pre-check (Bun.serve
    // sets the header) and the post-read fallback. Either catches the oversize.
    const big = new Uint8Array(6 * 1024 * 1024);
    routes["/huge"] = () => new Response(big, { status: 200 });

    try {
      await fetchScript(url("/huge"));
      throw new Error("expected throw");
    } catch (e) {
      expect(e).toBeInstanceOf(FetchScriptError);
      expect((e as FetchScriptError).reason).toBe("too-large");
    }
  });
});

// =========================================================================
// Timeout → FetchScriptError("timeout")
// =========================================================================

describe("timeout", () => {
  test("a server that never responds aborts with reason=timeout", async () => {
    // Handler returns a Promise that never resolves — the AbortController
    // inside fetchScript fires after the (overridden) timeout.
    routes["/hang"] = () => new Promise<Response>(() => {});

    __setTimeoutMs(50);

    try {
      await fetchScript(url("/hang"));
      throw new Error("expected throw");
    } catch (e) {
      expect(e).toBeInstanceOf(FetchScriptError);
      expect((e as FetchScriptError).reason).toBe("timeout");
    }
  });
});

// =========================================================================
// Network error → FetchScriptError("network")
// =========================================================================

describe("network errors", () => {
  test("connection refused (bogus port) throws reason=network", async () => {
    // Port 1 is reserved + unbound on a dev box — connection refused.
    try {
      await fetchScript("http://127.0.0.1:1/whatever");
      throw new Error("expected throw");
    } catch (e) {
      expect(e).toBeInstanceOf(FetchScriptError);
      expect((e as FetchScriptError).reason).toBe("network");
    }
  });
});

// =========================================================================
// External abort signal (the session controller threads cancel into fetch)
// =========================================================================

describe("external abort signal", () => {
  test("a pre-aborted external signal rejects the fetch with a FetchScriptError", async () => {
    // The session controller (step 5) cancels an in-flight fetch by aborting its
    // own controller. Proves the external signal reaches the underlying fetch: a
    // signal already aborted before the call must abort the request immediately
    // (rather than running to completion or hanging). The reason is allowed to be
    // `timeout` — an abort is indistinguishable from the internal timeout abort,
    // and the session layer discards the error anyway (cancel wins).
    const body = new TextEncoder().encode("#!/bin/sh\necho hi\n");
    routes["/abortme"] = () => new Response(body, { status: 200 });

    await expect(
      fetchScript(url("/abortme"), { signal: AbortSignal.abort() }),
    ).rejects.toBeInstanceOf(FetchScriptError);
  });
});
