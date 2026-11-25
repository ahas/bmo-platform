import { existsSync } from "node:fs";
import { join } from "node:path";

import { codeRoot, dataRoot, deployRoot } from "@agency/support/roots";

// What each harness provider answers about this machine — whether it can answer here, and which
// models it offers — read by the harness-providers crate's binary, which the control port's front
// links as a library (docs/provider-presence.md). Nothing here reads the machine itself.

const BINARY = "harness-providers";

// A screen asks every provider for every work key at once, so one reading of the machine answers every
// question put within a second of it, for the same environment.
const presenceLastsMs = 1_000;
const readings = new Map<string, { at: number; answer: Promise<Presence> }>();

/**
 * The binary the sidecars and the verbs run: the deploy tree's, where the deployment put it, and where
 * none stands there — the code running out of a checkout — that checkout's own debug build, in the
 * target directory Cargo builds it into (docs/provider-presence.md).
 */
export function providerFactsBinary(): string {
  const deployed = join(deployRoot(), "bin", BINARY);
  const named = process.env.CARGO_TARGET_DIR;
  const target = named === undefined || named === "" ? join(codeRoot, "target") : named;

  return existsSync(deployed) ? deployed : join(target, "debug", BINARY);
}

/** A listing a provider's catalogue could not give, with the reason the machine gave. */
export class ProviderListingError extends Error {}

// The environment a question is asked in: this process's, and the caller's over it.
function machineEnv(env: Record<string, string | undefined>): Record<string, string> {
  const merged: Record<string, string> = {};

  for (const [name, value] of Object.entries({ ...process.env, ...env })) {
    if (value !== undefined) merged[name] = value;
  }

  return merged;
}

// One question to the binary, its answer read off the standing output; a failure throws with what
// the binary said.
async function ask(args: readonly string[], env: Record<string, string | undefined> = {}): Promise<unknown> {
  const merged = machineEnv(env);
  const child = Bun.spawn([providerFactsBinary(), ...args, "--data-root", dataRoot()], {
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
    env: merged,
  });
  const [code, said, complained] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);

  if (code !== 0) {
    throw new ProviderListingError(complained.trim() || `${BINARY} ${args.join(" ")} ended with ${code}`);
  }

  return JSON.parse(said);
}

interface Presence {
  providers: { name: string; missing: string | null }[];
}

// The machine's presence for an environment, read afresh once the last reading is a second old. A
// reading that failed is not kept.
function presence(env: Record<string, string | undefined>): Promise<Presence> {
  const key = JSON.stringify([dataRoot(), machineEnv(env)]);
  const now = Date.now();
  const held = readings.get(key);

  if (held !== undefined && now - held.at < presenceLastsMs) {
    return held.answer;
  }

  const answer = ask(["presence"], env) as Promise<Presence>;

  readings.set(key, { at: now, answer });
  answer.catch(() => readings.delete(key));

  return answer;
}

/**
 * Why this machine cannot use a provider, in the owner's language, or null where it can. The
 * environment given is laid over the process's for the question, so a caller can hand the key its
 * configuration holds. A binary that cannot be asked is a reason too.
 */
export async function missingOnMachine(name: string, env: Record<string, string | undefined> = {}): Promise<string | null> {
  try {
    const answered = await presence(env);

    return answered.providers.find((one) => one.name === name)?.missing ?? null;
  } catch (error) {
    return `The provider check (${BINARY}) could not be asked: ${error instanceof Error ? error.message : String(error)}`;
  }
}

/** A provider's catalogue as this machine holds it, under a login's directory where it keeps one there. */
export async function machineCatalogue(name: string, configDir?: string): Promise<unknown> {
  return await ask(["models", name, ...(configDir === undefined ? [] : ["--config-dir", configDir])]);
}
