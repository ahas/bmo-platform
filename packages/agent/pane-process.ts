import { openArchiveSql } from "@agency/archive/archive-place";
import type { AgentProviderName } from "./harness";
import { executionCancelKey } from "./execution-identity";
import { providerSwitchKey, switchedOn } from "./provider-order";
import { spawn } from "node:child_process";
import { appendFile, mkdir, readFile, writeFile, rename, rm } from "node:fs/promises";
import { createWriteStream, existsSync, readdirSync, readFileSync } from "node:fs";
import { pipeline } from "node:stream/promises";
import { randomUUID } from "node:crypto";
import { join } from "node:path";
import { identityAlive, ownIdentity, processBirth, type Placement, type ProcessIdentity } from "./agent-host";
import type { AgentHome } from "./herdr-run";
import type { HerdrAnswer } from "./herdr";
import type { HarnessCommand, HarnessOutput } from "./harness";

export class PaneTransportError extends Error {
  constructor(message: string) { super(message); this.name = "PaneTransportError"; }
}

/**
 * A run that ended because somebody stopped it, which is not the transport failing
 * (`docs/agent-control.md` 「Stopping」). It is a transport error still, so everything that catches
 * one — the fallback road, the job recovery — goes on treating a stop exactly as it did; what
 * reads the two apart is only what has to tell a stop from a breakdown, and a stop is not carried
 * to another provider and is nobody's failure to report.
 */
export class ExecutionStopped extends PaneTransportError {
  constructor(message: string) { super(message); this.name = "ExecutionStopped"; }
}

/** Whether this is a run somebody stopped, wherever the error was caught. */
export function wasStopped(error: unknown): boolean {
  return error instanceof ExecutionStopped;
}

export interface PaneEnvelope {
  command: HarnessCommand;
  // What the host's pane shows for this execution.
  title?: string;
  // The collector that owns the execution; an execution with none is detached and outlives its
  // launcher.
  owner?: ProcessIdentity;
  // The names under which a standing cancellation refuses the child at launch: its execution's own
  // id and the names that execution runs under.
  // `providers` is the build's, which the agent host's supervisor reads the stored order against;
  // `supervisePane` knows them already.
  policy?: { address: string; provider: AgentProviderName; names?: string[]; providers?: readonly string[] };
}
export interface PaneOutcome extends HarnessOutput {
  cause: "exited" | "cancelled" | "launch-error";
}
const pause = () => Bun.sleep(25);
async function exists(path: string): Promise<boolean> { return Bun.file(path).exists(); }
async function publish(path: string, value: unknown): Promise<void> {
  await writeFile(`${path}.new`, JSON.stringify(value), { mode: 0o600 });
  await rename(`${path}.new`, path);
}
function readJson(path: string): any {
  try { return JSON.parse(readFileSync(path, "utf8")); } catch { return null; }
}

// Whether any member of a process group is still running. A zombie counts as ended: it runs nothing,
// and reaping it is its new parent's business.
function groupRunning(group: number): boolean {
  try { process.kill(-group, 0); } catch { return false; }
  for (const entry of readdirSync("/proc")) {
    if (!/^\d+$/.test(entry)) continue;
    let fields: string[];
    try { fields = readFileSync(`/proc/${entry}/stat`, "utf8").split(") ").at(-1)!.split(" "); } catch { continue; }
    if (Number(fields[2]) === group && fields[0] !== "Z") return true;
  }
  return false;
}

// A SIGKILL is delivered when the kernel next runs its target, which on a busy machine is a while
// after the signal is sent; what follows the kill waits for the group to be gone.
async function groupEnded(group: number): Promise<void> {
  while (groupRunning(group)) await pause();
}

