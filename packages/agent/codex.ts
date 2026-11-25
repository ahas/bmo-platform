import { codexTurnRequestUsages } from "./codex-request-usage";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { codexResidentHome, codexRolloutWindows } from "./codex-local";
import { commandDigest } from "./resident";
import { PLACE_VARIABLE, withoutTurnVariables } from "./agent-place";
import { levelReasoning, levelsIn } from "./reasoning";
import { memberProvider } from "./member-provider";
import { TOOL_HOST_WAIT_SECONDS, bearerVariable, httpServerEntry } from "./tool-host";
import { machineCatalogue, missingOnMachine } from "./provider-facts";
import { providerDeclaration } from "./provider-declaration";
import declaredCodex from "@agency/policy/providers/codex.toml";

import type { AgentReply } from "./agent-reply";
import { allowsWhatItShows, handedRules } from "./harness";
import type { AgentAdapter, HarnessCommand, HarnessOutput, HarnessRun, NativeChildReader, AgentProvider, ResidentSurface, ResidentTurnInput, ResidentTurnReading } from "./harness";

// What the installed Codex CLI offers to a chooser. The CLI owns this catalogue: agency keeps only
// the few fields a screen needs and never carries a second list that can age beside it, and the
// catalogue is read on this machine by the harness-providers binary (docs/provider-presence.md).
export interface CodexModel {
  id: string;
  name: string;
  description: string;
  contextLength: number | null;
  defaultReasoningEffort: string;
  supportedReasoningEfforts: string[];
}

interface CodexUsage {
  input_tokens?: unknown;
  cached_input_tokens?: unknown;
  cache_write_input_tokens?: unknown;
  output_tokens?: unknown;
  reasoning_output_tokens?: unknown;
}

interface CodexEvent {
  type?: unknown;
  thread_id?: unknown;
  message?: unknown;
  error?: { message?: unknown };
  usage?: CodexUsage;
  item?: { type?: unknown; text?: unknown };
}

const binary = "codex";
// What Codex declares of itself, its own file of the shared policy: its label, its effort words —
// null among them, the mapping that names none — and what a launch makes of them.
const declared = providerDeclaration(declaredCodex);
const codexEfforts = declared.efforts;

// The bundled catalogue belongs to the installed executable and needs neither a network request
// nor a model run.
export async function codexModels(): Promise<CodexModel[]> {
  return ((await machineCatalogue("codex")) as { models: CodexModel[] }).models;
}

// What Codex's stream says about the agents a run collaborated with. Its children are threads of
// its own, so a child's name is also its session, and the state of each is read out of the one
// item the CLI keeps rewriting as the collaboration goes on.
function codexNativeChildReader(): NativeChildReader {
  return {
    *read(event) {
      const item = (event as any)?.item;

      if (!["collab_tool_call", "collab_agent_tool_call"].includes(item?.type)) return;
      if (!Array.isArray(item.receiver_thread_ids)) return;

      for (const id of item.receiver_thread_ids) {
        if (typeof id !== "string") continue;
        const status = item.agents_states?.[id]?.status;
        const state = status === "completed" ? "completed" : status === "errored" ? "failed"
          : status === "running" ? "started" : status === "pending_init" ? "requested" : "unresolved";

        yield { id, state, evidence: `codex.receiver:${status ?? "unknown"}`, session: true };
      }
    },
  };
}

// Every agency prompt is already the complete system prompt for its line. Codex normally adds its
// own coding-agent instructions and the AGENTS.md files around the working directory; this road
// replaces the first with agency's file and sets the second's byte limit to zero. The user config,
// exec rules, hosted web, apps, memories, goals and collaboration are all absent as well. What is
// left is the local shell only where the run said it had tools.
function config(key: string, value: string | number | boolean | string[]): string {
  return `${key}=${typeof value === "string" ? JSON.stringify(value) : JSON.stringify(value)}`;
}

