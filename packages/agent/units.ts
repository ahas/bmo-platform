import { executionCancelled } from "./execution-identity";
import { providerSettingsArchive } from "./provider-order";
// Where everything that runs apart lives: how one is started, whether one is still alive, and how
// one is ended from outside. Everything the bot launches outside its own cgroup — a runner, the job
// worker — gets a place apart, and a run killed from outside never gets to
// write its own end, so what the bot wrote down says what a run is and its supervisor says whether
// it is still going on.
//
// New runs require herdr: an agent's host, or for an interactive turn a pane of its own. An
// unavailable herdr refuses launch.
// Legacy unit liveness and stop operations remain for executions started before migration.
// This is still the one place that speaks to systemd; what it is no longer is the only place a
// run can be.

import { existsSync, readFileSync, unlinkSync } from "node:fs";
import { hostname } from "node:os";
import { dirname, join } from "node:path";

import { ownRunName, RUN_NAME_VARIABLE } from "./herdr";
import { launchInHost } from "./agent-process";
import { executionsRoot, hostCodeRoot } from "./agent-host";
import { HEAD_MARK, HEAD_STOPPED, type AgentHome } from "./herdr-run";
import { JOB_RUNNER_MARKER } from "./live-claude-guard";
import { nodeHeadEnv, nodeHerdrSocket, nodeRootsOf } from "./work-node";
import { processAlive } from "@agency/archive/process-mark";
import type { RunProcess } from "@agency/archive/run-store";
import { dataRoot, dataRootCopy } from "@agency/support/roots";
import { runPath } from "@agency/support/service-path";
import { userBus } from "@agency/support/user-bus";

import type { Archive } from "@agency/archive/archive";

// What this very run is called. That matters where the same script is launched under more than one
// name — the hourly digest and a digest asked for one room are the same file under different names
// — and a row that named the wrong one would be settled against a run that is not this one.
//
// Asked of the environment first, because that answer holds wherever the run is: a pane has no
// cgroup naming it, so the launcher hands the name in, and it hands it in on the unit path too so
// that there is one answer rather than two. The cgroup is read after, for a run that was started
// without being told — a unit somebody asked for by hand, or one that outlived this change.
//
// A run with neither has no name to give. A run like that has nothing for anybody to ask about,
// so what it gets back is nothing.
export function ownUnit(): string | null {
  const named = ownRunName();

  if (named !== null) {
    return named;
  }

  try {
    for (const line of readFileSync("/proc/self/cgroup", "utf8").split("\n")) {
      const found = /\/([^/]+)\.service$/.exec(line.trim());

      if (found?.[1] !== undefined) {
        return found[1];
      }
    }
  } catch {
    // Nothing to read means no unit to name, which is what a machine without this file is
    // saying about the process too.
  }

  return null;
}

// Whether an open row's run is still there. A unit's name is held by whichever launch of it stands
// now, so a row naming its process on this machine is answered by that process alone; a row naming
// none, or a process on another machine, by whatever holds its unit.
export function runAlive(run: { unit: string; process: RunProcess | null }, unitHeld: (unit: string) => boolean): boolean {
  if (run.process !== null && run.process.host === hostname()) {
    return processAlive({ pid: run.process.pid, start: run.process.start });
  }

  return unitHeld(run.unit);
}

export function isUnitRunning(unit: string): boolean {
  try {
    const probe = Bun.spawnSync(["systemctl", "--user", "is-active", "--quiet", `${unit}.service`], { env: userBus() });

    return probe.exitCode === 0;
  } catch {
    return false;
  }
}

// How long one answer stands before systemd is asked again. Asking is a synchronous spawn on
// the caller's own loop, and a caller may ask both on a clock of its own and every time a gate
// starts or ends, so a busy round would otherwise ask a dozen times over the seconds one search takes.
// Nothing can go stale inside the window: a run that ended closed its own row and is no longer
// among the units anybody asks about.
const answerTtlMs = 2_000;

let remembered: { at: number; asked: string; alive: Set<string> } | null = null;

// A second supervisor's word, where one is standing. systemd is not the only thing that can be
// holding a run up: a run that lives in a pane has no unit at all, and what holds it is whatever
// owns that pane. So the question goes to both and the answers are joined — a run is alive if
// either of them is holding it.
//
// The join is the safe direction, and which direction is safe is not symmetric. A name neither
// supervisor holds is a run that is over, which is what the record needs to hear so the row it
// left open gets closed. A name only one of them holds is still running, and calling it dead
// would close a row out from under a run that is still working. So this only ever adds, and a
// machine with nothing else standing gets exactly the answer systemd gave.
//
// Installed rather than reached for, because what answers it is a picture kept up from a stream of
// events and only whoever is holding that stream open has one. Starting a run and closing one are
// calls anyone can make and are reached for directly; knowing what is alive without waiting is
// not, and a caller with no picture gets systemd's answer alone.
let secondSupervisor: (() => ReadonlySet<string>) | null = null;

