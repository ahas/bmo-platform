import { wording } from "#shared/wording";

import type { components } from "@agency/client/schema";

type PromptDocumentChange = components["schemas"]["PromptDocumentChange"];
import type { BasePromptsView } from "#shared/agency";

// One change of the base prompts — one made, saved, removed or chosen by an agent — carried to the agency under
// the signed-in user, who it is kept as made by (the agency's `docs/api.md`, 「prompt-documents」).
function textOf(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}

export default defineEventHandler(async (event): Promise<BasePromptsView> => {
  const session = await requireUserSession(event);
  const body = await readBody<Record<string, unknown>>(event);

  if (typeof body !== "object" || body === null || typeof body.action !== "string") throw fail(400, wording("agents.error.base_prompt_action"));

  const change: PromptDocumentChange = {
    action: body.action,
    name: textOf(body.name),
    label: textOf(body.label),
    text: typeof body.text === "string" ? body.text : null,
    agent: textOf(body.agent),
    prompt: textOf(body.prompt),
    by: session.user.name
  };
  const documentName = change.name ?? change.prompt ?? change.agent ?? "";

  await agencyCall(agency().PATCH("/prompt-documents/{documentName}", { params: { path: { documentName } }, body: change }));

  const page = await agencyCall(agency().GET("/prompt-documents", { params: { query: { kind: "base" } } }));

  return basePromptsOf(page.items);
});
