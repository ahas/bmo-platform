import { unitBytes } from "./units";

import type { H3Event } from "h3";

// The roads of bmo-artifact's console plugin bundle, handed on whole (docs/boundary.md, 「Calls the console makes」
// and 「The public address」; bmo-artifact's docs/layout.md, 「The console plugin bundle」): every road under
// `/api/archive` behind the console's sign-in, and every road under `/api/share/` with none, the token being the
// whole of the permission. A road is handed on as it came — its method, its query, its body and the headers that
// say what it asks for — to the address `ARTIFACT_ADDRESS` names, and its answer comes back as bmo-artifact gave it,
// a document's file and pictures as the bytes they are. A refusal bmo-artifact answers with one of its codes is said
// by the pack's sentence for that code; the maker of a share link is the signed-in user, named here and never by the
// browser.

/** bmo-artifact's refusal codes on these roads (its docs/api.md, 「Documents (HTTP)」 and 「Share links (HTTP)」), each with the pack's key. */
export const ARTIFACT_CODES: Readonly<Record<string, string>> = {
  bad_id: "archive.error.bad_id",
  bad_attachment_id: "archive.error.bad_attachment_id",
  bad_cursor: "common.error.bad_cursor",
  bad_expiry: "common.error.bad_expiry",
  bad_maker: "archive.error.bad_maker",
  document_not_found: "archive.error.document_not_found",
  attachment_not_found: "archive.error.attachment_not_found",
  no_such_source: "archive.error.no_such_source",
  image_unavailable: "common.error.image_unavailable",
  share_not_found: "common.error.share_not_found",
  share_expired: "common.error.share_expired",
  already_shared: "archive.error.already_shared",
  nothing_to_share: "archive.error.nothing_to_share"
};

// The headers a road is asked with that bmo-artifact reads.
const ASKED_HEADERS = ["accept", "content-type", "range", "if-none-match", "if-modified-since"];

// The road a share link is made on, whose maker is the signed-in user.
const MAKING_A_LINK = /^\/api\/archive\/[^/]+\/share$/;

/** One road as the browser asked it. */
export interface ArtifactAsk {
  method: string;
  path: string;
  query: Record<string, unknown>;
  headers: Record<string, string>;
  body: string | undefined;
  // The signed-in user's name, who makes a share link; null on a road no session stands behind.
  by: string | null;
}

function search(query: Record<string, unknown>): string {
  const asked = new URLSearchParams();

  for (const [name, value] of Object.entries(query)) {
    for (const one of Array.isArray(value) ? value : [value]) {
      if (one !== undefined && one !== null) asked.append(name, String(one));
    }
  }

  const text = asked.toString();

  return text === "" ? "" : `?${text}`;
}

// The body a road is handed on with: as it came, and for a link being made, with its maker named.
function bodyOf(ask: ArtifactAsk): string | undefined {
  if (ask.method !== "POST" || !MAKING_A_LINK.test(ask.path) || ask.by === null) return ask.body;

  let fields: Record<string, unknown> = {};

  try {
    const read: unknown = ask.body === undefined || ask.body === "" ? {} : JSON.parse(ask.body);

    if (typeof read === "object" && read !== null && !Array.isArray(read)) fields = read as Record<string, unknown>;
  } catch {
    // A body that does not read is handed on as fields of its own, and bmo-artifact refuses what it lacks.
  }

  return JSON.stringify({ ...fields, by: ask.by });
}

/** One road handed on to bmo-artifact, answered as it answered, or refused in the pack's words for its code. */
export async function forwardToArtifact(ask: ArtifactAsk): Promise<Response> {
  const body = ask.method === "GET" || ask.method === "HEAD" ? undefined : bodyOf(ask);
  const headers: Record<string, string> = {};

  for (const name of ASKED_HEADERS) {
    if (ask.headers[name] !== undefined) headers[name] = ask.headers[name];
  }

  if (body !== undefined && MAKING_A_LINK.test(ask.path)) headers["content-type"] = "application/json";

  return await unitBytes("ARTIFACT", `${ask.path}${search(ask.query)}`, { method: ask.method, headers, body, codes: ARTIFACT_CODES });
}

/** The road a request names, as bmo-artifact serves it: the root it stands under and what follows. */
export function artifactPath(root: "/api/archive" | "/api/share", rest: string | undefined): string {
  const tail = (rest ?? "").split("/").filter((part) => part !== "").map(encodeURIComponent).join("/");

  return tail === "" ? root : `${root}/${tail}`;
}

/** What the browser asked, read off its request. */
export async function artifactAsk(event: H3Event, path: string, by: string | null): Promise<ArtifactAsk> {
  const method = getMethod(event).toUpperCase();
  const headers: Record<string, string> = {};

  for (const name of ASKED_HEADERS) {
    const value = getRequestHeader(event, name);

    if (value !== undefined) headers[name] = value;
  }

  return {
    method,
    path,
    query: getQuery(event),
    headers,
    body: method === "GET" || method === "HEAD" ? undefined : await readRawBody(event, "utf8"),
    by
  };
}
