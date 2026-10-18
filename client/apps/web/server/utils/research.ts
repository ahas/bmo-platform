import { unitFetch } from "./units";

import type { ResearchSettingsView } from "#shared/research";

// The research settings, bmo-artifact's (bmo-artifact's `docs/api.md`, 「Research settings」): read whole, and
// changed one action at a time. Its refusals are said in the pack's words by their codes; a profile's own faults
// are checked by the form before it sends, so a profile refused here is said as refused.

const RESEARCH_CODES: Readonly<Record<string, string>> = {
  not_json: "research.error.refused",
  bad_action: "research.error.refused",
  bad_pool_limit: "research.error.bad_pool_limit",
  bad_capacity: "research.error.bad_capacity",
  bad_key: "research.error.bad_key",
  no_values: "research.error.refused",
  invalid_profile: "research.error.invalid_profile",
  agency_failed: "research.error.agency_failed"
};

/** The pool, the profiles and the engines as bmo-artifact answers them. */
export async function researchRead(): Promise<ResearchSettingsView> {
  return await unitFetch<ResearchSettingsView>("ARTIFACT", "/research-settings", undefined, { codes: RESEARCH_CODES });
}

/** One change handed to bmo-artifact, which checks it and writes it. */
export async function researchChange(change: unknown): Promise<{ ok: true }> {
  return await unitFetch<{ ok: true }>(
    "ARTIFACT",
    "/research-settings",
    { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(change) },
    { codes: RESEARCH_CODES, passStatus: [422] }
  );
}
