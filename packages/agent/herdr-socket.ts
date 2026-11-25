import { join } from "node:path";

import { homeDir } from "@agency/support/roots";

// Where herdr's socket is. `XDG_CONFIG_HOME` is the only thing that moves it — there is no flag for
// the socket path alone, and `HERDR_CONFIG_PATH` moves the configuration file while leaving the
// socket where it was, which is a trap worth naming since it looks like it should work.
//
// It stands apart from `packages/agent/herdr.ts` so a process that only needs the path loads none of the
// guards a call to herdr goes through.
export function herdrSocketPath(env: Record<string, string | undefined> = process.env): string {
  const configured = env.XDG_CONFIG_HOME;
  const root = configured === undefined || configured === "" ? join(homeDir(), ".config") : configured;

  return join(root, "herdr", "herdr.sock");
}
