// The agent log: one line for everything an agent did, written to the archive as it happens.
// Whoever reads a run's stream writes it — the bot for a chat session and a gate, the runner or
// the worker for itself — so the whole of what agency's agents do lands in one table under each
// agent's own name, and the web reads that table directly. A line is committed the moment it
// arrives, so a run killed from outside leaves everything up to the kill, and the ids run
// forward, so a finished run is scrolled back rather than only tailed.

import { readFileSync } from "node:fs";

import { keptThroughLock } from "@agency/archive/archive-lock";
import { CodexFileChanges, streamPaths } from "./codex-file-changes";
import { calledTool, type CalledTool } from "./mcp-names";
import { deploymentNames } from "@agency/support/names";
import { SpanTally } from "./job-spans";
import { maskedValue, maskSecrets, readsSecretFile } from "@agency/support/secrets";
import { WorkflowTrail } from "./workflow-log";

import type { Archive } from "@agency/archive/archive";
import type { NewAgentLogLine } from "@agency/archive/run-store";
import type { AgentProviderName, HarnessRun } from "./harness";
import type { JobSpan } from "./job-spans";

/** Which sort of thing entered a turn, as the caller that composed the turn names it. */
export type ContextSource = "document" | "person" | "rule" | "situation" | "knowledge" | "memory" | "room" | "archive";

/**
 * How a reference document went in, as the caller that composed the turn says: `attached` where its
 * full text went with the turn, `held` where the session already held that version and a mark went
 * instead.
 */
export type ContextState = "attached" | "held";

/** One thing the turn was handed, as its caller names it. */
export interface TurnContext {
  source: ContextSource;
  /** What identifies it: a slug or path, a person's sheet, a range of messages with its count. */
  text: string;
  /** Settled once the request is opened for the session it continues, never while it is built. */
  state?: ContextState;
}

// `context` is what the turn was handed before it ran — a document put in front of it, a person's
// sheet, messages read back out of the archive — and its name is which sort of thing that was.
export type RunLogKind =
  | "start"
  | "thinking"
  | "text"
  | "tool"
  | "tool-result"
  | "prompt"
  | "context"
  | "note"
  | "end";

// One line before it is written: what it is, what it says, and the ties that let a viewer draw
// it in place — the tool call it is or answers, and the Task branch it came out of.
export interface RunLogLine {
  kind: RunLogKind;
  text: string;
  name?: string | null;
  ref?: string | null;
  parent?: string | null;
  source?: RunLogSource;
  sourceEvent?: number | null;
  sourceBlock?: number | null;
  sourceId?: string | null;
  state?: RunLogState | null;
  upstreamTruncated?: boolean;
  metadata?: unknown;
}

// Where a line came from: the mode its run was taken in, lines written before a source was
// recorded ("legacy"), or a name its writer gives. Which names are used is the writer's own.
export type RunLogSource = RunLogMode | "legacy" | (string & {});
export type RunLogState = "requested" | "running" | "succeeded" | "failed" | "unverified" | "unavailable" | ContextState;

export interface RunLogTarget {
  archive: Archive;
  // The agent's internal name: chat:<chatId>, a gate's stage, a runner's, the worker's.
  agent: string;
  // Which single run of that agent the lines belong to, so a viewer can draw a boundary where
  // it changes: a unit name, a gate run's id, a chat turn's chat and second.
  run: string;
  chatId: number | null;
  // Only a worker job has this. Its lines keep complete, lazy detail and do not take part in the
  // rolling agent-log prune; every other agent keeps the existing lightweight log.
  jobId?: number | null;
  // Whether this agent's log keeps whole what each run was handed: every prompt uncut, and the
  // system prompt once for each version of it. The agent decides it for itself — the supervisor does,
  // so its screen reads what the session was actually given; a gate's bulk input stays a preview.
  keepsLaunch?: boolean;
}

// The name a system prompt line carries, so a reader tells it apart from a turn's prompt.
export const systemPromptLine = "system";

// What one line may carry. What a run or the bot wrote in its own words — a job's report, a
// failure reason, a gate's verdict — is what a person reads, and it is kept whole. A tool's
// arguments, what the tool handed back and a prompt handed to a model are bulk: their list preview
// is cut. A worker job retains the complete value separately for bounded detail reads; other agent
// logs keep only the preview they have always kept.
export const maxToolChars = 2000;
export const maxPromptChars = 4000;
export const maxJobPreviewChars = 4000;

