import { describe, expect, test } from "bun:test";

import "../../support/nitro";
import { answer, standIn } from "../../support/units";

import type { Asked } from "../../support/units";

// The board the TODO panel stands on, read from a stand-in bmo-job: its listings of every item's line, each item
// opened on its own, the queue's holds and the pool board. What is pinned here is how those readings become the
// rows the panel shows: which list each line stands in, what a row carries of its reading, how the conditions
// and a search narrow it, and what the board says of the queue's holds.

interface Line {
  id: number;
  title: string;
  state: Record<string, unknown> | null;
  status: string;
  marks: Record<string, unknown>[];
  model: { shown: string; marks: Record<string, unknown>[] } | null;
  worker: string | null;
  hold: { word: string; reason: string | null } | null;
}

function line(id: number, over: Partial<Line> = {}): Line {
  return { id, title: `Item ${id}`, state: null, status: "waiting", marks: [], model: null, worker: null, hold: null, ...over };
}

function reading(of: Line, over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    line: of,
    held_at: of.hold === null ? null : 1_790_000_000_000,
    merged_into: null,
    merged_here: [],
    repairs: null,
    repaired_by: [],
    requester: "owner",
    requester_id: 42,
    approved: of.state?.kind !== "pending_approval",
    priority: of.id,
    worker: of.worker ?? "worker",
    worker_assigned: of.worker !== null,
    cowork: of.marks.some((mark) => mark.kind === "cowork"),
    impact: null,
    job_impact: null,
    instruction: `Do item ${of.id}`,
    draft: null,
    owner_note: null,
    notes: [],
    merged_work: [],
    questions: [],
    time: null,
    report: null,
    unfinished: null,
    ...over
  };
}

// What the stand-in holds: the lines of each listing, the readings by number, the holds and the pool board.
const held = {
  open: [] as Line[],
  aside: [] as Line[],
  merged: [] as Line[],
  readings: new Map<number, Record<string, unknown>>(),
  holds: { paused: false, deploy_hold: null as unknown, deploy_waits_on: [] as number[] },
  pool: { read_at: 0, buckets: [] as unknown[], workers: [] as unknown[] }
};

function stand(lines: { open?: Line[]; aside?: Line[]; merged?: Line[] }, readings: Record<string, unknown>[] = []): void {
  held.open = lines.open ?? [];
  held.aside = lines.aside ?? [];
  held.merged = lines.merged ?? [];
  held.readings = new Map(
    [...held.open, ...held.aside, ...held.merged].map((one) => [one.id, readings.find((r) => (r.line as Line).id === one.id) ?? reading(one)])
  );
}

const job = standIn("JOB", (asked: Asked) => {
  const url = new URL(asked.path, "http://job");

  if (url.pathname === "/todos") {
    const chat = url.searchParams.get("chat");
    const from = url.searchParams.has("held") ? held.aside : url.searchParams.has("merged") ? held.merged : held.open;

    return answer(chat === null ? from : from.filter((one) => one.id % 2 === Number(chat) % 2));
  }

  const opened = /^\/todos\/(\d+)$/.exec(url.pathname);

  if (opened !== null) {
    const found = held.readings.get(Number(opened[1]));

    return found === undefined ? answer({ error: "no such item", code: "item_absent" }, 404) : answer(found);
  }

  if (url.pathname === "/queue/holds") return answer(held.holds);
  if (url.pathname === "/coworker-pool") return answer(held.pool);

  return answer({ error: "no route", code: "route_absent" }, 404);
});

const { readTodoBoard, readTodoItem, readTodoItems, readTodoNotes, searchedFor, doneLimit } = await import("../../../apps/web/server/utils/todos");
const { NO_CONDITIONS } = await import("../../../apps/web/app/domain/todos");

const ids = (rows: { id: number }[]) => rows.map((row) => row.id);

