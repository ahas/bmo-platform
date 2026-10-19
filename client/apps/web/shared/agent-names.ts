// The chat agent's names, which stand in no code file and no language pack (docs/language-pack.md): the name its
// messages stand under, and the label rooms call it by. The deployment's settings name them, and the app and its
// server each register them once as they start (`plugins/names`), before anything is drawn or answered.

import type { Translate } from "./wording";

export interface ChatAgentNames {
  // The name the chat agent's messages stand under, as its registration gives it.
  name: string;
  // What a room calls it, the label its registration carries.
  label: string;
}

let names: ChatAgentNames = { name: "", label: "" };

/** The names the settings give, registered for every reader of `chatAgent`. */
export function registerChatAgent(given: ChatAgentNames): void {
  names = { name: given.name, label: given.label };
}

/** The chat agent's names; empty where the settings named none. */
export function chatAgent(): ChatAgentNames {
  return names;
}

/**
 * What the console is called, on a tab, at the head of its screens and under an installed copy's icon: the pack's
 * name for it, filled with the chat agent's label, or with the pack's word for its role where the settings name none.
 */
export function consoleName(t: Translate, key: "app.label.name" | "app.label.manifest_name" = "app.label.name"): string {
  return t(key, { agent: names.label !== "" ? names.label : t("common.label.chat_agent") });
}
