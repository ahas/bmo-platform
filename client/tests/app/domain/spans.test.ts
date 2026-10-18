import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";

import { BRANCH_WORD } from "../../../apps/web/shared/text-marks";
import { modelFace } from "../../../apps/web/app/domain/model-face";
import { shownInside, spanFace, toolFace } from "../../../apps/web/app/domain/spans";
import { modelLabel } from "../../../apps/web/app/domain/status";

import type { TodoSpan } from "../../../apps/web/shared/types";
import KO from "../../fixtures/app/domain/spans.test.ko.json";

function span(kind: TodoSpan["kind"], name: string, seconds = 1): TodoSpan {
  return { kind, name, seconds, times: 1, tokens: null };
}

describe("a tool by the name a person reads it under", () => {
  test("is the act the call performed", () => {
    expect(toolFace(wording, "Bash")).toBe(wording("tool.label.bash"));
    expect(toolFace(wording, "ToolSearch")).toBe(KO.line_20);
    expect(toolFace(wording, "TaskOutput")).toBe(wording("tool.label.task_output"));
  });

  test("says which server and which tool for an MCP call", () => {
    expect(toolFace(wording, "mcp__chrome-devtools__take_snapshot")).toBe(KO.line_25);
    expect(toolFace(wording, "mcp__web__web_fetch")).toBe(KO.line_26);
    expect(toolFace(wording, "mcp__toolbook__list")).toBe(KO.line_27);
  });

  test("keeps a server's own name where there is no word for it, rather than dropping the shape", () => {
    expect(toolFace(wording, "mcp__toolhost__toolhost_launch_job")).toBe("toolhost · toolhost_launch_job");
  });

  test("is the name it came with for a tool that first ran today", () => {
    expect(toolFace(wording, "Frobnicate")).toBe("Frobnicate");
  });
});

describe("a span as the whole of its name reads", () => {
  test("gives a Claude Workflow branch its role first and its model after", () => {
    expect(spanFace(wording, span("agent", KO.line_41))).toBe("build:guard · Opus 5");
    expect(spanFace(wording, span("agent", KO.line_42))).toBe(
      "investigate:leak-or-load · Haiku 4.5"
    );
  });

  test("falls back to the branch's number where the Claude Workflow named no role", () => {
    expect(spanFace(wording, span("agent", KO.line_48))).toBe(KO.line_48_2);
    expect(spanFace(wording, span("agent", KO.line_49))).toBe(KO.line_49);
  });

  test("drops the model half for a branch that was handed none", () => {
    expect(spanFace(wording, span("agent", KO.line_53))).toBe("build");
  });

  test("names a co-worker by the kind it is and what it was asked", () => {
    expect(spanFace(wording, span("agent", KO.line_57))).toBe(wording("tool.label.coworker_kind", { kind: KO.line_57_2 }));
    expect(spanFace(wording, span("agent", "Task"))).toBe(KO.line_58);
  });

  test("leaves a phase exactly as it came, the words being the owner's own", () => {
    expect(spanFace(wording, span("phase", KO.line_62))).toBe(KO.line_62);
  });
});

describe("what of the run's inside is worth showing", () => {
  test("drops the wait on a Claude Workflow where its branches are standing in the list", () => {
    const inside = [span("tool", "TaskOutput", 1464), span("agent", KO.line_68, 881)];

    expect(shownInside(inside).map((one) => one.name)).toEqual([KO.line_70]);
  });

  test("keeps that wait where there is no branch to stand in its place", () => {
    const inside = [span("tool", "TaskOutput", 1464), span("tool", "Bash", 101)];

    expect(shownInside(inside).map((one) => one.name)).toEqual(["TaskOutput", "Bash"]);
  });
});

// The word a Claude Workflow's branch is filed under is written by the agency as it gathers the span and read
// back here as the console draws it, from the data file beside the pack. A drift makes every branch fall through to
// the raw name it was stored under, which is the very thing this module exists to stop.
describe("a Claude Workflow's branch and a model", () => {
  test("a branch is read out from under the word it is filed under", () => {
    expect(spanFace(wording, span("agent", `${BRANCH_WORD} 1 · build · claude-opus-5`))).toBe("build · Opus 5");
  });

  // A model is no table the console keeps: a Claude identifier is read by the one rule, for the models the
  // catalogue names today and equally for one no list has ever held.
  test("every Claude model reads under the name the rule gives it", () => {
    for (const model of ["claude-opus-5-5", "claude-fable-5-1", "claude-sonnet-5", "claude-haiku-4-5-20251001", "claude-opus-4-8", "claude-opus-9"]) {
      expect(modelLabel(model)).toBe(modelFace(model));
    }

    expect(modelLabel("claude-opus-5-5")).toBe("Opus 5.5");
  });
});
