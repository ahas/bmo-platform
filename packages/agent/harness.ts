import { readFileSync } from "node:fs";
import declaredKeepalive from "@agency/policy/keepalive.toml";

import type { AgentReply } from "./agent-reply";
import type { Archive } from "@agency/archive/archive";
import type { AgentHome } from "./herdr-run";
import type { OwnLlmService, OwnLlmSettings } from "./own-llm";
import type { AgentReasoning, ProviderReasoning } from "./reasoning";
import type { TierMapping } from "@agency/archive/model-tiers";
import type { HerdrAnswer } from "./herdr";
import { agentProviderNames } from "@agency/archive/provider-names";
import type { AgentProviderName, AgentSlot } from "@agency/archive/provider-names";

// A line is the identity of a run — what the run is for rather than the program that carries it —
// and it is the shape a harness is asked whether it can carry: what a line needs of its program is
// the same whichever work key is running on it. Which lines there are is the deployment's own
// (`run-lines.ts` for agency's); a line is named here by the name its deployment gave it.
export type RunLine = string;

// What a line may need of the program that carries it, and what a program may offer:
//   - `tool-gate`: every tool call the run makes passes a hook of the caller's that can refuse it;
//   - `mcp-only-tools`: the run's whole tool set can be the MCP servers it is handed, with the
//     program's own tools withheld.
export const runCapabilities = ["tool-gate", "mcp-only-tools"] as const;

export type RunCapability = (typeof runCapabilities)[number];

// A line as its deployment declares it.
export interface RunLineDeclaration {
  name: RunLine;
  // What its program has to offer for the line to go out on it.
  needs: readonly RunCapability[];
  // Whether its turns follow one another on one session: a room's rounds come off one common queue
  // and a worker's turns are held one at a time by its lock. Every other line is a call that
  // stands alone and may overlap another of the same agent, which one program holding one
  // conversation cannot carry.
  keepsSession: boolean;
  // Whether a line a person types into the line's standing program is handed, by the place's
  // submit hook, the composition a turn the deployment puts in would carry. Left out, it is handed
  // nothing.
  composesTypedLines?: boolean;
  // Whether the place's stop hook holds a bot's turn's end while its background work runs, while its
  // final answer is a tool call written as text, or while that answer uses a word the glossary bans.
  // Left out, it holds none.
  holdsStops?: boolean;
  // What its runs may reach of the tool host, as capabilities. A run is issued a session record holding
  // these, narrowed where it holds less. Absent on a line that is handed no tools.
  capabilities?: readonly string[];
  // The tool host namespaces a run of the line is issued. Absent on a line that is handed no tools.
  namespaces?: readonly string[];
  // The entries a run of the line is shown by full name in their namespaces.
  shown?: readonly string[];
}

// Whether a program carries a line: it offers everything the line needs.
export function carriesLine(adapter: Pick<AgentAdapter, "offers">, line: Pick<RunLineDeclaration, "needs">): boolean {
  return line.needs.every((need) => adapter.offers.includes(need));
}

// What a keepalive says, agency's policy's (docs/policy.md). A ping is upkeep for the
// prefix a line or a session is cached on — nobody reads its answer — and the word stands here with
// the rest of what a run is made of rather than with any one program's.
export const keepalivePrompt: string = declaredKeepalive.prompt;

// What comes back out of a run: a stream to be read as it arrives, one object at the end, the
// bare text, or nothing at all. A run whose result nobody follows asks for the object; a run
// something watches while it works asks for the stream.
//
// The fourth is a run that prints nothing to anybody, because it is not being read from outside
// at all — it is being watched where it works, and its answer is read afterwards out of the record
// it wrote for itself. A run like that is the only one that can be spoken to while it is going,
// which is the whole reason it gives up printing.
export type AnswerShape = "stream" | "object" | "text" | "unread";

