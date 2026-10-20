import { wording } from "#shared/wording";

import type { DocumentFragmentView } from "#shared/types";

interface StandingBody {
  root?: unknown;
  path?: unknown;
  active?: unknown;
}

// One reference fragment taken out of what its gate is offered, or offered again. It is the only write the viewer
// makes, and it writes the brain's row rather than a document (`docs/documents-viewer.md`). The fragment is named
// by the document it stands behind, so a request can only ever reach a row this viewer would have shown.
export default defineEventHandler(async (event): Promise<DocumentFragmentView> => {
  await requireUserSession(event);

  const body = await readBody<StandingBody>(event);

  if (typeof body?.active !== "boolean") {
    throw fail(400, wording("documents.error.fragment_direction"));
  }

  const roots = documentRoots();
  const named = requireDocument(body.root, body.path, roots);
  const file = resolveDocument(named.root, named.path);

  if (file === null) {
    throw fail(404, wording("documents.error.no_such_document"));
  }

  const standing = (await documentFragments(roots, [{ root: named.root.key, path: named.path, file }])).get(`${named.root.key}/${named.path}`);

  if (standing === undefined) {
    throw fail(404, wording("documents.error.not_fragment"));
  }

  // A row the brain cannot find is its refusal, said by its code.
  const written = await brainCall(
    brain().PATCH("/fragments/{fragmentKey}", {
      params: { path: { fragmentKey: standing.slug } },
      body: { state: body.active ? "active" : "off" }
    })
  );

  return await fragmentView(standingOf(written));
});
