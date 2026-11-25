import type { AgentProviderName } from "./provider-names";

// What a run goes out on at one provider, as an order's entry names it (docs/provider-order.md, 「What a road is」): the model, the effort word, and on the registry's provider the answer's ceiling. The name a
// run carries beside it is a label its record keeps and nothing reads a choice from; no store keeps these.
export type ModelTier = string;
export type TierProvider = AgentProviderName;
// The registry's entry also names the ceiling of the answer's tokens; null is the registration's own.
export interface TierMapping { provider: TierProvider; tier: ModelTier; model: string; effort: string | null; answerTokens?: number | null }
