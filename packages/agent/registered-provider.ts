import { join } from "node:path";

import { fillMarks } from "@agency/support/marks";

import { claudeCodeAt, claudeResidentAt } from "./claude-code";
import { defaultOwnLlmAnswerTokens } from "./own-llm";
import { providerSettingsArchive } from "./provider-order";
import { dataPath } from "@agency/support/roots";
import { gatewayLongToolHeaders, gatewaySamplingHeaders, gatewayUrl } from "./gateway/address";
import { missingOnMachine } from "./provider-facts";
import { providerDeclaration } from "./provider-declaration";
import declaredRegistered from "@agency/policy/providers/registered.toml";

import type { ClaudeEndpoint, EndpointModel } from "./claude-code";
import type { AgentAdapter, AgentProvider, HarnessRun, RunLine } from "./harness";
import type { OwnLlmService, OwnLlmSettings } from "./own-llm";
import type { TierMapping } from "@agency/archive/model-tiers";
import type { Archive } from "@agency/archive/archive";
import type { ProviderReasoning } from "./reasoning";
import type { ModelRegistryStore, Registration } from "@agency/archive/model-registry";

// The models the owner registers, as one provider whose models are the registrations (docs/own-llm.md,
// 「Registered models」). Its program is Claude Code pointed at agency's gateway, which serves every
// registration under its name and asks its addresses at their chat completions paths; what a run on
// one is sized to — its window, its answer, its wait, whether it thinks — is the registration's, read
// out of the archive as the run is started, so a saved change reaches the next run with no
// deployment. A chat with no tool on one is one request to the gateway under the same name, asked in
// the agent's host.

// What the registry declares of itself, its own file of the shared policy (docs/provider-presence.md).
const declared = providerDeclaration(declaredRegistered);
const presence = declaredRegistered.presence as { key_file: string; no_archive: string; no_address: string; not_registered: string; node_missing: string };

// The key agency's gateway opens with; where it answers is `gatewayUrl`'s.
const gatewayKeyFile = presence.key_file;

const providerName = "registered";

const providerLabel = declared.label;

// The effort a mapping of the registry names: none, which leaves thinking to the registration, or
// thinking on or off outright.
export const registeredEfforts = declared.efforts;

function registry(archive: Archive | null = providerSettingsArchive()): ModelRegistryStore | null {
  return archive === null ? null : archive.modelRegistry();
}

// The registration a run names, or a refusal naming it: nothing stands in for a registration the
// registry does not hold.
async function registrationNamed(model: string): Promise<Registration> {
  const held = await registry()?.get(model) ?? null;

  if (held === null || held.addresses.length === 0) throw new Error(`${presence.not_registered}: ${model}`);

  return held;
}

// Whether a run on a registration thinks: the mapping's word where it names one, the registration's
// default otherwise.
function thinks(registration: Registration, effort: string): boolean {
  return effort === "thinking" ? true : effort === "no-thinking" ? false : registration.thinking;
}

// Under the data root, since a session resumed across a deployment needs its record, and the agents root
// is laid anew at every start of the bot (docs/own-llm.md, 「How requests go」).
const homes = () => dataPath("claude-roads", providerName);

// Where a new session on a registration stands (docs/own-llm.md, 「Nodes and sessions」): on one of
// several addresses, the one given a new session least recently — an address never given one first,
// and the registration's own order among equals — recorded as given now; on a registration of one
// address, nowhere in particular, since there is nowhere else to go.
export async function placeRegistered(run: Pick<HarnessRun, "model">, archive: Archive | null = providerSettingsArchive(), now: number = Date.now()): Promise<string | null> {
  const store = registry(archive);
  const held = await store?.get(run.model) ?? null;

  if (store === null || held === null || held.addresses.length < 2) return null;

  const [chosen] = held.addresses.toSorted((a, b) => (a.placedAt ?? -1) - (b.placedAt ?? -1));

  if (chosen === undefined) return null;

  await store.markPlaced(held.name, chosen.node, now);

  return chosen.node;
}

// The gateway's road, handed to Claude Code, which knows no registration.
export const registeredEndpoint: ClaudeEndpoint = {
  provider: providerName,
  get baseUrl() {
    return gatewayUrl();
  },
  keyPath: () => dataPath("env", gatewayKeyFile),
  // The note the gateway opens a system turn with where it turns one into a user message of its own
  // (docs/own-llm.md, 「The gateway」).
  ownWords: (declaredRegistered as { gateway: { own_words: string[] } }).gateway.own_words,
  // A run on a registration of several nodes names the one its session stands on, so every request
  // of the session is asked of that node alone.
  async model(run: Pick<HarnessRun, "model" | "effort" | "placement" | "sampling">, longTools?: readonly string[]): Promise<EndpointModel> {
    const registration = await registrationNamed(run.model);

    if (run.placement !== undefined && !registration.addresses.some((one) => one.node === run.placement)) {
      throw new Error(fillMarks(presence.node_missing, { name: registration.name, node: run.placement }));
    }

    return {
      name: run.placement === undefined ? registration.name : `${registration.name}@${run.placement}`,
      label: registration.label,
      contextTokens: registration.contextTokens,
      answerTokens: registration.answerTokens,
      headers: {
        ...gatewaySamplingHeaders(run.sampling),
        ...gatewayLongToolHeaders(longTools),
      },
      thinking: thinks(registration, run.effort),
      home: join(homes(), registration.name),
    };
  },
  // Thinking is `medium`, which Claude Code sends as adaptive thinking, and not thinking is `low` with
  // no thinking tokens; the gateway carries either to the address (docs/own-llm.md, 「The gateway」).
  effort: ({ thinking }) => (thinking ? "medium" : "low"),
  place: (run) => placeRegistered(run),
  get homes() {
    return homes();
  },
  // The program is Claude Code, which gates each call and withholds its own tools as it does on its
  // own road. Whether a runner's prompt fits is a registration's window, which the caller that names
  // a registration for a session on a runner line answers for.
  offers: ["tool-gate", "mcp-only-tools"],
};

