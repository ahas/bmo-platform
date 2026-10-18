import { describe, expect, test } from "bun:test";

import { draftKey, draftText } from "../../../apps/web/app/domain/drafts";
import KO from "../../fixtures/app/domain/drafts.test.ko.json";

describe("where a draft is kept", () => {
  test("one key to a page, under the prefix the browser's other choices use", () => {
    expect(draftKey("room", -1001)).toBe("bmo-web.draft.room.-1001");
    expect(draftKey("gate", "content-judge")).toBe("bmo-web.draft.gate.content-judge");
    expect(draftKey("launch", "worker")).toBe("bmo-web.draft.launch.worker");
  });

  test("no two prompts share one key", () => {
    expect(draftKey("room", -1001)).not.toBe(draftKey("room", -1002));
    expect(draftKey("gate", "output-decide")).not.toBe(draftKey("gate", "output-verify"));
    expect(draftKey("launch", "search")).not.toBe(draftKey("launch", "browse"));
  });
});

describe("reading a draft back", () => {
  test("gives the words as they were typed", () => {
    expect(draftText(KO.line_21)).toBe(KO.line_21);
  });

  test("gives nothing where the browser holds nothing", () => {
    expect(draftText(null)).toBe("");
  });
});
