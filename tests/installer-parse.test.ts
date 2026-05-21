import { describe, expect, test } from "bun:test";
import {
  type InstallCommand,
  type ParseError,
  parseInstallCommand,
} from "../src/installer/parse.ts";

// ---- helpers --------------------------------------------------------------

const ok = (input: string): InstallCommand => {
  const r = parseInstallCommand(input);
  if ("kind" in r) throw new Error(`expected accept, got ${r.kind}: ${r.message}`);
  return r;
};

const refused = (input: string): ParseError => {
  const r = parseInstallCommand(input);
  if (!("kind" in r)) throw new Error(`expected refusal, got ${JSON.stringify(r)}`);
  return r;
};

// =========================================================================
// ACCEPTED
// =========================================================================

describe("accepted: baseline curl | sh", () => {
  test("curl <url> | sh", () => {
    const r = ok("curl https://example.com/install.sh | sh");
    expect(r.envVars).toEqual({});
    expect(r.sudo).toBe(false);
    expect(r.shell).toBe("sh");
    expect(r.scriptArgs).toEqual([]);
    expect(r.url).toBe("https://example.com/install.sh");
  });

  test("curl with -fsSL flags", () => {
    const r = ok("curl -fsSL https://example.com/install.sh | sh");
    expect(r.url).toBe("https://example.com/install.sh");
    expect(r.shell).toBe("sh");
    expect(r.sudo).toBe(false);
  });

  test("curl piped to bash", () => {
    const r = ok("curl -fsSL https://example.com/install.sh | bash");
    expect(r.shell).toBe("bash");
  });

  test("curl piped to zsh", () => {
    const r = ok("curl -fsSL https://example.com/install.sh | zsh");
    expect(r.shell).toBe("zsh");
  });

  test("raw is preserved verbatim (not trimmed)", () => {
    const input = "  curl https://example.com/install.sh | sh  ";
    const r = ok(input);
    expect(r.raw).toBe(input);
  });

  test("url is the http(s) URL on the LHS", () => {
    const r = ok("curl -o /tmp/x.sh https://example.com/x.sh | sh");
    expect(r.url).toBe("https://example.com/x.sh");
  });

  test("http (non-tls) URL accepted", () => {
    const r = ok("curl http://example.com/install.sh | sh");
    expect(r.url).toBe("http://example.com/install.sh");
  });
});

describe("accepted: wget fetcher", () => {
  test("wget -qO- <url> | sh", () => {
    const r = ok("wget -qO- https://example.com/install.sh | sh");
    expect(r.url).toBe("https://example.com/install.sh");
    expect(r.shell).toBe("sh");
  });

  test("wget -O- <url> | bash", () => {
    const r = ok("wget -O- https://example.com/install.sh | bash");
    expect(r.url).toBe("https://example.com/install.sh");
    expect(r.shell).toBe("bash");
  });

  test("wget plain (no -qO-) | sh", () => {
    const r = ok("wget https://example.com/install.sh | sh");
    expect(r.url).toBe("https://example.com/install.sh");
  });
});

describe("accepted: bash -s -- args", () => {
  test("| bash -s -- --to /usr/local/bin", () => {
    const r = ok("curl -fsSL https://example.com/install.sh | bash -s -- --to /usr/local/bin");
    expect(r.shell).toBe("bash");
    expect(r.scriptArgs).toEqual(["--to", "/usr/local/bin"]);
  });

  test("| bash -s -- single arg", () => {
    const r = ok("curl https://example.com/x | bash -s -- v0.5.2");
    expect(r.scriptArgs).toEqual(["v0.5.2"]);
  });

  test("| bash -s -- no args after --", () => {
    const r = ok("curl https://example.com/x | bash -s --");
    expect(r.scriptArgs).toEqual([]);
  });

  test("| bash -s (no --, no args) -> scriptArgs []", () => {
    const r = ok("curl https://example.com/x | bash -s");
    expect(r.scriptArgs).toEqual([]);
  });

  test("| sh -s -- v1.2.3", () => {
    const r = ok("curl https://example.com/install.sh | sh -s -- v1.2.3");
    expect(r.shell).toBe("sh");
    expect(r.scriptArgs).toEqual(["v1.2.3"]);
  });

  test("| bash -- arg1 arg2 (no -s)", () => {
    const r = ok("curl https://example.com/x | bash -- arg1 arg2");
    expect(r.scriptArgs).toEqual(["arg1", "arg2"]);
  });

  test("multiple script args", () => {
    const r = ok("curl https://example.com/x | bash -s -- --foo --bar baz");
    expect(r.scriptArgs).toEqual(["--foo", "--bar", "baz"]);
  });
});

