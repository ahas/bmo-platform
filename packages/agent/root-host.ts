// The launcher's side of the agent host of a data root (docs/agent-control.md, 「The host」): its
// claim read to know it is up, a copy's host started where none stands, an execution put in by a
// pointer naming its agent and its envelope, and the agent's view written to by the collector.

import { appendFileSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join } from "node:path";

import { executionsRoot, identityAlive, nameDirectory, type Placement, type ProcessIdentity } from "./agent-host";
import { dataRoot, dataRootCopy, deployedTree } from "@agency/support/roots";
import { userBus } from "@agency/support/user-bus";

import type { HerdrAnswer } from "./herdr";
import type { Placer } from "./pane-process";

/** The agent host's own directory among the envelopes. */
export function rootHostDirectory(): string {
  return join(executionsRoot(), "agent-host");
}

/** Where an agent's view file stands, which its head pane follows. */
export function agentViewPath(agent: string): string {
  return join(rootHostDirectory(), "views", nameDirectory(agent));
}

/** The host holding the data root now, or null where none live holds it. */
export function rootHostClaim(): ProcessIdentity | null {
  try {
    const claim = JSON.parse(readFileSync(join(rootHostDirectory(), "claim"), "utf8")) as ProcessIdentity;

    return identityAlive(claim) ? { pid: claim.pid, birth: claim.birth } : null;
  } catch {
    return null;
  }
}

/** The binary a copy's host runs: the deploy tree's. */
function hostBinary(): string {
  return join(deployedTree(join("bin", "agent-host")), "bin", "agent-host");
}

// The transient user unit a copy's host runs as, named for the copy's root.
function copyHostUnit(root: string): string {
  return `bmo-agent-host-copy-${createHash("sha256").update(root).digest("hex").slice(0, 12)}`;
}

// A copy's host, started as a transient user unit with the properties of the bot's own: its main
// process alone stopped with it, and the unit left standing when one process is killed for memory. It
// stands until its root is taken away or its unit is stopped.
async function startCopyHost(root: string): Promise<string | null> {
  const unit = copyHostUnit(root);
  const started = Bun.spawn(
    ["systemd-run", "--user", "--quiet", "--collect", `--unit=${unit}`, "-p", "KillMode=process", "-p", "OOMPolicy=continue",
      hostBinary(), "serve", "--data-root", root],
    { env: userBus(), stdin: "ignore", stdout: "ignore", stderr: "pipe" },
  );
  const said = await new Response(started.stderr).text();

  return (await started.exited) === 0 ? null : said.trim() || `systemd-run exited ${started.exitCode}`;
}

// Whether a user unit is still standing: anything but systemd saying it is inactive or failed. A
// systemctl that cannot be asked has not said the unit is gone.
async function unitStanding(unit: string): Promise<boolean> {
  try {
    const asked = Bun.spawn(["systemctl", "--user", "is-active", `${unit}.service`], { env: userBus(), stdin: "ignore", stdout: "pipe", stderr: "ignore" });
    const said = (await new Response(asked.stdout).text()).trim();

    await asked.exited;

    return !["inactive", "failed"].includes(said);
  } catch {
    return true;
  }
}

/**
 * The data root's host, waited for until it takes the claim: on the bot's own root the unit's host,
 * which a launch never starts and systemd stands up again however long that takes; on a copy's, the
 * copy's host, started first where none holds its claim, and its unit ending without a claim ends
 * the wait.
 */
export async function ensureRootHost(): Promise<HerdrAnswer<ProcessIdentity>> {
  const standing = rootHostClaim();

  if (standing !== null) return { ok: true, result: standing };

  const copy = dataRootCopy();
  const unit = copyHostUnit(dataRoot());

  if (copy !== null) {
    const refused = await startCopyHost(dataRoot());

    if (refused !== null) return { ok: false, error: { code: "host-unavailable", message: `agent-host for ${copy.root} could not be started: ${refused}` } };
  }

  for (;;) {
    await Bun.sleep(100);

    const claim = rootHostClaim();

    if (claim !== null) return { ok: true, result: claim };

    if (copy !== null && !(await unitStanding(unit)) && rootHostClaim() === null) {
      return { ok: false, error: { code: "host-unavailable", message: `the copy's agent-host unit ${unit} ended without taking the claim on ${dataRoot()}` } };
    }
  }
}

/**
 * Where an execution is put on a line standing on the agent host: a pointer naming the agent and the
 * envelope, written beside its name and renamed into the host's inbox once the host is up.
 */
export function rootHostPlacer(): Placer {
  return async (request): Promise<HerdrAnswer<Placement>> => {
    const host = await ensureRootHost();

    if (!host.ok) return host;

    const inbox = join(rootHostDirectory(), "inbox");
    const pointer = join(inbox, request.name);

    mkdirSync(inbox, { recursive: true, mode: 0o700 });
    writeFileSync(`${pointer}.new`, JSON.stringify({ agent: request.agent, envelope: request.directory }), { mode: 0o600 });
    renameSync(`${pointer}.new`, pointer);

    return { ok: true, result: { paneId: null, host: host.result, reensure: () => ensureRootHost() } };
  };
}

/** Lines appended to an agent's view; the host keeps the file to its newest part. */
export function appendView(agent: string, lines: readonly string[]): void {
  if (lines.length === 0) return;

  const path = agentViewPath(agent);

  try {
    mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
    appendFileSync(path, `${lines.join("\n")}\n`, { mode: 0o600 });
  } catch (error) {
    console.error(`${agent}'s view could not be written: ${error instanceof Error ? error.message : String(error)}`);
  }
}
