import { existsSync, readFileSync } from "node:fs";
import { connect } from "node:net";
import { join } from "node:path";

import { dataRoot, serviceSocketDir } from "@agency/support/roots";
import { nodePath } from "@agency/archive/work-node-store";

import type { Workplace } from "@agency/archive/work-node-store";

// A worker's place on a work node, as this machine reaches it (docs/work-nodes.md): where the node's
// link keeps its sockets, its state and the node's readings here, how this machine's paths read on the
// node, the spec the link is held to, and the questions about a place's processes, answered locally
// for a place here and through the link for a place on a node.

/** Where everything this machine keeps of one node's link stands. */
export function nodeDir(node: string, root: string = dataRoot()): string {
  return join(root, "nodes", node);
}

/**
 * Where the one connection a run of commands to a node shares is held: under the runtime directory,
 * since a socket's path holds at most 107 bytes and a data root may be deep. OpenSSH names it by the
 * login it holds.
 */
export function sharedLoginPath(): string {
  return join(serviceSocketDir(), "ssh-%C");
}

/** The node's herdr session, forwarded here: where every herdr call for an agent on that node goes. */
export function nodeHerdrSocket(node: string, root: string = dataRoot()): string {
  return join(nodeDir(node, root), "herdr.sock");
}

export function linkStatePath(node: string, root: string = dataRoot()): string {
  return join(nodeDir(node, root), "link-state.json");
}

/** The node's load, memory and pressure files as the link last read them, a proc root of its own. */
export function nodeProcRoot(node: string, root: string = dataRoot()): string {
  return join(nodeDir(node, root), "proc");
}

/** The file a place on a node carries naming its node, which every question about its processes reads. */
const PLACE_NODE_FILE = "node.json";

/** The node a place stands on, or null for a place here. */
export function placeNode(place: string): string | null {
  return readJson<{ node?: unknown }>(join(place, PLACE_NODE_FILE))?.node as string | null ?? null;
}

function readJson<T>(path: string): T | null {
  try {
    return JSON.parse(readFileSync(path, "utf8")) as T;
  } catch {
    return null;
  }
}

/** A node's workplace roots as the node reads them, once its setup has read the node's home. */
export interface NodeRoots {
  node: string;
  root: string;
  home: string;
}

export function nodeRootsOf(workplace: Workplace): NodeRoots | null {
  return workplace.home === null ? null : { node: workplace.node, root: nodePath(workplace.root, workplace.home), home: workplace.home };
}

/** The bridge's socket on the node: the one name every program there is handed. */
export function nodeBridgeSocket(roots: NodeRoots): string {
  return join(roots.root, "data", "bridge.sock");
}

/** The search path a program on the node is given ahead of the node's own. */
export function nodePathHead(roots: NodeRoots): string[] {
  return [join(roots.home, ".bun", "bin"), join(roots.home, ".local", "bin"), join(roots.home, ".cargo", "bin"), join(roots.root, "deploy", "bin")];
}

/**
 * What a head pane on the node is started on: the node's own home and search path, its data root and
 * code, and the bridge. Nothing of this machine's environment goes, a token of the bot's least of all.
 */
export function nodeHeadEnv(roots: NodeRoots, runName: string): Record<string, string> {
  return {
    HOME: roots.home,
    PATH: [...nodePathHead(roots), "/usr/local/bin", "/usr/bin", "/bin"].join(":"),
    LANG: process.env.LANG ?? "C.UTF-8",
    BMO_DATA_DIR: join(roots.root, "data"),
    BMO_SOURCE_DIR: join(roots.root, "deploy"),
    BMO_HOME_BRIDGE: nodeBridgeSocket(roots),
    BMO_NODE: roots.node,
    BMO_RUN_NAME: runName,
  };
}

/** Where a checkout on a node is read here: what the tool host's session record names. */
export function checkoutShadow(node: string, cwd: string, root: string = dataRoot()): string {
  return join(nodeDir(node, root), "shadow", cwd.replace(/[^A-Za-z0-9]/g, "-"));
}

/** What the link last wrote of itself. */
export interface LinkState {
  node: string;
  state: "connected" | "down";
  since: number | null;
  heard: number | null;
  home: string | null;
  dropped: string | null;
  pid: number;
}

export function readLinkState(node: string, root: string = dataRoot()): LinkState | null {
  return readJson<LinkState>(linkStatePath(node, root));
}

/** How long the link may go without a word from the node before it is taken as down. */
const HEARD_WITHIN_MS = 20_000;

/** Whether a node's link stands now: connected, and heard from lately. */
export function linkUp(node: string, now: number = Date.now(), root: string = dataRoot()): boolean {
  const state = readLinkState(node, root);

  return state !== null && state.state === "connected" && state.heard !== null && now - state.heard <= HEARD_WITHIN_MS;
}

/** A process as a place's records name it. */
export interface PlaceProcess {
  pid: number;
  birth: string | null;
}

/**
 * Whether a process of a place on a node is alive, as the link last told it; null where the link has
 * told nothing lately, which says nothing either way.
 */
export function aliveOnNode(place: string, process: PlaceProcess, now: number = Date.now()): boolean | null {
  const told = readJson<{ at?: number; alive?: { pid: number; birth: string }[] }>(join(place, "node-alive.json"));

  if (told?.at === undefined || now - told.at > HEARD_WITHIN_MS) return null;

  return (told.alive ?? []).some((one) => one.pid === process.pid && (process.birth === null || one.birth === process.birth));
}

/**
 * A signal for a process of a place on a node, carried by its link; whether the link took it, as the link
 * answers it or its socket closes or fails.
 */
export async function signalOnNode(node: string, process: PlaceProcess, signal: number = 15, root: string = dataRoot()): Promise<boolean> {
  const socket = join(nodeDir(node, root), "control.sock");

  if (!existsSync(socket)) return false;

  return await new Promise<boolean>((resolve) => {
    const client = connect(socket);
    let answer = "";

    client.on("connect", () => client.write(`${JSON.stringify({ signal: { pid: process.pid, birth: process.birth, signal } })}\n`));
    client.on("data", (chunk) => {
      answer += chunk.toString("utf8");
    });
    client.on("end", () => {

      try {
        resolve((JSON.parse(answer) as { ok?: boolean }).ok === true);
      } catch {
        resolve(false);
      }
    });
    client.on("error", () => {
      resolve(false);
    });
  });
}

