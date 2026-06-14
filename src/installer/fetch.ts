/**
 * Fetch an install script over HTTP and return the bytes plus a content hash.
 *
 * Used by the installer to materialize the script bytes that go into the CAS
 * (`store/scripts.ts`) and onto the child shell's stdin (`installer/exec.ts`).
 *
 * Contract (per spec `vault/impl-specs/v0.md` → `src/installer/fetch.ts`):
 *
 *   - Bun's global `fetch`, redirects followed by default.
 *   - 30s end-to-end timeout via a single `AbortController` covering BOTH the
 *     headers phase and the body read. Override from tests via `__setTimeoutMs`
 *     (same convention as `db.ts:__resetForTests`). Without covering the body
 *     read a slow-loris server could trickle bytes forever.
 *   - An optional EXTERNAL `signal` (the install session's cancel) is OR'd into
 *     that same controller: when either the timeout or the caller's signal fires,
 *     the in-flight `fetch`/body-read aborts. An external abort is classified the
 *     same as a timeout (the session layer discards it — cancel wins).
 *   - 5 MiB body cap. Refuse early via `Content-Length` if the server is honest;
 *     otherwise enforce post-read. Install scripts are tiny; an unbounded read
 *     is a footgun.
 *   - sha256 via `Bun.CryptoHasher`.
 *   - All failure modes throw `FetchScriptError` with a discriminant `reason`
 *     so the orchestrator can build a diagnostic `error_message` for the
 *     `fetch_failed` invocation row.
 */

export type FetchedScript = {
  bytes: Uint8Array;
  sha256: string;
  finalUrl: string; // post-redirect
  fetchedAt: string; // ISO 8601 UTC
  status: number; // HTTP status of the final response
};

/** Typed error so the orchestrator can distinguish fetch failures from other throws. */
export class FetchScriptError extends Error {
  constructor(
    public reason: "non-2xx" | "timeout" | "too-large" | "network",
    message: string,
  ) {
    super(message);
    this.name = "FetchScriptError";
  }
}

const MAX_BYTES = 5 * 1024 * 1024; // 5 MiB
const DEFAULT_TIMEOUT_MS = 30_000;

let timeoutMs = DEFAULT_TIMEOUT_MS;

/** Test-only seam: override the request timeout. Mirrors `db.ts:__resetForTests`. */
export function __setTimeoutMs(ms: number): void {
  timeoutMs = ms;
}

export async function fetchScript(
  url: string,
  opts?: { signal?: AbortSignal },
): Promise<FetchedScript> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);

  // Fold the caller's external signal into our controller so a session cancel
  // aborts the in-flight request. Already-aborted → abort immediately.
  const external = opts?.signal;
  const onExternalAbort = () => controller.abort();
  if (external?.aborted) {
    controller.abort();
  } else {
    external?.addEventListener("abort", onExternalAbort, { once: true });
  }

  try {
    let response: Response;
    try {
      response = await fetch(url, { signal: controller.signal });
    } catch (e) {
      throw classifyFetchError(e, controller, url, "fetching");
    }

    // Wall-clock AFTER headers, BEFORE body — matches the spec's
    // "time we got the headers back" semantics.
    const fetchedAt = new Date().toISOString();

    if (!response.ok) {
      // Don't read the body on non-2xx — saves bandwidth on error pages and
      // keeps the error_message compact.
      throw new FetchScriptError(
        "non-2xx",
        `HTTP ${response.status} ${response.statusText}: ${url}`,
      );
    }

    // Honest servers ship a Content-Length — refuse before reading 5 MiB into
    // memory. Servers that lie (or omit the header) still get caught by the
    // post-read check below.
    const declared = Number(response.headers.get("content-length"));
    if (Number.isFinite(declared) && declared > MAX_BYTES) {
      throw new FetchScriptError(
        "too-large",
        `script body ${declared} bytes exceeds ${MAX_BYTES}-byte limit (per Content-Length): ${url}`,
      );
    }

    let buf: ArrayBuffer;
    try {
      buf = await response.arrayBuffer();
    } catch (e) {
      throw classifyFetchError(e, controller, url, "reading body from");
    }

    if (buf.byteLength > MAX_BYTES) {
      throw new FetchScriptError(
        "too-large",
        `script body ${buf.byteLength} bytes exceeds ${MAX_BYTES}-byte limit: ${url}`,
      );
    }

    const bytes = new Uint8Array(buf);
    const hasher = new Bun.CryptoHasher("sha256");
    hasher.update(bytes);
    const sha256 = hasher.digest("hex");

    return {
      bytes,
      sha256,
      finalUrl: response.url,
      fetchedAt,
      status: response.status,
    };
  } finally {
    clearTimeout(timer);
    external?.removeEventListener("abort", onExternalAbort);
  }
}

/**
 * Map a thrown error from `fetch`/`arrayBuffer` to a typed `FetchScriptError`.
 * Checks the error's `name === "AbortError"` first — that's the strongest
 * signal it came from our controller. Falls back to `signal.aborted` for
 * runtimes that throw a generic `Error` on abort. Anything else is a network
 * error wrapping the underlying message.
 */
function classifyFetchError(
  e: unknown,
  controller: AbortController,
  url: string,
  phase: "fetching" | "reading body from",
): FetchScriptError {
  const name = e instanceof Error ? e.name : "";
  if (name === "AbortError" || controller.signal.aborted) {
    return new FetchScriptError("timeout", `request timed out after ${timeoutMs}ms: ${url}`);
  }
  const msg = e instanceof Error ? e.message : String(e);
  return new FetchScriptError("network", `network error ${phase} ${url}: ${msg}`);
}