// How the prompt is handed over. Written to the run's input, standing on the command line as an
// argument of its own, or spoken to the run once it is up — the first two exist because the
// harnesses differ on which they take, and a fold in particular is asked for the way its own
// harness accepts it.
//
// The third is for a run that is not started with its work: it comes up, and then it is told. A
// run reached that way can be told a second thing later, which is the point; the command carries
// no prompt at all and whoever is holding the run delivers it.
export type PromptDelivery = "input" | "argument" | "channel";

// The login a run goes out under, where its provider holds more than one of them. The id is what
// the run's own records are kept under, the name is what the owner calls it, and the credential is where
// that login is reached: a path, and what kind of thing stands at it. A credential that is an
// endpoint rather than a login also names where its requests go, the file its key sits in and the
// model id each family word stands for there. None of it is read here — what the fields mean, and
// what a program is handed to make it use that login rather than another, is the provider's own.
export interface AgentAccount {
  id: string;
  name: string;
  credential: {
    kind: string;
    path: string;
    endpointUrl?: string;
    keyFile?: string;
    models?: Readonly<Record<string, string | undefined>>;
  };
}

// The request fields a run's sampling names: each a number by the name the model's server reads it
// under, and the chat template's own words, each on, off or a number, under `chat_template_kwargs`.
export type Sampling = Readonly<Record<string, number | Readonly<Record<string, number | boolean>>>>;

// What a run is, in agency's own words and no CLI's. Every field is named for what it means to the
// run, so a second harness answers the same questions in whatever words its own program uses:
// nothing here is a flag, and nothing here may become one.
export interface HarnessRun {
  tier?: string;
  line: RunLine;
  model: string;
  // How hard the run thinks. The words are the harness's own — Claude Code's are medium, high
  // and max — so this is passed through rather than interpreted here.
  effort: string;
  // The system prompt as a path to a file rather than as text. Both harnesses take it that way,
  // and for the same reason: a chat's prompt runs to tens of kilobytes and no command line will
  // hold it, and the file is what the prompt cache is keyed on, so handing over the path is
  // handing over the identity of the prefix. Null where the run keeps the harness's own prompt,
  // which is what a job worker does.
  systemPromptPath: string | null;
  // What the run is shown, what it may use, and what is kept from it. Shown and allowed are two
  // questions and not one: the tool definitions are the front of a cached prefix, so what is
  // shown is the same in every run of a line, and privilege is decided by what is allowed.
  // Null means the run says nothing about it and takes whatever the harness gives it.
  toolsShown: string | null;
  toolsAllowed: string | null;
  toolsDenied: string | null;
  // The only built-in tools a run that names what it is denied holds beside that, where a supervisor's tool
  // limit narrows its program (docs/supervisors.md, 「What each capability stands for」). Absent or null where
  // the program keeps every built-in tool but those it is denied.
  toolsKept?: string | null;
  // The file naming the MCP servers this run may reach, or null for a run that reaches none.
  mcpConfigPath: string | null;
  // What those servers show, where the file naming them does not settle it — a digest of their tools
  // for the prefix to move with. Absent where the file is the whole of it.
  mcpTools?: string;
  prompt: string;
  // Whether this run is a keepalive touch, whatever its words say.
  ping?: boolean;
  // The room's own block, and the one line that restates it to a session holding this version of it
  // from earlier.
  roomContext?: { chatId: number; text: string; mark: string };
  promptDelivery: PromptDelivery;
  // The session this run continues, and whether a session is kept at all. A judgement keeps none:
  // it is one prompt in and one text out, and a session file for it would be a file nobody reads.
  resumeSessionId: string | null;
  keepSession: boolean;
  // What the run's work is about, where its caller names it — a job's item. A note left by a
  // replaced session goes only to a run on the thread that session was on, so one piece of work's
  // memory never lands at the head of another's prompt. Absent is a thread of its own.
  thread?: string;
  // The session this run is to be opened as, where the caller names it rather than learning it
  // afterwards. A run that prints its result says which session it answered in; a run that prints
  // nothing cannot, so the name is decided before it starts and the record it writes is then
  // findable without asking anybody. Null for every run that has its session named back to it.
  openSessionId?: string | null;
  ephemeral?: boolean;
  // Whether the harness's own commands are available to the run. They are off wherever a run
  // answers a room or judges something, and on for the fold, which is asked for with one.
  hostCommands: boolean;
  // Whether the model thinks before it answers. Off for a judgement that needs none, where the
  // thinking is what pushed the run past its timeout.
  thinking: boolean;
  // The agent's own thinking and effort, as its row names them, where the run is an agent's: what each
  // provider makes of it as a tier mapping is laid on the run. Absent on a run no agent's choice
  // reaches — a gate, a co-worker, a keepalive of a place no agent owns.
  reasoning?: AgentReasoning;
  // The ceiling on the answer's tokens, where the run's provider caps them (`capsAnswerTokens`) and
  // the key's tier named one. Absent where the provider's own default stands.
  answerTokens?: number;
  // The sampling settings the run is asked with, each by the name the model's server reads it under,
  // where its caller names any; a road that can carry them sends them, and any other leaves them.
  sampling?: Sampling;
  answerShape: AnswerShape;
  cwd: string;
  env: Record<string, string | undefined>;
  // Which of its provider's logins this run goes out under. A run that names none goes out on
  // whatever login its program finds by itself, which is what every run did before there was
  // more than one to choose between.
  account?: AgentAccount | null;
  // Where among its provider's places this run's session stands — one node of a model served on
  // several — as the provider named it when the session opened. A run that resumes a session goes
  // back to the place that session opened on; absent where the provider has no places to choose.
  placement?: string;
  // Where the agent's resident program keeps what agency reads of it while it stays up, for a run that
  // starts one: the agent's place under the data root, and the settings file that names the status
  // line and the hooks writing into it. Absent on every run a host carries.
  resident?: ResidentPlace | null;
  // What a line that loads its program's own extensions is handed — the settings, the plugin, the
  // co-worker definitions and the rules appended to the program's own prompt — set by whoever
  // composes the line. Absent on a line that loads none.
  extensions?: RunExtensions;
}