interface McpServerEntry {
  command?: unknown;
  args?: unknown;
  env?: unknown;
}

interface McpConfigFile {
  mcpServers?: Record<string, McpServerEntry>;
}

// Agency's MCP files are provider-neutral at the boundary but each CLI spells its configuration
// differently. Codex takes dotted TOML overrides, one server at a time; the secret-free command,
// arguments and environment in the deployed JSON are translated here and nowhere else.
function mcpConfig(run: HarnessRun): string[] {
  if (run.mcpConfigPath === null || run.mcpConfigPath === "") {
    return [];
  }

  const file = JSON.parse(readFileSync(run.mcpConfigPath, "utf8")) as McpConfigFile;
  const args: string[] = [];

  for (const [name, server] of Object.entries(file.mcpServers ?? {})) {
    const http = httpServerEntry(server);

    // A server on the tool host is its URL and the variable its token is read from, which Codex
    // reads from its own environment; nothing of the run is forwarded to it.
    if (http !== null) {
      const token = bearerVariable(http.headers);

      args.push("--config", config(`mcp_servers.${name}.url`, http.url));
      if (token !== null) args.push("--config", config(`mcp_servers.${name}.bearer_token_env_var`, token));
      args.push("--config", config(`mcp_servers.${name}.tool_timeout_sec`, TOOL_HOST_WAIT_SECONDS));
      continue;
    }

    if (typeof server.command !== "string") {
      continue;
    }

    args.push("--config", config(`mcp_servers.${name}.command`, server.command));

    if (Array.isArray(server.args) && server.args.every((value) => typeof value === "string")) {
      args.push("--config", config(`mcp_servers.${name}.args`, server.args));
    }

    if (typeof server.env === "object" && server.env !== null) {
      for (const [key, value] of Object.entries(server.env)) {
        if (typeof value === "string") {
          args.push("--config", config(`mcp_servers.${name}.env.${key}`, value));
        }
      }
    }

    // Codex starts a server on a handful of names of its own and none of the run's (measured), so
    // the run's own agency variables are named for it to hand on — by name, so no value is on the
    // command line — and the place a resident program reads a run's variables from with them.
    args.push("--config", config(`mcp_servers.${name}.env_vars`, forwardedNames(run)));
  }

  return args;
}

function forwardedNames(run: HarnessRun): string[] {
  return [...new Set([PLACE_VARIABLE, ...Object.keys(run.env).filter((key) => key.startsWith("BMO_") && run.env[key] !== undefined)])].sort();
}

// What a run is allowed and how hard it thinks, in the terms both of Codex's programs are told it.
interface CodexShape {
  worker: boolean;
  hasTools: boolean;
  mayWrite: boolean;
  bypassSandbox: boolean;
  reasoning: string;
}

function shapeOf(run: HarnessRun): CodexShape {
  const worker = run.line === "worker";
  const hasTools = worker || (run.toolsShown !== null && run.toolsShown !== "");
  const mayWrite = hasTools && allowsWhatItShows(run);

  return {
    worker,
    hasTools,
    mayWrite,
    // A chat with its tools on is allowed all four it shows, in every room, and its scripts call
    // Agency's loopback services, which bubblewrap cannot set up on this machine. The dangerous flag is
    // limited to such a chat and the worker: a run that allows less than it shows still receives the
    // read-only sandbox, and a gate still receives no shell.
    bypassSandbox: worker || (run.line === "chat" && mayWrite),
    reasoning: run.thinking ? run.effort : "low",
  };
}

