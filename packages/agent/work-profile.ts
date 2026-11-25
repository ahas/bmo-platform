import { fallbackRouteModes, isFallbackRouteMode } from "@agency/archive/route-modes";
import type { Sampling } from "./harness";
import type { FallbackRouteMode } from "@agency/archive/route-modes";

// What a work key's profile is: the defaults a piece of work runs on where its owner has chosen nothing.
// The profiles themselves — which work keys there are, what each is called and where each starts — are the
// deployment's own, handed to whatever reads them. Routes are configured for work, not for whichever model
// happens to do that work today, so a launch-model change never silently moves another work's road.

// The route modes a key may take stand with the archive, which stores them (`route-modes.ts`).
export { fallbackRouteModes, isFallbackRouteMode };
export type { FallbackRouteMode };

export interface CodexChoice {
  model: string;
  effort: string;
}

export interface WorkProfile<Key extends string = string, Kind extends string = string, Line extends string = string, Tier extends string = string> {
  key: Key;
  label: string;
  kind: Kind;
  // The run line the work goes out on, which is what decides whether a provider's program can
  // carry it at all.
  line: Line;
  // The road taken where the owner has written no route for the key.
  defaultRouteMode: FallbackRouteMode;
  defaultClaudeModel: string;
  defaultCodex: CodexChoice;
  // The tier the key starts on where nobody has assigned one (docs/model-tiers.md).
  tier: Tier;
  // The two request options a gate's own work decides, read only for a gate and only where the
  // owner has chosen nothing for that gate. Both are written out in `docs/own-llm.md`.
  //
  // The room a gate's answer is given, where that gate's answer is not the shape the common
  // default was measured against.
  gateAnswerTokens?: number;
  // How the gate's request with no tools samples, by the engine's own names: a gate's answer is a
  // verdict or a faithful rewrite, never the chat's conversation, so the engine's defaults are not
  // what it is asked with (docs/own-llm.md, 「Sampling per gate」).
  gateSampling?: Sampling;
  // The least room a run of the work is given for its answer on a road whose model sizes it, where the
  // work writes a whole document in one call (docs/own-llm.md, 「Answer slots」); none leaves the
  // road's own size.
  answerFloor?: number;
  // The least room where the run thinks, as far as a quarter of the road's window allows: its
  // thinking is written out of the same room before the answer, so a work whose answer is a whole
  // document needs room for both; none leaves `answerFloor` alone.
  thinkingAnswerFloor?: number;
  // Whether the owner may switch this gate off: its caller has a way of its own without it, so a
  // gate switched off is simply not asked. Every other gate always runs.
  switchable?: true;
  // Whether its turns are asked about a session rather than held as one, so a turn of it leaves
  // nothing behind to continue or warm, whichever road carried it.
  holdsNoSession?: true;
}

// The deployment's work: every profile it runs, which of those keys are gates — the keys a
// judgement runs under, whose options are a gate's own — and the two keys the
// base itself has to tell apart: the chat's, whose session is kept in settings of its own, and the
// one a job's turns run under, whose scopes carry the worker's name after it. The deployment hands
// them over as a function, read the first time they are asked for; a read with nothing composed
// fails, naming what is missing.
export interface DeploymentWork {
  profiles: readonly WorkProfile[];
  gateKeys: readonly string[];
  chatKey: string;
  jobKey: string;
}

export class WorkProfilesUncomposed extends Error {
  constructor() {
    super("no work profiles are composed: a process reads its configuration before it asks a work key's profile, or imports the deployment's registrations itself");
  }
}

let composedWork: (() => DeploymentWork) | null = null;

export function composeWorkProfiles(work: () => DeploymentWork): void {
  composedWork = work;
}

function deploymentWork(): DeploymentWork {
  if (composedWork === null) {
    throw new WorkProfilesUncomposed();
  }

  return composedWork();
}

export function workProfiles(): readonly WorkProfile[] {
  return deploymentWork().profiles;
}

export function isWorkKey(value: unknown): value is string {
  return typeof value === "string" && workProfiles().some((one) => one.key === value);
}

export function workProfile(key: string): WorkProfile {
  const profile = workProfiles().find((one) => one.key === key);

  if (profile === undefined) {
    throw new Error(`unknown work key: ${key}`);
  }

  return profile;
}

export function gateWorkKeys(): readonly string[] {
  return deploymentWork().gateKeys;
}

/** The key a job's turns run under; a job's own scope is this key, a colon and the worker's name. */
export function jobWorkKey(): string {
  return deploymentWork().jobKey;
}

export function isGateWorkKey(value: unknown): value is string {
  return typeof value === "string" && gateWorkKeys().includes(value);
}
