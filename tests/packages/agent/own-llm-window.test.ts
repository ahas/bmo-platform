import "@agency/test-support/setup";

import { afterEach, expect, test } from "bun:test";
import { join } from "node:path";

import { Archive } from "@agency/archive/archive";
import { ownLlmWindow } from "@agency/agent/own-llm";
import { installProviderSettings } from "@agency/agent/provider-order";
import { scratchArchive } from "@agency/test-support/store";

import type { OwnLlmService } from "@agency/agent/own-llm";

// A request with no tools is sized to its registration's context window, read afresh by every request
// (docs/own-llm.md, 「The context window」): a change reaches the next request with nothing else
// changed or restarted, and a request with no window to read is refused rather than sized at a number
// nobody chose.

const member = {
  name: "registered",
  label: "Small",
  shortLabel: "small",
  baseUrl: "http://127.0.0.1:18999",
  keyFile: "llm-gateway.key",
  registration: "small",
} satisfies OwnLlmService;

afterEach(() => installProviderSettings(null));

async function archiveWithSmall(): Promise<Archive> {
  const archive = await scratchArchive();

  await archive.modelRegistry().create({ name: "small", label: "Small", model: "m", contextTokens: 32_768, answerTokens: 4_096, thinking: false, walked: false, keepsResearch: false });

  return archive;
}

test("the window a request is sized to follows the registration from the next request on", async () => {
  const archive = await archiveWithSmall();

  installProviderSettings(archive);
  expect(await ownLlmWindow(member)).toBe(32_768);

  await archive.modelRegistry().change("small", { contextTokens: 1_048_576 });
  expect(await ownLlmWindow(member)).toBe(1_048_576);
});

test("a request whose registration is gone, or a process holding no archive, is refused naming it", async () => {
  const archive = await archiveWithSmall();

  await archive.modelRegistry().remove("small");

  expect(() => ownLlmWindow(member, archive)).toThrow("small: there is no registered model small to read the context window of.");
  expect(() => ownLlmWindow(member, null)).toThrow("small: there is no store to read the context window from.");
});
