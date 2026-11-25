import { requireStoreAddress } from "@agency/archive/archive-place";
import queueWords from "@agency/policy/queue-words.toml";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { NativeExecutions } from "./native-execution";
import { deploymentNames } from "@agency/support/names";
import { beginExecution, executionCancelled, executionRoot, type ExecutionIdentity } from "./execution-identity";
import { ExecutionStopped, launchDetachedExecution, runPaneProcess, PaneTransportError, wasStopped, type Placer } from "./pane-process";
import { executionsRoot } from "./agent-host";
import { guardClaudeLaunch } from "./live-claude-guard";
import { hostHome, ownUnit } from "./units";
import { configuredProviderEnabled, providerSettingsArchive } from "./provider-order";
import { provider as providerNamed } from "./agent-providers";
import { Archive } from "@agency/archive/archive";
import { MCP_TOKEN_VARIABLE, revokeToolToken, toolTokenFor } from "./tool-host";
import { appendView, rootHostPlacer } from "./root-host";
import { OutputFollower, endedLine } from "./agent-pane";
import type { HerdrAnswer } from "./herdr";
import { agentProviderNames } from "./harness";
import type { HarnessCommand, RunLine, AgentProviderName } from "./harness";

// Who runs an execution. Every caller names the agent whose host takes it; a caller that names
// none is taken to be the agent its work key is named after, which is true of every key that is an
// agent's own name.
export interface AgentExecution extends ExecutionIdentity {
  agent?: string;
}

let placer: Placer | undefined;
let opened: Archive | null = null;

// The archive execution records go to: the one this process installed, or else the machine's own,
// opened the first time a process that installed none launches a run.
function executionArchive(): Archive {
  const installed = providerSettingsArchive();
  if (installed) return installed;
  opened ??= new Archive(requireStoreAddress());
  return opened;
}

// A test's stand-in for the host; nothing else installs one.
export function installPlacer(stand: Placer | null): void {
  placer = stand ?? undefined;
}

// Where a line's executions go: a test's stand-in where one is installed, the data root's agent host
// otherwise.
function placerFor(): Placer {
  return placer ?? rootHostPlacer();
}

// How long a structured run's session record may stand if its collector never takes it back.
const recordBackstopMs = 24 * 3_600_000;

