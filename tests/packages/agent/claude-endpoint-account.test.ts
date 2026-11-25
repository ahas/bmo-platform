import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { claudeCode, claudeCodeProvider } from "@agency/agent/claude-code";
import { registeredRoad } from "@agency/agent/claude-road";
import { gatewayUrl } from "@agency/agent/gateway/address";
import { dataPath, sourceRoot } from "@agency/support/roots";
import { scratchDir } from "@agency/test-support/scratch";

import type { AgentAccount, HarnessRun } from "@agency/agent/harness";

// An endpoint account is Claude Code run against another provider's Anthropic-compatible endpoint
// (docs/cache-fallback.md, 「The Claude Code road is a list of accounts」). Two things follow from
// where it stands: the environment the program is handed names that endpoint and nothing else, and
// the program stays on the Claude Code road even though its requests go somewhere else
// (docs/agent-control.md, 「The registered models through Claude Code on the gateway」).

const root = scratchDir("bmo-claude-endpoint-");
const home = join(root, "deepseek-home");
const keyFile = join(root, "deepseek.key");
const base = join(root, "base.md");
const harnessed = join(root, "harnessed.md");

mkdirSync(home, { recursive: true });
mkdirSync(join(root, "plugin"), { recursive: true });
writeFileSync(keyFile, "  sk-not-a-real-key\n");
writeFileSync(base, "the dsh prompt\n");
writeFileSync(harnessed, "the harness, then the rules\n");
for (const name of ["settings.json", "mcp.json", "rules.md"]) writeFileSync(join(root, name), name === "rules.md" ? "the rules\n" : "{}");

function workerRun(overrides: Partial<HarnessRun> = {}): HarnessRun {
  return {
    line: "worker",
    model: "claude-opus-4-6",
    effort: "high",
    systemPromptPath: null,
    toolsShown: null,
    toolsAllowed: null,
    toolsDenied: "",
    mcpConfigPath: join(root, "mcp.json"),
    prompt: "work",
    promptDelivery: "channel",
    resumeSessionId: null,
    keepSession: true,
    hostCommands: true,
    thinking: true,
    answerShape: "unread",
    cwd: sourceRoot(),
    env: {},
    extensions: {
      settingsPath: join(root, "settings.json"),
      pluginDir: join(root, "plugin"),
      agentsJson: "{}",
      rulesPath: join(root, "rules.md"),
      harnessedPath: harnessed,
    },
    ...overrides,
  };
}

function endpointAccount(models: Record<string, string | undefined>): AgentAccount {
  return {
    id: "deepseek",
    name: "DeepSeek",
    credential: {
      kind: "endpoint",
      path: home,
      endpointUrl: "https://api.deepseek.com/anthropic",
      keyFile,
      models,
    },
  };
}

describe("the environment an endpoint account's run is handed", () => {
  test("names its home, base URL, key and model ids, and clears every other credential", async () => {
    const command = await claudeCode.command(workerRun({
      account: endpointAccount({ opus: "deepseek-v4-pro", sonnet: "deepseek-flash", haiku: "deepseek-flash" }),
      env: { CLAUDE_CODE_OAUTH_TOKEN: "a-login-the-process-carried", ANTHROPIC_API_KEY: "another-key" },
    }));

    expect(command.env).toMatchObject({
      CLAUDE_CONFIG_DIR: home,
      ANTHROPIC_BASE_URL: "https://api.deepseek.com/anthropic",
      ANTHROPIC_AUTH_TOKEN: "sk-not-a-real-key",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "deepseek-v4-pro",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "deepseek-flash",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "deepseek-flash",
    });
    // Cleared rather than left as the launching process carried them.
    expect(command.env.CLAUDE_CODE_OAUTH_TOKEN).toBeUndefined();
    expect(command.env.ANTHROPIC_API_KEY).toBeUndefined();
    // The key's content is nowhere but the child's environment.
    expect(command.args.join(" ")).not.toContain("sk-not-a-real-key");
  });

  test("clears a family its map leaves out, so no stale variable reaches the endpoint", async () => {
    const command = await claudeCode.command(workerRun({
      account: endpointAccount({ opus: "deepseek-v4-pro" }),
      env: { ANTHROPIC_DEFAULT_HAIKU_MODEL: "a-model-of-another-road" },
    }));

    expect(command.env.ANTHROPIC_DEFAULT_OPUS_MODEL).toBe("deepseek-v4-pro");
    expect(command.env.ANTHROPIC_DEFAULT_SONNET_MODEL).toBeUndefined();
    expect(command.env.ANTHROPIC_DEFAULT_HAIKU_MODEL).toBeUndefined();
  });

  test("a key that cannot be read refuses the launch, naming the account and not the key", async () => {
    const account = endpointAccount({ opus: "deepseek-v4-pro" });

    account.credential.keyFile = join(root, "missing.key");

    await expect(claudeCode.command(workerRun({ account }))).rejects.toThrow("The key file of account deepseek cannot be read.");
  });

  test("a base the run carries is the whole prompt, and the harnessed file is without one", async () => {
    const account = endpointAccount({ opus: "deepseek-v4-pro" });
    const withBase = await claudeCode.command(workerRun({ account, systemPromptPath: base }));
    const without = await claudeCode.command(workerRun({ account }));

    expect(withBase.args).toContain("--system-prompt-file");
    expect(withBase.args).not.toContain("--append-system-prompt-file");
    expect(withBase.args[withBase.args.indexOf("--system-prompt-file") + 1]).toBe(base);
    expect(without.args).toContain("--system-prompt-file");
    expect(without.args).not.toContain("--append-system-prompt-file");
    expect(without.args[without.args.indexOf("--system-prompt-file") + 1]).toBe(harnessed);
  });
});

describe("the registered models' road", () => {
  const homes = dataPath("claude-roads", "registered");
  const gateway = gatewayUrl({});

  test("is a registration's records home or agency's gateway, and no other endpoint", () => {
    expect(registeredRoad({ CLAUDE_CONFIG_DIR: join(homes, "a-registration") })).toBe(true);
    expect(registeredRoad({ ANTHROPIC_BASE_URL: gateway })).toBe(true);
    expect(registeredRoad({ ANTHROPIC_BASE_URL: `${gateway}/` })).toBe(true);
    expect(registeredRoad({ CLAUDE_CONFIG_DIR: home, ANTHROPIC_BASE_URL: "https://api.deepseek.com/anthropic" })).toBe(false);
    expect(registeredRoad({})).toBe(false);
  });

  test("leaves an endpoint account's program on the Claude Code road", () => {
    const program = (env: Record<string, string | undefined>) => claudeCodeProvider.resident?.owns({ args: ["claude"], env });

    expect(program({ CLAUDE_CONFIG_DIR: home, ANTHROPIC_BASE_URL: "https://api.deepseek.com/anthropic" })).toBe(true);
    expect(program({ CLAUDE_CONFIG_DIR: join(homes, "a-registration"), ANTHROPIC_BASE_URL: gateway })).toBe(false);
  });
});
