import { PAGING } from "#shared/config";

import { mcpCallOf } from "#shared/mcp-names";

import { agency, agencyCall } from "./agency";

import type { LogLine } from "@agency/client";
import type { AgentLogLine, AgentLogPage, AgentLogState, AgentLogTool } from "#shared/types";

// An agent's log as the agency's fixed API holds it (the agency's `docs/api.md`, 「log-lines」): a run's lines after
// the last one a reader has, five hundred a read, narrowed to the agent's own. The log is read by its run, so a
// screen that opened at one run reads that run and follows it; the log of an agent across its runs — its newest
// page, the pages before it, the page at a moment — is a read the agency serves no route for, and stands
// unavailable (docs/boundary.md, 「Gaps the console meets」).

// The most lines one read of the agency answers.
const AGENCY_PAGE = 500;

/** The page the log stands at where no run names it: none, since the agency reads a log by its run. */
export const UNAVAILABLE: AgentLogPage = { lines: [], hasMore: false, unavailable: true };

// One more line than asked for is read, and its presence is what says a page is waiting.
function pageWindow(limit: number): number {
  return Math.min(limit, PAGING.maxSize) + 1;
}

// The run a screen names by its own name, as the agency numbers it.
async function runIdOf(run: string): Promise<number | null> {
  const page = await agencyCall(agency().GET("/runs", { params: { query: { names: run, limit: 1 } } }));

  return page.items[0]?.id ?? null;
}

/** One run's lines of an agent's, oldest first, after `after` where it is given; `hasMore` says another page waits. */
export async function readRunPage(agent: string, run: string, after: number, limit: number): Promise<AgentLogPage> {
  const runId = await runIdOf(run);

  if (runId === null) return UNAVAILABLE;

  const window = pageWindow(limit);
  const found: AgentLogLine[] = [];
  let cursor = after;

  for (;;) {
    const page = await agencyCall(
      agency().GET("/log-lines", { params: { query: { runId, agentName: agent, after: cursor, limit: AGENCY_PAGE } } })
    );

    found.push(...page.items.map(asLine));

    if (found.length >= window || !page.more || page.items.length === 0) break;

    cursor = page.items[page.items.length - 1]!.id;
  }

  return { lines: found.slice(0, window - 1), hasMore: found.length > window - 1, hasOlder: false };
}

// The fields a line's writer kept beside it, read by name where they are there.
function fieldOf<T>(line: LogLine, name: string, read: (value: unknown) => T | null): T | null {
  const fields = line.fields;

  return typeof fields === "object" && fields !== null && name in fields ? read((fields as Record<string, unknown>)[name]) : null;
}

function stringField(line: LogLine, name: string): string | null {
  return fieldOf(line, name, (value) => (typeof value === "string" ? value : null));
}

function asLine(line: LogLine): AgentLogLine {
  return {
    id: line.id,
    agent: line.agentName,
    run: stringField(line, "run") ?? String(line.runId),
    chatId: fieldOf(line, "chatId", (value) => (typeof value === "number" ? value : null)),
    at: Math.floor(line.at / 1000),
    kind: line.kind as AgentLogLine["kind"],
    name: stringField(line, "name"),
    tool: toolOf({ kind: line.kind as AgentLogLine["kind"], name: stringField(line, "name") }),
    parent: stringField(line, "parent"),
    ref: stringField(line, "ref"),
    state: fieldOf(line, "state", (value) => (typeof value === "string" ? (value as AgentLogState) : null)),
    truncated: fieldOf(line, "truncated", (value) => value === true) === true,
    text: line.text,
    patches: [],
    patchesMore: 0
  };
}

/**
 * How a call and its result show their tool: the tool's own name — an MCP tool's read out of the name its road
 * logged it under, without the server. Which icon a tool declares is the tool host's to say, and no unit hands
 * it to the console yet, so every tool takes the one generic icon.
 */
export function toolOf(line: Pick<AgentLogLine, "kind" | "name">): AgentLogTool | null {
  if (line.name === null || (line.kind !== "tool" && line.kind !== "tool-result")) {
    return null;
  }

  return { name: mcpCallOf(line.name)?.tool ?? line.name, icon: null };
}
