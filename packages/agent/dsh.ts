import { randomUUID } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { allowsWhatItShows, handedRules } from "./harness";
import { agentsRoot } from "@agency/support/roots";
import { openRouterApiBase, openRouterKeyEnv, openRouterModels } from "./openrouter";
import { levelReasoning, levelsIn } from "./reasoning";
import { memberProvider } from "./member-provider";
import { MCP_TOKEN_VARIABLE, TOOL_HOST_WAIT_SECONDS, bearerVariable, httpServerEntry } from "./tool-host";
import { missingOnMachine } from "./provider-facts";
import { providerDeclaration } from "./provider-declaration";
import declaredOpenRouter from "@agency/policy/providers/openrouter.toml";

import type { AgentReply } from "./agent-reply";
import type { AgentAdapter, AgentProviderName, HarnessCommand, HarnessOutput, HarnessRun, AgentProvider } from "./harness";

// dsh — the DeepSeek Harness — as the second program agency runs a line on. It answers the same
// description of a run the Claude Code implementation answers, and it answers it in dsh's own
// vocabulary: nothing here is a flag, because dsh has almost none. What a dsh run is comes out of
// a patch overlay — a list of rows, each naming one plugin of the composed profile and either
// carrying that row's configuration or switching it off — and the overlay is written fresh for
// every run.
//
// What it cannot answer it says so about rather than pretending, and there are five of those. dsh
// assembles its own system prompt and the run's is one section of it, so a preamble of dsh's own
// always stands in front of what agency wrote, where the CLI's prompt file replaces everything. dsh's
// tools are its own and are named nothing like the CLI's, so a run does not name a few of them by
// the CLI's names: it takes none of them, or the filesystem rows its file tools stand for with the
// shell beside them only where the run is shown one — the one mapping agency makes onto them — and a
// job takes the whole working set. The run's effort is named only where an agent chose a level, for
// the reason under the route below. dsh always writes a session log of its own and headless takes one
// task, so there is no session for agency to keep or to resume and every reply here names none. And headless prints
// the answer and nothing else, so what a run cost is not in what the process left behind: the reply
// carries no spend, and a run through this harness is recorded without figures rather than with
// invented ones.

// The version agency runs dsh at. It is pinned rather than floating because a harness is its whole
// composition — which rows the profile mounts, the ids they are addressed by, the sections its
// prompt is assembled from — and the overlay below addresses rows by id. `@deepseek-ai/dsh`
// without a version fetches whatever is newest, so a release that renamed a row would leave a run
// holding tools it was told not to have, or with no route to a model at all: the same command
// standing for a different program on a different day. It moves when somebody reads what moved
// with it.
const version = "0.2.1-rc.2";

// The profile that runs one task and prints the answer. dsh's other profiles open a server or a
// browser UI, and neither is a thing a bot spawns.
const profile = "headless";

// The persona prefix the headless profile of the pinned version opens its system prompt with, which
// a job's rules are written after. It belongs to the version above: `{{model}}` is dsh's own prompt
// variable, filled as the prompt is rendered, and the working directory is dsh's own runtime context.
const headlessOpener = "You are a coding agent powered by the {{model}} model.";

// The word dsh's own commands are reached by. Nothing here ever asks for it: headless keeps no
// session agency could resume, so there is no session of dsh's to fold.
const foldPrompt = "/compact";

// Every row of the pinned profile that puts a tool in front of the model, and the plan mode that
// puts one more there on its own account. A run shown nothing has all of them switched off, which
// is the only way dsh takes that instruction — there is no list to cut down to. The list belongs
// to the version above and is half the reason that version is pinned.
const toolRows = [
  "tool-bash",
  "tool-pwsh",
  "tool-jobs",
  "tool-fs",
  "tool-fs-search",
  "tool-skill",
  "tool-subagent-control",
  "tool-subagent-list-agents",
  "tool-subagent",
  "tool-subagent-fork",
  "tool-workflow",
  "tool-todo",
  "tool-goal",
  "tool-str-replace-editor",
  "tool-working-directory",
  "tool-web",
  "plan-mode",
];

// Two rows switched off in every run agency makes. The first reads the instruction files lying
// around the working directory, which is what the CLI's empty setting sources shut off on the
// other side: what an agent of agency's is told comes from this repository and never from whatever
// tree the run happens to stand in. The second spends a second model call giving the session a
// title, and nothing ever opens the session it would be titling.
const quietRows = ["agent-instructions", "session-title-llm"];