export function installSecondSupervisor(held: (() => ReadonlySet<string>) | null): void {
  secondSupervisor = held;
}

// The same question for a whole list in one call: is-active prints one line per unit in the
// order it was given, so the answers line up with what was asked. The exit code says only that
// something in the list is not running, which is not the question, so the lines are what is
// read. Nothing to ask about is not asked at all. Where systemd gave no answer, what it is taken to
// have said is that nothing is running.
export async function activeUnits(units: string[]): Promise<Set<string>> {
  if (units.length === 0) {
    return new Set();
  }

  return await joinHeld(units, activeSystemdUnits(units) ?? new Set());
}

// The same answer, or null where systemd gave none — for a caller to whom "nothing is running" and
// "nobody could say" call for different things, as the sweep of dead jobs does: it closes a row whose
// runner is gone on the first and leaves every row standing on the second.
export async function answeredUnits(units: string[]): Promise<Set<string> | null> {
  if (units.length === 0) {
    return new Set();
  }

  const bySystemd = activeSystemdUnits(units);

  return bySystemd === null ? null : await joinHeld(units, bySystemd);
}

// systemd's answer with whatever else is holding a run joined to it.
async function joinHeld(units: string[], systemd: ReadonlySet<string>): Promise<Set<string>> {
  const bySystemd = new Set(systemd);
  const archive = providerSettingsArchive();
  // Unresolved physical work retains its business owner across a lost herdr connection.
  // Absence from systemd is not permission to release or automatically replay that work. The
  // clause holds only while the row is open, so what makes this cheap and what makes it correct
  // are the same thing: rows close as the evidence reaches them, and only the open ones are read.
  for (const row of await archive?.executions().openExecutions() ?? []) {
    if (row.sharedPane) continue;
    // A run in a host whose host has published how it ended is over, whether or not reconciliation
    // has written that down yet.
    if (row.line === "coordinator" && existsSync(join(executionsRoot(), row.id, "outcome.json"))) continue;
    for (const owner of [row.parent, row.root]) {
      if (!owner || !units.includes(owner)) continue;
      // A row a cancellation covers says nothing about its owner being alive. The cancellation is
      // the statement that it must not be, and a row still open under one is a row reconciliation
      // has not reached yet — read as life, it would hold that name busy for as long as the
      // reconciler took, which is the whole stretch in which nothing may be named in its place.
      if (await executionCancelled(archive?.settings(), [owner, row.id])) continue;
      bySystemd.add(owner);
    }
  }
  const held = secondSupervisor?.();

  if (held === undefined) {
    return bySystemd;
  }

  // Copied rather than added to, because the systemd answer above is remembered for a couple of
  // seconds and the next caller must get what systemd said rather than what this call joined.
  const alive = new Set(bySystemd);

  for (const unit of units) {
    if (held.has(unit)) {
      alive.add(unit);
    }
  }

  return alive;
}

// Null where systemd could not be asked or gave fewer lines than it was asked about — a bus it could
// not reach says so on its error stream and prints nothing — and such a non-answer is not remembered.
function activeSystemdUnits(units: string[]): Set<string> | null {
  const asked = units.join(" ");

  if (remembered !== null && remembered.asked === asked && Date.now() - remembered.at < answerTtlMs) {
    return remembered.alive;
  }

  try {
    const probe = Bun.spawnSync([
      "systemctl",
      "--user",
      "is-active",
      ...units.map((unit) => `${unit}.service`),
    ], { env: userBus() });
    const lines = probe.stdout.toString().split("\n").map((line) => line.trim()).filter((line) => line !== "");

    if (lines.length < units.length) {
      return null;
    }

    const alive = new Set(units.filter((_, index) => lines[index] === "active"));

    remembered = { at: Date.now(), asked, alive };

    return alive;
  } catch {
    return null;
  }
}

