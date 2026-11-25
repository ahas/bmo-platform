// Starting and ending a run in a herdr pane, which is the other place a run apart can live.
//
// A pane is not a unit and the differences are the whole of this file. A workspace takes a label,
// a cwd and an environment but no command; the command arrives in a second call that puts one pane
// running that argv directly, with no shell between. Giving the argv rather than typing it buys
// more than not having to quote it: the pane ends when the command ends, so a run's ending reaches
// the picture with nothing having to announce it.
//
// Where the pane goes is decided by whose run it is. Every agent has a workspace of its own under
// the name the owner calls it, whose head pane keeps the workspace standing and holds the agent's
// resident program or follows its view (`packages/agent/resident-pane.ts`,
// `packages/agent/root-host.ts`); a run apart from both opens in a tab of its own there. A run with
// no agent opens in a workspace of its own, labelled with what the run is.
// Either way the run's name is a token on its pane, never a label: labels are what a person reads,
// and a person reads a title, not a unit name.
//
// What a label does not do is refuse. systemd would not start a second unit under a name it
// already held, so the name was the mutex and nothing had to ask; herdr makes both workspaces
// happily. So anything that must have one run per name asks first, and that question belongs to
// whoever has the name rather than here.

import declaredStop from "@agency/policy/run-stop.toml";
import { guardClaudeLaunch, guardPanePrompt } from "./live-claude-guard";
import {
  commandOfPane,
  herdrCall,
  liveRuns,
  METADATA_SOURCE,
  runOfPane,
  RUN_TOKEN,
  type HerdrAnswer,
  type HerdrSnapshot,
  type LayoutNode,
} from "./herdr";

// What a launch answers with. The pane is named so a caller can read it, write to it and close it;
// the workspace is named because that is where it stands.
export interface OpenedPane {
  workspaceId: string;
  paneId: string;
}

// An agent's own workspace: which agent it is, the name the owner calls it — which is the
// workspace's label — and the head pane, which shows the agent and keeps the workspace up. A
// home of a data root other than the bot's own names that root, and stands apart from the bot's
// (docs/agent-control.md, 「What goes where」).
export interface AgentHome {
  agent: string;
  label: string;
  // The label to go back to once a turn standing under `label` has ended, for an agent whose label
  // says what runs in it rather than what it is.
  resting?: string;
  data?: string;
  head: { command: readonly string[]; cwd: string; env: Record<string, string> };
  // The herdr the home stands in, where it is not this machine's: a work node's session, forwarded
  // here (docs/work-nodes.md, 「The bot's side of a remote agent」).
  socket?: string;
}

// Where a call about a home goes: the socket the caller names, or else the herdr the home stands in.
function at(home: AgentHome, options: Where): Where {
  return options.socket !== undefined || home.socket === undefined ? options : { ...options, socket: home.socket };
}

// The mark a head pane's loop runs under, `bmo-head-<agent>`, whichever head it starts. It is how a head
// is told apart from everything else in the workspace, since a head carries no run token and herdr
// keeps no other mark a rebuild would drop.
export const HEAD_MARK = "bmo-head-";

// The head program a head pane runs, as the deploy tree's `bin/` holds it.
export const PANE_HEAD = "/bin/pane-head";

/** Whether a pane's command line is an agent's head loop. */
export function isHeadCommand(command: readonly string[]): boolean {
  return command.some((part) => part.startsWith(HEAD_MARK));
}

/** The agent a head loop stands for, read off its mark, or null for a command that is no head. */
export function headAgentOf(command: readonly string[]): string | null {
  const mark = command.find((part) => part.startsWith(HEAD_MARK));

  return mark === undefined ? null : mark.slice(HEAD_MARK.length);
}

// The one exit a head makes that its pane is not to start it again on. A head ends for two other
// reasons — its tree moved to another revision, or it fell over — and on both the pane starts the
// deployed tree's head again in the same place, which is what makes the pane the agent's one
// standing pane rather than one opened and closed around a run. This code says the opposite: a
// duplicate host, or a stop meant for the pane itself.
export const HEAD_STOPPED = 3;

// The verb a worker's dev-server pane runs, a word of its command line after the verbs' binary, and
// the tab it stands in. Like the head, the pane carries no run token, and the command it was started
// with is the only mark it has.
export const DEV_VERB = "dev-pane";
export const DEV_TAB_LABEL = "dev server";

