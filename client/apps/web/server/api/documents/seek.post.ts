import { DOCUMENT_QUESTION_LIMIT } from "#shared/config";
import { wording } from "#shared/wording";

import type { DocumentSeekResult } from "#shared/types";

interface SeekBody {
  question?: unknown;
  root?: unknown;
  path?: unknown;
}

// Where a question is answered, in one document or in every document. The seek gate is the gate unit's, which
// serves the viewer no route, so the question is answered by the documents' text alone
// (`docs/documents-viewer.md`, 「Asking to be taken somewhere」).
export default defineEventHandler(async (event): Promise<DocumentSeekResult> => {
  await requireUserSession(event);

  const body = await readBody<SeekBody>(event);
  const question = typeof body?.question === "string" ? body.question.trim() : "";

  if (question === "") {
    throw fail(400, wording("documents.error.seek_empty"));
  }

  if (question.length > DOCUMENT_QUESTION_LIMIT) {
    throw fail(400, wording("documents.error.question_too_long", { n: DOCUMENT_QUESTION_LIMIT }));
  }

  const roots = documentRoots();

  if (body?.root !== undefined || body?.path !== undefined) {
    const view = openDocument(body?.root, body?.path, roots);

    return { picks: seekInText(roots, question, { root: findDocumentRoot(view.root, roots)!, path: view.path }) };
  }

  return { picks: seekInText(roots, question, null) };
});