// The tool rows a run's shown file tools stand for — reading, searching and editing files — and
// the shell, which is on only where the run is shown Bash. A chat is shown the shell and the
// filesystem on the Claude road (Bash, Read, Write, Edit) and gets both; a run shown file tools
// alone (Read, Write, Edit, Glob, Grep) gets the filesystem with no shell, so what it
// may touch is exactly what the sandbox below lets a file tool touch. The names are not
// translated one for one — dsh's tools are named nothing like the CLI's — but what those names
// are is a shell and a filesystem, and these are dsh's. The agentic rows neither is shown —
// subagent, workflow, todo, goal, skill, the working-directory tool, web, plan mode — stay off with
// the rest.
const fileToolRows = ["tool-fs", "tool-fs-search", "tool-str-replace-editor"];
const shellToolRow = "tool-bash";

function shownRows(run: HarnessRun): string[] {
  const shown = (run.toolsShown ?? "").split(",").map((tool) => tool.trim());

  return shown.includes("Bash") ? [shellToolRow, ...fileToolRows] : fileToolRows;
}

// One row of a patch overlay: a plugin of the composed profile, addressed by the id the profile
// gave it. A row either reconfigures that plugin or switches it off, and `name` appears only on a
// row being inserted rather than addressed.
// A provider's road as dsh's one route: the name dsh knows it by, which is the provider's own, the
// variable its key is read from, where its API answers, and — for a road whose key stands in a file of
// the deployment's own rather than in the process's variables — that file.
export interface DshRoute {
  provider: AgentProviderName;
  keyEnv: string;
  apiBase(): string;
  keyFile?: () => string;
}

// The public gateway agency's stand-ins are picked from.
export const openRouterRoute: DshRoute = {
  provider: "openrouter",
  keyEnv: openRouterKeyEnv,
  apiBase: openRouterApiBase,
};

// A text of agency's as dsh's prompt takes it word for word: dsh reads every `{{name}}` in a persona as
// a variable of its own and refuses the whole prompt on a name it has not registered, so each `{{` is
// broken by a word joiner, which the model reads past.
export function dshLiteral(text: string): string {
  return text.replaceAll("{{", "{\u2060{");
}

export interface OverlayRow {
  id: string;
  name?: string;
  disabled?: boolean;
  config?: Record<string, unknown>;
}

// An entry of the overlay: a change to a row the profile holds, or rows it inserts beside them.
export type OverlayEntry = OverlayRow | { insert: OverlayRow[] };

interface McpServerEntry {
  command?: unknown;
  args?: unknown;
  env?: unknown;
}

interface McpConfigFile {
  mcpServers?: Record<string, McpServerEntry>;
}

/**
 * Where dsh keeps its profiles, its sessions and its storages. It goes under the root the agents'
 * own deployed documents go under rather than under the user's home, which is where dsh would put
 * it left alone: a run of agency's writes where agency writes, and nothing else on this machine reads
 * what it leaves. Everything under it is scratch — no part of it is read back.
 */
export function dshHome(): string {
  return join(agentsRoot(), "dsh");
}

/**
 * What file effects a run may have, in dsh's own words, and null for a run that composes none.
 *
 * dsh fences file writes to a workspace root of its own, set to the directory the run works in —
 * this repository for a chat, so a write lands where the room's scripts already write. The mode is
 * what the run allows measured against what it shows: a run that allows exactly what it shows (every
 * chat with its tools on) writes within that root;
 * one that allows less reads but does not write; a job may do anything, since editing a checkout
 * is what it is for. dsh has no per-command allow list, so a script-by-script grant would have no
 * equal here — read-only is what stands in its place. The mode fences
 * file writes alone: reads and the network are open under any of them.
 *
 * It is one word rather than two because dsh reads it as one. The mode sets the sandbox, and the
 * policy on whether a run stops to ask before it acts is a second knob that dsh pairs with the
 * first: its permission table names the pairs, and a pair the table cannot name is refused as it
 * composes, before a word is sent anywhere. So the mode is handed over as the one name dsh
 * resolves both ends of, rather than set on the sandbox alone and left to meet whatever the other
 * knob happened to hold.
 */
