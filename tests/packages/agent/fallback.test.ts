import "@agency/test-support/setup";

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, test } from "bun:test";

import { installProviderSettings } from "@agency/agent/provider-order";
import { codeRoot } from "@agency/support/roots";
import { scratchArchive } from "@agency/test-support/store";
import { processPaneFixture } from "./pane-process-fixture";

// The launch checks read the providers' switches from a settings archive made once for the file, and
// the panes a run would open stand in a fixture of the file's own.
const launchArchive = await scratchArchive();
let processFixture: ReturnType<typeof processPaneFixture>;
const previousPaneConfig = process.env.XDG_CONFIG_HOME;

beforeEach(() => installProviderSettings(launchArchive));
afterEach(() => installProviderSettings(null));
beforeAll(() => {
  processFixture = processPaneFixture();
  process.env.XDG_CONFIG_HOME = processFixture.root;
});
afterAll(async () => {
  await processFixture.close();
  if (previousPaneConfig === undefined) delete process.env.XDG_CONFIG_HOME;
  else process.env.XDG_CONFIG_HOME = previousPaneConfig;
});

describe("no Claude launch line moved", () => {
  const root = codeRoot;
  const source = (path: string): string => readFileSync(join(root, path), "utf8");

  test("the chat flags remain in their fixed order", () => {
    const text = source("packages/agent/claude-code.ts");
    const chat = text.slice(text.indexOf("function chatLine"), text.indexOf("function withPrompt"));

    expect([...chat.matchAll(/"(--[a-zA-Z-]+)"/g)].map((match) => match[1])).toEqual([
      "--model", "--effort", "--system-prompt-file",
      "--setting-sources", "--strict-mcp-config",
      // agency's own server, beside the strict flag that keeps every other out.
      "--mcp-config", "--disable-slash-commands", "--no-chrome",
      // A supervisor's program: bypass mode, the only built-in tools a limited supervisor keeps, and the denied
      // list, which would swallow a word after it were a flag not to follow.
      "--permission-mode", "--tools", "--disallowedTools",
      // Any other run of the line: its mode, then what it is shown and allowed.
      "--permission-mode", "--tools", "--allowedTools",
      // Only the resident program is handed its settings, after the list that would swallow them.
      "--settings",
    ]);
  });
});

