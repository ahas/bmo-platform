// The launcher's side of the tool host: a run's session record built out of its line and its command, a
// token issued for it over the host's control socket before anything starts, and revoked when the run
// ends. A host that does not answer refuses the launch, naming itself and its socket.

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { storeAddress } from "@agency/archive/archive-place";
import { lineCapabilities, lineNamespaces, lineShown } from "./line-declarations";
import { PLACE_VARIABLE, dataRoot, mcpHostPort, mcpHostSocket } from "@agency/support/roots";
import { isTurnVariable } from "./agent-place";

/** The variable a run's program reads its session token from, which its configuration names. */
export const MCP_TOKEN_VARIABLE = "BMO_MCP_TOKEN";

/** The variable a launcher may name the issuer token in, where a deployment keeps it elsewhere. */
export const ISSUER_TOKEN_VARIABLE = "BMO_MCP_HOST_ISSUER_TOKEN";

/** A session record, in the fields the tool host reads. */
export interface ToolSessionRecord {
  /** Who holds the session, a name a tool may read. */
  holderName: string;
  /** The namespaces the session may use. */
  namespaces: string[];
  /** What the session may be shown, by the names its manifests declare. */
  capabilities: string[];
  /** The resources the session may act on. */
  grants: { resource: string }[];
  /** The full names of the entries its namespaces list by name. */
  shown?: string[];
  /** The variables handed to every call's environment. */
  variables?: Record<string, string>;
  /** A file of variables the host reads afresh at every call and lays over `variables`. */
  variablesFile?: string;
  /** The issuer's own metadata, which the host never reads. */
  labels?: Record<string, unknown>;
  /** How long the session stands, in milliseconds. */
  ttlMs?: number;
}

/** A run a record is issued for: the directory it works in, its environment and the agent it runs for. */
export interface ToolRecordRun {
  cwd: string;
  env: Record<string, string | undefined>;
  agent?: string;
}

/**
 * What the deployment adds to a record out of the run: what its agent is given, and the labels the
 * deployment names.
 */
export type ToolRecordReader = (run: ToolRecordRun) => ToolRecordReading | Promise<ToolRecordReading>;

export type ToolRecordReading = {
  // The role of the agent the run is issued for, where it holds one.
  role?: string;
  // The issuer's own values the deployment adds.
  labels?: Record<string, unknown>;
  // The capabilities the run's settings leave out, for a run holding less than it is given.
  withheld?: readonly string[];
  // The capabilities the run holds beside its line's: the bundles its agent's role gives.
  granted?: readonly string[];
};

let reader: ToolRecordReader = () => ({});

/** The deployment's reader of a run's own settings, composed at registration. */
export function composeToolRecord(read: ToolRecordReader): void {
  reader = read;
}

/** A launch refused because the tool host did not answer. */
export class ToolHostDown extends Error {
  constructor(socket: string, why: string) {
    super(`The tool host did not answer on ${socket}, so nothing ran: ${why}`);
    this.name = "ToolHostDown";
  }
}

/** Where a namespace is served, as every line's configuration names it. */
export function toolHostUrl(namespace: string): string {
  return `http://127.0.0.1:${mcpHostPort()}/mcp/${namespace}`;
}

/** The header every configuration hands the host, with the token read from the program's environment. */
export const TOOL_HOST_AUTHORIZATION = `Bearer \${${MCP_TOKEN_VARIABLE}}`;

/**
 * What a program is told it may wait on one call of the tool host, in seconds. A call is waited for until it
 * answers, but Codex and dsh give a call up after a minute unless told a wait, and a client that gave up
 * would lose a result the call had already taken from the next turn; so they are told the longest wait the
 * runtime's own timer holds, about twenty-four days, which no call reaches.
 */
export const TOOL_HOST_WAIT_SECONDS = 2_147_483;

/** An HTTP server as agency's configuration files write one: its URL and the headers it is handed. */
export interface HttpServerEntry {
  url: string;
  headers: Record<string, string>;
}

/** An entry of a configuration file read as an HTTP server, or null for any other. */
export function httpServerEntry(entry: unknown): HttpServerEntry | null {
  const said = entry as { type?: unknown; url?: unknown; headers?: unknown } | null;

  if (said?.type !== "http" || typeof said.url !== "string") return null;

  const headers = Object.fromEntries(
    Object.entries(typeof said.headers === "object" && said.headers !== null ? said.headers : {}).filter((one): one is [string, string] => typeof one[1] === "string"),
  );

  return { url: said.url, headers };
}

