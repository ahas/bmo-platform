import chatLine from "@agency/policy/chat-line.toml";
import { TOOL_HOST_AUTHORIZATION, toolHostUrl } from "./tool-host";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";

import { agentsRoot, assetRoot, dataRoot } from "@agency/support/roots";
import declaredBrowser from "@agency/policy/browser.toml";
import promptLayers from "@agency/policy/prompt-layers.toml";
import { systemPromptFile } from "./prompt-files";

// Every agent the bot runs is given exactly the documents under prompts/agents/<name>/ plus the
// rulebooks under prompts/rules/ that name it as a reader, and nothing else. This module is the one
// place that composes those documents into what an agent sees; the launch lines read from
// here, and deployAgents writes the same composition out where a person can inspect it.

// The kind of room a prompt is composed for, as the road it belongs to names it.
export type ChatKind = string;

export interface DeployedFile {
  path: string;
  bytes: number;
}

const rulesDir = join(assetRoot(), "prompts", "rules");
const agentsDir = join(assetRoot(), "prompts", "agents");
const platformDir = join(assetRoot(), "prompts", promptLayers.platform_folder as string);
const runnerNames = ["search", "browse"] as const;

// Where the runners' working directories are deployed (`agentsRoot`).
export const deployRoot = agentsRoot;

export function runnerCwd(name: (typeof runnerNames)[number]): string {
  return join(deployRoot(), name);
}

export function browseMcpConfigPath(): string {
  return join(runnerCwd("browse"), ".mcp.json");
}

// Every line's configuration names the tool host's namespaces its line uses.
function linePath(name: string): string {
  return join(deployRoot(), `${name}.mcp.json`);
}

// One namespace of the tool host as a configuration names it: its URL, and the header that hands
// the run's token, read from the program's own environment.
export function hostServer(namespace: string): Record<string, unknown> {
  return { type: "http", url: toolHostUrl(namespace), headers: { Authorization: TOOL_HOST_AUTHORIZATION } };
}

// Every line is handed the toolbook beside its namespaces, at the end, where it reaches the tools its
// namespaces do not list.
function mcpFile(servers: Record<string, unknown>): string {
  return `${JSON.stringify({ mcpServers: { ...servers, toolbook: hostServer("toolbook") } }, null, 2)}\n`;
}

// The MCP configuration a job worker's line is handed, and no other line is — which is why the
// file is named for the worker rather than for either of the namespaces in it. It carries two: the
// web one every agent that may use the web is given, and the worker's own (docs/worker-mcp.md).
// Its contents are hashed into the worker prefix, so a namespace added here moves that digest.
export function workerMcpConfigPath(): string {
  return linePath("worker");
}

// The two namespaces a worker is handed, as its configuration file holds them. The worker's own — the
// graph's reading verbs and the queue's two — is not folded into the web one because that one is
// shared with the search and browse runners, and the graph of this repository and the owner's queue
// are no business of a runner line's run: a tool shown to a line that will never call it is paid for in
// that line's prompt on every turn.
export async function workerMcpConfig(): Promise<string> {
  return mcpFile({ web: hostServer("web"), worker: hostServer("worker") });
}

// The search runner's standing MCP configuration: the web namespace, its tools shown by what each run's
// record holds. Every run on the line is handed it, the labels its caller named in its record.
export function searchMcpConfigPath(): string {
  return linePath("search");
}

// The variables through which a program finds a person's graphical session, and the arguments every
// Chrome agency starts carries so that nothing of that session is asked for: agency's policy
// (docs/policy.md).
const browserPolicy = declaredBrowser as { session_variables: string[]; detached_arguments: string[] };

