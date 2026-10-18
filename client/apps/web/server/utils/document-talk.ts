import { useWebSql } from "./db";

import type { DocumentTalkTurn, TalkSpeaker } from "#shared/types";

// The talk about each document (docs/documents-viewer.md, 「The record」): the console's own `document_talk`
// table, tied to the document's name. The receiver's talk takes the earlier turns from its caller, so the
// thread is kept here: the owner's words as soon as they are sent, with the passage, heading and version
// selected then, and the chat agent's answer when the talk answers. A second is the time a turn keeps.

/** Where the owner stood in a document as they said their words. */
export interface SaidOver {
  passage: string | null;
  heading: string | null;
  version: string | null;
  translated: boolean;
}

// A row as the database answers it, its whole numbers as the text of the number.
interface TalkRow extends Omit<DocumentTalkTurn, "id" | "speaker" | "saidAt" | "translated"> {
  id: number | string;
  speaker: string;
  saidAt: number | string;
  translated: number | string;
}

/** The talk one document carries, oldest first. */
export async function readDocumentTalk(document: string): Promise<DocumentTalkTurn[]> {
  const rows = await useWebSql().all(`SELECT id, speaker, text, said_at AS "saidAt", passage, heading, version, translated
       FROM document_talk WHERE document = $1 ORDER BY id`, document) as TalkRow[];

  return rows.map((row) => ({
    ...row,
    id: Number(row.id),
    speaker: row.speaker as TalkSpeaker,
    saidAt: Number(row.saidAt),
    translated: Number(row.translated) === 1
  }));
}

async function say(document: string, speaker: TalkSpeaker, text: string, saidAt: number, over: SaidOver | null): Promise<number> {
  const row = await useWebSql().get(`INSERT INTO document_talk (document, speaker, text, said_at, passage, heading, version, translated)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id`, document, speaker, text, Math.floor(saidAt / 1000),
  over?.passage ?? null, over?.heading ?? null, over?.version ?? null, over?.translated === true ? 1 : 0) as { id: number | string };

  return Number(row.id);
}

/** The owner's words written as they are sent, with where they stood in the document; the turn's id answers. */
export async function writeOwnerWords(document: string, text: string, over: SaidOver, now: number = Date.now()): Promise<number> {
  return await say(document, "owner", text, now, over);
}

/** The chat agent's answer written as the talk answers it; the turn's id answers. */
export async function writeAgentAnswer(document: string, text: string, now: number = Date.now()): Promise<number> {
  return await say(document, "bmo", text, now, null);
}
