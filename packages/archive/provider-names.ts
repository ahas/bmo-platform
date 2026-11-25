// The names the owner chooses a road by, which is the whole of this family: one name per provider
// this build carries. Every other name the program has for the same providers — the mode a reply
// and a usage row are stored under, the provider column of a tier mapping, the route modes less
// `auto`, which is a policy over these and never a program — reads it rather than writing them out
// again. The archive stores these names in its rows, so the list stands with the archive; each
// provider's implementation is the agent base's (`harness.ts`).
export const agentProviderNames = ["claude-code", "codex", "openrouter", "registered", "deepseek"] as const;

export type AgentProviderName = (typeof agentProviderNames)[number];

// Where a reading belongs: the provider that reported it and, where that provider holds logins,
// the login it was reported by. Built only by `agentSlot`, which decides whether an account
// belongs on it.
export interface AgentSlot {
  provider: AgentProviderName;
  account: string | null;
}
