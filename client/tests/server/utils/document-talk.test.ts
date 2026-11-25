import { expect, test } from "bun:test";

import "../../support/nitro";
import { scratchWebDatabase } from "../../support/web-database";

// The talk about a document as the console keeps it in its own table: the owner's words with where they stood,
// the chat agent's answer bare, each document's talk read back alone and in the order it was said, a turn's time
// kept to the second.

await scratchWebDatabase();

const { readDocumentTalk, writeAgentAnswer, writeOwnerWords } = await import("../../../apps/web/server/utils/document-talk");

test("a document's talk reads back alone, oldest first, with where the owner stood", async () => {
  await writeOwnerWords("notes/a.md", "What does this part mean?", { passage: "the passage", heading: "A heading", version: "abc123", translated: true }, 100_900);
  await writeOwnerWords("notes/b.md", "Another document", { passage: null, heading: null, version: null, translated: false }, 101_000);
  await writeAgentAnswer("notes/a.md", "It means this.", 102_000);

  const turns = await readDocumentTalk("notes/a.md");

  expect(turns.map(({ id: _id, ...turn }) => turn)).toEqual([
    { speaker: "owner", text: "What does this part mean?", saidAt: 100, passage: "the passage", heading: "A heading", version: "abc123", translated: true },
    { speaker: "bmo", text: "It means this.", saidAt: 102, passage: null, heading: null, version: null, translated: false }
  ]);
  expect(turns[0]!.id).toBeLessThan(turns[1]!.id);
  expect(await readDocumentTalk("notes/none.md")).toEqual([]);
});
