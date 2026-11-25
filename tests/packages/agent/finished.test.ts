import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { readClaudeOutput } from "@agency/agent/claude";

const event = (value: Record<string, unknown>): string => JSON.stringify(value);
const tasksChanged = (tasks: { task_id: string; description: string }[]): string =>
  event({ type: "system", subtype: "background_tasks_changed", tasks });
const result = event({ type: "result", result: "All done.", session_id: "s1", is_error: false });

describe("the background work a run leaves behind", () => {
  test("work collected before the run ended leaves nothing live", () => {
    const stdout = [
      tasksChanged([{ task_id: "t1", description: "research workflow" }]),
      tasksChanged([]),
      result,
    ].join("\n");

    expect(readClaudeOutput(stdout, "", 0).liveTasks).toEqual([]);
  });

  test("what the CLI kills after the result frame does not count as collected", () => {
    const stdout = [
      tasksChanged([{ task_id: "t1", description: "research workflow" }]),
      result,
      tasksChanged([]),
    ].join("\n");

    expect(readClaudeOutput(stdout, "", 0).liveTasks).toHaveLength(1);
  });

  test("a run with no background task of its own has nothing live", () => {
    expect(readClaudeOutput(result, "", 0).liveTasks).toEqual([]);
  });
});

// The one place a failure's reason is written, and the one place it is read back. Two records
// carry it — the row a work order keeps and the line a room hears — and they are written by
// different files, so what keeps them saying the same thing is that both go through here.

