import type { AgentProviderName } from "./provider-names";

// What the archive keeps of the runs agents make: a job's spans, a run's spend, what an agent
// decided and the stage it decided at, and how an item's runs ended. The archive stores these rows,
// so their shape stands with it; the modules that write them are the agent and job bases'.

// Which level of a job a span belongs to. The phases are the runner's own clock over the whole
// job and they sum to it; the tools and the co-workers all happened inside one of those phases,
// so the kind is what keeps a reader from adding the two levels together.
export type JobSpanKind = "phase" | "tool" | "agent" | "usage";

// One span of a job's time: what it is, how long it took in seconds, how many times it happened
// — a tool called two hundred times is one row — and the tokens stated for it. Nothing here
// counts a token of its own; the figures come from whoever stated them, which is the stream for
// a Claude Workflow branch and the run's own answer for what the job spent.
//
// What the seconds of a tool answer is how long calls to it were outstanding, added up call by
// call. A run that sends three calls at once and is answered ten seconds later has each of them
// waiting ten seconds, so the three together read as thirty for ten seconds of the job. That is
// the honest answer to the question asked — how long was this tool busy — and it is why a tool
// is not a partition of anything: the phases are the partition, and whoever prints these says so.
export interface JobSpan {
  kind: JobSpanKind;
  name: string;
  seconds: number;
  times: number;
  tokens: number | null;
  // How that figure divided, where whoever stated it stated the division. A run's own spending
  // arrives from the CLI as four numbers and keeps all four; a Claude Workflow branch arrives as one
  // number and no more, so a branch carries none of these and none may be filled in for it —
  // splitting a figure nobody split would be inventing the answer to the question being asked.
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
  cacheWriteTokens: number | null;
}

// What one model cost in one run. The API reports no model in its windows, so this is the only
// ground there is for saying what a model has been spending.
//
// The four kinds are kept apart because they are not one thing. What was sent in, what was
// written out and what was written into the cache were burnt by this run; what came back out of
// the cache is the prefix a resumed session sends again on every turn, and counting it as
// spending makes the figure climb turn after turn while nothing new is being bought. The sum is
// kept beside them because it is what the whole of the traffic weighs, and because a row that
// carries no division of its own has nothing else to say.
export interface ModelSpend {
  // Which account/backend reported the traffic. Rows written before this common field existed
  // are read as Claude Code at the archive boundary.
  mode?: AgentProviderName;
  model: string;
  // The four below, added.
  tokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  cost: number;
}

// The stage a decision was made at, as the agent that made it names it: the chat run, a gate, a
// round asked again, the send. Which stages there are is the deployment's own.
export type JudgementStage = string;

export interface Judgement {
  chatId: number;
  // The message the decision answers or was made for, when one is known: the last message
  // the run read, or the message a reply is to.
  messageId: number | null;
  stage: JudgementStage;
  // The verdict in one line: PASS, FAIL: <reason>, SKIP, WAIT, REPLY, PLAIN, a list of slugs.
  verdict: string;
  // The whole of what the stage produced, for reading in full.
  detail: string | null;
  // How long the stage took, where somebody wrapped the call to measure it; empty where nobody
  // did.
  ms?: number | null;
  // The round of a room's session the decision was made in. Left out, it is the round the
  // recording code is running inside, and none outside every round.
  round?: string | null;
}

export interface JudgementRow extends Judgement {
  id: number;
  createdAt: number;
}

export interface RunCall {
  name: string | null;
  text: string;
  // What the call answered, and how it ended; null while nothing came back.
  result: string | null;
  state: string | null;
}

/** How one run of an item reads back: when it ended, in seconds, and how. */
export interface QueueRun {
  finishedAt: number | null;
  outcome: string | null;
}
