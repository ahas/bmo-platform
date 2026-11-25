import "@agency/test-support/setup";

import { beforeAll, afterAll } from "bun:test";
import { processPaneFixture } from "./pane-process-fixture";
let processFixture: ReturnType<typeof processPaneFixture>;
const previousPaneConfig = process.env.XDG_CONFIG_HOME;
beforeAll(() => { processFixture = processPaneFixture(); process.env.XDG_CONFIG_HOME = processFixture.root; });
afterAll(async () => { await processFixture.close(); if (previousPaneConfig === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = previousPaneConfig; });
import { expect, test } from "bun:test";
import { rmSync } from "node:fs";
import { Archive } from "@agency/archive/archive";
import { installProviderSettings, setProviderEnabled } from "@agency/agent/provider-order";
import { openPane } from "@agency/agent/herdr-run";
import { herdrModel } from "@agency/agent/herdr-model";
import { scratchDir } from "@agency/test-support/scratch";
import queueWords from "@agency/policy/queue-words.toml";
import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

for (const standing of [false, true]) {
  test(`Claude disabled during workspace setup cannot launch (${standing ? "standing" : "private"})`, async () => {
    const archive = await scratchArchive();
    installProviderSettings(archive);
    const model = herdrModel();
    const events: { method: string; params: any }[] = [];
    const root = scratchDir("bmo-provider-boundary-");
    const socket = `${root}/herdr.sock`;
    const server = Bun.listen({ unix: socket, socket: { async data(connection, chunk) {
      for (const line of chunk.toString().split("\n").filter(Boolean)) {
        const request = JSON.parse(line);
        if (request.method === "workspace.create") await setProviderEnabled(archive, "claude-code", false);
        events.push(request);
        connection.write(JSON.stringify({ id: request.id, result: model.answer(request.method, request.params) }) + "\n");
        connection.end();
      }
    } } });
    try {
      await expect(openPane({ name: "off-race", title: "test", cwd: root, command: ["claude", "--session-id", "test"], env: {},
        home: standing ? { agent: "worker", label: "worker", head: { command: ["bun", "head"], cwd: root, env: {} } } : null,
      }, { socket })).rejects.toThrow(queueWords.pool.off);
      expect(events.some(e => e.method === "layout.apply" && JSON.stringify(e.params).includes('"claude"'))).toBe(false);
      if (!standing) expect(events.some(e => e.method === "workspace.close")).toBe(true);
    } finally { server.stop(true); rmSync(root, { recursive: true, force: true }); installProviderSettings(null); }
  });
}

