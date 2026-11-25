// The one place that speaks to herdr, the way `units.ts` is the one place that speaks to systemd.
//
// herdr is a JSON-RPC server on a Unix socket: one request is one line, one response is one line.
// Everything below is written against what this machine actually answered rather than against the
// documentation, because two of the things that matter most here are not documented — that a
// connection carries exactly one request, and that the order of attaching decides whether events
// are lost.
//
// What a run is called is a token on its pane. That is the whole of the mapping: the launcher
// reports the run's own name on the pane it opened, liveness reads the tokens back, and the run
// itself is told its name in its environment because a pane has no cgroup to read one out of.
// Labels are left to what a person reads — the sidebar shows a workspace's, a pane's border shows
// its own — because a name a person can read is not a name systemd will take, and because a herdr
// that comes back rebuilds its labels but not its tokens, which is exactly the difference between
// a place and the run that was in it.

import { existsSync } from "node:fs";

import { noteHerdrSilence } from "./herdr-silences";
import { herdrSocketPath } from "./herdr-socket";
import { runNameOf } from "./agent-place";
import { guardHerdrCall, recordRefusal, socketRefusal } from "./live-claude-guard";

// The one error code that is not a failure. A blocked agent is one waiting on a person — the
// trust confirmation a fresh checkout puts up, most often — and the measured behaviour is that
// the call is refused with the pane left byte-identical, so nothing was delivered and nothing
// was lost. Pushing again returns the same refusal, so a caller that treats this as a failure
// to retry spins for nothing.
export const AGENT_BLOCKED = "agent_blocked";

export interface HerdrError {
  code: string;
  message: string;
}

// What one call answers. The error is carried rather than thrown because the interesting case —
// `agent_blocked` — is a state of the world and not a fault of the caller's.
export type HerdrAnswer<T> = { ok: true; result: T } | { ok: false; error: HerdrError };

export function isBlocked<T>(answer: HerdrAnswer<T>): boolean {
  return !answer.ok && answer.error.code === AGENT_BLOCKED;
}

export { herdrSocketPath };

// The pauses before a request that never reached herdr is sent again, about two seconds in all.
const resendPausesMs = [250, 600, 1_200];

// The longest request line herdr reads, its newline aside (measured on herdr 0.9.3): a longer one has its
// connection dropped with nothing answered, so it is refused here as too large without being sent.
export const herdrLineCapBytes = 1_048_576;
export const TOO_LARGE = "too_large";

// What a call answers where the connection failed or closed; only these may be a request that never
// went out.
const connectionFailures = new Set(["unreachable", "closed"]);

// How long a call goes unanswered before it is written down once as herdr falling silent. The notice
// paces nothing and ends nothing: the call goes on waiting for its answer or its connection's end.
const silenceNoticeMs = 5_000;

export interface HerdrCallOptions {
  socket?: string;
  /** What waits between one sending and the next; the clock's own wait unless a test names another. */
  pause?: (ms: number) => Promise<unknown>;
}

// One request, one connection, waited on until herdr answers it or the connection is refused, breaks or
// closes. A second request written to the same connection is refused with a broken pipe, so nothing
// here is pooled or reused; the exception is the subscription below, which is the one connection the
// server keeps open to write on.
//
// A request that never reached herdr is sent again, a few times over about two seconds
// (docs/agent-control.md, 「herdr」). herdr drops a connection whose line does not come within a few
// seconds, so this thread held that long by a statement on a busy disk finds the connection gone
// with herdr well. A request written whole is never sent again, since herdr may have acted on it,
// and nor is one to a socket that is not there at all, since no herdr is there to take it.
export async function herdrCall<T = unknown>(
  method: string,
  params: unknown,
  options: HerdrCallOptions = {},
): Promise<HerdrAnswer<T>> {
  // A job does not start the CLI in a pane or put words into one before it has said what for, whoever
  // calls; this throws before anything is sent (docs/live-claude-guard.md).
  await guardHerdrCall(method, params);

  const socket = options.socket ?? herdrSocketPath();
  const id = `bmo-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  // A test reaches only a herdr of its own; the machine's is refused before a connection is made,
  // and the refusal is written where the test's own preload reads it.
  const refused = socketRefusal(socket);

  if (refused !== null) {
    recordRefusal(refused);

    return { ok: false, error: { code: "refused", message: refused } };
  }

  const lineBytes = Buffer.byteLength(JSON.stringify({ id, method, params }));

  if (lineBytes > herdrLineCapBytes) {
    return { ok: false, error: { code: TOO_LARGE, message: `the ${method} request is ${lineBytes} bytes, past herdr's ${herdrLineCapBytes}-byte line` } };
  }

  const pause = options.pause ?? Bun.sleep;

  for (let resent = 0; ; resent += 1) {
    const sent = await sendOnce<T>(method, params, id, socket);

    if (sent.answer.ok || sent.delivered || !connectionFailures.has(sent.answer.error.code) || resent >= resendPausesMs.length || !existsSync(socket)) {
      return sent.answer;
    }

    await pause(resendPausesMs[resent] as number);
  }
}

