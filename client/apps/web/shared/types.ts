// The shapes the console's server and its screens share. A shape another unit answers with is written here
// as that unit's document states it, since the console reads every unit over HTTP and imports none of their
// code (docs/boundary.md, 「Calls the console makes」). Each screen's group adds the shapes it reads.

import type { PushKind } from "./config";

// bmo-job's shapes (bmo-job's `docs/api.md`).

/** What the deployment's hold says of itself, for every screen and every fact that shows it. */
export interface DeployHold {
  /** When the hold was taken, in seconds. */
  since: number;
  /** When the deployment gives up waiting and lets it go, in seconds. */
  until: number;
  /** The commits that wait to go live, with their items. */
  commits: { commit: string; todoId: number | null }[];
  /**
   * What those commits take away that a process loaded before them may still name; null where it could
   * not be read.
   */
  breaks: string[] | null;
  /** The jobs at work it waits for, one line each with why, as of its last look; null where they could not be read. */
  waitingOn: string[] | null;
  /** The items of the jobs it waits for, as of its last look; null where they could not be read. */
  waitingFor: number[] | null;
  /** The jobs at work it does not wait for, one line each with why, as of its last look. */
  passing: string[];
}

/** What a job waits on: a unit by its name, or a process by its id. */
export type WaitTarget = { kind: "unit"; name: string } | { kind: "pid"; pid: number };

/** A stopped work order's place in its worker's queue. */
export interface WorkQueuePlace {
  // Its turn to be resumed, a turn it is passing to what stands behind it, or set aside.
  turn: "resume" | "passed" | "held";
  // Why it has not been resumed yet, in the judgement's own words.
  why: string;
}

/** A stopped work order's stand. */
export interface WorkOrderStop {
  // The first line of what the order stopped for.
  reason: string | null;
  // Null where the order is not first in its worker's queue, or still holds its checkout.
  place: WorkQueuePlace | null;
  failures: number;
}

/** What a task ran on, and whether that changed from what it was planned on. */
export interface RanOn {
  provider: string;
  model: string;
  changed: boolean;
}

// The agency's shapes (the agency's `docs/api.md`).

/** The providers an agent runs on. */
export type AgentProviderName = "claude-code" | "codex" | "openrouter" | "registered";

/** What a turn's record says it wrote: a note, a memory, a rule, a TODO row or a watch. */
export type TurnRecordKind = "note" | "memory" | "rule" | "todo-add" | "todo-append" | "watch";

export interface TurnRecord {
  kind: TurnRecordKind;
  // What was written, as the call handed it over. Empty where the call carried it some way the reading
  // does not follow.
  text: string;
  // The note, the memory or the TODO row the record landed in, where the call's own answer named it.
  id: number | null;
  // Whether the call set something that wakes the agent: a watch, or a memory written with an event or with
  // a check of its own.
  wakes: boolean;
}

// The shapes of units not yet split, which no screen of this console reads while they are not.

/** The emotion a voice briefing is spoken with, as the voice unit names it. */
export type BriefingEmotion = string;
/** A machine's standing and every node's reading, as the node unit gives them. */
export type MachineStanding = unknown;
export type NodesReading = unknown;


export interface RoomSummary {
  chatId: number;
  title: string;
  messageCount: number;
  lastMessageAt: number | null;
  // Whether the room was made for companions and names who answers in it (the agency's docs/companions.md): it
  // takes words alone, and its TODO panel shows the items filed in it. Absent on every other room.
  madeForCompanions?: true;
  // The companions among those who answer in it, by agent name, where it was made for companions.
  companions?: string[];
}

// A place in a room's line: when the message was sent, and of those sent in one second the order
// the archive took them in.
export interface ReadPlace {
  sentAt: number;
  order: number;
}

// How far the signed-in reader has read a room: how many of its messages they have not read, the
// first of them, which the room opens at, and the place read to that the count runs from — null
// where the archive keeps no marks (docs/web.md, 「Sidebar」). It is the answer to a room marked read too.
export interface RoomRead {
  unread: number;
  firstUnread: number | null;
  readTo: ReadPlace | null;
}

// A room as the room list hands it to the signed-in reader, with how far they have read it.
export interface ListedRoom extends RoomSummary, RoomRead {}

// Whose picture a stored file is. A room and a person can carry the same number, so the kind
// stands beside it wherever one is asked for.
export type AvatarKind = "room" | "person";

// One subject's profile picture, as the console asks for it. The address carries the moment the
// picture was taken, so a browser keeps what it holds until the picture itself changes.
export interface AvatarRef {
  kind: AvatarKind;
  id: number;
  url: string;
}