// What a run apart is started with. It inherits nothing from whoever asked for it, in a unit or in
// a pane, so what it needs has to be named: PATH, because the runner inside spawns the CLI and
// bun; every BMO_ variable the launcher itself is carrying; and HOME, because a runner of the bmo
// binary finds its data root, its trees and its logins under it, and the agent host starts every
// run on an emptied environment. The variables matter because they are how a run is pointed
// somewhere other than the real thing — a copy of the archive, a stand-in token — and a run that
// quietly ignored them would do its work against the machine's own data while whoever asked
// believed it was working on the copy. The service sets none of them, so a run started by the bot
// is launched exactly as it always is.
//
// The job runner's marker is the one BMO_ variable left behind. A run apart is the bot's own tool
// run even when a job asked for it, so it is not a job's process (`docs/live-claude-guard.md`), and
// a run carrying that marker would have every Claude line inside it — a search's pooled branch among
// them — refused as a job opening a session.
//
// The run's own name goes in last and wins, whatever the launcher was carrying. A launcher that is
// itself a run carries its own name, and a child that inherited it would report itself as its
// parent — which is exactly the case a room's own person-book worker is, launched by a run of the
// same script.
export function runEnvironment(env: Record<string, string | undefined>, name: string): Record<string, string> {
  return env.HOME === undefined ? namedEnvironment(env, name) : { ...namedEnvironment(env, name), HOME: env.HOME };
}

// The search path, every BMO_ variable but the job runner's marker, and the run's own name: what
// a run apart and a pane running a line's own command are both handed. A pane is left without the
// home, which herdr lays on from its own environment, so the program standing in it is the same
// program whoever composed it.
function namedEnvironment(env: Record<string, string | undefined>, name: string): Record<string, string> {
  const carried: Record<string, string> = { PATH: runPath(env.PATH ?? "") };

  for (const variable of Object.keys(env).sort()) {
    const value = env[variable];

    if (variable.startsWith("BMO_") && variable !== JOB_RUNNER_MARKER && value !== undefined) {
      carried[variable] = value;
    }
  }

  carried[RUN_NAME_VARIABLE] = name;

  return carried;
}

// What a pane that runs a harness's own command is handed: what any run apart is handed but the
// home, and the variables the line itself sets, named by whoever owns the line.
//
// A runner in a pane starts the CLI for itself and builds the line's environment in its own
// process. A job's CLI is the pane's own process, so nothing else is there to set what the line
// sets — the git status it keeps out of the prompt, most of all — and those have to arrive with
// the pane. They are handed over by name rather than worked out: the launcher's own environment is
// exactly what must not be passed on whole, since a launcher inside a Claude session carries that
// session's markers and a CLI that inherits them stops saving its transcript — which is the record
// a run in a pane is read from. Nor can they be told apart by comparing the two environments,
// because a launcher already carrying a setting would then carry nothing.
//
// The job runner's marker goes with the pane where the launcher holds it. A job's CLI is the
// runner's own work rather than a run apart, and the marker is what makes the guard in front of a
// job's shell and in the bot's launch code know it for one (`docs/live-claude-guard.md`); a
// keepalive launched by the bot carries none, so its pane carries none either.
export function lineEnvironment(
  env: Record<string, string | undefined>,
  own: Readonly<Record<string, string>>,
  name: string,
): Record<string, string> {
  const marker = env[JOB_RUNNER_MARKER];
  const job: Record<string, string> = marker === undefined || marker === "" ? {} : { [JOB_RUNNER_MARKER]: marker };

  return { ...namedEnvironment(env, name), ...job, ...own, [RUN_NAME_VARIABLE]: name };
}

// Where a supervisor or a repository's checkout stands in herdr: its workspace under the name the owner has
// herdr show it by, or else the name the owner calls it, whose head pane is its host. The rows are asked
// rather than a list kept here, so one the owner adds stands in herdr with nothing changed in the code,
// and a name the owner gives it is the name herdr shows. A name no row stands under has no home here; an
// interactive turn under such a name opens in a workspace of its own.
export async function agentHome(archive: Archive | null, agent: string): Promise<AgentHome | null> {
  const record = await archive?.agents().getAgent(agent) ?? null;

  if (record === null) return null;

  const home = homeOf(record.name, record.herdrLabel ?? record.label ?? record.name);

  return record.node === null || archive === null ? home : await nodeHome(archive, home, record.node);
}

// A home on a work node (docs/work-nodes.md, 「The bot's side of a remote agent」): in the node's herdr
// session, reached on its forwarded socket, its head the node's own `pane-head` asking the node's agent
// host, started on the node's roots and nothing of this machine's environment.
async function nodeHome(archive: Archive, home: AgentHome, node: string): Promise<AgentHome> {
  const workplace = await archive.workNodes().workplace(node);
  const roots = workplace === null ? null : nodeRootsOf(workplace);
  const socket = nodeHerdrSocket(node);

  if (roots === null) return { ...home, socket };

  const code = join(roots.root, "deploy");

  return { ...home, socket, head: { command: headCommand(home.agent, code), cwd: join(roots.root, "data"), env: nodeHeadEnv(roots, `${HEAD_MARK}${home.agent}`) } };
}

// The home a host is started in. A name with no row — a co-work role, message sending — still has
// one host, in a workspace labelled with that name.
export async function hostHome(archive: Archive | null, agent: string): Promise<AgentHome> {
  return await agentHome(archive, agent) ?? homeOf(agent, agent);
}

