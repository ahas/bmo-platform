import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { join } from "node:path";

import { Archive } from "@agency/archive/archive";
import { scratchDir } from "@agency/test-support/scratch";
import { OWNER_ID, UNIT } from "@agency/test-support/ids";
import { branchWord, mergeSpans, runPhase, spanClock, SpanTally } from "@agency/agent/job-spans";
import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

// A clock the test moves itself, so a span's seconds are the ones the test meant rather than
// however long the test machine took to get from one line to the next.
function clock(): { now: () => number; pass: (seconds: number) => void } {
  let at = 1_700_000_000_000;

  return { now: () => at, pass: (seconds: number) => { at += seconds * 1000; } };
}

function tool(id: string, name: string, input: Record<string, unknown> = {}): unknown {
  return { type: "assistant", message: { content: [{ type: "tool_use", id, name, input }] } };
}

function answer(id: string, text = "done"): unknown {
  return { type: "user", message: { content: [{ type: "tool_result", tool_use_id: id, content: text }] } };
}

const workflowCall = "toolu_workflow";
const workflowTaskId = "wtask1";

function launchedWorkflow(): unknown {
  return {
    type: "system",
    subtype: "task_started",
    task_id: workflowTaskId,
    tool_use_id: workflowCall,
    task_type: "local_workflow",
    workflow_name: "usage",
  };
}

function branchProgress(branches: unknown[]): unknown {
  return {
    type: "system",
    subtype: "task_progress",
    task_id: workflowTaskId,
    tool_use_id: workflowCall,
    workflow_progress: branches,
  };
}

// Everything a phase, a tool call or a Claude Workflow branch says about how its tokens divided, which
// is nothing: the CLI states no division for any of them, so the four kinds stand as unmeasured
// and are written out once here rather than on every expected shape below.
const unsplit = { inputTokens: null, outputTokens: null, cacheReadTokens: null, cacheWriteTokens: null };

describe("what the stream says about where a run's time went", () => {
  test("a call and the result that answers it are one span, and the same tool called twice is one row", () => {
    const time = clock();
    const tally = new SpanTally(time.now);

    tally.see(tool("t1", "Bash", { command: "bun test" }));
    time.pass(30);
    tally.see(answer("t1"));
    tally.see(tool("t2", "Bash", { command: "git diff" }));
    time.pass(10);
    tally.see(answer("t2"));
    tally.see(tool("t3", "Read", { file_path: "/tmp/a" }));
    time.pass(2);
    tally.see(answer("t3"));

    expect(tally.spans()).toEqual([
      { kind: "tool", name: "Bash", seconds: 40, times: 2, tokens: null, ...unsplit },
      { kind: "tool", name: "Read", seconds: 2, times: 1, tokens: null, ...unsplit },
    ]);
  });

  test("a call made through the toolbook is timed under the tool it reached", () => {
    const time = clock();
    const tally = new SpanTally(time.now);

    tally.see(tool("t1", "mcp__toolbook__call", { tool: "web/naver_price", arguments: { query: "RTX 5090" } }));
    time.pass(4);
    tally.see(answer("t1"));

    expect(tally.spans()).toEqual([{ kind: "tool", name: "mcp__web__naver_price", seconds: 4, times: 1, tokens: null, ...unsplit }]);
  });

  test("a call nobody answered is no span at all", () => {
    // What a run killed part-way leaves: the tool that was going when the kill came never
    // closed, and the run still hands over everything that did.
    const time = clock();
    const tally = new SpanTally(time.now);

    tally.see(tool("t1", "Bash", { command: "sleep 600" }));
    time.pass(600);

    expect(tally.spans()).toEqual([]);

    tally.see(tool("t2", "Read", {}));
    time.pass(1);
    tally.see(answer("t2"));

    expect(tally.spans()).toEqual([{ kind: "tool", name: "Read", seconds: 1, times: 1, tokens: null, ...unsplit }]);
  });

  test("a co-worker stands as itself, and what it did inside is not counted a second time", () => {
    const time = clock();
    const tally = new SpanTally(time.now);

    tally.see(tool("t1", "Task", { subagent_type: "scout", description: "find where it is" }));
    // The branch's own tools arrive with the call that opened it as their parent. They are the
    // branch's seconds, and the branch is already one span.
    tally.see({ ...(tool("inner", "Bash", { command: "rg todo" }) as object), parent_tool_use_id: "t1" });
    time.pass(50);
    tally.see({ ...(answer("inner") as object), parent_tool_use_id: "t1" });
    time.pass(10);
    tally.see(answer("t1", "found it"));

    expect(tally.spans()).toEqual([
      { kind: "agent", name: "Task(scout) · find where it is", seconds: 60, times: 1, tokens: null, ...unsplit },
    ]);
  });

  test("a Claude Workflow's branches carry the seconds and the tokens the CLI states outright", () => {
    const tally = new SpanTally(clock().now);
    const branch = { index: 1, label: "build:usage", agentId: "a1", model: "claude-opus-5" };

    tally.see(launchedWorkflow());
    // The call that launches a Claude Workflow answers for the whole of it, so it is no span of its
    // own: its branches are what the time went to.
    tally.see(tool(workflowCall, "Workflow", { script: "return 1" }));
    tally.see(answer(workflowCall, "Workflow launched in background."));
    tally.see(branchProgress([{ ...branch, state: "start" }]));

    expect(tally.spans()).toEqual([]);

    tally.see(branchProgress([{ ...branch, state: "done", durationMs: 1_218_000, tokens: 292_463, toolCalls: 119 }]));
    // The stream repeats the whole picture of a Claude Workflow in every progress event, and a branch
    // said twice is still one branch.
    tally.see(branchProgress([{ ...branch, state: "done", durationMs: 1_218_000, tokens: 292_463, toolCalls: 119 }]));

    expect(tally.spans()).toEqual([
      { kind: "agent", name: "branch 1 · build:usage · claude-opus-5", seconds: 1218, times: 1, tokens: 292_463, ...unsplit },
    ]);
  });

  test("a command left running in the background is not a Claude Workflow and leaves nothing", () => {
    const tally = new SpanTally(clock().now);
    const shell = { type: "system", task_id: "bash_1", tool_use_id: "toolu_bash", task_type: "shell" };

    tally.see({ ...shell, subtype: "task_started" });
    tally.see({ ...shell, subtype: "task_progress", workflow_progress: [{ index: 1, durationMs: 9000 }] });
    tally.see({ ...shell, subtype: "task_notification", status: "completed", summary: "bun test" });

    expect(tally.spans()).toEqual([]);
  });
});

describe("the same span across several runs", () => {
  test("keeps a tool and a co-worker of the same name apart", () => {
    const merged = mergeSpans([
      { kind: "tool", name: "Task", seconds: 5, times: 1, tokens: null, ...unsplit },
      { kind: "agent", name: "Task", seconds: 50, times: 1, tokens: null, ...unsplit },
    ]);

    expect(merged).toHaveLength(2);
  });
});

describe("a length of time as it is read out", () => {
  test("counts in seconds, then in minutes, then in hours", () => {
    expect(spanClock(3)).toBe("3s");
    expect(spanClock(59.4)).toBe("59s");
    expect(spanClock(492)).toBe("8m 12s");
    expect(spanClock(4920)).toBe("1h 22m");
    expect(spanClock(-5)).toBe("0s");
  });
});