// Detail offsets are Unicode code points, and a list preview must not tear one in half before a
// reader asks for that detail. `String#slice` measures UTF-16 code units, which would turn an
// emoji precisely on a preview boundary into an unpaired surrogate.
function preview(text: string, limit: number): { text: string; truncated: boolean } {
  if (!Number.isFinite(limit)) return { text, truncated: false };
  let end = 0;
  let characters = 0;

  for (const character of text) {
    if (characters === limit) return { text: text.slice(0, end), truncated: true };
    end += character.length;
    characters += 1;
  }

  return { text, truncated: false };
}

function lineCap(kind: RunLogKind, keepsLaunch: boolean): number {
  if (kind === "tool" || kind === "tool-result") {
    return maxToolChars;
  }

  return kind === "prompt" && !keepsLaunch ? maxPromptChars : Number.POSITIVE_INFINITY;
}

// What the table keeps: the newest lines of each agent, and nothing at all past the age.
export const maxLinesPerAgent = 4000;
// And the text they may hold between them. A log that keeps whole prompts grows by the size of a
// room's context each turn, so the line count alone no longer bounds it.
export const maxBytesPerAgent = 64 * 1024 * 1024;
export const maxAgeSeconds = 7 * 24 * 60 * 60;
// A run writes as it goes and may never reach an end, so the pruning goes with the writing
// rather than waiting for one.
export const pruneEveryLines = 200;

// The shapes the CLI's stream-json carries that a log line is made of. Everything else in an
// event is somebody else's business — the usage counts, the rate limits, the background tasks —
// and is read where that business lives.
interface StreamContentBlock {
  type?: string;
  text?: string;
  thinking?: string;
  name?: string;
  id?: string;
  input?: Record<string, unknown>;
  tool_use_id?: string;
  content?: unknown;
  is_error?: boolean;
  truncated?: boolean;
  is_truncated?: boolean;
  output_truncated?: boolean;
}

interface StreamEvent {
  type?: string;
  subtype?: string;
  parent_tool_use_id?: string | null;
  model?: string;
  tools?: string[];
  is_error?: boolean;
  isApiErrorMessage?: boolean;
  result?: string;
  uuid?: string;
  toolUseResult?: unknown;
  truncated?: boolean;
  is_truncated?: boolean;
  output_truncated?: boolean;
  message?: { id?: string; content?: StreamContentBlock[] };
}

// What one event of a run's stream leaves in the log. Nothing else produces a line: an event
// that says nothing about what the run did is not a thing the run did.
// The provider a run went out on. Only Claude Code and Codex write an event stream this log reads;
// every other road's output is its answer text, which the caller writes down itself.
export type RunLogMode = AgentProviderName;

export function runLogLines(event: unknown, mode: RunLogMode = "claude-code"): RunLogLine[] {
  if (mode === "codex") return codexLogLines(event);
  if (mode !== "claude-code") return [];
  if (event === null || typeof event !== "object") {
    return [];
  }

  const parsed = event as StreamEvent;
  // A line made inside a Task call belongs to that branch, and the id of the call is what ties
  // it there, so the branch reads on its own instead of being interleaved with the top level.
  const parent = typeof parsed.parent_tool_use_id === "string" ? parsed.parent_tool_use_id : null;

  return linesOf(parsed).map((line) => ({ ...line, parent }));
}