export function dshPermissionMode(run: HarnessRun): string | null {
  if (run.toolsShown === "") {
    return null;
  }

  return run.line === "worker"
    ? "danger-full-access"
    : allowsWhatItShows(run)
      ? "workspace-write"
      : "read-only";
}

// The overlay for one run, written to a file of its own because two runs may be in flight at
// once. It is written as JSON, which YAML reads: a chat's system prompt runs to hundreds of
// kilobytes of arbitrary Markdown, and JSON is the one way to put it in a file without a quoting
// rule of our own standing between the document and what the model is told.
//
// No secret is in it. The route names the environment variable the key is read from and never
// the key, so the file may be read by anyone who can read the directory and still say nothing;
// the key itself travels in the run's environment.
export function dshOverlay(run: HarnessRun, route: DshRoute = openRouterRoute): OverlayEntry[] {
  const rows: OverlayEntry[] = [];

  if (run.systemPromptPath !== null) {
    rows.push({ id: "system-prompt", config: { personaPrefix: dshLiteral(readFileSync(run.systemPromptPath, "utf8")) } });
  } else if (run.line === "worker") {
    // A job keeps dsh's own prompt and is handed the worker's rules beside it, as the Claude line
    // appends them to that CLI's. The persona prefix is the one place in dsh's system prompt a
    // run writes, and the headless profile fills it with its opening line, so the rules follow
    // that line there rather than taking its place.
    rows.push({ id: "system-prompt", config: { personaPrefix: `${headlessOpener}\n\n${dshLiteral(handedRules(run))}` } });
  }

  for (const id of quietRows) {
    // A worker keeps the harness's instruction-file reader: unlike the supervisor and a gate it has no
    // complete system-prompt file of its own, and the checkout's AGENTS.md/CLAUDE.md is where the
    // repository tells a coding agent how work is finished. The title call is scratch everywhere.
    if (run.line !== "worker" || id !== "agent-instructions") {
      rows.push({ id, disabled: true });
    }
  }

  // Shown nothing means every tool row off — a judgement, and a chat whose tools the owner has
  // turned off. Shown something means the rows those tools stand for on and every other tool row
  // off: the filesystem, the shell where one is shown, and none of the agentic rows. A job takes
  // the whole working set but the web, which reaches it through agency's own server. What the tools
  // may do is dsh's own sandbox, set below.
  if (run.toolsShown === "") {
    for (const id of toolRows) {
      rows.push({ id, disabled: true });
    }
  } else {
    const working = new Set(run.line === "worker" ? toolRows.filter((id) => id !== "tool-web") : shownRows(run));

    for (const id of toolRows) {
      if (!working.has(id)) {
        rows.push({ id, disabled: true });
      }
    }

    rows.push({ id: "sandbox-policy", config: { mode: dshPermissionMode(run), workspaceRoot: run.cwd } });
  }

  // The gateway agency's stand-ins are picked from, declared as a route of its own with the model
  // this run wants on it. It is the one route this harness knows, because it is the one agency has
  // models for; another gateway would be another route, written here the day there is one.
  //
  // dsh refuses outright — before a word is sent — a model asked for a reasoning level it does not
  // offer, and which levels a model offers is a property of whatever model the owner registered
  // rather than of the line being stood in for. So a run that thinks with no effort is left at the
  // model's own, and a run that must not think names off, the one level every model answers to. A
  // run that thinks at a level its agent chose declares that level as the one its model offers and
  // asks for it, which goes out as OpenRouter's `reasoning.effort`: the gateway carries it to the
  // routed model in that model's own terms (docs/model-tiers.md, 「An agent's own thinking and
  // effort」).
  //
  // An Anthropic model is asked for Anthropic's one-hour cache rather than its five-minute default,
  // the same lifetime the Claude road runs on, so a turn's prefix outlives the keepalive interval
  // instead of expiring between pings. pi-ai marks the cache breakpoints for these ids on its own
  // and adds the longer lifetime only where both switches below are set; no other model has them.
  const anthropic = run.model.startsWith("anthropic/");
  // A level is only ever an agent's: an OpenRouter mapping names none, and a word a run's line was
  // written with before any mapping was laid on it is no choice of anybody's for this model.
  const level = !run.thinking ? "off" : run.reasoning?.effort == null || run.effort === "" ? null : run.effort;
  const modelReasoning: { reasoningEfforts?: Record<string, string | null>; compat?: { thinkingFormat: "openrouter" } } =
    level === null || level === "off" ? {} : { reasoningEfforts: { off: null, [level]: level }, compat: { thinkingFormat: "openrouter" } };

  rows.push({
    id: "llm-pi-ai",
    config: {
      providers: {
        [route.provider]: {
          apiKeyEnv: route.keyEnv,
          baseURL: run.env.BMO_DSH_LOCAL_URL ?? route.apiBase(),
          api: "openai-completions",
          ...(level === null ? {} : { reasoning: level }),
          ...(anthropic ? { cacheRetention: "long" } : {}),
          models: [{ id: run.model, ...modelReasoning, ...(anthropic ? { compat: { ...modelReasoning.compat, supportsLongCacheRetention: true } } : {}) }],
        },
      },
    },
  });

  rows.push({ id: "agent-default-model", config: { provider: route.provider, model: run.model } });

  // The clients are rows the profile does not hold, so they stand in one insert entry: a row naming an id
  // the profile lacks is a change to nothing, which dsh mounts nothing for.
  const servers = mcpRows(run);

  if (servers.length > 0) {
    rows.push({ insert: servers });
  }

  return rows;
}