export interface RunExtensions {
  settingsPath: string;
  pluginDir: string;
  // The co-worker definitions, as the one JSON object the program takes them in.
  agentsJson: string;
  // The rules every job is held to, which close the worker's prompt.
  rulesPath: string;
  // The whole system prompt a worker with no base prompt is handed: the platform's harness opening with
  // the rules after it; absent in a kept run written before it was composed.
  harnessedPath?: string | null;
}

export interface ResidentPlace {
  // The agent's own directory: the records a resident program's hooks and status line write, the
  // current run's name, and — for a program agency runs itself — the inbox its turns arrive in.
  place: string;
  // The settings file a program that reads one is started with, or null where it reads none.
  settingsPath: string | null;
}

// How a turn reaches a program that stays up in its pane. A program herdr recognises as an agent is
// spoken to over herdr like a person would type; a program agency runs itself reads its own inbox.
export type TurnDoor = "agent-prompt" | "inbox";

// How a working turn of a resident program is cut short. Keys the program itself reads as an
// interruption, where that has been measured to leave the session usable; otherwise the program is
// ended and its pane's loop starts it again on the same session.
export type ResidentInterrupt = { keys: readonly string[] } | { endProgram: true };

// What the program's own record added since a turn went in, as the lines it wrote, and the two
// readings of its running totals either side of the turn where the program hands those over apart
// from its record. What a line or a reading holds is the program's own shape: only its surface reads it.
export interface ResidentTurnInput {
  lines: readonly unknown[];
  statusBefore?: unknown;
  statusAfter?: unknown;
}

// Where a resident turn stands. It is going until the record says it ended; one that was cut short
// says that instead, and is not an answer.
export type ResidentTurnState = "working" | "ended" | "interrupted";

