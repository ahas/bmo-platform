// Agency's work keys and the profile each starts on, as the policy declares them (docs/policy.md,
// docs/model-tiers.md), read here from the file agency's Rust side reads and checked as it is read. What
// a profile is, and the route modes a key may take, are `work-profile.ts`'s.

import declaredTiers from "@agency/policy/tiers.toml";
import declaredWork from "@agency/policy/work-keys.toml";

import { generationRefusal, generationSetting, samplingRun } from "./generation-settings";
import { isFallbackRouteMode } from "./work-profile";

import type { GenerationValues } from "./generation-settings";
import type { RunLine } from "./run-lines";
import type { CodexChoice, DeploymentWork, FallbackRouteMode, WorkProfile } from "./work-profile";

// The kinds of work a key names: the supervisor's own, the job worker's, a runner line's and a gate's.
export type WorkKind = "bmo" | "worker" | "runner" | "gate";

const kinds: readonly WorkKind[] = ["bmo", "worker", "runner", "gate"];

// A profile as the policy's file declares it.
interface DeclaredProfile {
  key: string;
  label: string;
  kind: string;
  line: string;
  default_route_mode: string;
  default_claude_model: string;
  default_codex: CodexChoice;
  tier: string;
  gate_answer_tokens?: number;
  gate_sampling?: Record<string, unknown>;
  answer_floor?: number;
  thinking_answer_floor?: number;
  switchable?: boolean;
  holds_no_session?: boolean;
}

function profileOf(declared: DeclaredProfile): WorkProfile<string, WorkKind, RunLine> {
  const refused = (what: string): never => {
    throw new Error(`the work key ${declared.key} declares ${what}`);
  };

  if (!(kinds as readonly string[]).includes(declared.kind)) refused(`a kind no work has: ${declared.kind}`);
  if (!isFallbackRouteMode(declared.default_route_mode)) refused(`a road no key takes: ${declared.default_route_mode}`);
  if (!(declaredTiers.ladder as string[]).includes(declared.tier)) refused(`a tier off the ladder: ${declared.tier}`);

  for (const [name, value] of Object.entries(declared.gate_sampling ?? {})) {
    const setting = generationSetting(name);
    const refusal = setting === null ? "no engine reads it" : generationRefusal(setting, value, null);

    if (refusal !== null) refused(`a sampling setting ${name} it cannot be asked with: ${refusal}`);
  }

  const gateSampling = declared.gate_sampling === undefined ? undefined : samplingRun(declared.gate_sampling as GenerationValues).sampling;

  return {
    key: declared.key,
    label: declared.label,
    kind: declared.kind as WorkKind,
    line: declared.line as RunLine,
    defaultRouteMode: declared.default_route_mode as FallbackRouteMode,
    defaultClaudeModel: declared.default_claude_model,
    tier: declared.tier,
    defaultCodex: { model: declared.default_codex.model, effort: declared.default_codex.effort },
    ...(declared.gate_answer_tokens === undefined ? {} : { gateAnswerTokens: declared.gate_answer_tokens }),
    ...(gateSampling === undefined ? {} : { gateSampling }),
    ...(declared.answer_floor === undefined ? {} : { answerFloor: declared.answer_floor }),
    ...(declared.thinking_answer_floor === undefined ? {} : { thinkingAnswerFloor: declared.thinking_answer_floor }),
    // A mark the file sets stands on the profile as true, and one it leaves out is absent.
    ...(declared.switchable === true ? { switchable: true as const } : {}),
    ...(declared.holds_no_session === true ? { holdsNoSession: true as const } : {}),
  };
}

/** Every work key the policy declares, in its order. */
export const workKeyProfiles: readonly WorkProfile<string, WorkKind, RunLine>[] = (declaredWork.profile as DeclaredProfile[]).map(profileOf);

// The one key of a kind the policy declares, refused where it declares none.
function keyOfKind(kind: WorkKind): string {
  const found = workKeyProfiles.find((one) => one.kind === kind);

  if (found === undefined) throw new Error(`the work keys declare no ${kind} key`);

  return found.key;
}

/** The work the policy declares, as `composeWorkProfiles` is handed it: the gates in the policy's order. */
export function policyWork(): DeploymentWork {
  return {
    profiles: workKeyProfiles,
    gateKeys: workKeyProfiles.filter((one) => one.kind === "gate").map((one) => one.key),
    chatKey: keyOfKind("bmo"),
    jobKey: keyOfKind("worker"),
  };
}
