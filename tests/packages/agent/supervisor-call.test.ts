import "@agency/test-support/setup";

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { afterAll, describe, expect, test } from "bun:test";

import { scratchArchive } from "@agency/test-support/store";
import { scratchDir } from "@agency/test-support/scratch";
import { agencyRegisteredProvider, installHarnessRegistry } from "@agency/agent/harnesses";
import { installProviderSettings } from "@agency/agent/provider-order";
import { cliProjectsRoot, transcriptPath } from "@agency/agent/cli-transcripts";

import type { HarnessRun } from "@agency/agent/harness";

const archive = await scratchArchive();

// The registration the turn goes out on, served at one address, as the owner registers one.
await archive.modelRegistry().create({ name: "local-0-1", label: "Local", model: "local-0-1", contextTokens: 131_072, answerTokens: 8_192, thinking: false, walked: false, keepsResearch: false });
await archive.modelRegistry().setAddress("local-0-1", { node: "able", url: "http://127.0.0.1:19001/v1/chat/completions", keyFile: null, keyOwned: false });
installHarnessRegistry(archive);
afterAll(() => installProviderSettings(null));

// A turn that called tools has a result summing every request, and the road's stream counts none of
// its own, so what the session holds is read off its transcript against the registration's window
// (docs/own-llm.md, 「How requests go」).
describe("what a kept session on a registration holds after a turn", () => {
  test("is its transcript's last request against the registration's window, whatever the result sums", async () => {
    const window = (await archive.modelRegistry().get("local-0-1"))!.contextTokens;
    const cwd = scratchDir("bmo-supervisor-held-");
    const systemPromptPath = join(cwd, "system.md");

    writeFileSync(systemPromptPath, "s");

    const run = { line: "chat", model: "local-0-1", effort: "", cwd, systemPromptPath, mcpConfigPath: null, prompt: "p", promptDelivery: "input", resumeSessionId: "held-1", keepSession: true, hostCommands: false, thinking: false, answerShape: "unread" } as unknown as HarnessRun;
    const home = (await agencyRegisteredProvider.adapter.command(run)).env.CLAUDE_CONFIG_DIR!;
    const record = transcriptPath(cwd, "held-1", cliProjectsRoot(home));
    const result = JSON.stringify({ type: "result", result: "yes", session_id: "held-1", is_error: false, usage: { input_tokens: 2_000, cache_read_input_tokens: 40_000 }, modelUsage: { "local-0-1": { contextWindow: window } } });

    mkdirSync(dirname(record), { recursive: true });
    writeFileSync(record, [{ message: { usage: { input_tokens: 1_000, cache_read_input_tokens: 9_000 } } }, { message: { usage: { input_tokens: 600, cache_read_input_tokens: 10_000 } } }].map((one) => JSON.stringify(one)).join("\n"));

    const reply = await agencyRegisteredProvider.adapter.readAnswer({ stdout: result, stderr: "", exitCode: 0 }, run);

    expect(reply.contextTokens).toBe(10_600);
    expect(reply.contextRatio).toBeCloseTo(10_600 / window, 10);
    // A run that kept no session has no record, and its result stands.
    expect((await agencyRegisteredProvider.adapter.readAnswer({ stdout: result, stderr: "", exitCode: 0 }, { ...run, keepSession: false })).contextTokens).toBe(42_000);
  });
});
