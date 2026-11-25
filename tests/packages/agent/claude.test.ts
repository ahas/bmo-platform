import "@agency/test-support/setup";

import { join } from "node:path";
import { afterAll, describe, expect, test } from "bun:test";

import { readClaudeOutput } from "@agency/agent/claude";
import { ClaudeLimitError, ClaudeOutageError } from "@agency/agent/claude-failure";
import { installPlacer } from "@agency/agent/agent-process";

// No agent host stands in a test: a run a gate here would place in one is refused at once rather than
// waiting out a host that never starts.
installPlacer(async () => ({ ok: false, error: { code: "no-host", message: "no agent host in this test" } }));
afterAll(() => installPlacer(null));

const retry = (status: number, attempt: number): string =>
  JSON.stringify({ type: "system", subtype: "api_retry", attempt, max_retries: 10, retry_delay_ms: 500, error_status: status, error: "overloaded" });

describe("readClaudeOutput", () => {
  test("a run that only retried its API call and stopped is an outage", () => {
    const stdout = [JSON.stringify({ type: "system", subtype: "init" }), retry(529, 1), retry(529, 2), retry(500, 3)].join("\n");

    expect(() => readClaudeOutput(stdout, "", 1)).toThrow(ClaudeOutageError);

    try {
      readClaudeOutput(stdout, "", 1);
    } catch (error) {
      expect(error).toBeInstanceOf(ClaudeOutageError);
      expect((error as ClaudeOutageError).attempts).toBe(3);
      expect((error as ClaudeOutageError).status).toBe(500);
    }
  });

  test("a result after retries is still a reply, and a limit is still a limit", () => {
    const result = JSON.stringify({ type: "result", result: "OK", session_id: "s1", is_error: false, usage: { input_tokens: 1 } });

    expect(readClaudeOutput([retry(529, 1), result].join("\n"), "", 0).text).toBe("OK");
    expect(() => readClaudeOutput("", "Claude AI usage limit reached|1788469200", 1)).toThrow(ClaudeLimitError);
    expect(() => readClaudeOutput("", "", 1)).toThrow(/exited with code 1/);
  });

  test("a run that exited after writing its result says why from that result", () => {
    expect(() => readClaudeOutput(JSON.stringify({ type: "result", result: "API Error: the answer ran past its ceiling", session_id: "s1", is_error: true }), "[a warning]", 1)).toThrow(/ran past its ceiling/);
  });

  // A resumed session's result names the session's running total model by model, and the run's own
  // usage beside it (docs/own-llm.md, 「How requests go」).
  test("reads a run's own spend off its usage where asked, and the model-by-model figures otherwise", () => {
    const result = JSON.stringify({
      type: "result", result: "green", session_id: "s1", is_error: false,
      usage: { input_tokens: 9_091, output_tokens: 3, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
      modelUsage: { "local-0-1": { inputTokens: 12_840, outputTokens: 2_995, cacheReadInputTokens: 34_122, cacheCreationInputTokens: 0 } },
    });

    expect(readClaudeOutput(result, "", 0, null, { ownRun: true }).spend).toMatchObject([{ model: "local-0-1", inputTokens: 9_091, outputTokens: 3, cacheReadTokens: 0 }]);
    expect(readClaudeOutput(result, "", 0).spend).toMatchObject([{ model: "local-0-1", inputTokens: 12_840, outputTokens: 2_995 }]);
  });
});

// A Claude Workflow is not a background task the stream reports as one: the CLI opens it as a task of
// its own kind, and what it did with it is in the three task events alone. So what a reply says
// about a Claude Workflow is read here, out of a stream written the way the CLI writes one.
describe("the Claude Workflows a run's stream reports", () => {
  const answer = JSON.stringify({ type: "result", result: "All done.", session_id: "s1", is_error: false });

  const started = (call: string, task: string, name: string, kind = "local_workflow"): string =>
    JSON.stringify({
      type: "system",
      subtype: "task_started",
      task_id: task,
      tool_use_id: call,
      task_type: kind,
      workflow_name: name,
      description: "ask two things side by side",
      prompt: "export const meta = { name: 'two-answers' }",
    });

  // The ending names the task rather than the call as often as not, which is why the call the
  // task was opened under is what a Claude Workflow is kept by.
  const ended = (task: string, status: string): string =>
    JSON.stringify({
      type: "system",
      subtype: "task_notification",
      task_id: task,
      status,
      summary: "ask two things side by side",
      usage: { total_tokens: 34402, tool_uses: 2, duration_ms: 4079 },
    });

  test("an ending reported after the result frame is still read", () => {
    const reply = readClaudeOutput([started("toolu_1", "wtask1", "two-answers"), answer, ended("wtask1", "stopped")].join("\n"), "", 0);

    expect(reply.workflows[0]?.status).toBe("stopped");
  });

  // A command a run left going in the background opens and ends through these same three
  // events. Refusing a job over one of those would refuse a job that closed everything it did.
  test("the CLI's other background tasks are none of this", () => {
    const reply = readClaudeOutput(
      [started("toolu_bash", "bash_1", "", "shell"), ended("bash_1", "completed"), answer].join("\n"),
      "",
      0,
    );

    expect(reply.workflows).toEqual([]);
  });

  test("a run that launched no Claude Workflow reports none", () => {
    expect(readClaudeOutput(answer, "", 0).workflows).toEqual([]);
  });
});

