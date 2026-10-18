import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";
import { BASE_NONE, basePromptChoices, basePromptSave, choiceValue, choosersText, chosenPrompt, savingResets } from "../../../apps/web/app/domain/base-prompts";

import type { BasePromptView } from "../../../apps/web/shared/agency";
import KO from "../../fixtures/app/domain/base-prompts.test.ko.json";

// The base prompts as the agents screen reads and changes them (the agency's docs/base-prompts.md).

const t = wording;
const prompt: BasePromptView = {
  name: "claude-code",
  label: KO.line_13,
  text: "You are an interactive agent.",
  updatedAt: null,
  updatedBy: null,
  characters: 29,
  choosers: [{ agent: "tester", label: KO.line_18 }],
};

describe("a choice", () => {
  test("is none or a prompt by its label, and none goes back to the bot as null", () => {
    expect(basePromptChoices(t, [prompt])).toEqual([{ label: KO.line_23, value: BASE_NONE }, { label: KO.line_23_2, value: "claude-code" }]);
    expect(choiceValue({ choices: { tester: "claude-code" } }, "tester")).toBe("claude-code");
    expect(choiceValue({ choices: {} }, "tester")).toBe(BASE_NONE);
    expect(chosenPrompt(BASE_NONE)).toBeNull();
    expect(choosersText(t, prompt)).toBe(KO.line_27);
    expect(choosersText(t, { choosers: [{ agent: "companion-a", label: "companion-a" }] })).toBe("companion-a");
    expect(choosersText(t, { choosers: [] })).toBe(wording("agents.label.base_prompt_unused"));
  });
});

describe("a save of the modal", () => {
  test("makes a new prompt whole, sends only what changed of a held one, and asks first where agents choose it", () => {
    expect(basePromptSave({ name: "deepseek", label: " ", text: "x" }, null)).toEqual({ action: "create", name: "deepseek", label: "deepseek", text: "x" });
    expect(basePromptSave({ name: "claude-code", label: KO.line_36, text: "changed" }, prompt)).toEqual({ action: "save", name: "claude-code", text: "changed" });
    expect(savingResets({ name: "claude-code", label: "renamed", text: prompt.text }, prompt)).toBe(false);
    expect(savingResets({ name: "claude-code", label: prompt.label, text: "changed" }, prompt)).toBe(true);
    expect(savingResets({ name: "claude-code", label: prompt.label, text: "changed" }, { ...prompt, choosers: [] })).toBe(false);
  });
});