describe("the lists each line stands in", () => {
  test("the queue, what waits on approval, what is set aside, what is merged and what is finished", async () => {
    stand({
      open: [
        line(1),
        line(2, { state: { kind: "pending_approval", requester: "owner" } }),
        line(3, { state: { kind: "done" }, status: "done" }),
        line(4, { state: { kind: "done" }, marks: [{ kind: "merged_into", id: 1 }] }),
        line(5, { state: { kind: "done" }, status: "done" })
      ],
      aside: [line(6, { hold: { word: "plain", reason: "not now" } })],
      merged: [line(7, { state: { kind: "merging_into", id: 1 } })]
    });

    const board = await readTodoBoard();

    expect(ids(board.queue)).toEqual([1]);
    expect(ids(board.pending)).toEqual([2]);
    expect(ids(board.held)).toEqual([6]);
    expect(ids(board.merged)).toEqual([7]);
    // The finished stand newest first, by number, which is the order they were filed in.
    expect(ids(board.done)).toEqual([5, 4, 3]);
    expect(board.doneTotal).toBe(3);
    expect(board.standing).toEqual({ queue: 1, running: 0, pending: 1, held: 1, merged: 1 });
  });

  test("a row wound down or still being stopped stands in the queue, a plain hold among the holds", async () => {
    stand({
      open: [line(1, { hold: { word: "pause", reason: null } }), line(2, { hold: { word: "stopping", reason: "wrong turn" } })],
      aside: [line(1, { hold: { word: "pause", reason: null } }), line(2, { hold: { word: "stopping", reason: "wrong turn" } }), line(3, { hold: { word: "failure", reason: null } })]
    });

    const board = await readTodoBoard();

    expect(ids(board.queue)).toEqual([1, 2]);
    expect(ids(board.held)).toEqual([3]);
    expect(board.queue.map((row) => row.heldKind)).toEqual(["pause", "stop"]);
    expect(board.queue.map((row) => row.holdPending)).toEqual([false, true]);
    expect(board.held[0]?.heldKind).toBe("failure");
  });

  test("the finished list is read as far back as the panel asked, and counted whole", async () => {
    stand({ open: [1, 2, 3, 4].map((id) => line(id, { state: { kind: "done" } })) });
    job.asked.length = 0;

    const board = await readTodoBoard({ limit: 2 });

    expect(ids(board.done)).toEqual([4, 3]);
    expect(board.doneTotal).toBe(4);
    // The finished further back are not opened at all.
    expect(job.asked.some((one) => one.path === "/todos/1")).toBe(false);
  });
});

describe("what a row carries of its reading", () => {
  test("its time, its road and the model it asked for, its worker and its words", async () => {
    const one = line(9, { worker: "worker-b", model: { shown: "provider-a/model-a", marks: [{ kind: "requested", model: "provider-b::model-b" }, { kind: "changed" }] } });

    stand({ open: [one] }, [
      reading(one, {
        owner_note: "keep it short",
        report: "done as asked",
        time: {
          attempts: [{ job: 1, took_ms: 60_000, outcome: "failed" }, { job: 2, took_ms: 30_000, outcome: "done" }],
          total_ms: 90_500,
          finished: 2,
          phases: [{ kind: "phase", name: "agent run", called: null, said: null, seconds: 80, times: 1, tokens: null }],
          split: null,
          inside: [{ kind: "tool", name: "Bash", called: null, said: null, seconds: 40, times: 12, tokens: null }],
          inside_more: 0,
          usage: [],
          missing: 0
        },
        unfinished: { text: "the docs", resolved_at: 1_790_000_100_000, resolution: "raised as #10", raised: [] },
        notes: [{ id: 3, author: "owner", text: "also this", added_at: 1_790_000_000_000 }]
      })
    ]);

    const item = await readTodoItem(9);

    expect(item).toMatchObject({
      id: 9,
      worker: "worker-b",
      model: "provider-b::model-b",
      ranOn: { provider: "provider-a", model: "model-a", changed: true },
      attempts: 2,
      seconds: 90,
      note: "keep it short",
      report: "done as asked",
      unfinished: "the docs",
      unfinishedResolvedAt: 1_790_000_100,
      unfinishedResolution: "raised as #10"
    });
    expect(item?.spans).toEqual([
      { kind: "phase", name: "agent run", seconds: 80, times: 1, tokens: null },
      { kind: "tool", name: "Bash", seconds: 40, times: 12, tokens: null }
    ]);
    expect(await readTodoNotes(9)).toMatchObject([{ id: 3, todoId: 9, author: "owner", text: "also this", addedAt: 1_790_000_000 }]);
  });

  test("a model nothing has gone out on yet is the one it asked for, and an unassigned row names no worker", async () => {
    const one = line(9, { model: { shown: "auto", marks: [] } });

    stand({ open: [one] });

    expect(await readTodoItem(9)).toMatchObject({ model: "auto", ranOn: null, worker: null, seconds: null });
  });

  test("a row the pool board names at work is running", async () => {
    stand({ open: [line(1), line(2)] });
    held.pool = { read_at: 0, buckets: [{ tiles: [{ state: "working", work: { kind: "cowork", item: 2 } }] }], workers: [{ tile: { state: "idle" } }] };

    const board = await readTodoBoard();

    held.pool = { read_at: 0, buckets: [], workers: [] };
    expect(board.queue.map((row) => row.running)).toEqual([false, true]);
    expect(board.standing.running).toBe(1);
  });

  test("numbers with no item behind them are left out, and the rest come in number order", async () => {
    stand({ open: [line(1), line(2)] });

    expect(ids(await readTodoItems([2, 99, 1, 2]))).toEqual([1, 2]);
    expect(await readTodoItem(99)).toBeNull();
  });
});