// One connection carrying the request, and whether its line was written whole before the call
// ended.
function sendOnce<T>(method: string, params: unknown, id: string, socket: string): Promise<{ answer: HerdrAnswer<T>; delivered: boolean }> {
  return new Promise((resolve) => {
    let held = "";
    let delivered = false;
    // A long answer — a screen read back — arrives in pieces that may part a character.
    const reading = new TextDecoder();
    let settled = false;
    // What of the request line the socket has not taken yet. A Bun socket writes what the kernel takes
    // at once and keeps none of the rest, so a line longer than the socket's buffer — a long prompt,
    // measured at about 214KB — goes in pieces, each after the socket has drained. herdr waits for the
    // line's end and gives up on a request left short.
    let unsent = new Uint8Array(0);
    const flush = (connection: { write(data: Uint8Array): number }): void => {
      while (unsent.length > 0) {
        const wrote = connection.write(unsent);

        if (wrote <= 0) return;
        unsent = unsent.subarray(wrote);
      }

      delivered = true;
    };
    const answer = (value: HerdrAnswer<T>): void => {
      if (settled) {
        return;
      }

      settled = true;
      clearTimeout(notice);
      resolve({ answer: value, delivered });
    };
    // A herdr that is not there is a fact for the caller to act on rather than an exception to climb
    // out of: the launcher falls back to a unit on it. One slow to answer is written down for the
    // machine once the notice's span has passed, since herdr falling silent is what a machine short of
    // room shows first — this machine's herdr alone, since a work node's falling silent is its link's,
    // which says nothing of this machine's room (docs/work-nodes.md) — and the call goes on waiting.
    const notice = setTimeout(() => {
      if (socket === herdrSocketPath()) noteHerdrSilence(method);
    }, silenceNoticeMs);

    Bun.connect({
      unix: socket,
      socket: {
        async open(connection) {
          try {
            await guardHerdrCall(method, params);
            unsent = Buffer.from(`${JSON.stringify({ id, method, params })}\n`);
            flush(connection);
          } catch (error) {
            answer({ ok: false, error: { code: "refused", message: String(error) } });
            connection.end();
          }
        },
        drain(connection) {
          flush(connection);
        },
        data(connection, chunk) {
          held += reading.decode(chunk, { stream: true });

          const end = held.indexOf("\n");

          if (end < 0) {
            return;
          }

          // Answered before the connection is ended, because ending it runs the close handler
          // there and then: end first and the close below settles the call as "closed" while the
          // line that actually came back is thrown away by the guard.
          answer(readFrame<T>(held.slice(0, end)));
          connection.end();
        },
        error(_connection, error) {
          answer({ ok: false, error: { code: "unreachable", message: String(error) } });
        },
        close() {
          // Only a close with nothing read is a close before answering. herdr hangs up after
          // every answer — one request is one connection — so this fires on the ordinary path too
          // and must not overwrite what was read.
          if (held.includes("\n")) {
            return;
          }

          answer({ ok: false, error: { code: "closed", message: `herdr closed before answering ${method}` } });
        },
      },
    }).catch((error: unknown) => {
      answer({ ok: false, error: { code: "unreachable", message: String(error) } });
    });
  });
}

// One line off the socket, read into an answer. A line that is not JSON at all is the server
// saying something nobody here understands, which is a failure of its own rather than a result.
export function readFrame<T>(line: string): HerdrAnswer<T> {
  let frame: { result?: unknown; error?: { code?: unknown; message?: unknown } };

  try {
    frame = JSON.parse(line) as typeof frame;
  } catch {
    return { ok: false, error: { code: "unreadable", message: line.slice(0, 200) } };
  }

  if (frame.error !== undefined && frame.error !== null) {
    return {
      ok: false,
      error: {
        code: typeof frame.error.code === "string" ? frame.error.code : "unknown",
        message: typeof frame.error.message === "string" ? frame.error.message : "",
      },
    };
  }

  return { ok: true, result: frame.result as T };
}

// What a snapshot says about the world. Only the parts anything here reads are named; herdr
// answers with more and the rest is passed over.
export interface HerdrSnapshot {
  workspaces?: { workspace_id?: string; label?: string | null; number?: number; tokens?: Record<string, string> | null }[];
  // Which panes each workspace has open, which tab each one hangs off, and what was reported on
  // it. A run is the pane carrying its name, so this is where a run is read; the tab is read with
  // it because a pane can leave by its tab closing, with nothing said about the pane.
  panes?: HerdrPane[];
  agents?: { terminal_id?: string; agent?: string; agent_status?: string }[];
}

