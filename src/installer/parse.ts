/**
 * Parse a user-pasted install command (`curl … | sh` and a small family of
 * close variants) into the 5-field structure `InstallCommand`, or refuse with
 * a typed `ParseError`.
 *
 * v0 is deliberately narrow: the set of accepted shapes is finite and
 * recognized directly. When in doubt, refuse — mis-parsing a hostile shell
 * fragment is worse than rejecting it. The spec's accepted/refused tables
 * (see `vault/impl-specs/v0.md`) are authoritative.
 *
 * Implementation is plain string scanning + targeted regex — no shell-grammar
 * library. Three top-level shapes:
 *
 *   1. Pipe form:    `[env=val …] [sudo] (curl|wget) … <url> … | [sudo] <shell> [args]`
 *   2. `$()` form:   `[env=val …] [sudo] <shell> -c "$(curl|wget … <url>)"`
 *   3. `<()` form:   `[env=val …] [sudo] <shell> <(curl|wget … <url>)`
 *
 * Chain operators (`&&`, `||`, `;`) anywhere outside quotes refuse with
 * `chain`. We track quote state (single + double) just enough to avoid
 * splitting inside an env-var value; we do NOT implement full shell quoting.
 */

export type InstallCommand = {
  envVars: Record<string, string>;
  sudo: boolean;
  shell: "sh" | "bash" | "zsh";
  scriptArgs: string[];
  url: string;
  raw: string; // verbatim input, NOT trimmed
};

export type ParseError = {
  kind: "empty" | "chain" | "no-fetcher" | "no-pipe" | "no-url" | "unsupported";
  message: string;
};

const SHELLS = new Set(["sh", "bash", "zsh"]);
const FETCHERS = new Set(["curl", "wget"]);
// Strict POSIX-style env var name. Lower-case keys are unusual in install
// commands (and unusual in real-world env vars) — refuse rather than guess.
const ENV_VAR_LINE = /^([A-Z_][A-Z0-9_]*)=(.*)$/;
const URL_RE = /https?:\/\/\S+/g;

export function parseInstallCommand(input: string): InstallCommand | ParseError {
  if (input.trim() === "") {
    return { kind: "empty", message: "empty input" };
  }

  // Chain detection happens on the raw (trimmed) input, with quote-awareness.
  const trimmed = input.trim();
  if (containsChainOperator(trimmed)) {
    return {
      kind: "chain",
      message: "chained commands (&&, ||, ;) are refused — paste a single install command",
    };
  }

  // Pull leading env vars off the front. Returns the remainder + the parsed map.
  const { envVars, rest } = stripLeadingEnvVars(trimmed);

  // Branch on top-level shape. `$()` and `<()` are recognized by substring.
  if (matchesProcessSubstShape(rest)) {
    return parseProcessSubst(rest, envVars, input);
  }
  if (matchesCmdSubstShape(rest)) {
    return parseCmdSubst(rest, envVars, input);
  }
  return parsePipeShape(rest, envVars, input);
}

// =========================================================================
// Chain detection
// =========================================================================

/**
 * Scan for `&&`, `||`, or `;` outside single/double quotes. We don't try to
 * be a real shell — we just need to not split inside `"…"` / `'…'` so that
 * env-var values containing those operators don't false-positive.
 */
function containsChainOperator(s: string): boolean {
  let quote: '"' | "'" | null = null;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (quote) {
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      continue;
    }
    const next = s[i + 1];
    if ((ch === "&" && next === "&") || (ch === "|" && next === "|")) return true;
    if (ch === ";") return true;
  }
  return false;
}

// =========================================================================
// Env var prefix
// =========================================================================

/**
 * Consume leading `KEY=VALUE` tokens from the front of the input. Keys must
 * match `^[A-Z_][A-Z0-9_]*$`. Values may be bare, single-quoted, or
 * double-quoted (quotes are stripped from the stored value). Stops at the
 * first token that doesn't look like a `KEY=…`.
 */