describe("accepted: sudo on runner", () => {
  test("| sudo bash -> sudo=true", () => {
    const r = ok("curl https://example.com/install.sh | sudo bash");
    expect(r.sudo).toBe(true);
    expect(r.shell).toBe("bash");
  });

  test("| sudo sh -> sudo=true", () => {
    const r = ok("curl https://example.com/install.sh | sudo sh");
    expect(r.sudo).toBe(true);
    expect(r.shell).toBe("sh");
  });

  test("| sudo -E bash -> sudo=true (sudo flags ignored)", () => {
    const r = ok("curl https://example.com/install.sh | sudo -E bash");
    expect(r.sudo).toBe(true);
    expect(r.shell).toBe("bash");
  });

  test("| sudo bash -s -- arg -> sudo=true, scriptArgs", () => {
    const r = ok("curl https://example.com/install.sh | sudo bash -s -- --force");
    expect(r.sudo).toBe(true);
    expect(r.scriptArgs).toEqual(["--force"]);
  });
});

describe("accepted: sudo on fetcher is ignored", () => {
  test("sudo curl … | bash -> sudo=false (sudo is on fetcher, moot)", () => {
    const r = ok("sudo curl https://example.com/install.sh | bash");
    expect(r.sudo).toBe(false);
    expect(r.shell).toBe("bash");
  });

  test("sudo wget … | sh -> sudo=false", () => {
    const r = ok("sudo wget -qO- https://example.com/install.sh | sh");
    expect(r.sudo).toBe(false);
  });
});

describe("accepted: env vars", () => {
  test("single env var", () => {
    const r = ok("VERSION=1.4 curl https://example.com/install.sh | sh");
    expect(r.envVars).toEqual({ VERSION: "1.4" });
    expect(r.url).toBe("https://example.com/install.sh");
  });

  test("multiple env vars", () => {
    const r = ok("FOO=bar BAZ=qux curl https://example.com/install.sh | sh");
    expect(r.envVars).toEqual({ FOO: "bar", BAZ: "qux" });
  });

  test("env var with quoted value", () => {
    const r = ok('VERSION="1.4.0" curl https://example.com/install.sh | sh');
    expect(r.envVars).toEqual({ VERSION: "1.4.0" });
  });

  test("env var with single-quoted value", () => {
    const r = ok("VERSION='1.4.0' curl https://example.com/install.sh | sh");
    expect(r.envVars).toEqual({ VERSION: "1.4.0" });
  });

  test("env var with underscores", () => {
    const r = ok("BUN_INSTALL=/opt/bun curl https://example.com/install.sh | bash");
    expect(r.envVars).toEqual({ BUN_INSTALL: "/opt/bun" });
  });

  test("env var with empty value", () => {
    const r = ok("FOO= curl https://example.com/install.sh | sh");
    expect(r.envVars).toEqual({ FOO: "" });
  });

  test("env var key starting with underscore", () => {
    const r = ok("_X=1 curl https://example.com/install.sh | sh");
    expect(r.envVars).toEqual({ _X: "1" });
  });

  test("env var with numeric chars after first letter", () => {
    const r = ok("V1_BETA=on curl https://example.com/install.sh | sh");
    expect(r.envVars).toEqual({ V1_BETA: "on" });
  });

  test("env var + sudo on runner", () => {
    const r = ok("VERSION=1.0 curl https://example.com/install.sh | sudo bash");
    expect(r.envVars).toEqual({ VERSION: "1.0" });
    expect(r.sudo).toBe(true);
  });
});

