// The owner's switch on each provider (docs/provider-order.md, 「The owner's switch on a provider」): a
// settings row of its own per provider, `provider_switch:<provider>`, holding `off` while the provider is off
// and on otherwise. There is no order shared by every run; each supervisor's and each role's order is its row's.
import type { Archive } from "@agency/archive/archive";
import { agentProviderNames } from "./harness";
import type { AgentProviderName } from "./harness";

const switchPrefix = "provider_switch:";
export const allProvidersOffReason = "Not every provider can be turned off: keep at least one on.";

/** Where the owner's switch on a provider is kept. */
export function providerSwitchKey(name: string): string {
  return `${switchPrefix}${name}`;
}

/** Whether a stored switch leaves its provider on: on unless it holds `off`, as the Rust side reads it. */
export function switchedOn(stored: string | null): boolean {
  return stored !== "off";
}

export async function providerEnabled(archive: Archive, name: AgentProviderName): Promise<boolean> {
  return switchedOn(await archive.settings().getSetting(providerSwitchKey(name)));
}

// The providers the owner has on, in the order this build carries them.
export async function enabledProviders(archive: Archive): Promise<AgentProviderName[]> {
  const on: AgentProviderName[] = [];

  for (const name of agentProviderNames) {
    if (await providerEnabled(archive, name)) on.push(name);
  }

  return on;
}

// The owner's switch thrown; the last provider left on is not turned off, since nothing would answer.
export async function setProviderEnabled(archive: Archive, name: AgentProviderName, enabled: boolean): Promise<void> {
  if (!agentProviderNames.includes(name)) throw new Error("Unknown provider.");
  if (!enabled && (await enabledProviders(archive)).every((one) => one === name)) throw new Error(allProvidersOffReason);

  await archive.settings().setSetting(providerSwitchKey(name), enabled ? "on" : "off");
}

// The archive for launch-boundary checks in processes that installed the harness registry.
let launchStore: Archive | null = null;
export function installProviderSettings(archive: Archive | null): void { launchStore = archive; }
export async function configuredProviderEnabled(name: AgentProviderName): Promise<boolean> {
  return launchStore === null || await providerEnabled(launchStore, name);
}

export function providerSettingsArchive(): Archive | null { return launchStore; }