// One execution, supervised in this process as the `agent-host` crate's supervisor supervises it,
// for a stand-in host that runs what it is given inside a test's own process. Exact bytes go to
// private files. `beforeOutcome` is called once the program has ended and its output is all written,
// before the outcome is published: the one moment the output is whole and the collector, which
// removes the envelope as soon as it reads the outcome, has not come for it yet.
export async function supervisePane(directory: string, options: { stop?: AbortSignal; beforeOutcome?: () => void } = {}): Promise<void> {
  const envelope: PaneEnvelope = JSON.parse(await readFile(join(directory, "command.json"), "utf8"));
  const refuse = async (cause: PaneOutcome["cause"], reason: string) => {
    await writeFile(join(directory, "stdout"), "", { mode: 0o600 });
    await writeFile(join(directory, "stderr"), reason, { mode: 0o600 });
    await publish(join(directory, "outcome.json"), { exitCode: 1, cause });
  };
  const ownerGone = () => envelope.owner !== undefined && processBirth(envelope.owner.pid) !== envelope.owner.birth;
  while (!(await exists(join(directory, "grant")))) {
    if (await exists(join(directory, "cancel")) || options.stop?.aborted || ownerGone()) {
      await refuse("launch-error", "launch cancelled or its collector gone before the grant");
      return;
    }
    await pause();
  }
  if (await exists(join(directory, "cancel")) || options.stop?.aborted) {
    await refuse("cancelled", "cancelled before launch");
    return;
  }
  try {
    if (envelope.policy) {
      const { sql, close } = openArchiveSql(envelope.policy.address, { readonly: true });
      let enabled = false;
      let cancelled = false;
      try {
        const row = await sql.get("SELECT value FROM settings WHERE key = ?", providerSwitchKey(envelope.policy.provider)) as { value: string } | null;
        enabled = switchedOn(row?.value ?? null);
        for (const name of envelope.policy.names ?? []) {
          if (await sql.get("SELECT value FROM settings WHERE key = ?", executionCancelKey(name)) !== null) cancelled = true;
        }
      } finally { await close(); }
      // A name a cancellation stands over must not start, and a launch refused for that reason is
      // the stop reaching this run rather than a launch that broke: it ends as a stop, so nobody
      // reads it as a run whose work could be asked of something else.
      if (cancelled) {
        await refuse("cancelled", "execution cancelled at child launch");
        return;
      }
      if (!enabled) {
        await refuse("launch-error", "provider disabled at child launch");
        return;
      }
    }
    if (existsSync(join(directory, "cancel")) || ownerGone()) throw new Error("launch cancelled or its collector gone");
  } catch (error) {
    await refuse("launch-error", String(error));
    return;
  }
  let cause: PaneOutcome["cause"] = "exited";
  const child = spawn(envelope.command.args[0]!, envelope.command.args.slice(1), {
    cwd: envelope.command.cwd, env: envelope.command.env, detached: true, stdio: "pipe",
  });
  const guardian = child.pid ? spawn(process.execPath, [join(import.meta.dir, "pane-guardian.ts"), String(child.pid)], {
    detached: true, stdio: ["pipe", "ignore", "ignore"],
  }) : null;
  const guarded = guardian ? new Promise<void>(resolve => { guardian.once("exit", () => resolve()); guardian.once("error", () => resolve()); }) : Promise.resolve();
  guardian?.stdin?.on("error", () => {});
  const signalGroup = (signal: NodeJS.Signals) => { if (child.pid) { try { process.kill(-child.pid, signal); } catch {} } };
  // Asked to stop, the group is sent a SIGTERM once and its leader is waited on however long its own
  // ending takes; nothing forces it.
  let stopped = false;
  const stop = () => {
    if (stopped) return;
    stopped = true; cause = "cancelled"; signalGroup("SIGTERM");
  };
  options.stop?.addEventListener("abort", stop);
  const timer = setInterval(() => {
    void exists(join(directory, "cancel")).then(cancelled => { if (cancelled || ownerGone()) stop(); });
  }, 25);
  let outcome: Pick<PaneOutcome, "exitCode" | "cause"> | undefined;
  const copy = (name: string, input: NodeJS.ReadableStream) => pipeline(input, createWriteStream(join(directory, name), { mode: 0o600 }));
  try {
    const exited = new Promise<number>(resolve => {
      child.once("error", () => { cause = "launch-error"; child.stdin.destroy(); child.stdout.destroy(); child.stderr.destroy(); resolve(127); });
      child.once("exit", code => resolve(code ?? 1));
    });
    child.once("spawn", () => { void publish(join(directory, "started.json"), { pid: child.pid, birth: processBirth(child.pid!), supervisor: ownIdentity() }); });
    child.stdin.on("error", () => {});
    child.stdin.end(envelope.command.stdin ?? undefined);
    const streams = Promise.all([copy("stdout", child.stdout), copy("stderr", child.stderr)]);
    if (options.stop?.aborted) stop();
    const exitCode = await exited;
    // The run is over once its leader has ended, so a descendant still standing is an orphan of it and
    // must not keep pipes or model work alive: it is killed at once.
    signalGroup("SIGKILL");
    await streams.catch(error => { if (cause !== "launch-error") throw error; });
    // A program that never started has its streams torn down, on a busy machine before their files
    // were opened; the collector reads both, so both stand, empty where nothing came. The cause is
    // set by the child's error event, which the compiler cannot follow here.
    if ((cause as PaneOutcome["cause"]) === "launch-error") await Promise.all(["stdout", "stderr"].map(name => appendFile(join(directory, name), "", { mode: 0o600 })));
    outcome = { exitCode, cause };
  } finally {
    clearInterval(timer); signalGroup("SIGKILL");
    if (child.pid) await groupEnded(child.pid);
    options.stop?.removeEventListener("abort", stop);
    guardian?.stdin?.end();
    await guarded;
  }
  if (outcome) {
    try { options.beforeOutcome?.(); } catch {}
    await publish(join(directory, "outcome.json"), outcome);
  }
}