/**
 * The variable a bearer header reads its token from, in the `${NAME}` form agency's files write it: for
 * a program that takes the variable's name rather than the header.
 */
export function bearerVariable(headers: Record<string, string>): string | null {
  return /^Bearer \$\{([A-Za-z_][A-Za-z0-9_]*)\}$/.exec(headers.Authorization ?? "")?.[1] ?? null;
}

function present(value: string | undefined): string | undefined {
  return value === undefined || value === "" ? undefined : value;
}

/**
 * The issuer token a launcher asks the host with: the one the environment names, else the host's own under
 * the data root, which it makes at its first start.
 */
export function issuerToken(): string | null {
  const named = present(process.env[ISSUER_TOKEN_VARIABLE])?.trim();

  if (named !== undefined && named !== "") return named;

  try {
    const kept = readFileSync(join(dataRoot(), "tool-host", "issuer-token"), "utf8").trim();

    return kept === "" ? null : kept;
  } catch {
    return null;
  }
}

/**
 * A run's record: its line's namespaces, shown entries and capabilities, with what its agent's role grants
 * less what the run's own settings withhold; the agent it runs for; the data root and the run's own
 * variables every call reads; and its agent's own values under the issuer's labels. How long it stands is
 * the deadline's distance from now.
 */
export async function toolSessionRecord(line: string, command: ToolRecordRun, deadline?: number): Promise<ToolSessionRecord> {
  const env = command.env;
  const own = await reader(command);
  const withheld = new Set(own.withheld ?? []);
  const capabilities = [...new Set([...lineCapabilities(line), ...(own.granted ?? [])])].filter((one) => !withheld.has(one));
  const variables: Record<string, string> = Object.fromEntries(
    Object.entries(env).filter((entry): entry is [string, string] => isTurnVariable(entry[0]) && entry[1] !== undefined),
  );
  const root = present(env.BMO_DATA_DIR) ?? dataRoot();
  const place = present(env[PLACE_VARIABLE]);
  const labels: Record<string, unknown> = {
    line,
    checkout: command.cwd,
    archive: storeAddress(env, root) ?? "",
  };

  if (own.labels !== undefined) Object.assign(labels, own.labels);
  if (own.role !== undefined) labels.role = own.role;
  if (place !== undefined) labels.place = place;

  variables.BMO_DATA_DIR = root;

  const shown = lineShown(line);

  return {
    holderName: command.agent ?? line,
    namespaces: [...lineNamespaces(line)],
    capabilities,
    grants: [],
    ...(shown.length === 0 ? {} : { shown: [...shown] }),
    variables,
    labels,
    ...(deadline === undefined ? {} : { ttlMs: Math.max(0, deadline - Date.now()) }),
  };
}

// How often a launch asks again while the host is restarting under a deployment.
const retryEveryMs = 200;

/**
 * A token issued for a record, waited for however long the host takes to answer: a host restarting under a
 * deployment, or not yet writing its issuer token, is asked again until it does.
 */
export async function issueToolToken(record: ToolSessionRecord, options: { socket?: string } = {}): Promise<string> {
  const socket = options.socket ?? mcpHostSocket();

  for (;;) {
    const issuer = issuerToken();

    if (issuer === null) {
      await Bun.sleep(retryEveryMs);
      continue;
    }

    try {
      const answer = await fetch("http://localhost/tool-sessions", {
        unix: socket,
        method: "POST",
        headers: { "content-type": "application/json", Authorization: `Bearer ${issuer}` },
        body: JSON.stringify(record),
      });
      const said = (await answer.json().catch(async () => null)) as { token?: unknown; error?: unknown } | null;

      if (answer.ok && typeof said?.token === "string") return said.token;

      // A host that answered and refused the record is no host restarting: it is said at once.
      throw new ToolHostDown(socket, `${answer.status} ${typeof said?.error === "string" ? said.error : ""}`.trim());
    } catch (error) {
      if (error instanceof ToolHostDown) throw error;
    }

    await Bun.sleep(retryEveryMs);
  }
}

/**
 * What a namespace's owner says of it beside its tools, as the tool host hands it on; null where the host
 * does not answer or the namespace says nothing.
 */
