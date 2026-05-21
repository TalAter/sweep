import { mkdirSync } from "node:fs";
import { createAppFs } from "wrap-core/fs";

export const sweepFs = createAppFs({ app: "sweep" });

/** Idempotent. Creates ~/.sweep/ if missing. Safe to call every invocation. */
export function ensureSweepHome(): void {
  mkdirSync(sweepFs.root, { recursive: true });
}
