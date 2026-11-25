import type { AgentAdapter, AgentProvider, AgentSlot } from "./harness";
import type { OwnLlmService } from "./own-llm";
import type { AgentProviderName } from "@agency/archive/provider-names";
import type { Archive } from "@agency/archive/archive";
import { enabledProviders } from "./provider-order";

// Which agent providers there are and which program carries a line, over a list the deployment
// composes. What exists is
// code — an implementation is a file, and a build carries the ones it was written with — so the
// deployment names its members where it composes them and nothing here names one. Which provider
// runs which work key is the owner's choice, a row of the archive's route table.

export class AgentProvidersUncomposed extends Error {
  constructor() {
    super("no agent providers are composed: a process reads its configuration before it asks for a provider, or imports the deployment's registrations itself");
  }
}

let composed: (() => readonly AgentProvider[]) | null = null;

// The deployment's providers, read the first time they are asked for rather than when they are
// handed over: an implementation reaches this module through the modules it reads, so a list
// taken while those load would be reading a binding whose file has not finished. The order is the
// order a screen offers them in, and the first is the program a line's own code spawns on.
export function composeAgentProviders(providers: () => readonly AgentProvider[]): void {
  composed = providers;
}

export function providerImplementations(): AgentProvider[] {
  if (composed === null) {
    throw new AgentProvidersUncomposed();
  }

  return [...composed()];
}

// The service a request with no tools on this provider and model is asked at, or null where the
// provider's runs are a program's or the model is no service of its (docs/own-llm.md, 「A request
// with no tools」).
export async function roadService(name: AgentProviderName, model: string, archive?: Archive | null, providers: readonly AgentProvider[] = providerImplementations()): Promise<OwnLlmService | null> {
  return await provider(name, providers).serviceOf?.(model, archive ?? undefined) ?? null;
}

// Whether a provider carries a line: every line its program carries, where it names none of its own.
export function carriesWorkLine(name: AgentProviderName, line: string, providers: readonly AgentProvider[] = providerImplementations()): boolean {
  const lines = provider(name, providers).lines;

  return lines === undefined || lines.includes(line);
}

// Every program the providers are carried on, each once however many providers it serves.
export function harnessImplementations(): AgentAdapter[] {
  return [...new Set(providerImplementations().map((one) => one.adapter))];
}

export function provider(name: AgentProviderName, providers: readonly AgentProvider[] = providerImplementations()): AgentProvider {
  const found = providers.find((one) => one.name === name);

  if (found === undefined) {
    throw new Error(`no provider named ${name} in this build`);
  }

  return found;
}

// Where a reading of this provider belongs. A provider that holds logins files it under the login
// named, or under the one it files a login-less reading under; a provider that holds none files it
// under no login at all. This is the one place that decides whether an account belongs on a row.
export function agentSlot(
  mode: AgentProviderName,
  account: string | { id: string } | null | undefined,
  providers: readonly AgentProvider[] = providerImplementations(),
): AgentSlot {
  const logins = provider(mode, providers).logins;

  return { provider: mode, account: logins === null ? null : logins.recorded(account) };
}

// The program a line runs on when it is spawned by its own code: the first provider's. Another
// provider is reached only through the route runner, which asks for its program by provider.
//
// Tests install a chooser to run a line on a stand-in program.
export type HarnessChooser = (line: string) => AgentAdapter;

let chooser: HarnessChooser | null = null;

export function setHarnessChooser(choose: HarnessChooser | null): void {
  chooser = choose;
}

export function harnessFor(line: string): AgentAdapter {
  if (chooser !== null) {
    return chooser(line);
  }

  const first = providerImplementations()[0];

  if (first === undefined) {
    throw new AgentProvidersUncomposed();
  }

  return first.adapter;
}

