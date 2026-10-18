// What opened a room's turns, read by the receiver (the receiver's `docs/boundary-api.md`, 「What opened a turn」) and
// worn by the judgements of those turns. One asking covers every round the judgements name; a receiver that cannot
// be reached leaves them as they were.
import { receiverRead } from "./receiver";

import type { Judgement, RoundOpening } from "#shared/types";

// A page waits this long at most: the verdicts stand without the openings rather than late.
const OPENINGS_TIMEOUT_MS = 3_000;

export async function withOpenings(chatId: number, judgements: Judgement[]): Promise<Judgement[]> {
  const rounds = [...new Set(judgements.flatMap((judgement) => (judgement.round === null || judgement.round === undefined ? [] : [judgement.round])))];

  if (rounds.length === 0) {
    return judgements;
  }

  let read: Record<string, RoundOpening>;

  try {
    const asked = new URLSearchParams({ chat: String(chatId), rounds: rounds.join(",") });

    read = (await receiverRead<{ rounds: Record<string, RoundOpening> }>(`/rounds/openings?${asked}`, { timeoutMs: OPENINGS_TIMEOUT_MS })).rounds;
  } catch (error) {
    console.error(`openings of chat ${chatId} could not be read:`, error);

    return judgements;
  }

  return judgements.map((judgement) => {
    const opening = judgement.round === null || judgement.round === undefined ? undefined : read[judgement.round];

    return opening === undefined ? judgement : { ...judgement, opening };
  });
}

// One instruction's whole text, for the room whose turn it opened.
export async function readInstructionText(chatId: number, id: number): Promise<{ id: number; text: string }> {
  const asked = new URLSearchParams({ chat: String(chatId), id: String(id) });

  return await receiverRead<{ id: number; text: string }>(`/rounds/instruction?${asked}`);
}
