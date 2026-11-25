import { appendFileSync, renameSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// The machine's record of the calls herdr had not answered once a few seconds had passed: a line per
// call, written as a notice while the call goes on waiting, kept in the temporary directory every
// process on the machine shares, so the bot reads what a job's runner met as readily as what it met
// itself (docs/agent-control.md, 「Room on the machine」).
// BMO_HERDR_SILENCES names another file, which is how a test keeps its own.

// Past this size the record starts over beside its last stretch, which is kept one file back.
const recordCapBytes = 256 * 1024;

export function herdrSilencesPath(env: Record<string, string | undefined> = process.env): string {
  const named = env.BMO_HERDR_SILENCES;

  return named !== undefined && named !== "" ? named : join(tmpdir(), "bmo-herdr-silences.jsonl");
}

/** Writes one unanswered call down. A record that cannot be written costs nothing but itself. */
export function noteHerdrSilence(method: string, at: number = Date.now(), path: string = herdrSilencesPath()): void {
  try {
    if ((statSync(path, { throwIfNoEntry: false })?.size ?? 0) > recordCapBytes) renameSync(path, `${path}.old`);

    appendFileSync(path, `${JSON.stringify({ at, method })}\n`, { mode: 0o600 });
  } catch {
    // A full disk or a gone directory: the notice is lost and nothing else.
  }
}