// A runner line's MCP configuration: the web namespace alone, what a run's server is told traveling
// in the run's environment, which its session record is read from. A browse run's sits beside the
// chrome-devtools server from the browse agent's own document. Its Chrome never reaches a person's
// session bus or keyring, and on the visible strategy it is shown the private display the runner
// started instead of its headless flag. The server's environment is laid over the runner's, so a
// session variable is emptied rather than left out.
export async function runnerMcpConfig(options: {
  kind: "search" | "browse";
  browserStrategy?: string;
  display?: { display: string; xauthority: string; runtimeDir: string } | null;
}): Promise<string> {
  const web = hostServer("web");

  if (options.kind !== "browse") {
    return mcpFile({ web });
  }
  const own = JSON.parse(readFileSync(join(agentsDir, "browse", ".mcp.json"), "utf8")) as { mcpServers?: Record<string, { args?: string[]; env?: Record<string, string> }> };
  const servers: Record<string, unknown> = { ...(own.mcpServers ?? {}) };
  const chrome = own.mcpServers?.["chrome-devtools"];
  if (chrome !== undefined) {
    const visible = options.browserStrategy === "visible" && options.display ? options.display : null;
    const chromeArgs = browserPolicy.detached_arguments.map((arg) => `--chromeArg=${arg}`);
    const args = (chrome.args ?? []).filter((arg) => !(visible !== null && arg === "--headless"));
    const emptied = Object.fromEntries(browserPolicy.session_variables.map((name) => [name, ""]));
    servers["chrome-devtools"] = {
      ...chrome,
      args: [...args, ...chromeArgs.filter((arg) => !args.includes(arg))],
      env: {
        ...(chrome.env ?? {}),
        ...emptied,
        DBUS_SESSION_BUS_ADDRESS: "disabled:",
        ...(visible === null ? {} : { DISPLAY: visible.display, XAUTHORITY: visible.xauthority, XDG_RUNTIME_DIR: visible.runtimeDir }),
      },
    };
  }
  return mcpFile({ ...servers, web });
}

// The browse runner's standing MCP configuration, as a run on the headless strategy is given it.
async function browseMcpConfig(): Promise<string> {
  return runnerMcpConfig({ kind: "browse" });
}

function readRequired(path: string): string {
  return readFileSync(path, "utf8").trim();
}

// A document of one agent's own folder under the prompts, as it stands in the asset root.
export function agentFile(agent: string, file: string): string {
  return readRequired(join(agentsDir, agent, file));
}

// A rulebook as an agent is given it: named, so the agent can say which rule it applied.
export function rulebook(name: string): string {
  return `RULEBOOK prompts/rules/${name}.md:\n${readRequired(join(rulesDir, `${name}.md`))}`;
}

// One of the platform's rulebooks, as the platform administrator's agents are given it: named, as every
// rulebook is (docs/prompt-layers.md).
export function platformRulebook(name: string): string {
  return `RULEBOOK prompts/${promptLayers.platform_folder}/${name}.md:\n${readRequired(join(platformDir, `${name}.md`))}`;
}

// The rulebook every agent that acts — the supervisor, the workers and their co-workers and the runners
// — is given, forbidding it to destroy the system it runs in. A gate holds no tool and is not
// given it.
export const selfPreservation = "self-preservation";

export function render(template: string, vars: Record<string, string | number>): string {
  return template.replaceAll(/\{\{(\w+)\}\}/g, (match, key: string) => {
    const value = vars[key];

    return value === undefined ? match : String(value);
  });
}

// The ABSOLUTE RULES, as every agent that speaks to people opens with them: bare, under their own
// heading line, since the manuals refer to them by number as ABSOLUTE RULES rather than as a file.
// One copy serves every supervisor's prompt (docs/supervisors.md).
export function absoluteRules(): string {
  return readRequired(join(rulesDir, chatLine.system.absolute_rules));
}

// The platform's harness opening, the first layer of every whole prompt a line hands Claude Code
// (docs/base-prompts.md, 「The harness layer」).
export function harness(): string {
  return readRequired(join(assetRoot(), "prompts", "harness.md"));
}

// The chat's documents around the absolute rules and the rulebooks it reads, as the chat line's policy
// names them for both sides, opened by the harness and the platform's rules a supervisor of no account carries.
export function chatSystemStatic(_kind?: ChatKind): string {
  return [harness(), ...(chatLine.system.platform as string[]).map((name) => platformRulebook(name)),
    agentFile("chat", chatLine.system.opening), absoluteRules(), agentFile("chat", chatLine.system.manual),
    ...(chatLine.system.rulebooks as string[]).map((name) => rulebook(name))].join("\n\n");
}

// The chat's system prompt with the base persona after it under its header, where the caller hands one:
// the persona is the caller's, read from where it keeps it (docs/launch-lines.md).
export function chatSystem(basePersona?: string): string {
  const persona = basePersona?.trim() ?? "";

  return persona === "" ? chatSystemStatic() : `${chatSystemStatic()}

${chatLine.persona.header}
${persona}`;
}

export { systemPromptFile };

// The translator renders one part of a document into the language its prompt names.
export function translatorSystem(): string {
  return agentFile("translator", "instructions.md");
}

// The same gate rendering a text for a job worker rather than for a person reading a document: the
// worker cannot read the original, so what a person said is translated too (`docs/worker-language.md`).
export function workerTranslatorSystem(): string {
  return agentFile("translator", "worker.md");
}

// The same gate the other way: a worker's English record on an item rendered into the Korean the
// owner reads it in, under the plain-writing rulebook every text a person reads follows
// (`docs/worker-language.md`, 「What a worker writes onto an item」).
export function recordTranslatorSystem(): string {
  return [rulebook("plain-writing"), agentFile("translator", "record.md")].join("\n\n");
}

