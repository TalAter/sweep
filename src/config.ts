/**
 * Sweep's config seam. `ensureConfig` reads `config.jsonc` under sweep home and
 * folds in the `SWEEP_CONFIG` JSON env override, handing back the parsed
 * provider-bearing config. The name and signature are the stable seam — a future
 * version grows the body (e.g. a first-run wizard, mirroring wrap) without
 * changing callers (`main.ts` calls it once at startup).
 *
 * A malformed `config.jsonc` throws a bare `ConfigError`; sweep's "sweep:" voice
 * is applied centrally by `main.ts`'s top-level catch, so we do NOT catch here.
 */

import { loadJsoncConfig, type ProvidersConfig } from "wrap-core/config";
import { sweepFs } from "./fs.ts";

// Sweep's config is currently just the provider-bearing subset; extend later
// (e.g. nerdFonts) when those fields are actually consumed.
export type Config = ProvidersConfig;

export function ensureConfig(): Config {
  return loadJsoncConfig<Config>(sweepFs, "config.jsonc", { envOverrideVar: "SWEEP_CONFIG" });
}
