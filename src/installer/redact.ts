/**
 * Blank secret VALUES in a parsed install command's literal text before it is
 * fed into LLM analysis. The single public export, `redactCommand`, returns
 * `cmd.raw` (the verbatim line the user typed/pasted) with every secret value
 * replaced by the literal token `<redacted>` — everything else byte-for-byte
 * intact.
 *
 * Why blank at all: a secret value carries ~zero verdict signal, and shipping
 * the user's own credential to the configured provider is a leak we can cheaply
 * avoid. The matching `InstallCommand` tells us WHICH names/flags are secret;
 * we then locate them in `raw` and span the value that follows.
 *
 * Design rules (all load-bearing):
 *  - **Anchor on the name/flag, NEVER on the value.** We use `envVars`
 *    (name→value) and `scriptArgs` (flat token array) only to decide which
 *    NAMES/FLAGS are secret, then find those anchors in `raw`. Searching for the
 *    value string would be fragile (the same bytes may appear elsewhere) and
 *    would mean carrying the raw secret around as a search needle.
 *  - **A raw secret value must NEVER appear in the output.** The redacted string
 *    is built in one left-to-right pass over `raw`; once a secret span is
 *    located it is replaced before anything is emitted.
 *  - **Quoted values are redacted whole, quotes included** (`TOKEN="sk 123"` →
 *    `TOKEN=<redacted>`). "Minding quoting" means spanning the matching close
 *    quote so an interior space doesn't cut the value short.
 *
 * Secret rule: a value is secret when its env-var NAME or arg FLAG contains,
 * case-insensitively, any of KEY, TOKEN, SECRET, PASS, AUTH, CRED (substring).
 *
 * Accepted misses (deliberate — it's the user's own secret going to a provider
 * they chose; value-shape/entropy detection is deferred): short flags (`-t`),
 * odd names with none of the keywords (`GITHUB_PAT`), and URL/positional
 * secrets are NOT redacted — none of them present a name/flag anchor the
 * keyword list recognizes. Also: a `--flag value` whose value begins with `-`
 * (e.g. `--token -dash`) is left unredacted — it's indistinguishable from the
 * flag being valueless, so we treat it as such.
 */

import type { InstallCommand } from "./parse.ts";

const SECRET_KEYWORDS = ["KEY", "TOKEN", "SECRET", "PASS", "AUTH", "CRED"] as const;
const REDACTED = "<redacted>";

/** Does this env-var name or arg flag contain a secret keyword (case-insensitive)? */
function hasSecretKeyword(name: string): boolean {
  const upper = name.toUpperCase();
  return SECRET_KEYWORDS.some((kw) => upper.includes(kw));
}

/**
 * A half-open `[start, end)` span over `raw` whose contents are a secret value
 * to be replaced by `<redacted>`. Spans are collected, then applied right-to-
 * left so earlier offsets stay valid as the string is spliced.
 */
type Span = { start: number; end: number };

/**
 * Span of a value token beginning at `start` in `raw`. A leading quote spans to
 * its matching close quote (inclusive); otherwise the token runs to the next
 * whitespace. An unterminated quote spans to end-of-string.
 *
 * Mirrors the quoting model of `parse.ts`'s `readValueToken` (bare-until-
 * whitespace, `"…"`/`'…'` to the matching close, unterminated→end), but is
 * span-based over `raw` rather than slice-based: it returns offsets and keeps
 * the quotes, whereas `readValueToken` unwraps the quotes and works on a slice.
 * The contracts differ enough that a shared helper isn't worth it.
 */
function valueSpanAt(raw: string, start: number): Span {
  const first = raw[start];
  if (first === '"' || first === "'") {
    const close = raw.indexOf(first, start + 1);
    const end = close === -1 ? raw.length : close + 1;
    return { start, end };
  }
  let end = start;
  while (end < raw.length && !/\s/.test(raw[end] as string)) end++;
  return { start, end };
}

/**
 * Spans for EVERY occurrence of the env-var assignment `NAME=` in `raw`. Anchors
 * on the literal `NAME=` so the value is never used to find it. A name can
 * appear more than once (`API_KEY=a API_KEY=b`); the `envVars` map collapses
 * duplicates to the last value, but `raw` keeps them all — so we scan the whole
 * string, advancing past each match. Returns [] if the anchor isn't present (the
 * parsed command and `raw` should agree, but redaction must never throw).
 */
function envSecretSpans(raw: string, name: string): Span[] {
  // Match `NAME=` at a token boundary: start-of-string or preceded by
  // whitespace, so a longer name ending in `NAME` can't false-match. Sticky so
  // `lastIndex` walks every occurrence.
  const anchor = new RegExp(`(?:^|\\s)${escapeRegex(name)}=`, "g");
  const spans: Span[] = [];
  let m: RegExpExecArray | null;
  // biome-ignore lint/suspicious/noAssignInExpressions: idiomatic global-exec walk
  while ((m = anchor.exec(raw)) !== null) {
    const eq = m.index + m[0].length - 1; // index of the `=`
    spans.push(valueSpanAt(raw, eq + 1));
    // A zero-width-ish match can't happen here (`=` is consumed), but guard the
    // cursor anyway so a future edit can't loop forever.
    if (anchor.lastIndex <= m.index) anchor.lastIndex = m.index + 1;
  }
  return spans;
}

