import { wording } from "#shared/wording";

interface TranslationBody {
  root?: unknown;
  path?: unknown;
}

// Asking for a document's translation, which the gate unit's translator would take; it serves the viewer no route
// yet, so the asking is answered as unavailable.
export default defineEventHandler(async (event): Promise<{ ok: true; taken: number }> => {
  await requireUserSession(event);

  const body = await readBody<TranslationBody>(event);

  requireDocument(body?.root, body?.path);

  throw fail(503, wording("documents.error.translation_unavailable"));
});
