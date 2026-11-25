import declaredOpenRouter from "@agency/policy/providers/openrouter.toml";

import { machineCatalogue } from "./provider-facts";

// What OpenRouter is, to a bot that runs its models through a harness rather than by hand. Two
// things are known here and nothing else: where the gateway answers, and the name of the
// environment variable its key is read from. The stand-ins the emergency mode runs are picked
// from the listing below, read on this machine by the harness-providers binary
// (docs/provider-presence.md), and the run itself goes through the dsh harness, which declares this
// route and resolves the key per request out of the environment — so no completion is made here
// and no key ever becomes a value this module holds.

// What a chooser needs to know about one model and nothing else. The listing carries pricing,
// modality, tokenizer and a description apiece; a screen offering a stand-in asks four things —
// which model, what it is called, how much it can be told, and whether it costs anything.
export interface OpenRouterModel {
  id: string;
  name: string;
  contextLength: number | null;
  free: boolean;
}

/** Where the gateway answers, and what the harness points its route at: its declared address, or a
 * stand-in's the environment names as it is asked. */
const declaredGateway = (declaredOpenRouter as { gateway: { api: string; api_env: string } }).gateway;
export function openRouterApiBase(): string {
  return process.env[declaredGateway.api_env] || declaredGateway.api;
}

/** The one place the dormant dsh road's key was kept: the variable its route names. A member road reads
 * its key file instead. */
export const openRouterKeyEnv: string =
  (declaredOpenRouter as { dsh?: { key_env?: string } }).dsh?.key_env ?? "BMO_OPENROUTER_KEY";

// Every model OpenRouter offers, for a screen to choose a stand-in from: a `:free` id priced at
// zero is free. The listing needs no key at all, so the console can show the choice on a machine
// that has not been given one yet — which is the order the owner does it in: pick the model, then
// put the key in the environment.
export async function openRouterModels(): Promise<OpenRouterModel[]> {
  return ((await machineCatalogue("openrouter")) as { models: OpenRouterModel[] }).models;
}