async function commandFor(run: HarnessRun): Promise<HarnessCommand> {
  const resuming = run.resumeSessionId !== null;
  const shape = shapeOf(run);
  const { bypassSandbox, mayWrite } = shape;
  const args = [
    binary,
    "exec",
    ...(resuming ? ["resume"] : []),
    "--json",
    "--ignore-user-config",
    "--ignore-rules",
    "--strict-config",
    "--skip-git-repo-check",
    "--model",
    run.model,
  ];

  if (!run.keepSession) {
    args.push("--ephemeral");
  }

  // A worker already runs on Claude's bypass-permissions line and has to commit, push and deploy
  // outside the checkout to finish its job. A chat allowed every tool it shows needs the same
  // unsandboxed host access for its loopback scripts. Every other agency run keeps its narrower boundary.
  if (bypassSandbox) {
    args.push("--dangerously-bypass-approvals-and-sandbox");
  } else if (!resuming) {
    args.push("--sandbox", mayWrite ? "workspace-write" : "read-only");
  }

  if (!resuming) {
    args.push("--cd", run.cwd, "--color", "never");
  }

  args.push(...configRows(run, shape));

  if (resuming) {
    args.push(run.resumeSessionId as string);
  }

  args.push("-");

  return {
    args,
    cwd: run.cwd,
    env: run.env,
    stdin: run.prompt,
  };
}

// The configuration both programs are handed row by row: the approvals and sandbox, the prompt, the
// reasoning, what is switched off, and the run's MCP servers.
function configRows(run: HarnessRun, shape: CodexShape): string[] {
  const { worker, hasTools, mayWrite, bypassSandbox, reasoning } = shape;
  const args = [
    "--config",
    config("approval_policy", "never"),
    "--config",
    config("sandbox_mode", bypassSandbox ? "danger-full-access" : mayWrite ? "workspace-write" : "read-only"),
  ];

  if (run.systemPromptPath === null) {
    // The job worker is a coding agent, so it keeps Codex's own base prompt and reads this
    // checkout's AGENTS.md. A checkout that still uses the older common filename is read too.
    args.push("--config", config("project_doc_fallback_filenames", ["CLAUDE.md"]));

    if (worker) {
      // The worker's rules, which the Claude line appends to that CLI's prompt, are Codex's
      // developer instructions: a message of their own beside the base prompt, which stays whole.
      args.push("--config", config("developer_instructions", handedRules(run)));
    }
  } else if (worker) {
    // A worker that chooses a base prompt is handed it with its rules after it as the whole file in
    // place of Codex's own prompt, and still reads the checkout's AGENTS.md, which is its work's.
    args.push(
      "--config",
      config("model_instructions_file", run.systemPromptPath),
      "--config",
      config("project_doc_fallback_filenames", ["CLAUDE.md"]),
    );
  } else {
    // The supervisor and the gates are themselves: their complete system file replaces Codex's coding-agent
    // prompt, and no project document from the directory they happen to stand in is appended.
    args.push(
      "--config",
      config("model_instructions_file", run.systemPromptPath),
      "--config",
      config("project_doc_max_bytes", 0),
    );
  }

  args.push(
    "--config",
    config("model_reasoning_effort", reasoning),
    "--config",
    config("web_search", "disabled"),
    "--config",
    config("features.apps", false),
    "--config",
    config("features.multi_agent", worker),
    "--config",
    config("features.memories", false),
    "--config",
    config("features.goals", false),
    "--config",
    config("features.shell_tool", hasTools),
  );

  if (!worker) {
    args.push(
      "--config",
      config("include_permissions_instructions", false),
      "--config",
      config("include_apps_instructions", false),
      "--config",
      config("include_collaboration_mode_instructions", false),
      "--config",
      config("include_environment_context", false),
    );
  }

  args.push(...mcpConfig(run));

  return args;
}

