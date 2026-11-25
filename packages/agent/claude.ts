import declaredClaudeCode from "@agency/policy/providers/claude-code.toml";
import { ClaudeLimitError, ClaudeOutageError } from "./claude-failure";
import { RunDied } from "./doctor";
import { failureText } from "./failure-text";
import { claudeSpendOf } from "./spend";
import { workflowTask } from "./workflow-log";

import type { AgentSessionReply, SessionUsage, UsageWindowReading } from "./agent-reply";
import type { LiveTask, RunWorkflow } from "./finished";
import type { ClaudeModelUsage, ModelSpend } from "./spend";

interface ClaudeTokenCounts {
  input_tokens?: number;
  cache_creation_input_tokens?: number;
  cache_read_input_tokens?: number;
}

interface ClaudeUsage extends ClaudeTokenCounts {
  output_tokens?: number;
  iterations?: ClaudeTokenCounts[];
}

// What the CLI hands back as one object at the end of a run. A line that asks for json alone
// gets exactly this, so a runner reading its result rather than a stream reads what a run cost
// and what it ended carrying out of the same object the answer is in.
export interface ClaudeJsonResult {
  result?: string;
  session_id?: string;
  is_error?: boolean;
  usage?: ClaudeUsage;
  modelUsage?: Record<string, ClaudeModelUsage>;
}

interface ClaudeRateLimitWindow {
  utilization?: number;
  resetsAt?: number;
}

interface ClaudeRateLimitInfo {
  status?: string;
  resetsAt?: number;
  unifiedWindows?: Record<string, ClaudeRateLimitWindow | undefined>;
}

interface ClaudeBackgroundTask {
  task_id?: string;
  description?: string;
}

interface ClaudeStreamEvent extends ClaudeJsonResult {
  subtype?: string;
  error_status?: number;
  type?: string;
  rate_limit_info?: ClaudeRateLimitInfo;
  tasks?: ClaudeBackgroundTask[];
  // What the three task events carry about one background task: which call opened it, what kind
  // of task it is, what it was launched as, and how it ended.
  task_id?: string;
  tool_use_id?: string;
  task_type?: string;
  workflow_name?: string;
  description?: string;
  status?: string;
  message?: { content?: { type?: string; text?: string }[] };
}

interface ClaudeStreamOutput {
  result: ClaudeJsonResult | null;
  rateLimit: ClaudeRateLimitInfo | null;
  // The CLI's own retries of a failing API call, with the last status it saw.
  retries: number;
  lastErrorStatus: number | null;
  // The background tasks still live at the moment the result was produced.
  liveTasks: LiveTask[];
  // The Claude Workflows the run launched, and what its stream said became of each.
  workflows: RunWorkflow[];
}

// What every line sets on the process it starts, named apart from the environment it is set on.
// A command a host runs is handed the whole environment with these on top; a process that is a
// pane's own is handed these by name, because the environment of whoever launched it is not
// something to pass on — a launcher that is itself inside a Claude session carries that session's
// markers, and a CLI that inherits them turns its transcript saving off.
//
// Which they are, and why, is Claude Code's own declaration in agency's policy, read by every side
// that starts one of its lines (docs/launch-lines.md).
export const launchVariables: Readonly<Record<string, string>> = (declaredClaudeCode as { launch: { variables: Record<string, string> } }).launch.variables;

export function launchEnv(): Record<string, string | undefined> {
  return { ...process.env, ...launchVariables };
}

// Every line of a stream that parses as JSON. A stream read from a run that was killed ends
// mid-line, and that last broken line is simply not an event.
function streamEvents(stdout: string): ClaudeStreamEvent[] {
  const events: ClaudeStreamEvent[] = [];

  for (const line of stdout.split("\n")) {
    const trimmed = line.trim();

    if (!trimmed.startsWith("{")) {
      continue;
    }

    try {
      events.push(JSON.parse(trimmed) as ClaudeStreamEvent);
    } catch {
      continue;
    }
  }

  return events;
}

// What the run wrote, in the order it wrote it, taken from the stream rather than from the
// result frame. A run that was killed never reaches its result frame, so this is the only
// place its work survives.
export function assistantTexts(stdout: string): string[] {
  return streamEvents(stdout)
    .filter((event) => event.type === "assistant")
    .flatMap((event) => event.message?.content ?? [])
    .filter((block) => block.type === "text" && typeof block.text === "string")
    .map((block) => block.text as string);
}

