import type { DocumentView } from "#shared/types";

// One document, whole, with its version and the language it is written in.
export default defineEventHandler(async (event): Promise<DocumentView> => {
  await requireUserSession(event);

  const asked = getQuery(event);
  const { file: _file, ...view } = openDocument(asked.root, asked.path);

  return view;
});
