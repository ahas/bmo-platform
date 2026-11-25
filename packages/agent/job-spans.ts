// Where a job's time went, gathered from the run's own stream while it is being read.
//
// The figures are all in the stream already — a tool call and the result that answers it are a
// span with the run's own clock across them, and a Claude Workflow's branches are handed over with
// their seconds and their tokens stated outright. What they are not is kept: the agent log they
// arrive beside is live material, capped per agent and swept by age, so an item opened a month
// later has none of those lines to read back. So the tally is made in the one pass the log
// already makes over the stream, and the job writes it down as it ends, where it stops being
// live material and becomes the item's record.

import spanWords from "@agency/policy/span-words.toml";
import { fillMarks } from "@agency/support/marks";
import { calledTool } from "./mcp-names";
import { branchTools } from "./runlog";
import { workflowTask } from "./workflow-log";

// A span's row and its kinds stand with the archive, which keeps them (`run-rows.ts`).
export type { JobSpan, JobSpanKind } from "@agency/archive/run-rows";
import type { JobSpan, JobSpanKind } from "@agency/archive/run-rows";

// The kind a job's own spending is filed under. It is a span of the job like the rest and is
// read back by the same call, which is why it lives here rather than in a table of its own; what
// makes it its own kind is that it measures no time at all, so nothing that adds seconds may
// pick it up.
export const usageKind: JobSpanKind = "usage";

// The four kinds for a span whose source states no division: a phase, a tool call, a co-worker
// branch. Written once so that a span built anywhere says the same nothing.
const noSplit = {
  inputTokens: null,
  outputTokens: null,
  cacheReadTokens: null,
  cacheWriteTokens: null,
};

// The word a Claude Workflow's branch is named by, standing in front of the number of the branch. It
// is the mark that says a span is one branch of a run rather than a tool call, so the name is
// written with it and read back by it.
export const branchWord = "branch";

// Whether a span's name is one a Claude Workflow branch was filed under. It is the one thing that
// tells a branch from a co-worker opened as a tool call, the two of them being agent spans
// alike, and the difference decides where the seconds sat: a branch ran inside the wait the
// main thread held on it, and a call the main thread made was the main thread itself.
export function isBranchName(name: string): boolean {
  return name.startsWith(`${branchWord} `);
}

export const runPhase = "Claude run";

// The same span seen across several runs, added up: one row per kind and name, longest first.
// It is what an item read back wants — an item tried three times spent its Bash time in three
// places and the reader is asking about the item — and the attempts stay readable one by one
// beside it.
export function mergeSpans(spans: readonly JobSpan[]): JobSpan[] {
  const totals = new Map<string, JobSpan>();

  for (const span of spans) {
    const key = `${span.kind} ${span.name}`;
    const held = totals.get(key);

    if (held === undefined) {
      totals.set(key, { ...span });
      continue;
    }

    held.seconds += span.seconds;
    held.times += span.times;
    held.tokens = added(held.tokens, span.tokens);
    held.inputTokens = added(held.inputTokens, span.inputTokens);
    held.outputTokens = added(held.outputTokens, span.outputTokens);
    held.cacheReadTokens = added(held.cacheReadTokens, span.cacheReadTokens);
    held.cacheWriteTokens = added(held.cacheWriteTokens, span.cacheWriteTokens);
  }

  return [...totals.values()].sort((left, right) => right.seconds - left.seconds);
}

// Two figures of the same kind added, where either may be missing. A missing one adds nothing
// and leaves the other standing, and two missing ones stay missing: nothing measured added to
// nothing measured is still nothing measured, and a nought there would read as a run that spent
// none.
function added(held: number | null, more: number | null): number | null {
  return more === null ? held : (held ?? 0) + more;
}

// A length of time as it is read out: seconds while a person still counts in seconds, minutes
// once they stop, and hours for a job that ran all evening, where the seconds say nothing.
export function spanClock(seconds: number): string {
  const whole = Math.max(0, Math.round(seconds));

  if (whole < 60) {
    return fillMarks(spanWords.clock.seconds, { seconds: String(whole) });
  }

  if (whole < 3600) {
    return fillMarks(spanWords.clock.minutes, { minutes: String(Math.floor(whole / 60)), seconds: String(whole % 60) });
  }

  return fillMarks(spanWords.clock.hours, { hours: String(Math.floor(whole / 3600)), minutes: String(Math.floor(whole / 60) % 60) });
}

// The tool the CLI runs a Claude Workflow with. Its call opens the branches and answers for all of
// them, so it is no span of its own: the branches are what the time went to, and counting the
// call as well would count that time twice.
const workflowTool = "Workflow";

// How much of a co-worker's instruction goes into its name. Enough to tell one branch of a run
// from another, and no more: the whole of what it was asked is in the log while the log lasts.
const namedInstruction = 60;

interface SpanBlock {
  type?: string;
  name?: string;
  id?: string;
  input?: Record<string, unknown>;
  tool_use_id?: string;
}

interface SpanBranch {
  index?: number;
  label?: string;
  agentId?: string;
  model?: string;
  tokens?: number;
  durationMs?: number;
}

interface SpanEvent {
  type?: string;
  subtype?: string;
  parent_tool_use_id?: string | null;
  task_id?: string;
  tool_use_id?: string;
  task_type?: string;
  workflow_progress?: SpanBranch[];
  message?: { content?: SpanBlock[] };
}

interface OpenCall {
  kind: JobSpanKind;
  name: string;
  at: number;
}

