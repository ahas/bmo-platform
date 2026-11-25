import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import type { HarnessCommand } from "./harness";

// Whether an agent's pane holds its program or its host (docs/agent-control.md, 「Which agents are
// resident」). Two things decide it and neither is a name: whether the provider the agent is on now
// declares a program that can stay up, and whether the agent's line keeps a session for it to stay
// up on, as the line's own declaration says (`RunLineDeclaration.keepsSession`). The receiver, the
// conversation and the runners ask this and nothing else.

// The status command a settings file names now, or nothing where it names none or cannot be read.
function statusCommandOf(path: string): string {
  try {
    const command = (JSON.parse(readFileSync(path, "utf8")) as { statusLine?: { command?: unknown } } | null)?.statusLine?.command;

    return typeof command === "string" ? command : "";
  } catch {
    return "";
  }
}

// The identity of a command a program was started on: its words, where it stands, what it is handed,
// and the status command the settings file it is handed names now, in a fixed order. Two commands
// with one digest start the same program; any other difference is a different program, and since a
// program keeps the status line and hooks it started with, one whose settings came to call another
// command is another program. A value the command carries that is left unset is no part of it.
export function commandDigest(command: Pick<HarnessCommand, "args" | "cwd" | "env">): string {
  const env = Object.entries(command.env)
    .filter((entry): entry is [string, string] => entry[1] !== undefined)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  const settings = command.args.flatMap((arg, at) => (arg === "--settings" && at + 1 < command.args.length ? [statusCommandOf(resolve(command.cwd, command.args[at + 1]!))] : []));

  return createHash("sha256")
    .update(JSON.stringify({ args: command.args, cwd: command.cwd, env, settings }))
    .digest("hex")
    .slice(0, 32);
}
