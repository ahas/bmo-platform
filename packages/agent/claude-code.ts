import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

import { enabledClaudeAccounts, ENDPOINT_FAMILIES, machineConfigDir, recordedAccount, recordsDirOf } from "./claude-account";
import { machineCatalogue } from "./provider-facts";
import { providerDeclaration } from "./provider-declaration";
import declaredClaudeCode from "@agency/policy/providers/claude-code.toml";
import declaredRunLines from "@agency/policy/run-lines.toml";
import { walkingClaudeAccount } from "./claude-dispatch";
import { calledTool } from "./mcp-names";
import { providerSettingsArchive } from "./provider-order";
import { cliProjectsRoot, readLastRequestTokens, transcriptPath } from "./cli-transcripts";
import { claudeConfigFile, markOnboarded, trustFolder } from "./claude-trust";
import { writePlaceSettings } from "./agent-place";
import { registeredRoad } from "./claude-road";
import { readClaudeOutput } from "./claude";
import { handedPath } from "@agency/support/launch-paths";
import { dataPath } from "@agency/support/roots";
import { usableKeyFile } from "./members";
import { levelReasoning, levelsIn } from "./reasoning";

import type { AgentReply, SessionUsage, TurnAnswer, UsageWindowReading } from "./agent-reply";
import { commandDigest } from "./resident";
import { lineComposesTypedLines, lineHoldsStops, lineKeepsSession } from "./line-declarations";
import { readTurn } from "./worker-pane";

import type { AgentAdapter, AgentLogins, AgentProviderName, RunCapability, HarnessCommand, HarnessOutput, HarnessRun, NativeChildReader, AgentProvider, ProviderModel, ResidentSurface, ResidentTurnInput, ResidentTurnReading, AgentAccount, RunExtensions } from "./harness";

// Claude Code as one harness among however many there come to be. Every flag agency's runs are
// carried on is written here and nowhere else: what a line asks for arrives as a run, and what
// leaves here is a command. So the words `--tools` and `--permission-mode` exist in this file
// alone, and a second harness answers the same runs without ever seeing them.
//
// The orders below are not free. A prompt cache matches a byte-identical prefix, and the prefix
// is the command plus the files the command points at, so a flag moved from one place in a line
// to another costs every warm session standing on that line its whole context. The lines are the
// owner's, CLAUDE.md says so, and this file is where that rule is kept rather than where it is
// relaxed: each builder writes its line out in the order that line has, and two lines that
// happen to share an order share a builder only because they really do.

// What the CLI's own commands are reached by, and the one of them agency uses: the fold.
const foldPrompt = "/compact";

// What the CLI writes itself into a session's conversation: the opening of the environment block it
// sends after a session's first words, and the tag its reminders stand in.
const claudeOwnWords: readonly string[] = (declaredClaudeCode as { launch: { own_words: string[] } }).launch.own_words;

// Its tools whose calls wait long, which a run on the node pool names in every request.
const claudeLongTools: readonly string[] = (declaredClaudeCode as { launch: { long_tools: string[] } }).launch
  .long_tools;

// What Claude Code declares of itself, its own file of the shared policy: every effort word this
// harness takes, in the order a screen shows them, with `null` the mapping that names none — it may
// be held, and `effortFault` is what refuses to launch on it.
const declared = providerDeclaration(declaredClaudeCode);
const claudeEfforts = declared.efforts;

const everyClaudeEffort: readonly string[] = claudeEfforts.filter((word) => word !== null) as readonly string[];

// Which models this harness offers, and what each of them takes, read from the catalogue the CLI
// keeps for the login a run would go out on. The read is one file off the disk and costs a
// fraction of a millisecond, so every question asks it afresh: a model the CLI began offering an
// hour ago is offered here with nothing restarted and nothing deployed.
//
// A catalogue that cannot be read is a refusal for a listing, since a screen showing an empty
// picker would read as a harness offering nothing. It is not a refusal for a check against one
// model in hand: a name the catalogue does not carry, and a model whose entry names no effort
// because it does not think at all, both take every word this harness takes, because a name
// missing from a list is not evidence against it (`docs/model-tiers.md`, 「The Claude catalogue」).
function offeredEfforts(named: readonly string[]): readonly string[] {
  return named.length === 0 ? everyClaudeEffort : named;
}

// Whose catalogue is read: the login this harness walks onto first, which is the one a run with no
// login named of its own goes out on, and whose CLI therefore refreshes the file as agency uses it. A
// login keeping no directory of its own reads the machine's, and so does a process that installed
// no store to ask — a script, a fixture — which is the same directory the CLI itself would use
// there.
async function claudeCatalogueDir(): Promise<string> {
  const archive = providerSettingsArchive();

  return archive === null ? machineConfigDir() : await recordsDirOf(archive, await walkingClaudeAccount(archive));
}

/** One model of the catalogue, as whoever offers a choice of them needs it. */
export interface ClaudeListedModel {
  id: string;
  // Whether the CLI serves this generation or has superseded it. Both are offered — a mapping made
  // when a model was current still names it after it moves — and which of the two it is belongs
  // beside the name.
  inService: boolean;
  efforts: readonly string[];
}

export interface ClaudeModelListing {
  models: readonly ClaudeListedModel[];
  // When the CLI fetched the catalogue this list came from, and whether that read is past the
  // moment the CLI itself set for fetching it again. A stale read is served as it stands and says
  // so; it is the last list there is, and an empty picker would say less.
  readAt: number;
  stale: boolean;
}

// The catalogue as the harness-providers binary reads it under a login's directory
// (docs/provider-presence.md).
interface MachineCatalogue {
  models: ClaudeListedModel[];
  fetchedAt: number;
  staleAt: number;
}

export async function claudeModelListing(now: number = Date.now()): Promise<ClaudeModelListing> {
  const catalogue = (await machineCatalogue("claude-code", await claudeCatalogueDir())) as MachineCatalogue;

  return {
    models: catalogue.models.map((model) => ({ id: model.id, inService: model.inService, efforts: offeredEfforts(model.efforts) })),
    readAt: catalogue.fetchedAt,
    // Past the moment the CLI itself set for fetching it again.
    stale: catalogue.staleAt <= now,
  };
}

export async function claudeCatalogueModels(): Promise<ProviderModel[]> {
  return (await claudeModelListing()).models.map((model) => ({ id: model.id, supportedReasoningEfforts: [...model.efforts] }));
}