// The interactive program started in the agent's pane: the same model, sandbox and configuration as
// a run of `exec`, resumed by its thread where the agent has one, and no prompt, which goes in once
// it is up. It stands under the resident home, and two of the screens it raises before its input box
// are switched off here: the update offer and the hooks review. The folder trust is in the home's
// own configuration. It is spawned as the program itself rather than through a shell, so no shell
// function of the same name stands in front of it.
// The program stands across many runs, so it is built from none of a run's own variables: what it
// runs reads those from its place, and its command is the same from one room's turn to the next.
async function residentCommandFor(given: HarnessRun): Promise<HarnessCommand> {
  const run: HarnessRun = { ...given, env: withoutTurnVariables(given.env) };
  const shape = shapeOf(run);
  const args = [binary];

  if (run.resumeSessionId !== null) {
    args.push("resume", run.resumeSessionId);
  }

  args.push("--model", run.model);

  if (shape.bypassSandbox) {
    args.push("--dangerously-bypass-approvals-and-sandbox");
  } else {
    args.push("--sandbox", shape.mayWrite ? "workspace-write" : "read-only");
  }

  args.push(
    "--cd",
    run.cwd,
    ...configRows(run, shape),
    "--config",
    config("check_for_update_on_startup", false),
    "--config",
    config("features.hooks", false),
  );

  return { args, cwd: run.cwd, env: { ...run.env, CODEX_HOME: codexResidentHome() }, stdin: null };
}

// One line of the rollout file the interactive program writes for its thread.
interface RolloutLine {
  type?: unknown;
  payload?: {
    type?: unknown;
    id?: unknown;
    model?: unknown;
    message?: unknown;
    last_agent_message?: unknown;
    error?: unknown;
    info?: { last_token_usage?: CodexUsage } | null;
    rate_limits?: unknown;
    codex_error_info?: unknown;
  };
}

// A turn out of the rollout lines the program wrote since it went in: the task it started and
// completed or aborted, the agent's last words, the token count of each request and the windows the
// last count carried. The same tokens split the way `exec`'s reader splits them.
export function readCodexRollout(input: ResidentTurnInput, run: HarnessRun): ResidentTurnReading {
  const lines = input.lines as RolloutLine[];
  const events = lines.filter((line) => line.type === "event_msg").map((line) => line.payload ?? {});
  const lastStart = events.findLastIndex((event) => event.type === "task_started");
  const turn = lastStart === -1 ? events : events.slice(lastStart);
  const completed = turn.findLast((event) => event.type === "task_complete");
  const aborted = turn.some((event) => event.type === "turn_aborted");
  const said = turn
    .filter((event) => event.type === "agent_message" && typeof event.message === "string")
    .map((event) => event.message as string)
    .at(-1);
  const text = typeof completed?.last_agent_message === "string" ? completed.last_agent_message : (said ?? "");
  const failed = completed?.error === undefined || completed.error === null ? null : codexErrorText(completed.error);
  // This turn's own counts: a failed request writes one with no figures, and a compaction inside the
  // turn one with none of input, and neither is a request.
  const counts = turn.filter((event) => event.type === "token_count");
  let inputTokens = 0;
  let cacheRead = 0;
  let cacheWrite = 0;
  let outputTokens = 0;
  const requests: NonNullable<AgentReply["lastRequestUsage"]>[] = [];

  for (const event of counts) {
    const usage = event.info?.last_token_usage;

    if (usage === undefined || count(usage.input_tokens) === 0) continue;

    const input = count(usage.input_tokens);
    const read = count(usage.cached_input_tokens);
    const write = count(usage.cache_write_input_tokens);
    const fresh = Math.max(0, input - read - write);

    inputTokens += fresh;
    cacheRead += read;
    cacheWrite += write;
    outputTokens += count(usage.output_tokens);
    requests.push({ inputTokens: fresh, cacheReadTokens: read, cacheWriteTokens: write, outputTokens: count(usage.output_tokens) });
  }

  // The windows the newest count carried, which a failed request writes too.
  const windows = codexRolloutWindows(counts.findLast((event) => event.rate_limits !== undefined && event.rate_limits !== null)?.rate_limits)
    .map(({ name, utilization, resetsAt, durationMinutes, limitName }) => ({ name, utilization, resetsAt, durationMinutes, limitName }));
  const model =
    lines.filter((line) => line.type === "turn_context" && typeof line.payload?.model === "string").map((line) => line.payload?.model as string).at(-1)
    ?? run.model;
  const sessionId =
    lines.filter((line) => line.type === "session_meta" && typeof line.payload?.id === "string").map((line) => line.payload?.id as string).at(-1)
    ?? run.resumeSessionId;
  const tokens = inputTokens + outputTokens + cacheRead + cacheWrite;

  return {
    state: aborted ? "interrupted" : completed !== undefined ? "ended" : "working",
    // The rollout records nothing of a word held on the screen, and a task never completes with
    // one still held, so an ended task has none queued (measured).
    queued: 0,
    error: failed,
    money: null,
    reply: {
      mode: "codex",
      model,
      effort: run.effort,
      text: failed === null ? text.trim() : "",
      sessionId,
      contextRatio: null,
      contextTokens: null,
      sessionUsage: null,
      windows,
      spend: tokens === 0
        ? []
        : [{ mode: "codex", model, tokens, inputTokens, outputTokens, cacheReadTokens: cacheRead, cacheWriteTokens: cacheWrite, cost: 0 }],
      liveTasks: [],
      workflows: [],
      ...(requests.length === 0 ? {} : { lastRequestUsage: requests.at(-1), requestUsages: requests }),
    },
  };
}

