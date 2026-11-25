import { wording } from "#shared/wording";

import type { DocumentView } from "#shared/types";

import type { DocumentRoot } from "./documents";

// A document a request names, looked up rather than trusted: what arrives is a root key and a path a browser
// sent, and what stands behind them is a declared root and a file inside its base. Any name that is not a
// document is the same 404, whatever made it one.
export function requireDocument(root: unknown, path: unknown, roots: readonly DocumentRoot[] = documentRoots()): { root: DocumentRoot; path: string } {
  const declared = findDocumentRoot(root, roots);

  if (declared === null || !isDocumentPath(path)) {
    throw fail(404, wording("documents.error.no_such_document"));
  }

  return { root: declared, path };
}

// A named document read whole, with the file it was read from.
export function openDocument(root: unknown, path: unknown, roots: readonly DocumentRoot[] = documentRoots()): DocumentView & { file: string } {
  const named = requireDocument(root, path, roots);
  let read: (DocumentView & { file: string }) | null;

  try {
    read = readDocument(named.root, named.path);
  } catch (error) {
    if (error instanceof DocumentTooLarge) {
      throw fail(413, wording("documents.error.too_large"));
    }

    throw error;
  }

  if (read === null) {
    throw fail(404, wording("documents.error.no_such_document"));
  }

  return read;
}

// The name a document's talk is kept under: its root and its path.
export function documentName(root: string, path: string): string {
  return `${root}/${path}`;
}