// The MCP servers the run may reach, each as a client of its own. Agency writes its server files in
// the CLI's shape — a dict of names, each the tool host's URL or a command to spawn — and dsh takes one
// row per server with the name as the namespace its tools are called under, so the translation is a
// rename and nothing more. A server agency wrote as neither is not one this reads.
function mcpRows(run: HarnessRun): OverlayRow[] {
  if (run.mcpConfigPath === null || run.mcpConfigPath === "") {
    return [];
  }

  const file = JSON.parse(readFileSync(run.mcpConfigPath, "utf8")) as McpConfigFile;
  const rows: OverlayRow[] = [];

  for (const [name, server] of Object.entries(file.mcpServers ?? {})) {
    const http = httpServerEntry(server);
    const token = http === null ? null : bearerVariable(http.headers);

    // A server on the tool host is a Streamable HTTP row, its bearer header read from dsh's own
    // environment, where the run's token is.
    if (http !== null) {
      rows.push({
        id: `mcp-${name}`,
        name: "@deepseek-ai/dsh-mcp-client",
        config: {
          transport: "streamable-http",
          serverName: name,
          url: http.url,
          headers: token === null ? http.headers : { ...http.headers, Authorization: bearerFromEnvironment(token) },
          toolCallTimeoutMs: TOOL_HOST_WAIT_SECONDS * 1000,
          failOnStartupError: false,
        },
      });
    } else if (typeof server.command === "string") {
      rows.push({
        id: `mcp-${name}`,
        name: "@deepseek-ai/dsh-mcp-client",
        config: {
          transport: "stdio",
          serverName: name,
          command: server.command,
          args: Array.isArray(server.args) ? server.args : [],
          env: typeof server.env === "object" && server.env !== null ? server.env : {},
          cwd: run.cwd,
          failOnStartupError: false,
        },
      });
    }
  }

  return rows;
}

// A header that dsh evaluates from its own environment when it loads the overlay, which a JSON value
// cannot say: it is written as a placeholder and laid into the overlay's text as YAML's `!!js` tag.
const environmentMark = "bmo-bearer-from-environment:";

function bearerFromEnvironment(variable: string): string {
  return `${environmentMark}${variable}`;
}

// The overlay as dsh reads it: JSON, which YAML reads too, with each placeholder replaced by the tagged
// expression it stands for.
function overlayText(overlay: unknown, variables: readonly string[]): string {
  let text = `${JSON.stringify(overlay, null, 2)}\n`;

  for (const variable of variables) {
    text = text.split(JSON.stringify(bearerFromEnvironment(variable))).join(`!!js ${JSON.stringify("`Bearer ${process.env." + variable + "}`")}`);
  }

  return text;
}

// The overlay on disk, under a name no other run in flight can be holding.
function overlayFile(run: HarnessRun, route: DshRoute): string {
  const dir = join(dshHome(), "overlays");
  const path = join(dir, `${run.line}-${randomUUID()}.yml`);

  mkdirSync(dir, { recursive: true });
  writeFileSync(path, overlayText(dshOverlay(run, route), [MCP_TOKEN_VARIABLE]));

  return path;
}

