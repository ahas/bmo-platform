import type { AgentProvider } from "./harness";

// What every provider's own file of the shared policy declares (docs/provider-presence.md), read the
// same way whoever the provider is: each provider imports its own file and hands it here.

export interface DeclaredProvider {
  label: string;
  caps_answer_tokens: boolean;
  effort: {
    words: string[];
    none_held: boolean;
    launch: "required" | "refused" | "either";
    fault?: string;
  };
}

// What a provider takes from its file. The lines it carries name the deployment's lines, so they are
// read by the deployment that composes the provider, never here.
export type ProviderDeclaration = Pick<AgentProvider, "label" | "efforts" | "effortFault" | "capsAnswerTokens">;

export function providerDeclaration(file: unknown): ProviderDeclaration {
  const declared = file as DeclaredProvider;
  const { words, none_held: noneHeld, launch, fault } = declared.effort;
  const refusal = fault ?? "this provider refuses the effort";

  return {
    label: declared.label,
    efforts: [...(noneHeld ? [null] : []), ...words],
    effortFault: (effort) => (launch === "required" && effort === null) || (launch === "refused" && effort !== null) ? refusal : null,
    capsAnswerTokens: declared.caps_answer_tokens,
  };
}
