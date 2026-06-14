/**
 * `ensureConfig` — sweep's config seam. It reads `config.jsonc` under sweep
 * home via `sweepFs` and folds in the `SWEEP_CONFIG` JSON env override, handing
 * back the parsed `ProvidersConfig`. Parse failures throw a bare `ConfigError`
 * (main.ts's top-level catch dresses it as a `sweep:` line). The wrap-core
 * `config` module owns the merge/validation mechanics and is tested there; this
 * file pins only sweep's wiring — that ensureConfig is pointed at the right
 * file under sweep home, honors the override var, and propagates the error.
 *
 * Isolation: `tests/sweep-home-preload.ts` pins SWEEP_HOME to a fresh temp dir
 * per test and wipes it in beforeEach, so `sweepFs.write` lands under that dir.
 */

import { afterEach, describe, expect, test } from "bun:test";
import { ConfigError } from "wrap-core/config";
import { ensureConfig } from "../src/config.ts";
import { sweepFs } from "../src/fs.ts";

afterEach(() => {
  delete process.env.SWEEP_CONFIG;
});

describe("ensureConfig", () => {
  test("absent config.jsonc → {}", () => {
    expect(ensureConfig()).toEqual({});
  });

  test("valid config.jsonc under sweep home → parsed providers + defaultProvider", () => {
    sweepFs.write(
      "config.jsonc",
      `{
        // sweep's default analysis provider
        "defaultProvider": "anthropic",
        "providers": {
          "anthropic": { "model": "claude-sonnet-4-6", "apiKey": "sk-x" },
        },
      }`,
    );
    expect(ensureConfig()).toEqual({
      defaultProvider: "anthropic",
      providers: { anthropic: { model: "claude-sonnet-4-6", apiKey: "sk-x" } },
    });
  });

  test("SWEEP_CONFIG env override is folded over the file (env wins top-level)", () => {
    sweepFs.write(
      "config.jsonc",
      `{ "defaultProvider": "anthropic", "providers": { "anthropic": { "model": "a" } } }`,
    );
    process.env.SWEEP_CONFIG = JSON.stringify({ defaultProvider: "openai" });
    expect(ensureConfig()).toEqual({
      defaultProvider: "openai",
      providers: { anthropic: { model: "a" } },
    });
  });

  test("malformed config.jsonc → throws bare ConfigError naming the file", () => {
    sweepFs.write("config.jsonc", "{ not valid");
    expect(() => ensureConfig()).toThrow(ConfigError);
    try {
      ensureConfig();
    } catch (err) {
      const msg = (err as Error).message;
      expect(msg).toBe("config.jsonc contains invalid JSON.");
      // Bare message — main.ts applies sweep's voice centrally.
      expect(msg).not.toMatch(/sweep:/);
    }
  });
});