// How much of a stderr tail is worth keeping beside an exit code: enough to name what went
// wrong, short enough to sit in a log line beside everything else about the run.
const failureTailChars = 300;

/**
 * What dsh said about a run that did not answer. A failure is one line on stderr in the harness's
 * own form, and the rest of stderr is the model's reasoning as it was written, so the line is
 * looked for rather than the tail taken.
 *
 * Where there is no such line, whatever went wrong was not dsh's to say — the package was never
 * fetched, the process was killed, the gateway refused the key — and the exit code alone names
 * none of those. So the last line that is not the model thinking aloud is carried with the code,
 * because the alternative is a record of the failure that is a single number.
 */
export function dshFailure(output: HarnessOutput): string {
  const lines = output.stderr
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "");
  const aloud = (line: string): boolean => !line.startsWith("dsh: reasoning");
  // dsh's own word about the failure, wherever in the line it stands. It writes one plainly on a
  // line of its own when a run it had started went wrong, and wraps the same sentence in a thrown
  // error when what failed was composing the run at all — an overlay it cannot make sense of
  // never reaches a gateway, and that shape is the one such a failure takes. Reading only the
  // plain form left the record naming whatever the runtime printed last, which is its own version
  // banner: a line that says nothing about anything.
  const said = lines.findLast((line) => line.includes("dsh: ") && aloud(line));

  if (said !== undefined) {
    return said.slice(0, failureTailChars);
  }

  const tail = lines.findLast(aloud);

  return tail === undefined ? `exit ${output.exitCode}` : `exit ${output.exitCode}: ${tail.slice(0, failureTailChars)}`;
}

// The most one argument may hold. The kernel refuses to start a process any one of whose arguments
// is 128 KiB or more, and a room's batch for the person book runs past that, so a task that long
// would never reach dsh at all. This stays well inside the limit whatever the text is made of.
const wordBytes = 100_000;
// UTF-8 spends at most three bytes on one UTF-16 unit, so this many units always fit.
const wordUnits = Math.floor(wordBytes / 3);

/**
 * The task as the words headless is handed, each short enough to be one argument.
 *
 * Headless joins its task words with one space, so the task is cut at a space and that space is
 * dropped: joined again it is the task exactly. A word never begins with a hyphen, since the
 * command line would read that word as an option of its own. A stretch too long to hold any such
 * space is cut where it runs out, never inside a surrogate pair, and there alone the joined task
 * carries one space more than was written. A NUL cannot stand in an argument at all and is left
 * out.
 */
export function taskWords(prompt: string): string[] {
  const words: string[] = [];
  let rest = prompt.replaceAll("\0", "");

  while (Buffer.byteLength(rest) > wordBytes) {
    let cut = rest.lastIndexOf(" ", wordUnits);

    while (cut > 0 && rest[cut + 1] === "-") {
      cut = rest.lastIndexOf(" ", cut - 1);
    }

    if (cut > 0) {
      words.push(rest.slice(0, cut));
      rest = rest.slice(cut + 1);
      continue;
    }

    let end = /[\uD800-\uDBFF]/.test(rest[wordUnits - 1] ?? "") ? wordUnits - 1 : wordUnits;

    while (end > 1 && rest[end] === "-") {
      end -= 1;
    }

    words.push(rest.slice(0, end));
    rest = rest.slice(end);
  }

  return [...words, rest];
}

// The task's words on disk, NUL after each, under a name no other run in flight can be holding.
function taskFile(run: HarnessRun): string {
  const dir = join(dshHome(), "tasks");
  const path = join(dir, `${run.line}-${randomUUID()}.words`);

  mkdirSync(dir, { recursive: true });
  writeFileSync(path, taskWords(run.prompt).map((word) => `${word}\0`).join(""));

  return path;
}

// What npx runs once the pinned package is on its path. npx hands its command to a shell as one
// string, and the kernel refuses that string whole once it is as long as a long task, so the task
// never travels in it: the shell becomes bash, bash reads the words out of the task file, removes
// the file, and becomes dsh with each word an argument of its own. Both steps replace the process
// rather than start a child, so a signal meant for the run reaches dsh and dsh's exit is the exit.
const launch = `exec bash -c 'mapfile -d "" -t words < "$1"; rm -f -- "$1"; shift; exec dsh "$@" "\${words[@]}"' bmo-dsh-task`;

