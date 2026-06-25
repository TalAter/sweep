/**
 * `redactCommand` — blank secret VALUES in the literal install command before
 * it is fed into LLM analysis. Operates on `cmd.raw` (the verbatim pasted
 * line), located via the name/flag anchors carried by the parsed
 * `InstallCommand` (`envVars` name→value, `scriptArgs` token array) — NEVER by
 * searching for the secret value itself. Everything that isn't a secret value
 * stays byte-for-byte intact.
 *
 * Secret rule: a value is secret when its env-var NAME or arg FLAG contains,
 * case-insensitively, any of KEY, TOKEN, SECRET, PASS, AUTH, CRED (substring).
 * The value is replaced with the literal token `<redacted>`; quoted values are
 * redacted whole, quotes included.
 *
 * The locked invariant under test throughout: a raw secret value must NEVER
 * appear in the output. Fixtures are harmless if executed (`echo`, `true`) per
 * testing.md.
 */

import { describe, expect, test } from "bun:test";
import type { InstallCommand } from "../src/installer/parse.ts";
import { redactCommand } from "../src/installer/redact.ts";

/** Build an InstallCommand with just the fields redaction reads; the rest are
 *  filled with harmless defaults so the type is satisfied. */
function cmd(
  raw: string,
  parts: { envVars?: Record<string, string>; scriptArgs?: string[] } = {},
): InstallCommand {
  return {
    raw,
    envVars: parts.envVars ?? {},
    scriptArgs: parts.scriptArgs ?? [],
    sudo: false,
    shell: "sh",
    url: "https://example.com/install.sh",
  };
}

describe("redactCommand — env var secrets", () => {
  test("bare value: API_TOKEN=sk123 → API_TOKEN=<redacted>, value gone", () => {
    const raw = "API_TOKEN=sk123 sh install.sh";
    const out = redactCommand(cmd(raw, { envVars: { API_TOKEN: "sk123" } }));
    expect(out).toBe("API_TOKEN=<redacted> sh install.sh");
    expect(out).not.toContain("sk123");
  });

  test("double-quoted value with a space is spanned whole, quotes included", () => {
    const raw = 'API_TOKEN="sk 123" sh install.sh';
    const out = redactCommand(cmd(raw, { envVars: { API_TOKEN: "sk 123" } }));
    expect(out).toBe("API_TOKEN=<redacted> sh install.sh");
    expect(out).not.toContain("sk 123");
    expect(out).not.toContain('"sk');
  });

  test("single-quoted value with a space is spanned whole, quotes included", () => {
    const raw = "API_SECRET='hunter 2' sh install.sh";
    const out = redactCommand(cmd(raw, { envVars: { API_SECRET: "hunter 2" } }));
    expect(out).toBe("API_SECRET=<redacted> sh install.sh");
    expect(out).not.toContain("hunter 2");
  });

  test("each keyword anchors redaction: KEY, TOKEN, SECRET, PASS, AUTH, CRED", () => {
    const cases: Array<[string, string]> = [
      ["API_KEY", "v1"],
      ["MY_TOKEN", "v2"],
      ["CLIENT_SECRET", "v3"],
      ["DB_PASSWORD", "v4"],
      ["AUTH_HEADER", "v5"],
      ["AWS_CRED", "v6"],
    ];
    for (const [name, value] of cases) {
      const raw = `${name}=${value} sh install.sh`;
      const out = redactCommand(cmd(raw, { envVars: { [name]: value } }));
      expect(out).toBe(`${name}=<redacted> sh install.sh`);
      expect(out).not.toContain(value);
    }
  });

  test("case-insensitive name match: api_token / Api_Token are redacted", () => {
    // parse.ts only accepts upper-case env names, but redaction's keyword match
    // is case-insensitive by contract — pin it directly on the value carried.
    const lower = redactCommand(
      cmd("api_token=sk9 sh install.sh", { envVars: { api_token: "sk9" } }),
    );
    expect(lower).toBe("api_token=<redacted> sh install.sh");
    expect(lower).not.toContain("sk9");

    const mixed = redactCommand(
      cmd("Api_Token=sk9 sh install.sh", { envVars: { Api_Token: "sk9" } }),
    );
    expect(mixed).toBe("Api_Token=<redacted> sh install.sh");
  });

  test("mixed secret + non-secret env vars: only the secret is blanked", () => {
    const raw = "VERSION=1.2.3 API_KEY=sk123 sh install.sh";
    const out = redactCommand(cmd(raw, { envVars: { VERSION: "1.2.3", API_KEY: "sk123" } }));
    expect(out).toBe("VERSION=1.2.3 API_KEY=<redacted> sh install.sh");
    expect(out).not.toContain("sk123");
    expect(out).toContain("VERSION=1.2.3");
  });

  test("multiple secrets in one command are all blanked", () => {
    const raw = "API_KEY=sk1 DB_PASSWORD=pw2 sh install.sh";
    const out = redactCommand(cmd(raw, { envVars: { API_KEY: "sk1", DB_PASSWORD: "pw2" } }));
    expect(out).toBe("API_KEY=<redacted> DB_PASSWORD=<redacted> sh install.sh");
    expect(out).not.toContain("sk1");
    expect(out).not.toContain("pw2");
  });

  test("env var in the right-hand env position (after the pipe) is redacted", () => {
    const raw = "curl https://example.com/i.sh | API_TOKEN=sk7 sudo -E sh";
    const out = redactCommand(cmd(raw, { envVars: { API_TOKEN: "sk7" } }));
    expect(out).toBe("curl https://example.com/i.sh | API_TOKEN=<redacted> sudo -E sh");
    expect(out).not.toContain("sk7");
  });
});

