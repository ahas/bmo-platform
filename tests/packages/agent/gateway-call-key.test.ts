import "@agency/test-support/setup";

import { chmodSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, describe, expect, test } from "bun:test";

import { scratchDir } from "@agency/test-support/scratch";

import { claudeCodeAt, type ClaudeEndpoint } from "@agency/agent/claude-code";
import type { HarnessRun } from "@agency/agent/harness";

// A run of Claude Code on a gateway road goes out under the call key of the account it is for, where agency
// issued that account one, and under the road's own key otherwise; every request names the run's place as its run
// (docs/own-llm.md, 「The keys」; docs/supply.md, 「Reservations」).

const root = scratchDir("bmo-gateway-call-key-");
const realDataDir = process.env.BMO_DATA_DIR;

process.env.BMO_DATA_DIR = root;

afterAll(() => {
  if (realDataDir === undefined) delete process.env.BMO_DATA_DIR;
  else process.env.BMO_DATA_DIR = realDataDir;
  rmSync(root, { recursive: true, force: true });
});

const roadKey = join(root, "env", "llm-gateway.key");
const callKey = join(root, "env", "llm-gateway-calls", "42.key");

function privateFile(path: string, text: string): void {
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
  chmodSync(path, 0o600);
}

privateFile(roadKey, "the-road-key\n");
privateFile(callKey, "bmo-call-person\n");

const endpoint: ClaudeEndpoint = {
  provider: "deepseek",
  baseUrl: "http://[::1]:9",
  keyPath: () => roadKey,
  async model() {
    return { name: "deepseek-test", contextTokens: 100_000, answerTokens: 8_000, thinking: true, home: join(root, "home") };
  },
  effort: ({ thinking }) => (thinking ? "medium" : "low"),
  homes: root,
  offers: [],
};

function run(env: Record<string, string>): HarnessRun {
  return {
    line: "chat", model: "deepseek-test", effort: "high", systemPromptPath: null, toolsShown: "", toolsAllowed: null,
    toolsDenied: null, mcpConfigPath: null, prompt: "hello", promptDelivery: "input", resumeSessionId: null,
    keepSession: true, hostCommands: false, thinking: true, answerShape: "unread", cwd: root,
    env: { PATH: "/usr/bin:/bin", ...env },
  };
}

// The key file the settings' helper prints for a command.
function helperKey(args: readonly string[]): string {
  const settings = args[args.indexOf("--settings") + 1] as string;

  return JSON.parse(readFileSync(settings, "utf8")).apiKeyHelper;
}

describe("a run of Claude Code on a gateway road", () => {
  test("goes out under its account's call key and names its place as its run", async () => {
    const command = await claudeCodeAt(endpoint).command(
      run({ BMO_OWNER_ACCOUNT: "42", BMO_AGENT_PLACE: join(root, "agents", "u42-helper") }),
    );

    expect(helperKey(command.args)).toBe(`cat '${callKey}'`);
    expect(command.env?.ANTHROPIC_CUSTOM_HEADERS).toBe("x-bmo-run: u42-helper");
  });

  test("names a thrown-away session's place by its session's agent", async () => {
    const place = join(root, "agent-places", "apart-u42-work-1234");

    mkdirSync(place, { recursive: true });
    writeFileSync(join(place, "apart-place.json"), JSON.stringify({ place: "apart-u42-work-1234", session: "u42-work@1234" }));

    const command = await claudeCodeAt(endpoint).command(run({ BMO_OWNER_ACCOUNT: "42", BMO_AGENT_PLACE: place }));

    expect(command.env?.ANTHROPIC_CUSTOM_HEADERS).toBe("x-bmo-run: u42-work");
  });

  test("goes out under the road's own key for an account with no call key, and for a run of no account", async () => {
    const runs: Record<string, string>[] = [{ BMO_OWNER_ACCOUNT: "7" }, {}];

    for (const env of runs) {
      const command = await claudeCodeAt(endpoint).command(run(env));

      expect(helperKey(command.args)).toBe(`cat '${roadKey}'`);
      expect(command.env?.ANTHROPIC_CUSTOM_HEADERS).toBeUndefined();
    }
  });

  test("leaves a call key that is not private to its owner unused", async () => {
    chmodSync(callKey, 0o644);

    const command = await claudeCodeAt(endpoint).command(run({ BMO_OWNER_ACCOUNT: "42" }));

    expect(helperKey(command.args)).toBe(`cat '${roadKey}'`);
    chmodSync(callKey, 0o600);
  });
});