function parseStream(stdout: string): ClaudeStreamOutput {
  let result: ClaudeJsonResult | null = null;
  let rateLimit: ClaudeRateLimitInfo | null = null;
  let retries = 0;
  let lastErrorStatus: number | null = null;
  // The CLI reports its whole set of live background tasks whenever the set changes, so the
  // last report before the result frame is what was still running when the run ended.
  let running: LiveTask[] = [];
  let liveTasks: LiveTask[] = [];
  // A Claude Workflow is reported through the task events rather than as a background task, so it is
  // gathered on its own: kept under the call that launched it, since an ending may name only
  // the task, and read across the whole stream rather than cut off at the result frame — an
  // ending that arrives after that frame is the CLI saying what it did to a Claude Workflow as the run
  // went down, which is the very thing worth knowing.
  const workflows = new Map<string, RunWorkflow>();
  const workflowCalls = new Map<string, string>();

  for (const event of streamEvents(stdout)) {
    if (event.type === "result") {
      result = event;
      liveTasks = running;
    } else if (event.type === "system" && event.subtype === "task_started") {
      // Only a task whose own start said it was a Claude Workflow. The CLI runs its other background
      // tasks through these same three events, and those are somebody else's business.
      if (event.task_type === workflowTask && event.tool_use_id !== undefined) {
        if (event.task_id !== undefined) {
          workflowCalls.set(event.task_id, event.tool_use_id);
        }

        workflows.set(event.tool_use_id, {
          call: event.tool_use_id,
          name: event.workflow_name ?? event.description ?? "",
          status: null,
        });
      }
    } else if (event.type === "system" && event.subtype === "task_notification") {
      const call = event.tool_use_id ?? (event.task_id === undefined ? undefined : workflowCalls.get(event.task_id));
      const flow = call === undefined ? undefined : workflows.get(call);

      if (flow !== undefined && event.status !== undefined) {
        flow.status = event.status;
      }
    } else if (event.type === "system" && event.subtype === "background_tasks_changed") {
      running = (event.tasks ?? []).map((task) => ({
        id: task.task_id ?? "",
        description: task.description ?? "",
      }));
    } else if (event.type === "rate_limit_event" && event.rate_limit_info !== undefined) {
      rateLimit = event.rate_limit_info;
    } else if (event.type === "system" && event.subtype === "api_retry") {
      retries += 1;
      lastErrorStatus = typeof event.error_status === "number" ? event.error_status : lastErrorStatus;
    }
  }

  return { result, rateLimit, retries, lastErrorStatus, liveTasks, workflows: [...workflows.values()] };
}

const limitWordings: RegExp[] = [
  /claude (?:ai )?usage limit reached/i,
  /usage limit reached\|\d+/i,
  /you'?ve (?:hit|reached) your [^\n]{0,40}limit/i,
];

export function mentionsLimit(text: string): boolean {
  return limitWordings.some((pattern) => pattern.test(text));
}

export function limitResetFromText(text: string): number | null {
  const match = /limit reached\|(\d{9,11})/i.exec(text);
  const epoch = match?.[1] === undefined ? null : Number.parseInt(match[1], 10);

  return epoch === null || Number.isNaN(epoch) ? null : epoch;
}

function sessionUsageOf(rateLimit: ClaudeRateLimitInfo | null): SessionUsage | null {
  const window = rateLimit?.unifiedWindows?.five_hour;

  if (window?.utilization === undefined || window.resetsAt === undefined) {
    return null;
  }

  const weekly = rateLimit?.unifiedWindows?.seven_day;

  return {
    utilization: window.utilization,
    resetsAt: window.resetsAt,
    weeklyUtilization: weekly?.utilization ?? null,
    weeklyResetsAt: weekly?.resetsAt ?? null,
  };
}

