import { afterAll, describe, expect, test } from "bun:test";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import "../../support/nitro";
import { answer, standIn } from "../../support/units";
import { scratchDir } from "../../support/scratch";

// The join between the documents the viewer lists and the rows a stood-in brain holds. What it is held to is that
// a document is matched by where its body sits under a root the viewer reads and by nothing else — not by its
// root's name and not by its kind's — that a document no row names carries nothing at all, that a kind is called
// by the name the brain gives it, and that a brain that does not answer leaves every document a document.

const scratch = scratchDir("bmo-web-document-fragments-");
const checkout = join(scratch, "checkout");
const ROOM = -100;

function put(file: string, text: string): void {
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, text);
}

put(join(checkout, "STRUCTURE.md"), "# Structure\n");
put(join(checkout, "docs", "lore", "chunks", "candy.md"), "# Candy\n");
put(join(checkout, "docs", "web.md"), "# Web console\n");
put(join(checkout, "docs", "rules", "answer-lines.md"), "# Answer length\n");

afterAll(() => rmSync(scratch, { recursive: true, force: true }));

function fragment(id: number, over: Record<string, unknown>): Record<string, unknown> {
  return {
    id,
    key: `fragment-${id}`,
    kind: "room-rule",
    scope: null,
    subjectKey: null,
    title: null,
    path: null,
    text: "",
    state: "active",
    switchedOff: [],
    labels: { agent: "agent-a" },
    hitCount: 0,
    lastHitAt: null,
    createdAt: 1_700_000_000_000,
    changedAt: 1_700_000_000_000,
    version: "v1",
    libraryName: null,
    ...over
  };
}

let brainUp = true;

const brain = standIn("BRAIN", (one) => {
  if (!brainUp) return answer({ code: "not-ready", error: "the database does not answer" }, 409);

  const path = one.path.split("?")[0]!;

  if (path === "/fragment-kinds") {
    return answer({ items: [{ name: "room-rule", description: "", fragmentCount: 2 }, { name: "system-document", description: "", fragmentCount: 1 }], more: false });
  }

  if (path === "/fragments") {
    return answer({
      items: [
        fragment(1, { kind: "system-document", key: "structure", title: "the system's shape", path: "STRUCTURE.md" }),
        fragment(2, { kind: "lore", key: "candy", title: "Candy", path: "docs/lore/chunks/candy.md", state: "off", labels: { agent: "agent-a", keywords: "sweets", handAt: "1700000100000" }, hitCount: 3, lastHitAt: 1_700_000_000_000 }),
        fragment(3, { kind: "room-rule", key: "answer-lines", title: "how many lines an answer runs to", path: "docs/rules/answer-lines.md", labels: { agent: "agent-a", chatId: String(ROOM) } }),
        fragment(4, { kind: "room-rule", key: "elsewhere", title: "a row whose body is under no root", path: "../outside/elsewhere.md" })
      ],
      more: false
    });
  }

  return answer({ code: "no-route", error: "no route" }, 404);
});

standIn("RECEIVER", (one) => {
  const path = one.path.split("?")[0]!;

  if (path === "/rooms") {
    return answer({ items: [{ id: ROOM, title: "Room A", messageCount: 1, lastMessageAt: 1, pinned: false, senders: [], companions: [], ownerRoom: false, registered: true, workRoom: 0 }], more: false });
  }

  if (path === "/owner") return answer({ room: 5, senderId: 5 });

  return answer({ code: "not-found", error: "no route" }, 404);
});

const { checkoutRoot } = await import("../../../apps/web/server/utils/documents");
const { documentFragments, fragmentView } = await import("../../../apps/web/server/utils/document-fragments");

const roots = [checkoutRoot("first", "First", checkout)];
const listed = [
  { root: "first", path: "STRUCTURE.md", file: join(checkout, "STRUCTURE.md") },
  { root: "first", path: "docs/web.md", file: join(checkout, "docs", "web.md") },
  { root: "first", path: "docs/lore/chunks/candy.md", file: join(checkout, "docs", "lore", "chunks", "candy.md") },
  { root: "first", path: "docs/rules/answer-lines.md", file: join(checkout, "docs", "rules", "answer-lines.md") }
];

describe("the fragment behind a document", () => {
  test("a document whose body a row names finds its row, its kind called by the brain's name", async () => {
    const found = await documentFragments(roots, listed);

    expect(found.get("first/STRUCTURE.md")).toMatchObject({ kind: "system-document", kindLabel: "system-document", slug: "structure", room: null, agent: "agent-a" });
    expect(found.get("first/docs/rules/answer-lines.md")).toMatchObject({ kind: "room-rule", kindLabel: "room-rule", chatId: ROOM, room: "Room A", title: "how many lines an answer runs to" });
    expect(found.get("first/docs/lore/chunks/candy.md")?.kindLabel).toBe("lore");
    expect(brain.asked.some((one) => one.path === "/fragments?limit=500")).toBe(true);
  });

  test("a document no row names carries nothing, and a row under no root matches nothing", async () => {
    const found = await documentFragments(roots, listed);

    expect(found.has("first/docs/web.md")).toBe(false);
    expect(found.size).toBe(3);
  });

  test("what the row says is what stands: the standing, the keywords, the counts and the hand", async () => {
    expect((await documentFragments(roots, listed)).get("first/docs/lore/chunks/candy.md")).toMatchObject({
      active: false,
      keywords: ["sweets"],
      hits: 3,
      lastHitAt: 1_700_000_000,
      handAt: 1_700_000_100
    });
  });

  test("a row handed back from a write reads the same way as one read from the list", async () => {
    const written = {
      kind: "room-rule",
      chatId: ROOM,
      owner: null,
      agent: "agent-a",
      slug: "answer-lines",
      title: "how many lines an answer runs to",
      keywords: "",
      path: "docs/rules/answer-lines.md",
      active: true,
      lastHitAt: null,
      hits: 0,
      handAt: null
    };

    expect(await fragmentView(written)).toEqual((await documentFragments(roots, listed)).get("first/docs/rules/answer-lines.md")!);
  });

  test("a brain that does not answer leaves every document a document", async () => {
    brainUp = false;

    try {
      expect((await documentFragments(roots, listed)).size).toBe(0);
    } finally {
      brainUp = true;
    }
  });
});