// The mark a standing pane of an owner's own carries on its command line, as the name its shell runs
// under: a pane that keeps a process up beside the head for as long as its owner wants it, which no
// pass over leftovers closes. What the pane is for is the owner's; this layer knows only the mark.
export const STANDING_MARK = "bmo-standing-";

// A command line run as a standing pane, under the mark and a name of the owner's choosing.
export function standingCommand(name: string, command: readonly string[]): string[] {
  return ["/bin/sh", "-c", 'exec "$@"', `${STANDING_MARK}${name}`, ...command];
}

// Whether a pane's command line is a standing one of any kind: a head, a dev server, a standing pane.
function keepsStanding(command: readonly string[]): boolean {
  return isHeadCommand(command) || command.some((part) => part === DEV_VERB || part.startsWith(STANDING_MARK));
}

// The token a standing agent's workspace is reported under, and the one naming the data root of a
// workspace that is not the bot's own.
const AGENT_TOKEN = "agent";
const DATA_TOKEN = "data";

function homeTokens(home: AgentHome): Record<string, string> {
  return home.data === undefined ? { [AGENT_TOKEN]: home.agent } : { [AGENT_TOKEN]: home.agent, [DATA_TOKEN]: home.data };
}

function marksHome(tokens: Record<string, string> | null | undefined, home: AgentHome): boolean {
  return tokens?.[AGENT_TOKEN] === home.agent && tokens[DATA_TOKEN] === home.data;
}

interface WorkspaceCreated {
  workspace?: { workspace_id?: string };
  tab?: { tab_id?: string };
}

interface LayoutApplied {
  layout?: { tab_id?: string; root?: { pane_id?: string } };
}

type Where = { socket?: string };

export async function openPane(
  run: {
    // The run's own name, reported as a token on the pane. That token is the whole mapping from
    // the row the archive holds to the pane the work is in.
    name: string;
    // What the run is, in the words a person watching herdr reads: the workspace's label for a run
    // of its own, the pane's label for a run beside a standing agent's head.
    title: string;
    // The standing agent whose workspace this run opens in, or nothing for a run that has one of
    // its own.
    home?: AgentHome | null;
    cwd: string;
    command: readonly string[];
    env: Record<string, string>;
  },
  options: Where = {},
): Promise<HerdrAnswer<OpenedPane>> {
  // Asked before a workspace is made for it, so a refused run leaves nothing standing in herdr
  // (docs/live-claude-guard.md).
  await guardClaudeLaunch("pane", run.command, run.env);

  const title = shortTitle(run.title);

  if (run.home !== undefined && run.home !== null) {
    const home = await standingWorkspace(run.home, options);

    if (home.ok) {
      return await openInHome(run, title, home.result, options);
    }

    // A standing workspace that could not be had is no reason to leave the work undone: the run
    // goes up in a workspace of its own, where a herdr that answers at all can still put it.
    console.error(`herdr: ${run.home.label}'s workspace could not be had (${home.error.code}); ${run.name} opens on its own`);
  }

  const made = await herdrCall<WorkspaceCreated>("workspace.create", { label: title, cwd: run.cwd, env: run.env }, options);

  if (!made.ok) {
    return made;
  }

  const workspaceId = made.result.workspace?.workspace_id;
  const tabId = made.result.tab?.tab_id;

  if (workspaceId === undefined || tabId === undefined) {
    return { ok: false, error: { code: "unreadable", message: "workspace.create named no workspace and tab" } };
  }

  // The tab alone, never the tab and the workspace together — herdr refuses both at once, and the
  // workspace alone would add a tab beside the shell one instead of replacing it, leaving a shell
  // behind that keeps the workspace standing after the run has ended.
  let applied: HerdrAnswer<LayoutApplied>;
  try {
    applied = await herdrCall<LayoutApplied>(
      "layout.apply",
      { tab_id: tabId, root: { type: "pane", command: [...run.command], cwd: run.cwd, env: run.env } },
      options,
    );
  } catch (error) {
    await herdrCall("workspace.close", { workspace_id: workspaceId }, options);
    throw error;
  }
  const paneId = applied.ok ? applied.result.layout?.root?.pane_id : undefined;

  if (paneId === undefined) {
    // A workspace with nothing running in it is left behind in the sidebar with nobody to close it.
    await herdrCall("workspace.close", { workspace_id: workspaceId }, options);

    return applied.ok ? { ok: false, error: { code: "unreadable", message: "layout.apply named no pane" } } : applied;
  }

  return await named(run.name, { workspaceId, paneId }, options);
}

