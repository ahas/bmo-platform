// The PATH every run apart is handed, whoever launched it (docs/agent-control.md).
//
// Agency's server names its PATH in its unit, and everything the server launches inherits it. A
// launcher run from anywhere else — a chat session's shell, a scheduled unit, a job's line — carries
// whatever PATH that place had, and a narrow one would reach the runner it starts, the head that
// runner stands up and the program in that head, which then cannot find the CLI at all. So the run
// is handed the launcher's own PATH with every directory of the service's that it lacks added after.
//
// The unit file is the one place those directories are written; nothing here keeps a second list.

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { codeRoot, homeDir } from "./roots";

/** The systemd unit agency's server runs as, kept under the code root's `deploy/`. */
export const SERVER_UNIT = "bmo-agency";

// The directories agency's server runs on, as its unit names them: `%h` is the home, and a unit naming no
// PATH names nothing. Only the one `Environment=PATH=` assignment is read, line by line, since no reader
// of systemd's unit syntax stands among the installed packages and nothing else of the unit is needed.
export function servicePath(unitText: string, home: string): string[] {
  for (const line of unitText.split("\n")) {
    const found = /^Environment="?PATH=([^"]*)"?\s*$/.exec(line.trim());

    if (found?.[1] !== undefined) {
      return found[1].replaceAll("%h", home).split(":").filter((dir) => dir !== "");
    }
  }

  return [];
}

// The server's unit in the code root's `deploy/`, read once. A tree without one hands on the launcher's
// PATH alone.
let serviceDirs: string[] | null = null;

function ownServicePath(): string[] {
  if (serviceDirs === null) {
    try {
      serviceDirs = servicePath(readFileSync(join(codeRoot, "deploy", `${SERVER_UNIT}.service`), "utf8"), homeDir());
    } catch {
      serviceDirs = [];
    }
  }

  return serviceDirs;
}

// The launcher's own directories stay first, so a stand-in it put ahead of the real one is still what
// the run finds.
export function runPath(launcher: string, service: readonly string[] = ownServicePath()): string {
  const dirs = launcher.split(":").filter((dir) => dir !== "");

  return [...dirs, ...service.filter((dir) => !dirs.includes(dir))].join(":");
}
