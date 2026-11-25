import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { instructionOf, maxBranchLines, WorkflowTrail } from "@agency/agent/workflow-log";

import type { BranchTranscript } from "@agency/agent/workflow-log";

const call = "toolu_workflow";
const task = "wtask1";

function launched(): unknown {
  return {
    type: "system",
    subtype: "task_started",
    task_id: task,
    tool_use_id: call,
    task_type: "local_workflow",
    workflow_name: "two-answers",
    description: "ask two things side by side",
    prompt: "export const meta = { name: 'two-answers' }\nreturn 1",
  };
}

function launchAnswer(): unknown {
  return {
    type: "user",
    message: {
      content: [
        {
          type: "tool_result",
          tool_use_id: call,
          content: "Workflow launched in background. Task ID: wtask1\nTranscript dir: /tmp/flow\nRun ID: wf_1",
        },
      ],
    },
  };
}

function progress(branches: unknown[]): unknown {
  return { type: "system", subtype: "task_progress", task_id: task, tool_use_id: call, workflow_progress: branches };
}

function notification(): unknown {
  return {
    type: "system",
    subtype: "task_notification",
    task_id: task,
    tool_use_id: call,
    status: "completed",
    summary: "ask two things side by side",
    usage: { total_tokens: 34402, tool_uses: 2, duration_ms: 4079 },
  };
}

const branchOne = {
  index: 1,
  label: "review:bugs",
  agentId: "a1",
  model: "claude-sonnet-5",
  promptPreview: "find the bugs in this change",
};

// One branch's own record, in the shape the workflow writes it: what it was told, what it
// called, what came back, and what it said in the end.
function transcript(agentId: string, entries: unknown[] = []): BranchTranscript {
  return {
    agentId,
    entries: [
      { type: "user", message: { content: "find the bugs in this change" } },
      { type: "attachment", attachment: { type: "date" } },
      {
        type: "assistant",
        message: { content: [{ type: "tool_use", id: "toolu_b1", name: "Bash", input: { command: "git diff" } }] },
      },
      {
        type: "user",
        message: { content: [{ type: "tool_result", tool_use_id: "toolu_b1", content: "M packages/agent/runlog.ts" }] },
      },
      { type: "assistant", message: { content: [{ type: "text", text: "There were no bugs." }] } },
      ...entries,
    ],
  };
}

describe("a workflow the run launched", () => {
  test("puts the script it was given under the call that launched it, as a prompt", () => {
    const trail = new WorkflowTrail(() => []);
    const [line] = trail.lines(launched());

    expect(line?.kind).toBe("prompt");
    expect(line?.parent).toBe(call);
    expect(line?.text).toContain("workflow · two-answers · ask two things side by side");
    expect(line?.text).toContain("export const meta");
  });

  test("leaves the CLI's other background tasks alone", () => {
    const trail = new WorkflowTrail(() => []);

    expect(trail.lines({ ...(launched() as object), task_type: "shell" })).toEqual([]);
    expect(trail.lines({ type: "system", subtype: "background_tasks_changed", tasks: [] })).toEqual([]);
    expect(trail.lines({ type: "assistant", message: { content: [] } })).toEqual([]);
  });

  test("leaves them alone at their end too, however alike the three events look", () => {
    // A command a run left going in the background ends through the same events a workflow does,
    // and it has no branches and no records: a job that ran twenty of them would otherwise fill
    // its log with lines saying a workflow it never launched had finished.
    const trail = new WorkflowTrail(() => []);
    const shell = { type: "system", task_id: "bash_1", tool_use_id: "toolu_bash" };

    trail.lines({ ...shell, subtype: "task_started", task_type: "shell", description: "bun test" });

    expect(trail.lines({ ...shell, subtype: "task_progress", workflow_progress: [{ index: 1, state: "start" }] })).toEqual([]);
    expect(
      trail.lines({
        ...shell,
        subtype: "task_notification",
        status: "completed",
        summary: "bun test 2>&1 | tail -20",
        usage: { total_tokens: 0, tool_uses: 0, duration_ms: 4000 },
      }),
    ).toEqual([]);
  });
});

describe("each branch of a workflow", () => {
  test("says what it was given when it starts and what it handed back when it ends", () => {
    const trail = new WorkflowTrail(() => []);

    trail.lines(launched());

    const [start] = trail.lines(progress([{ ...branchOne, state: "start" }]));

    expect(start?.parent).toBe(call);
    expect(start?.text).toContain("branch 1 · review:bugs · claude-sonnet-5 · start");
    expect(start?.text).toContain("find the bugs in this change");

    const [done] = trail.lines(
      progress([
        {
          ...branchOne,
          state: "done",
          tokens: 17201,
          toolCalls: 4,
          durationMs: 2951,
          resultPreview: "no bugs",
        },
      ]),
    );

    expect(done?.text).toContain("done");
    expect(done?.text).toContain("3s · 17201 tokens · 4 tool calls");
    expect(done?.text).toContain("no bugs");
  });

  test("is said once, however often the stream repeats the same picture", () => {
    const trail = new WorkflowTrail(() => []);

    trail.lines(launched());
    trail.lines(progress([{ ...branchOne, state: "start" }]));

    expect(trail.lines(progress([{ ...branchOne, state: "start" }]))).toEqual([]);
    expect(trail.lines(progress([{ ...branchOne, state: "done", resultPreview: "no bugs" }]))).toHaveLength(1);
  });

  test("is said once even though its own id only appears after it has started", () => {
    const trail = new WorkflowTrail(() => []);
    const queued = { index: 2, label: "review:docs", model: "sonnet", state: "start", promptPreview: "read the documents" };

    trail.lines(launched());
    expect(trail.lines(progress([queued]))).toHaveLength(1);
    expect(trail.lines(progress([{ ...queued, agentId: "a2", model: "claude-sonnet-5" }]))).toEqual([]);
  });
});