export async function claudeEffortsFor(model: string): Promise<readonly string[]> {
  try {
    return (await claudeModelListing()).models.find((one) => one.id === model)?.efforts ?? everyClaudeEffort;
  } catch {
    return everyClaudeEffort;
  }
}

// What the CLI's stream says about the co-workers a run opened. Every word read here is Claude
// Code's own — a tool_use block, the structured result beside a tool return, the system lines a task
// writes — and reading them is this file's work, while what the evidence means to a record is the
// caller's. Plain tool text, an asynchronous launch acknowledgement included, is not evidence that a
// child finished: only a structured result or a task notification says that.
function claudeNativeChildReader(): NativeChildReader {
  // A task announces itself under a name of its own, and only the tool return that launched it
  // joins the two, so the pairing is kept for as long as the run lasts.
  const tasks = new Map<string, string>();

  return {
    *read(event, known) {
      const turn = event as any;

      for (const block of Array.isArray(turn?.message?.content) ? turn.message.content : []) {
        if (block.type === "tool_use" && typeof block.id === "string" && ["Agent", "Task", "Workflow"].includes(block.name)) {
          yield { id: block.id, state: "requested", evidence: "claude.tool_use" };
        }

        if (block.type === "tool_result" && known(block.tool_use_id) && block.is_error === true) {
          yield { id: block.tool_use_id, state: "failed", evidence: "claude.tool_result.is_error" };
        }

        if (block.type === "tool_result" && known(block.tool_use_id)) {
          const result = turn.toolUseResult;

          if (typeof result?.agentId === "string" && result.status === "completed") {
            yield { id: block.tool_use_id, state: "completed", evidence: "claude.toolUseResult.completed" };
          } else if (result?.status === "async_launched") {
            yield { id: block.tool_use_id, state: "started", evidence: "claude.toolUseResult.async_launched" };
            const task = result.taskId ?? result.agentId;

            if (typeof task === "string") tasks.set(task, block.tool_use_id);
          }
        }
      }

      if (turn?.type !== "system") return;
      const id = typeof turn.tool_use_id === "string" ? turn.tool_use_id : tasks.get(turn.task_id);

      if (!id) return;
      if (!known(id) && !["local_agent", "local_workflow"].includes(turn.task_type)) return;
      if (typeof turn.task_id === "string") tasks.set(turn.task_id, id);
      if (turn.subtype === "task_started") yield { id, state: "started", evidence: "claude.task_started" };

      if (turn.subtype === "task_notification") {
        const state = turn.status === "completed" ? "completed" : turn.status === "failed" ? "failed" : "unresolved";

        yield { id, state, evidence: `claude.task_notification:${turn.status}` };
      }
    },
  };
}

// The worker line's extensions, which its composition hands every run of it.
function extensionsOf(run: HarnessRun): RunExtensions {
  if (run.extensions === undefined) {
    throw new Error(`${run.line}: this run was handed no settings, plugin or co-worker definitions, which its line loads`);
  }

  return run.extensions;
}

// The head of every line. Claude Code is started only as the interactive program, standing in a
// pane and read back from the record it writes (docs/agents.md, 「No print mode」), so a run asking
// for its answer on the program's output is refused here, whichever line it is of.
function programHead(run: HarnessRun): string[] {
  if (run.answerShape !== "unread") {
    throw new Error(`the ${run.line} line asks Claude Code for a ${run.answerShape} answer on its output, and Claude Code runs only as the interactive program`);
  }

  return ["claude"];
}

// Everything a job worker is handed beyond its prompt — the skills, the co-worker definitions,
// the settings — is Claude Code's own vocabulary for them, so the run says nothing about it and
// this answers for the worker line. A harness with no notion of a skill simply hands over less.
//
// A job is run as a person runs it: it holds its terminal and can be spoken to while it works, and
// the session it answers in is named on the way in because nothing will print it back. Its fold is
// the same program's `/compact`, so it stands on the same prefix.
// The flag and the file a worker's prompt is handed as: a decided base or the harnessed file whole, or
// the rules alone appended for a kept run written before the harnessed file was composed.
function wholePrompt(run: HarnessRun): string[] {
  if (run.systemPromptPath !== null) return ["--system-prompt-file", handedPath(run.line, run.systemPromptPath)];

  const harnessed = extensionsOf(run).harnessedPath ?? null;

  if (harnessed !== null) return ["--system-prompt-file", handedPath(run.line, harnessed)];

  return ["--append-system-prompt-file", handedPath(run.line, extensionsOf(run).rulesPath)];
}

// The setting sources a line's program reads, as the line's carries name them (docs/launch-lines.md, 「What a line
// carries」), joined as the program's flag takes them; none for a line that names none.
export function settingSources(line: string): string {
  const carried = (declaredRunLines as Record<string, { carries?: { setting_sources?: string[] } }>)[line]?.carries;

  return (carried?.setting_sources ?? []).join(",");
}

function workerLine(run: HarnessRun): string[] {
  const args = programHead(run);

  args.push(
    "--model",
    run.model,
    "--effort",
    run.effort,
    "--permission-mode",
    "bypassPermissions",
    "--disallowedTools",
    run.toolsDenied ?? "",
    // The whole system prompt, never the CLI's own: a worker whose base prompt is decided — its
    // account's where the account names one, which wins over the agent's own choice, and the agent's
    // otherwise — is handed that base with the rules after it; one with none is handed the platform's
    // harness opening with the same rules after it (docs/base-prompts.md). A kept run written before
    // the harnessed file was composed still carries only its rules, appended as it was started.
    ...wholePrompt(run),
    // What this machine's own configuration used to supply, supplied from the repository
    // instead. User scope is shut off, so a skill, a co-worker definition or an MCP server the
    // machine happens to carry is no longer loaded beside the one that moved here — exactly one
    // copy of each reaches a job, and it is the copy a checkout on any machine holds. The
    // project and local scopes stay on: they are the worker's own checkout's, which is the
    // repository being edited, and shutting them off would take a checkout's own configuration
    // away from the jobs that work in it. The machine's own CLAUDE.md comes in through that project
    // scope — every checkout under the home directory walks up to it — so the settings below name it
    // in their exclusions, and the rules a job needs of it are the ones appended above.
    "--setting-sources",
    settingSources("worker"),
    // Every path this line hands over is answered for before it is written into the command: one
    // that is not there costs the run its whole start and says nothing about why (core/roots/launch-paths.ts).
    // A worker standing in its pane is handed its own settings merged with the status line and the
    // hooks its resident program writes through; every other run of the line reads the worker's.
    "--settings",
    handedPath(run.line, run.resident?.settingsPath ?? extensionsOf(run).settingsPath),
    // The MCP flag takes a list, so it never ends the line: anything after it that does not
    // begin a flag of its own would be read as another configuration file.
    "--mcp-config",
    handedPath(run.line, run.mcpConfigPath),
    "--plugin-dir",
    handedPath(run.line, extensionsOf(run).pluginDir),
    "--agents",
    extensionsOf(run).agentsJson,
  );

  return withPrompt(run, args);
}

