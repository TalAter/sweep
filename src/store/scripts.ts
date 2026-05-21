import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { sweepFs } from "../fs.ts";

/** Absolute path of the content-addressed slot for `sha256`. No I/O. */
export function scriptPath(sha256: string): string {
  return sweepFs.resolve(`cache/scripts/${sha256}`);
}

/**
 * Idempotent CAS write. Returns the absolute path either way.
 *
 * `wx` (O_EXCL) makes the write atomic against concurrent callers and lets us
 * recognize "slot already exists" as success without a separate stat. Caller's
 * contract: `sha256 === sha256(bytes)` — on an existing slot we keep the bytes
 * on disk untouched, since by CAS they're equal to ours by definition.
 *
 * Bytes I/O goes through `node:fs` directly — `sweepFs.write` is utf-8 text
 * only and would mangle non-UTF8 script bytes.
 */
export function saveScript(sha256: string, bytes: Uint8Array): string {
  const path = scriptPath(sha256);
  try {
    writeFileSync(path, bytes, { flag: "wx" });
  } catch (err) {
    const code = (err as NodeJS.ErrnoException).code;
    if (code === "EEXIST") return path;
    if (code !== "ENOENT") throw err;
    // Parent dir missing — create it and retry. A racing writer that wins
    // between mkdir and our retry surfaces as EEXIST, which is success.
    mkdirSync(dirname(path), { recursive: true });
    try {
      writeFileSync(path, bytes, { flag: "wx" });
    } catch (retryErr) {
      if ((retryErr as NodeJS.ErrnoException).code !== "EEXIST") throw retryErr;
    }
  }
  return path;
}

/** Reads the bytes at the CAS slot. Returns `null` if the slot does not exist. */
export function readScript(sha256: string): Uint8Array | null {
  try {
    return readFileSync(scriptPath(sha256));
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw err;
  }
}
