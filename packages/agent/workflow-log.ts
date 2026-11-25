// What a workflow a run launched leaves in that run's log. A workflow is not a chain of tool
// calls the stream carries: the CLI runs it as a background task of its own, and the stream says
// only that a task started, how its branches are getting on, and that it ended. So the branches
// are read out of those task events, and what each branch actually did — the tools it called,
// their arguments and what came back — is read afterwards from the transcript the workflow keeps
// for it, which is the only place that is written down.

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { runLogLines } from "./runlog";

import type { RunLogLine } from "./runlog";

// One branch's own record: the file the workflow wrote for it, read as the entries it holds.
export interface BranchTranscript {
  agentId: string;
  entries: unknown[];
}

export type TranscriptReader = (dir: string) => BranchTranscript[];

// How many lines one branch may put into the log. A workflow that fans wide would otherwise
// crowd the run's own lines out of the table long before the age or the count did.
export const maxBranchLines = 300;

// The task kind the CLI gives a workflow. Its other background tasks are somebody else's
// business and leave nothing here.
export const workflowTask = "local_workflow";

// Where the workflow keeps its branches' records: the tool that launched it says so in the very
// answer it gives back, so nothing has to be guessed from a session id or a directory listing.
const transcriptDirPattern = /Transcript dir:\s*(\S+)/;

interface WorkflowAgent {
  index?: number;
  label?: string;
  agentId?: string;
  model?: string;
  state?: string;
  promptPreview?: string;
  resultPreview?: string;
  tokens?: number;
  toolCalls?: number;
  durationMs?: number;
}

interface TaskEvent {
  type?: string;
  subtype?: string;
  task_id?: string;
  tool_use_id?: string;
  task_type?: string;
  workflow_name?: string;
  description?: string;
  prompt?: string;
  status?: string;
  summary?: string;
  usage?: { total_tokens?: number; tool_uses?: number; duration_ms?: number };
  workflow_progress?: WorkflowAgent[];
  message?: { content?: unknown };
}

// The trail one run's workflows leave. It holds what the stream will not repeat — which call a
// task belongs to, where its branches' records are, and what has already been said about each
// branch — so a line is written once and no state has to be dug out of the log again.
export class WorkflowTrail {
  private readonly read: TranscriptReader;
  // The launching call's id, by task, so every line of a workflow sits under that call.
  private readonly calls = new Map<string, string>();
  // The calls that opened a workflow. The CLI runs its other background tasks — a command left
  // running behind a job, above all — through the same three events, and those say a task
  // ended, not that a workflow did. A task is a workflow here only because its start said so,
  // so this is what every later event about it is held against.
  private readonly opened = new Set<string>();
  // Where a call's branches are written down, by the id of that call.
  private readonly dirs = new Map<string, string>();
  // What was last said about a branch, so a progress event that repeats itself says nothing.
  // A run launches more than one workflow, and two of them number their branches from one
  // alike, so everything a branch is known by is held under the call it belongs to as well.
  private readonly states = new Map<string, string>();
  // What each branch is called on the board and where it stands in the workflow, so its own
  // record reads under the same name and in the same order.
  private readonly labels = new Map<string, string>();
  private readonly places = new Map<string, number>();

  constructor(read: TranscriptReader = readBranchTranscripts) {
    this.read = read;
  }

  lines(event: unknown): RunLogLine[] {
    if (event === null || typeof event !== "object") {
      return [];
    }

    const parsed = event as TaskEvent;

    if (parsed.type === "user") {
      this.rememberDir(parsed);

      return [];
    }

    if (parsed.type !== "system") {
      return [];
    }

    if (parsed.subtype === "task_started") {
      return this.started(parsed);
    }

    if (parsed.subtype === "task_progress") {
      return this.progress(parsed);
    }

    if (parsed.subtype === "task_notification") {
      return this.finished(parsed);
    }

    return [];
  }

  // The launch answer names the directory the branches are written to. It is an ordinary tool
  // result, already a line of its own in the log; this only takes the path out of it.
  private rememberDir(event: TaskEvent): void {
    for (const block of blocksOf(event.message?.content)) {
      const found = transcriptDirPattern.exec(block.text);

      if (found?.[1] !== undefined && block.toolUseId !== null) {
        this.dirs.set(block.toolUseId, found[1]);
      }
    }
  }