// A run in a standing agent's workspace, in a tab of its own. An agent's turns go into the program
// standing in its head, so what still opens here is a run apart from that program, and it is never
// split into the head's tab: the head is the agent's one pane (docs/agent-control.md, 「One standing
// pane per agent」). Laying the head's tab out would also start every pane in it afresh.
async function openInHome(
  run: { name: string; cwd: string; command: readonly string[]; env: Record<string, string> },
  title: string,
  home: StandingWorkspace,
  options: Where,
): Promise<HerdrAnswer<OpenedPane>> {
  const applied = await herdrCall<LayoutApplied>(
    "layout.apply",
    {
      workspace_id: home.workspaceId,
      tab_label: title,
      root: { type: "pane", label: title, command: [...run.command], cwd: run.cwd, env: run.env },
    },
    options,
  );
  const paneId = applied.ok ? applied.result.layout?.root?.pane_id : undefined;

  if (paneId === undefined) {
    return applied.ok ? { ok: false, error: { code: "unreadable", message: "layout.apply named no pane" } } : applied;
  }

  return await named(run.name, { workspaceId: home.workspaceId, paneId }, options);
}

// The run's name put on the pane it is in. A pane carrying no name is a run nothing can find —
// not the liveness question, not a stop — so one that could not be named is closed and the launch
// reported as failed, which is what sends the caller to its fallback.
async function named(name: string, pane: OpenedPane, options: Where): Promise<HerdrAnswer<OpenedPane>> {
  const reported = await herdrCall(
    "pane.report_metadata",
    { pane_id: pane.paneId, source: METADATA_SOURCE, tokens: { [RUN_TOKEN]: name } },
    options,
  );

  if (!reported.ok) {
    await herdrCall("pane.close", { pane_id: pane.paneId }, options);

    return reported;
  }

  return { ok: true, result: pane };
}

// A title kept to what a sidebar and a pane border can show. The first line only, since a title
// is one line wherever herdr draws it.
export function shortTitle(text: string, most = 48): string {
  const line = text.split("\n").map((one) => one.trim()).find((one) => one !== "") ?? "";
  const flat = line.replace(/\s+/g, " ");

  return flat.length <= most ? flat : `${flat.slice(0, most - 1).trimEnd()}…`;
}

export interface StandingWorkspace {
  workspaceId: string;
  headPaneId: string;
  headTabId: string;
}

interface ExportedLayout {
  layout?: { root?: LayoutNode };
}

// The fault a home's workspace found gone before its head was looked for answers.
const WORKSPACE_GONE = "workspace_not_found";

// A standing agent's workspace, found or made, with its head running.
//
// Found by the token this reports on it first, and by its label after: the token is live state a
// herdr that comes back does not rebuild, while the label is. A workspace found under a label the
// owner has since changed is given the new one. A workspace with no head running in it — one herdr
// rebuilt, whose panes are fresh shells — gets its head started again in a tab of its own, and the
// rebuilt shells are closed.
//
// Two launches can both find none and both make one. Each reads the picture again after making its
// own, and where the agent now has more than one each takes the lowest-numbered and closes its own
// if that is not it, so both arrive at the same one.
//
// A workspace found that is gone by the time its head is looked for — its one pane's loop ended
// between the two readings — is looked for again, and made where none stands, once.
export async function standingWorkspace(home: AgentHome, options: Where = {}): Promise<HerdrAnswer<StandingWorkspace>> {
  options = at(home, options);
  const stood = await foundOrMade(home, options);

  return !stood.ok && stood.error.code === WORKSPACE_GONE ? await foundOrMade(home, options) : stood;
}