describe('accepted: bash -c "$(curl <url>)"', () => {
  test('bash -c "$(curl <url>)"', () => {
    const r = ok('bash -c "$(curl https://example.com/install.sh)"');
    expect(r.shell).toBe("bash");
    expect(r.url).toBe("https://example.com/install.sh");
    expect(r.scriptArgs).toEqual([]);
    expect(r.sudo).toBe(false);
  });

  test('sh -c "$(curl <url>)"', () => {
    const r = ok('sh -c "$(curl https://example.com/install.sh)"');
    expect(r.shell).toBe("sh");
    expect(r.url).toBe("https://example.com/install.sh");
  });

  test('bash -c "$(curl -fsSL <url>)"', () => {
    const r = ok('bash -c "$(curl -fsSL https://example.com/install.sh)"');
    expect(r.url).toBe("https://example.com/install.sh");
  });

  test('bash -c "$(wget -qO- <url>)"', () => {
    const r = ok('bash -c "$(wget -qO- https://example.com/install.sh)"');
    expect(r.url).toBe("https://example.com/install.sh");
  });
});

describe("accepted: bash <(curl <url>)", () => {
  test("bash <(curl <url>)", () => {
    const r = ok("bash <(curl https://example.com/install.sh)");
    expect(r.shell).toBe("bash");
    expect(r.url).toBe("https://example.com/install.sh");
  });

  test("bash <(curl -fsSL <url>)", () => {
    const r = ok("bash <(curl -fsSL https://example.com/install.sh)");
    expect(r.url).toBe("https://example.com/install.sh");
    expect(r.shell).toBe("bash");
  });

  test("sh <(wget -qO- <url>)", () => {
    const r = ok("sh <(wget -qO- https://example.com/install.sh)");
    expect(r.shell).toBe("sh");
    expect(r.url).toBe("https://example.com/install.sh");
  });
});

// =========================================================================
// REAL-WORLD CORPUS
// =========================================================================

describe("real-world: ollama", () => {
  test("curl -fsSL https://ollama.com/install.sh | sh", () => {
    const r = ok("curl -fsSL https://ollama.com/install.sh | sh");
    expect(r.url).toBe("https://ollama.com/install.sh");
    expect(r.shell).toBe("sh");
    expect(r.sudo).toBe(false);
    expect(r.envVars).toEqual({});
    expect(r.scriptArgs).toEqual([]);
  });
});

describe("real-world: bun", () => {
  test("curl -fsSL https://bun.sh/install | bash", () => {
    const r = ok("curl -fsSL https://bun.sh/install | bash");
    expect(r.url).toBe("https://bun.sh/install");
    expect(r.shell).toBe("bash");
  });

  test("curl -fsSL https://get.bun.sh | bash", () => {
    const r = ok("curl -fsSL https://get.bun.sh | bash");
    expect(r.url).toBe("https://get.bun.sh");
    expect(r.shell).toBe("bash");
  });

  test("BUN_INSTALL=$HOME/.bun curl … | bash (env at front)", () => {
    const r = ok("BUN_INSTALL=$HOME/.bun curl -fsSL https://bun.sh/install | bash");
    expect(r.envVars).toEqual({ BUN_INSTALL: "$HOME/.bun" });
    expect(r.url).toBe("https://bun.sh/install");
    expect(r.shell).toBe("bash");
  });
});

describe("real-world: nvm", () => {
  test("curl -o- <url> | bash", () => {
    const r = ok("curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash");
    expect(r.url).toBe("https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh");
    expect(r.shell).toBe("bash");
  });
});

describe("real-world: mise", () => {
  test("curl https://mise.run | sh", () => {
    const r = ok("curl https://mise.run | sh");
    expect(r.url).toBe("https://mise.run");
    expect(r.shell).toBe("sh");
  });
});

describe("real-world: deno", () => {
  test("curl -fsSL https://deno.land/install.sh | sh", () => {
    const r = ok("curl -fsSL https://deno.land/install.sh | sh");
    expect(r.url).toBe("https://deno.land/install.sh");
    expect(r.shell).toBe("sh");
  });
});

