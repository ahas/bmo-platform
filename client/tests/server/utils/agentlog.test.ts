import { describe, expect, test } from "bun:test";

import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

import { toolOf } from "../../../apps/web/server/utils/agentlog";

import type { AgentLogPage } from "../../../apps/web/shared/types";

// An agent's log as the agency's fixed API holds it (the agency's `docs/api.md`, 「log-lines」): read by its run,
// after the last line a reader has. A page of the log across an agent's runs is a read the agency serves no route
// for, and answers as unavailable.

function line(id: number, agentName: string, kind: string, fields: Record<string, unknown> = {}, text = `line ${id}`): Record<string, unknown> {
  return { id, runId: 7, jobId: null, agentName, kind, text, at: 1_000 + id, fields };
}

// The run's lines: the agent's own, a co-worker's branch written under the same run, and a prompt standing whole.
const LINES = [
  line(1, "agent-a", "prompt", {}, "the whole prompt"),
  line(2, "agent-b", "text"),
  line(3, "agent-a", "tool", { name: "mcp__toolbook__call" }),
  line(4, "agent-a", "tool-result", { name: "Read" })
];

const agency = standIn("AGENCY", (call) => {
  const url = new URL(call.path, "http://agency");

  if (url.pathname === "/runs") return answer({ items: [{ id: 7, name: "run-a", agentName: "agent-a", unitName: "unit", workerName: null, jobId: null, state: "ended", startedAt: 1_000, endedAt: null, outcome: null, capEndsAt: null, keptUntil: null }], more: false });

  const after = Number(url.searchParams.get("after") ?? "0");
  const agent = url.searchParams.get("agentName");

  return answer({ items: LINES.filter((one) => (one.id as number) > after && (agent === null || one.agentName === agent)), more: false });
});
const handler = await routeOf<AgentLogPage>("../../apps/web/server/api/agents/[name]/log.get");

describe("an agent's log read by its run", () => {
  test("is the agent's own lines of that run, a prompt standing whole", async () => {
    const page = (await asked(handler, { params: { name: "agent-a" }, query: { run: "run-a" } })).answer!;

    expect(page.lines.map((one) => one.id)).toEqual([1, 3, 4]);
    expect(page.lines[0]).toMatchObject({ kind: "prompt", text: "the whole prompt", patches: [], patchesMore: 0 });
    expect(page.hasMore).toBe(false);
    expect(agency.asked.at(-1)?.path).toBe("/log-lines?runId=7&agentName=agent-a&after=0&limit=500");
  });

  test("is followed from the last line a reader has", async () => {
    const page = (await asked(handler, { params: { name: "agent-a" }, query: { run: "run-a", after: "3" } })).answer!;

    expect(page.lines.map((one) => one.id)).toEqual([4]);
  });

  test("says another page waits where it read more than one page asks for", async () => {
    const page = (await asked(handler, { params: { name: "agent-a" }, query: { run: "run-a", limit: "2" } })).answer!;

    expect(page.lines.map((one) => one.id)).toEqual([1, 3]);
    expect(page.hasMore).toBe(true);
  });
});

describe("an agent's log across its runs", () => {
  test("answers as unavailable, asking nothing of the agency", async () => {
    agency.asked.length = 0;

    expect((await asked(handler, { params: { name: "agent-a" }, query: {} })).answer).toEqual({ lines: [], hasMore: false, unavailable: true });
    expect(agency.asked).toEqual([]);
  });
});

describe("how a call shows its tool", () => {
  test("an MCP tool by its own name without its server, and every tool with the one generic icon", () => {
    expect(toolOf({ kind: "tool", name: "mcp__toolbook__call" })).toEqual({ name: "call", icon: null });
    expect(toolOf({ kind: "tool-result", name: "toolbook/call" })).toEqual({ name: "call", icon: null });
    expect(toolOf({ kind: "tool", name: "Read" })).toEqual({ name: "Read", icon: null });
  });

  test("a line about no tool shows none", () => {
    expect(toolOf({ kind: "text", name: "Read" })).toBeNull();
    expect(toolOf({ kind: "tool", name: null })).toBeNull();
  });
});
