// What a chat session's turn says about itself while it is still going, read from the CLI's
// stream as the bytes arrive: the phase it is in, the thinking tokens it has counted so far, the
// size of the request it has just sent with the share of it that came from the cache, and what the
// turn's requests so far have spent.
//
// The stream states each request's input, cache read and cache write on every block of that
// request's message, so a request is added once, when its message id is first seen. The output
// count the stream carries is a placeholder (it reads 4 while the answer runs to hundreds) and the
// true one lands only in the result frame, so it is never read; what the turn has written is
// estimated from the length of its answer and tool calls instead, and what it has thought is the
// CLI's own estimate.

import type { ContextState } from "./runlog";

// Where the turn is: turning it over, writing the answer, working a tool, or waiting for its engine
// while a Job holds it (docs/supervisors.md, 「The engine it shares」).
export type RunPhase = "thinking" | "writing" | "tool" | "waiting";

// The Job a turn waits on: its number, and the item it is a run of, where it is one.
export interface EngineHold {
  jobId: number;
  todoId: number | null;
}

// One thing the turn has done, as the room's own block shows it while the turn runs: what it was
// handed before it began, and each tool it has called since. Both are made of the very line the log
// writes — the same normalizer, the same masking — so the block and the record cannot come to say
// different things about one action.
export interface TurnAction {
  // `context` is something put in front of the turn, `tool` a call it made.
  kind: "context" | "tool";
  // The tool's name, or which sort of thing was handed over.
  name: string | null;
  // What the action is about: a call's arguments, or the piece that was handed over.
  text: string;
  // How the call ended, once its result has come back, and null while it is still out. On what was
  // handed over, whether a reference document went in full or as an already-loaded mark, and null
  // on anything else.
  state: "succeeded" | "failed" | ContextState | null;
  // The call a result answers on, which is how a result finds the action it belongs to. It is not
  // shown anywhere; it is what ties the two halves of one call together.
  ref: string | null;
}

export interface RunProgress {
  phase: RunPhase;
  // What the tool being used is called, for the phase that has one.
  tool: string | null;
  // The CLI's own running estimate of the thinking tokens spent, null before it says.
  thinkingTokens: number | null;
  // The request just sent: what it carried, and how much of that came out of the cache.
  contextTokens: number | null;
  cacheReadTokens: number | null;
  // The message the last request answered with, so its blocks add its spend once.
  requestId: string | null;
  // What the turn's requests have reported so far, added once each; null before the first.
  spent: RequestSpend | null;
  // An estimate of the tokens the turn has written — its answer and its tool calls — from their
  // length; null before anything was written.
  writtenTokens: number | null;
  // What the turn has done so far, oldest first and no more than the bound.
  actions: TurnAction[];
  // How many actions fell off the front of that list, and how many of those were things handed to
  // the turn rather than calls it made, so a reader keeping the two apart counts each whole.
  droppedActions: number;
  droppedContext: number;
  // The Job the turn waits on, in the waiting phase alone.
  waitingFor?: EngineHold | null;
}

export interface RequestSpend {
  inputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
}