export interface MessageAttachment {
  id: number;
  kind: "audio" | "video" | "document" | "sticker";
  name: string;
  bytes: number | null;
  type: string | null;
  available: boolean;
  // The shape a video stands in, where Telegram said what it was.
  shape?: VideoShape;
}

export interface VideoShape {
  width: number;
  height: number;
}

export interface TelegramExpression {
  offset: number;
  length: number;
  attachment: MessageAttachment;
}

// What a reply shows of the message it answers: who said it, what it said, and what it carried — a
// kept picture, and the first file it kept — so the quote can stand a small preview of it. A message
// the archive does not hold has no speaker, no words and nothing carried.
export interface ReplyQuote {
  messageId: number;
  senderName: string | null;
  text: string;
  pictured?: boolean;
  attachment?: MessageAttachment | null;
}

export interface ArchivedMessage {
  chatId: number;
  messageId: number;
  senderId: number | null;
  senderName: string;
  sentAt: number;
  text: string;
  imagePath: string | null;
  replyToMessageId: number | null;
  // The answered message as a reply shows it, when this message is a reply.
  replyQuote?: ReplyQuote | null;
  attachments?: MessageAttachment[];
  expressions?: TelegramExpression[];
  // Whether an agent sent this message, the chat agent or a companion, as the server reads it off the archive;
  // a screen draws an agent's answer by it rather than by any name. Absent where the server did not
  // say, which a screen reads as a person's words.
  agent?: boolean;
  // Whether the bot sent this message as the service rather than as the chat agent — a system notice, which a
  // screen stands as a system line rather than an answer. Absent where the server did not say.
  notice?: boolean;
  // How long the room waited for this answer, in whole seconds, on messages an agent answered with.
  // Null for everything anybody else said, and for an answer the archive holds no measurement
  // of.
  answerSeconds?: number | null;
  // The round of the chat agent's session this message answered in, on a message it sent in one.
  round?: string | null;
  // The album a picture went out in, named by the number of the album's first picture, on every
  // picture of a send of several; null on everything else.
  album?: number | null;
  // What that round cost, on the last message the round sent and on no other.
  spend?: RoundSpend | null;
  // Where what the turn did is kept, on that same last message: the room opens it from here and
  // the lines themselves are read out of the log when it does.
  actions?: TurnActionsRef | null;
  // The voice briefing that speaks this line, where one was asked for it.
  briefing?: MessageBriefing | null;
  // On a voice briefing that speaks a line standing in the room, that line: the room stands the
  // voice on it rather than as a row of its own.
  briefs?: number | null;
  // On a summary whose round has not sent its line yet: the room stands it as no row until the
  // round ends, on the line it is tied to or as its own row where the round sent none.
  awaitsLine?: boolean;
}

// A line's full reading (docs/web.md, the room screen's chips): nothing made yet, being made, there to play, or failed
// to be made. Once it is there, where its sound is served from — the voice message it went out as,
// for a line read out before readings were kept, or the reading kept on the line — and that sound.
export interface MessageBriefing {
  state: "idle" | "making" | "ready" | "failed";
  source: "voice" | "kept" | null;
  voiceMessageId: number | null;
  sound: MessageAttachment | null;
}

// A finished turn's own actions, as a message points at them: the agent whose log holds the run,
// the run its lines stand under, and how many documents it was handed and calls it made — enough for
// the room to stand a chip for each without reading a single line of the log — beside what the
// round wrote down, which is carried whole.
export interface TurnActionsRef {
  agent: string;
  run: string;
  documents: number;
  // The reference documents the turn went on holding from earlier in its session, which went as a
  // one-line mark: listed with the rest, but not counted among what it was handed.
  held: number;
  tools: number;
  records: TurnRecord[];
}

// What a run, or a set of runs, reported spending: each kind added over the runs that reported
// it, and how many runs did and did not report. A run that reported nothing adds nothing to the
// kinds; it is counted as unmeasured instead, so its share is never read as nought.
export interface SpendFigure {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  measured: number;
  unmeasured: number;
}

// What one answer cost: every run of the round that produced it, added up, and the runs
// themselves in the order they ran; and how full the answering session's context window stood once
// the last of its runs that reported it ended, as a share of it, or null where none did.
export interface RoundSpend extends SpendFigure {
  roads: TurnRoad[];
  context: number | null;
}

/**
 * One road a gate run walked on its way to a verdict: the provider and model it asked, how that
 * road ended, and how long it held the turn. A stage that asked J3090, waited out its share and
 * was answered by the Claude line carries two of these, in the order they were walked.
 */
