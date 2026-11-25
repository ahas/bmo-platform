import health from "@agency/policy/chat-health.toml";
import { fillMarks } from "@agency/support/marks";
import { deploymentNames } from "@agency/support/names";

import type { Archive } from "@agency/archive/archive";

// The doctor: why a run of agency's own died, and what is said when one answers again.
//
// What the diagnosis may say is bounded by what was actually read where the run ended: how the
// process finished, what the run was last seen doing, and what the program said back. A cause
// nobody could read is said to be unknown, with where the whole account is, and never guessed at. The diagnosis is handed to the supervisor as an order to find and
// fix the cause by the caller that holds the room; the composed line here is what is sent only when
// the supervisor cannot answer that order.
//
// The same place remembers the failure, which is what the other half needs: the next run that
// answers is known to be a recovery, and where the owner heard of the failure from the bot's own
// line, the owner hears that the supervisor is back without having to ask.

/** How a run ended, as whoever watched it could read it. Never inferred past what was seen. */
export type RunEnding =
  // The process ended by itself and said it had not worked.
  | { kind: "exit"; code: number }
  // The process ran to its end and handed back an error in place of an answer.
  | { kind: "error-result" }
  // The run held together and produced no answer where one was owed.
  | { kind: "silent" }
  // The run was never had: the agent's pane or host could not be stood, or went away under it.
  | { kind: "transport" }
  // Nothing about the ending was readable. Everything that throws outside a run lands here.
  | { kind: "unknown" };

/** What the run was last seen doing, as its own log kept it. */
export interface LastAction {
  tool: string;
  detail: string | null;
}

/**
 * A run that ended with no answer, carrying what was readable where it ended. It is thrown where
 * a run is read rather than by the reader, so that every caller past that point has the facts
 * instead of a sentence it would have to take apart again.
 */
export class RunDied extends Error {
  readonly ending: RunEnding;
  readonly said: string | null;
  /** The run's own log name, which is where the rest of what it did is. */
  readonly run: string | null;

  constructor(what: { ending: RunEnding; said: string | null; message: string; run?: string | null }) {
    super(what.message);
    this.ending = what.ending;
    this.said = what.said;
    this.run = what.run ?? null;
  }
}

const detailLimit = 60;

// One line of what a run said back, with nothing of its shape left to spread over the message.
export function oneLine(text: string, limit: number): string | null {
  const flat = text.replace(/\s+/gu, " ").trim();

  if (flat === "") {
    return null;
  }

  return [...flat].length <= limit ? flat : `${[...flat].slice(0, limit).join("")}…`;
}

/** What a run was last doing, as the owner reads it in a dead run's account. */
export function actionLine(action: LastAction): string {
  const words = health.account;

  return fillMarks(words.action, { tool: action.tool, detail: action.detail === null ? "" : fillMarks(words.detail, { detail: action.detail }) });
}

/** What the run was last seen doing, read off its own log: the supervisor's own, or the worker's a job ran as. */
export async function lastActionOf(archive: Archive, run: string, agent: string = deploymentNames().agentKey): Promise<LastAction | null> {
  const line = await archive.runs().lastAgentTool(agent, run);

  if (line === null) {
    return null;
  }

  return { tool: line.name ?? health.account.unnamed_tool, detail: actionDetail(line.text) };
}

// What a tool call was given, short enough to read in the same breath as the tool's name. A
// command is what it ran and a file is what it touched; anything else is the tool's own business
// and the name of the tool already says as much as a person needs.
function actionDetail(text: string): string | null {
  let input: unknown;

  try {
    input = JSON.parse(text);
  } catch {
    return null;
  }

  const fields = input as { command?: unknown; file_path?: unknown; path?: unknown } | null;
  const value = [fields?.command, fields?.file_path, fields?.path].find((field) => typeof field === "string" && field !== "");

  return typeof value === "string" ? oneLine(value, detailLimit) : null;
}