// The registration a process stands under: one started by a program on this road — the program
// itself, a shell command of it, a server it started — carries the home the program was handed, and
// the home is the registration's own. Null for a process under anything else.
export function registrationRunning(env: Record<string, string | undefined> = process.env): string | null {
  const home = env.CLAUDE_CONFIG_DIR;
  const root = `${homes()}/`;

  if (home === undefined || !home.startsWith(root)) return null;

  const [name] = home.slice(root.length).split("/");

  return name === undefined || name === "" ? null : name;
}

// A registration as the service a chat with no tool asks directly: the gateway, under the
// registration's name, sized to the registration's window.
async function serviceOf(model: string, archive?: Archive): Promise<OwnLlmService | null> {
  const held = await registry(archive)?.get(model) ?? null;

  return held === null || held.addresses.length === 0
    ? null
    : {
      name: providerName,
      label: held.label,
      shortLabel: held.label,
      baseUrl: gatewayUrl(),
      keyFile: gatewayKeyFile,
      registration: held.name,
    };
}

// What an agent's host pane shows of a line a run on the registry printed. Two kinds of run print
// there: Claude Code on the gateway, whose typed events are read as Claude Code's are, and a request
// with no tool asked directly, which prints its answer as it stands and is shown so.
function hostProgress(claude: NonNullable<AgentAdapter["progressOf"]>): NonNullable<AgentAdapter["progressOf"]> {
  return (line) => {
    const trimmed = line.trim();

    if (trimmed === "") return null;
    if (!trimmed.startsWith("{")) return line;

    let event: unknown;

    try {
      event = JSON.parse(trimmed);
    } catch {
      return line;
    }

    return typeof (event as { type?: unknown } | null)?.type === "string" ? claude(line) : line;
  };
}

// A registered model's only reasoning setting is whether it thinks, which is its effort word: an
// agent's thinking on or off stands over the mapping's word, and no level reaches it.
const registeredReasoning: ProviderReasoning = {
  thinking: true,
  levels: async () => [],
  effort: async (mapped, choice) => (choice.thinking === null ? mapped : choice.thinking ? "thinking" : "no-thinking"),
};

// What a mapping of the registry asks a run for: its registration, thinking as the mapping names it
// or else as the registration does by default, the mapping's answer ceiling or else the
// registration's, and the registration's wait and window. A mapping naming a registration the
// registry does not hold asks for it all the same, and the run is refused where it is launched.
async function runSettings(mapping: TierMapping | null): Promise<OwnLlmSettings> {
  const held = mapping === null ? null : await registry()?.get(mapping.model) ?? null;
  const effort = mapping?.effort ?? null;

  return {
    provider: providerName,
    model: mapping?.model ?? "",
    effort,
    thinking: held === null ? effort === "thinking" : thinks(held, effort ?? ""),
    answerTokens: mapping?.answerTokens ?? held?.answerTokens ?? defaultOwnLlmAnswerTokens,
    ...(held === null ? {} : { contextTokens: held.contextTokens }),
  };
}

// Why the registry cannot answer on this machine now, or null where it can: the gateway's key file,
// which the machine answers for, and then a registration with an address, which the archive does.
async function unavailable(): Promise<string | null> {
  const missing = await missingOnMachine(providerName);

  if (missing !== null) return missing;

  const held = registry();

  if (held === null) return presence.no_archive;
  if (!(await held.list()).some((one) => one.addresses.length > 0)) return presence.no_address;

  return null;
}

export function registeredProvider(options: { lines: readonly RunLine[] }): AgentProvider {
  const carried = claudeCodeAt(registeredEndpoint);

  return {
    name: providerName,
    label: providerLabel,
    adapter: { ...carried, ...(carried.progressOf === undefined ? {} : { progressOf: hostProgress(carried.progressOf) }) },
    unavailable,
    models: async () => (await registry()?.list() ?? []).filter((one) => one.addresses.length > 0).map((one) => ({ id: one.name })),
    efforts: registeredEfforts,
    effortFault: declared.effortFault,
    reasoning: registeredReasoning,
    capsAnswerTokens: declared.capsAnswerTokens,
    logins: null,
    // An agent on a registered model stands Claude Code on the gateway in its pane.
    resident: claudeResidentAt(registeredEndpoint, declaredRegistered.resident as { interrupt: string[]; words_mid_turn: boolean }),
    serviceOf,
    lines: options.lines,
    runSettings,
    place: (run, archive, now) => placeRegistered(run, archive, now),
    // A worker pinned to a registration, and a job naming one, run on it with its own defaults.
    namedModelFault: async (model, archive) => ((await archive.modelRegistry().get(model))?.addresses.length ?? 0) > 0 ? null : `${presence.not_registered}: ${model}`,
  };
}