export interface HerdrPane {
  pane_id?: string;
  workspace_id?: string;
  tab_id?: string;
  label?: string | null;
  tokens?: Record<string, string> | null;
}

// The token a pane's run name is reported under, and the source it is reported from. herdr keeps
// what one source reported apart from what another did, so the source is fixed rather than
// whatever each launcher happened to call itself.
export const RUN_TOKEN = "run";
// What herdr already holds panes reported under, a name fixed outside the code.
export const METADATA_SOURCE = "bmo";

// The run a pane carries, or nothing for a pane that is not one — a standing agent's head, a
// shell somebody opened, a pane herdr rebuilt after coming back.
export function runOfPane(pane: { tokens?: Record<string, string> | null } | null | undefined): string | null {
  const name = pane?.tokens?.[RUN_TOKEN];

  return typeof name === "string" && name !== "" ? name : null;
}

// A tab's arrangement as `layout.export` answers it. Asked about one pane, herdr describes the whole
// tab that pane is in, so the pane's own node is looked for in the tree rather than read off the
// root — a pane beside a head is the second half of a split, and the root has no command at all.
export interface LayoutNode {
  type?: string;
  pane_id?: string;
  command?: string[] | null;
  first?: LayoutNode;
  second?: LayoutNode;
}

// What herdr started a pane with: its command, or nothing for a pane holding only a shell — which
// is what a pane herdr rebuilt after coming back holds.
export function commandOfPane(root: LayoutNode | undefined, paneId: string): string[] {
  if (root === undefined) {
    return [];
  }

  if (root.pane_id === paneId) {
    return root.command ?? [];
  }

  const inFirst = commandOfPane(root.first, paneId);

  return inFirst.length > 0 ? inFirst : commandOfPane(root.second, paneId);
}

export interface HerdrEvent {
  event?: string;
  data?: unknown;
}

// Attaching without losing anything, which is an order and not a set of calls.
//
// The subscription has to be open and acknowledged *before* the snapshot is asked for, and the
// events that arrive while the snapshot is in flight have to be held rather than applied, because
// the snapshot does not contain them. Measured: an event raised in exactly that window is absent
// from the snapshot and present on the buffered stream exactly once. Ask for the snapshot first
// and whatever happens in that window is gone with no trace that it ever did.
//
// The transport is handed in so the order can be held to without a live server: `subscribe` hands
// back its acknowledgement and a stream, and `snapshot` is whatever asks for the picture.
export async function attachWithoutLoss<S, E>(transport: {
  // Opens the subscription and resolves once the server has acknowledged it. Everything the
  // stream yields after that is buffered by this function until the snapshot is installed.
  subscribe: (buffer: (event: E) => void) => Promise<void>;
  snapshot: () => Promise<S>;
  install: (snapshot: S) => void;
  apply: (event: E) => void;
}): Promise<void> {
  const held: E[] = [];
  let installed = false;
  // Before the snapshot is in place an event has nothing to be applied to, so it waits. After it
  // is in place the same door leads straight through, which is what makes the hand-over seamless
  // rather than a second pass over a list that is still growing.
  const take = (event: E): void => {
    if (installed) {
      transport.apply(event);

      return;
    }

    held.push(event);
  };

  await transport.subscribe(take);

  const snapshot = await transport.snapshot();

  transport.install(snapshot);
  installed = true;

  // Drained by index rather than by shift so that anything arriving during the drain — the
  // stream is still live — is applied in the order it came and never dropped.
  for (let at = 0; at < held.length; at += 1) {
    transport.apply(held[at] as E);
  }

  held.length = 0;
}

// Which of these runs herdr still holds. A run is the token on its pane, so this is a read of the
// tokens and nothing more; a name no pane carries is a run that is over, or one that was never
// herdr's to begin with.
export function liveRuns(snapshot: HerdrSnapshot, names: readonly string[]): Set<string> {
  const carried = new Set((snapshot.panes ?? []).map(runOfPane).filter((name): name is string => name !== null));

  return new Set(names.filter((name) => carried.has(name)));
}

// The name this run was given, which a pane cannot work out for itself. systemd named a transient
// unit's cgroup after the unit, so a process could read its own name out of `/proc/self/cgroup`; a
// pane has nothing of the kind, so the launcher puts the name in the environment and this reads it
// back. A run started any other way has no name to give, which is the same answer it had before.
export const RUN_NAME_VARIABLE = "BMO_RUN_NAME";

export function ownRunName(env: Record<string, string | undefined> = process.env): string | null {
  return runNameOf(env);
}
