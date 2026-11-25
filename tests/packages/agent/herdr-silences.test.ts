import "@agency/test-support/setup";

import { expect, test } from "bun:test";

import { herdrSilencesPath } from "@agency/agent/herdr-silences";

// The machine's record of the calls herdr left unanswered (docs/agent-control.md, 「Pressure told as
// a fact」), kept in a file of the test's own.

test("is the machine's own file unless the environment names another", () => {
  expect(herdrSilencesPath({})).toEndWith("/bmo-herdr-silences.jsonl");
  expect(herdrSilencesPath({ BMO_HERDR_SILENCES: "/elsewhere/s.jsonl" })).toBe("/elsewhere/s.jsonl");
});
