import { existsSync } from "node:fs";

// A path a line hands the CLI besides its own words: a configuration of servers, a settings file,
// a directory of skills. A path that is not on the disk ends the launch in under a second: the CLI
// names the file on its error stream and exits 1 before it has read a word of the prompt. A job is
// started in a pane and read back out of its own record, and that record keeps only that the run
// answered nothing — so the one sentence saying why is written here, where it will be read.
//
// So the absence is caught where the command is built, that being the last place anybody can still
// be told, and it is caught by the path rather than by the flag that carries it: which flags there
// are is one file's business and stays there.

/**
 * Why a launch handed this path must not be made, or nothing. It is said in the words of whoever
 * reads a job's record or a unit's log, because that is where the question is asked and the CLI's
 * own silence is the whole of what would otherwise be there to answer it.
 */
export function handedPathFault(line: string, path: string): string | null {
  if (existsSync(path)) {
    return null;
  }

  return [
    `${path}, which the ${line} line hands the CLI, is missing.`,
    "Without it the CLI ends with code 1 before it reads a word of its prompt, and the run's record says only that it gave no answer.",
    "This happens where the deploy tree is behind the checkout: deploy, then run it again.",
  ].join(" ");
}

/**
 * The path as the command carries it, and never one that is not there. A line with nothing to hand
 * over says so with an empty value, which is no path at all and is passed straight through.
 */
export function handedPath(line: string, path: string | null): string {
  if (path === null || path === "") {
    return "";
  }

  const fault = handedPathFault(line, path);

  if (fault !== null) {
    throw new Error(fault);
  }

  return path;
}
