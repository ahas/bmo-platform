import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { hostCodeRoot } from "@agency/agent/agent-host";
import { codeRoot } from "@agency/support/roots";
import { scratchDir } from "@agency/test-support/scratch";
import { hostHome } from "@agency/agent/units";

// The tree an agent's head runs from (docs/agent-control.md, 「One standing pane per agent」).

describe("the code a head runs", () => {
  test("is the deploy tree's where one is set up, and the running code's own otherwise", async () => {
    const deployed = scratchDir("bmo-agent-host-deploy-");
    const before = process.env.BMO_DEPLOY_DIR;

    try {
      process.env.BMO_DEPLOY_DIR = join(deployed, "missing");
      expect(hostCodeRoot()).toBe(codeRoot);
      mkdirSync(join(deployed, "bin"), { recursive: true });
      writeFileSync(join(deployed, "bin", "pane-head"), "");
      process.env.BMO_DEPLOY_DIR = deployed;
      expect(hostCodeRoot()).toBe(deployed);
      // The head's own path stays a word of the pane's command line, which is what the loop around it
      // runs and what tells a head pane from everything else in the workspace.
      expect((await hostHome(null, "any-agent")).head.command.at(-2)).toBe(join(deployed, "bin", "pane-head"));
      expect((await hostHome(null, "any-agent")).head.command.at(-1)).toBe("any-agent");
    } finally {
      if (before === undefined) delete process.env.BMO_DEPLOY_DIR; else process.env.BMO_DEPLOY_DIR = before;
    }
  });
});
