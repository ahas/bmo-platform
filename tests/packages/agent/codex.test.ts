import "@agency/test-support/setup";

import { join } from "node:path";
import { describe, expect, test } from "bun:test";

import { writeFileSync } from "node:fs";

import { codex, codexFailure, codexResident } from "@agency/agent/codex";
import { scratchDir } from "@agency/test-support/scratch";

import type { HarnessRun } from "@agency/agent/harness";

const root = "/srv/bmo";
const prompt = join(root, "prompt.md");

function runFor(overrides: Partial<HarnessRun> = {}): HarnessRun {
  return {
    line: "gate",
    model: "gpt-5.6-terra",
    effort: "medium",
    systemPromptPath: prompt,
    toolsShown: "",
    toolsAllowed: "",
    toolsDenied: null,
    mcpConfigPath: null,
    prompt: "something to judge",
    promptDelivery: "input",
    resumeSessionId: null,
    keepSession: false,
    hostCommands: false,
    thinking: true,
    answerShape: "text",
    cwd: root,
    env: { PATH: "/usr/bin" },
    ...overrides,
  };
}

// A job worker's run as its line composes it: no system prompt of its own, every tool, and the
// rules file the line hands over.
const rules = "# Standing rules\n\n- Never destroy the system.\n";
const rulesPath = join(scratchDir("bmo-codex-rules-"), "worker-rules.md");

writeFileSync(rulesPath, rules);

function worker(overrides: Partial<HarnessRun> = {}): HarnessRun {
  return runFor({
    line: "worker",
    systemPromptPath: null,
    toolsShown: null,
    toolsAllowed: null,
    extensions: { settingsPath: join(root, "settings.json"), pluginDir: join(root, "plugin"), agentsJson: "{}", rulesPath },
    ...overrides,
  });
}

function configValues(args: string[]): Map<string, string> {
  const values = new Map<string, string>();

  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === "--config") {
      const value = args[index + 1] ?? "";
      values.set(value.slice(0, value.indexOf("=")), value.slice(value.indexOf("=") + 1));
    }
  }

  return values;
}