async function foundOrMade(home: AgentHome, options: Where): Promise<HerdrAnswer<StandingWorkspace>> {
  const first = await snapshotOf(options);

  if (!first.ok) {
    return first;
  }

  let workspaceId = homeCandidates(first.result, home)[0] ?? null;

  if (workspaceId === null) {
    const made = await herdrCall<WorkspaceCreated>("workspace.create", { label: home.label, cwd: home.head.cwd, env: home.head.env }, options);

    if (!made.ok) {
      return made;
    }

    const madeId = made.result.workspace?.workspace_id;
    const tabId = made.result.tab?.tab_id;

    if (madeId === undefined || tabId === undefined) {
      return { ok: false, error: { code: "unreadable", message: "workspace.create named no workspace and tab" } };
    }

    const applied = await herdrCall<LayoutApplied>(
      "layout.apply",
      { tab_id: tabId, tab_label: home.label, root: headNode(home) },
      options,
    );

    if (!applied.ok) {
      await herdrCall("workspace.close", { workspace_id: madeId }, options);

      return applied;
    }

    await herdrCall("workspace.report_metadata", { workspace_id: madeId, source: METADATA_SOURCE, tokens: homeTokens(home) }, options);

    const again = await snapshotOf(options);
    const settled = again.ok ? (homeCandidates(again.result, home)[0] ?? madeId) : madeId;

    if (settled !== madeId) {
      await herdrCall("workspace.close", { workspace_id: madeId }, options);
    }

    workspaceId = settled;
  }

  return await headed(home, workspaceId, options);
}

// The agent's workspace closed, with everything standing in it: for an agent that is no more, such
// as a pool place taken away. A herdr that cannot be asked closes nothing and says so.
export async function closeStandingWorkspace(home: AgentHome, options: Where = {}): Promise<HerdrAnswer<number>> {
  options = at(home, options);
  const snapshot = await snapshotOf(options);

  if (!snapshot.ok) return snapshot;

  let closed = 0;

  for (const workspaceId of homeCandidates(snapshot.result, home)) {
    if ((await herdrCall("workspace.close", { workspace_id: workspaceId }, options)).ok) closed += 1;
  }

  return { ok: true, result: closed };
}

// The agent's workspace shown under another label where it stands, and nothing stood where it does
// not: the sidebar's entry, and the tab and border of the head pane named.
export async function relabelHome(home: AgentHome, label: string, headPaneId: string, options: Where = {}): Promise<void> {
  options = at(home, options);
  const seen = await snapshotOf(options);

  if (!seen.ok) return;

  const workspaceId = homeCandidates(seen.result, home)[0];
  const workspace = (seen.result.workspaces ?? []).find((one) => one.workspace_id === workspaceId);

  if (workspaceId === undefined || workspace === undefined || workspace.label === label) return;

  await herdrCall("workspace.rename", { workspace_id: workspaceId, label }, options);

  const head = (seen.result.panes ?? []).find((pane) => pane.pane_id === headPaneId && pane.workspace_id === workspaceId);

  if (head?.tab_id !== undefined) await herdrCall("tab.rename", { tab_id: head.tab_id, label }, options);
  if (head !== undefined) await herdrCall("pane.rename", { pane_id: headPaneId, label }, options);
}

// The workspaces that are this agent's, the one to use first. A workspace is this agent's when it
// carries this agent's token, or when it carries no agent's token and the agent's label or the label
// it rests on — which is what one herdr rebuilt looks like, or one another launch has only just made. The lowest number
// comes first, since that is the order every launch reading the same picture agrees on.
function homeCandidates(snapshot: HerdrSnapshot, home: AgentHome): string[] {
  return (snapshot.workspaces ?? [])
    .filter((workspace) => marksHome(workspace.tokens, home)
      || (workspace.tokens?.[AGENT_TOKEN] === undefined && (workspace.label === home.label || (home.resting !== undefined && workspace.label === home.resting))))
    .sort((one, two) => (one.number ?? 0) - (two.number ?? 0))
    .map((workspace) => workspace.workspace_id)
    .filter((id): id is string => id !== undefined);
}