export interface JudgementRoad {
  id: number;
  provider: string;
  model: string;
  outcome: string;
  ms: number;
  // What the road's run spent, where the record carries it; a road written before the figures
  // were kept, or one whose run read none, has none.
  tokens?: TokenSplit | null;
}

// One run a chat turn made, the turn's own included: which stage it was, the road it took and what
// it spent.
export interface TurnRoad extends JudgementRoad {
  stage: string;
}

export interface Judgement {
  id: number;
  chatId: number;
  messageId: number | null;
  stage: string;
  verdict: string;
  detail: string | null;
  createdAt: number;
  // How long the stage took, where the record carries it. A judgement announced by the live
  // stream carries neither this nor its roads: both are read from the archive.
  ms?: number | null;
  roads?: JudgementRoad[];
  // The round of the chat agent's session the decision was made in, where the record names one.
  round?: string | null;
  // What opened that round and what its gates were offered, where bmo-server read any.
  opening?: RoundOpening;
}

// One quiet instruction a round settled, without its text (docs/web.md, the room screen's judgements). Its sort is the
// word its producer named it by, and none where it named none.
export interface TurnInstruction {
  id: number;
  sort: string | null;
  summary: string;
  createdAt: number;
}

// One gate's row of the offers record: how many candidates its libraries held, how many it was
// asked about, what each rule kept, and the rules a candidate could be kept by where fewer than all
// of them could.
export interface GateOfferRecord {
  stage: string;
  held: number;
  asked: number;
  kept: { item: number; keyword: number; person: number };
  capped: number;
  rules: string[] | null;
}

// What one round settled and was offered, as bmo-server reads it.
export interface RoundOpening {
  instructions: TurnInstruction[];
  offers: GateOfferRecord[];
}

// The four kinds of token a run reports, as a window's rows add them up. Null is not nought: it
// says the rows this was read from were written before the kinds were kept apart, and nothing at
// all is known about how the traffic divided.
export interface TokenSplit {
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
  cacheWriteTokens: number | null;
}

export interface ControlStatus {
  reachable: boolean;
  slow?: boolean;
  // The owner's own private chat, as the bot names it. A screen knows whose room it is
  // standing in by this, and nothing but this; a bot that does not answer leaves it null.
  ownerChatId: number | null;
}

// One of the things an item's runs spent their time on, with the figures added up across every
// run of that item: a phase of the run, a tool it called, a co-worker it opened. The kind is
// what keeps the two levels apart — the phases divide the whole of the item's time between them
// and everything else happened inside one of them — so nothing may add a phase to a tool.
//
// The name is the CLI's own and is read out through the span vocabulary rather than shown as it
// stands. What a job spent is no length of time and is not one of these; it is read where an
// item's cost is read.
export interface TodoSpan {
  kind: "phase" | "tool" | "agent";
  name: string;
  seconds: number;
  // How many calls the seconds are added over — a tool called two hundred times is one of
  // these — and the tokens whoever stated them stated, which for a Claude Workflow branch is its own
  // figure and for everything else is nothing measured.
  times: number;
  tokens: number | null;
}

