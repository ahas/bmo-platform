import { PAGING } from "#shared/config";

import { mcpCallOf } from "#shared/mcp-names";

import { agencyRead } from "./agency";

import type { AgentLogLine, AgentLogPage, AgentLogTool } from "#shared/types";

// An agent's log as the agency's run log holds it (the agency's `docs/api.md`, 「Run log and run records」): a
// run's lines after the last one a reader has, five hundred a read. The log is read by its run, so a screen that
// opened at one run reads that run and follows it; the log of an agent across its runs — its newest page, the
// pages before it, the page at a moment — is a read the agency serves no route for, and stands unavailable
// (docs/boundary.md, 「Gaps the console meets」).

// The most lines one read of the agency answers.
const AGENCY_PAGE = 500;

// A line as the agency answers it.
interface RunLogLine extends Omit<AgentLogLine, "tool" | "patches" | "patchesMore"> {
  detail: string | null;
}

/** The page the log stands at where no run names it: none, since the agency reads a log by its run. */
export const UNAVAILABLE: AgentLogPage = { lines: [], hasMore: false, unavailable: true };

// One more line than asked for is read, and its presence is what says a page is waiting.
function pageWindow(limit: number): number {
  return Math.min(limit, PAGING.maxSize) + 1;
}

/** One run's lines of an agent's, oldest first, after `after` where it is given; `hasMore` says another page waits. */
export async function readRunPage(agent: string, run: string, after: number, limit: number): Promise<AgentLogPage> {
  const window = pageWindow(limit);
  const found: AgentLogLine[] = [];
  let cursor = after;

  // A run's lines are the agent's own but for a co-worker's branch written under the same run, so the read
  // goes on until a page of the agent's is full or the run has no more.
  for (;;) {
    const { lines } = await agencyRead<{ lines: RunLogLine[] }>(`/run-log?run=${encodeURIComponent(run)}&after=${cursor}`);

    for (const line of lines) {
      if (line.agent === agent) found.push(asLine(line));
    }

    if (found.length >= window || lines.length < AGENCY_PAGE) break;

    cursor = lines[lines.length - 1]!.id;
  }

  return { lines: found.slice(0, window - 1), hasMore: found.length > window - 1, hasOlder: false };
}

function asLine({ detail, ...line }: RunLogLine): AgentLogLine {
  return { ...line, text: line.kind === "prompt" && detail !== null ? detail : line.text, tool: toolOf(line), patches: [], patchesMore: 0 };
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
