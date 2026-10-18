import { unitFetch } from "./units";

// A companion added with a room of the console's own, and taken out again, as the receiver carries both
// (the receiver's `docs/boundary-api.md`, 「The web unit」): the receiver writes the room's half and asks the
// agency for the agent's. Its refusals are its `CompanionRefusal` codes (the receiver's `docs/codes.md`), each
// said by the pack's sentence, and a refusal about one field names it so the form says it there.

const COMPANION_CODES: Readonly<Record<string, string>> = {
  "not-companion": "agents.error.companion_not_companion",
  name: "agents.error.companion_name",
  "name-taken": "agents.error.companion_name_taken",
  label: "agents.error.companion_label",
  provider: "agents.error.companion_provider",
  model: "agents.error.companion_model",
  worker: "agents.error.companion_worker",
  room: "agents.error.companion_room",
  "seeded-companion": "agents.error.companion_seeded",
  action: "agents.error.companion_change"
};

// The form field each refusal is about.
const FIELD_OF: Readonly<Record<string, string>> = {
  name: "name",
  "name-taken": "name",
  label: "label",
  provider: "provider",
  model: "model",
  worker: "worker",
  room: "room"
};

/** A POST to the receiver's companion routes, its refusal naming the field it is about. */
export async function companionCall<T>(path: string, body: Record<string, unknown>, timeoutMs: number): Promise<T> {
  try {
    return await unitFetch<T>(
      "RECEIVER",
      path,
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) },
      { timeoutMs, codes: COMPANION_CODES, passStatus: [409] }
    );
  } catch (error) {
    const refused = error as { statusCode?: number; message?: string; data?: { code?: unknown } } | null;
    const code = typeof refused?.data?.code === "string" ? refused.data.code : null;

    throw code !== null && code in FIELD_OF
      ? createError({ statusCode: refused?.statusCode ?? 400, message: refused?.message ?? "", data: { field: FIELD_OF[code] } })
      : error;
  }
}