// One piece of work the owner is owed, as the archive's todos table holds it. The instruction
// is the owner's words and the chat agent's reply verbatim, which is why an item is worth keeping at all.
export interface TodoItem {
  id: number;
  title: string;
  instruction: string;
  // The organized instruction the chat agent keeps for the item's jobs, in English, and its Korean rendering made
  // beside it. Null until a job of the item is launched or the chat agent rewrites it, and the rendering null until
  // the bot has made it.
  brief: string | null;
  briefKorean: string | null;
  requester: string;
  requesterId: number | null;
  chatId: number | null;
  approved: boolean;
  done: boolean;
  // Whether a job of this item is under way, read from the run itself: the item has a job row
  // that has not finished. The state line is a word somebody typed, so it is not asked.
  running: boolean;
  priority: number;
  status: string;
  model: string | null;
  // The provider and model the item's work last went out on, which `model` — `auto` above all — does
  // not say: a co-work item's own line as its order keeps it, any other item's latest run. Null until a
  // run of it has gone out (bmo-job's docs/work-orders.md 「The model a step ran on」).
  ranOn: RanOn | null;
  // Which worker is to do this work, and so which checkout it will be done in. It is an
  // assignment and not a record: the queue reads it to decide whose turn the item is, and the
  // job is launched on the worker it names. Null is unassigned, which is the built-in worker.
  worker: string | null;
  // Whether the job that runs this item fans out into branches. Off is what every item runs as
  // unless it is turned on, so a row that says nothing is one session.
  cowork: boolean;
  // How many times this item has been run. One run needs no saying; more than one is what makes
  // a row read as an attempt of this item rather than as a thing of its own.
  attempts: number;
  // How far the item's work order has come while a run of it is going — stage 2/3, building 1/4, design —
  // in the words the bot's board uses. Null on a row nothing is running for and on a one-session
  // job, which has nothing to count.
  progress: string | null;
  // Where the item's stopped work order stands — what it stopped for, its place at the head of its
  // worker's queue and why it is not resumed yet, the failures counted — as the bot left it on the
  // order. Null where no order of the item is stopped.
  stopped: WorkOrderStop | null;
  // What the item's work order is standing on while it holds its worker's checkout, in the words
  // the bot's runner wrote there. Null where no order of the item holds one or none is waiting.
  holding: string | null;
  // The long wait the job running this item marked for itself (the agency's docs/agent-control.md 「A job's own
  // mark on a long wait」): what it waits on, the one thing whose life says it goes on, and when it
  // expects to be done. Null where no job of the item carries a mark, or its target has gone.
  waiting: { what: string; target: WaitTarget; until: number } | null;
  // Whether a backlog deployment holding the queue is waiting for this item's job to end.
  deployWaits: boolean;
  // Held while a run of it still goes: the item reads as waiting to stop and stays in the queue until the
  // run has ended (bmo-job's docs/work-orders.md).
  holdPending: boolean;
  // Whether the item's work order is under way, a run of it going or not: what it has in flight —
  // a step waiting for a place included — is what a stop ends.
  orderActive: boolean;
  // How long the work took, in seconds: every job that carried the item added up, because an
  // attempt that failed is time the item cost. Null until a job of it has finished, and a job
  // still running counts as nothing rather than as a figure that grows while it is looked at.
  seconds: number | null;
  // What that time went to, longest first: the phases the item's runs were divided into and
  // everything that happened inside them, whole. Which of them a screen has room for is the
  // screen's own to decide, so nothing is left out on the way here.
  spans: TodoSpan[];
  // What the owner said when they approved it — a remark or a question of their own — and null
  // where they said nothing.
  note: string | null;
  // How the work was done, written by the job that did it as it closed the item, and what the
  // work left behind. Both null on an item nobody has finished, and on one finished before a
  // closing job was asked for its account.
  report: string | null;
  unfinished: string | null;
  // Whether that open matter still stands, and the one line saying what makes it no longer outstanding.
  // The note itself is never taken off the row: what is added is when it was resolved and why.
  unfinishedResolvedAt: number | null;
  unfinishedResolution: string | null;
  // When it was set aside, and the line saying why where one was given. Null on everything the
  // queue can still see.
  heldAt: number | null;
  heldReason: string | null;
  // Which kind of setting aside: the owner stopping the work mid-run (a stop), the owner winding it down to be
  // resumed (waiting to resume), the failure count (held for failures), or null for a plain hold.
  heldKind: "stop" | "pause" | "failure" | null;
  // The item this one was merged into — its number, its title and when — and null on a row never
  // merged. A merged row waits for that item and is finished with it; the link stays once it is.
  mergedInto: number | null;
  mergedAt: number | null;
  mergedIntoTitle: string | null;
  // The items merged into this one, in number order, with whether each is finished.
  mergedFrom: MergedTodo[];
  createdAt: number;
  doneAt: number | null;
}

// One item merged into another, as the item it was merged into lists it.
export interface MergedTodo {
  id: number;
  title: string;
  done: boolean;
}

// The three lists the panel stands on: what is approved and unfinished, in the owner's order;
// what a room has asked for and the owner has not approved; and what was lately finished, which stays
// in the table as the record of it.
export interface TodoBoard {
  queue: TodoItem[];
  pending: TodoItem[];
  // What the owner has set aside for now. It is out of the queue and out of the requests waiting on
  // them, and it is kept apart rather than mixed in, because the whole of setting one aside is
  // not seeing it.
  held: TodoItem[];
  // What is merged into another unfinished item and waits for it. Kept apart like what is set
  // aside, since its work is carried by the row it names.
  merged: TodoItem[];
  done: TodoItem[];
  // How many finished items there are behind the page in hand — every one of them where nothing
  // is being searched for, and the ones that match where something is. It is what tells the
  // panel there is more to fetch, and what lets it say how far through it has got.
  doneTotal: number;
  // Every item whose open matter is still standing, whether or not it is on the page in hand. It is the
  // list a new row's sources are picked from: a strand of work is often left in two notes at
  // once, and the row that takes it up comes out of both of them. A note already resolved is not
  // among them — it has been settled, and settling it again is a different act done on its own
  // row. The finished ones are paged, so this is asked for separately rather than found among
  // them.
  standingNotes: StandingNote[];
  // What the table's own columns hold, for the panel's conditions to be picked from: every worker
  // that does a row (an unassigned row being the built-in worker's), every room a request came from (null for one filed outside every room) and
  // every name a request was filed under. Read from the whole table and never narrowed, so a
  // condition that empties the list can still be changed to another.
  choices: TodoChoices;
  // How many rows the unfinished lists hold, whatever a search or a condition left of them.
  standing: TodoStanding;
  // Whether the owner has paused the auto-progression. It rides on the board because the panel
  // already asks for the board and the pause is a fact about this queue; it is read from the
  // archive like the rows are, so the screen says what is true whether the bot is up or not.
  paused: boolean;
  // A deployment holding the queue until no job is at work, apart from the owner's pause and let go by
  // another button: what it carries, what it takes away, whom it waits for and until when.
  deployHold: DeployHold | null;
}

