import "@agency/test-support/setup";

import { expect, test } from "bun:test";
import { chmodSync, mkdirSync, writeFileSync } from "node:fs";

import { dataPath } from "@agency/support/roots";
import { deepSeekProvider } from "@agency/agent/deepseek";

// DeepSeek's road (docs/own-llm.md, 「The gateway's members」): its members, its program Claude Code on the
// gateway, its key files, and the words a run's thinking and effort go out with.

const member = (name: string, label: string) => ({
  name,
  road: "deepseek",
  label,
  provider: "deepseek",
  model: name,
  api_base: "https://api.deepseek.com/v1",
  key_file: "deepseek.key",
  context_tokens: 1_048_576,
  answer_tokens: 32_768,
  thinking: true,
});

function withMembers() {
  mkdirSync(dataPath("llm-gateway"), { recursive: true });
  mkdirSync(dataPath("env"), { recursive: true });
  writeFileSync(
    dataPath("llm-gateway", "members.json"),
    JSON.stringify([member("deepseek-flash", "DeepSeek V4.1 Flash"), member("deepseek-v4-pro", "DeepSeek V4 Pro")]),
  );

  for (const name of ["llm-gateway.key", "deepseek.key"]) {
    const file = dataPath("env", name);

    writeFileSync(file, "a key\n", { mode: 0o600 });
    chmodSync(file, 0o600);
  }
}

test("its runs stand Claude Code on the gateway under the members of its road", async () => {
  withMembers();

  expect(deepSeekProvider.adapter.name).toBe("claude-code:deepseek");
  expect(deepSeekProvider.resident).not.toBeNull();
  expect(await deepSeekProvider.models()).toEqual([{ id: "deepseek-flash" }, { id: "deepseek-v4-pro" }]);
  expect(await deepSeekProvider.unavailable({ config: { openrouterKey: null } } as never)).toBeNull();
});

test("an agent's thinking off is DeepSeek's own none, and a level the run takes stands", async () => {
  expect(await deepSeekProvider.reasoning.effort("high", { thinking: false, effort: null }, "deepseek-flash")).toBe("none");
  expect(await deepSeekProvider.reasoning.effort("high", { thinking: true, effort: "max" }, "deepseek-flash")).toBe("max");
  expect(await deepSeekProvider.reasoning.effort("low", { thinking: null, effort: null }, "deepseek-flash")).toBe("low");
  expect(deepSeekProvider.efforts).toEqual([null, "none", "low", "medium", "high", "max"]);
  expect(deepSeekProvider.lines).toEqual(["chat", "gate", "worker", "search"]);
});

test("a pin naming one of its members is taken and another is refused", async () => {
  withMembers();

  expect(await deepSeekProvider.namedModelFault?.("deepseek-v4-pro", null as never)).toBeNull();
  expect(await deepSeekProvider.namedModelFault?.("someone-else", null as never)).toContain("Not a DeepSeek model");
});