// What a head pane runs: the head program under a loop that starts it again wherever it ends. The
// pane is the agent's one standing pane, and the program in it is not — the head becomes the agent's
// program or a follower of its view, or a deployment retires it — so the loop is what keeps the place
// while what stands behind it is replaced. The deploy path is fixed and its contents are what move,
// so the same line starts whichever revision stands there now. `HEAD_STOPPED` is the one exit it obeys.
//
// The head is `pane-head` of the deploy tree's `bin/`. It runs under the mark `bmo-head-<agent>`, as
// the shell's own name, which is how a head pane is told apart from everything else in the workspace.
export function headCommand(agent: string, root: string = hostCodeRoot()): string[] {
  return [
    "/bin/sh",
    "-c",
    `while :; do "$1" "$2"; case $? in ${HEAD_STOPPED}) exit 0;; esac; sleep 1; done`,
    `${HEAD_MARK}${agent}`,
    join(root, "bin", "pane-head"),
    agent,
  ];
}

// A home of a data root other than the bot's own is that root's, labelled with its name, so the two
// never share a workspace (docs/agent-control.md, 「What goes where」).
export function homeOf(agent: string, label: string): AgentHome {
  const copy = dataRootCopy();

  return {
    agent,
    label: copy === null ? label : `${label} · copy ${copy.name}`,
    ...(copy === null ? {} : { data: copy.root }),
    head: { command: headCommand(agent, hostCodeRoot()), cwd: dataRoot(), env: runEnvironment(process.env, `${HEAD_MARK}${agent}`) },
  };
}

// The same thing said the way systemd-run takes it.
export function unitEnvironment(env: Record<string, string | undefined>, name: string): string[] {
  return Object.entries(runEnvironment(env, name)).map(([variable, value]) => `--setenv=${variable}=${value}`);
}

// The utility entrypoints that may stand in a transient unit of their own where their host refuses
// them: a binary the deployment placed in the tree's own `bin/` — the `bmo` binary's voice, say and
// mp3 runners among them — never a command from anywhere else.
function isUtilityRun(cwd: string, command: readonly string[]): boolean {
  const [program = ""] = command;

  return dirname(program) === join(cwd, "bin");
}

// Starting one tool that runs apart. A search, a browse and a briefing are the same launch three
// times over: the row for the run is opened first, so the run is in the record from the moment it
// exists, and then a place to run is asked for and the runner inside it closes that row with how
// it ended. What they share and what this is here for is the other ending — the place that never
// came up at all, where there is no runner to close anything.
//
// Included work goes into its agent's host. Only the explicitly listed utility entrypoints may
// use a transient unit after refusal. Each runner reports its own eventual outcome.
//
// A launch that never started answers false, and the caller closes its own record of the run then,
// where what happened is known: nothing ever ran, so the ending is the one a run that never reached
// its work takes, and never the one that means the agent answered and its answer was a failure.

export async function launchRunUnit(options: {
  archive: Archive;
  unit: string;
  // What the run is, in the words a person watching herdr reads.
  title: string;
  // The agent the run belongs to, which settles where in herdr it opens.
  agent: string;
  // Where the run works, which is this repository for every runner started here.
  cwd: string;
  // What runs inside it.
  command: readonly string[];
  // What the launcher wrote for the run to read: a task, a query, the words of a briefing. A
  // launch that never started will never read them, and some of them carry somebody's own words,
  // so they go with the failed launch rather than being left in the temporary directory.
  scratch?: readonly string[];
  utilityFallback?: boolean;
}): Promise<boolean> {
  let reason: string;
  try {
    const placed = await launchInHost(options.archive, {
      name: options.unit,
      title: options.title,
      agent: options.agent,
      cwd: options.cwd,
      command: options.command,
      env: runEnvironment(process.env, options.unit),
    });
    if (placed.ok) return true;
    reason = placed.error.code;
  } catch (error) {
    reason = String(error);
  }

  if (options.utilityFallback && isUtilityRun(options.cwd, options.command)) {
    try {
      const process = Bun.spawn(["systemd-run", "--user", "--quiet", "--collect", `--unit=${options.unit}`,
        ...unitEnvironment(globalThis.process.env, options.unit), `--working-directory=${options.cwd}`, ...options.command],
        { env: userBus(), stdout: "inherit", stderr: "inherit" });
      if (await process.exited === 0) return true;
    } catch (error) { reason += `; utility launch: ${String(error)}`; }
  }

  for (const path of options.scratch ?? []) {
    try {
      unlinkSync(path);
    } catch {
      console.error(`failed to delete ${path} after a failed launch`);
    }
  }

  console.error(`failed to launch ${options.unit}: herdr refused (${reason})`);

  return false;
}