export interface TodoStanding {
  queue: number;
  // The rows of the queue a run is going on for, a held one still ending included.
  running: number;
  pending: number;
  held: number;
  merged: number;
}

export interface TodoChoices {
  workers: string[];
  chats: (number | null)[];
  requesters: string[];
}

// One item that left an open matter nobody has settled: enough to pick it by and nothing else, since the
// note itself is read on the row it belongs to.
export interface StandingNote {
  id: number;
  title: string;
}

// Who settled a question a job put to the owner: their own answer, wherever they gave it, or the
// wait running out.
export type QuestionSettler = "owner" | "deadline";

// One question a job stopped to ask the owner, as the console reads it. The job is standing
// still while it is open, which is why the console shows it rather than leaving it to the chat.
export interface JobQuestion {
  id: number;
  // The item the asking job is a run of. A question is read back with its item afterwards.
  todoId: number;
  question: string;
  options: string[];
  // Which option the job would take by itself, counted from zero. The deadline settles on it.
  recommended: number;
  // The one line saying why the job would not decide this itself.
  why: string;
  askedAt: number;
  // When the question settles on the recommendation if nobody answers: the end of the countdown
  // while it runs, the end of the stop while they have stopped it.
  dueAt: number;
  chosen: number | null;
  // What the owner wrote instead of picking, where the options covered none of what they wanted.
  answer: string | null;
  // The seconds the countdown still had when they stopped it, or null while it runs.
  pausedLeft: number | null;
  settledAt: number | null;
  settledBy: QuestionSettler | null;
}

// One run that is going on right now, as the bot reports it. A gate runs inside the bot, so
// the bot's own memory is the whole truth of it; a runner, an editor and a job worker run in
// units of their own, and the bot asks the unit whether the run is still there.
/**
 * What a stop is aimed at, as every row of the picture names it (the agency's `docs/agent-control.md`
 * 「Stopping」). The kind decides the reach: a chat turn and a gate stand in the bot's own host and
 * are named by their execution, a runner, an editor and a job worker by their unit, and a work
 * order by its item.
 */
export type StopTargetKind = "execution" | "unit" | "item";

export interface StopTarget {
  kind: StopTargetKind;
  id: string;
}

export interface LiveRun {
  id: number;
  kind: LiveRunKind;
  agent: string;
  chatId: number | null;
  messageId: number | null;
  startedAt: number;
  // The unit this run stands in, and what a stop from the board names. Null for a gate.
  unit: string | null;
  // The ceiling this run is held to, null where it has none; a search has none.
  capSeconds: number | null;
  // A search run alone: basic, thorough or exhaustive.
  depth: string | null;
  // Actual worker provider/model once started; requested model until then.
  mode?: AgentMode | null;
  model: string | null;
  // Who set this run off: the console, or the chat whose session launched it. Null for a gate,
  // and for a run whose launcher was the chat it belongs to.
  startedBy: string | null;
  // A worker run alone: which TODO item it is a run of. The item is the only number shown for a
  // run; one with no item behind it carries none.
  todoId: number | null;
  // A worker run with no item alone: why it was launched, as its launcher wrote it, which a screen
  // shows where an item's number would stand. Absent or null for every other run.
  purpose?: string | null;
  // How far the work order this run is a step of has come — stage 2/3, building 1/4, design — in the
  // bot's own words. Null for a run that is no step of an order under way.
  progress: string | null;
  // Which step of a series its opener planned this run is — a batch of a room's person-book run —
  // and how many the series has. Absent or null for a run that is no step of one.
  step?: { step: number; steps: number } | null;
  // A worker run alone: standing still on a pane confirmation put to the owner, or on a question it
  // asked them. Absent or null for a run that waits on nobody.
  waiting?: "confirmation" | "question" | null;
  // What a stop on this row is aimed at. The bot names it per row, so no screen works one out from
  // the row's shape; null where there is nothing to aim at, and absent from a picture a bot drew
  // before it carried one.
  stop?: StopTarget | null;
}