// The chat line, and a supervisor's turn, which is of the same shape with its own prompt, tools and
// server. Its system prompt replaces the CLI's own, nothing of this machine rides along but the one
// MCP configuration it is handed, and a supervisor's program holds every tool of its own but the
// line's denied list in bypass-permissions mode, the same in every run of every supervisor on every
// road because the tools are the front of the cached prefix: owner-only work is the session's
// judgement, not a lock (docs/launch-lines.md, 「What a line's program is allowed, by road」).
//
// Standing in its pane the line is started as a person starts it and takes its rounds as they are
// spoken to it; it keeps its slash commands, which it folds and clears with, and is handed the
// settings naming its status line and hooks. Neither moves the prefix (docs/agent-control.md, 「The
// cache prefix」). The settings come after the tool lists, each of which would read any word after it
// as another tool.
function chatLine(run: HarnessRun): string[] {
  const args = programHead(run);

  args.push(
    "--model",
    run.model,
    "--effort",
    run.effort,
    "--system-prompt-file",
    run.systemPromptPath ?? "",
    "--setting-sources",
    settingSources(run.line),
    "--strict-mcp-config",
  );

  // The one MCP configuration the chat is handed, beside the strict flag that keeps every other
  // server out. The flag takes a list, so a flag of its own always follows it.
  if (run.mcpConfigPath !== null && run.mcpConfigPath !== "") {
    args.push("--mcp-config", handedPath(run.line, run.mcpConfigPath));
  }

  if (!run.hostCommands) {
    args.push("--disable-slash-commands");
  }

  args.push("--no-chrome");

  // A run that names what it is denied is a supervisor's: every tool of its own but those, each call
  // going ahead unasked, with no list of what it is shown or allowed. What it is shown and allowed stand
  // for a program that is told its tools, which this one is not. A limited supervisor's run also names the
  // only built-in tools it keeps.
  if (run.toolsDenied !== null) {
    args.push("--permission-mode", "bypassPermissions");

    if (run.toolsKept !== undefined && run.toolsKept !== null) {
      args.push("--tools", run.toolsKept);
    }

    args.push("--disallowedTools", run.toolsDenied);
  } else {
    // A run with no allow list allows what it is shown, every call going ahead unasked as a job's do.
    // Every other run is refused whatever it was not allowed, and a run that says nothing of what it is
    // shown takes everything the CLI offers.
    args.push("--permission-mode", run.toolsAllowed === null ? "bypassPermissions" : "dontAsk");

    if (run.toolsShown !== null) {
      args.push("--tools", run.toolsShown);
    }

    if (run.toolsAllowed !== null) {
      args.push("--allowedTools", run.toolsAllowed);
    }
  }

  if (run.resident?.settingsPath !== undefined && run.resident.settingsPath !== null) {
    args.push("--settings", handedPath(run.line, run.resident.settingsPath));
  }

  return withPrompt(run, args);
}

// The session to continue, and the prompt where the run takes it on the command line rather than
// on its input. Both close the line, in that order, so a resumed fold reads as the line it is
// followed by the word that folds it.
//
// A session named on the way in stands where the resume would, and never beside it: one says
// which session to open and the other says which to continue, and a run asking for both at once
// is asking for two different things.
function withPrompt(run: HarnessRun, args: string[]): string[] {
  if (run.resumeSessionId !== null) {
    args.push("--resume", run.resumeSessionId);
  } else if (run.openSessionId !== undefined && run.openSessionId !== null) {
    args.push("--session-id", run.openSessionId);
  }

  if (run.promptDelivery === "argument") {
    args.push(run.prompt);
  }

  return args;
}

function argsFor(run: HarnessRun): string[] {
  switch (run.line) {
    case "chat":
      return chatLine(run);
    case "worker":
      return workerLine(run);
    default:
      throw new Error(`no Claude Code line is written for the ${run.line} line`);
  }
}

// The account's login as the CLI is handed it. A directory account names the configuration home
// and the CLI reads the login it keeps there; a token account names the file its token sits in,
// and the token itself is what the CLI takes, so the file is opened here and its word goes no
// further than this one process's child; an endpoint account names its own configuration home, the
// base URL its requests go to, the key read from its file as the run's authorisation token and the
// model id its map names for each family.
//
// The three are not symmetrical, and the asymmetry is the point. A directory account clears any
// token the launching process happened to be carrying, because a token in the environment would
// answer for the run instead of the directory's own login and the account the owner chose would be
// quietly not the one that answered. A token account leaves the configuration directory alone:
// the token says which login answers, the directory says where the CLI keeps its records, and a
// token account has no records directory of its own — it uses the machine's, which is what the
// launching environment already names. An endpoint account clears every credential the launching
// process carried, because any of them would answer for the run instead of its key, and names the
// model family variables outright, an unmapped family included, so a stale one cannot reach the
// endpoint (docs/cache-fallback.md, 「The Claude Code road is a list of accounts」).
function accountEnv(account: AgentAccount | null | undefined): Record<string, string | undefined> {
  if (account === null || account === undefined) {
    return {};
  }

  const credential = account.credential;

  if (credential.kind === "token") {
    try {
      return { CLAUDE_CODE_OAUTH_TOKEN: readFileSync(credential.path, "utf8").trim() };
    } catch {
      // A launch that cannot reach its token is refused here rather than started and left to fail
      // as an authentication error nobody can place. What is said is which account, never what was
      // in the file or what was expected to be.
      throw new Error(`The token file of account ${account.id} cannot be read.`);
    }
  }

  if (credential.kind === "endpoint") {
    const { endpointUrl, keyFile } = credential;

    if (endpointUrl === undefined || endpointUrl === "" || keyFile === undefined || keyFile === "") {
      throw new Error(`Account ${account.id} names no endpoint to run on.`);
    }

    let key = "";

    try {
      key = readFileSync(keyFile, "utf8").trim();
    } catch {
      // Said below, so a file that cannot be read and one that holds nothing read alike.
    }

    if (key === "") {
      // The key goes into no log and no record: what is said is which account, never what was in
      // the file or what was expected to be.
      throw new Error(`The key file of account ${account.id} cannot be read.`);
    }

    const env: Record<string, string | undefined> = {
      CLAUDE_CONFIG_DIR: credential.path,
      ANTHROPIC_BASE_URL: endpointUrl,
      ANTHROPIC_AUTH_TOKEN: key,
      ANTHROPIC_API_KEY: undefined,
      CLAUDE_CODE_OAUTH_TOKEN: undefined,
    };

    for (const family of ENDPOINT_FAMILIES) {
      env[`ANTHROPIC_DEFAULT_${family.toUpperCase()}_MODEL`] = credential.models?.[family];
    }

    return env;
  }

  return { CLAUDE_CONFIG_DIR: credential.path, CLAUDE_CODE_OAUTH_TOKEN: undefined };
}