export interface ResidentTurnReading {
  state: ResidentTurnState;
  // Words the program was handed that it has not yet taken in. A round ends only when this is zero.
  queued: number;
  // The program's own words where it wrote them in the answer's place, and nothing otherwise.
  error: string | null;
  // What the turn cost in money as a whole, where the program reports it, and null where it does
  // not; the reply's spend carries the tokens.
  money: number | null;
  reply: AgentReply;
}

// A program a provider can keep standing in its agent's pane: how it is started, how it is spoken to,
// how what it did is read back, and how it is stopped, folded and cleared. A provider with none says
// so, and an agent on it is carried by its host.
export interface ResidentSurface {
  // The program to start in the pane, on the session the run names. It carries no prompt: turns go
  // in afterwards, through the door.
  command(run: HarnessRun): Promise<HarnessCommand>;
  door: TurnDoor;
  takesWordsMidTurn: boolean;
  readTurn(input: ResidentTurnInput, run: HarnessRun): ResidentTurnReading;
  interrupt: ResidentInterrupt;
  // The words the program takes as an order to fold or to clear its session, or null where the
  // program has none and a fresh session is a new program on the handed-over summary.
  fold: string | null;
  clear: string | null;
  // What the standing program is compared by before every turn: a program started on a different
  // command is a different program, and is started again between turns.
  identity(command: HarnessCommand): string;
  // Whether a program standing in a pane was started by this surface, read off its command — its
  // words, and the environment it was handed where that is what tells two surfaces' programs apart —
  // on any model or session: a pinned worker's place holding a program its pin's surface does not
  // own has it replaced by the standing pass (docs/agent-control.md, 「A worker standing with nothing
  // to do」).
  owns(command: { args: readonly string[]; env?: Record<string, string | undefined> }): boolean;
  // One turn of the program from start to end, for a surface that carries its own turns: the program
  // stood up by its command in the agent's pane, the turn put in through its door, and read back until
  // it has ended. A surface whose turns its lines run themselves declares none.
  turn?: (request: ResidentTurnRequest) => Promise<ResidentTurnResult>;
}

// A turn put into an agent's resident program: the run it is, the agent whose pane holds the program,
// what the run is called and what it is, and the work it belongs to, so a stop of that work reaches it.
// No clock goes with it: the turn lasts until it ends or is stopped.
export interface ResidentTurnRequest {
  run: HarnessRun;
  home: AgentHome;
  name: string;
  title: string;
  root?: string;
  parent?: string | null;
  socket?: string;
  // Anything to say to the turn while it works, for a surface whose program takes words mid-turn; a
  // surface that takes none leaves it unasked.
  aside?: TurnAside;
}

// Something to say to a working turn, asked on every look and handed the way to say it: the words
// appended to a job's item, or said to the worker while it works.
export type TurnAside = (say: (text: string) => Promise<HerdrAnswer<unknown>>) => Promise<void>;

// How the turn came out: an answer, or the reason there is none, in the words a failed step of the
// fallback walk is carried in.
export type ResidentTurnResult = { reply: AgentReply } | { error: string };

// A run made ready to start: what to spawn, where, with what around it, and what to write to its
// input. Null input is a run whose prompt is already standing in the command.
export interface HarnessCommand {
  args: string[];
  cwd: string;
  env: Record<string, string | undefined>;
  stdin: string | null;
}

// What a finished process left behind, before anybody has read a meaning into it.
export interface HarnessOutput {
  stdout: string;
  stderr: string;
  exitCode: number;
}