export type LiveRunKind = "gate" | "runner" | "editor" | "worker";

// A chat whose context is being folded up. The fold opens no row and writes no ending, so the
// bot's own picture is the only place it can be read while it is happening.
export interface ChatCompaction {
  chatId: number;
  since: number;
}

// What a turn was handed to read: the room's own messages, and the quiet instructions put in
// front of them. A figure the bot did not supply is not made up from anything else on screen.
export interface TurnReceived {
  messages: number;
  instructions: number;
}

// A chat answering a round right now. Like a fold it opens no row and writes no ending, so the
// bot's own picture is the only place it can be read while it is going on — which is what lets a
// screen that arrives in the middle of a turn, or comes back from a dropped stream, show it.
// Who answers a turn, where it is not the chat agent: a companion, by the number its words stand under in the
// room and its name. The chat agent's own turns name nobody.
export interface TurnSpeaker {
  id: number | null;
  name: string;
}

export interface ChatTurn extends TurnReceived {
  chatId: number;
  since: number;
  // The newest message the round carries, which it reads last and which its gates and answer judge;
  // null for a round carrying none, absent from a bot that does not say.
  messageId?: number | null;
  // A companion's turn names its speaker, so a room two agents answer holds a turn of each at once.
  speaker?: TurnSpeaker;
  // The Job the turn waits on for its engine, while it waits; null or absent once it runs.
  waiting?: EngineHold | null;
  // The chat agent answering stands in no unit, so its stop is named by the execution the turn's run opened.
  stop?: StopTarget | null;
}

// The Job a turn waits on: its number, and the item it is a run of, where it is one.
export interface EngineHold {
  jobId: number;
  todoId: number | null;
}

// A message the bot has put into its room's queue for the chat agent which no turn of its has taken yet, the
// newest of each room alone; it is waiting to be judged.
export interface QueuedMessage {
  chatId: number;
  messageId: number;
  since: number;
}

export interface LiveSnapshot {
  at: number;
  runs: LiveRun[];
  compacting: ChatCompaction[];
  turns: ChatTurn[];
  // Absent from a bot that does not say.
  queued?: QueuedMessage[];
}

export type BotEventType =
  | "avatars-updated"
  | "queued"
  | "run-started"
  | "judgement"
  | "sent"
  | "skipped"
  | "run-failed"
  // The round ended because somebody stopped the run: nothing failed and nothing is reported as a
  // breakdown.
  | "run-stopped"
  // The round ended by waiting for more of what someone is still typing: nothing was said and
  // nothing failed.
  | "waited"
  | "live"
  // What the session's own turn is doing right now, as its stream says it.
  | "run-progress"
  // The TODO changed. What it holds now is read from the archive, so the event says only that.
  | "todo"
  | "work-order"
  // The talk about one item moved: the owner said something, or the chat agent's turn on it ended.
  | "todo-talk"
  // The talk about one document in the document viewer moved, the same way.
  | "document-talk"
  // The talk under one code review moved, the same way.
  | "code-review-talk"
  // A code review was made, or a TODO row was filed out of one.
  | "code-review"
  // A part of a document's translation was written, or a translation stopped without one.
  | "document-translation"
  // A room's session was closed and a new one opened with a summary of it, because re-writing
  // the old context cost more than starting over.
  | "session-replaced"
  // A room's answer came off a lesser model than the one it was launched on, because the
  // account's safeguard refused that model on the session's context.
  | "downgraded"
  // A message arrived in a room and was archived. What the room now holds is read from the
  // archive, so the event says only which room and which message.
  | "message"
  // A room's window was opened, moved on or shut. How long it has left is read from the archive.
  | "window"
  // A node's state moved or the node list changed. What it holds is read from the archive.
  | "nodes"
  // The Laya server's health moved. What it holds is read from the archive.
  | "laya"
  // A voice briefing was put in a room. The message is the voice message, by which the console
  // serves its sound (the voice unit's docs/voice-briefing.md).
  | "briefing"
  // A room's registration was removed with its whole record. It names the room, which no screen
  // shows again.
  | "room-removed";

// The ways a turn ends: the bot answered, it held its peace, its run failed, somebody stopped it,
// or it did nothing but wait for the rest of what someone is typing. Whatever a turn moves — a
// session's context, a usage window, the archive of a room — stands still again by the time one
// arrives.
export const TURN_ENDED: readonly BotEventType[] = ["sent", "skipped", "run-failed", "run-stopped", "waited"];

