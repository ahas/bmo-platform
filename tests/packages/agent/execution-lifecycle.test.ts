import "@agency/test-support/setup";

import { test, expect } from "bun:test";
import { paneTurnEnding } from "@agency/agent/worker-pane";

test("a turn's own collector closes its row only when herdr answers that the pane is gone", () => {
  expect(paneTurnEnding(true, "exited", "absent")).toEqual({ state: "exited", ended: true });
  expect(paneTurnEnding(true, "timeout", "absent")).toEqual({ state: "timeout", ended: true });
  expect(paneTurnEnding(true, "cancelled", "absent")).toEqual({ state: "cancelled", ended: true });
  expect(paneTurnEnding(false, "exited", "absent")).toEqual({ state: "launch-error", ended: true });
});

// The two answers that are not "gone" are different facts and the row must not read them as one.
// A pane herdr still holds may still be working; a herdr that said nothing said nothing.
test("a pane herdr still holds and a herdr that did not answer both keep the row open", () => {
  expect(paneTurnEnding(true, "exited", "present")).toEqual({ state: "unknown", ended: false });
  expect(paneTurnEnding(true, "exited", "unanswered")).toEqual({ state: "unknown", ended: false });
  expect(paneTurnEnding(true, "timeout", "unanswered")).toEqual({ state: "unknown", ended: false });
});