// Codex exec emits completed items, not Claude message blocks. Only completed messages are
// recorded: item updates repeat their accumulated text. Tool starts and results share the item ID.
function codexLogLines(event: unknown): RunLogLine[] {
  if (event === null || typeof event !== "object") return [];
  const e = event as { type?: string; thread_id?: string; message?: string; error?: { message?: string }; item?: Record<string, unknown> };
  if (e.type === "thread.started") return [{ kind: "note", text: `Codex thread ${e.thread_id ?? ""}`, sourceId: e.thread_id ?? null }];
  if (e.type === "turn.failed" || e.type === "error") {
    return [{ kind: "note", text: e.error?.message ?? e.message ?? "Codex error", state: "failed" }];
  }
  const item = e.item;
  if (!item || (e.type !== "item.started" && e.type !== "item.completed")) return [];
  const completed = e.type === "item.completed";
  const text = typeof item.text === "string" ? item.text : "";
  const ref = typeof item.id === "string" ? item.id : null;
  if (item.type === "agent_message" || item.type === "reasoning") {
    return completed && text !== ""
      ? [{ kind: item.type === "reasoning" ? "thinking" : "text", text, sourceId: ref, upstreamTruncated: wasUpstreamTruncated(item) }]
      : [];
  }
  const types: Record<string, string> = { command_execution: "Bash", mcp_tool_call: "MCP", file_change: "Edit", web_search: "web_search", collab_tool_call: "Agent" };
  const type = String(item.type ?? "");
  const name = types[type];
  if (name === undefined) return [];
  if (!completed) {
    const called = type === "mcp_tool_call" ? calledTool(`${item.server ?? "mcp"}/${item.tool ?? "tool"}`, item.arguments) : null;

    return [{ kind: "tool", name: called?.name ?? name, ref,
      sourceId: ref, state: "running", upstreamTruncated: wasUpstreamTruncated(item),
      text: JSON.stringify(type === "command_execution" ? { command: item.command } : called?.input ?? item.arguments ?? item.changes ?? item),
      ...(called === null ? {} : toolbookMark(called)) }];
  }
  const result = item.aggregated_output ?? item.result ?? item.error ?? item.changes ?? item;
  return [{
    kind: "tool-result",
    ref,
    name,
    sourceId: ref,
    state: codexResultState(item),
    upstreamTruncated: wasUpstreamTruncated(item),
    metadata: { item },
    text: typeof result === "string" ? result : JSON.stringify(result),
  }];
}

function codexResultState(item: Record<string, unknown>): RunLogState {
  const status = typeof item.status === "string" ? item.status.toLowerCase() : "";
  const exitCode = typeof item.exit_code === "number" ? item.exit_code
    : typeof item.exitCode === "number" ? item.exitCode
    : null;

  if (item.error !== undefined || ["failed", "error", "errored", "cancelled"].includes(status) || (exitCode !== null && exitCode !== 0)) {
    return "failed";
  }

  if (["completed", "succeeded", "success"].includes(status) || exitCode === 0) {
    return "succeeded";
  }

  return "unverified";
}

function linesOf(event: StreamEvent): RunLogLine[] {
  if (event.type === "system" && event.subtype === "init") {
    return [{ kind: "start", text: startText(event) }];
  }

  if (event.type === "assistant") {
    return assistantLines(Array.isArray(event.message?.content) ? event.message.content : [],event.message?.id ?? event.uuid ?? null, event.isApiErrorMessage === true);
  }

  if (event.type === "user") {
    // A plain prompt turn carries its content as a string, and it holds no tool result.
    return toolResultLines(event, Array.isArray(event.message?.content) ? event.message.content : [],event.message?.id ?? event.uuid ?? null);
  }

  if (event.type === "result") {
    return [{ kind: "end", text: endText(event), state: event.is_error === true ? "failed" : "succeeded" }];
  }

  return [];
}

function startText(event: StreamEvent): string {
  const tools = event.tools ?? [];

  return [event.model ?? "", tools.length === 0 ? "" : `tools: ${tools.join(", ")}`]
    .filter((part) => part !== "")
    .join(" · ");
}

function endText(event: StreamEvent): string {
  const outcome = event.is_error === true ? "error" : "done";
  const result = typeof event.result === "string" ? event.result.trim() : "";

  return result === "" ? outcome : `${outcome}\n${result}`;
}

