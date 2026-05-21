/**
 * Derive a human-readable slug from an install URL.
 *
 * v0 algorithm (hand-rolled, conservative — no PSL, no TLD normalization,
 * no dedup):
 *
 *   1. `new URL(url)`. If construction throws, return `"unknown"`.
 *   2. Take `url.hostname.toLowerCase()`. Empty hostname → `"unknown"`
 *      (would otherwise emit an empty slug and break list rendering).
 *   3. Strip a single leading prefix label if it matches one of the known
 *      delivery prefixes. At most one strip (no recursion).
 *   4. Return the first dot-separated label.
 *
 * The signature is stable across v0 and v1; v1 swaps the body for a
 * registry-backed lookup.
 */

const STRIPPABLE_PREFIXES = ["www.", "get.", "install.", "download.", "dl.", "cdn."] as const;

export function slugFromUrl(url: string): string {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return "unknown";
  }

  let host = parsed.hostname.toLowerCase();
  if (host === "") return "unknown";

  for (const prefix of STRIPPABLE_PREFIXES) {
    if (host.startsWith(prefix)) {
      host = host.slice(prefix.length);
      break;
    }
  }

  const firstLabel = host.split(".")[0];
  return firstLabel || "unknown";
}
