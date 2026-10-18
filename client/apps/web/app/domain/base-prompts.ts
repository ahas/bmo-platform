import type { BasePromptView, BasePromptsView } from "#shared/agency";
import type { Translate } from "#shared/wording";

// The base prompts as the agents screen reads and changes them (the agency's docs/base-prompts.md): each a whole
// text an agent's system prompt opens with, and each agent's choice of one or none.

// What a choice of none is written as in a list: no name takes it, since a name opens with a letter.
export const BASE_NONE = "-";

// Every choice an agent may make: none, then every prompt by its label.
export function basePromptChoices(t: Translate, prompts: readonly Pick<BasePromptView, "name" | "label">[]): { label: string; value: string }[] {
  return [{ label: t("agents.label.base_prompt_none"), value: BASE_NONE }, ...prompts.map((one) => ({ label: one.label, value: one.name }))];
}

// An agent's choice as a list holds it, and as the bot takes it back.
export function choiceValue(view: Pick<BasePromptsView, "choices"> | null | undefined, agent: string): string {
  return view?.choices[agent] ?? BASE_NONE;
}

export function chosenPrompt(value: string): string | null {
  return value === BASE_NONE ? null : value;
}

// The agents that choose a prompt, each by its label with its name where they differ, since a worker
// and a companion may share a label; or that none does.
export function choosersText(t: Translate, prompt: Pick<BasePromptView, "choosers">): string {
  return prompt.choosers.length === 0
    ? t("agents.label.base_prompt_unused")
    : prompt.choosers.map((one) => (one.label === one.agent ? one.label : `${one.label} (${one.agent})`)).join(", ");
}

// A prompt's length, with its thousands separators.
export function charactersText(t: Translate, prompt: Pick<BasePromptView, "characters">): string {
  return t("agents.label.base_prompt_characters", { count: prompt.characters.toLocaleString("en-US") });
}

export function timeText(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString("ko-KR", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

// When and by whom a prompt was last saved; nothing for a seed nobody has saved since.
export function savedText(prompt: Pick<BasePromptView, "updatedAt" | "updatedBy">): string {
  return prompt.updatedAt === null ? "" : [timeText(prompt.updatedAt), prompt.updatedBy ?? ""].filter((one) => one !== "").join(" · ");
}

// A prompt as the modal edits it.
export interface BasePromptForm {
  name: string;
  label: string;
  text: string;
}

export function emptyBasePromptForm(): BasePromptForm {
  return { name: "", label: "", text: "" };
}

export function basePromptForm(prompt: BasePromptView): BasePromptForm {
  return { name: prompt.name, label: prompt.label, text: prompt.text };
}

// What a save of the modal asks: a new prompt whole, or only what changed of one held.
export function basePromptSave(form: BasePromptForm, before: BasePromptView | null): Record<string, unknown> {
  const name = form.name.trim();
  const label = form.label.trim() === "" ? name : form.label.trim();

  if (before === null) return { action: "create", name, label, text: form.text };

  return { action: "save", name, ...(label === before.label ? {} : { label }), ...(form.text === before.text ? {} : { text: form.text }) };
}

// Whether a save changes the text that agents choose, which starts their sessions afresh.
export function savingResets(form: BasePromptForm, before: BasePromptView | null): boolean {
  return before !== null && before.choosers.length > 0 && form.text !== before.text;
}

// Where in the modal a refusal stands: the field the bot named, or the modal as a whole.
export function basePromptRefusalPlace(field: string | null | undefined): "name" | "label" | "text" | "form" {
  return (["name", "label", "text"] as const).find((one) => one === field) ?? "form";
}
