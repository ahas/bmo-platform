import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { Archive } from "@agency/archive/archive";
import { lastActionOf } from "@agency/agent/doctor";
import { deploymentNames } from "@agency/support/names";
import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

function toolLine(run: string, name: string, text: string) {
  return { agent: deploymentNames().agentKey, run, chatId: null, kind: "tool", name, parent: null, ref: null, truncated: false, text };
}

describe("the last thing a dead run did", () => {
  test("it is the newest tool of that run, with what the call was given", async () => {
    const archive = await scratchArchive();

    await archive.runs().appendAgentLog(toolLine("chat:7:1:1", "Read", JSON.stringify({ file_path: "/srv/checkout/README.md" })));
    await archive.runs().appendAgentLog(toolLine("chat:7:1:1", "Bash", JSON.stringify({ command: "bun scripts/todo.ts --list" })));
    await archive.runs().appendAgentLog(toolLine("chat:7:1:2", "Grep", JSON.stringify({ pattern: "x" })));

    expect(await lastActionOf(archive, "chat:7:1:1")).toEqual({ tool: "Bash", detail: "bun scripts/todo.ts --list" });
    // A call whose arguments are its own business says only which tool it was.
    expect(await lastActionOf(archive, "chat:7:1:2")).toEqual({ tool: "Grep", detail: null });
    expect(await lastActionOf(archive, "chat:7:9:9")).toBeNull();
  });
});