describe("the Codex fallback harness", () => {
  test("runs a one-turn line with agency's prompt file and none of the machine's additions", async () => {
    const command = await codex.command(runFor());
    const settings = configValues(command.args);

    expect(command.args.slice(0, 6)).toEqual([
      "codex",
      "exec",
      "--json",
      "--ignore-user-config",
      "--ignore-rules",
      "--strict-config",
    ]);
    expect(command.args).toContain("--skip-git-repo-check");
    expect(command.args.slice(command.args.indexOf("--model"), command.args.indexOf("--model") + 2)).toEqual([
      "--model",
      "gpt-5.6-terra",
    ]);
    expect(settings.get("model_instructions_file")).toBe(JSON.stringify(prompt));
    expect(settings.get("project_doc_max_bytes")).toBe("0");
    expect(settings.get("web_search")).toBe('"disabled"');
    expect(settings.get("features.apps")).toBe("false");
    expect(settings.get("features.multi_agent")).toBe("false");
    expect(settings.get("features.shell_tool")).toBe("false");
    expect(command.args.at(-1)).toBe("-");
    expect(command.stdin).toBe("something to judge");
  });

  test("resumes a kept thread without replacing its original line", async () => {
    const command = await codex.command(runFor({
      keepSession: true,
      resumeSessionId: "0199cbbb-1234-7000-8000-000000000001",
    }));

    expect(command.args.slice(0, 4)).toEqual(["codex", "exec", "resume", "--json"]);
    expect(command.args).not.toContain("--ephemeral");
    expect(command.args).not.toContain("--cd");
    expect(command.args.slice(command.args.indexOf("--model"), command.args.indexOf("--model") + 2)).toEqual([
      "--model",
      "gpt-5.6-terra",
    ]);
    expect(command.args.at(-2)).toBe("0199cbbb-1234-7000-8000-000000000001");
    expect(command.args.at(-1)).toBe("-");
    expect(command.stdin).toBe("something to judge");
  });

  test("gives a privileged chat the host, and fences a restricted chat read-only", async () => {
    const privileged = await codex.command(
      runFor({ line: "chat", toolsShown: "Bash,Read,Write,Edit", toolsAllowed: "Bash,Read,Write,Edit" }),
    );
    const restricted = await codex.command(
      runFor({ line: "chat", toolsShown: "Bash,Read,Write,Edit", toolsAllowed: "Read" }),
    );

    expect(privileged.args).toContain("--dangerously-bypass-approvals-and-sandbox");
    expect(privileged.args).not.toContain("--sandbox");
    expect(configValues(privileged.args).get("approval_policy")).toBe('"never"');
    expect(configValues(privileged.args).get("sandbox_mode")).toBe('"danger-full-access"');
    expect(configValues(privileged.args).get("features.shell_tool")).toBe("true");
    expect(restricted.args.slice(restricted.args.indexOf("--sandbox"), restricted.args.indexOf("--sandbox") + 2)).toEqual([
      "--sandbox",
      "read-only",
    ]);
    expect(restricted.args).not.toContain("--dangerously-bypass-approvals-and-sandbox");
    expect(configValues(restricted.args).get("sandbox_mode")).toBe('"read-only"');
  });

  // An MCP server a run is allowed stands beside its file tools; a gate shown file tools still writes
  // only inside its sandbox.
  test("keeps a run allowed its MCP server beside its file tools in a sandbox it writes", async () => {
    const run = await codex.command(
      runFor({ line: "gate", toolsShown: "Read,Write,Edit,Glob,Grep", toolsAllowed: "Read,Write,Edit,Glob,Grep,mcp__web" }),
    );

    expect(configValues(run.args).get("sandbox_mode")).toBe('"workspace-write"');
  });

  test("keeps the YOLO boundary when a privileged chat resumes its thread", async () => {
    const command = await codex.command(runFor({
      line: "chat",
      toolsShown: "Bash,Read,Write,Edit",
      toolsAllowed: "Bash,Read,Write,Edit",
      keepSession: true,
      resumeSessionId: "0199cbbb-1234-7000-8000-000000000003",
    }));

    expect(command.args).toContain("--dangerously-bypass-approvals-and-sandbox");
    expect(configValues(command.args).get("approval_policy")).toBe('"never"');
    expect(configValues(command.args).get("sandbox_mode")).toBe('"danger-full-access"');
  });

  test("uses the line's effort, except where thinking was deliberately switched off", async () => {
    expect(configValues((await codex.command(runFor())).args).get("model_reasoning_effort")).toBe('"medium"');
    expect(configValues((await codex.command(runFor({ thinking: false }))).args).get("model_reasoning_effort")).toBe('"low"');
  });

  test("reads the final JSON event and its disjoint token split, while keeping no session", async () => {
    const stdout = [
      { type: "item.completed", item: { type: "agent_message", text: "  answered\n" } },
      {
        type: "turn.completed",
        usage: {
          input_tokens: 120,
          cached_input_tokens: 80,
          cache_write_input_tokens: 10,
          output_tokens: 7,
          reasoning_output_tokens: 3,
        },
      },
    ].map((event) => JSON.stringify(event)).join("\n");
    const run = runFor();
    const answer = await codex.readAnswer({ stdout, stderr: "progress\n", exitCode: 0 }, run);

    expect(answer.text).toBe("answered");
    expect(answer).toMatchObject({ mode: "codex", model: run.model, effort: run.effort });
    expect(answer.sessionId).toBeNull();
    expect(answer.spend).toEqual([{
      mode: "codex",
      model: run.model,
      tokens: 127,
      inputTokens: 30,
      outputTokens: 7,
      cacheReadTokens: 80,
      cacheWriteTokens: 10,
      cost: 0,
    }]);
    expect((await codex.readAnswer({ stdout, stderr: "failed", exitCode: 1 }, run)).text).toBe("");
    expect(codexFailure({ stdout: "", stderr: "first\nlast\n", exitCode: 1 })).toBe("last");
    expect(codexFailure({
      stdout: JSON.stringify({ type: "turn.failed", error: { message: "the model failed" } }),
      stderr: "less useful",
      exitCode: 1,
    })).toBe("the model failed");
  });

  test("keeps the thread id when the caller asked for a resumable session", async () => {
    const stdout = [
      { type: "thread.started", thread_id: "0199cbbb-1234-7000-8000-000000000002" },
      { type: "item.completed", item: { type: "agent_message", text: "answered" } },
      { type: "turn.completed", usage: { input_tokens: 1, output_tokens: 1 } },
    ].map((event) => JSON.stringify(event)).join("\n");

    expect((await codex.readAnswer(
      { stdout, stderr: "", exitCode: 0 },
      runFor({ keepSession: true }),
    )).sessionId).toBe("0199cbbb-1234-7000-8000-000000000002");
  });
});

