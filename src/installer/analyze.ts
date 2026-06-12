/**
 * Step 5 of the install pipeline: LLM analysis of the fetched script.
 *
 * Env gate — `SWEEP_TEST_RESPONSES` is sweep's own test-provider contract.
 * wrap-core reads no env vars (test-provider *selection* is consumer
 * policy; playback is core mechanics), so sweep names the variable and
 * builds the config itself. Set → the value becomes canned playback for a
 * `test`-kind provider and the analysis conversation runs for real, end to
 * end. Absent or empty → this function is a strict no-op: `createLlm`
 * never runs, so real installs are untouched this promotion (eager config
 * validation must never throw mid-install). Empty counts as off because an
 * exported-but-empty `SWEEP_TEST_RESPONSES=` must not flip analysis on —
 * it would print a parse-failure line on every install.
 *
 * Env value shape: JSON when parseable — one response or an array of
 * responses (strings or objects; core's `TestResponses`). A non-JSON value
 * is taken verbatim as a single repeating response, which keeps
 * `ERROR:`-prefixed provider-failure playback and malformed-reply fixtures
 * quote-free.
 *
 * Error policy: analysis never fails the install. Every throw is caught,
 * surfaced as one `sweep:` stderr line (chrome, per the stdout-is-payload
 * invariant), and swallowed — exit codes belong to the exec path alone.
 */

import { createLlm, type TestResponses } from "wrap-core/llm";
import { z } from "zod";

// Future default, deliberately unreachable this promotion: a hardcoded
// real provider whose key resolves through core's `$ENV_VAR` indirection.
// To flip analysis on for real users, build this config when
// SWEEP_TEST_RESPONSES is absent (instead of returning early below) — and
// decide then what a missing key does, because `createLlm` throws
// `LlmConfigError` when the named variable is unset. Also pass
// `AbortSignal.timeout(...)` to `send` (core's send accepts a signal) so a
// hung provider can't stall installs.
//
//   const PRODUCTION_CONFIG: LlmConfig = {
//     name: "anthropic",
//     model: "claude-sonnet-4-5",
//     apiKey: "$ANTHROPIC_API_KEY",
//   };

const SYSTEM_PROMPT =
  "You analyze shell install scripts before a user runs them. " +
  "Given a script fetched from a URL, reply with JSON containing a single " +
  '"summary" field: 1-3 plain-language sentences covering what the script ' +
  "installs, where it writes, and anything a cautious user should know " +
  "before running it.";

const analysisSchema = z.object({ summary: z.string() });

/** JSON when parseable; otherwise the raw value as a single response. */
function parseCannedResponses(raw: string): TestResponses {
  try {
    return JSON.parse(raw) as TestResponses;
  } catch {
    return raw;
  }
}

/**
 * Analyze the fetched script with an LLM and surface a one-line summary on
 * stderr. No-op without `SWEEP_TEST_RESPONSES` (see module doc). Never
 * throws.
 */
export async function maybeAnalyzeScript(args: {
  url: string;
  scriptBytes: Uint8Array;
}): Promise<void> {
  const canned = process.env.SWEEP_TEST_RESPONSES;
  if (!canned) return;

  try {
    const llm = createLlm({ name: "test", responses: parseCannedResponses(canned) });
    const chat = llm.startConversation({ system: SYSTEM_PROMPT });
    chat.add({
      role: "user",
      content: `Install script fetched from ${args.url}:\n\n${new TextDecoder().decode(args.scriptBytes)}`,
    });
    const { summary } = await chat.send(analysisSchema);
    console.error(`sweep: script analysis: ${summary}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    console.error(`sweep: script analysis failed: ${message}`);
  }
}
