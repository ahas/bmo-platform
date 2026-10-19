import { describe, expect, test } from "bun:test";

import {
  shownPart,
  textLanguage,
  translationCut,
  translationFailureLine,
  translationLanguage,
  translationParts
} from "../../apps/web/shared/translation";
import KO from "../fixtures/shared/translation.test.ko.json";

// How a text is cut for the translator and put back together. What is pinned here is that nothing is
// lost on the round trip — the parts joined with no translation at all are the text as it was — and
// that a builder turn's JSON answer survives it as JSON, with the words a machine keys on untouched.

describe("the language of a text", () => {
  test("tells English from Korean by the letters outside code", () => {
    expect(textLanguage(KO.line_18)).toBe("en");
    expect(textLanguage(KO.line_19)).toBe("ko");
    expect(textLanguage(KO.line_20)).toBe("ko");
  });

  test("is translated into the other one", () => {
    expect(translationLanguage("en")).toBe("ko");
    expect(translationLanguage("ko")).toBe("en");
  });
});

describe("translation parts", () => {
  test("pack sections while they fit, cut a section too large at its paragraphs, and join back into the text", () => {
    const section = (title: string) => `## ${title}\n\n${KO.line_31.repeat(40)}\n\n${KO.line_31.repeat(40)}\n\n`;
    const text = `${KO.line_32}${section("One")}${section("Two")}${section("Three")}`;
    const parts = translationParts(text, 150);

    expect(parts.join("")).toBe(text);
    expect(parts.length).toBeGreaterThan(3);
    expect(translationParts(text)).toEqual([text]);
    expect(translationParts("")).toEqual([""]);
  });

  test("a translated part that lost its trailing line keeps the block it ended", () => {
    expect(shownPart(KO.line_42, "## Title\n\nBody")).toBe("## Title\n\nBody\n\n");
    expect(shownPart(KO.line_43, "## Title\nBody")).toBe("## Title\nBody\n");
    expect(shownPart(KO.line_44, null)).toBe(KO.line_44);
  });
});

describe("what the bot says went wrong", () => {
  test("names the road where it has one", () => {
    expect(translationFailureLine({ reason: KO.line_50, provider: "engine-a", model: "model-a" })).toBe(
      KO.line_51
    );
    expect(translationFailureLine({ reason: KO.line_53, provider: null, model: null })).toBe(KO.line_53);
    expect(translationFailureLine(null)).toBe("");
  });
});

describe("cutting prose", () => {
  test("joins back into the text when nothing has been translated", () => {
    const text = "# Plan\n\nRead the files.\n\n## Then\n\nWrite the change.\n";
    const cut = translationCut(text);

    expect(cut.parts.join("")).toBe(text);
    expect(cut.join(cut.parts.map(() => null))).toBe(text);
  });

  test("puts the parts that came back in their places and leaves the rest in the original", () => {
    const text = "# Plan\n\nRead the files.\n\n## Then\n\nWrite the change.\n";
    const cut = translationCut(text, 10);

    expect(cut.parts.length).toBeGreaterThan(1);
    expect(cut.join(cut.parts.map((part, at) => (at === 0 ? KO.line_72 : null)))).toContain(KO.line_72_2);
    expect(cut.join(cut.parts.map((part, at) => (at === 0 ? KO.line_73 : null)))).toContain("Write the change.");
  });
});

const PLAN = JSON.stringify({
  stages: [
    {
      title: "Trace the console screens",
      tasks: [
        { title: "Find the panel", role: "scout", model: "claude-haiku-4-5", instruction: "Read the work order panel and report the files." },
        { title: "Check", role: "verifier", model: "claude-sonnet-5", instruction: "Confirm the report against the files." }
      ]
    }
  ]
});

describe("cutting a builder turn's JSON answer", () => {
  test("takes the sentences inside it and leaves the words a machine keys on alone", () => {
    const cut = translationCut(PLAN);

    // A role, a model and a one-word title are single words, and a sentence is not: the cut takes
    // what has a space in it and leaves the rest to stand as the machine wrote it.
    expect(cut.parts).toEqual([
      "Trace the console screens",
      "Find the panel",
      "Read the work order panel and report the files.",
      "Confirm the report against the files."
    ]);
  });

  test("comes back as JSON of the same shape, with what was not translated still in the original", () => {
    const cut = translationCut(PLAN);
    const joined = cut.join([KO.line_105, null, KO.line_105_2, null]);
    const read = JSON.parse(joined) as { stages: Array<{ title: string; tasks: Array<Record<string, string>> }> };

    expect(read.stages[0]!.title).toBe(KO.line_108);
    expect(read.stages[0]!.tasks[0]).toEqual({
      title: "Find the panel",
      role: "scout",
      model: "claude-haiku-4-5",
      instruction: KO.line_113
    });
    expect(read.stages[0]!.tasks[1]).toEqual({
      title: "Check",
      role: "verifier",
      model: "claude-sonnet-5",
      instruction: "Confirm the report against the files."
    });
  });

  test("gives back the answer unchanged when nothing has been translated", () => {
    const cut = translationCut(PLAN);

    expect(JSON.parse(cut.join(cut.parts.map(() => null)))).toEqual(JSON.parse(PLAN));
  });

  test("reads an answer wrapped in a fence", () => {
    const cut = translationCut("```json\n" + PLAN + "\n```");

    expect(cut.parts[0]).toBe("Trace the console screens");
    expect(JSON.parse(cut.join(cut.parts.map(() => null))).stages[0].tasks[0].role).toBe("scout");
  });

  test("cuts one string too large for a part into several, and joins them back into that one string", () => {
    const long = `${"Read the files and report what they say. ".repeat(40)}\n\n${"Then write the change down. ".repeat(40)}`;
    const cut = translationCut(JSON.stringify({ tasks: [{ role: "builder", instruction: long }] }), 150);

    expect(cut.parts.length).toBeGreaterThan(1);
    expect(cut.parts.join("")).toBe(long);
    expect(JSON.parse(cut.join(cut.parts.map(() => null))).tasks[0].instruction).toBe(long);
  });

  test("is prose where the text is not JSON at all", () => {
    expect(translationCut("Read the files.\n").parts).toEqual(["Read the files.\n"]);
    expect(translationCut("{ not json").parts).toEqual(["{ not json"]);
  });
});
