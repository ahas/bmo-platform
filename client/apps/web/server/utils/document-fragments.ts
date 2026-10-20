import { realpathSync } from "node:fs";
import { isAbsolute, relative, resolve } from "node:path";

import { listRooms } from "./archive";
import { brain, brainCall } from "./brain";

import type { Fragment as BrainFragment } from "bmo-brain-client";
import type { DocumentFragmentView } from "#shared/types";
import type { DocumentRoot } from "./documents";

// The viewer's side of the reference fragments (`docs/documents-viewer.md`, 「Reference fragments」), read from the
// brain's rows (the brain's `docs/boundary-api.md`, 「Fragments」). A document is matched to its row by where its
// body sits: a row names its body as a path, and the document whose file that path names under a root the viewer
// reads is that fragment. Nothing here turns on a root's name or on a kind's name to make the match, and a row
// whose body is under no root the viewer reads matches nothing. A brain that does not answer leaves every document
// standing as a document.

// A fragment row as the brain answers it, as much of it as a screen is shown and a write names it by.
export interface FragmentStanding {
  kind: string;
  chatId: number | null;
  owner: number | null;
  agent: string | null;
  slug: string;
  title: string;
  keywords: unknown;
  path: string;
  active: boolean;
  lastHitAt: number | null;
  hits: number;
  handAt: number | null;
}

// A document as the join needs it: where it stands in the viewer, and the real file behind it.
export interface PlacedDocument {
  root: string;
  path: string;
  file: string;
}

// A row's keywords, as a list or as one text of words separated by commas.
function keywordsOf(value: unknown): string[] {
  if (Array.isArray(value)) return value.filter((word): word is string => typeof word === "string");
  if (typeof value === "string") return value.split(",").map((word) => word.trim()).filter((word) => word !== "");

  return [];
}

function realOf(path: string): string | null {
  try {
    return realpathSync(path);
  } catch {
    return null;
  }
}

// The file a row's path names under one of the roots, where it names one.
function fileUnder(roots: readonly DocumentRoot[], path: string): string | null {
  for (const root of roots) {
    const target = resolve(root.base, path);
    const away = relative(root.base, target);

    if (away === "" || away.startsWith("..") || isAbsolute(away)) continue;

    const real = realOf(target);

    if (real !== null) return real;
  }

  return null;
}

// A fragment as the brain's fixed API answers it, read into the row the viewer joins documents by. The room, the
// person and the agent the viewer shows are the labels its caller wrote; a fragment with none stands as a shared
// one (docs/boundary.md, 「Gaps the console meets」).
export function standingOf(fragment: BrainFragment): FragmentStanding {
  const labels = fragment.labels;
  const number = (key: string): number | null => {
    const value = Number(labels[key]);

    return labels[key] !== undefined && Number.isSafeInteger(value) ? value : null;
  };

  return {
    kind: fragment.kind,
    chatId: number("chatId"),
    owner: number("owner"),
    agent: labels.agent ?? null,
    slug: fragment.key,
    title: fragment.title ?? fragment.key,
    keywords: labels.keywords ?? "",
    path: fragment.path ?? "",
    active: fragment.state === "active",
    lastHitAt: fragment.lastHitAt === null ? null : Math.floor(fragment.lastHitAt / 1000),
    hits: fragment.hitCount,
    handAt: number("handAt") === null ? null : Math.floor(Number(labels.handAt) / 1000)
  };
}

// What each kind is called on a screen, as the brain names its kinds; a kind it does not name stands by itself.
async function kindLabels(): Promise<Map<string, string>> {
  const page = await brainCall(brain().GET("/fragment-kinds", { params: { query: { limit: 500 } } }));

  return new Map(page.items.map((one) => [one.name, one.name]));
}

// Every fragment the brain holds, a page at a time.
export async function readFragments(): Promise<BrainFragment[]> {
  const found: BrainFragment[] = [];
  let after: number | undefined;

  for (;;) {
    const page = await brainCall(brain().GET("/fragments", { params: { query: { limit: 500, ...(after === undefined ? {} : { after }) } } }));

    found.push(...page.items);

    if (!page.more || page.items.length === 0) return found;

    after = page.items[page.items.length - 1]!.id;
  }
}

// The names the rooms of the given fragments are known by.
async function roomNames(standings: readonly FragmentStanding[]): Promise<Map<number, string>> {
  if (!standings.some((one) => one.chatId !== null)) return new Map();

  return new Map((await listRooms().catch(() => [])).map((room) => [room.chatId, room.title]));
}

function viewOf(standing: FragmentStanding, kinds: Map<string, string>, rooms: Map<number, string>): DocumentFragmentView {
  return {
    kind: standing.kind,
    kindLabel: kinds.get(standing.kind) ?? standing.kind,
    chatId: standing.chatId,
    room: standing.chatId === null ? null : (rooms.get(standing.chatId) ?? String(standing.chatId)),
    owner: standing.owner,
    agent: standing.agent ?? null,
    slug: standing.slug,
    title: standing.title,
    keywords: keywordsOf(standing.keywords),
    active: standing.active,
    lastHitAt: standing.lastHitAt,
    hits: standing.hits,
    handAt: standing.handAt
  };
}

/** One row as a screen shows it, for a row a write has just handed back. */
export async function fragmentView(standing: FragmentStanding): Promise<DocumentFragmentView> {
  const [kinds, rooms] = await Promise.all([kindLabels().catch(() => new Map<string, string>()), roomNames([standing])]);

  return viewOf(standing, kinds, rooms);
}

/**
 * The fragment behind each of the given documents, by the name the viewer calls that document — its root and its
 * path. A document that is no fragment is not in the map at all, and none is while the brain does not answer.
 */
export async function documentFragments(roots: readonly DocumentRoot[], documents: readonly PlacedDocument[]): Promise<Map<string, DocumentFragmentView>> {
  const found = new Map<string, DocumentFragmentView>();

  if (documents.length === 0) return found;

  let standings: FragmentStanding[];
  let kinds: Map<string, string>;

  try {
    const [fragments, labels] = await Promise.all([readFragments(), kindLabels()]);

    standings = fragments.map(standingOf);
    kinds = labels;
  } catch {
    return found;
  }

  // One body is one fragment, so a file names at most one row; the first row of a file stands.
  const byFile = new Map<string, FragmentStanding>();

  for (const standing of standings) {
    const file = fileUnder(roots, standing.path);

    if (file !== null && !byFile.has(file)) byFile.set(file, standing);
  }

  const matched = documents.flatMap((document) => {
    const standing = byFile.get(document.file);

    return standing === undefined ? [] : [[`${document.root}/${document.path}`, standing] as const];
  });
  const rooms = await roomNames(matched.map(([, standing]) => standing));

  for (const [name, standing] of matched) {
    found.set(name, viewOf(standing, kinds, rooms));
  }

  return found;
}