function assistantLines(blocks: StreamContentBlock[], sourceId: string | null, failed: boolean): RunLogLine[] {
  const lines: RunLogLine[] = [];

  for (const [sourceBlock, block] of blocks.entries()) {
    if (block.type === "text" && typeof block.text === "string") {
      lines.push({ kind: "text", text: block.text, sourceBlock, sourceId, ...(failed ? { state: "failed" as const } : {}) });
    } else if (block.type === "thinking" && typeof block.thinking === "string" && block.thinking !== "") {
      // A thinking block reaches the stream carrying its signature and nothing else whenever
      // the run's thinking is not surfaced, and an empty line says nothing anyone can read.
      lines.push({ kind: "thinking", text: block.thinking, sourceBlock, sourceId, ...(failed ? { state: "failed" as const } : {}) });
    } else if (block.type === "tool_use") {
      const called = calledTool(block.name ?? "", block.input ?? {});

      lines.push({
        kind: "tool",
        name: toolName(called),
        ref: block.id ?? null,
        sourceBlock,
        sourceId,
        state: "requested",
        upstreamTruncated: wasUpstreamTruncated(block),
        text: JSON.stringify(called.input),
        ...toolbookMark(called),
      });
    }
  }

  return lines;
}

// A branch is read by whose it is, so the call that opens one carries the kind of co-worker it
// opened in the tool's own name; what it was asked to do stays in the arguments with everything
// else. Which tool opens a branch is the CLI's to name and it answers to more than one name, so
// both are known here — but nothing depends on this: what ties a line to its branch is the id
// of the call it came out of, whatever that call was called. A search's pooled branch is a
// call to its web server's branch tool; that branch runs in a process of its own, so its stream
// holds the call and the report it returned rather than the branch's own queries.
export const branchTools = new Set(["Task", "Agent", "mcp__web__research_branch"]);

// A logged call's MCP server and tool, read back out of the names this file writes (`mcp-names.ts`).
export { mcpCallOf, type LoggedMcpCall } from "./mcp-names";

// A call made through the toolbook is written as the tool it reached; the line's metadata keeps
// that it went through the toolbook, since its name and arguments no longer say so.
function toolbookMark(called: CalledTool): { metadata?: { throughToolbook: true } } {
  return called.throughToolbook ? { metadata: { throughToolbook: true } } : {};
}

function toolName(called: CalledTool): string {
  const name = called.name;
  const input = called.input as Record<string, unknown> | null;
  const kind = input?.["subagent_type"];

  return branchTools.has(name) && typeof kind === "string" && kind !== ""
    ? `${name}(${kind})`
    : name;
}

function toolResultLines(event: StreamEvent, blocks: StreamContentBlock[], sourceId: string | null): RunLogLine[] {
  const lines: RunLogLine[] = [];

  for (const [sourceBlock, block] of blocks.entries()) {
    if (block.type === "tool_result") {
      const metadata: Record<string, unknown> = {};

      if (event.toolUseResult !== undefined) metadata.toolUseResult = event.toolUseResult;
      if (typeof block.is_error === "boolean") metadata.isError = block.is_error;

      lines.push({
        kind: "tool-result",
        ref: block.tool_use_id ?? null,
        sourceBlock,
        sourceId,
        state: claudeResultState(block, event.toolUseResult),
        upstreamTruncated: wasUpstreamTruncated(event, block, event.toolUseResult),
        metadata: Object.keys(metadata).length === 0 ? undefined : metadata,
        text: flatten(block.content),
      });
    }
  }

  return lines;
}

function claudeResultState(block: StreamContentBlock, result: unknown): RunLogState {
  if (block.is_error === true) return "failed";
  if (block.is_error === false) return "succeeded";
  if (result === null || typeof result !== "object") return "unverified";

  const structured = result as Record<string, unknown>;
  const status = typeof structured.status === "string" ? structured.status.toLowerCase() : "";

  if (structured.is_error === true || structured.isError === true || structured.success === false || structured.error !== undefined
    || ["failed", "error", "errored", "cancelled"].includes(status)) {
    return "failed";
  }

  const type = typeof structured.type === "string" ? structured.type.toLowerCase() : "";
  const appliedFileEvidence = Array.isArray(structured.structuredPatch)
    || (typeof structured.filePath === "string" && ["create", "update", "delete", "write"].includes(type));

  if (structured.success === true || appliedFileEvidence || ["completed", "succeeded", "success"].includes(status)) {
    return "succeeded";
  }

  return status === "running" || status === "async_launched" ? "running" : "unverified";
}

function wasUpstreamTruncated(...values: unknown[]): boolean {
  return values.some((value) => {
    if (value === null || typeof value !== "object") return false;
    const record = value as Record<string, unknown>;

    return record.truncated === true || record.is_truncated === true || record.isTruncated === true
      || record.output_truncated === true || record.outputTruncated === true;
  });
}

