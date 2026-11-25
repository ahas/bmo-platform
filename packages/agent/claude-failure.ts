import { harnessFailure } from "./harness";

import type { AgentAdapter, HarnessOutput } from "./harness";

// What a Claude run's failure is. Two of these are the login's own — its window is full, its
// login is gone — one is the service's, and everything else is the work itself having failed.
// They live together because telling them apart is one judgement made in one place: a caller that
// read the difference for itself would read it differently from the next caller, and a login at
// its ceiling would be retried by one line and given up on by another.

/** The login's window is full: it answers nothing until that window resets. */
export class ClaudeLimitError extends Error {
  readonly resetsAt: number | null;

  constructor(resetsAt: number | null) {
    super("claude usage limit reached");
    this.resetsAt = resetsAt;
  }
}

// The API itself would not answer: the CLI retried its call and gave up. Not a limit and not a
// fault of the run — the service is down or overloaded, and the caller waits it out.
export class ClaudeOutageError extends Error {
  readonly status: number | null;
  readonly attempts: number;

  constructor(status: number | null, attempts: number) {
    super(`claude api unavailable after ${attempts} retr${attempts === 1 ? "y" : "ies"}${status === null ? "" : ` (last status ${status})`}`);
    this.status = status;
    this.attempts = attempts;
  }
}

/**
 * What a finished run said about the login it went out on rather than about the work.
 *
 * - `limit` — that login's window is full. Another login answers; this one rests until its own
 *   window resets.
 * - `login` — that login is gone: nobody runs on it until a person signs in again. Another login
 *   answers.
 * - `outage` — the service itself would not answer. No login would answer either, so the run is
 *   not carried to another: the caller waits it out as it always did.
 */
export type AccountFailure = "limit" | "login" | "outage";

/**
 * The one place a login being gone is recognised by what was said: the shape of the message, with
 * the CLI's own words for a long-lived token expired or revoked; the run's ending and the
 * credential's own expiry are read beside it rather than instead of it, and correcting the wording is
 * correcting this one line and its twin in the Rust runner.
 */
export const loginGoneWording = /(?:please )?(?:run )?\/?login\b|not (?:logged in|authenticated)|invalid api key|oauth token (?:has )?expired|oauth (?:access )?token (?:has been )?revoked|invalid bearer token/i;

/** A failure of the login itself, thrown where a run is read rather than by the reader. */
export class ClaudeAccountFailed extends Error {
  constructor(readonly failure: AccountFailure, detail: string) {
    super(detail);
  }
}

/**
 * Which of them a failure is. It takes either what was thrown or what the process left behind,
 * because the two roads a run can fail on both end here: a reader that threw, and an exit code
 * with the harness's own reading of it. Nothing outside this reads a failure apart.
 */
export function accountFailure(from: { error?: unknown; harness?: AgentAdapter; output?: HarnessOutput }): AccountFailure | null {
  const { error, harness, output } = from;

  if (error instanceof ClaudeAccountFailed) {
    return error.failure;
  }

  if (error instanceof ClaudeLimitError) {
    return "limit";
  }

  if (error instanceof ClaudeOutageError) {
    return "outage";
  }

  if (output === undefined || output.exitCode === 0) {
    return null;
  }

  // What the program said about a run that did not answer, read through its own harness and
  // never taken apart here.
  const said = harness === undefined ? `${output.stderr}\n${output.stdout}` : harnessFailure(harness, output);

  return loginGoneWording.test(said) ? "login" : null;
}