// The agent's workspace as it stands, with every pane in it that runs a head loop, or null where the
// agent has no workspace. Nothing is made, renamed or closed here: the standing pass decides from
// this which heads are left over.
async function agentHeads(
  home: AgentHome,
  options: Where = {},
): Promise<HerdrAnswer<{ workspaceId: string; heads: string[] } | null>> {
  options = at(home, options);
  const seen = await snapshotOf(options);

  if (!seen.ok) {
    return seen;
  }

  const workspaceId = homeCandidates(seen.result, home)[0];

  if (workspaceId === undefined) {
    return { ok: true, result: null };
  }

  const heads: string[] = [];

  for (const pane of seen.result.panes ?? []) {
    if (pane.pane_id !== undefined && pane.workspace_id === workspaceId && runOfPane(pane) === null
      && isHeadCommand(await exportedCommand(pane.pane_id, options))) {
      heads.push(pane.pane_id);
    }
  }

  return { ok: true, result: { workspaceId, heads } };
}

/**
 * The agents herdr's workspaces stand for, read off each workspace's agent token: the archive's
 * agents and the names that launch with no row alike. A workspace of another data root counts under
 * its own name as well; a home built for the name finds only the workspaces of its own root.
 *
 * A restarted herdr revives workspaces without their tokens, so a workspace with none counts under
 * the name its head loops carry in their marks; one with no head is nobody's. A home built for that
 * name then finds the workspace by its label, which is this root's only where the label is the one
 * this root gives the name.
 */
export async function workspaceAgents(options: Where = {}): Promise<HerdrAnswer<string[]>> {
  const seen = await snapshotOf(options);

  if (!seen.ok) {
    return seen;
  }

  const names = new Set<string>();

  for (const workspace of seen.result.workspaces ?? []) {
    const token = workspace.tokens?.[AGENT_TOKEN];

    if (token !== undefined) {
      names.add(token);
      continue;
    }

    for (const pane of seen.result.panes ?? []) {
      if (pane.pane_id === undefined || pane.workspace_id !== workspace.workspace_id || runOfPane(pane) !== null) continue;

      const agent = headAgentOf(await exportedCommand(pane.pane_id, options));

      if (agent !== null) names.add(agent);
    }
  }

  return { ok: true, result: [...names] };
}

/**
 * The agent's head panes whose loop starts anything but `pane-head` — one a herdr kept standing from
 * an earlier deployment — for the standing pass to replace once nothing of the agent's runs on them.
 */
export async function headsOfOtherKind(home: AgentHome, options: Where = {}): Promise<HerdrAnswer<string[]>> {
  options = at(home, options);
  const standing = await agentHeads(home, options);

  if (!standing.ok || standing.result === null) return standing.ok ? { ok: true, result: [] } : standing;

  const other: string[] = [];

  for (const paneId of standing.result.heads) {
    if (!(await exportedCommand(paneId, options)).some((part) => part.endsWith(PANE_HEAD))) other.push(paneId);
  }

  return { ok: true, result: other };
}

function headNode(home: AgentHome): Record<string, unknown> {
  return { type: "pane", label: home.label, command: [...home.head.command], cwd: home.head.cwd, env: home.head.env };
}