describe("real-world: rustup", () => {
  test("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh", () => {
    const r = ok("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh");
    expect(r.url).toBe("https://sh.rustup.rs");
    expect(r.shell).toBe("sh");
  });
});

describe("real-world: starship", () => {
  test("curl -sS https://starship.rs/install.sh | sh", () => {
    const r = ok("curl -sS https://starship.rs/install.sh | sh");
    expect(r.url).toBe("https://starship.rs/install.sh");
    expect(r.sudo).toBe(false);
  });
});

describe("real-world: oh-my-zsh", () => {
  test('sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)"', () => {
    const r = ok(
      'sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)"',
    );
    expect(r.shell).toBe("sh");
    expect(r.url).toBe("https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh");
  });
});

describe("real-world: fnm", () => {
  test("curl -fsSL https://fnm.vercel.app/install | bash", () => {
    const r = ok("curl -fsSL https://fnm.vercel.app/install | bash");
    expect(r.url).toBe("https://fnm.vercel.app/install");
    expect(r.shell).toBe("bash");
  });
});

describe("real-world: pnpm", () => {
  test("curl -fsSL https://get.pnpm.io/install.sh | sh - (trailing dash)", () => {
    const r = ok("curl -fsSL https://get.pnpm.io/install.sh | sh -");
    expect(r.url).toBe("https://get.pnpm.io/install.sh");
    expect(r.shell).toBe("sh");
    // The trailing `-` is treated as a script arg (POSIX positional, '-' is stdin sentinel).
    expect(r.scriptArgs).toEqual(["-"]);
  });
});

describe("real-world: atuin", () => {
  test("curl --proto '=https' --tlsv1.2 -LsSf https://setup.atuin.sh | sh", () => {
    const r = ok("curl --proto '=https' --tlsv1.2 -LsSf https://setup.atuin.sh | sh");
    expect(r.url).toBe("https://setup.atuin.sh");
    expect(r.shell).toBe("sh");
  });
});

describe("real-world: eza", () => {
  test("curl -sSf https://eza.rocks/install.sh | sh", () => {
    const r = ok("curl -sSf https://eza.rocks/install.sh | sh");
    expect(r.url).toBe("https://eza.rocks/install.sh");
  });
});

describe("real-world: just", () => {
  test("curl -sSf https://just.systems/install.sh | bash -s -- --to ~/bin", () => {
    const r = ok("curl -sSf https://just.systems/install.sh | bash -s -- --to ~/bin");
    expect(r.url).toBe("https://just.systems/install.sh");
    expect(r.shell).toBe("bash");
    expect(r.scriptArgs).toEqual(["--to", "~/bin"]);
  });
});

describe("real-world: lazygit", () => {
  test("curl -fsSL …/lazygit/master/install.sh | bash", () => {
    const r = ok(
      "curl -fsSL https://raw.githubusercontent.com/jesseduffield/lazygit/master/install.sh | bash",
    );
    expect(r.url).toBe("https://raw.githubusercontent.com/jesseduffield/lazygit/master/install.sh");
    expect(r.shell).toBe("bash");
  });
});

describe("real-world: zoxide via webi", () => {
  test("curl -sS https://webi.sh/zoxide | sh", () => {
    const r = ok("curl -sS https://webi.sh/zoxide | sh");
    expect(r.url).toBe("https://webi.sh/zoxide");
    expect(r.shell).toBe("sh");
  });
});

describe("real-world: pyenv", () => {
  test("curl https://pyenv.run | bash", () => {
    const r = ok("curl https://pyenv.run | bash");
    expect(r.url).toBe("https://pyenv.run");
    expect(r.shell).toBe("bash");
  });
});

// =========================================================================
// REFUSED
// =========================================================================

describe("refused: empty", () => {
  test("empty string", () => {
    expect(refused("").kind).toBe("empty");
  });

  test("only spaces", () => {
    expect(refused("   ").kind).toBe("empty");
  });

  test("only tabs/newlines", () => {
    expect(refused("\t\n  \n").kind).toBe("empty");
  });
});

