import { describe, expect, test } from "bun:test";

import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

import type { JobLine, JobPrompt, StatusView } from "../../../apps/web/shared/types";

// A worker's job line with its prompt, and the jobs that have just run on the live board, read from a stand-in
// bmo-job. What is pinned here is that each is asked on the route bmo-job states, for the run the screen was
// opened at, and read into the screens' shapes: seconds where bmo-job answers milliseconds, an item and its run
// out of a job's label, and a job no worker is named for the built-in worker's.

const job = standIn("JOB", (one) => {
  const url = new URL(one.path, "http://job");

  if (url.pathname === "/workers/worker-b/job") {
    return answer({
      job: {
        run: url.searchParams.get("run") ?? "job-unit-2",
        item: 41,
        title: "Item 41",
        purpose: null,
        launched_at: 1_790_000_000_000,
        finished_at: 1_790_000_090_500,
        tools: 12,
        models: [{ model: "model-a", input_tokens: 10, output_tokens: 20, cache_read_tokens: 30, cache_write_tokens: null }]
      }
    });
  }

  if (url.pathname === "/workers/worker-c/job") return answer({ job: null });
  if (url.pathname === "/workers/worker-b/job/prompt") return answer({ prompt: { text: "the prompt as read", english: "the prompt as handed" } });
  if (url.pathname === "/workers/worker-c/job/prompt") return answer({ prompt: null });

  if (url.pathname === "/jobs/recent") {
    const at = 1_790_000_000_000;

    return answer([
      { id: 9, unit: "job-unit-9", chat: 7, model: null, launched_at: at, finished_at: null, outcome: null, ok: null, label: { kind: "item", id: 41 }, purpose: null, worker: "worker-b" },
      { id: 8, unit: "job-unit-8", chat: 7, model: "model-a", launched_at: at, finished_at: at + 60_000, outcome: "done", ok: true, label: { kind: "item", id: 41, attempt: 2 }, purpose: null, worker: "worker-b" },
      { id: 7, unit: "job-unit-7", chat: 7, model: null, launched_at: at, finished_at: at + 5_000, outcome: "failed", ok: false, label: { kind: "itemless" }, purpose: "a look around", worker: null },
      { id: 6, unit: "job-unit-6", chat: 7, model: null, launched_at: at, finished_at: at + 1_000, outcome: "done", ok: true, label: { kind: "item", id: 40 }, purpose: null, worker: "worker-b" }
    ]);
  }

  return answer({ error: "no route", code: "route_absent" }, 404);
});

standIn("RECEIVER", (one) => (one.path === "/live" ? answer({ at: 1_790_000_100, compacting: [] }) : answer({ error: "no route", code: "not-found" }, 404)));

const line = await routeOf<{ job: JobLine | null }>("../../apps/web/server/api/agents/[name]/job.get");
const prompt = await routeOf<{ prompt: JobPrompt | null }>("../../apps/web/server/api/agents/[name]/job-prompt.get");
const status = await routeOf<StatusView>("../../apps/web/server/api/status.get");

describe("a worker's job line", () => {
  test("is the job bmo-job names for the run the screen was opened at, in seconds", async () => {
    job.asked.length = 0;

    const read = await asked(line, { params: { name: "worker-b" }, query: { run: "job-unit-1" } });

    expect(job.asked.map((one) => one.path)).toEqual(["/workers/worker-b/job?run=job-unit-1"]);
    expect(read.answer).toEqual({
      job: {
        run: "job-unit-1",
        todoId: 41,
        title: "Item 41",
        purpose: null,
        startedAt: 1_790_000_000,
        finishedAt: 1_790_000_090,
        tools: 12,
        models: [{ model: "model-a", inputTokens: 10, outputTokens: 20, cacheReadTokens: 30, cacheWriteTokens: null }]
      }
    });
  });

  test("asks for no run where the screen was opened at none, and is nothing for a worker with no job", async () => {
    job.asked.length = 0;

    expect((await asked(line, { params: { name: "worker-c" } })).answer).toEqual({ job: null });
    expect(job.asked.map((one) => one.path)).toEqual(["/workers/worker-c/job"]);
  });

  test("refuses a name no agent can have, without asking bmo-job", async () => {
    job.asked.length = 0;

    expect(await asked(line, { params: { name: "../workers" } })).toMatchObject({ status: 400 });
    expect(job.asked).toEqual([]);
  });
});

describe("the prompt of that job", () => {
  test("is its reading and the English its worker was handed, for the same run", async () => {
    job.asked.length = 0;

    expect((await asked(prompt, { params: { name: "worker-b" }, query: { run: "job-unit-1" } })).answer).toEqual({
      prompt: { text: "the prompt as read", english: "the prompt as handed" }
    });
    expect(job.asked.map((one) => one.path)).toEqual(["/workers/worker-b/job/prompt?run=job-unit-1"]);
    expect((await asked(prompt, { params: { name: "worker-c" } })).answer).toEqual({ prompt: null });
  });
});

describe("the jobs that have just run on the live board", () => {
  test("are the finished ones bmo-job lists, with their item, run and worker", async () => {
    job.asked.length = 0;

    const board = (await asked(status, {})).answer;

    expect(job.asked.map((one) => one.path)).toEqual(["/jobs/recent?limit=10"]);
    expect(board?.reachable).toBe(true);
    expect(board?.jobs).toEqual([
      { id: 8, unit: "job-unit-8", chatId: 7, model: "model-a", launchedAt: 1_790_000_000, finishedAt: 1_790_000_060, outcome: "done", ok: true, todoId: 41, attempt: 2, purpose: null, worker: "worker-b" },
      { id: 7, unit: "job-unit-7", chatId: 7, model: null, launchedAt: 1_790_000_000, finishedAt: 1_790_000_005, outcome: "failed", ok: false, todoId: null, attempt: 0, purpose: "a look around", worker: "worker" },
      { id: 6, unit: "job-unit-6", chatId: 7, model: null, launchedAt: 1_790_000_000, finishedAt: 1_790_000_001, outcome: "done", ok: true, todoId: 40, attempt: 1, purpose: null, worker: "worker-b" }
    ]);
  });
});