async function headed(home: AgentHome, workspaceId: string, options: Where): Promise<HerdrAnswer<StandingWorkspace>> {
  const seen = await snapshotOf(options);

  if (!seen.ok) {
    return seen;
  }

  const workspace = (seen.result.workspaces ?? []).find((one) => one.workspace_id === workspaceId);
  const panes = (seen.result.panes ?? []).filter((pane) => pane.workspace_id === workspaceId);

  if (workspace === undefined) {
    return { ok: false, error: { code: WORKSPACE_GONE, message: `${home.label}'s workspace went away` } };
  }

  // A name the owner has changed since the workspace was made goes on everything that shows it:
  // the sidebar's entry here, and the tab and the head's own border once the head is found.
  const renamed = workspace.label !== home.label;

  if (renamed) {
    await herdrCall("workspace.rename", { workspace_id: workspaceId, label: home.label }, options);
  }

  if (!marksHome(workspace.tokens, home)) {
    await herdrCall("workspace.report_metadata", { workspace_id: workspaceId, source: METADATA_SOURCE, tokens: homeTokens(home) }, options);
  }

  let head: { paneId: string; tabId: string } | null = null;
  const leftovers: string[] = [];

  // What carries no run is either something running its own command — a head, a retiring head
  // beside it, a worker's dev server, a standing pane — or nothing at all: a shell herdr rebuilt, or
  // a pane whose head loop let go. The first kind is kept, the second closed, and this is read of every pane
  // rather than only when a head has to be made, since a leftover beside a live head stands as a
  // tab of nothing for as long as nobody looks.
  for (const pane of panes) {
    if (pane.pane_id === undefined || runOfPane(pane) !== null) {
      continue;
    }

    const exported = await herdrCall<ExportedLayout>("layout.export", { pane_id: pane.pane_id }, options);
    const command = exported.ok ? commandOfPane(exported.result.layout?.root, pane.pane_id) : [];

    if (isHeadCommand(command)) {
      head ??= { paneId: pane.pane_id, tabId: pane.tab_id ?? "" };
    } else if (!keepsStanding(command)) {
      leftovers.push(pane.pane_id);
    }
  }

  if (head === null) {
    const applied = await herdrCall<LayoutApplied>(
      "layout.apply",
      { workspace_id: workspaceId, tab_label: home.label, root: headNode(home) },
      options,
    );
    const paneId = applied.ok ? applied.result.layout?.root?.pane_id : undefined;
    const tabId = applied.ok ? applied.result.layout?.tab_id : undefined;

    if (paneId === undefined || tabId === undefined) {
      return applied.ok ? { ok: false, error: { code: "unreadable", message: "layout.apply named no head pane" } } : applied;
    }

    await closePanes(leftovers, options);

    return { ok: true, result: { workspaceId, headPaneId: paneId, headTabId: tabId } };
  }

  await closePanes(leftovers, options);

  if (renamed) {
    await herdrCall("tab.rename", { tab_id: head.tabId, label: home.label }, options);
    await herdrCall("pane.rename", { pane_id: head.paneId, label: home.label }, options);
  }

  return { ok: true, result: { workspaceId, headPaneId: head.paneId, headTabId: head.tabId } };
}

async function exportedCommand(paneId: string, options: Where): Promise<string[]> {
  const exported = await herdrCall<ExportedLayout>("layout.export", { pane_id: paneId }, options);

  return exported.ok ? commandOfPane(exported.result.layout?.root, paneId) : [];
}

async function closePanes(paneIds: readonly string[], options: Where): Promise<void> {
  for (const paneId of paneIds) {
    await herdrCall("pane.close", { pane_id: paneId }, options);
  }
}

// The leftovers of an agent's workspace closed, with nothing made and nothing renamed: a pane with
// no run in it and no command of its own is a tab of nothing, and the bot sweeps them as it stands
// the heads up. A workspace that is not there has none.
export async function closeLeftovers(home: AgentHome, options: Where = {}): Promise<HerdrAnswer<number>> {
  options = at(home, options);
  const seen = await snapshotOf(options);

  if (!seen.ok) {
    return seen;
  }

  const workspaceId = homeCandidates(seen.result, home)[0];

  if (workspaceId === undefined) {
    return { ok: true, result: 0 };
  }

  const leftovers: string[] = [];

  for (const pane of seen.result.panes ?? []) {
    if (pane.pane_id === undefined || pane.workspace_id !== workspaceId || runOfPane(pane) !== null) {
      continue;
    }

    const command = await exportedCommand(pane.pane_id, options);

    if (!keepsStanding(command)) {
      leftovers.push(pane.pane_id);
    }
  }

  await closePanes(leftovers, options);

  return { ok: true, result: leftovers.length };
}