// What a listener needs of a briefing beyond the message it went out as.
export interface BriefingNews {
  text: string;
  // How long the sound runs, where it could be read off the file.
  seconds: number | null;
  // When it went out, in epoch milliseconds.
  sentAt: number;
  // The emotion it was spoken with, where one was named. The island draws no face and ignores it.
  emotion?: BriefingEmotion;
  // The face it shows, as the face's own entry answered for its emotion. The island draws no face and ignores it.
  expression?: string;
  // What a summary is about, in one line, which a listener shows in place of the words.
  title?: string;
  // Whether a listener plays it by itself; one that does not say is played.
  autoplay?: boolean;
}

interface BotEventFields {
  id: number;
  at: number;
  type: BotEventType;
  chatId: number;
  messageId: number | null;
  stage?: string;
  verdict?: string;
  detail?: string | null;
  // A live event belongs to no room and carries the whole picture rather than a change to it.
  live?: LiveSnapshot;
  // A run-started event alone: what the turn was handed, as it stood when the round opened.
  received?: TurnReceived;
  // A run-started event and the ending of the same turn: who is answering, where it is not the chat agent, so
  // the ending closes that speaker's turn and not another one going in the same room.
  speaker?: TurnSpeaker;
  // A run-progress event alone: where the turn is and what its stream has counted so far.
  progress?: RunProgress;
  // A sent event alone: the round the answer went out in, which every message of that answer names.
  round?: string;
  // A briefing event alone.
  briefing?: BriefingNews;
}

export interface WorkOrderProgress {
  status: "draft" | "active" | "paused" | "done";
  phase: "planning" | "research" | "synthesis" | "implementation" | "finalization";
  stage: number;
  updatedAt: number;
  tasks: Array<{
    id: string;
    state: "pending" | "running" | "done" | "failed" | "skipped";
    attempt: number;
    jobId: number | null;
    model: string;
    error: string | null;
  }>;
}

export type BotEvent = BotEventFields & ({
  type: "work-order";
  todoId: number;
  workOrder: WorkOrderProgress;
  document?: never;
} | {
  type: "todo-talk";
  todoId: number;
  workOrder?: never;
  document?: never;
} | {
  type: "document-talk" | "document-translation";
  // The document's name, its root and its path inside that root.
  document: string;
  todoId?: never;
  workOrder?: never;
} | {
  type: "code-review-talk" | "code-review";
  // The review's number.
  review: number;
  todoId?: never;
  workOrder?: never;
  document?: never;
} | {
  type: Exclude<BotEventType, "work-order" | "todo-talk" | "document-talk" | "document-translation" | "code-review-talk" | "code-review">;
  todoId?: never;
  workOrder?: never;
  document?: never;
  review?: never;
});

// What a running turn says of itself. Only what the stream actually carries: the output count
// is absent because the stream's own is a placeholder until the run ends.
// One thing a running turn has done, as the bot publishes it: what it was handed before it began,
// and each tool it has called since. The kind and the name are the log line's own, so the room's
// block reads them with the same look the agent's own page reads a line with.
export interface TurnAction {
  kind: "context" | "tool";
  name: string | null;
  // How a call shows its tool, where the log line it was read off says it; a call read off the bot's live
  // picture says nothing, and is shown by its name and its tool's declared icon.
  tool?: AgentLogTool | null;
  text: string;
  // How a call ended, once its result came back, and null while it is still out. On what was
  // handed over, whether a reference document went in full or as an already-loaded mark.
  state: "succeeded" | "failed" | AgentLogContextState | null;
}

export interface RunProgress {
  phase: "thinking" | "writing" | "tool" | "waiting";
  tool: string | null;
  thinkingTokens: number | null;
  contextTokens: number | null;
  cacheReadTokens: number | null;
  // The message the last request answered with.
  requestId: string | null;
  // What the turn's requests have reported so far, each added once; null before the first.
  spent: { inputTokens: number; cacheReadTokens: number; cacheWriteTokens: number } | null;
  // An estimate, from their length, of the tokens the turn's answer and tool calls have taken.
  writtenTokens: number | null;
  // What the turn has done so far, oldest first and bounded, with how many fell off the front.
  actions: TurnAction[];
  droppedActions: number;
  droppedContext: number;
  // The Job the turn waits on, in the waiting phase alone.
  waitingFor?: EngineHold | null;
}

// One gate run as the archive kept it: the verdict is what it ended with, and no detail is
// read here — the live status screen shows nothing anyone said.
export interface GateRunRecord {
  id: number;
  agent: string;
  chatId: number;
  messageId: number | null;
  verdict: string;
  ms: number | null;
  at: number;
}

