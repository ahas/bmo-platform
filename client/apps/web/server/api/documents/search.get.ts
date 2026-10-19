import { DOCUMENT_SEARCH_LIMIT } from "#shared/config";
import { wording } from "#shared/wording";

import type { DocumentMatch } from "#shared/types";

// The documents whose name, title or body holds the words, among the documents the list shows.
export default defineEventHandler(async (event): Promise<DocumentMatch[]> => {
  await requireUserSession(event);

  const asked = getQuery(event);
  const query = typeof asked.q === "string" ? asked.q.trim() : "";

  if (query === "") {
    return [];
  }

  if (query.length > 200) {
    throw fail(400, wording("documents.error.search_too_long"));
  }

  // A fragment stands in the list by the line its index entry carries, so the search looks in that line too and
  // names what it found the way the list does.
  const roots = documentRoots();
  const shown = new Map([...await documentFragments(roots, listDocuments(roots))].map(([name, one]) => [name, one.title]));

  return searchDocuments(roots, query, DOCUMENT_SEARCH_LIMIT, shown);
});