// The tally one run leaves. It is fed the same events the log is and keeps only what a clock
// says about them, so a run killed part-way hands over everything that had already closed.
export class SpanTally {
  private readonly clock: () => number;
  // The calls that have gone out and not yet been answered, by the id of the call.
  private readonly open = new Map<string, OpenCall>();
  // What has been answered, by kind and name.
  private readonly totals = new Map<string, JobSpan>();
  // The one call that opened each Claude Workflow, under every id an event about it may arrive under.
  // The CLI names a task by its own id in one event and by the call that launched it in
  // another, so a branch keyed on whichever id came along would be two branches; every id of a
  // Claude Workflow points at the same call here, and that call is what a branch is filed under. The
  // CLI runs more than one kind of task in the background and only this one has branches, so a
  // task whose start did not say it was a Claude Workflow is not in here at all.
  private readonly workflows = new Map<string, string>();
  // One branch's own figures, by the task it belongs to and its place in it. The stream repeats
  // the whole picture of a Claude Workflow in every progress event, so the last word about a branch is
  // what stands rather than being added to what came before.
  private readonly branches = new Map<string, JobSpan>();

  constructor(clock: () => number = Date.now) {
    this.clock = clock;
  }

  see(event: unknown): void {
    if (event === null || typeof event !== "object") {
      return;
    }

    const parsed = event as SpanEvent;

    if (parsed.type === "system") {
      this.task(parsed);

      return;
    }

    // A line made inside a branch belongs to that branch, and the branch is already a span of
    // its own: counting its tools here as well would put the same seconds at both levels.
    if (typeof parsed.parent_tool_use_id === "string") {
      return;
    }

    if (parsed.type === "assistant") {
      this.calls(parsed.message?.content ?? []);
    } else if (parsed.type === "user") {
      this.answers(parsed.message?.content ?? []);
    }
  }

  // Everything gathered: what a call was answered in, and what each Claude Workflow branch reported.
  spans(): JobSpan[] {
    return [...this.totals.values(), ...this.branches.values()];
  }

  private task(event: SpanEvent): void {
    if (event.subtype === "task_started") {
      if (event.task_type === workflowTask) {
        const call = event.tool_use_id ?? event.task_id ?? "";

        for (const id of idsOf(event)) {
          this.workflows.set(id, call);
        }
      }

      return;
    }

    const call = idsOf(event)
      .map((id) => this.workflows.get(id))
      .find((found) => found !== undefined);

    if (event.subtype !== "task_progress" || call === undefined) {
      return;
    }

    for (const branch of event.workflow_progress ?? []) {
      // A branch is worth a span once the CLI has said how long it ran. Until then the stream
      // says only that it is going, and a span with no seconds in it says nothing.
      if (branch.durationMs === undefined) {
        continue;
      }

      // The CLI states a branch's tokens as one number and never says how it divided, so the
      // branch keeps the one number and carries no split at all.
      this.branches.set(`${call}:${branch.index ?? branch.agentId ?? ""}`, {
        kind: "agent",
        name: branchName(branch),
        seconds: branch.durationMs / 1000,
        times: 1,
        tokens: branch.tokens ?? null,
        ...noSplit,
      });
    }
  }

  private calls(blocks: SpanBlock[]): void {
    for (const block of blocks) {
      // A call made through the toolbook is timed under the tool it reached.
      const called = calledTool(block.name ?? "", block.input ?? {});
      const name = called.name;

      if (block.type !== "tool_use" || block.id === undefined || name === "" || name === workflowTool) {
        continue;
      }

      this.open.set(block.id, {
        kind: branchTools.has(name) ? "agent" : "tool",
        name: branchTools.has(name) ? coworkerName(name, called.input as Record<string, unknown>) : name,
        at: this.clock(),
      });
    }
  }

  private answers(blocks: SpanBlock[]): void {
    for (const block of blocks) {
      const id = block.tool_use_id;

      if (block.type !== "tool_result" || id === undefined) {
        continue;
      }

      const call = this.open.get(id);

      if (call === undefined) {
        continue;
      }

      this.open.delete(id);
      this.add(call, (this.clock() - call.at) / 1000);
    }
  }

  private add(call: OpenCall, seconds: number): void {
    const key = `${call.kind} ${call.name}`;
    const held = this.totals.get(key);

    if (held === undefined) {
      this.totals.set(key, { kind: call.kind, name: call.name, seconds, times: 1, tokens: null, ...noSplit });

      return;
    }

    held.seconds += seconds;
    held.times += 1;
  }
}

// Which ids an event of a background task names. A task is known by its own id, and the call
// that launched it answers for it as well, so either identifies the same task.
function idsOf(event: SpanEvent): string[] {
  return [event.task_id, event.tool_use_id].filter((id): id is string => typeof id === "string");
}

// A co-worker by the name a reader knows it under: the kind of branch it is, whose it is, and
// enough of what it was asked to tell it from the one beside it. The model is not there to
// carry — the CLI names none for a branch it opens as a tool, since the agent's own definition
// pins it — and a Claude Workflow branch, which is handed one, keeps it.
export function coworkerName(tool: string, input: Record<string, unknown>): string {
  const kind = input["subagent_type"];
  const said = input["description"];
  const head = typeof kind === "string" && kind !== "" ? `${tool}(${kind})` : tool;

  return typeof said === "string" && said !== "" ? `${head} · ${clip(said)}` : head;
}

// What a branch of a Claude Workflow is filed under: which branch of the run it was, what it was there
// to do, and what it ran on. The three are written into one name because that is what a span is
// keyed by, and whoever reads it back tells the parts apart by this same word standing in front.
export function branchName(branch: SpanBranch): string {
  return [`${branchWord} ${branch.index ?? branch.agentId ?? "?"}`, branch.label ?? "", branch.model ?? ""]
    .filter((part) => part !== "")
    .join(" · ");
}

function clip(text: string): string {
  const flat = text.replace(/\s+/g, " ").trim();

  return flat.length <= namedInstruction ? flat : `${flat.slice(0, namedInstruction)}…`;
}