// One program agency can run its lines on.
//
// What a new harness has to write is the whole of this: a name it is known by in code and logs,
// the lines it can really carry, the word that makes it fold its own session, a command for each
// of those lines, and a reader that turns what the process left behind into the reply every caller
// already reads. Its command is free to answer a run however its own program is spoken to — flags
// in another order, a patch file, a profile, an argument where Claude Code takes input — because
// nothing outside this interface may look at the command's parts. The reader is the other half of
// that and holds to it just as strictly: nothing outside the implementation takes apart what a run
// printed, so a caller that wants the answer, the session, the windows, what the run cost or why
// it failed asks the harness rather than reaching into the output. A reader that cannot say what a
// run reported leaves the figure null rather than guessing: a harness that keeps no session names
// none, and one whose program says nothing about the account's windows carries none.
//
// What it does not write is everything above it. It composes no documents — the system prompt
// arrives as a path — decides no policy about who may use which tool, records no usage, judges
// nothing about the account, and never chooses which work runs on it. It is handed a run and
// answers with a command, and it is handed a finished process and answers with a reply.
export interface AgentAdapter {
  name: string;
  // How the run log reads what this program prints as it works, in the provider's name whose
  // stream it is: its own, which a road carrying the program under another name still prints.
  // Absent where it prints nothing the log reads as events.
  streamFormat?: AgentProviderName;
  // What this program can really give a line. A program that cannot withhold its own tools or
  // gate each call cannot be a runner however plainly a route says it is, and a route saying so
  // would only buy a run that threw on its first answer — so the implementation names what it
  // offers, and a provider whose program does not carry a line is refused for every work key on
  // that line, with the reason shown where the owner chooses.
  offers: readonly RunCapability[];
  // The word this harness's own program understands as an order to fold the session it is
  // resuming. Only the harness knows it, so a caller that wants a fold asks for this rather than
  // writing a command of somebody else's making.
  foldPrompt: string;
  command(run: HarnessRun): Promise<HarnessCommand>;
  readAnswer(output: HarnessOutput, run?: HarnessRun): Promise<AgentReply>;
  // How this program's stream is read for what it says about the children a run opened, made once
  // per run because a child's own name and the call that opened it are joined only by an earlier
  // event. Absent where the program opens no children of its own.
  nativeChildReader?(): NativeChildReader;
  // The session one streamed event names, or null where that event names none. A caller watching a
  // run while it works learns which session it is answering in from here: the words are the
  // program's own, so reading them is the implementation's and never the watcher's. Absent where
  // the program streams nothing a session could be read out of.
  sessionFromEvent?(event: unknown): string | null;
  // One line the program wrote to its output, as a person watching its pane reads it — what it said,
  // which tool it called — or null for a line that says nothing to a person. The words are the
  // program's own, so reading them is the implementation's. Absent where nothing it writes reads so.
  progressOf?(line: string): string | null;
  // What a run that did not answer said about it, as one line for a log and a record. Absent
  // where the program says nothing worth more than its exit code and the last line it wrote.
  failure?(output: HarnessOutput): string;
  // Words this program, or the road it is carried on, writes itself into the conversation a run
  // reads — an environment block, the tags a reminder stands in — which no answer of the model's own
  // carries, matched whatever their case. A caller about to send an answer on reads here what would
  // show the model repeated them instead of answering. Absent where it writes nothing there.
  ownWords?: readonly string[];
  // The tools of this program whose calls wait long — a shell command, a sub-agent, a fetch from the
  // web — which a road serving the run's model on the node pool sends with every request, so an engine
  // hands its cards on while one runs (docs/agent-control.md, 「What an agent provider is made of」).
  // The names are the program's own, so only the implementation holds them. Absent where it names none.
  longTools?: readonly string[];
  // The program whose event stream this one writes, as the run log reads streams by it: a program
  // carried on another road writes its own program's events. Absent where it writes no stream of
  // events, only its answer.
  streamsAs?: AgentProviderName;
}

// What one event said about one native child: which child, what it now is, and the words the
// program said it in. The state's vocabulary is the record's, so a reader answers in it directly.
export interface NativeChildEvidence {
  id: string;
  state: NonNullable<import("@agency/archive/execution-store").ExecutionRecord["nativeState"]>;
  evidence: string;
  // Whether this child's name is also the session it runs in, which is true of a program whose
  // children are threads of its own.
  session?: boolean;
}

