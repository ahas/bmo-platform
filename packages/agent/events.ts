// The events agency's TypeScript side publishes, by the names the boundary gives them (docs/api.md,
// 「Events」): on the stream of the process that publishes them, where a listener in that process reads
// each as it comes and one that attaches late picks up from the id it last saw within the last stretch;
// and relayed to agency's server, which publishes each on the event streams a consumer reads. Nothing is
// stored, and publishing waits on neither a listener nor the server.

import { readFileSync } from "node:fs";
import { parseEnv } from "node:util";

import { agencyClient } from "@agency/client";
import { dataPath } from "@agency/support/roots";

/** A login's end told (docs/cache-fallback.md, 「A login that is about to end」). */
export interface AccountLoginNotice {
  account: string;
  // Which login it is: the one on this machine, or the account's token for the work nodes.
  login: "here" | "node-token";
  stands: "gone" | "ending" | "missing";
  // How long the login has left, in milliseconds; null where nothing says.
  leftMs: number | null;
  // The words agency's policy composes of it, with the one command the owner runs.
  notice: string;
}

/** Every event this side publishes, by its name. */
export interface AgencyEvents {
  "account-login-notice": AccountLoginNotice;
}

export type AgencyEventName = keyof AgencyEvents;

/** One event as a listener reads it: its name, its body, its place in the stream and when it came. */
export type AgencyEvent = { [Name in AgencyEventName]: { name: Name; body: AgencyEvents[Name]; id: number; at: number } }[AgencyEventName];

export type AgencyEventListener = (event: AgencyEvent) => void;

// Long enough that a listener reconnecting after a dropped stream still finds where it was.
const bufferSize = 500;

const buffer: AgencyEvent[] = [];
const listeners = new Set<AgencyEventListener>();
const relaying = new Set<Promise<void>>();
let nextId = 1;
let missedSaid = false;

// The port agency's server listens on where nothing names one.
const defaultControlPort = 3220;

// The token the file a `_FILE` variable names holds, or none where nothing is named or the file does not read.
function tokenFromFile(path: string | null): string | null {
  if (path === null) {
    return null;
  }

  try {
    const token = readFileSync(path, "utf8").trim();

    return token === "" ? null : token;
  } catch {
    return null;
  }
}

/**
 * Agency's server as this process reaches it: the port and the token its environment names, else those
 * the data root's `env/agency.env` names (docs/api.md, 「The control port」), the token by value or in the
 * file `AGENCY_CONTROL_TOKEN_FILE` names; none where neither names a token.
 */
export function controlServer(): { port: number; token: string } | null {
  let file: Record<string, string | undefined> = {};

  try {
    file = parseEnv(readFileSync(dataPath("env", "agency.env"), "utf8"));
  } catch {
    // A data root with no such file names nothing.
  }

  const named = (name: string) => {
    const value = (process.env[name] ?? "").trim() || (file[name] ?? "").trim();

    return value === "" ? null : value;
  };
  const token = named("AGENCY_CONTROL_TOKEN") ?? tokenFromFile(named("AGENCY_CONTROL_TOKEN_FILE"));
  const port = Number(named("AGENCY_CONTROL_PORT") ?? defaultControlPort);

  return token === null || !Number.isInteger(port) ? null : { port, token };
}

// One event posted to agency's server, which publishes it on its streams, waiting for the server's answer: its
// end is that answer, or the connection refused or broken. A word that did not land is said once on the
// error stream.
async function relayed(server: { port: number; token: string }, name: AgencyEventName, body: AgencyEvent["body"]): Promise<void> {
  try {
    const client = agencyClient(`http://127.0.0.1:${server.port}`, server.token);
    const { response } = await client.POST("/events", {
      body: { kind: name, data: body },
    });

    if (!response.ok) throw new Error(`the server answered ${response.status}`);
  } catch (error) {
    if (!missedSaid) {
      missedSaid = true;
      console.error(`agency's server was not told of the event ${name}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}

/** Every relay to agency's server begun so far, ended. */
export async function agencyEventsRelayed(): Promise<void> {
  await Promise.all([...relaying]);
}

/**
 * An event published to every listener, and relayed to agency's server where this process reaches one; a
 * listener that throws is logged and the rest are told all the same.
 */
export function emitAgencyEvent<Name extends AgencyEventName>(name: Name, body: AgencyEvents[Name]): AgencyEvent {
  const emitted = { name, body, id: nextId, at: Math.floor(Date.now() / 1000) } as AgencyEvent;

  nextId += 1;
  buffer.push(emitted);

  if (buffer.length > bufferSize) {
    buffer.splice(0, buffer.length - bufferSize);
  }

  for (const listener of listeners) {
    try {
      listener(emitted);
    } catch (error) {
      console.error("agency event listener failed:", error);
    }
  }

  const server = controlServer();

  if (server !== null) {
    const relay = relayed(server, name, body).finally(() => relaying.delete(relay));

    relaying.add(relay);
  }

  return emitted;
}

/** The events published after the id given, within the last stretch kept. */
export function agencyEventsSince(id: number): AgencyEvent[] {
  return buffer.filter((event) => event.id > id);
}

/** A listener told of every event from now on, until the function it is handed back is called. */
export function subscribeAgencyEvents(listener: AgencyEventListener): () => void {
  listeners.add(listener);

  return () => {
    listeners.delete(listener);
  };
}