// The one launch for every harness command, whatever the line and whatever the provider: the
// command runs as a child of its agent's host, and the caller reads the same streams it would read
// from a process of its own. Placement does not change harness parsing.
export async function spawnAgentProcess(command: HarnessCommand, line: RunLine, provider: AgentProviderName, identity: AgentExecution = {}) {
  identity = { root: executionRoot(ownUnit()), parent: ownUnit(), ...identity };
  const agent = identity.agent ?? line;
  const check = async () => {
    const owner = ownUnit();
    // A launch refused because a cancellation stands over its name is that stop reaching this run:
    // the name must not run, so this is a stop and not the transport breaking, and nobody may read
    // it as a judgement that could be asked of something else (docs/agent-control.md 「Stopping」).
    if (await executionCancelled(providerSettingsArchive()?.settings(), [record.id, owner, identity.root])) throw new ExecutionStopped("execution cancelled");
    if (!await configuredProviderEnabled(provider)) throw new Error(`${provider}: ${queueWords.pool.off}`);
    await guardClaudeLaunch(line, command.args, command.env);
  };
  let archive: Archive;
  try { archive = executionArchive(); } catch (error) { throw new PaneTransportError(`execution archive unavailable: ${String(error)}`); }
  const record = await beginExecution(archive, provider, line, identity);
  try { await check(); } catch (error) {
    // A launch the standing cancellation refused ends as a stop: the row says cancelled, because
    // nothing here broke — the name was one that must not run.
    record.state = wasStopped(error) ? "cancelled" : "launch-error"; record.endedAt = Date.now(); await archive.executions().saveExecution(record);
    throw error;
  }
  const native = new NativeExecutions(archive, record);
  const decoder = new TextDecoder();
  let pending = "";
  const observe = async (bytes: Uint8Array) => {
    pending += decoder.decode(bytes, { stream: true });
    const lines = pending.split("\n"); pending = lines.pop() ?? "";
    for (const line of lines) {
      let event: any;
      try { event = JSON.parse(line); } catch { continue; }
      // Which session the run is answering in is read out of its own program's words by that
      // program's own reader, never here.
      const session = providerNamed(provider).adapter.sessionFromEvent?.(event) ?? null;
      if (session !== null && session !== record.sessionId) { record.sessionId = session; await archive.executions().saveExecution(record); }
      await native.observe(event);
    }
  };
  const controller = new AbortController();
  let callerStopped = false;
  let out!: ReadableStreamDefaultController<Uint8Array>;
  let err!: ReadableStreamDefaultController<Uint8Array>;
  const stdout = new ReadableStream<Uint8Array>({ start(c) { out = c; } });
  const stderr = new ReadableStream<Uint8Array>({ start(c) { err = c; } });
  const title = `${line} · ${provider}${record.model ? ` · ${record.model}` : ""} · attempt ${record.attempt}`;
  // The collector writes what the run says into its agent's view as its provider reads it, and how it
  // ended after its last read, so the host never reads a provider's output (docs/agent-control.md,
  // 「The view」).
  const adapter = providerNamed(provider).adapter;
  const follower = new OutputFollower(() => adapter.progressOf?.bind(adapter));
  const watched = { name: record.id, directory: join(executionsRoot(), record.id), provider };
  const view = (lines: readonly string[]) => appendView(agent, lines);
  const exited = (async () => {
    // Issued before anything is placed: a tool host that does not answer refuses the run here.
    const token = await toolTokenFor(line, { ...command, agent }, { deadline: Date.now() + recordBackstopMs });

    try {
      return await runPaneProcess({
        policy: { address: archive.path, provider, names: [record.id, record.root, record.parent].filter((name): name is string => !!name), providers: agentProviderNames },
        command: token === null ? command : { ...command, env: { ...command.env, [MCP_TOKEN_VARIABLE]: token } },
        root: executionsRoot(), name: record.id, home: await hostHome(archive, agent), place: placerFor(),
        title, beforeGrant: check,
        onOutcome: async result => {
          record.state = result.cause; record.exitCode = result.exitCode; record.endedAt = Date.now(); await archive.executions().saveExecution(record);
          view([...follower.look(watched, true), endedLine({ name: record.id, title, at: Date.now(), cause: result.cause, exitCode: result.exitCode })]);
        },
        onStarted: async () => { record.state = "running"; await archive.executions().saveExecution(record); },
        onPane: async paneId => { record.paneId = paneId; await archive.executions().saveExecution(record); }, signal: controller.signal,
        onOutput: async (stream, bytes) => {
          if (stream === "stdout") await observe(bytes);
          (stream === "stdout" ? out : err).enqueue(bytes);
          if (stream === "stdout") view(follower.look(watched));
        },
      });
    } finally {
      if (token !== null) void revokeToolToken(token);
    }
  })().then(async result => {
    record.state = result.cause; record.exitCode = result.exitCode; record.endedAt = Date.now(); await archive.executions().saveExecution(record);
    // A cancellation the caller never asked for is somebody stopping this run, and it ends as a
    // stop rather than as the transport breaking under it.
    if (result.cause === "cancelled" && !callerStopped) throw new ExecutionStopped(`execution ${result.cause}`);
    if (result.cause === "launch-error") throw new PaneTransportError(`execution ${result.cause}`);
    return result.cause === "exited" ? result.exitCode : (result.exitCode || 1);
  }, async error => {
    // A launch the standing cancellation refused, or a collector that met the stop on its way out:
    // the row is closed as cancelled and the stop is raised as a stop, so no caller reads it as the
    // transport breaking and asks somebody else instead.
    const stopped = wasStopped(error);
    record.state = stopped ? "cancelled" : record.state === "launching" ? "launch-error" : "unknown";
    if (stopped || record.state === "launch-error") record.endedAt = Date.now();
    await archive.executions().saveExecution(record);
    err.enqueue(new TextEncoder().encode(String(error)));
    throw stopped ? new ExecutionStopped(String(error)) : new PaneTransportError(String(error));
  }).finally(async () => {
    if (pending.trim()) { try { await native.observe(JSON.parse(pending)); } catch {} }
    await native.finish();
    out.close(); err.close();
  });
  void exited.catch(() => {}); // Callers may drain streams before awaiting the exit.
  return { stdout, stderr, exited, kill: () => { callerStopped = true; controller.abort(); } };
}

// A run started and left in its agent's host — a runner's coordinator, a briefing. Its execution record's root is the run's own name, which is what makes the name read as
// alive while it runs and what a stop of that name reaches.
export async function launchInHost(archive: Archive, run: {
  name: string;
  title: string;
  agent: string;
  cwd: string;
  command: readonly string[];
  env: Record<string, string>;
}): Promise<HerdrAnswer<unknown>> {
  const record = await beginExecution(archive, deploymentNames().agentKey, "coordinator", { agent: run.agent, root: run.name, parent: null, sessionId: null });
  const finish = async (state: "launch-error", reason: string) => {
    record.state = state; record.endedAt = Date.now(); record.exitCode = 1; record.reconciliationEvidence = reason; await archive.executions().saveExecution(record);
  };
  try {
    const placed = await launchDetachedExecution({
      command: { args: [...run.command], cwd: run.cwd, env: run.env, stdin: null },
      root: executionsRoot(), name: record.id, title: run.title, home: await hostHome(archive, run.agent), place: placerFor(),
    });
    if (!placed.ok) { await finish("launch-error", placed.error.code); return placed; }
    record.paneId = placed.result.paneId; record.state = "running"; await archive.executions().saveExecution(record);
    return placed;
  } catch (error) {
    await finish("launch-error", String(error));
    return { ok: false, error: { code: "launch-error", message: String(error) } };
  }
}

// Whether a run started in a host is still going: its record is open and its host has not yet
// published how it ended. The outcome is read here rather than waited for from reconciliation, so a
// script waiting on a run it started learns of the ending as soon as the host writes it.
export async function hostedRunning(archive: Archive, name: string): Promise<boolean> {
  return (await archive.executions().openExecutions()).some(row => row.root === name && row.line === "coordinator" && !existsSync(join(executionsRoot(), row.id, "outcome.json")));
}