// One run's reading of its program's stream. It is handed each event and whether a child is
// already known — a question only the caller can answer, because the caller is what records them —
// and it yields the evidence one piece at a time, so a piece read out of a later part of the same
// event sees what an earlier piece of it established. Whatever a program needs to follow a child
// across events, the reader keeps for itself.
export interface NativeChildReader {
  read(event: unknown, known: (id: string) => boolean): Iterable<NativeChildEvidence>;
}

/**
 * The rules a run's line hands over to stand beside its program's own prompt, as their text. A
 * program that keeps no such file of its own reads them here and puts them where it keeps
 * instructions of that kind. A run handed none is refused rather than started without them: they
 * carry the rule against destroying the system.
 */
export function handedRules(run: HarnessRun): string {
  if (run.extensions === undefined) {
    throw new Error(`${run.line}: this run was handed no rules, which its line hands over`);
  }

  return readFileSync(run.extensions.rulesPath, "utf8");
}

// The failure line of a harness that reads none of its own: the exit code and the last line on
// stderr, which is all a program that says nothing structured leaves behind.
/**
 * Whether a run may use every tool it is shown: what decides, on a harness that has no allow list of
 * its own, between writing and only reading. The MCP servers a run is allowed stand beside the tools
 * it shows rather than narrowing them, so they are left out of the comparison — a run allowed its
 * file tools and an MCP server may still write.
 */
export function allowsWhatItShows(run: Pick<HarnessRun, "toolsShown" | "toolsAllowed">): boolean {
  if (run.toolsAllowed === null || run.toolsShown === null) {
    return run.toolsAllowed === run.toolsShown;
  }

  const listed = (tools: string, keep: (tool: string) => boolean): string =>
    tools.split(",").map((tool) => tool.trim()).filter((tool) => tool !== "" && keep(tool)).join(",");

  return listed(run.toolsAllowed, (tool) => !tool.startsWith("mcp__")) === listed(run.toolsShown, () => true);
}

export function harnessFailure(harness: AgentAdapter, output: HarnessOutput): string {
  if (harness.failure !== undefined) {
    return harness.failure(output);
  }

  const tail = output.stderr.split("\n").map((line) => line.trim()).filter((line) => line !== "").at(-1);

  return tail === undefined ? `exit ${output.exitCode}` : `exit ${output.exitCode}: ${tail.slice(0, 300)}`;
}

// The names the owner chooses a road by stand with the archive, which stores them
// (`provider-names.ts`). A new provider is its own file, its name there and its entry in the
// implementation list; nothing else enumerates them.
export { agentProviderNames };
export type { AgentProviderName };

// What a screen or a check needs of one model a provider offers. Each provider's catalogue carries
// more — a name, a context length, a price — and passes it through untouched; what the checks
// themselves read is the id and, where the provider has them, its reasoning levels.
export interface ProviderModel {
  id: string;
  defaultReasoningEffort?: string;
  supportedReasoningEfforts?: string[];
}

// What a provider is asked about this machine when it says whether it can answer at all.
export interface ProviderContext {
  config: { openrouterKey: string | null };
}