function stripLeadingEnvVars(s: string): { envVars: Record<string, string>; rest: string } {
  const envVars: Record<string, string> = {};
  let rest = s;
  while (true) {
    const m = rest.match(ENV_VAR_LINE);
    if (!m) break;
    const key = m[1] as string;
    const after = m[2] as string;
    const { value, consumed } = readValueToken(after);
    envVars[key] = value;
    rest = after.slice(consumed).replace(/^\s+/, "");
  }
  return { envVars, rest };
}

/**
 * Read one shell-ish "word" off the front of `s`. Supports bare tokens
 * (until whitespace), `"…"`, and `'…'`. Returns the unwrapped value and
 * the number of input chars consumed.
 */
function readValueToken(s: string): { value: string; consumed: number } {
  if (s.length === 0) return { value: "", consumed: 0 };
  const first = s[0];
  if (first === '"' || first === "'") {
    const end = s.indexOf(first, 1);
    if (end === -1) return { value: s.slice(1), consumed: s.length };
    return { value: s.slice(1, end), consumed: end + 1 };
  }
  // Bare: read until whitespace.
  const m = s.match(/^\S*/);
  const tok = m ? m[0] : "";
  return { value: tok, consumed: tok.length };
}

// =========================================================================
// Shapes: bash <(curl …)  and  bash -c "$(curl …)"
// =========================================================================
//
// Same skeleton — only the inner-substitution regex differs. Both extract
// the fetcher invocation from inside a substitution and pull the last URL.

