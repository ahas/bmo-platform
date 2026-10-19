import { afterAll, describe, expect, test } from "bun:test";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import "../../support/nitro";
import { answer, standIn } from "../../support/units";
import { scratchDir } from "../../support/scratch";

// The join between the documents the viewer lists and the rows a stood-in brain holds. What it is held to is that
// a document is matched by where its body sits under a root the viewer reads and by nothing else — not by its
// root's name and not by its kind's — that a document no row names carries nothing at all, that a kind is called
// by the label the brain gives it, and that a brain that does not answer leaves every document a document.

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

const row = (fields: Record<string, unknown>) => ({
  chatId: null,
  owner: null,
  agent: "agent-a",
  allAgents: false,
  keywords: [],
  active: true,
  lastHitAt: null,
  hits: 0,
  setBy: null,
  setAt: 1,
  hand: null,
  handAt: null,
  ...fields
});

let brainUp = true;

const brain = standIn("BRAIN", (one) => {
  if (!brainUp) return answer({ code: "not-ready", error: "the database does not answer" }, 409);

  const path = one.path.split("?")[0]!;

  if (path === "/fragments/kinds") {
    return answer({ kinds: [{ kind: "room-rule", label: "Room rule" }, { kind: "system-document", label: "System document" }] });
  }

  if (path === "/fragments") {
    return answer({
      fragments: [
        row({ kind: "system-document", slug: "structure", title: "the system's shape", path: "STRUCTURE.md" }),
        row({ kind: "lore", slug: "candy", title: "Candy", path: "docs/lore/chunks/candy.md", keywords: ["sweets"], active: false, hits: 3, lastHitAt: 1_700_000_000, handAt: 1_700_000_100 }),
        row({ kind: "room-rule", chatId: ROOM, slug: "answer-lines", title: "how many lines an answer runs to", path: "docs/rules/answer-lines.md" }),
        row({ kind: "room-rule", slug: "elsewhere", title: "a row whose body is under no root", path: "../outside/elsewhere.md" })
      ]
    });
  }

  return answer({ code: "no-route", error: "no route" }, 404);
});

standIn("RECEIVER", (one) => {
  const path = one.path.split("?")[0]!;

  if (path === "/console/rooms") return answer({ rooms: [{ chatId: ROOM, title: "Room A", messageCount: 1, lastMessageAt: 1, pinned: false }] });
  if (path === "/owner") return answer({ chat: 5 });

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
  test("a document whose body a row names finds its row, its kind called by the brain's label", async () => {
    const found = await documentFragments(roots, listed);

    expect(found.get("first/STRUCTURE.md")).toMatchObject({ kind: "system-document", kindLabel: "System document", slug: "structure", room: null, agent: "agent-a" });
    expect(found.get("first/docs/rules/answer-lines.md")).toMatchObject({ kind: "room-rule", kindLabel: "Room rule", chatId: ROOM, room: "Room A", title: "how many lines an answer runs to" });
    // A kind the brain gives no label stands by its own name.
    expect(found.get("first/docs/lore/chunks/candy.md")?.kindLabel).toBe("lore");
    expect(brain.asked.some((one) => one.path === "/fragments?down=true")).toBe(true);
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
    const written = row({ kind: "room-rule", chatId: ROOM, slug: "answer-lines", title: "how many lines an answer runs to", path: "docs/rules/answer-lines.md" });

    expect(await fragmentView(written as never)).toEqual((await documentFragments(roots, listed)).get("first/docs/rules/answer-lines.md")!);
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
