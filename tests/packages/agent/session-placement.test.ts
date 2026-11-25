import "@agency/test-support/setup";

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, test } from "bun:test";

import { Archive } from "@agency/archive/archive";
import { installProviderSettings } from "@agency/agent/provider-order";
import { provider } from "@agency/agent/harnesses";
import { scratchDir } from "@agency/test-support/scratch";
import { scratchDatabase } from "@agency/test-support/store";

import type { HarnessRun } from "@agency/agent/harness";
import { mapInOrder } from "@agency/support/in-order";
import { fillMarks } from "@agency/support/marks";
import declaredRegistered from "@agency/policy/providers/registered.toml";

// A session on a registered model served on several nodes keeps to one node, since each node keeps the
// state of the conversation it last answered on its own disk (docs/own-llm.md, 「Nodes and sessions」).
// The place is chosen as a session opens, kept with the held session beside its model and its login,
// and gone back to by a resume and a fold; a session whose node has left is not resumed.

const root = scratchDir("bmo-session-placement-");
const saved = { agents: process.env.BMO_AGENTS_DIR, data: process.env.BMO_DATA_DIR };
const registered = provider("registered");
let archive: Archive;
// Made at the file's top, as a scratch store must be.
const store = await scratchDatabase();

// Three made-up nodes the "multi" registration is served on, each at a port of its own.
const NODES = ["able", "alpha", "bravo"];
const urlOf = (node: string) => `http://127.0.0.1:${19_001 + NODES.indexOf(node)}/v1/chat/completions`;

beforeAll(async () => {
  process.env.BMO_AGENTS_DIR = join(root, "agents");
  process.env.BMO_DATA_DIR = join(root, "data");
  mkdirSync(join(root, "data", "env"), { recursive: true });
  writeFileSync(join(root, "data", "env", "llm-gateway.key"), "gateway-key\n", { mode: 0o600 });
  archive = new Archive(store);

  const registry = archive.modelRegistry();

  await registry.create({ name: "multi", label: "Multi", model: "m", contextTokens: 65_536, answerTokens: 4_096, thinking: false, walked: false, keepsResearch: false });
  for (const node of NODES) await registry.setAddress("multi", { node, url: urlOf(node), keyFile: null, keyOwned: false });
  await registry.create({ name: "single", label: "Single", model: "s", contextTokens: 65_536, answerTokens: 4_096, thinking: false, walked: false, keepsResearch: false });
  await registry.setAddress("single", { node: "only", url: "http://127.0.0.1:19009/v1/chat/completions", keyFile: null, keyOwned: false });
  installProviderSettings(archive);
});

afterAll(() => {
  installProviderSettings(null);
  for (const [name, value] of [["BMO_AGENTS_DIR", saved.agents], ["BMO_DATA_DIR", saved.data]] as const) {
    if (value === undefined) delete process.env[name];
    else process.env[name] = value;
  }
});

function run(overrides: Partial<HarnessRun> = {}): HarnessRun {
  return {
    line: "worker", model: "multi", effort: "", systemPromptPath: null, toolsShown: null, toolsAllowed: null, toolsDenied: null,
    mcpConfigPath: null, prompt: "get to work", promptDelivery: "input", resumeSessionId: null, keepSession: true, hostCommands: false,
    thinking: false, answerShape: "unread", cwd: root, env: {},
    ...overrides,
  };
}

// A chat's run, whose line the program is handed without a worker's extensions.
function chat(overrides: Partial<HarnessRun> = {}): HarnessRun {
  return run({ line: "chat", systemPromptPath: join(root, "chat.md"), toolsShown: "Bash", toolsAllowed: "Bash", ...overrides });
}

const nodes = async () => (await archive.modelRegistry().get("multi"))?.addresses.map((one) => [one.node, one.placedAt]);

describe("where a new session on a registered model stands", () => {
  test("on the node given a new session least recently, the time recorded, so sessions spread over the nodes", async () => {
    const placed = await mapInOrder([1_000_000, 2_000_000, 3_000_000, 4_000_000], (now) => registered.place?.(run(), archive, now));

    expect(placed).toEqual(["able", "alpha", "bravo", "able"]);
    expect(await nodes()).toEqual([["able", 4_000], ["alpha", 2_000], ["bravo", 3_000]]);
  });

  test("a registration of one address is never placed, and its runs name it alone", async () => {
    expect(await registered.place?.(run({ model: "single" }), archive, 5_000_000)).toBeNull();
    expect((await registered.adapter.command(chat({ model: "single" }))).env.ANTHROPIC_MODEL).toBe("single");
  });

  test("every request of a placed session names its node, and a run handed over unplaced is placed once", async () => {
    const env = (await registered.adapter.command(chat({ placement: "bravo" }))).env;

    expect([env.ANTHROPIC_MODEL, env.ANTHROPIC_DEFAULT_HAIKU_MODEL, env.CLAUDE_CODE_SUBAGENT_MODEL]).toEqual(["multi@bravo", "multi@bravo", "multi@bravo"]);
    expect((await registered.adapter.command(chat())).env.ANTHROPIC_MODEL).toMatch(/^multi@(able|alpha|bravo)$/);
    expect(() => registered.adapter.command(chat({ placement: "zulu" }))).toThrow(fillMarks(declaredRegistered.presence.node_missing, { name: "multi", node: "zulu" }));
  });
});

