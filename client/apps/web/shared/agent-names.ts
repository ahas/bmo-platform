// The chat agent's names, which stand in no code file and no language pack (docs/language-pack.md): the name its
// messages stand under, and the label rooms call it by. The deployment's settings name them, and the app and its
// server each register them once as they start (`plugins/names`), before anything is drawn or answered.

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