describe("redactCommand — script arg secrets", () => {
  test("--flag value form: --token sk123 → --token <redacted>", () => {
    const raw = "curl https://example.com/i.sh | sh -s -- --token sk123";
    const out = redactCommand(cmd(raw, { scriptArgs: ["--token", "sk123"] }));
    expect(out).toBe("curl https://example.com/i.sh | sh -s -- --token <redacted>");
    expect(out).not.toContain("sk123");
  });

  test("--flag=value form: --token=sk123 → --token=<redacted>", () => {
    const raw = "curl https://example.com/i.sh | sh -s -- --token=sk123";
    const out = redactCommand(cmd(raw, { scriptArgs: ["--token=sk123"] }));
    expect(out).toBe("curl https://example.com/i.sh | sh -s -- --token=<redacted>");
    expect(out).not.toContain("sk123");
  });

  test("quoted arg value is redacted whole, quotes included", () => {
    const raw = 'curl https://example.com/i.sh | sh -s -- --auth "Bearer x y"';
    const out = redactCommand(cmd(raw, { scriptArgs: ["--auth", "Bearer x y"] }));
    expect(out).toBe("curl https://example.com/i.sh | sh -s -- --auth <redacted>");
    expect(out).not.toContain("Bearer x y");
  });

  test("mixed secret + non-secret args: only the secret flag's value is blanked", () => {
    const raw = "curl https://example.com/i.sh | sh -s -- --to /usr/local/bin --api-key sk5";
    const out = redactCommand(
      cmd(raw, { scriptArgs: ["--to", "/usr/local/bin", "--api-key", "sk5"] }),
    );
    expect(out).toBe(
      "curl https://example.com/i.sh | sh -s -- --to /usr/local/bin --api-key <redacted>",
    );
    expect(out).not.toContain("sk5");
    expect(out).toContain("--to /usr/local/bin");
  });

  test("case-insensitive flag match: --Api-Token value is redacted", () => {
    const raw = "curl https://example.com/i.sh | sh -s -- --Api-Token sk5";
    const out = redactCommand(cmd(raw, { scriptArgs: ["--Api-Token", "sk5"] }));
    expect(out).toBe("curl https://example.com/i.sh | sh -s -- --Api-Token <redacted>");
    expect(out).not.toContain("sk5");
  });

  test("--flag value with TWO spaces: value still redacted, secret gone", () => {
    const raw = "curl https://example.com/i.sh | sh -s -- --token  sk1secret";
    const out = redactCommand(cmd(raw, { scriptArgs: ["--token", "sk1secret"] }));
    expect(out).toBe("curl https://example.com/i.sh | sh -s -- --token  <redacted>");
    expect(out).not.toContain("sk1secret");
  });

  test("--flag value with a TAB separator (space+tab): value still redacted, secret gone", () => {
    const raw = "curl https://example.com/i.sh | sh -s -- --token \tsk1secret";
    const out = redactCommand(cmd(raw, { scriptArgs: ["--token", "sk1secret"] }));
    expect(out).toBe("curl https://example.com/i.sh | sh -s -- --token \t<redacted>");
    expect(out).not.toContain("sk1secret");
  });
});