describe("two workflows in one run", () => {
  function other(): unknown {
    return {
      type: "system",
      subtype: "task_started",
      task_id: "wtask2",
      tool_use_id: "toolu_other",
      task_type: "local_workflow",
      workflow_name: "second",
    };
  }

  test("keep their branches apart, however alike the two number them", () => {
    const trail = new WorkflowTrail(() => []);

    trail.lines(launched());
    trail.lines(other());
    expect(trail.lines(progress([{ ...branchOne, state: "start" }]))).toHaveLength(1);

    const second = trail.lines({
      type: "system",
      subtype: "task_progress",
      task_id: "wtask2",
      tool_use_id: "toolu_other",
      workflow_progress: [{ index: 1, label: "another branch", agentId: "b1", model: "haiku", state: "start" }],
    });

    expect(second).toHaveLength(1);
    expect(second[0]?.parent).toBe("toolu_other");
    expect(second[0]?.text).toContain("another branch");
  });

  test("read each other's records under their own names", () => {
    const trail = new WorkflowTrail((dir) => (dir === "/tmp/flow" ? [transcript("a1")] : []));

    trail.lines(launched());
    trail.lines(launchAnswer());
    trail.lines(other());
    trail.lines(
      progress([{ ...branchOne, state: "start" }]),
    );
    trail.lines({
      type: "system",
      subtype: "task_progress",
      task_id: "wtask2",
      tool_use_id: "toolu_other",
      workflow_progress: [{ index: 1, label: "another branch", agentId: "a1", model: "haiku", state: "start" }],
    });

    const head = trail.lines(notification()).find((line) => line.text.startsWith("branch "));

    expect(head?.text).toContain("branch review:bugs");
  });
});

describe("what a branch actually did", () => {
  function ran(read: (dir: string) => BranchTranscript[]): ReturnType<WorkflowTrail["lines"]> {
    const trail = new WorkflowTrail(read);

    trail.lines(launched());
    trail.lines(launchAnswer());
    trail.lines(progress([{ ...branchOne, state: "start" }]));

    return trail.lines(notification());
  }

  test("stands under the same call: its instruction, its tool calls, their answers and its words", () => {
    const lines = ran(() => [transcript("a1")]);

    expect(lines[0]?.text).toBe("workflow completed · ask two things side by side · 4s · 34402 tokens · 2 tool calls");
    expect(lines.every((line) => line.parent === call)).toBe(true);
    expect(lines[1]?.text).toContain("branch review:bugs");
    expect(lines[1]?.text).toContain("find the bugs in this change");
    expect(lines[2]).toMatchObject({ kind: "tool", name: "Bash", ref: "toolu_b1" });
    expect(lines[2]?.text).toContain("git diff");
    expect(lines[3]).toMatchObject({ kind: "tool-result", ref: "toolu_b1", text: "M packages/agent/runlog.ts" });
    expect(lines[4]).toMatchObject({ kind: "text", text: "There were no bugs." });
  });

  test("is read for every branch the workflow opened, in the order it opened them", () => {
    const trail = new WorkflowTrail(() => [transcript("a2"), transcript("a1")]);

    trail.lines(launched());
    trail.lines(launchAnswer());
    trail.lines(
      progress([
        { ...branchOne, state: "start" },
        { index: 2, label: "review:docs", agentId: "a2", model: "sonnet", state: "start" },
      ]),
    );

    const heads = trail
      .lines(notification())
      .filter((line) => line.text.startsWith("branch "))
      .map((line) => line.text.split(" · ")[0]);

    expect(heads).toEqual(["branch review:bugs", "branch review:docs"]);
  });

  test("says so plainly when the record cannot be read", () => {
    const trail = new WorkflowTrail(() => []);

    trail.lines(launched());

    const lines = trail.lines(notification());

    expect(lines[1]?.text).toContain("cannot be told");
    expect(ran(() => [])[1]?.text).toContain("left no record");
    expect(
      ran(() => {
        throw new Error("EACCES");
      })[1]?.text,
    ).toContain("could not be read");
  });

  test("is held to a few hundred lines, and says how much it left out", () => {
    const many = Array.from({ length: maxBranchLines + 20 }, (_, at) => ({
      type: "assistant",
      message: { content: [{ type: "text", text: `line ${at}` }] },
    }));
    const lines = ran(() => [transcript("a1", many)]);
    const last = lines[lines.length - 1];

    expect(lines).toHaveLength(1 + 1 + maxBranchLines + 1);
    expect(last?.text).toContain("lines past the limit were not kept");
  });
});

describe("the instruction a branch was given", () => {
  test("is the first thing said to it", () => {
    expect(instructionOf(transcript("a1").entries)).toBe("find the bugs in this change");
  });

  test("says so when there is none", () => {
    expect(instructionOf([{ type: "assistant", message: { content: [] } }])).toBe("No instruction was found.");
  });
});