/**
 * Where the CLI keeps the records of a run started on this environment — its transcripts above
 * all. It writes them under the configuration directory it was handed, so a reader looks under
 * the same directory the run was given rather than under whichever one the process asking happens
 * to carry. A run handed none uses the machine's own.
 */
export function recordsDirIn(env: Record<string, string | undefined>): string {
  const named = env.CLAUDE_CONFIG_DIR;

  return named === undefined || named === "" ? machineConfigDir() : named;
}

// Thinking is turned off in the environment rather than by a flag, which is why a run that wants
// none is not a different line: the model, the effort, the system prompt and the absent tools are
// all as they were, so a gate that thinks and one that does not stand on the same prefix. The
// account's login is set the same way and for the same reason: no flag of the line moves, so a
// run moved to another account stands on the line it always stood on and only the cache it is
// warm in changes.
function envFor(run: HarnessRun): Record<string, string | undefined> {
  const env = { ...run.env, ...accountEnv(run.account) };

  return run.thinking ? env : { ...env, MAX_THINKING_TOKENS: "0" };
}

export const claudeCode: AgentAdapter = {
  name: "claude-code",
  streamFormat: "claude-code",
  // Everything a line may ask. This is the program the lines were written for, so there is no line
  // it cannot answer and no row of the table it refuses.
  offers: ["tool-gate", "mcp-only-tools"],
  foldPrompt,

  async command(run: HarnessRun): Promise<HarnessCommand> {
    return {
      args: argsFor(run),
      cwd: run.cwd,
      env: envFor(run),
      stdin: run.promptDelivery === "input" ? run.prompt : null,
    };
  },

  nativeChildReader: claudeNativeChildReader,
  ownWords: claudeOwnWords,
  longTools: claudeLongTools,
  streamsAs: "claude-code",

  sessionFromEvent(event: unknown): string | null {
    const named = (event as { session_id?: unknown } | null)?.session_id;

    return typeof named === "string" ? named : null;
  },

  // A streamed response as its host's pane shows it: what the model wrote, and the name of each tool
  // it called. A run printing one object at its end streams nothing that reads so.
  progressOf(line: string): string | null {
    let event: { type?: unknown; message?: { content?: unknown } };

    try {
      event = JSON.parse(line);
    } catch {
      return null;
    }

    if (event?.type !== "assistant" || !Array.isArray(event.message?.content)) return null;

    // A call made through the toolbook is shown as the tool it reached.
    const said = (event.message.content as { type?: unknown; text?: unknown; name?: unknown; input?: unknown }[])
      .map((block) => (block.type === "text" && typeof block.text === "string" ? block.text : block.type === "tool_use" && typeof block.name === "string" ? `→ ${calledTool(block.name, block.input).name}` : ""))
      .filter((part) => part.trim() !== "");

    return said.length === 0 ? null : said.join(" ");
  },

  // The CLI names the session every one of its answers was written in, so what comes back off one
  // of these runs carries more than the interface promises — a session id that is never null. The
  // callers that need that guarantee read the output directly; this is the door for whoever is
  // holding a harness rather than this one.
  async readAnswer(output: HarnessOutput, run?: HarnessRun): Promise<AgentReply> {
    const reply = readClaudeOutput(output.stdout, output.stderr, output.exitCode, run?.model ?? null);

    return {
      ...reply,
      model: run?.model ?? reply.model,
      effort: run?.effort ?? reply.effort,
      // Which login answered. The windows in this reply are that account's own share of its own
      // limit, and the program says nothing about whose they are — the run does, because the run
      // is what put this answer on that login. A reply nobody handed its run says nothing at all
      // about an account, and what writes it down reads that as the login this machine already
      // had; the reading itself stays the CLI's own, word for word.
      ...(run === undefined ? {} : { account: recordedAccount(run.account) }),
    };
  },
};

// What the CLI hands its status line, in the parts a resident turn is read by.
interface StatusReading {
  session_id?: unknown;
  model?: { id?: unknown };
  cost?: { total_cost_usd?: unknown };
  context_window?: {
    used_percentage?: unknown;
    current_usage?: { input_tokens?: unknown; cache_creation_input_tokens?: unknown; cache_read_input_tokens?: unknown } | null;
  };
  rate_limits?: Record<string, { used_percentage?: unknown; resets_at?: unknown } | undefined>;
}

// A transcript line in the parts only a resident turn needs beyond what the pane reader reads.
interface ResidentRecord {
  type?: string;
  subtype?: string;
  operation?: string;
  sessionId?: string;
  isSidechain?: boolean;
  isMeta?: boolean;
  message?: { content?: unknown };
}

const interruptedMark = "[Request interrupted by user";
// How the program opens a line of its own that tells the model a background task has moved.
const noticeMark = "<task-notification>";

const statusOf = (value: unknown): StatusReading | null => (typeof value === "object" && value !== null ? (value as StatusReading) : null);