// Where an execution is put: the data root's agent host, or a stand-in in a test.
export type Placer = (request: { agent: string; name: string; title: string; directory: string }) => Promise<HerdrAnswer<Placement>>;

// The waits before a launch whose host did not come up tries again. Any other refusal is not tried
// again.
const placeRetryMs = [2_000, 5_000];
const retriedCodes = new Set(["host-unavailable"]);

// One execution put into its agent's host, tried again a bounded number of times
// (docs/agent-control.md, 「The host」). `ready` is called before each placement, so a window that
// runs from placement opens afresh with it.
async function placeRecovering(options: {
  agent: string;
  request: Parameters<Placer>[0];
  place: Placer;
  retryMs?: readonly number[];
  signal?: AbortSignal;
  ready: () => Promise<void>;
}): Promise<HerdrAnswer<Placement>> {
  await options.ready();

  let placed = await options.place(options.request);

  for (const waitMs of options.retryMs ?? placeRetryMs) {
    if (placed.ok || !retriedCodes.has(placed.error.code) || options.signal?.aborted) break;

    console.error(`${options.agent}: ${options.request.name} could not be placed (${placed.error.code}: ${placed.error.message}); trying again in ${waitMs}ms`);
    await Bun.sleep(waitMs);
    await options.ready();
    placed = await options.place(options.request);
  }

  return placed;
}

// The supervisor's liveness, as the launcher can ask it without herdr: the host that acknowledged
// the start once there is one, and before that the host the pointer was left with.
function supervisorState(directory: string, placement: Placement): "started" | "waiting" | "missing" {
  const started = readJson(join(directory, "started.json"));
  if (started) return identityAlive(started.supervisor) ? "started" : "missing";
  return identityAlive(placement.host) ? "waiting" : "missing";
}

export async function runPaneProcess(options: {
  command: HarnessCommand;
  root: string;
  name: string;
  title: string;
  // The agent this runs for.
  home: AgentHome;
  policy?: PaneEnvelope["policy"];
  signal?: AbortSignal;
  beforeGrant: () => void | Promise<void>;
  onPane?: (paneId: string | null) => void | Promise<unknown>;
  onStarted?: () => void | Promise<unknown>;
  onOutcome?: (outcome: PaneOutcome) => void | Promise<void>;
  onOutput?: (stream: "stdout" | "stderr", bytes: Uint8Array) => void | Promise<unknown>;
  place: Placer;
  placeRetryMs?: readonly number[];
}): Promise<PaneOutcome> {
  await mkdir(options.root, { recursive: true, mode: 0o700 });
  const directory = join(options.root, options.name);
  // An exclusive directory claim rejects simultaneous executions with the same identity.
  if (!/^[a-zA-Z0-9_-]+$/.test(options.name)) throw new Error("invalid execution identity");
  await mkdir(directory, { mode: 0o700 });
  let placement: Placement | undefined;
  let collected = false;
  const offsets = { stdout: 0, stderr: 0 };
  const drain = async () => {
    for (const stream of ["stdout", "stderr"] as const) {
      const file = Bun.file(join(directory, stream));
      if (!(await file.exists())) continue;
      const bytes = new Uint8Array(await file.slice(offsets[stream]).arrayBuffer());
      offsets[stream] += bytes.length;
      if (bytes.length) await options.onOutput?.(stream, bytes);
    }
  };
  const cancel = () => { void writeFile(join(directory, "cancel"), "", { mode: 0o600 }).catch(() => {}); };
  try {
    const envelope: PaneEnvelope = { command: options.command, title: options.title, policy: options.policy, owner: ownIdentity() };
    const placed = await placeRecovering({
      agent: options.home.agent,
      request: { agent: options.home.agent, name: options.name, title: options.title, directory },
      place: options.place,
      retryMs: options.placeRetryMs,
      signal: options.signal,
      ready: async () => { await publish(join(directory, "command.json"), envelope); },
    });
    // The code alone does not say which wait ran out; the host's own reason goes with it.
    if (!placed.ok) throw new Error(`herdr launch failed: ${placed.error.code}: ${placed.error.message}`);
    placement = placed.result;
    await options.onPane?.(placement.paneId);
    let aborted: (() => void) | undefined;
    try {
      await Promise.race([
        Promise.resolve().then(options.beforeGrant),
        new Promise<never>((_, reject) => {
          aborted = () => reject(new Error("execution cancelled before launch"));
          options.signal?.addEventListener("abort", aborted, { once: true });
          if (options.signal?.aborted) aborted();
        }),
      ]);
    } finally {
      if (aborted) options.signal?.removeEventListener("abort", aborted);
    }
    if (options.signal?.aborted) throw new Error("execution cancelled before launch");
    options.signal?.addEventListener("abort", cancel);
    await publish(join(directory, "grant"), { nonce: randomUUID() });
    let acknowledged = false;
    let lastLook = 0;
    while (!(await exists(join(directory, "outcome.json")))) {
      if (!acknowledged && await exists(join(directory, "started.json"))) { acknowledged = true; await options.onStarted?.(); }
      // Stopped before any supervisor has the run, nothing is waited for: the cancel file stands for the
      // host that takes the pointer, whose supervisor then starts nothing.
      if (!acknowledged && options.signal?.aborted && !existsSync(join(directory, "supervisor"))) throw new Error("execution cancelled before launch");
      await drain();
      if (Date.now() - lastLook >= 200) {
        lastLook = Date.now();
        const state = supervisorState(directory, placement);
        if (state === "missing" && !(await exists(join(directory, "outcome.json")))) {
          // A host gone before taking the pointer leaves it in the inbox for the next host; a host
          // gone after starting the child took the child with it, and that is never an answer.
          if (acknowledged || existsSync(join(directory, "started.json"))) throw new Error("herdr execution missing");
          const again = await placement.reensure();
          if (!again.ok) throw new Error(`herdr execution unavailable: ${again.error.code}`);
          placement = { ...placement, host: again.result };
        }
      }
      await pause();
    }
    await drain();
    const result = JSON.parse(await readFile(join(directory, "outcome.json"), "utf8"));
    const [stdout, stderr] = await Promise.all([readFile(join(directory, "stdout"), "utf8"), readFile(join(directory, "stderr"), "utf8")]);
    await options.onOutcome?.({ ...result, stdout, stderr });
    collected = true;
    return { ...result, stdout, stderr };
  } finally {
    options.signal?.removeEventListener("abort", cancel);
    if (!collected) {
      await writeFile(join(directory, "cancel"), "", { mode: 0o600 });
      // The supervisor is given until it has published the cancellation or is gone itself.
      if (placement) while (!(await exists(join(directory, "outcome.json"))) && supervisorState(directory, placement) === "started") await pause();
    }
    if (collected || !placement) await rm(directory, { recursive: true, force: true });
  }
}