  private started(event: TaskEvent): RunLogLine[] {
    if (event.task_type !== workflowTask || event.tool_use_id === undefined) {
      return [];
    }

    if (event.task_id !== undefined) {
      this.calls.set(event.task_id, event.tool_use_id);
    }

    this.opened.add(event.tool_use_id);

    const head = ["workflow", event.workflow_name ?? "", event.description ?? ""].filter((part) => part !== "");
    const script = typeof event.prompt === "string" ? event.prompt : "";

    return [
      {
        kind: script === "" ? "note" : "prompt",
        parent: event.tool_use_id,
        text: script === "" ? head.join(" · ") : `${head.join(" · ")}\n${script}`,
      },
    ];
  }

  // Every branch's own start and end, once each. The stream repeats the whole picture of a
  // workflow in every progress event, so what has already been written is what tells them apart.
  private progress(event: TaskEvent): RunLogLine[] {
    const call = this.callOf(event);

    if (call === null) {
      return [];
    }

    const lines: RunLogLine[] = [];

    for (const branch of event.workflow_progress ?? []) {
      // A branch is known by its place in the workflow it belongs to: its own id is not there
      // in the first event about it, and keying on that alone would say the same start twice.
      const id = `${call}:${branch.index === undefined ? (branch.agentId ?? "") : branch.index}`;
      const state = branch.state ?? "";

      if (state === "" || this.states.get(id) === state) {
        continue;
      }

      this.states.set(id, state);

      if (branch.agentId !== undefined) {
        if (branch.label !== undefined) {
          this.labels.set(`${call}:${branch.agentId}`, branch.label);
        }

        if (branch.index !== undefined) {
          this.places.set(`${call}:${branch.agentId}`, branch.index);
        }
      }

      lines.push({ kind: "note", parent: call, text: branchText(branch, state) });
    }

    return lines;
  }

  // The end of the workflow, and then what its branches did. Nothing of that is in the stream,
  // so it is read from the records the workflow kept — which are complete only now that it is
  // over.
  private finished(event: TaskEvent): RunLogLine[] {
    const call = this.callOf(event);

    if (call === null) {
      return [];
    }

    const dir = this.dirs.get(call);
    const tally = [
      event.status === undefined ? "" : `workflow ${event.status}`,
      event.summary ?? "",
      usageText(event.usage),
    ].filter((part) => part !== "");

    const lines: RunLogLine[] = [{ kind: "note", parent: call, text: tally.join(" · ") }];

    if (dir === undefined) {
      return [...lines, { kind: "note", parent: call, text: "Where the branch's record is cannot be told, so what it did is not kept." }];
    }

    return [...lines, ...this.branchLines(call, dir)];
  }

  private branchLines(call: string, dir: string): RunLogLine[] {
    let transcripts: BranchTranscript[];

    try {
      transcripts = this.read(dir);
    } catch (error) {
      return [{ kind: "note", parent: call, text: `The branch's record could not be read: ${String(error)}` }];
    }

    if (transcripts.length === 0) {
      return [{ kind: "note", parent: call, text: "The branch left no record." }];
    }

    // The records are read in the order the workflow opened its branches, not the order the
    // files happen to be named in.
    const ordered = [...transcripts].sort(
      (left, right) =>
        (this.places.get(`${call}:${left.agentId}`) ?? 0) - (this.places.get(`${call}:${right.agentId}`) ?? 0),
    );

    return ordered.flatMap((transcript) => this.oneBranch(call, transcript));
  }

  private oneBranch(call: string, transcript: BranchTranscript): RunLogLine[] {
    const label = this.labels.get(`${call}:${transcript.agentId}`) ?? transcript.agentId;
    const inside: RunLogLine[] = [];

    for (const entry of transcript.entries) {
      // The first thing said to a branch is its instruction, which stands in the head below;
      // everything else is what the branch did, in the same shape a stream carries.
      if (!Array.isArray((entry as { message?: { content?: unknown } }).message?.content)) {
        continue;
      }

      for (const line of runLogLines(entry)) {
        inside.push({ ...line, parent: call });
      }
    }

    const head: RunLogLine = {
      kind: "note",
      parent: call,
      text: `branch ${label} · ${instructionOf(transcript.entries)}`,
    };

    if (inside.length <= maxBranchLines) {
      return [head, ...inside];
    }

    return [
      head,
      ...inside.slice(0, maxBranchLines),
      { kind: "note", parent: call, text: `branch ${label}: ${inside.length - maxBranchLines} lines past the limit were not kept.` },
    ];
  }