// What a failed turn's error says, in the words the fallback road reads for a limit or an outage:
// the program's own code for it where it wrote one, and its message beside it.
function codexErrorText(error: unknown): string {
  if (typeof error === "string") return error;

  const held = typeof error === "object" && error !== null ? error as Record<string, unknown> : {};
  const code = typeof held.codex_error_info === "string" ? held.codex_error_info : null;
  const message = typeof held.message === "string" ? held.message : null;

  return [code, message].filter((part): part is string => part !== null).join(": ") || JSON.stringify(error);
}

// Codex's interactive program as the program an agent keeps standing in its pane. Words put in while
// a turn works are taken inside that task, and Escape interrupts it with the program left standing
// on its thread (measured); a fresh session is a new thread, opened on the summary.
export const codexResident: ResidentSurface = {
  command: residentCommandFor,
  door: "agent-prompt",
  takesWordsMidTurn: declaredCodex.resident.words_mid_turn as boolean,
  readTurn: readCodexRollout,
  interrupt: { keys: declaredCodex.resident.interrupt as string[] },
  fold: null,
  clear: null,
  identity: commandDigest,
  owns: (command) => command.args[0] === binary,
  // Its turns are read out of the thread's rollout, which is what this surface's own runner does.
  turn: (request) => import("./codex-resident").then(({ residentCodexTurn }) => residentCodexTurn({ ...request, surface: codexResident })),
};

export function codexFailure(output: HarnessOutput): string {
  const eventError = codexEvents(output.stdout)
    .map((event) => {
      if (event.type === "error" && typeof event.message === "string") {
        return event.message;
      }

      return event.type === "turn.failed" && typeof event.error?.message === "string"
        ? event.error.message
        : null;
    })
    .findLast((message): message is string => message !== null && message !== "");

  if (eventError !== undefined) {
    return eventError;
  }

  const said = output.stderr
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "")
    .at(-1);

  return said ?? `exit ${output.exitCode}`;
}

function codexEvents(stdout: string): CodexEvent[] {
  const events: CodexEvent[] = [];

  for (const line of stdout.split("\n")) {
    if (line.trim() === "") {
      continue;
    }

    try {
      const parsed: unknown = JSON.parse(line);

      if (typeof parsed === "object" && parsed !== null) {
        events.push(parsed as CodexEvent);
      }
    } catch {
      // JSON mode should be one object per line. A diagnostic line from a newer CLI is not an
      // answer and is ignored; an absent completed message still makes the whole reply empty.
    }
  }

  return events;
}

function count(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? Math.floor(value) : 0;
}