describe("the board narrowed", () => {
  test("by a worker, an unassigned row being the built-in worker's, co-work and an unfinished note", async () => {
    stand({
      open: [
        line(1, { worker: "worker-b" }),
        line(2, { marks: [{ kind: "cowork" }] }),
        line(3, { state: { kind: "done" }, marks: [{ kind: "unfinished" }] }),
        line(4, { state: { kind: "done" }, marks: [{ kind: "unfinished_resolved" }] })
      ]
    });

    expect(ids((await readTodoBoard({ conditions: { ...NO_CONDITIONS, worker: "worker-b" } })).queue)).toEqual([1]);
    expect(ids((await readTodoBoard({ conditions: { ...NO_CONDITIONS, worker: "worker" } })).queue)).toEqual([2]);
    expect(ids((await readTodoBoard({ conditions: { ...NO_CONDITIONS, cowork: "on" } })).queue)).toEqual([2]);
    expect(ids((await readTodoBoard({ conditions: { ...NO_CONDITIONS, unfinished: "open" } })).done)).toEqual([3]);
    expect(ids((await readTodoBoard({ conditions: { ...NO_CONDITIONS, unfinished: "resolved" } })).done)).toEqual([4]);
  });

  test("the choices and the standing notes are read from every line and are not narrowed", async () => {
    stand({ open: [line(1, { worker: "worker-b" }), line(2), line(3, { state: { kind: "done" }, marks: [{ kind: "unfinished" }] })] });

    const board = await readTodoBoard({ conditions: { ...NO_CONDITIONS, worker: "worker-b" } });

    expect(board.choices.workers.sort()).toEqual(["worker", "worker-b"]);
    expect(board.standingNotes).toEqual([{ id: 3, title: "Item 3" }]);
  });

  test("to one room, which the listings are asked for", async () => {
    stand({ open: [line(1), line(2), line(3)] });

    const board = await readTodoBoard({ room: 7 });

    expect(ids(board.queue)).toEqual([1, 3]);
    expect(job.asked.filter((one) => one.path.startsWith("/todos?")).slice(-3).map((one) => one.path)).toEqual(["/todos?all&chat=7", "/todos?all&held&chat=7", "/todos?all&merged&chat=7"]);
  });

  test("by a search, which reaches an open row whole and a finished one by its line", async () => {
    const asked = line(1);

    stand({ open: [asked, line(2), line(3, { state: { kind: "done" }, title: "the console tooltip" }), line(4, { state: { kind: "done" } })] }, [
      reading(asked, { instruction: "rename the tooltip" })
    ]);

    const board = await readTodoBoard({ query: "tooltip" });

    expect(ids(board.queue)).toEqual([1]);
    expect(ids(board.done)).toEqual([3]);
    expect(board.doneTotal).toBe(1);
  });
});

describe("the queue's holds", () => {
  test("the owner's pause and a deployment's hold ride on the board", async () => {
    stand({ open: [line(1)] });
    held.holds = { paused: true, deploy_hold: { since: 10, until: 20, commits: [{ commit: "abc", todoId: 1 }], waitingFor: [1, 2.5] }, deploy_waits_on: [] };

    const board = await readTodoBoard();

    held.holds = { paused: false, deploy_hold: null, deploy_waits_on: [] };
    expect(board.paused).toBe(true);
    expect(board.deployHold).toEqual({ since: 10, until: 20, commits: [{ commit: "abc", todoId: 1 }], breaks: null, waitingOn: null, waitingFor: [1], passing: [] });
    expect((await readTodoBoard()).deployHold).toBeNull();
  });
});

describe("what a caller asks for", () => {
  test("whitespace alone is no search, and a search is cut to its limit", () => {
    expect(searchedFor("   ")).toBeNull();
    expect(searchedFor(undefined)).toBeNull();
    expect(searchedFor(" tip ")).toBe("tip");
    expect(searchedFor("x".repeat(500))?.length).toBe(200);
  });

  test("how many finished items to read, within the most there is", () => {
    expect(doneLimit("40")).toBe(40);
    expect(doneLimit("0")).toBe(20);
    expect(doneLimit("nonsense")).toBe(20);
    expect(doneLimit("100000")).toBe(500);
  });
});