// A tool answers with a string, or with blocks of its own, or with something only it knows the
// shape of. The log is text, so whatever it is comes out as the text it reads as.
function flatten(content: unknown): string {
  if (typeof content === "string") {
    return content;
  }

  if (Array.isArray(content)) {
    return content.map((part: unknown) => flatten(part)).join("\n");
  }

  if (content === null || content === undefined) {
    return "";
  }

  if (typeof content === "object") {
    const block = content as StreamContentBlock;

    return typeof block.text === "string" ? block.text : JSON.stringify(content);
  }

  return String(content);
}

// A stream arrives in pieces that cut lines in half, so what is whole is handed over and the
// rest is held until its newline comes. A run killed mid-line loses that line and nothing else.
export class LineBuffer {
  private held = "";

  take(chunk: string): string[] {
    const parts = (this.held + chunk).split("\n");

    this.held = parts.pop() ?? "";

    return parts;
  }
}

// The writer: one INSERT per line, straight from the stream that produced it.
export class RunLog {
  private readonly target: RunLogTarget;
  private readonly decoder = new TextDecoder();
  private readonly buffer = new LineBuffer();
  // A Claude Workflow the run launches is not in the stream as tool calls, so what it did is gathered
  // apart from the events and written into the same log.
  private readonly workflows = new WorkflowTrail();
  // What the same events say about where the run's time went. The lines are pruned and the
  // tally is not, so it is gathered here — in the one pass there is over the stream — and
  // handed to whoever writes the run down.
  private readonly tally = new SpanTally();
  // The calls of this run that opened a file of credentials, by the ref their result answers on.
  private readonly openedSecrets = new Set<string>();
  private codexChanges: CodexFileChanges | null = null;
  private appended = 0;
  // Lines this run had to give up on because the archive was locked, waiting to be owned up to.
  private lost = 0;
  private sequence = 0;
  private sourceEvent = 0;
  // The last line this log took, as a clock asking about the run reads it: when, and its kind with
  // the tool or source it names.
  private latest: { at: number; what: string } | null = null;

  // Where a job's lines left off, read once before the first of this log's lines is numbered.
  private sequenceRead: Promise<void> | null = null;

  // Whoever reads the run's lines as they come, each handed every line written whether or not the
  // archive keeps it.
  private readonly watchers = new Set<(line: RunLogLine) => void>();

  constructor(target: RunLogTarget) {
    this.target = target;
  }

  // The run these lines are kept under.
  get run(): string {
    return this.target.run;
  }

  get lastLine(): { at: number | null; what: string | null } {
    return this.latest ?? { at: null, what: null };
  }

  // A reader of each line from now on, until the function handed back is called.
  watch(watcher: (line: RunLogLine) => void): () => void {
    this.watchers.add(watcher);

    return () => {
      this.watchers.delete(watcher);
    };
  }

  async write(line: RunLogLine): Promise<number | null> {
    for (const watcher of this.watchers) watcher(line);

    await this.knowSequence();
    const id = await this.kept(this.rowFor(line));

    if (id === null) {
      return null;
    }

    this.latest = { at: Date.now(), what: line.name == null || line.name === "" ? line.kind : `${line.kind} ${line.name}` };
    this.appended += 1;

    if (this.appended % pruneEveryLines === 0) {
      await this.throughTheLock("the log prune", () => pruneRunLog(this.target.archive, this.target.agent));
    }

    return id;
  }

  // Where the job's lines left off, read again before the next line: another writer of the same job
  // has written lines of its own meanwhile.
  resync(): void {
    this.sequenceRead = null;
  }

  // The number the job's last line was given, which this log's numbering carries on from. Every
  // write waits for the one reading of it, so two lines written at once never take the same number.
  private async knowSequence(): Promise<void> {
    const jobId = this.target.jobId;

    if (jobId == null) return;

    this.sequenceRead ??= this.target.archive.runs().lastJobActivitySequence(jobId).then((last) => {
      this.sequence = last;
    });
    await this.sequenceRead;
  }