export const codex: AgentAdapter = {
  name: "codex",
  streamFormat: "codex",
  // A judgement, a job and a room's turn. The runner owns a chat's and a worker's resumable thread; a
  // gate keeps none. A run shown file tools alone is shown them as the shell inside a workspace-write
  // sandbox rooted at the run's directory, and keeps Codex's own base prompt with the checkout's
  // AGENTS.md where it has no system prompt file of its own.
  // The runners are not carried: their tools are agency's own MCP wiring and co-worker gate, which
  // this road does not reproduce, so it offers neither the tool gate nor a tool set made of MCP
  // servers alone.
  offers: [],
  foldPrompt: "/compact",
  command: commandFor,
  failure: codexFailure,

  nativeChildReader: codexNativeChildReader,

  // Codex names the thread it opened once, on the event that opens it.
  sessionFromEvent(event: unknown): string | null {
    const opened = event as { type?: unknown; thread_id?: unknown } | null;

    return opened?.type === "thread.started" && typeof opened.thread_id === "string" ? opened.thread_id : null;
  },

  progressOf(line: string): string | null {
    const event = codexEvents(line)[0];
    const item = event?.item as { type?: unknown; text?: unknown; command?: unknown } | undefined;

    if (event?.type !== "item.completed" || item === undefined) return null;
    if (item.type === "agent_message" && typeof item.text === "string") return item.text;
    if (item.type === "command_execution" && typeof item.command === "string") return `$ ${item.command}`;

    return null;
  },

  async readAnswer(output, run): Promise<AgentReply> {
    const events = codexEvents(output.stdout);
    const text = events
      .filter((event) => event.type === "item.completed" && event.item?.type === "agent_message")
      .map((event) => event.item?.text)
      .filter((value): value is string => typeof value === "string")
      .at(-1) ?? "";
    const usage = events.findLast((event) => event.type === "turn.completed")?.usage;
    const sessionId = events
      .filter((event) => event.type === "thread.started")
      .map((event) => event.thread_id)
      .findLast((value): value is string => typeof value === "string" && value !== "")
      ?? (run?.resumeSessionId ?? null);
    const input = count(usage?.input_tokens);
    const cacheRead = count(usage?.cached_input_tokens);
    const cacheWrite = count(usage?.cache_write_input_tokens);
    const outputTokens = count(usage?.output_tokens);
    // Responses includes cached and cache-written tokens in input_tokens. Agency's common split
    // keeps the four kinds disjoint, as Claude's result does, so the ordinary input is the rest.
    const inputTokens = Math.max(0, input - cacheRead - cacheWrite);
    const model = run?.model ?? null;
    const tokens = inputTokens + outputTokens + cacheRead + cacheWrite;
    const requestUsages = codexTurnRequestUsages(sessionId);

    return {
      mode: "codex",
      model,
      effort: run?.effort ?? null,
      text: output.exitCode === 0 ? text.trim() : "",
      sessionId: run?.keepSession ? sessionId : null,
      lastRequestUsage: requestUsages?.at(-1) ?? null,
      requestUsages,
      contextRatio: null,
      contextTokens: null,
      sessionUsage: null,
      windows: [],
      spend: model === null || tokens === 0
        ? []
        : [{
            mode: "codex",
            model,
            tokens,
            inputTokens,
            outputTokens,
            cacheReadTokens: cacheRead,
            cacheWriteTokens: cacheWrite,
            // Codex subscription usage exposes tokens but no dollar charge. The archive keeps
            // its existing numeric cost field at zero and the mode tells every display why.
            cost: 0,
          }],
      liveTasks: [],
      workflows: [],
    };
  },
};

// The owner's Codex models, reached by Claude Code through agency's gateway member on an OpenAI API key.
export const codexProvider: AgentProvider = memberProvider({
  name: "codex",
  policy: declaredCodex,
  reasoning: levelReasoning(async () => levelsIn(codexEfforts)),
});