describe("redactCommand — combined and passthrough", () => {
  test("env + arg secrets together are both blanked in one pass", () => {
    const raw = "API_KEY=sk1 curl https://example.com/i.sh | sh -s -- --token sk2";
    const out = redactCommand(
      cmd(raw, { envVars: { API_KEY: "sk1" }, scriptArgs: ["--token", "sk2"] }),
    );
    expect(out).toBe(
      "API_KEY=<redacted> curl https://example.com/i.sh | sh -s -- --token <redacted>",
    );
    expect(out).not.toContain("sk1");
    expect(out).not.toContain("sk2");
  });

  test("no secrets: raw is returned byte-for-byte unchanged", () => {
    const raw = "VERSION=1.2.3 curl https://example.com/i.sh | sh -s -- --to /usr/local/bin";
    const out = redactCommand(
      cmd(raw, { envVars: { VERSION: "1.2.3" }, scriptArgs: ["--to", "/usr/local/bin"] }),
    );
    expect(out).toBe(raw);
  });
});

describe("redactCommand — repeated anchors (every occurrence redacted)", () => {
  test("repeated secret flag: both --token values are blanked, no corruption", () => {
    const raw = "curl https://example.com/i.sh | sh -s -- --token sk1 --token sk2";
    const out = redactCommand(cmd(raw, { scriptArgs: ["--token", "sk1", "--token", "sk2"] }));
    expect(out).toBe(
      "curl https://example.com/i.sh | sh -s -- --token <redacted> --token <redacted>",
    );
    expect(out).not.toContain("sk1");
    expect(out).not.toContain("sk2");
    expect(out).not.toContain("dacted>dacted>");
    expect(out).not.toContain("<redacted>dacted>");
  });

  test("repeated secret env name: both values are blanked even though map collapses", () => {
    const raw = "API_KEY=sk1 API_KEY=sk2 curl https://example.com/i.sh | sh";
    // envVars is a map — the duplicate name collapses to the LAST value, but raw
    // carries both occurrences and both must be redacted.
    const out = redactCommand(cmd(raw, { envVars: { API_KEY: "sk2" } }));
    expect(out).toBe("API_KEY=<redacted> API_KEY=<redacted> curl https://example.com/i.sh | sh");
    expect(out).not.toContain("sk1");
    expect(out).not.toContain("sk2");
    expect(out).not.toContain("<redacted>dacted>");
  });
});

describe("redactCommand — flag value is itself a flag", () => {
  test("--token --password sk1: --token is valueless, only --password's value blanks", () => {
    const raw = "curl https://example.com/i.sh | sh -s -- --token --password sk1";
    const out = redactCommand(cmd(raw, { scriptArgs: ["--token", "--password", "sk1"] }));
    expect(out).toBe("curl https://example.com/i.sh | sh -s -- --token --password <redacted>");
    expect(out).not.toContain("sk1");
    // --token must remain intact — the next token is a flag, not its value.
    expect(out).toContain("--token --password");
  });
});

describe("redactCommand — accepted misses (pinned current behavior)", () => {
  test("short flag -t is NOT redacted (name list misses short flags)", () => {
    const raw = "curl https://example.com/i.sh | sh -s -- -t sk123";
    const out = redactCommand(cmd(raw, { scriptArgs: ["-t", "sk123"] }));
    expect(out).toBe(raw);
  });

  test("odd env name with no keyword (GITHUB_PAT) is NOT redacted", () => {
    const raw = "GITHUB_PAT=ghp_secret sh install.sh";
    const out = redactCommand(cmd(raw, { envVars: { GITHUB_PAT: "ghp_secret" } }));
    expect(out).toBe(raw);
  });

  test("URL / positional secret is NOT redacted (no name/flag anchor)", () => {
    const raw = "curl https://example.com/i.sh?token=sk123 | sh";
    const out = redactCommand(cmd(raw));
    expect(out).toBe(raw);
    // The user's own secret to a provider they chose — accepted per spec.
    expect(out).toContain("token=sk123");
  });

  test("flag value beginning with '-' is NOT redacted (treated as valueless)", () => {
    // A value that starts with `-` looks like a flag, so the `--flag value` form
    // treats `--token` as valueless and leaves `-dashvalue` intact. Accepted miss.
    const raw = "curl https://example.com/i.sh | sh -s -- --token -dashvalue";
    const out = redactCommand(cmd(raw, { scriptArgs: ["--token", "-dashvalue"] }));
    expect(out).toBe(raw);
    expect(out).toContain("-dashvalue");
  });
});