// A runner line's instructions are handed in whole by whoever starts a session on it, and stand as its
// system prompt in place of the CLI's own; what one run is about is its prompt (docs/launch-lines.md).
// Nothing of them is composed here.

export function workerTrailer(): string {
  return render(agentFile("worker", "trailer.md"), {
    skills: join(workerPluginDir(), "skills"),
    plainWriting: join(rulesDir, "plain-writing.md"),
  });
}

const workerDir = join(agentsDir, "worker");

// What a job worker is given beyond its prompt, and where it comes from.
//
// The skills and co-worker definitions a job runs with come from this repository and the store,
// never from this machine's own configuration. Skills are repository assets. Role documents seed
// the roles once; the callable roles' rows, each with its own order, are what every launch
// serializes, so the owner edits the roles without a second live list (docs/agent-roles.md).
//
// The skills are a plugin directory rather than a checkout's own configuration directory,
// because a run works in whichever repository's checkout its work names and only one of those is this
// repository: a plugin is loaded by path and reaches every worker alike. The co-workers are
// handed over as JSON instead, because a plugin's agents are named under the plugin and the
// verification discipline names them plainly.
export function workerPluginDir(): string {
  return join(workerDir, "plugin");
}

function writeDeployed(into: DeployedFile[], path: string, text: string): void {
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text.endsWith("\n") ? text : `${text}\n`);
  into.push({ path, bytes: Buffer.byteLength(text) });
}

// A run's system prompt file is written by the launch that builds the run, just before the run
// starts, and the run reads it from then on — a Codex thread reads it again on every exec. So a
// file a run may still be reading was written no longer ago than a run can stand, and only such
// files are carried into a rebuilt tree, each with its own modification time so the next carry
// can still tell. A file no launch has written for longer than that is one nothing reads any
// more, and it is left behind with the tree it stood in; the next launch that wants it writes it
// again. A dotted name is a file still being written beside its prompt (`systemPromptFile`), which
// no run reads, so it is never carried.
const promptCarryMs = 24 * 60 * 60 * 1000;

export function carryPrompts(from: string, into: string, now = Date.now()): void {
  if (!existsSync(from)) {
    return;
  }

  mkdirSync(into, { recursive: true });

  for (const name of readdirSync(from)) {
    const path = join(from, name);
    const stat = statSync(path);

    if (stat.isFile() && !name.startsWith(".") && now - stat.mtimeMs <= promptCarryMs) {
      cpSync(path, join(into, name), { preserveTimestamps: true });
    }
  }
}

// A program standing in a place reads the files its command names for as long as it stands, and starts again on
// them whenever it dies, however long ago a launch wrote them: a supervisor's MCP configuration and system prompt
// are written by its turns, not by this deployment. So every file under the tree that a place's program names —
// its commands' words and the files it says it needs — is carried into the rebuilt tree whatever its age, where
// the deployment wrote no file of that path itself. A tree rebuilt from under a standing program would leave it
// naming a file that is gone (docs/agent-documents.md, 「How the documents are composed」).
export function carryNamed(root: string, into: string, places: string): void {
  if (!existsSync(places)) {
    return;
  }

  for (const place of readdirSync(places)) {
    let program: { command?: { args?: unknown }; restart?: { args?: unknown } | null; needs?: unknown };

    try {
      program = JSON.parse(readFileSync(join(places, place, "program.json"), "utf8"));
    } catch {
      continue;
    }

    const words = [program.command?.args, program.restart?.args, program.needs].flatMap((list) => (Array.isArray(list) ? list : []));

    for (const word of words) {
      if (typeof word !== "string" || !word.startsWith(`${root}/`)) {
        continue;
      }

      const carried = join(into, relative(root, word));

      if (existsSync(carried) || !existsSync(word) || !statSync(word).isFile()) {
        continue;
      }

      mkdirSync(dirname(carried), { recursive: true });
      cpSync(word, carried, { preserveTimestamps: true });
    }
  }
}

// What one agent document could not be composed into, and what stands in its place.
export interface AgentFault {
  // Where the document belongs inside the deployed tree.
  path: string;
  // Why composing it failed, in the words the failure came with.
  reason: string;
  // Whether the copy the last deployment wrote was carried over in its place.
  carried: boolean;
}

// What a deployment wrote, and what it could not compose.
export interface AgentDeployment {
  files: DeployedFile[];
  faults: AgentFault[];
}

