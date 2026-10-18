import { wording } from "#shared/wording";

import { unitFetch } from "./units";

import type { UnitCall } from "./units";

// The agency's routes as its document states them (the agency's `docs/api.md`, 「The control port」): JSON in
// camelCase under its token, a refusal answered with the status's kebab-case code and an English `error`, and a
// refusal about one field answered 400 with the `field` beside its `error`. The English is for an agent to read,
// so a person is told the code's sentence from the pack, and a refused field is said under that field.

// The agency's codes, each with the pack's key for its sentence.
const AGENCY_CODES: Readonly<Record<string, string>> = {
  "bad-request": "agents.error.agency_bad_request",
  "not-found": "agents.error.agency_not_found",
  conflict: "agents.error.agency_conflict",
  "unprocessable-entity": "agents.error.agency_unprocessable",
  "service-unavailable": "agents.error.agency_unavailable",
  "internal-server-error": "agents.error.agency_failed",
  "bad-gateway": "agents.error.agency_failed"
};

// A listing the agency reads from a provider's own catalogue takes as long as that provider answers.
export const LISTING_TIMEOUT_MS = 20_000;

/** One call on the agency, its refusals said in the pack's words. */
export async function agencyFetch<T>(path: string, init?: RequestInit, call: Omit<UnitCall, "codes"> = {}): Promise<T> {
  try {
    return await unitFetch<T>("AGENCY", path, init, { ...call, codes: AGENCY_CODES });
  } catch (error) {
    throw fieldRefusal(error);
  }
}

/** A read of the agency's. */
export async function agencyRead<T>(path: string, call: Omit<UnitCall, "codes"> = {}): Promise<T> {
  return await agencyFetch<T>(path, undefined, call);
}

/**
 * One change carried to the agency, which checks it and writes it, recorded under the signed-in user: the name
 * the change is kept under is this app's word for who is signed in, never one the browser sends.
 */
export async function agencyChange<T>(path: string, body: Record<string, unknown>, user: string, timeoutMs = 5_000): Promise<T> {
  return await agencyPost<T>(path, { ...body, by: user }, { timeoutMs });
}

/** A POST of a JSON body to the agency. */
export async function agencyPost<T>(path: string, body: unknown, call: Omit<UnitCall, "codes"> = {}): Promise<T> {
  return await agencyFetch<T>(path, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) }, call);
}

// A refusal about one field is said by the one sentence for a refused value; the screen puts it under the
// field the refusal names.
function fieldRefusal(error: unknown): unknown {
  const asked = error as { statusCode?: unknown; data?: { field?: unknown } } | null;

  return asked?.statusCode === 400 && typeof asked.data?.field === "string"
    ? createError({ statusCode: 400, message: wording("agents.error.field_refused"), data: { field: asked.data.field } })
    : error;
}
