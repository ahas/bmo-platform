import "@agency/test-support/setup";

import { mkdirSync, readFileSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { afterAll, expect, test } from "bun:test";

import { carryNamed } from "@agency/agent/agents";
import { scratchDir } from "@agency/test-support/scratch";

const dir = scratchDir("bmo-test-agents-carry-");

afterAll(() => rmSync(dir, { recursive: true, force: true }));

// "every file under the agents tree that a place's program names is carried into the rebuilt tree, whatever its age,
// and a file the deployment wrote itself is kept as it wrote it"
test("a rebuilt agents tree keeps every file a standing program names", () => {
  const root = join(dir, "agents");
  const building = join(dir, "agents.building");
  const places = join(dir, "agent-places");
  const write = (path: string, text: string) => {
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, text);
  };

  write(join(root, "chat.u7.mcp.json"), "u7's servers");
  write(join(root, "prompts", "supervisor-u7.md"), "u7's prompt");
  write(join(root, "worker.mcp.json"), "the last deployment's");
  write(join(root, "chat.gone.mcp.json"), "no program names it");
  write(join(building, "worker.mcp.json"), "this deployment's");
  write(
    join(places, "u7", "program.json"),
    JSON.stringify({
      identity: "one",
      command: { args: ["claude", "--mcp-config", join(root, "chat.u7.mcp.json"), "--add-dir", root], cwd: "/", env: {} },
      restart: { args: ["claude", "--mcp-config", join(root, "worker.mcp.json")], cwd: "/", env: {} },
      needs: [join(root, "prompts", "supervisor-u7.md"), join(root, "never-written.json"), "/elsewhere/file.json"],
    }),
  );
  write(join(places, "broken", "program.json"), "not json");

  carryNamed(root, building, places);

  expect(readFileSync(join(building, "chat.u7.mcp.json"), "utf8")).toBe("u7's servers");
  expect(readFileSync(join(building, "prompts", "supervisor-u7.md"), "utf8")).toBe("u7's prompt");
  expect(readFileSync(join(building, "worker.mcp.json"), "utf8")).toBe("this deployment's");
  expect(existsSync(join(building, "chat.gone.mcp.json"))).toBe(false);
  expect(existsSync(join(building, "never-written.json"))).toBe(false);
});