describe("a job on Codex", () => {
  test("keeps Codex's own prompt and the checkout's AGENTS.md, with the worker's rules as its developer instructions", async () => {
    for (const settings of [configValues((await codex.command(worker())).args), configValues((await codexResident.command(worker())).args)]) {
      expect(settings.has("model_instructions_file")).toBe(false);
      expect(settings.has("project_doc_max_bytes")).toBe(false);
      expect(settings.get("developer_instructions")).toBe(JSON.stringify(rules));
    }
  });

  test("is refused when its line handed it no rules", () => {
    expect(() => codex.command(worker({ extensions: undefined }))).toThrow("no rules");
  });
});

describe("an MCP server Codex starts", () => {
  // Codex hands a server a handful of names of its own and none of the run's, so the run's `BMO_`
  // names go with each server for Codex to hand on — the names, and never a value.
  const config = join(scratchDir("bmo-codex-mcp-"), "chat.mcp.json");

  writeFileSync(config, JSON.stringify({ mcpServers: { bmo: { type: "stdio", command: "/usr/bin/bun", args: ["/srv/bmo/scripts/chat-mcp.ts"], env: {} } } }));

  const turn = (room: string) => runFor({
    line: "chat",
    mcpConfigPath: config,
    toolsShown: "Bash",
    toolsAllowed: "Bash",
    env: { PATH: "/usr/bin", BMO_BOT_TOKEN: "0:secret-token", BMO_RUN_NAME: `chat:${room}:1:1`, BMO_TURN_CHAT: room },
  });

  test("is named the run's own BMO_ variables and the place, and handed none of their values", async () => {
    const args = (await codex.command(turn("-1"))).args;

    expect(configValues(args).get("mcp_servers.bmo.env_vars")).toBe(JSON.stringify(["BMO_AGENT_PLACE", "BMO_BOT_TOKEN", "BMO_RUN_NAME", "BMO_TURN_CHAT"]));
    expect(args.join(" ")).not.toContain("secret-token");
  });

  test("on the tool host is its URL, the variable its token is read from and a wait past the host's longest call, and nothing of the run is forwarded", async () => {
    const hosted = join(scratchDir("bmo-codex-mcp-"), "chat.mcp.json");

    writeFileSync(hosted, JSON.stringify({ mcpServers: { bmo: { type: "http", url: "http://127.0.0.1:3290/mcp/bmo", headers: { Authorization: "Bearer ${BMO_MCP_TOKEN}" } } } }));

    const values = configValues((await codex.command({ ...turn("-1"), mcpConfigPath: hosted })).args);

    expect(values.get("mcp_servers.bmo.url")).toBe(JSON.stringify("http://127.0.0.1:3290/mcp/bmo"));
    expect(values.get("mcp_servers.bmo.bearer_token_env_var")).toBe(JSON.stringify("BMO_MCP_TOKEN"));
    expect(Number(values.get("mcp_servers.bmo.tool_timeout_sec"))).toBeGreaterThan(180);
    expect(values.has("mcp_servers.bmo.env_vars")).toBe(false);
    expect(values.has("mcp_servers.bmo.command")).toBe(false);
  });

  test("by the resident program, which stands across rooms, is named the place and none of a run's own", async () => {
    const one = await codexResident.command(turn("-1"));
    const other = await codexResident.command(turn("42"));

    expect(configValues(one.args).get("mcp_servers.bmo.env_vars")).toBe(JSON.stringify(["BMO_AGENT_PLACE", "BMO_BOT_TOKEN"]));
    expect(one.args).toEqual(other.args);
  });
});

test("no Codex run is handed an output schema, fresh or resumed",async ()=>{
 for(const resumeSessionId of [null,"existing-thread"]){
  expect((await codex.command(worker({resumeSessionId}))).args).not.toContain("--output-schema");
 }
});