export async function namespaceFacts(namespace: string, options: { socket?: string } = {}): Promise<Record<string, unknown> | null> {
  const issuer = issuerToken();

  if (issuer === null) return null;

  try {
    const answer = await fetch(`http://localhost/namespaces/${encodeURIComponent(namespace)}`, {
      unix: options.socket ?? mcpHostSocket(),
      headers: { Authorization: `Bearer ${issuer}` },
    });
    const said: unknown = answer.ok ? await answer.json() : null;
    const facts = (said as { facts?: unknown } | null)?.facts;

    return typeof facts === "object" && facts !== null && !Array.isArray(facts) ? (facts as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

/** A token taken back; a host that does not answer has lost it already, so this never throws. */
export async function revokeToolToken(token: string, options: { socket?: string } = {}): Promise<void> {
  const issuer = issuerToken();

  if (issuer === null) return;

  try {
    await fetch(`http://localhost/tool-sessions/${encodeURIComponent(token)}`, {
      unix: options.socket ?? mcpHostSocket(),
      method: "DELETE",
      headers: { Authorization: `Bearer ${issuer}` },
    });
  } catch {
    // The record lapses with its deadline, or stands until the host is next told.
  }
}

/** A token for a run of `line`, or null where the run needs none: one whose record holds no capability. */
export async function toolTokenFor(
  line: string,
  command: ToolRecordRun,
  options: { deadline?: number; socket?: string } = {},
): Promise<string | null> {
  const record = await toolSessionRecord(line, command, options.deadline);

  return record.capabilities.length === 0 ? null : await issueToolToken(record, { socket: options.socket });
}

/** What a tool's entry answers a call with, in the fields a caller reads. */
export interface ToolAnswer {
  content?: { type: string; text?: string }[];
  isError?: boolean;
  structuredContent?: unknown;
}

/** An answer's text, its pieces joined a line each. */
export function toolAnswerText(answer: ToolAnswer): string {
  return (answer.content ?? []).map((piece) => piece.text ?? JSON.stringify(piece)).join("\n");
}

/**
 * A tool's entry called for the system on the host's control socket, under a session of its own: the
 * entry's capabilities, a shell's line, over this process's data root. An entry the host does not serve,
 * or a host that does not answer, throws; a refusal or a failure of the entry is its answer, marked as an
 * error. A call aborted before it answers hangs up on the host, which ends every process the call started.
 */
export async function callToolEntry(
  tool: string,
  args: Record<string, unknown>,
  options: { socket?: string; signal?: AbortSignal } = {},
): Promise<ToolAnswer> {
  const socket = options.socket ?? mcpHostSocket();
  const issuer = issuerToken();

  if (issuer === null) throw new Error(`the tool host at ${socket} has no issuer token`);
  const asked = async (path: string, token: string, body?: unknown, method?: string): Promise<Response> => {
    try {
      return await fetch(`http://localhost${path}`, {
        unix: socket,
        signal: options.signal,
        method: method ?? (body === undefined ? "GET" : "POST"),
        headers: {
          Authorization: `Bearer ${token}`,
          ...(body === undefined ? {} : { "content-type": "application/json" }),
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
    } catch (error) {
      if (options.signal?.aborted === true) throw error;

      throw new Error(`the tool host at ${socket} did not answer: ${error instanceof Error ? error.message : String(error)}`);
    }
  };
  const split = tool.indexOf("/");
  const namespace = split === -1 ? "" : tool.slice(0, split);
  const name = split === -1 ? tool : tool.slice(split + 1);
  const entry = await asked(`/namespaces/${encodeURIComponent(namespace)}/tools/${encodeURIComponent(name)}`, issuer);

  if (!entry.ok) throw new Error(`the tool host serves no ${tool}`);

  const { capabilities, grant } = (await entry.json()) as { capabilities?: string[]; grant?: string | null };
  const issued = await asked("/tool-sessions", issuer, {
    holderName: "shell",
    namespaces: [namespace],
    capabilities: capabilities ?? [],
    grants: grant === null || grant === undefined ? [] : [{ resource: grant }],
    variables: { BMO_DATA_DIR: dataRoot(), BMO_TURN_PRIVILEGED: "1" },
    labels: { line: "shell" },
  });

  if (!issued.ok) throw new Error(`the tool host refused the session for ${tool}`);

  const { token } = (await issued.json()) as { token: string };

  try {
    const answer = await asked("/tool-calls", token, { namespace, name, input: args });

    if (!answer.ok) throw new Error(`the tool host refused the call of ${tool}`);

    const said = (await answer.json()) as { result?: ToolAnswer };

    return said.result ?? {};
  } finally {
    await asked(`/tool-sessions/${encodeURIComponent(token)}`, issuer, undefined, "DELETE").catch(() => undefined);
  }
}
