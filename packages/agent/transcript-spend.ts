import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";

import { transcriptRecords } from "./transcript-tail";

import type { ModelSpend } from "./spend";

// What a Claude Code run spent, read off the transcript the CLI wrote for it rather than off its
// answer. A run answers with its own tally only when it ends by itself; one killed at its ceiling,
// stopped from outside or ending in output nobody can read never reaches that frame, and every
// request it made — its own and those of every Claude Code subagent it spawned — is still on the disk. The
// transcript is where those requests are written as they happen, so it is the one account of such
// a run there is.
//
// The session's own file and every file under its directory are read: Claude Code subagents, their own
// subagents and a Claude Workflow's agents all write beside the session that launched them. One request
// is written as several lines while it streams, all carrying the same message id; the input and
// cache figures are the same on every one of them while the output grows as the answer is
// written, so a request is counted once by that id and its output is the largest any of its lines
// states. A line the CLI wrote in place of an answer names
// the model `<synthetic>` and carried no traffic.
//
// Money is not in a transcript. What a run cost is the CLI's own figure and it is stated only in
// the frame such a run never wrote, so the rows this reads carry the four kinds as they happened
// and nought in money.

interface TranscriptLine {
  type?: string;
  timestamp?: string;
  message?: {
    id?: string;
    model?: string;
    role?: string;
    content?: unknown;
    usage?: Record<string, unknown>;
  };
}

const syntheticModel = "<synthetic>";

function count(usage: Record<string, unknown>, name: string): number {
  const value = usage[name];

  return typeof value === "number" && Number.isFinite(value) && value > 0 ? value : 0;
}

function transcriptsUnder(dir: string): string[] {
  if (!existsSync(dir)) {
    return [];
  }

  const found: string[] = [];

  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);

    if (entry.isDirectory()) {
      found.push(...transcriptsUnder(path));
    } else if (entry.name.endsWith(".jsonl")) {
      found.push(path);
    }
  }

  return found.sort();
}

// What a session standing across many runs has spent since one moment, out of its transcript and
// those of everything it spawned: what one job in a standing program has cost so far. Whoever asks
// asks again every few seconds while the job runs and a session's record runs to tens of megabytes,
// so the records are read as they grow; a request is placed by the moment it was written. The
// spawned records are read before the session's own, which keeps the largest file the latest read
// among those the tail reader holds.
export function transcriptSpendSince(transcript: string, sinceMs: number): ModelSpend[] {
  const spawned = transcriptsUnder(transcript.replace(/\.jsonl$/, ""));
  const records = [...spawned, transcript].flatMap((file) => transcriptRecords(file) as TranscriptLine[]);

  return spendOf(records.filter((record) => typeof record.timestamp === "string" && Date.parse(record.timestamp) >= sinceMs));
}

// The requests some transcript lines record, added up one entry a model.
function spendOf(records: readonly TranscriptLine[]): ModelSpend[] {
  const requests = new Map<string, { model: string; input: number; output: number; read: number; write: number }>();

  for (const line of records) {
    const message = line.message;

    if (line.type !== "assistant" || message?.usage === undefined || typeof message.id !== "string") {
      continue;
    }

    const model = message.model ?? "";

    if (model === "" || model === syntheticModel) {
      continue;
    }

    const output = count(message.usage, "output_tokens");
    const known = requests.get(message.id);

    if (known !== undefined) {
      known.output = Math.max(known.output, output);
      continue;
    }

    requests.set(message.id, {
      model,
      input: count(message.usage, "input_tokens"),
      output,
      read: count(message.usage, "cache_read_input_tokens"),
      write: count(message.usage, "cache_creation_input_tokens"),
    });
  }

  const byModel = new Map<string, ModelSpend>();

  for (const request of requests.values()) {
    const spend = byModel.get(request.model) ?? {
      mode: "claude-code",
      model: request.model,
      tokens: 0,
      inputTokens: 0,
      outputTokens: 0,
      cacheReadTokens: 0,
      cacheWriteTokens: 0,
      cost: 0,
    };

    spend.inputTokens += request.input;
    spend.outputTokens += request.output;
    spend.cacheReadTokens += request.read;
    spend.cacheWriteTokens += request.write;
    spend.tokens = spend.inputTokens + spend.outputTokens + spend.cacheReadTokens + spend.cacheWriteTokens;
    byModel.set(request.model, spend);
  }

  return [...byModel.values()];
}

