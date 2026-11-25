import "@agency/test-support/setup";

import { expect, test } from "bun:test";

import { enabledProviders, providerEnabled, providerSwitchKey, setProviderEnabled } from "@agency/agent/provider-order";
import { scratchArchive } from "@agency/test-support/store";

// The owner's switch on each provider (docs/provider-order.md, 「The owner's switch on a provider」).

test("every provider is on where no switch was thrown, in the build's order, and reading writes nothing", async () => {
  const archive = await scratchArchive();

  expect(await enabledProviders(archive)).toEqual(["claude-code", "codex", "openrouter", "registered", "deepseek"]);
  expect(await archive.settings().getSetting(providerSwitchKey("codex"))).toBeNull();
});

test("a switch is a row of its provider's own, read as the Rust side reads it", async () => {
  const archive = await scratchArchive();

  await setProviderEnabled(archive, "codex", false);
  expect(await archive.settings().getSetting(providerSwitchKey("codex"))).toBe("off");
  expect(await providerEnabled(archive, "codex")).toBe(false);
  expect(await enabledProviders(archive)).toEqual(["claude-code", "openrouter", "registered", "deepseek"]);

  await setProviderEnabled(archive, "codex", true);
  expect(await providerEnabled(archive, "codex")).toBe(true);

  // Anything but `off` leaves the provider on.
  await archive.settings().setSetting(providerSwitchKey("deepseek"), "not a switch");
  expect(await providerEnabled(archive, "deepseek")).toBe(true);
});

test("the last provider left on is not turned off", async () => {
  const archive = await scratchArchive();

  for (const name of ["claude-code", "codex", "openrouter", "deepseek"] as const) await setProviderEnabled(archive, name, false);

  expect(() => setProviderEnabled(archive, "registered", false)).toThrow("Not every provider can be turned off");
  expect(await enabledProviders(archive)).toEqual(["registered"]);
});
