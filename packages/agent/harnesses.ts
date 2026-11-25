import declaredRegistered from "@agency/policy/providers/registered.toml";

import { installProviderSettings } from "./provider-order";
import { claudeCodeProvider } from "./claude-code";
import { codexProvider } from "./codex";
import { deepSeekProvider } from "./deepseek";
import { openRouterProvider } from "./dsh";
import { registeredProvider } from "./registered-provider";

import { composeAgentProviders } from "./agent-providers";

import type { Archive } from "@agency/archive/archive";
import type { AgentProvider } from "./harness";
import type { RunLine } from "./run-lines";

// The providers this build carries, composed into the agent base's registry
// (`agent-providers.ts`), which every other module asks. What exists is code — an implementation is
// a file, and a build carries the ones it was written with — so this is the one place that names
// them. Which provider runs which work key is the owner's choice, and a choice of the owner's is a row
// in the store's route table, changed without a commit, and nothing here states it a second
// time where the two could drift apart.
//
// Handed over as a function, because an implementation imports the modules it delegates to and
// those reach back here; the list is read once everything has loaded. The order is the order a
// screen offers them in, and Claude Code first is the program a line's own code spawns on.
// The registered models carry the lines their file of the policy names: the chat, the worker and the
// supervisors as Claude Code on the gateway, and the gates as one request with no tools
// (docs/own-llm.md, docs/supervisors.md). Which lines it carries is said in its file of the policy and
// read here; which models there are is the registry's, which the owner changes with no commit.
export const agencyRegisteredProvider = registeredProvider({ lines: declaredRegistered.lines as RunLine[] });

export function agencyAgentProviders(): AgentProvider[] {
  return [claudeCodeProvider, agencyRegisteredProvider, codexProvider, openRouterProvider, deepSeekProvider];
}

composeAgentProviders(agencyAgentProviders);

export {
  harnessFor,
  harnessImplementations,
  provider,
  providerImplementations,
  setHarnessChooser,
} from "./agent-providers";

// What a process that holds the store does on its way up: the providers' settings are read out of
// that store by every run this process makes, rather than one runner silently keeping the code
// defaults. A gate's road and its runs are the gate unit's.
export function installHarnessRegistry(archive: Archive): void {
  installProviderSettings(archive);
}