// A worker's dev server brought to what its row says: one pane running the command given, in a
// tab of its own in the worker's standing workspace, or none at all where the command is null.
//
// The pane is told apart the way the head is — no run token, and a command herdr answers for it —
// and it is this worker's when the argument after the script is the worker's name. A pane running
// any other command line for this worker, a script of an older version most of all, is closed and
// the current one started. Where more than one stands, the first the snapshot lists is kept. The
// caller holds the worker's dev-pane lock, so nothing else is closing panes here meanwhile.
//
// Nothing is focused: a tab brought forward takes the screen from whoever is looking elsewhere.
export async function standingDevPane(
  home: AgentHome,
  dev: { command: readonly string[] | null; cwd: string; env: Record<string, string> },
  options: Where = {},
): Promise<HerdrAnswer<{ paneId: string | null; started: boolean }>> {
  options = at(home, options);
  if (dev.command === null) {
    const seen = await snapshotOf(options);

    if (!seen.ok) {
      return seen;
    }

    const places = new Set(homeCandidates(seen.result, home));

    for (const pane of seen.result.panes ?? []) {
      if (pane.pane_id !== undefined && places.has(pane.workspace_id ?? "") && runOfPane(pane) === null) {
        if (devWorkerOf(await exportedCommand(pane.pane_id, options)) === home.agent) {
          await herdrCall("pane.close", { pane_id: pane.pane_id }, options);
        }
      }
    }

    return { ok: true, result: { paneId: null, started: false } };
  }

  const standing = await standingWorkspace(home, options);

  if (!standing.ok) {
    return standing;
  }

  const seen = await snapshotOf(options);

  if (!seen.ok) {
    return seen;
  }

  const wanted = [...dev.command];
  let kept: string | null = null;

  for (const pane of seen.result.panes ?? []) {
    if (pane.pane_id === undefined || pane.workspace_id !== standing.result.workspaceId || runOfPane(pane) !== null) {
      continue;
    }

    const command = await exportedCommand(pane.pane_id, options);

    if (devWorkerOf(command) !== home.agent) {
      continue;
    }

    if (kept === null && command.length === wanted.length && command.every((part, at) => part === wanted[at])) {
      kept = pane.pane_id;
    } else {
      await herdrCall("pane.close", { pane_id: pane.pane_id }, options);
    }
  }

  if (kept !== null) {
    return { ok: true, result: { paneId: kept, started: false } };
  }

  const applied = await herdrCall<LayoutApplied>(
    "layout.apply",
    {
      workspace_id: standing.result.workspaceId,
      tab_label: DEV_TAB_LABEL,
      root: { type: "pane", label: DEV_TAB_LABEL, command: wanted, cwd: dev.cwd, env: dev.env },
    },
    options,
  );
  const paneId = applied.ok ? applied.result.layout?.root?.pane_id : undefined;

  if (paneId === undefined) {
    return applied.ok ? { ok: false, error: { code: "unreadable", message: "layout.apply named no dev-server pane" } } : applied;
  }

  return { ok: true, result: { paneId, started: true } };
}

// Whose dev server a command line is: the argument after the verb, or nothing for any other command.
function devWorkerOf(command: readonly string[]): string | null {
  const at = command.findIndex((part) => part === DEV_VERB);

  return at < 0 ? null : (command[at + 1] ?? null);
}

// One standing tab an owner wants in an agent's workspace: its label and the command line it runs,
// which carries the standing mark (`standingCommand`).
export interface StandingTab {
  label: string;
  command: readonly string[];
  cwd: string;
  env: Record<string, string>;
}

/**
 * An agent's standing tabs of one owner brought to what the owner wants: every pane of the workspace
 * whose command line the owner claims stays where it runs a line wanted and is closed where it does
 * not, an older line or one no longer wanted, and each wanted line no pane runs is opened in a tab of
 * its own. Of two panes running one line the first the snapshot lists is kept. Nothing is focused.
 */
export async function standingTabs(
  home: AgentHome,
  wanted: readonly StandingTab[],
  claims: (command: readonly string[]) => boolean,
  options: Where = {},
): Promise<HerdrAnswer<{ opened: number; closed: number }>> {
  options = at(home, options);
  const standing = await standingWorkspace(home, options);

  if (!standing.ok) return standing;

  const seen = await snapshotOf(options);

  if (!seen.ok) return seen;

  const same = (one: readonly string[], other: readonly string[]): boolean => one.length === other.length && one.every((part, at) => part === other[at]);
  const kept = new Set<number>();
  const closing: string[] = [];

  for (const pane of seen.result.panes ?? []) {
    if (pane.pane_id === undefined || pane.workspace_id !== standing.result.workspaceId || runOfPane(pane) !== null) continue;

    const command = await exportedCommand(pane.pane_id, options);

    if (!claims(command)) continue;

    const at = wanted.findIndex((tab, index) => !kept.has(index) && same(tab.command, command));

    if (at < 0) closing.push(pane.pane_id);
    else kept.add(at);
  }

  await closePanes(closing, options);

  let opened = 0;

  for (const [index, tab] of wanted.entries()) {
    if (kept.has(index)) continue;

    const applied = await herdrCall<LayoutApplied>(
      "layout.apply",
      {
        workspace_id: standing.result.workspaceId,
        tab_label: shortTitle(tab.label),
        root: { type: "pane", label: shortTitle(tab.label), command: [...tab.command], cwd: tab.cwd, env: tab.env },
      },
      options,
    );

    if (!applied.ok) return applied;

    opened += 1;
  }

  return { ok: true, result: { opened, closed: closing.length } };
}

