import { join } from "node:path";

import { dataPath } from "@agency/support/roots";

import { claudeCodeAt, claudeResidentAt } from "./claude-code";
import { gatewayLongToolHeaders, gatewaySamplingHeaders, gatewayUrl } from "./gateway/address";
import { memberKeyPath, memberNamed, roadMembers, usableKeyFile } from "./members";
import { providerDeclaration } from "./provider-declaration";
import declaredRegistered from "@agency/policy/providers/registered.toml";

import type { ClaudeEndpoint, EndpointModel } from "./claude-code";
import type { AgentAdapter, AgentProvider, AgentProviderName, HarnessRun, RunLine } from "./harness";
import type { OwnLlmService, OwnLlmSettings } from "./own-llm";
import type { ProviderReasoning } from "./reasoning";
import type { TierMapping } from "@agency/archive/model-tiers";

// The mechanism every provider whose runs are Claude Code on the gateway shares (docs/own-llm.md, 「The
// gateway's members」): the members of its road, their key files, the home each model keeps its records
// under, and what a run on one is sized to. A provider answers its own label, words and reasoning; this
// answers the program and the transport.

interface MemberPolicy {
  label: string;
  lines: string[];
  caps_answer_tokens: boolean;
  effort: {
    words: string[];
    none_held: boolean;
    launch: "required" | "refused" | "either";
    fault?: string;
  };
  presence: { missing: string; not_held: string };
  claude?: { effort?: Record<string, string> };
}

const gatewayKeyFile: string = declaredRegistered.presence.key_file;

function homesOf(provider: AgentProviderName): string {
  return dataPath("claude-roads", provider);
}

// Where a run on a member goes: the gateway, the member's own name, green window, answer and wait.
function memberEndpoint(provider: AgentProviderName, policy: MemberPolicy): ClaudeEndpoint {
  const homes = homesOf(provider);

  return {
    provider,
    get baseUrl() {
      return gatewayUrl();
    },
    keyPath: () => dataPath("env", gatewayKeyFile),
    ownWords: declaredRegistered.gateway.own_words,
    async model(run: Pick<HarnessRun, "model" | "effort" | "thinking" | "sampling">, longTools?: readonly string[]): Promise<EndpointModel> {
      const member = memberNamed(provider, run.model);

      if (member === undefined) throw new Error(`${policy.presence.not_held}: ${run.model}`);
      if (member.context_tokens === undefined || member.answer_tokens === undefined) {
        throw new Error(`${member.name} names no window or no answer ceiling`);
      }

      const effort = run.effort.trim();

      return {
        name: member.name,
        label: member.label ?? member.name,
        contextTokens: member.context_tokens,
        answerTokens: member.answer_tokens,
        headers: {
          ...gatewaySamplingHeaders(run.sampling),
          ...gatewayLongToolHeaders(longTools),
          ...(effort === "" ? {} : { "x-bmo-effort": effort }),
        },
        thinking: run.thinking && member.thinking !== false,
        home: join(homes, member.name),
      };
    },
    // The backend's word maps to the Claude Code word its own policy names; a word it does not hold leaves
    // the launch at the word for whether the run thinks.
    effort: ({ thinking, effort }) => policy.claude?.effort?.[effort] ?? (thinking ? "medium" : "low"),
    homes,
    offers: ["tool-gate", "mcp-only-tools"],
  };
}

function missingReason(policy: MemberPolicy, provider: AgentProviderName): string | null {
  if (!usableKeyFile(dataPath("env", gatewayKeyFile))) return policy.presence.missing;

  const members = roadMembers(provider);

  if (members.length === 0) return policy.presence.missing;
  if (members.some((member) => !usableKeyFile(memberKeyPath(member)))) return policy.presence.missing;

  return null;
}

export function memberProvider(options: {
  name: AgentProviderName;
  policy: unknown;
  reasoning: ProviderReasoning;
}): AgentProvider {
  const policy = options.policy as MemberPolicy;
  const declared = providerDeclaration(policy);
  const endpoint = memberEndpoint(options.name, policy);
  const carried = claudeCodeAt(endpoint);
  const label = (member?: { label?: string }) => member?.label ?? "";

  return {
    name: options.name,
    label: declared.label,
    adapter: carried,
    unavailable: async () => missingReason(policy, options.name),
    models: async () => roadMembers(options.name).map((member) => ({ id: member.name })),
    efforts: declared.efforts,
    effortFault: declared.effortFault,
    reasoning: options.reasoning,
    capsAnswerTokens: declared.capsAnswerTokens,
    logins: null,
    resident: claudeResidentAt(endpoint, { interrupt: ["escape"], words_mid_turn: true }),
    lines: policy.lines as RunLine[],
    serviceOf: async (model: string): Promise<OwnLlmService | null> => {
      const member = memberNamed(options.name, model);

      return member === undefined
        ? null
        : {
            name: options.name,
            label: label(member) || member.name,
            shortLabel: label(member) || member.name,
            baseUrl: gatewayUrl(),
            keyFile: gatewayKeyFile,
            registration: member.name,
          };
    },
    runSettings: async (mapping: TierMapping | null): Promise<OwnLlmSettings> => {
      const member = mapping === null ? undefined : memberNamed(options.name, mapping.model);

      return {
        provider: options.name,
        model: mapping?.model ?? "",
        effort: mapping?.effort ?? null,
        thinking: member?.thinking ?? false,
        answerTokens: mapping?.answerTokens ?? member?.answer_tokens ?? 4_096,
        ...(member?.context_tokens === undefined ? {} : { contextTokens: member.context_tokens }),
      };
    },
    namedModelFault: async (model: string) =>
      memberNamed(options.name, model) === undefined ? `${policy.presence.not_held}: ${model}` : null,
  };
}

// The adapter a provider's member road stands on, for a caller that needs the program alone.
export function memberAdapter(options: { name: AgentProviderName; policy: unknown }): AgentAdapter {
  return claudeCodeAt(memberEndpoint(options.name, options.policy as MemberPolicy));
}