// The one line a fault is said in, wherever a deployment is made: the document, the reason, what
// stands in its place and what to do about it. A stack trace carries the path and nothing of what
// it means, and whoever reads the log is reading it after the service has already given up, so the
// line has to answer the question by itself.
export function agentFaultLine(fault: AgentFault): string {
  const instead = fault.carried
    ? "The copy the last deployment wrote stands in its place."
    : "No copy stands in its place, so the document goes without.";

  return `The agent document ${fault.path} could not be composed: ${fault.reason} ${instead} A document and the code reading it come from the same tree, so this happens where a commit moved the document without the code following it; a commit mending it clears it once it is deployed.`;
}

// Writes what every agent is given to the deploy root: the composed prompt of each chat kind and of
// the translate runner, and for each runner line the MCP servers it gets, the browse line's in the
// directory it runs in. The deployed documents
// are rebuilt on every call, so nothing stale survives. The prompts directory is different: it
// holds the runtime files the runs in flight read, so the recent ones are carried into the
// rebuilt tree rather than erased underneath those runs.
//
// The build happens beside the root that is being read and is swung over only once every file
// is written, the way a console release is. A run that refuses the source root outright throws
// before anything is swung over, and emptying the live root first would leave every runner
// without the configuration it launches with until somebody noticed.
// No gate's prompt is written here: a gate hands its own with each turn it sends.
export async function deployAgents(): Promise<AgentDeployment> {
  const root = deployRoot();
  const building = `${root}.building`;
  const written: DeployedFile[] = [];
  const faults: AgentFault[] = [];
  // A configuration a line is handed, at the place inside the tree being built that the line
  // will look for it. The line and the deploy name the file once between them: a file renamed
  // where the line reads it cannot be left written under its old name here.
  const asHanded = (path: string): string => join(building, relative(root, path));

  // One document composed into the tree being built. The documents come out of the same tree as the
  // code reading them, so a checkout moving one reaches nothing here; what can still fail is a
  // commit whose code and documents disagree with each other. A document is therefore composed on
  // its own: the fault is kept and named, the copy the last deployment wrote stands in its place
  // where there is one, and the rest of the tree is built around it. What is lost is one agent's
  // document until the next deployment; what is kept is every other agent, the chat, the queue and
  // the control port.
  const compose = async (path: string, text: () => Promise<string>): Promise<void> => {
    try {
      writeDeployed(written, path, await text());
    } catch (error) {
      const standing = join(root, relative(building, path));
      const carried = existsSync(standing);

      if (carried) {
        mkdirSync(dirname(path), { recursive: true });
        cpSync(standing, path);
      }

      faults.push({ path: relative(building, path), reason: error instanceof Error ? error.message : String(error), carried });
    }
  };

  // The asset root has to be this repository before anything is built out of it. A root missing
  // either directory is not a document that moved but the wrong tree altogether, and a deployment
  // made from it would stand an empty tree in front of the running one and take every agent away
  // at once. This is the one refusal the path makes; everything past it is tolerated.
  for (const needed of [rulesDir, agentsDir]) {
    if (!existsSync(needed)) {
      throw new Error(`${needed} is missing: ${assetRoot()} is not agency's tree.`);
    }
  }

  rmSync(building, { recursive: true, force: true });
  mkdirSync(building, { recursive: true });

  await compose(join(building, "chat", "system.md"), async () => chatSystem());
  await compose(join(building, "translator", "system.md"), async () => translatorSystem());
  await compose(join(building, "translator", "worker.md"), async () => workerTranslatorSystem());
  await compose(join(building, "translator", "record.md"), async () => recordTranslatorSystem());
  await compose(asHanded(searchMcpConfigPath()), () => runnerMcpConfig({ kind: "search" }));
  // Each runner line's run goes out in the directory it is deployed to, which stands even where the line
  // is handed nothing of its own there.
  for (const name of runnerNames) {
    mkdirSync(asHanded(runnerCwd(name)), { recursive: true });
  }
  await compose(asHanded(browseMcpConfigPath()), browseMcpConfig);
  await compose(asHanded(workerMcpConfigPath()), workerMcpConfig);
  await compose(join(building, "worker", "trailer.md"), async () => workerTrailer());

  carryPrompts(join(root, "prompts"), join(building, "prompts"));
  carryNamed(root, building, join(dataRoot(), "agent-places"));

  const previous = `${root}.previous`;

  rmSync(previous, { recursive: true, force: true });

  if (existsSync(root)) {
    renameSync(root, previous);
  }

  renameSync(building, root);
  rmSync(previous, { recursive: true, force: true });

  return { files: written.map((file) => ({ ...file, path: join(root, relative(building, file.path)) })), faults };
}