async function snapshotOf(options: Where): Promise<HerdrAnswer<HerdrSnapshot>> {
  const answer = await herdrCall<{ snapshot?: HerdrSnapshot }>("session.snapshot", {}, options);

  if (!answer.ok) {
    return answer;
  }

  if (answer.result.snapshot === undefined) {
    return { ok: false, error: { code: "unreadable", message: "session.snapshot carried no snapshot" } };
  }

  return { ok: true, result: answer.result.snapshot };
}

// Ending a pane whose id is already in hand, which is the case for whoever opened it. A pane herdr
// no longer has is not a failure to report: a run whose own command ended took its pane with it,
// so this is asked after the ordinary ending too. The pane and never its workspace, because a run
// beside a standing agent's head shares that workspace with the head and with other runs.
export async function closePane(paneId: string, options: { socket?: string } = {}): Promise<boolean> {
  return (await herdrCall("pane.close", { pane_id: paneId }, options)).ok;
}

// Which pane herdr is holding a run in. Nothing stores this — the token is the name — so it is a
// read of the tokens and the id is only ever used immediately.
async function paneOf(name: string, options: { socket?: string } = {}): Promise<string | null> {
  const answer = await snapshotOf(options);

  if (!answer.ok) {
    return null;
  }

  return (answer.result.panes ?? []).find((pane) => runOfPane(pane) === name)?.pane_id ?? null;
}

// What a run is told when it is being stopped from outside, as the shared policy declares it
// (docs/agent-control.md, 「Stopping」).
export const stopInstruction: string = declaredStop.instruction;

// How a run in a pane was ended.
//
// `ended` is the one worth having: it was asked, and it finished itself. `closed` is the pane taken
// away at once because the run could not be asked. `absent` is a name herdr is not holding at all.
export type RunStop = "ended" | "closed" | "absent";

// Ending a run in a pane by asking.
//
// The words go in and the run is waited on until it has reached its own ending, however long that
// takes; nothing takes the pane away under a run that was told. A run that cannot be asked — one
// already waiting on a person — is closed at once, because nothing said to it would arrive.
export async function stopRun(
  name: string,
  options: { everyMs?: number; socket?: string } = {},
): Promise<RunStop> {
  const paneId = await paneOf(name, options);

  if (paneId === null) {
    return "absent";
  }

  await guardPanePrompt();
  const asked = await herdrCall("agent.prompt", { target: paneId, text: stopInstruction }, options);

  // Only a run that was actually told something is waited for. A refusal — a run already waiting
  // on a person, most of all, which leaves the pane byte-identical — is a run that was told
  // nothing, so there is nothing to wait for and the pane goes now.
  if (asked.ok) {
    await waitForRun(name, { everyMs: options.everyMs, socket: options.socket });

    return "ended";
  }

  await closePane(paneId, options);

  return "closed";
}

// Ending a run from outside, which kills what is in the pane. Measured: the process in the pane
// is sent a hangup and is gone once the pane is closed. This is the hard stop, the same place
// `systemctl stop` stood in, and not the asking-it-to-finish kind.
export async function closeRun(name: string, options: { socket?: string } = {}): Promise<boolean> {
  const paneId = await paneOf(name, options);

  if (paneId === null) {
    return false;
  }

  return await closePane(paneId, options);
}

// Waiting for a run in a pane to finish, until herdr no longer holds it.
export async function waitForRun(
  name: string,
  options: { everyMs?: number; socket?: string } = {},
): Promise<void> {
  const everyMs = options.everyMs ?? 2_000;

  for (;;) {
    const answer = await snapshotOf(options);

    // A herdr that cannot be asked is not one holding the run, so the wait is over — there is
    // nothing left for it to be waiting on.
    if (!answer.ok || !liveRuns(answer.result, [name]).has(name)) {
      return;
    }

    await Bun.sleep(everyMs);
  }
}