  // Which workflow call an event belongs to, and null for an event that belongs to none. The
  // id alone does not answer it: every background task carries one, so what makes a call a
  // workflow's is that a start of the workflow kind was seen for it.
  private callOf(event: TaskEvent): string | null {
    const named =
      typeof event.tool_use_id === "string"
        ? event.tool_use_id
        : event.task_id === undefined
          ? null
          : (this.calls.get(event.task_id) ?? null);

    return named !== null && this.opened.has(named) ? named : null;
  }
}

// One branch's line on the board: what it was asked, on what model, and — once it is over —
// what it cost and what it handed back.
function branchText(branch: WorkflowAgent, state: string): string {
  const head = [`branch ${branch.index ?? "?"}`, branch.label ?? "", branch.model ?? "", state]
    .filter((part) => part !== "")
    .join(" · ");
  const tail = [
    branch.durationMs === undefined ? "" : `${Math.round(branch.durationMs / 1000)}s`,
    branch.tokens === undefined ? "" : `${branch.tokens} tokens`,
    branch.toolCalls === undefined ? "" : `${branch.toolCalls} tool calls`,
  ]
    .filter((part) => part !== "")
    .join(" · ");
  const said = branch.state === "start" ? (branch.promptPreview ?? "") : (branch.resultPreview ?? "");

  return [tail === "" ? head : `${head} · ${tail}`, said].filter((part) => part !== "").join("\n");
}

function usageText(usage: TaskEvent["usage"]): string {
  if (usage === undefined) {
    return "";
  }

  return [
    usage.duration_ms === undefined ? "" : `${Math.round(usage.duration_ms / 1000)}s`,
    usage.total_tokens === undefined ? "" : `${usage.total_tokens} tokens`,
    usage.tool_uses === undefined ? "" : `${usage.tool_uses} tool calls`,
  ]
    .filter((part) => part !== "")
    .join(" · ");
}

// What a branch was told to do: the first thing said to it, which is the prompt the workflow
// handed over.
export function instructionOf(entries: readonly unknown[]): string {
  for (const entry of entries) {
    const content = (entry as { type?: string; message?: { content?: unknown } }).message?.content;

    if ((entry as { type?: string }).type === "user" && typeof content === "string") {
      return content;
    }
  }

  return "No instruction was found.";
}

interface ResultBlock {
  text: string;
  toolUseId: string | null;
}

function blocksOf(content: unknown): ResultBlock[] {
  if (!Array.isArray(content)) {
    return [];
  }

  return content
    .filter((block): block is { type?: string; content?: unknown; tool_use_id?: string } => typeof block === "object" && block !== null)
    .filter((block) => block.type === "tool_result")
    .map((block) => ({
      text: typeof block.content === "string" ? block.content : JSON.stringify(block.content ?? ""),
      toolUseId: typeof block.tool_use_id === "string" ? block.tool_use_id : null,
    }));
}

// The records a workflow keeps: one file to a branch, each holding that branch's whole
// conversation in the shape the stream uses, which is why the same reading serves both.
export function readBranchTranscripts(dir: string): BranchTranscript[] {
  if (!existsSync(dir)) {
    return [];
  }

  return readdirSync(dir)
    .filter((name) => name.startsWith("agent-") && name.endsWith(".jsonl"))
    .sort()
    .map((name) => ({
      agentId: name.slice("agent-".length, -".jsonl".length),
      entries: readFileSync(join(dir, name), "utf8")
        .split("\n")
        .map((line) => line.trim())
        .filter((line) => line.startsWith("{"))
        .flatMap((line) => {
          try {
            return [JSON.parse(line) as unknown];
          } catch {
            return [];
          }
        }),
    }));
}