/**
 * Spans for EVERY occurrence of a secret script-arg flag's value in `raw`.
 * Handles both `--flag=value` (the value after the `=` within the same token)
 * and `--flag value` (the following whitespace-separated value token). A flag
 * with no value of its own — at end-of-string, or whose next token STARTS WITH
 * `-` (a flag-looking value like `--token --password …`, treated as valueless —
 * an accepted miss) — contributes no span. Returns [] if the flag anchor isn't
 * found.
 */
function argSecretSpans(raw: string, flag: string): Span[] {
  // Anchor on the flag at a token boundary. The match may be `--flag` or, for
  // the `=` form, `--flag=` — we look for the flag then inspect what follows.
  // Sticky so `lastIndex` walks every occurrence.
  const anchor = new RegExp(`(?:^|\\s)${escapeRegex(flag)}(=|\\s|$)`, "g");
  const spans: Span[] = [];
  let m: RegExpExecArray | null;
  // biome-ignore lint/suspicious/noAssignInExpressions: idiomatic global-exec walk
  while ((m = anchor.exec(raw)) !== null) {
    const sep = m[1] as string;
    const valueStart = m.index + m[0].length;
    if (sep === "=") {
      // `--flag=value`: value begins right after the `=`.
      spans.push(valueSpanAt(raw, valueStart));
    } else {
      // `--flag value`: the anchor consumed exactly one whitespace char into the
      // separator, but the gap may be wider (`--flag  value`, a tab, …). Skip ALL
      // remaining whitespace so we span the value, not the gap — landing on the
      // gap would insert `<redacted>` into the whitespace and leak the real value.
      let vs = valueStart;
      while (vs < raw.length && /\s/.test(raw[vs] as string)) vs++;
      // Span the value unless the next token STARTS WITH `-` (a flag-looking
      // value is treated as valueless — an accepted miss) or there's nothing
      // left (end-of-string): then this flag carries no value of its own.
      if (vs < raw.length && raw[vs] !== "-") spans.push(valueSpanAt(raw, vs));
    }
    if (anchor.lastIndex <= m.index) anchor.lastIndex = m.index + 1;
  }
  return spans;
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Return `cmd.raw` with every secret value blanked in place. Secret env-var and
 * script-arg values are located via their name/flag anchors and replaced with
 * `<redacted>`; everything else is unchanged. Pure and total — never throws.
 */
export function redactCommand(cmd: InstallCommand): string {
  const spans: Span[] = [];

  // Build the UNIQUE set of secret env-name anchors. A name can repeat in `raw`
  // even though the `envVars` map collapses it — but we scan per unique anchor,
  // so the keys (already unique) need no extra dedup here.
  for (const name of Object.keys(cmd.envVars)) {
    if (!hasSecretKeyword(name)) continue;
    spans.push(...envSecretSpans(cmd.raw, name));
  }

  // scriptArgs is a flat token array: a flag is secret by its own name; its
  // value is either the same token's `=value` or the next token. Dedup the flag
  // anchors so a flag repeated in `scriptArgs` is scanned once (each scan already
  // finds every occurrence in `raw`).
  const seenFlags = new Set<string>();
  for (const tok of cmd.scriptArgs) {
    const flag = tok.includes("=") ? (tok.split("=", 1)[0] as string) : tok;
    if (!flag.startsWith("-")) continue; // values/positionals aren't anchors
    if (!hasSecretKeyword(flag)) continue;
    if (seenFlags.has(flag)) continue;
    seenFlags.add(flag);
    spans.push(...argSecretSpans(cmd.raw, flag));
  }

  if (spans.length === 0) return cmd.raw;

  // Defense-in-depth: sort ascending and merge any overlapping/duplicate spans
  // before splicing. Two anchors can target overlapping bytes (a duplicated
  // flag, or an env name that's a substring of another's value region); merging
  // guarantees the right-to-left splice can't emit mangled `<redacted>dacted>`.
  const merged = mergeSpans(spans);

  // Apply right-to-left so earlier indices remain valid across splices.
  merged.sort((a, b) => b.start - a.start);
  let out = cmd.raw;
  for (const { start, end } of merged) {
    out = out.slice(0, start) + REDACTED + out.slice(end);
  }
  return out;
}

/**
 * Coalesce overlapping or duplicate spans into a minimal non-overlapping set
 * (sorted ascending). Adjacent-but-disjoint spans are kept separate so each gets
 * its own `<redacted>`; only spans that actually overlap (or duplicate) merge.
 */
function mergeSpans(spans: Span[]): Span[] {
  const sorted = [...spans].sort((a, b) => a.start - b.start || a.end - b.end);
  const out: Span[] = [];
  for (const span of sorted) {
    const last = out[out.length - 1];
    if (last && span.start < last.end) {
      if (span.end > last.end) last.end = span.end;
    } else {
      out.push({ ...span });
    }
  }
  return out;
}
