import { describe, expect, test } from "bun:test";

import { agencyEvent, jobEvent } from "../../../apps/web/app/domain/unit-events";

// bmo-job's and the agency's events read into the console's own, so a screen that reads back on the TODO, a work
// order, an item's talk or a turn ending hears them as it hears the receiver's.

describe("bmo-job's events", () => {
  test("the queue moving is the TODO moving", () => {
    expect(jobEvent({ id: 3, at: 1_790_000_000_000, kind: "queue_moved", detail: "worker-b" })).toEqual({
      id: 3,
      at: 1_790_000_000_000,
      type: "todo",
      chatId: 0,
      messageId: null
    });
  });

  test("an order changing is that item's work order, its standing read into the console's shape", () => {
    const event = jobEvent({
      id: 4,
      at: 1,
      kind: "order_changed",
      item: 41,
      order: {
        status: "active",
        phase: "implementation",
        stage: 2,
        updated_at: 1_790_000_000_000,
        tasks: [{ id: "build-0", state: "running", attempt: 2, job: 9, model: "model-a" }, { id: "build-1", state: "pending", attempt: 0 }]
      }
    });

    expect(event).toEqual({
      id: 4,
      at: 1,
      type: "work-order",
      chatId: 0,
      messageId: null,
      todoId: 41,
      workOrder: {
        status: "active",
        phase: "implementation",
        stage: 2,
        updatedAt: 1_790_000_000_000,
        tasks: [
          { id: "build-0", state: "running", attempt: 2, jobId: 9, model: "model-a", error: null },
          { id: "build-1", state: "pending", attempt: 0, jobId: null, model: "", error: null }
        ]
      }
    });
  });

  test("the talk moving is that item's talk", () => {
    expect(jobEvent({ id: 5, at: 1, kind: "talk_moved", item: 41, failure: "resting" })).toMatchObject({ type: "todo-talk", todoId: 41 });
  });

  test("an event with no number, an order with no item and a kind the console reads nothing of are nothing", () => {
    expect(jobEvent({ at: 1, kind: "queue_moved" })).toBeNull();
    expect(jobEvent({ id: 6, at: 1, kind: "order_changed", order: {} })).toBeNull();
    expect(jobEvent({ id: 7, at: 1, kind: "something_else" })).toBeNull();
  });
});

describe("the agency's turns", () => {
  test("a turn going out and a turn ending, with its session and run", () => {
    expect(agencyEvent({ kind: "turn-started", session: "worker-b", run: "run-a", at: 1_790_000_000_000 })).toEqual({
      id: 0,
      at: 1_790_000_000_000,
      type: "agency-turn",
      chatId: 0,
      messageId: null,
      phase: "started",
      session: "worker-b",
      run: "run-a"
    });
    expect(agencyEvent({ kind: "turn-ended", session: "worker-b", run: "run-a", outcome: "answered" })).toMatchObject({ type: "agency-turn", phase: "ended" });
  });

  test("another kind, or a turn naming no session, is nothing", () => {
    expect(agencyEvent({ kind: "turn-question", session: "worker-b", run: "run-a" })).toBeNull();
    expect(agencyEvent({ kind: "turn-ended", run: "run-a" })).toBeNull();
  });
});