// One road the owner chooses for a work key: a service or an account, and the program that
// reaches it. The owner chooses a provider and never an adapter — the adapter follows from the
// provider — so the provider is the unit that is listed where the owner chooses, and it answers for
// itself whether this machine can use it, which models it offers, which logins it holds and which
// of its windows head its usage.
//
// A provider is declared beside the adapter that carries it, one program serving as many
// providers as it reaches: nothing central lists what a provider needs, and adding one is its own
// declaration plus its entry in the implementation list.
export interface AgentProvider {
  name: AgentProviderName;
  // The provider's name as a screen shows it.
  label: string;
  adapter: AgentAdapter;
  // Why this machine cannot use the provider right now, in the owner's language, or null.
  unavailable(context: ProviderContext): Promise<string | null>;
  models(): Promise<ProviderModel[]>;
  // The effort words a mapping of this provider may hold, null among them where a mapping of it may
  // name no effort at all. This is the provider's own vocabulary and nothing else's: a word another
  // provider takes is not a word this one does.
  efforts: readonly (string | null)[];
  // Where the models of this provider differ in what they accept, the efforts that one model takes.
  // Absent where every model of the provider takes the vocabulary whole — the question then has one
  // answer and it is `efforts`.
  modelEfforts?(model: string): Promise<readonly string[]>;
  // Why a run of this provider cannot go out with the effort its resolved mapping named, in the
  // owner's language, or null where it can. Not the same question as the vocabulary: a provider
  // whose mapping may record no effort can still refuse to launch without one, and one whose
  // program cannot transmit an effort refuses to launch with any.
  effortFault(effort: string | null): string | null;
  // What this provider makes of an agent's own thinking and effort (docs/model-tiers.md, 「An agent's
  // own thinking and effort」): the levels a run of one model takes, whether the thinking switch
  // reaches it, and the effort word it goes out with.
  reasoning: ProviderReasoning;
  // Whether a mapping of this provider carries a ceiling on the answer's tokens beside its model
  // and effort. A provider that publishes no such ceiling has none, and a mapping naming one is
  // refused rather than stored and ignored.
  capsAnswerTokens: boolean;
  // The logins agency chooses among for a run of this provider, or null where it holds none. The reply
  // of a provider that holds logins says whose reading it is, and what is written down — a session
  // record, a usage row, a cache state — says it too, because two logins report the very same
  // windows under the very same names. A provider that holds none has one road and nothing to name.
  logins: AgentLogins | null;
  // The program this provider keeps standing in an agent's pane, or null where it has none and every
  // run of it is a host's child (docs/agent-control.md, 「The interactive surface a provider declares」).
  resident: ResidentSurface | null;
  // Where every model of the provider is a service of its own — a registered model — the service one
  // model is asked at by a request with no tools, or null where the provider holds no such model
  // (docs/own-llm.md, 「A request with no tools」). Whatever is asked of a road because it is such a
  // service — its address, its key, its window — is asked of this, never of the provider's name.
  // The model is looked up in the archive given, or the provider settings' archive where none is.
  // Absent on every provider whose runs are a program's.
  serviceOf?(model: string, archive?: Archive): Promise<OwnLlmService | null>;
  // The lines the provider carries, where it carries fewer than its program could: a key on any other
  // line is not offered it and never walks to it. Absent where every line its program carries is its.
  lines?: readonly RunLine[];
  // Where a new session of this run is to stand, for a provider whose model is served in several
  // places and whose session must keep to one of them: the provider chooses, records its choice, and
  // answers the place, or null where the model has one place only. Absent on every other provider.
  place?(run: Pick<HarnessRun, "model">, archive: Archive, now?: number): Promise<string | null>;
  // Why one model of this provider cannot be named for a run of its own — a worker's pin, a job's
  // model — or null where it can: a run named one goes out on it with the model's own defaults,
  // past the tier mapping. Absent on a provider whose runs take their model from the mapping alone.
  namedModelFault?(model: string, archive: Archive): Promise<string | null>;
  // What a tier mapping of this provider asks a run for — the model, whether it thinks, its answer
  // ceiling and its wait — where the provider is an own LLM road the route runner asks as such. Absent
  // on every other provider.
  runSettings?(mapping: TierMapping | null): Promise<OwnLlmSettings>;
}

// What a provider that holds logins answers about them. The walk over them, their order and what
// makes one open are the provider's own; this is only the questions everything else asks.
export interface AgentLogins {
  // Every login the provider walks, in the order it walks them.
  held(archive: Archive): Promise<AgentAccount[]>;
  // The login a reading with no login in hand is filed under.
  recorded(account: string | { id: string } | null | undefined): string;
}

export type { AgentSlot };

