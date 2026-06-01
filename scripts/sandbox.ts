/**
 * Disposable Linux box for exercising real installers through the
 * locally-built `sweep` (and `wrap`). The box mounts the host `dist/` dirs
 * read-only; `sweep`/`wrap`/`w` inside resolve (via image symlinks) to the
 * mounted `*-linux-arm64` binaries, so whatever `build:watch` last produced
 * is what runs. This script is NOT a build tool — it assumes the binaries
 * already exist and only manages the container.
 *
 *   bun run sandbox        up:   build image if needed, create/start the
 *                                container, exec an interactive shell.
 *   bun run sandbox down   stop the container (in-box state preserved).
 *   bun run sandbox kill   destroy the container + its in-box ~/.sweep.
 *
 * One container per working copy: the name is keyed to the git toplevel, so
 * main and each worktree get an independent box, runnable in parallel.
 */

import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

const IMAGE = "sweep-sandbox";
const DOCKERFILE = join(import.meta.dir, "sandbox.Dockerfile");
const WRAP_DIST = join(homedir(), "mysite", "wrap", "dist");

/** Run a command, capture stdout, never inherit. For queries. */
function query(cmd: string[]): { code: number; out: string } {
  const p = Bun.spawnSync(cmd, { stdout: "pipe", stderr: "pipe" });
  return { code: p.exitCode, out: p.stdout.toString().trim() };
}

/** Run a command with inherited stdio. For interactive/visible commands. */
async function run(cmd: string[]): Promise<number> {
  const p = Bun.spawn(cmd, { stdin: "inherit", stdout: "inherit", stderr: "inherit" });
  return await p.exited;
}

/** Git toplevel of the working copy (falls back to cwd outside a repo). */
function workingCopyRoot(): string {
  const r = query(["git", "rev-parse", "--show-toplevel"]);
  return r.code === 0 && r.out ? r.out : process.cwd();
}

function containerName(root: string): string {
  const hash = createHash("sha256").update(root).digest("hex").slice(0, 8);
  return `${IMAGE}-${hash}`;
}

/** "absent" | "running" | "stopped" */
function containerState(name: string): "absent" | "running" | "stopped" {
  const r = query(["docker", "container", "inspect", "-f", "{{.State.Running}}", name]);
  if (r.code !== 0) return "absent";
  return r.out === "true" ? "running" : "stopped";
}

function imageExists(): boolean {
  return query(["docker", "image", "inspect", IMAGE]).code === 0;
}

async function ensureImage(): Promise<void> {
  if (imageExists()) return;
  console.error("sandbox: building image (first run, one-time)…");
  // The Dockerfile COPYs nothing, so the build context is irrelevant — pass
  // the scripts dir (small) rather than the whole repo.
  const code = await run(["docker", "build", "-t", IMAGE, "-f", DOCKERFILE, dirname(DOCKERFILE)]);
  if (code !== 0) throw new Error("image build failed");
}

function warnIfMissing(path: string, hint: string): void {
  if (!existsSync(path)) console.error(`sandbox: ${path} missing — ${hint}`);
}

async function up(root: string, name: string): Promise<number> {
  const sweepDist = join(root, "dist");
  warnIfMissing(
    join(sweepDist, "sweep-linux-arm64"),
    "run `bun run build:watch` (or `bun run build`) so `sweep` exists in the box.",
  );
  warnIfMissing(
    join(WRAP_DIST, "wrap-linux-arm64"),
    "build wrap (`bun run build` in ~/mysite/wrap) so `w`/`wrap` exist in the box.",
  );

  await ensureImage();

  const state = containerState(name);
  if (state === "absent") {
    // --init: tini as PID 1 forwards SIGTERM to `sleep`, so `down`
    // (docker stop) returns immediately instead of waiting out the 10s grace
    // period and SIGKILLing.
    const code = await run([
      "docker",
      "run",
      "-d",
      "--init",
      "--name",
      name,
      "-v",
      `${sweepDist}:/sweep-bin:ro`,
      "-v",
      `${WRAP_DIST}:/wrap-bin:ro`,
      IMAGE,
      "sleep",
      "infinity",
    ]);
    if (code !== 0) return code;
  } else if (state === "stopped") {
    const code = await run(["docker", "start", name]);
    if (code !== 0) return code;
  }

  // Interactive shell. Drop -t when stdin isn't a TTY (e.g. piped/CI) so
  // `docker exec` doesn't error out.
  const tty = process.stdin.isTTY ? "-it" : "-i";
  return await run(["docker", "exec", tty, "-w", "/home/dev", name, "bash"]);
}

async function down(name: string): Promise<number> {
  if (containerState(name) === "absent") {
    console.error("sandbox: no container to stop.");
    return 0;
  }
  return await run(["docker", "stop", name]);
}

async function kill(name: string): Promise<number> {
  if (containerState(name) === "absent") {
    console.error("sandbox: no container to kill.");
    return 0;
  }
  return await run(["docker", "rm", "-f", name]);
}

const verb = process.argv[2] ?? "up";
const root = workingCopyRoot();
const name = containerName(root);

switch (verb) {
  case "up":
    process.exitCode = await up(root, name);
    break;
  case "down":
    process.exitCode = await down(name);
    break;
  case "kill":
    process.exitCode = await kill(name);
    break;
  default:
    console.error(`sandbox: unknown verb "${verb}" (use: up | down | kill)`);
    process.exitCode = 2;
}