  // One line as the table takes it.
  private rowFor(line: RunLogLine): NewAgentLogLine {
    const own = lineCap(line.kind, this.target.keepsLaunch === true);
    const cap = this.target.jobId == null ? own : Math.min(own, maxJobPreviewChars);
    // What is written is what a reader may see, so a credential is taken out of it before either
    // the preview or the detail is made of it: the archive is never handed the value at all.
    const text = this.withoutSecrets(line);
    const clipped = preview(text, cap);
    const cut = clipped.truncated;
    const keepsDetail = this.target.jobId != null;

    return {
      agent: this.target.agent,
      run: this.target.run,
      chatId: this.target.chatId,
      kind: line.kind,
      name: line.name ?? null,
      parent: line.parent ?? null,
      ref: line.ref ?? null,
      truncated: cut,
      text: clipped.text,
      jobId: this.target.jobId ?? null,
      sequence: this.target.jobId == null ? null : ++this.sequence,
      source: line.source ?? deploymentNames().agentKey,
      sourceEvent: line.sourceEvent ?? null,
      sourceBlock: line.sourceBlock ?? null,
      sourceId: line.sourceId ?? null,
      state: line.state ?? null,
      upstreamTruncated: line.upstreamTruncated === true,
      detailComplete: keepsDetail || !cut,
      detail: keepsDetail && cut ? text : null,
      metadata: keepsDetail && line.metadata !== undefined ? JSON.stringify(line.metadata) : null,
    };
  }

  // A line into the table, waiting out a writer that holds the lock. A line the lock never lets
  // through is a line lost and nothing worse: a run is not ended by its own record-keeping, so the
  // loss is counted and the next line that does get through carries the count into the log.
  private async kept(row: NewAgentLogLine): Promise<number | null> {
    const id = await this.throughTheLock(`a ${row.kind} line`, () => this.target.archive.runs().appendAgentLog(row));

    if (id === null) {
      this.lost += 1;

      return null;
    }

    await this.ownUpToWhatWasLost();

    return id;
  }

  // What the lock cost this run, written into the run's own log the moment the lock frees.
  private async ownUpToWhatWasLost(): Promise<void> {
    if (this.lost === 0) {
      return;
    }

    const lost = this.lost;

    this.lost = 0;

    if (await this.throughTheLock("the note of what was lost", () => this.target.archive.runs().appendAgentLog(this.rowFor({ kind: "note", text: `The store was locked, so ${lost} lines were not kept` }))) === null) {
      this.lost += lost;
    }
  }

  // Archive work a held lock may refuse, named for the journal by what would be lost.
  private async throughTheLock<T>(what: string, work: () => Promise<T>): Promise<T | null> {
    return await keptThroughLock(`${this.target.agent}: ${what}`, work);
  }

  // One line's text with no credential left in it. A tool's arguments and what a tool handed back
  // are the two that carry one: the arguments because a command line or a flag can hold it, the
  // result because a file of credentials read is handed back whole. A result of a call that opened
  // such a file keeps nothing at all — there is no shape to look for in a file whose every line is
  // a value — and the call itself keeps its arguments, so the line still says which file was read.
  // Every other kind is what the run and the bot wrote in their own words and is left alone.
  private withoutSecrets(line: RunLogLine): string {
    if (line.kind === "tool") {
      if (line.ref != null && readsSecretFile(line.text)) {
        this.openedSecrets.add(line.ref);
      }

      return maskSecrets(line.text);
    }

    if (line.kind !== "tool-result") {
      return line.text;
    }

    return line.ref != null && this.openedSecrets.has(line.ref) ? maskedValue : maskSecrets(line.text);
  }

  // What the run is about to be handed, for a log that keeps it: the system prompt as the file the
  // program reads holds it, written only when it differs from the version this agent last wrote,
  // and the prompt exactly as it goes out. Any other log writes nothing here.
  async launched(run: Pick<HarnessRun, "systemPromptPath" | "prompt">, context: readonly TurnContext[] = []): Promise<void> {
    if (this.target.keepsLaunch !== true) {
      return;
    }

    const system = readSystemPrompt(run.systemPromptPath);

    if (system !== null) {
      const version = new Bun.CryptoHasher("sha256").update(system).digest("hex");

      if (await this.target.archive.runs().latestPromptRef(this.target.agent, systemPromptLine) !== version) {
        await this.write({ kind: "prompt", name: systemPromptLine, ref: version, text: system });
      }
    }

    await this.write({ kind: "prompt", text: run.prompt });

    // What went into that prompt without the session asking for anything, in the order it entered
    // it. The lines stand after the prompt because the prompt is what they are about.
    for (const piece of context) {
      await this.write({ kind: "context", name: piece.source, text: piece.text, state: piece.state ?? null });
    }
  }

