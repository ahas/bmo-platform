import { readFileSync } from "node:fs";
import { join } from "node:path";

import { cliProjectsRoot, transcriptPath } from "./cli-transcripts";

// The program an agent's place holds for its resident pane, as the launcher writes it and the head
// reads it (`packages/agent/resident-pane.ts`, docs/agent-control.md, 「One standing pane per agent」). It stands
// apart so a reader that only needs the record loads nothing of the pane machinery to read it.

export interface ResidentCommand {
  args: string[];
  cwd: string;
  env: Record<string, string>;
}

export interface ResidentProgram {
  identity: string;
  command: ResidentCommand;
  // The same program started again: a command that opened a session under a name it chose opens it
  // again by resuming it, since the name is taken by then. Null where the first command does both.
  restart: ResidentCommand | null;
  // The record whose existence says the session the first command opens is open. A program that opens
  // a session under a name it chose writes nothing of it until its first prompt, and resuming a name
  // with no record behind it fails, so until this exists every start takes the first command. Absent
  // where the restart can always be taken.
  opened?: string | null;
}

export const programFile = (place: string) => join(place, "program.json");

export function readProgram(place: string): ResidentProgram | null {
  try {
    return JSON.parse(readFileSync(programFile(place), "utf8")) as ResidentProgram;
  } catch {
    return null;
  }
}

// The session record the program writes into: the session its command names, in the CLI's projects
// directory under the configuration directory its environment names. A program whose command names
// no session keeps a record nobody can find from here, and has none.
export function programTranscript(program: ResidentProgram): string | null {
  const command = program.restart ?? program.command;
  const at = command.args.findIndex((arg) => arg === "--resume" || arg === "--session-id");
  const session = at === -1 ? undefined : command.args[at + 1];

  if (session === undefined || session === "") return null;

  const configDir = command.env.CLAUDE_CONFIG_DIR;

  return transcriptPath(command.cwd, session, configDir === undefined || configDir === "" ? undefined : cliProjectsRoot(configDir));
}
