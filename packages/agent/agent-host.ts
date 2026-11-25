// What a launcher knows of the agent host (`docs/agent-control.md`, 「The host」): a process told
// apart from one that reused its pid, where executions keep their envelopes, the tree a head runs
// from, and what a placement answers. The host itself is the `agent-host` crate, and the launcher's
// side of placing in it is `root-host.ts`.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { dataRoot, deployedTree } from "@agency/support/roots";

import type { HerdrAnswer } from "./herdr";

export interface ProcessIdentity {
  pid: number;
  birth: string;
}

// A process's start time as the kernel counts it, which is what tells a pid reused by another
// process apart from the one that was recorded.
export function processBirth(pid: number): string | null {
  try {
    return readFileSync(`/proc/${pid}/stat`, "utf8").split(") ").at(-1)!.split(" ")[19] ?? null;
  } catch {
    return null;
  }
}

export function identityAlive(identity: ProcessIdentity | null | undefined): boolean {
  if (!identity || !Number.isSafeInteger(identity.pid) || identity.pid <= 1) {
    return false;
  }

  const birth = processBirth(identity.pid);

  if (birth === null) {
    return false;
  }

  // A zombie still has a stat line and is no longer anybody's supervisor.
  try {
    if (readFileSync(`/proc/${identity.pid}/stat`, "utf8").split(") ")[1]?.startsWith("Z")) {
      return false;
    }
  } catch {
    return false;
  }

  return birth === identity.birth;
}

export function ownIdentity(): ProcessIdentity {
  return { pid: process.pid, birth: processBirth(process.pid) ?? "" };
}

// Where executions keep their envelopes.
export function executionsRoot(): string {
  return join(dataRoot(), "executions");
}

// A name as a directory. Agent names are plain words; anything else is hashed rather than trusted
// as a path.
export function nameDirectory(name: string): string {
  return /^[a-zA-Z0-9_-]{1,64}$/.test(name) ? name : `h-${createHash("sha256").update(name).digest("hex").slice(0, 16)}`;
}

// The tree a head runs from: the deployed one where it is set up, so a script run out of a checkout
// and the bot itself stand the same head; the running code's own tree otherwise — a test, or a
// machine with no deployment.
export function hostCodeRoot(): string {
  return deployedTree(join("bin", "pane-head"));
}

export interface Placement {
  paneId: string | null;
  host: ProcessIdentity;
  // Where the host that took the pointer is gone before starting it, a new host is started and
  // takes it; this is how a launcher asks for that.
  reensure: () => Promise<HerdrAnswer<ProcessIdentity>>;
}