  // The run's stream as it arrives, in whatever pieces it arrives in: read as `mode`'s program
  // writes it, and filed under `source`, the road it went out on, which a registered model run by
  // Claude Code is not Claude Code's.
  async feed(chunk: Uint8Array, mode: RunLogMode = "claude-code", source: string = mode): Promise<void> {
    await this.read(this.decoder.decode(chunk, { stream: true }), mode, source);
  }

  async finish(mode: RunLogMode = "claude-code", source: string = mode): Promise<void> {
    await this.read(this.decoder.decode() + "\n", mode, source);
    // What the record wrote once the stream had closed is the last a waiting change will get.
    await this.codexChanges?.settle();
  }

  private async read(text: string, mode: RunLogMode, source: string): Promise<void> {
    for (const line of this.buffer.take(text)) {
      const trimmed = line.trim();

      if (!trimmed.startsWith("{")) {
        continue;
      }

      let event: unknown;

      // A line the CLI wrote that is not JSON is not an event, and neither is the broken tail
      // of a run that was killed. Either way there is nothing in it to log.
      try {
        event = JSON.parse(trimmed);
      } catch {
        continue;
      }

      this.sourceEvent += 1;

      if (mode === "claude-code") this.tally.see(event);

      if (mode === "codex") this.codexStarted(event);

      for (const logLine of [...runLogLines(event, mode), ...(mode === "claude-code" ? this.workflows.lines(event) : [])]) {
        await this.writeRead({ ...logLine, source, sourceEvent: this.sourceEvent }, mode);
      }

      if (mode === "codex") await this.codexChanges?.settle();
    }
  }

  // A Codex job's session record is where its file changes' diffs are; only a job keeps the
  // metadata they go into.
  private codexStarted(event: unknown): void {
    const started = event as { type?: string; thread_id?: unknown } | null;

    if (this.target.jobId == null || started?.type !== "thread.started" || typeof started.thread_id !== "string") {
      return;
    }

    this.codexChanges ??= new CodexFileChanges();
    this.codexChanges.started(started.thread_id);
  }

  private async writeRead(line: RunLogLine, mode: RunLogMode): Promise<void> {
    const item = (line.metadata as { item?: Record<string, unknown> } | undefined)?.item;
    const paths = mode === "codex" && line.kind === "tool-result" && item?.type === "file_change" ? streamPaths(item) : null;

    if (paths === null || this.codexChanges === null) {
      await this.write(line);

      return;
    }

    let id: number | null = null;
    const found = this.codexChanges.take(paths, async (fileDiffs) => {
      const written = id;

      if (written !== null) {
        await this.throughTheLock("a line's metadata", () => this.target.archive.runs().setAgentLogMetadata(written, JSON.stringify({ ...(line.metadata as object), fileDiffs })));
      }
    });

    id = await this.write(found === null ? line : { ...line, metadata: { ...(line.metadata as object), fileDiffs: found } });
  }

  // Where the run's time went, as far as the stream has got. A run killed part-way answers with
  // everything that had closed before the kill, which is the whole point of gathering it as the
  // stream arrives rather than reading it back afterwards.
  spans(): JobSpan[] {
    return this.tally.spans();
  }
}

function readSystemPrompt(path: string | null): string | null {
  if (path === null) {
    return null;
  }

  try {
    return readFileSync(path, "utf8");
  } catch {
    return null;
  }
}

// What the log may not keep: the lines of one agent past the newest few thousand or past its share of text,
// and every line anywhere past the age. An agent named sweeps its own rows, which is what a writer does as
// it goes; null sweeps the whole table, every agent at once.
export async function pruneRunLog(archive: Archive, agent: string | null): Promise<void> {
  await archive.runs().pruneAgentLog({
    agent,
    keep: maxLinesPerAgent,
    maxBytes: maxBytesPerAgent,
    before: Date.now() - maxAgeSeconds * 1000,
  });
}