describe("refused: chain", () => {
  test("&& chained", () => {
    expect(refused("curl https://example.com/x | sh && echo done").kind).toBe("chain");
  });

  test("|| chained", () => {
    expect(refused("curl https://example.com/x | sh || true").kind).toBe("chain");
  });

  test("; chained", () => {
    expect(refused("curl https://example.com/x | sh ; sudo rm -rf /").kind).toBe("chain");
  });

  test("; chained even just trailing", () => {
    expect(refused("curl https://example.com/x | sh ;").kind).toBe("chain");
  });

  test("&& before pipe", () => {
    expect(refused("echo hi && curl https://example.com/x | sh").kind).toBe("chain");
  });

  test("|| before pipe", () => {
    expect(refused("foo || curl https://example.com/x | sh").kind).toBe("chain");
  });

  test("; between two installs", () => {
    expect(refused("curl https://a/x | sh ; curl https://b/y | sh").kind).toBe("chain");
  });

  test("multiple operators (&& and ;)", () => {
    expect(refused("curl https://a/x | sh && echo a ; echo b").kind).toBe("chain");
  });
});

describe("refused: chain operators inside quotes are NOT chains", () => {
  test("&& inside double quotes (env value) is fine", () => {
    const r = ok('FOO="a && b" curl https://example.com/x | sh');
    expect(r.envVars).toEqual({ FOO: "a && b" });
  });

  test("; inside single quotes (env value) is fine", () => {
    const r = ok("FOO='a ; b' curl https://example.com/x | sh");
    expect(r.envVars).toEqual({ FOO: "a ; b" });
  });
});

describe("refused: no-fetcher", () => {
  test("cat /tmp/x | sh", () => {
    expect(refused("cat /tmp/x | sh").kind).toBe("no-fetcher");
  });

  test("echo … | sh", () => {
    expect(refused("echo something | sh").kind).toBe("no-fetcher");
  });

  test("printf … | sh", () => {
    expect(refused("printf 'foo' | sh").kind).toBe("no-fetcher");
  });
});

describe("refused: no-pipe", () => {
  test("curl https://example.com/x (just curl)", () => {
    expect(refused("curl https://example.com/x").kind).toBe("no-pipe");
  });

  test("curl -fsSL https://example.com/x", () => {
    expect(refused("curl -fsSL https://example.com/x").kind).toBe("no-pipe");
  });

  test("wget https://example.com/x", () => {
    expect(refused("wget https://example.com/x").kind).toBe("no-pipe");
  });
});

describe("refused: no-url", () => {
  test("curl | sh", () => {
    expect(refused("curl | sh").kind).toBe("no-url");
  });

  test("curl -fsSL | sh (flags but no URL)", () => {
    expect(refused("curl -fsSL | sh").kind).toBe("no-url");
  });

  test("wget | sh", () => {
    expect(refused("wget | sh").kind).toBe("no-url");
  });

  test('bash -c "$(curl)"', () => {
    expect(refused('bash -c "$(curl)"').kind).toBe("no-url");
  });

  test("bash <(curl)", () => {
    expect(refused("bash <(curl)").kind).toBe("no-url");
  });
});

describe("refused: unsupported", () => {
  test("unknown shell on runner side", () => {
    expect(refused("curl https://example.com/x | dash").kind).toBe("unsupported");
  });

  test("unknown shell on bash-c form", () => {
    expect(refused('fish -c "$(curl https://example.com/x)"').kind).toBe("unsupported");
  });

  test("piped to non-shell command", () => {
    expect(refused("curl https://example.com/x | tee /tmp/x").kind).toBe("unsupported");
  });

  test("multiple pipes (curl | tee | sh)", () => {
    expect(refused("curl https://example.com/x | tee /tmp/x | sh").kind).toBe("unsupported");
  });

  test("backticks form (legacy) — not supported in v0", () => {
    expect(refused("bash -c `curl https://example.com/x`").kind).toBe("unsupported");
  });

  test("nonsense words", () => {
    expect(refused("hello world").kind).toBe("unsupported");
  });

  test("just 'sh'", () => {
    expect(refused("sh").kind).toBe("unsupported");
  });

  test("just 'bash'", () => {
    expect(refused("bash").kind).toBe("unsupported");
  });
});