export interface RunnerRecord {
  id: number;
  agent: string;
  // The systemd unit the run ran in, which is the name its log lines are kept under.
  unit: string;
  chatId: number;
  depth: string | null;
  capSeconds: number | null;
  startedAt: number;
  finishedAt: number | null;
  outcome: string | null;
}

export interface JobRecord {
  id: number;
  // The systemd unit the job ran in, which is the name its log lines are kept under.
  unit: string;
  chatId: number;
  model: string | null;
  launchedAt: number;
  finishedAt: number | null;
  outcome: string | null;
  ok: boolean | null;
  // The TODO item this run belongs to — the only number the screen shows for it — and which
  // attempt of that item it was. Null and 0 for a job nobody filed an item for.
  todoId: number | null;
  attempt: number;
  // Why a job with no item was launched, as its launcher wrote it; null for a run of an item and
  // for a row written before launches had to say.
  purpose: string | null;
  // The worker that ran it, which is the name its log is kept under and the screen it belongs
  // to. A row from before there was more than one worker carries the built-in worker's name.
  worker: string;
}

// What the live status screen is answered with: the bot's picture of now, and the archive's
// record of what came before. A bot that does not answer leaves the first half empty.
export interface ExecutionHistory {
  id: string;
  root?: string;
  workKey?: string;
  model?: string | null;
  attempt?: number;
  nativeState?: string;
  sharedPane?: boolean;
  parent: string | null;
  provider: string;
  line: string;
  state: string;
  exitCode: number | null;
}
// A standing watch the chat agent set on itself: what it waits for and how its checks have gone. A check
// watch is held by a runner whose unit the row names; a timer is held by no run.
export interface WatchView {
  id: number;
  kind: "condition" | "timer";
  chatId: number;
  // The short name a line shows the watch by, and the note the whole of what it promised. A watch
  // carrying no name is shown by the head of its note instead, which is what a null means.
  name: string | null;
  note: string;
  condition: string | null;
  dueAt: number | null;
  deadlineAt: number;
  intervalSeconds: number | null;
  lastCheckedAt: number | null;
  lastExit: number | null;
  unit: string | null;
  // The memory it serves, or null for a watch nothing closing a memory will end. An archive copied
  // before the column reads it as null.
  memoryId: number | null;
  // The run holding this watch, as a stop names it. A timer and a check between two runs are held
  // by nobody and name nothing.
  stop?: StopTarget | null;
}

export interface StatusView {
  executions?: ExecutionHistory[];
  watches?: WatchView[];
  reachable: boolean;
  slow?: boolean;
  live: LiveRun[];
  compacting: ChatCompaction[];
  liveAt: number | null;
  gates: GateRunRecord[];
  runners: RunnerRecord[];
  jobs: JobRecord[];
  // The machine's room as the bot last read it; null before it has.
  machine?: MachineStanding | null;
  // The machines work goes round, with what each was given (the agency's docs/work-nodes.md); null before the bot
  // has read them.
  nodes?: NodesReading | null;
}

export interface AccountUser {
  name: string;
}

// The sorts of thing a `context` line carries, as its name: which of the things put in front of a
// turn before it ran this one was. The writer settles it — a picker's library and a standing entry
// each answer for their own — and a sort this list does not know still reads as what it is.
// What a line says became of what it records, in the writer's own words.
// How a reference document went in, on its `context` line: its full text with that turn, or a
// one-line mark for a version the session already held.
export type AgentLogContextState = "attached" | "held";

// One line of what an agent did, as whoever read that run's stream wrote it down. The bot
// writes it and this app only reads it.
// The tool a line is about, as its chip shows it.
export interface AgentLogTool {
  name: string;
  icon: string | null;
}

export interface PushDevice {
  endpoint: string;
  kinds: PushKind[];
  createdAt: number;
  lastSentAt: number | null;
}

// What the console can say about push before a browser has been asked anything: whether this
// machine holds keys at all, the public half a browser needs to subscribe, and the devices
// already subscribed under this account.
export interface PushState {
  configured: boolean;
  publicKey: string | null;
  devices: PushDevice[];
}

// What one notification carries. It is a nudge to open the console rather than the thing
// itself: a line to read at a glance and where to go for the rest of it.
export interface PushPayload {
  // The kind of news, or `test` for the account screen's test ring, which no device can refuse.
  kind: PushKind | "test";
  title: string;
  body: string;
  // Where pressing it goes, as a path inside the console.
  url: string;
  // What this notification is about, so a second one about the same thing replaces it on the
  // screen instead of standing beside it.
  tag: string;
}

// Every provider the bot carries, as the bot names them.
export type AgentMode = AgentProviderName;