function numberOf(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function spokenText(record: ResidentRecord): string {
  const content = record.message?.content;

  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";

  return content
    .map((block: { type?: unknown; text?: unknown }) => (block?.type === "text" && typeof block.text === "string" ? block.text : ""))
    .join("");
}

// Whether a line is words handed to the program — the round's prompt or words pushed into it — rather
// than a tool's answer, the program's own bookkeeping, a background task's notice or an interruption.
function handedWords(record: ResidentRecord): boolean {
  if (record.type !== "user" || record.isSidechain === true || record.isMeta === true) return false;

  const text = spokenText(record).trimStart();

  return text !== "" && !text.startsWith(noticeMark) && !text.startsWith(interruptedMark);
}

// A turn of the resident program out of what its record added since the turn went in and the two
// status readings either side of it (docs/agent-control.md, 「What a resident turn reads」).
//
// A word handed over is queued at once and taken either inside the turn or as a turn of its own once
// that one ends, so what was handed and not yet taken is the queued entries without a removal, and
// the turn has ended only when none is left and the end line stands after the last of them. Its
// answer is built turn by turn: a turn opened by words handed over writes the reply again whole and
// replaces it, and a turn the program opened on a background task's notice adds to it (「Words that
// arrive while a turn is working」); its tokens are every response; its money is the session's running
// total after it less the total before, and nothing where the two readings are not of the same
// session. The windows and the context are the reading after.
// What a round of the program still has coming, out of the record it wrote since the round went in
// (docs/agent-control.md, 「Words that arrive while a turn is working」): the words it was handed and
// has not yet taken, and the turns the round owes — the one it opened with, and one more for every
// stretch between two end lines in which words were taken off the queue. Words that waited together
// are taken off together and answered in one turn, and no stretch opens more than one, since every
// turn writes its end line. A word taken inside a turn is removed rather than taken off, and opens
// nothing; words pulled back into the input leave nothing queued. A word pushed after the turn had
// ended is never queued: the program takes it straight as a prompt, so words handed over that open
// a stretch nothing was taken off the queue for owe a turn of their own.
export function residentTurnsOwed(lines: readonly unknown[]): { queued: number; owed: number } {
  let queued = 0;
  let owed = 1;
  let takenSinceEnd = false;
  // Right after an end line, until the next words handed over: whether the turn that follows was
  // already counted by what was taken off the queue before the end.
  let afterEnd: { counted: boolean } | null = null;

  for (const record of lines as ResidentRecord[]) {
    if (record.type === "system" && record.subtype === "turn_duration") {
      afterEnd = { counted: takenSinceEnd };
      takenSinceEnd = false;
      continue;
    }

    if (afterEnd !== null && handedWords(record)) {
      if (!afterEnd.counted && !takenSinceEnd) owed += 1;
      afterEnd = null;
      continue;
    }

    if (record.type !== "queue-operation") continue;

    if (record.operation === "enqueue") queued += 1;
    else if (record.operation === "remove") queued = Math.max(0, queued - 1);
    else if (record.operation === "popAll") queued = 0;
    else if (record.operation === "dequeue") {
      queued = Math.max(0, queued - 1);
      if (!takenSinceEnd) owed += 1;
      takenSinceEnd = true;
    }
  }

  return { queued, owed };
}

export function readClaudeResidentTurn(input: ResidentTurnInput, run: HarnessRun): ResidentTurnReading {
  const records = input.lines as ResidentRecord[];
  const whole = readTurn(input.lines as Parameters<typeof readTurn>[0], 0);
  const { queued, owed } = residentTurnsOwed(input.lines);
  let ends = 0;
  let lastEnd = -1;
  let lastInterrupt = -1;
  let answer = "";
  const answers: TurnAnswer[] = [];
  let from = 0;

  records.forEach((record, at) => {
    if (record.type === "system" && record.subtype === "turn_duration") {
      lastEnd = at;
      ends += 1;
      const turn = input.lines.slice(from, at + 1);
      const text = readTurn(turn as Parameters<typeof readTurn>[0], 0).text;

      if (text !== "") {
        const opened = (turn as ResidentRecord[]).some(handedWords) ? "handed" : "program";

        answer = answer === "" || opened === "handed" ? text : `${answer}\n\n${text}`;
        answers.push({ text, opened });
      }
      from = at + 1;
    } else if (record.type === "user" && spokenText(record).startsWith(interruptedMark)) {
      lastInterrupt = at;
    }
  });

  const interrupted = lastInterrupt > lastEnd;
  const ended = !interrupted && queued === 0 && ends >= owed;
  const figures = statusFigures(input.statusBefore, input.statusAfter);
  const spend = figures.money !== null && whole.spend.length === 1 ? whole.spend.map((row) => ({ ...row, cost: figures.money as number })) : whole.spend;
  const sessionId = figures.sessionId ?? records.findLast((record) => typeof record.sessionId === "string")?.sessionId ?? null;

  return {
    state: interrupted ? "interrupted" : ended ? "ended" : "working",
    queued,
    error: whole.error,
    money: figures.money,
    reply: {
      mode: "claude-code",
      model: whole.model ?? figures.model ?? run.model,
      effort: whole.effort ?? run.effort,
      text: whole.error === null ? answer : "",
      sessionId,
      contextRatio: figures.contextRatio,
      contextTokens: figures.contextTokens ?? whole.contextTokens,
      sessionUsage: figures.sessionUsage,
      windows: figures.windows,
      spend,
      liveTasks: whole.liveTasks,
      workflows: whole.workflows,
      account: recordedAccount(run.account),
      ...(whole.error === null && answers.length > 0 ? { answers } : {}),
    },
  };
}

// What the two status readings either side of a turn say about it: its money — the running total's
// difference, and nothing where the two are not of one session — and, from the reading after it,
// the account's windows, the context the model is holding, the model and the session.
export function statusFigures(statusBefore: unknown, statusAfter: unknown): {
  money: number | null;
  windows: UsageWindowReading[];
  sessionUsage: SessionUsage | null;
  contextRatio: number | null;
  contextTokens: number | null;
  model: string | null;
  sessionId: string | null;
} {
  const before = statusOf(statusBefore);
  const after = statusOf(statusAfter);
  const sameSession = before !== null && after !== null && before.session_id === after.session_id;
  const totalBefore = numberOf(before?.cost?.total_cost_usd);
  const totalAfter = numberOf(after?.cost?.total_cost_usd);
  const windows = Object.entries(after?.rate_limits ?? {})
    .filter(([, window]) => numberOf(window?.used_percentage) !== null)
    .map(([name, window]) => ({
      name,
      utilization: (numberOf(window?.used_percentage) as number) / 100,
      resetsAt: numberOf(window?.resets_at),
    }));
  const short = windows.find((window) => window.name === "five_hour");
  const weekly = windows.find((window) => window.name === "seven_day");
  const used = numberOf(after?.context_window?.used_percentage);
  // What the model is holding, as the status line counts its last request: everything sent to it,
  // read from the cache or written into it.
  const current = after?.context_window?.current_usage;

  return {
    money: sameSession && totalBefore !== null && totalAfter !== null ? Math.max(0, totalAfter - totalBefore) : null,
    windows,
    sessionUsage:
      short === undefined || short.resetsAt === null
        ? null
        : { utilization: short.utilization, resetsAt: short.resetsAt, weeklyUtilization: weekly?.utilization ?? null, weeklyResetsAt: weekly?.resetsAt ?? null },
    contextRatio: used === null ? null : used / 100,
    contextTokens:
      current === undefined || current === null
        ? null
        : (numberOf(current.input_tokens) ?? 0) + (numberOf(current.cache_creation_input_tokens) ?? 0) + (numberOf(current.cache_read_input_tokens) ?? 0),
    model: typeof after?.model?.id === "string" ? after.model.id : null,
    sessionId: typeof after?.session_id === "string" ? after.session_id : null,
  };
}

// Claude Code's interactive program as the program an agent keeps standing in its pane: the line's
// own command without print mode, spoken to over herdr, taking words while it works, cut short by
// Escape and folded and cleared with its own commands (docs/agent-control.md, 「The interactive
// surface a provider declares」). Only a line that keeps a session has one to stand on.
export const claudeResident: ResidentSurface = {
  async command(run: HarnessRun): Promise<HarnessCommand> {
    if (!lineKeepsSession(run.line)) {
      throw new Error(`the ${run.line} line keeps no session for a resident program to stand on`);
    }

    return await claudeCode.command({ ...run, answerShape: "unread", promptDelivery: "channel", hostCommands: true });
  },
  door: "agent-prompt",
  takesWordsMidTurn: declaredClaudeCode.resident.words_mid_turn as boolean,
  readTurn: readClaudeResidentTurn,
  interrupt: { keys: declaredClaudeCode.resident.interrupt as string[] },
  fold: foldPrompt,
  clear: "/clear",
  identity: commandDigest,
  // The login's own program only: a Claude Code program on the registered models' road stands for
  // that road (「A road to another's endpoint」 below). A program on an endpoint account's endpoint
  // is this road's still, since the account is one of its logins.
  owns: (command) => command.args[0] === "claude" && !registeredRoad(command.env ?? {}),
};

// ─── A road to another's endpoint ───────────────────────────────────────────────────────────────
//
// The same program pointed at an endpoint that speaks Anthropic's Messages API for a model that is
// not Anthropic's. What differs from a run on a login is only where the requests go and what the
// program is told about the model there; the lines are the lines, and the flags are written here as
// every other flag is. Whose endpoint it is, which key opens it and what its model holds is the
// road's own and handed in: this knows no road.

// What a road hands the program it is carried on.
export interface ClaudeEndpoint {
  // The provider the road is, which every answer on it is filed under.
  provider: AgentProviderName;
  // Where the program's requests go.
  baseUrl: string;
  // The file the key is read from, on each request, by the program itself.
  keyPath(): string;
  // What the road holds for the model one run names, read as the run is started. A model the road
  // does not hold is refused here, with a line naming it, before anything is launched. The tools of
  // the program whose calls wait long are handed in by a run started on it, for the road to name.
  model(run: Pick<HarnessRun, "model" | "effort" | "thinking" | "placement" | "sampling">, longTools?: readonly string[]): Promise<EndpointModel>;
  // The effort word a run is sent, given whether it thinks and the word its tier mapping and agent chose:
  // the words the endpoint takes are its own.
  effort(run: { thinking: boolean; effort: string }): string;
  // Where a run handed over with no place of its own stands, on a road whose model is served in
  // several places, or null where it has one. Absent on a road that places nothing.
  place?(run: Pick<HarnessRun, "model">): Promise<string | null>;
  // The folder every home of this road stands under: a program whose configuration is under it is
  // this road's.
  homes: string;
  // What the road gives a line, which may be less than the program could: a line whose prompt and
  // tools the road's model cannot hold is not carried on it.
  offers: readonly RunCapability[];
  // Words the road itself writes into what the model reads, beside the program's own. Absent where
  // it writes none.
  ownWords?: readonly string[];
}

// One model on a road, as a run of it is sent.
export interface EndpointModel {
  // The model as the endpoint names it.
  name: string;
  // What the program shows it as at the top of its screen, where the endpoint calls it something
  // else for people than the name it is sent as.
  label?: string;
  // Its window, in tokens.
  contextTokens: number;
  // The answer ceiling of a run that names none.
  answerTokens: number;
  // The headers every request of the run carries; none where the endpoint reads none.
  headers?: Record<string, string>;
  // Whether the run thinks.
  thinking: boolean;
  // Where the program keeps this model's configuration and records, under the road's homes. It holds
  // no login.
  home: string;
}

// The variable a run names the account it is for in, and the one it names its place in.
const ownerAccountVariable = "BMO_OWNER_ACCOUNT";
const placeVariable = "BMO_AGENT_PLACE";

// The key file a run's requests are made with: the call key of the account the run is for, where agency issued
// that account one — every account but the deployment's own — else the road's own key (docs/own-llm.md, 「The
// keys」). A person's node holds no key but their call key, so a run of theirs never goes out on the platform's.
export function runKeyPath(endpoint: Pick<ClaudeEndpoint, "keyPath">, env: Record<string, string | undefined> | undefined): string {
  const account = env?.[ownerAccountVariable]?.trim() ?? "";
  const callKey = /^\d+$/.test(account) ? dataPath("env", "llm-gateway-calls", `${account}.key`) : null;

  return callKey !== null && usableKeyFile(callKey) ? callKey : endpoint.keyPath();
}

// The agent a place stands for: a thrown-away session's place read as its session's agent, as its record names
// it, and any other place as its own name.
function placeAgent(place: string): string {
  try {
    const record = JSON.parse(readFileSync(join(place, "apart-place.json"), "utf8")) as { place?: unknown; session?: unknown };
    const agent = typeof record.session === "string" ? record.session.split("@")[0] : "";

    if (record.place === basename(place) && agent) return agent;
  } catch {
    // A place with no record of a thrown-away session is its own agent's.
  }

  return basename(place);
}

// The headers every request of a run carries: the road's own, and the run's name, its place's agent, by which the
// gateway's request records are read for what the run spent (docs/supply.md, 「Reservations」).
export function runHeaders(headers: Record<string, string> | undefined, env: Record<string, string | undefined> | undefined): Record<string, string> | undefined {
  const place = env?.[placeVariable]?.trim();

  return place ? { ...headers, "x-bmo-run": placeAgent(place) } : headers;
}

// The longest wait the program's timers hold, about twenty-four days: the program gives a request up after a
// wait of its own and its runtime drops a connection silent past one, and neither takes a value meaning none,
// so both are told this one, which no request reaches.
const noWaitMs = 2_147_483_647;

// The endpoint's headers as the program reads them, one `name: value` to a line; none where it has none.
function customHeaders(headers: Record<string, string> | undefined): string | undefined {
  const lines = Object.entries(headers ?? {}).map(([name, value]) => `${name}: ${value}`);

  return lines.length === 0 ? undefined : lines.join("\n");
}

// The share of the window, in percent, the program is told to fold its conversation at: what the answer
// ceiling leaves of it, rounded down to a ten-thousandth, so a request at the fold still carries the whole
// ceiling (docs/own-llm.md, 「A turn at the input limit」).
function foldShare(window: number, answer: number): string {
  const millionths = Math.min(1_000_000, Math.max(1, Math.floor((Math.max(0, window - answer) * 1_000_000) / Math.max(1, window))));

  return `${Math.floor(millionths / 10_000)}.${String(millionths % 10_000).padStart(4, "0")}`;
}

// A run as it goes out on the road. Thinking is the road's effort word — a run's effort on a road is
// whether it thinks, and nothing more — and a run that does not think asks for no thinking tokens.
// Every model the program asks for a call of its own is the road's, so nothing it does asks a model
// the road does not serve. The window and the answer ceiling are said outright, since the program
// sizes a model it does not know at a window and an answer the road would refuse. The block the
// program puts at the head of every request, which changes each time, is left out, so the road's
// prefix cache holds; nothing is sent to the program's own services beside the model; and no request is
// given up on a clock: the program and the runtime under it, which otherwise drops a connection silent for
// minutes, a model still reading a long prompt among them, are told the longest wait their timers hold
// (docs/own-llm.md, 「How requests go」). No login
// goes with it: any a launching process carried is cleared, and the key is the settings' helper's.
async function endpointRun(endpoint: ClaudeEndpoint, run: HarnessRun): Promise<HarnessRun> {
  const held = await endpoint.model(run, claudeLongTools);
  const model = held.name;

  return {
    ...run,
    model,
    effort: endpoint.effort({ thinking: held.thinking, effort: run.effort }),
    thinking: held.thinking,
    account: null,
    env: {
      ...run.env,
      CLAUDE_CODE_OAUTH_TOKEN: undefined,
      ANTHROPIC_API_KEY: undefined,
      ANTHROPIC_AUTH_TOKEN: undefined,
      CLAUDE_CONFIG_DIR: held.home,
      ANTHROPIC_BASE_URL: endpoint.baseUrl,
      ANTHROPIC_MODEL: model,
      ANTHROPIC_DEFAULT_OPUS_MODEL: model,
      ANTHROPIC_DEFAULT_SONNET_MODEL: model,
      ANTHROPIC_DEFAULT_HAIKU_MODEL: model,
      CLAUDE_CODE_SUBAGENT_MODEL: model,
      CLAUDE_CODE_ATTRIBUTION_HEADER: "0",
      CLAUDE_CODE_MAX_CONTEXT_TOKENS: String(held.contextTokens),
      CLAUDE_CODE_MAX_OUTPUT_TOKENS: String(run.answerTokens ?? held.answerTokens),
      CLAUDE_AUTOCOMPACT_PCT_OVERRIDE: foldShare(held.contextTokens, run.answerTokens ?? held.answerTokens),
      CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: "1",
      API_TIMEOUT_MS: String(noWaitMs),
      BUN_CONFIG_HTTP_IDLE_TIMEOUT: String(Math.floor(noWaitMs / 1_000)),
      ANTHROPIC_CUSTOM_HEADERS: customHeaders(runHeaders(held.headers, run.env)),
    },
  };
}

// A word as the shell reads it back whole, quoted.
function shellWord(word: string): string {
  return `'${word.replaceAll("'", "'\\''")}'`;
}

// The settings a run on the road is handed: the line's own, where it has any, with the helper that
// prints the run's key file beside them, and the warning a program started in bypass mode stands on
// until a person accepts it passed over, as every login's own settings pass it (measured: the
// acceptance a configuration file records does not hold it back). The program takes one settings
// argument, so they are one file, written under the road's home by what it was made from and moved
// into place whole.
function keyedSettings(keyPath: string, home: string, given: string | null): string {
  const base: Record<string, unknown> = given === null
    ? {}
    : JSON.parse(given.trimStart().startsWith("{") ? given : readFileSync(given, "utf8"));
  const dir = join(home, "settings");
  const path = join(dir, `${createHash("sha256").update(given ?? "").digest("hex").slice(0, 16)}.json`);
  const fresh = `${path}.${process.pid}`;

  mkdirSync(dir, { recursive: true });
  writeFileSync(fresh, `${JSON.stringify({ ...base, apiKeyHelper: `cat ${shellWord(keyPath)}`, skipDangerousModePermissionPrompt: true }, null, 2)}\n`, { mode: 0o600 });
  renameSync(fresh, path);

  return path;
}

function withKeyedSettings(keyPath: string, home: string, args: readonly string[]): string[] {
  const at = args.indexOf("--settings");

  if (at !== -1) return args.map((word, index) => (index === at + 1 ? keyedSettings(keyPath, home, word) : word));

  // A flag and its value right behind the program's name come before anything a list could swallow.
  return [args[0] as string, "--settings", keyedSettings(keyPath, home, null), ...args.slice(1)];
}

// Whether a program's configuration folder is one of this road's homes.
function underHomes(endpoint: Pick<ClaudeEndpoint, "homes">, folder: string | undefined): boolean {
  return folder !== undefined && folder.startsWith(`${endpoint.homes}/`);
}

// A reply as the road's: filed under its provider, with no login, and costing nothing — the money the
// program works out for a model it does not know is its guess, not a price.
export function endpointReply(endpoint: Pick<ClaudeEndpoint, "provider">, reply: AgentReply): AgentReply {
  const { account: _account, ...rest } = reply;

  return { ...rest, mode: endpoint.provider, spend: reply.spend.map((row) => ({ ...row, mode: endpoint.provider, cost: 0 })) };
}

// The program on a road, as the adapter the road's provider is carried on.
export function claudeCodeAt(endpoint: ClaudeEndpoint): AgentAdapter {
  return {
    name: `claude-code:${endpoint.provider}`,
    streamFormat: "claude-code",
    offers: endpoint.offers,
    foldPrompt,

    async command(given: HarnessRun): Promise<HarnessCommand> {
      // A run that reaches the program with no place yet — one stood in a pane with nothing put in —
      // is placed here, once, so every request of its session goes to one place.
      const place = given.placement === undefined ? await endpoint.place?.(given) ?? null : null;
      const run = place === null ? given : { ...given, placement: place };
      const sent = await endpointRun(endpoint, run);
      return {
        args: withKeyedSettings(runKeyPath(endpoint, given.env), sent.env?.CLAUDE_CONFIG_DIR as string, argsFor(sent)),
        cwd: sent.cwd,
        env: envFor(sent),
        stdin: sent.promptDelivery === "input" ? sent.prompt : null,
      };
    },

    nativeChildReader: claudeNativeChildReader,
    sessionFromEvent: claudeCode.sessionFromEvent,
    progressOf: claudeCode.progressOf,
    ownWords: [...claudeOwnWords, ...(endpoint.ownWords ?? [])],
    longTools: claudeLongTools,
    streamsAs: "claude-code",

    async readAnswer(output: HarnessOutput, run?: HarnessRun): Promise<AgentReply> {
      // A resumed run is counted by what it spent itself (docs/own-llm.md, 「How requests go」).
      const reply = readClaudeOutput(output.stdout, output.stderr, output.exitCode, run?.model ?? null, { ownRun: run?.resumeSessionId != null });

      return endpointReply(endpoint, { ...reply, ...await heldContext(endpoint, run, reply.sessionId), model: run?.model ?? reply.model, effort: run === undefined ? reply.effort : run.effort || null });
    },
  };
}

// What a kept session on the road holds after a run of it: its last request, read off the record the
// program keeps under the model's home, against the window the road sized the model to. A run's result
// sums every request of a turn that called tools, and the road's stream carries no request's own
// count, so the record is what says it. Nothing where the run kept no session, the record names no
// request, or the model is not the road's.
async function heldContext(endpoint: ClaudeEndpoint, run: HarnessRun | undefined, sessionId: string | null): Promise<Partial<Pick<AgentReply, "contextTokens" | "contextRatio">>> {
  if (run === undefined || sessionId === null || !run.keepSession) return {};

  let held: EndpointModel;

  try {
    held = await endpoint.model(run);
  } catch {
    return {};
  }

  const tokens = readLastRequestTokens(transcriptPath(run.cwd, sessionId, cliProjectsRoot(held.home)));

  return tokens === null ? {} : { contextTokens: tokens, contextRatio: tokens / held.contextTokens };
}

// The first-run questions of an interactive program on one of the road's homes answered before it
// comes up: the colour theme, which a home holding no login asks for, and the folder, trusted.
export function settleEndpointHome(home: string, folder: string): void {
  mkdirSync(home, { recursive: true });

  const file = claudeConfigFile({ CLAUDE_CONFIG_DIR: home });

  markOnboarded(file);
  trustFolder(file, folder);
}

// The place's settings for a run on the road standing in an agent's pane: the status line and the
// hooks the place is read by, over the worker's own settings where the run is a worker's, with a
// person's typed line composed and a bot's turn's end held where the run's line declares each.
export function endpointPlaceSettings(run: HarnessRun, place: string, model: EndpointModel): string {
  const base = run.line === "worker" && run.extensions !== undefined
    ? JSON.parse(readFileSync(run.extensions.settingsPath, "utf8")) as Record<string, unknown>
    : {};
  // The model's row in the program's picker, whose label the program shows at the top of its screen
  // in place of the name the run is sent under, its node included (docs/own-llm.md, 「Claude Code in
  // the pane」).
  const named = model.label === undefined ? {} : { modelPicker: { options: [{ model: model.name, label: model.label }] } };

  const holds = lineHoldsStops(run.line);

  return writePlaceSettings(place, { ...base, ...named }, undefined, {
    holdForTasks: holds,
    holdForTextToolCall: holds,
    holdForWording: holds,
    composeTyped: lineComposesTypedLines(run.line),
  });
}

// The road's interactive program as the program an agent keeps standing in its pane: the line's own
// command on the road, without print mode, spoken to over herdr and read from its transcript. A
// program pointed at one of this road's homes is this road's, and no other's. A turn the bot puts in goes on
// in the session the program stands on where its caller asks for that and the agent holds it, and
// opens a session of its own otherwise; the words of the agent waiting while one works wait for the
// next.
// What the road's provider declares of its standing program: the keys that cut a working turn short, and
// whether words said while one works are taken into it.
export function claudeResidentAt(endpoint: ClaudeEndpoint, declared: { interrupt: readonly string[]; words_mid_turn: boolean }): ResidentSurface {
  const adapter = claudeCodeAt(endpoint);
  const surface: ResidentSurface = {
    async command(run: HarnessRun): Promise<HarnessCommand> {
      if (!lineKeepsSession(run.line)) {
        throw new Error(`the ${run.line} line keeps no session for a resident program to stand on`);
      }

      return await adapter.command({ ...run, answerShape: "unread", promptDelivery: "channel", hostCommands: true });
    },
    door: "agent-prompt",
    takesWordsMidTurn: declared.words_mid_turn,
    readTurn: (input, run) => {
      const reading = readClaudeResidentTurn(input, run);

      return { ...reading, reply: endpointReply(endpoint, reading.reply) };
    },
    interrupt: { keys: [...declared.interrupt] },
    fold: foldPrompt,
    clear: "/clear",
    identity: commandDigest,
    owns: (command) => command.args[0] === "claude" && underHomes(endpoint, command.env?.CLAUDE_CONFIG_DIR),
    turn: (request) => import("./claude-endpoint-turn").then(({ endpointTurn }) => endpointTurn({ ...request, surface, endpoint })),
  };

  return surface;
}

// The logins agency holds for this program, walked in the owner's order. The walk itself, the order and
// what makes a login open are the Claude road's own (`claude-dispatch.ts`, `claude-account.ts`).
const claudeLogins: AgentLogins = {
  held: (archive) => enabledClaudeAccounts(archive),
  recorded: (account) => recordedAccount(account),
};

// The account this program is signed in to, as a road the owner chooses. It is the road every
// line was written for, so there is no machine state that shuts it: a rest is the account's word
// about the moment, and auto is the policy that reads it.
export const claudeCodeProvider: AgentProvider = {
  name: "claude-code",
  label: declared.label,
  adapter: claudeCode,
  unavailable: async () => null,
  models: async () => claudeCatalogueModels(),
  efforts: claudeEfforts,
  modelEfforts: claudeEffortsFor,
  effortFault: declared.effortFault,
  // An agent's level is the `--effort` word where the model's catalogue entry names it, and its
  // thinking off is no thinking tokens in the environment.
  reasoning: levelReasoning(async (model) => levelsIn(await claudeEffortsFor(model))),
  capsAnswerTokens: declared.capsAnswerTokens,
  logins: claudeLogins,
  resident: claudeResident,
};