const PROC_SUBST_RE = /^(?:(sudo(?:\s+-\S+)*)\s+)?(sh|bash|zsh)\s+<\(([^)]*)\)\s*$/;
const CMD_SUBST_RE = /^(?:(sudo(?:\s+-\S+)*)\s+)?(sh|bash|zsh)\s+-c\s+["']?\$\(([^)]*)\)["']?\s*$/;

function matchesProcessSubstShape(s: string): boolean {
  return /<\(/.test(s);
}
function matchesCmdSubstShape(s: string): boolean {
  return /-c\s+["']?\$\(/.test(s);
}

function parseProcessSubst(
  s: string,
  envVars: Record<string, string>,
  raw: string,
): InstallCommand | ParseError {
  return parseSubstShape(s, envVars, raw, PROC_SUBST_RE, "process-subst");
}

function parseCmdSubst(
  s: string,
  envVars: Record<string, string>,
  raw: string,
): InstallCommand | ParseError {
  return parseSubstShape(s, envVars, raw, CMD_SUBST_RE, "$()");
}

function parseSubstShape(
  s: string,
  envVars: Record<string, string>,
  raw: string,
  shapeRe: RegExp,
  label: string,
): InstallCommand | ParseError {
  const m = s.match(shapeRe);
  if (!m) return { kind: "unsupported", message: `cannot recognize ${label} shape: ${s}` };
  const sudo = !!m[1];
  const shell = m[2] as InstallCommand["shell"];
  const inner = m[3] as string;
  const urlOrErr = extractUrlFromInner(inner);
  if (typeof urlOrErr !== "string") return urlOrErr;
  return { envVars, sudo, shell, scriptArgs: [], url: urlOrErr, raw };
}

/**
 * Inner of a `$(...)` or `<(...)` — the fetcher invocation. We don't try to
 * fully parse its flags; we just check there's a fetcher word and pull the
 * last http(s) token. (Spec: we re-fetch ourselves, so flags don't matter.)
 */
function extractUrlFromInner(inner: string): string | ParseError {
  const tokens = inner.trim().split(/\s+/).filter(Boolean);
  if (tokens.length === 0 || !FETCHERS.has(tokens[0] as string)) {
    return { kind: "unsupported", message: `expected curl or wget inside substitution: ${inner}` };
  }
  const url = lastUrl(inner);
  if (!url) return { kind: "no-url", message: "no URL found in install command" };
  return url;
}

// =========================================================================
// Shape: pipe (the common case)
// =========================================================================

function parsePipeShape(
  rest: string,
  envVars: Record<string, string>,
  raw: string,
): InstallCommand | ParseError {
  // Find the (single) pipe. Multiple pipes → unsupported.
  const pipeIdxs = findTopLevelPipes(rest);
  if (pipeIdxs.length === 0) {
    return classifyNoPipe(rest);
  }
  if (pipeIdxs.length > 1) {
    return { kind: "unsupported", message: "multiple pipes are not supported" };
  }
  const pipeIdx = pipeIdxs[0] as number;
  const lhs = rest.slice(0, pipeIdx).trim();
  const rhsRaw = rest.slice(pipeIdx + 1).trim();

  // LHS: optional `sudo …` then a fetcher token, then args containing a URL.
  const lhsTokens = lhs.split(/\s+/).filter(Boolean);
  // Strip a leading `sudo` (and any -flag args) — sudo on the fetcher is moot.
  let i = 0;
  if (lhsTokens[i] === "sudo") {
    i++;
    while (i < lhsTokens.length && lhsTokens[i]?.startsWith("-")) i++;
  }
  const fetcher = lhsTokens[i];
  if (!fetcher || !FETCHERS.has(fetcher)) {
    return {
      kind: "no-fetcher",
      message: `left side of pipe must start with curl or wget: ${lhs}`,
    };
  }
  const url = lastUrl(lhs);
  if (!url) return { kind: "no-url", message: "no URL found in install command" };

  // RHS: optional `sudo …`, then sh|bash|zsh, then optional `-s` `--` args.
  const rhsTokens = rhsRaw.split(/\s+/).filter(Boolean);
  let j = 0;
  const sudo = rhsTokens[j] === "sudo";
  if (sudo) {
    j++;
    while (j < rhsTokens.length && rhsTokens[j]?.startsWith("-")) j++;
  }
  const shellTok = rhsTokens[j];
  if (!shellTok || !SHELLS.has(shellTok)) {
    return {
      kind: "unsupported",
      message: `right side of pipe must be sh, bash, or zsh: ${rhsRaw}`,
    };
  }
  const shell = shellTok as InstallCommand["shell"];
  j++;
  // Optional `-s`.
  if (rhsTokens[j] === "-s") j++;
  // Optional `--` separator.
  if (rhsTokens[j] === "--") j++;
  // Everything after is scriptArgs.
  const scriptArgs = rhsTokens.slice(j);

  return { envVars, sudo, shell, scriptArgs, url, raw };
}

/**
 * Pipes outside quotes. Depends on `containsChainOperator` having already
 * rejected `||` upstream — the neighbor-not-`|` check is defense-in-depth
 * so a stray `||` doesn't silently classify as "no pipes found" if chain
 * detection is ever reordered.
 */
function findTopLevelPipes(s: string): number[] {
  const out: number[] = [];
  let quote: '"' | "'" | null = null;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (quote) {
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      continue;
    }
    if (ch === "|" && s[i + 1] !== "|" && s[i - 1] !== "|") out.push(i);
  }
  return out;
}

/**
 * No pipe in input. Classify the failure: was there a fetcher (so it's
 * just missing the pipe), or is the whole shape unrecognized?
 */
function classifyNoPipe(s: string): ParseError {
  const tokens = s.split(/\s+/).filter(Boolean);
  let i = 0;
  if (tokens[i] === "sudo") {
    i++;
    while (i < tokens.length && tokens[i]?.startsWith("-")) i++;
  }
  if (tokens[i] && FETCHERS.has(tokens[i] as string)) {
    return { kind: "no-pipe", message: "expected `<fetcher> <url> | <shell>` but found no pipe" };
  }
  return { kind: "unsupported", message: `unrecognized install command shape: ${s}` };
}

// =========================================================================
// URL extraction
// =========================================================================

/**
 * Last `http(s)://…` token in `s`, with surrounding quote chars trimmed.
 * Real installers occasionally wrap the URL in single quotes (`'https://…'`)
 * to placate shell quoting — strip those.
 */
function lastUrl(s: string): string | null {
  const matches = s.match(URL_RE);
  if (!matches || matches.length === 0) return null;
  let url = matches[matches.length - 1] as string;
  // Strip trailing quote/paren characters that might've gotten captured by \S+.
  url = url.replace(/[)'"`]+$/, "");
  return url;
}