function shellWord(word: string): string {
  return `'${word.replaceAll("'", `'\\''`)}'`;
}

// dsh on one provider's route. Every provider whose runs go through dsh is this adapter on its own
// route: the name says which, and the reply is filed under that provider.
export function dshOn(route: DshRoute, name = `dsh:${route.provider}`): AgentAdapter {
  return {
  name,
  // Every line whose run headless can answer as one task: a judgement with no tools, and a room's
  // turn or a job, whose
  // prompt already carries what it needs and whose road keeps no session on this program. The
  // runners are not carried: their tools are agency's own MCP servers and co-worker gate, and a run
  // without them would come back with nothing to record, so it offers neither.
  offers: [],
  foldPrompt,
  failure: dshFailure,

  // One task, one answer. The prompt stands in dsh's command line whatever the run asked, because
  // headless reads its task from there and nothing from its input — as the words it is cut into,
  // brought in from the task file by the launch above. The overlay carries what the run is made
  // of, and both files are written as this is called.
  async command(run: HarnessRun): Promise<HarnessCommand> {
    const mode = dshPermissionMode(run);

    return {
      args: [
        "npx",
        "--yes",
        "--package",
        `@deepseek-ai/dsh@${version}`,
        "--call",
        [launch, taskFile(run), "--profile", profile, "--patch", overlayFile(run, route), ...(run.resumeSessionId === null ? [] : ["--session-id", run.resumeSessionId])].map((word, at) => (at === 0 ? word : shellWord(word))).join(" "),
      ],
      cwd: run.cwd,
      // dsh keeps everything it writes under one directory named here, so a run of agency's leaves
      // nothing in the home directory of whoever the process is running as. The permission mode
      // stands beside it because that is where dsh takes it whole: the overlay row it also stands
      // on sets the sandbox alone, and the profile resolves this name into every knob that hangs
      // off it. A run that composes no file effects names none and keeps dsh's own.
      env: { ...run.env, ...routeKey(route), DSH_HOME: dshHome(), ...(mode === null ? {} : { DSH_PERMISSION_MODE: mode }) },
      stdin: null,
    };
  },

  // Headless prints the answer and nothing else, so every line it writes is the answer as it comes.
  progressOf(line: string): string | null {
    return line.trim() === "" ? null : line;
  },

  // Everything dsh reports is the text: the answer on stdout, the model's reasoning on stderr, and
  // an exit code. A run that ended any other way answers nothing, and what it said about that is
  // read by the caller through the failure above rather than carried in a reply.
  async readAnswer(output: HarnessOutput, run?: HarnessRun): Promise<AgentReply> {
    return {
      mode: route.provider,
      model: run?.model ?? null,
      effort: null,
      text: output.exitCode === 0 ? output.stdout.trim() : "",
      sessionId: null,
      contextRatio: null,
      contextTokens: null,
      sessionUsage: null,
      windows: [],
      spend: [],
      liveTasks: [],
      workflows: [],
    };
  },
};
}

// The key a route reads from a file of the deployment's own, as the variable the route names; none for a
// route whose key is already among the process's variables.
function routeKey(route: DshRoute): Record<string, string> {
  if (route.keyFile === undefined) return {};

  try {
    return { [route.keyEnv]: readFileSync(route.keyFile(), "utf8").trim() };
  } catch {
    return {};
  }
}

// OpenRouter's gateway is the route this program was first written for, and keeps its name.
export const dsh: AgentAdapter = dshOn(openRouterRoute, "dsh");

// What OpenRouter declares of itself, its own file of the shared policy: among it its own effort
// words, which its gateway carries to whichever model a request is routed to.
const declared = providerDeclaration(declaredOpenRouter);
const openRouterEfforts = declared.efforts;
const openRouterLevels = levelsIn(openRouterEfforts);

// The public gateway, reached with the key in the environment, is the one road this program carries.
export const openRouterProvider: AgentProvider = memberProvider({
  name: "openrouter",
  policy: declaredOpenRouter,
  reasoning: levelReasoning(async () => openRouterLevels),
});