// A run that is launched and left: granted before it is placed, owned by nobody, collected by
// nobody. What it reports it reports itself, and reconciliation closes its record from the outcome
// its host publishes. The launch waits only until the host has started it, however long that takes,
// and ends early only where its host is gone and cannot stand again.
export async function launchDetachedExecution(options: {
  command: HarnessCommand;
  root: string;
  name: string;
  title: string;
  home: AgentHome;
  place: Placer;
  placeRetryMs?: readonly number[];
}): Promise<HerdrAnswer<Placement>> {
  await mkdir(options.root, { recursive: true, mode: 0o700 });
  if (!/^[a-zA-Z0-9_-]+$/.test(options.name)) throw new Error("invalid execution identity");
  const directory = join(options.root, options.name);
  await mkdir(directory, { mode: 0o700 });
  const envelope: PaneEnvelope = { command: options.command, title: options.title };
  let placed: HerdrAnswer<Placement>;
  try {
    placed = await placeRecovering({
      agent: options.home.agent,
      request: { agent: options.home.agent, name: options.name, title: options.title, directory },
      place: options.place,
      retryMs: options.placeRetryMs,
      ready: async () => {
        await publish(join(directory, "command.json"), envelope);
        await publish(join(directory, "grant"), { nonce: randomUUID() });
      },
    });
  } catch (error) {
    await rm(directory, { recursive: true, force: true });
    throw error;
  }
  if (!placed.ok) {
    await rm(directory, { recursive: true, force: true });
    return placed;
  }
  let placement = placed.result;
  while (!existsSync(join(directory, "started.json")) && !existsSync(join(directory, "outcome.json"))) {
    if (supervisorState(directory, placement) === "missing") {
      const again = await placement.reensure();
      if (!again.ok) { await writeFile(join(directory, "cancel"), "", { mode: 0o600 }); return again; }
      placement = { ...placement, host: again.result };
    }
    await Bun.sleep(50);
  }
  if (!existsSync(join(directory, "started.json"))) {
    const outcome = readJson(join(directory, "outcome.json"));
    let message = "";
    try { message = readFileSync(join(directory, "stderr"), "utf8").slice(0, 300); } catch {}
    return { ok: false, error: { code: outcome?.cause ?? "launch-error", message } };
  }
  return { ok: true, result: placement };
}