// =========================================================================
// EDGE CASES
// =========================================================================

describe("edge: whitespace", () => {
  test("leading/trailing whitespace tolerated", () => {
    const r = ok("   curl https://example.com/x | sh   ");
    expect(r.url).toBe("https://example.com/x");
    expect(r.shell).toBe("sh");
  });

  test("multiple spaces between tokens", () => {
    const r = ok("curl    -fsSL    https://example.com/x   |    sh");
    expect(r.url).toBe("https://example.com/x");
  });

  test("tabs as separators", () => {
    const r = ok("curl\t-fsSL\thttps://example.com/x\t|\tsh");
    expect(r.url).toBe("https://example.com/x");
  });

  test("raw preserves leading/trailing whitespace", () => {
    const input = "  curl https://example.com/x | sh\n";
    expect(ok(input).raw).toBe(input);
  });
});

describe("edge: URL extraction", () => {
  test("URL with path and query", () => {
    const r = ok("curl 'https://example.com/x?token=abc&v=1' | sh");
    expect(r.url).toBe("https://example.com/x?token=abc&v=1");
  });

  test("URL with port", () => {
    const r = ok("curl https://example.com:8443/install.sh | sh");
    expect(r.url).toBe("https://example.com:8443/install.sh");
  });

  test("last URL-shaped token wins (if curl has -o URL or similar)", () => {
    // Real installs don't do this, but in case curl had an earlier URL-shaped arg,
    // the script URL is the last http(s) token before the pipe.
    const r = ok("curl -L https://example.com/install.sh | sh");
    expect(r.url).toBe("https://example.com/install.sh");
  });
});

describe("edge: defaults pinned", () => {
  test("envVars defaults to {}", () => {
    expect(ok("curl https://example.com/x | sh").envVars).toEqual({});
  });

  test("scriptArgs defaults to []", () => {
    expect(ok("curl https://example.com/x | sh").scriptArgs).toEqual([]);
  });

  test("sudo defaults to false", () => {
    expect(ok("curl https://example.com/x | sh").sudo).toBe(false);
  });
});

describe("edge: env vars are at FRONT only", () => {
  test("KEY=VAL in the middle is not an env var", () => {
    // A `KEY=VAL` after curl is a curl arg, not an env var. The whole input is
    // unusual — refuse rather than mis-parse.
    const r = parseInstallCommand("curl FOO=bar https://example.com/x | sh");
    // We don't pin a particular kind here; just that envVars stays {} if accepted.
    if (!("kind" in r)) {
      expect(r.envVars).toEqual({});
    }
  });
});

describe("edge: bash -c shape variants", () => {
  test("env var prefix combined with bash -c form", () => {
    const r = ok('VERSION=1.0 bash -c "$(curl https://example.com/x)"');
    expect(r.envVars).toEqual({ VERSION: "1.0" });
    expect(r.shell).toBe("bash");
    expect(r.url).toBe("https://example.com/x");
  });

  test("sudo bash -c form: sudo=true", () => {
    const r = ok('sudo bash -c "$(curl https://example.com/x)"');
    expect(r.sudo).toBe(true);
    expect(r.shell).toBe("bash");
    expect(r.url).toBe("https://example.com/x");
  });

  test("single-quoted: sh -c '$(curl …)'", () => {
    const r = ok("sh -c '$(curl https://example.com/x)'");
    expect(r.shell).toBe("sh");
    expect(r.url).toBe("https://example.com/x");
  });
});

describe("edge: env-var value containing =", () => {
  test("KEY=a=b: the whole bare-token value is captured", () => {
    const r = ok("KEY=a=b curl https://example.com/x | sh");
    expect(r.envVars).toEqual({ KEY: "a=b" });
  });
});

describe("edge: shell choice", () => {
  test("only sh|bash|zsh accepted as runner shell", () => {
    // ksh is unsupported in v0
    expect(refused("curl https://example.com/x | ksh").kind).toBe("unsupported");
  });
});
