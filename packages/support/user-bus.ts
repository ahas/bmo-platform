// How any process here reaches the user's own service manager, whatever line it was started on.
//
// `systemctl --user` and `systemd-run --user` find the manager through the runtime directory, and
// a process started without one — a job's line hands none down and marks the session bus off, and
// a chat session's shell is handed neither — reaches nothing: systemd answers
// "Failed to connect to bus: No medium found" and nothing is started or asked. So every caller
// hands what it spawns the environment built here, and a launcher that must not open a row for a
// run that cannot start asks first whether the manager is there to be reached.

import { existsSync } from "node:fs";
import { join } from "node:path";
import { userInfo } from "node:os";

// The runtime directory is named where it is missing and the disabled bus address is taken off; a
// directory or an address already given is kept as it stands.
export function userBusEnvironment(
  env: Readonly<Record<string, string | undefined>>,
  uid: number,
): { set: Record<string, string>; unset: string[] } {
  const set: Record<string, string> = {};

  if (env.XDG_RUNTIME_DIR === undefined || env.XDG_RUNTIME_DIR === "") {
    set.XDG_RUNTIME_DIR = `/run/user/${uid}`;
  }

  return { set, unset: env.DBUS_SESSION_BUS_ADDRESS === "disabled:" ? ["DBUS_SESSION_BUS_ADDRESS"] : [] };
}

// A copy of an environment with the user bus reachable. It is handed over by name rather than
// written into this process's own, because a spawn that is handed nothing takes the environment the
// process started with, not what it became.
export function withUserBus(env: Readonly<Record<string, string | undefined>>, uid: number): Record<string, string> {
  const { set, unset } = userBusEnvironment(env, uid);
  const out: Record<string, string> = {};

  for (const [name, value] of Object.entries(env)) {
    if (value !== undefined && !unset.includes(name)) {
      out[name] = value;
    }
  }

  return { ...out, ...set };
}

// This process's own environment with the user bus reachable, which is what every spawn that asks
// systemd anything is handed.
export function userBus(): Record<string, string> {
  return withUserBus(process.env, userInfo().uid);
}

// Why the manager cannot be reached from an environment built above, or null where it can. The
// manager listens on a private socket under the runtime directory and the user bus beside it, and
// `systemd-run --user` connects to the first and falls back to the second; a directory holding
// neither is a session that is not there, and asking would only fail the same way.
export function userBusRefusal(env: Readonly<Record<string, string | undefined>>): string | null {
  const runtime = env.XDG_RUNTIME_DIR ?? "";

  if (runtime === "") {
    return "no runtime directory is named, so the user's service manager cannot be reached";
  }

  if (existsSync(join(runtime, "systemd", "private")) || existsSync(join(runtime, "bus"))) {
    return null;
  }

  return `the runtime directory ${runtime} holds no socket of the user's service manager`;
}

// Whether what systemd printed says it never reached the manager, as against refusing what it was
// asked to start.
export function busUnreachable(output: string): boolean {
  return /Failed to connect to .*bus/i.test(output);
}