// `ownRun` reads what the run spent off its own usage, under the one model it names, rather than off
// the model-by-model usage, which a resumed session carries as its running total (docs/own-llm.md,
// 「How requests go」).
export function readClaudeOutput(stdout: string, stderr: string, exitCode: number, launched: string | null = null, options: { ownRun?: boolean } = {}): AgentSessionReply {
  const { result, rateLimit, retries, lastErrorStatus, liveTasks, workflows } = parseStream(stdout);

  if (
    exitCode !== 0 ||
    result === null ||
    result.is_error === true ||
    result.result === undefined ||
    result.session_id === undefined
  ) {
    const errorText = `${stderr}\n${result?.result ?? ""}`;

    if (rateLimit?.status === "rejected" || mentionsLimit(errorText)) {
      throw new ClaudeLimitError(
        rateLimit?.resetsAt ??
          rateLimit?.unifiedWindows?.five_hour?.resetsAt ??
          limitResetFromText(errorText),
      );
    }

    // A run that only ever retried its API call and then stopped met an outage, not a fault.
    if (result === null && retries > 0) {
      throw new ClaudeOutageError(lastErrorStatus, retries);
    }

    // What the reader of a run knows about how it died, carried with the sentence rather than
    // only inside it: whoever tells the owner why the run broke reads these rather than taking the
    // message back apart (`packages/agent/doctor.ts`).
    // A run that wrote its result before it exited says why in that result, where its standard error
    // may hold no more than the CLI's own warnings.
    if (exitCode !== 0) {
      const said = failureText(result?.result === undefined || result.result.trim() === "" ? stderr : `${stderr}\n${result.result}`);

      throw new RunDied({
        ending: { kind: "exit", code: exitCode },
        said,
        message: `claude exited with code ${exitCode}: ${said}`,
      });
    }

    throw new RunDied({
      ending: { kind: "error-result" },
      said: failureText(stdout),
      message: `claude returned an error result: ${failureText(stdout)}`,
    });
  }

  return {
    mode: "claude-code",
    model: result.modelUsage === undefined ? null : (Object.keys(result.modelUsage)[0] ?? null),
    effort: null,
    text: result.result,
    sessionId: result.session_id,
    contextRatio: contextRatioOf(result, launched),
    contextTokens: contextTokensOf(result),
    sessionUsage: sessionUsageOf(rateLimit),
    windows: windowsOf(rateLimit),
    spend: options.ownRun === true ? ownRunSpend(result) : claudeSpendOf(result.modelUsage),
    liveTasks,
    workflows,
  };
}

// What one run spent by its own usage, under the one model the result names; the model-by-model
// figures where it names more than one, or no usage of its own.
function ownRunSpend(result: ClaudeJsonResult): ModelSpend[] {
  const models = Object.keys(result.modelUsage ?? {});
  const usage = result.usage;

  if (models.length !== 1 || usage === undefined) return claudeSpendOf(result.modelUsage);

  return claudeSpendOf({
    [models[0]!]: {
      inputTokens: usage.input_tokens,
      outputTokens: usage.output_tokens,
      cacheReadInputTokens: usage.cache_read_input_tokens,
      cacheCreationInputTokens: usage.cache_creation_input_tokens,
    },
  });
}

// Every window the answer carried, under the name the API gave it. A window without a figure
// says nothing and is left out rather than recorded as zero.
export function windowsOf(rateLimit: ClaudeRateLimitInfo | null): UsageWindowReading[] {
  const windows = rateLimit?.unifiedWindows ?? {};

  return Object.entries(windows)
    .filter(([, window]) => typeof window?.utilization === "number")
    .map(([name, window]) => ({
      name,
      utilization: window?.utilization ?? 0,
      resetsAt: window?.resetsAt ?? null,
    }));
}

// What the turn's last request carried: everything the model was sent, whether it came out of
// the cache or was written into it. That whole sum is what a resume writes again once the
// prefix behind it has been invalidated.
export function contextTokensOf(parsed: ClaudeJsonResult): number | null {
  const iterations = parsed.usage?.iterations ?? [];
  const counts = iterations.length > 0 ? iterations[iterations.length - 1] : parsed.usage;

  if (counts === undefined) {
    return null;
  }

  const tokens =
    (counts.input_tokens ?? 0) +
    (counts.cache_creation_input_tokens ?? 0) +
    (counts.cache_read_input_tokens ?? 0);

  return tokens > 0 ? tokens : null;
}

// How full the context is: what the turn carried against the window of the model it was launched
// on. A run reports a window for every model it spent on, the CLI's own small housekeeping model
// among them, so the launched model's own window is the one asked for and the widest reported is
// what answers where the caller did not say which model it launched.
function contextRatioOf(parsed: ClaudeJsonResult, launched: string | null): number | null {
  const tokens = contextTokensOf(parsed) ?? 0;
  const windows = Object.values(parsed.modelUsage ?? {})
    .map((usage) => usage.contextWindow ?? 0)
    .filter((window) => window > 0);
  const contextWindow = (launched === null ? undefined : parsed.modelUsage?.[launched]?.contextWindow) ?? Math.max(0, ...windows);

  if (tokens <= 0 || contextWindow === undefined || contextWindow <= 0) {
    return null;
  }

  return tokens / contextWindow;
}
