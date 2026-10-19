import { DONE_MAX, DONE_PAGE_SIZE, TODO_SEARCH_LIMIT, WORKER_AGENT } from "#shared/config";
import { filterRecords } from "bmo-ui/shared/search";
import { wording } from "#shared/wording";
import { NO_CONDITIONS, NO_ROOM, todoSearchFields } from "../../app/domain/todos";
import { jobRead } from "./job";

import type { DeployHold, RanOn, TodoBoard, TodoChoices, TodoItem, TodoNote, TodoSpan, TodoStanding } from "#shared/types";
import type { TodoConditions } from "../../app/domain/todos";

// The TODO as bmo-job reads it (bmo-job's `docs/api.md`, 「Readings」; its `docs/reading.md`): a listing of every
// item's line, and each item opened on its own, read into the rows the panel shows. A reading carries no
// sentence; its kinds become the row's own fields, and the panel lays them out in the pack's words. What no
// reading carries stands empty on the row (docs/boundary.md, 「Gaps the console meets」): when an item was filed
// and finished, the room it came from, its brief, the progress and stand of its work order and the wait its job
// marked.

// What a caller asked to find, or null for the ordinary list. Whitespace alone is not a search: it would match
// every row and read as the list having been filtered to itself.
export function searchedFor(value: unknown): string | null {
  const query = typeof value === "string" ? value.trim().slice(0, TODO_SEARCH_LIMIT) : "";

  return query === "" ? null : query;
}

// How many finished items to read. The panel asks for the whole run it has scrolled through rather than for the
// next page alone.
export function doneLimit(value: unknown): number {
  const asked = Number.parseInt(typeof value === "string" ? value : "", 10);

  return Number.isInteger(asked) && asked > 0 ? Math.min(asked, DONE_MAX) : DONE_PAGE_SIZE;
}

// ## The readings as bmo-job answers them

type LineMark =
  | { kind: "merged_into"; id: number }
  | { kind: "merging_into"; id: number }
  | { kind: "done" | "cowork" | "owner_note" | "report" | "unfinished" | "unfinished_resolved" }
  | { kind: "pending_approval"; requester: string };

type HeldWord = "stop" | "stopping" | "pause" | "pausing" | "failure" | "plain" | "pending";

type ModelMark = { kind: "fallback" | "changed" } | { kind: "requested"; model: string };

interface ItemLine {
  id: number;
  title: string;
  state: LineMark | null;
  status: string;
  marks: LineMark[];
  model: { shown: string; marks: ModelMark[] } | null;
  worker: string | null;
  hold: { word: HeldWord; reason: string | null } | null;
}

type RowState = "done" | "open" | "merged" | "held" | "opened";

interface LinkedRow {
  id: number;
  title: string;
  state: RowState;
}

interface NoteReading {
  id: number;
  author: string;
  text: string;
  added_at: number;
}

interface SpanReading {
  kind: string;
  name: string;
  seconds: number;
  times: number;
  tokens: number | null;
}

interface ItemReading {
  line: ItemLine;
  held_at: number | null;
  merged_into: { id: number; title: string | null; at: number | null; follows: boolean } | null;
  merged_here: LinkedRow[];
  requester: string;
  requester_id: number | null;
  approved: boolean;
  priority: number;
  worker: string;
  worker_assigned: boolean;
  cowork: boolean;
  instruction: string;
  owner_note: string | null;
  notes: NoteReading[];
  time: { attempts: { job: number; took_ms: number | null; outcome: string | null }[]; total_ms: number; finished: number; phases: SpanReading[]; inside: SpanReading[] } | null;
  report: string | null;
  unfinished: { text: string; resolved_at: number | null; resolution: string | null } | null;
}

// The pool board's work, which names the item a turn or a co-work branch is at work for.
interface PoolWork {
  item?: number | null;
}

interface PoolBoard {
  buckets: { tiles: { work?: PoolWork | null }[] }[];
  workers: { tile: { work?: PoolWork | null } }[];
}

const seconds = (ms: number | null | undefined): number | null => (ms === null || ms === undefined ? null : Math.floor(ms / 1000));

// The held words of a row whose run is still ending.
const ENDING: readonly HeldWord[] = ["stopping", "pausing", "pending"];

const HELD_KINDS: Record<HeldWord, TodoItem["heldKind"]> = {
  stop: "stop",
  stopping: "stop",
  pause: "pause",
  pausing: "pause",
  failure: "failure",
  plain: null,
  pending: null
};

// The road an item last went out on and the model it asked for, read back from the reading's model: a road is
// `provider/model`, and where it marks no request of its own the request was that road itself, or `auto` where the
// road was the order's fallback.
function modelOf(line: ItemLine): { model: string | null; ranOn: RanOn | null } {
  const reading = line.model;

  if (reading === null) return { model: null, ranOn: null };

  const at = reading.shown.indexOf("/");
  const requested = reading.marks.find((mark): mark is { kind: "requested"; model: string } => mark.kind === "requested");

  if (at === -1) return { model: reading.shown, ranOn: null };

  const ranOn: RanOn = { provider: reading.shown.slice(0, at), model: reading.shown.slice(at + 1), changed: reading.marks.some((mark) => mark.kind === "changed") };
  const fallback = reading.marks.some((mark) => mark.kind === "fallback");

  return { model: requested?.model ?? (fallback ? "auto" : `${ranOn.provider}::${ranOn.model}`), ranOn };
}

function spanOf(span: SpanReading, kind: TodoSpan["kind"]): TodoSpan {
  return { kind, name: span.name, seconds: span.seconds, times: span.times, tokens: span.tokens };
}

/** One item opened, as the row the panel shows. */
export function itemOf(reading: ItemReading, running: ReadonlySet<number>): TodoItem {
  const line = reading.line;
  const has = (kind: LineMark["kind"]) => line.marks.some((mark) => mark.kind === kind) || line.state?.kind === kind;
  const held = line.hold;
  const { model, ranOn } = modelOf(line);
  const time = reading.time;

  return {
    id: line.id,
    title: line.title,
    instruction: reading.instruction,
    brief: null,
    briefKorean: null,
    requester: reading.requester,
    requesterId: reading.requester_id,
    chatId: null,
    approved: reading.approved,
    done: has("done") || line.marks.some((mark) => mark.kind === "merged_into"),
    running: running.has(line.id) || (held !== null && ENDING.includes(held.word)),
    priority: reading.priority,
    status: line.status,
    model,
    ranOn,
    worker: reading.worker_assigned ? reading.worker : null,
    cowork: reading.cowork,
    attempts: time?.attempts.length ?? 0,
    progress: null,
    stopped: null,
    holding: null,
    waiting: null,
    deployWaits: false,
    holdPending: held !== null && ENDING.includes(held.word),
    orderActive: false,
    seconds: time === null || time.finished === 0 ? null : Math.floor(time.total_ms / 1000),
    spans: time === null ? [] : [...time.phases.map((span) => spanOf(span, "phase")), ...time.inside.map((span) => spanOf(span, span.kind === "agent" ? "agent" : "tool"))],
    note: reading.owner_note,
    report: reading.report,
    unfinished: reading.unfinished?.text ?? null,
    unfinishedResolvedAt: seconds(reading.unfinished?.resolved_at),
    unfinishedResolution: reading.unfinished?.resolution ?? null,
    heldAt: seconds(reading.held_at),
    heldReason: held?.reason ?? null,
    heldKind: held === null ? null : HELD_KINDS[held.word],
    mergedInto: reading.merged_into?.id ?? null,
    mergedAt: seconds(reading.merged_into?.at),
    mergedIntoTitle: reading.merged_into?.title ?? null,
    mergedFrom: reading.merged_here.map((row) => ({ id: row.id, title: row.title, done: row.state === "done" })),
    createdAt: 0,
    doneAt: null
  };
}

// The items a turn or a co-work branch is at work for now, as the pool board names them.
async function runningItems(): Promise<Set<number>> {
  try {
    const board = await jobRead<PoolBoard>("/coworker-pool");
    const works = [...board.workers.map((worker) => worker.tile.work), ...board.buckets.flatMap((bucket) => bucket.tiles.map((tile) => tile.work))];

    return new Set(works.flatMap((work) => (typeof work?.item === "number" ? [work.item] : [])));
  } catch {
    return new Set();
  }
}

async function readingOf(id: number): Promise<ItemReading | null> {
  try {
    return await jobRead<ItemReading>(`/todos/${id}`);
  } catch (error) {
    if ((error as { statusCode?: number }).statusCode === 404) return null;
    throw error;
  }
}

// Several items opened, in number order, against the items at work as the pool board named them.
async function itemsOf(ids: readonly number[], running: ReadonlySet<number>): Promise<TodoItem[]> {
  const wanted = [...new Set(ids)].sort((one, other) => one - other);
  const readings = await Promise.all(wanted.map(readingOf));

  return readings.flatMap((reading) => (reading === null ? [] : [itemOf(reading, running)]));
}

/** Several items by their numbers, each opened, in number order; a number with no item behind it is left out. */
export async function readTodoItems(ids: readonly number[]): Promise<TodoItem[]> {
  return ids.length === 0 ? [] : await itemsOf(ids, await runningItems());
}

/** One item by its number, or null where there is no such item. */
export async function readTodoItem(id: number): Promise<TodoItem | null> {
  return (await readTodoItems([id]))[0] ?? null;
}

/** The words added to one item, oldest first, as its reading carries them. */
export async function readTodoNotes(todoId: number): Promise<TodoNote[]> {
  const reading = await readingOf(todoId);

  return (reading?.notes ?? []).map((note) => ({
    id: note.id,
    todoId,
    author: note.author,
    authorId: null,
    text: note.text,
    addedAt: Math.floor(note.added_at / 1000),
    jobId: null,
    deliveredAt: null,
    carried: false,
    kind: null,
    removedAt: null,
    removedBy: null
  }));
}

// ## The board

// Where a line stands among the panel's lists.
type ListName = "queue" | "pending" | "held" | "merged" | "done";

function listOf(line: ItemLine): ListName {
  if (line.state?.kind === "done" || line.marks.some((mark) => mark.kind === "merged_into")) return "done";
  if (line.state?.kind === "merging_into") return "merged";
  if (line.state?.kind === "pending_approval") return "pending";
  // A held item still running stays in the queue, where it stood, until its run has ended; one wound down stays
  // there for good, since it is work asked to be resumed rather than put away.
  if (line.hold !== null && !ENDING.includes(line.hold.word) && line.hold.word !== "pause") return "held";

  return "queue";
}

// Whether a line meets the conditions a line carries enough to judge; the requester is judged once an item is
// opened, and the room by the listing's own narrowing.
function meets(line: ItemLine, conditions: TodoConditions): boolean {
  const worker = line.worker ?? WORKER_AGENT;
  const unfinished = line.marks.some((mark) => mark.kind === "unfinished");
  const resolved = line.marks.some((mark) => mark.kind === "unfinished_resolved");
  const cowork = line.marks.some((mark) => mark.kind === "cowork");

  if (conditions.worker !== null && worker !== conditions.worker) return false;
  if (conditions.unfinished === "open" && !unfinished) return false;
  if (conditions.unfinished === "resolved" && !resolved) return false;
  if (conditions.cowork !== null && cowork !== (conditions.cowork === "on")) return false;

  return true;
}

function standingOf(lines: readonly ItemLine[], running: ReadonlySet<number>): TodoStanding {
  const of = (name: ListName) => lines.filter((line) => listOf(line) === name);
  const queue = of("queue");

  return {
    queue: queue.length,
    running: queue.filter((line) => running.has(line.id) || ENDING.includes(line.hold?.word ?? "plain")).length,
    pending: of("pending").length,
    held: of("held").length,
    merged: of("merged").length
  };
}

// A deployment's hold as its deploy wrote it, kept as it stands; a hold that does not read as one is still a hold.
export function holdOf(value: unknown): DeployHold | null {
  if (value === null || value === undefined) return null;

  const hold = (typeof value === "object" ? value : {}) as Partial<DeployHold>;

  return {
    since: typeof hold.since === "number" ? hold.since : 0,
    until: typeof hold.until === "number" ? hold.until : 0,
    commits: Array.isArray(hold.commits) ? hold.commits : [],
    breaks: Array.isArray(hold.breaks) ? hold.breaks : null,
    waitingOn: Array.isArray(hold.waitingOn) ? hold.waitingOn : null,
    waitingFor: Array.isArray(hold.waitingFor) ? hold.waitingFor.filter((item) => Number.isInteger(item)) : null,
    passing: Array.isArray(hold.passing) ? hold.passing : []
  };
}

// Every line, finished ones included: a listing leaves out what is set aside and what is merged unless it is
// asked for those alone, so the three are read and put together, each item once, in the queue's order.
async function listing(room: number | null): Promise<ItemLine[]> {
  const chat = room === null ? "" : `&chat=${room}`;
  const read = await Promise.all(["", "&held", "&merged"].map((only) => jobRead<ItemLine[]>(`/todos?all${only}${chat}`)));

  return [...new Map(read.flat().map((line) => [line.id, line])).values()];
}

/**
 * The lists the panel stands on, read from bmo-job: the queue, what waits on the owner, what is set aside, what is
 * merged, and the finished as far back as the panel asked, narrowed by its conditions and its room. A search
 * reaches every open item whole and the finished by what their lines carry, since opening every finished item
 * to read it whole is a read per item.
 */
export async function readTodoBoard(
  input: { limit?: number; query?: string | null; conditions?: TodoConditions; room?: number | null } = {}
): Promise<TodoBoard> {
  const limit = input.limit ?? DONE_PAGE_SIZE;
  const query = input.query ?? null;
  const conditions = input.conditions ?? NO_CONDITIONS;
  const room = input.room ?? (conditions.chat !== null && conditions.chat !== NO_ROOM ? Number(conditions.chat) : null);
  const [all, holds, running] = await Promise.all([
    listing(room),
    jobRead<{ paused: boolean; deploy_hold?: unknown }>("/queue/holds"),
    runningItems()
  ]);
  const lines = all.filter((line) => meets(line, conditions));
  const open = lines.filter((line) => listOf(line) !== "done");
  // The finished stand newest first, by number, which is the order they were filed in.
  const finished = lines.filter((line) => listOf(line) === "done").sort((one, other) => other.id - one.id);
  const finishedFound =
    query === null ? finished : filterRecords(finished, (line) => [{ label: "", text: `#${line.id} ${line.title} ${line.status}`, shown: true }], query);
  const items = await itemsOf([...open.map((line) => line.id), ...finishedFound.slice(0, limit).map((line) => line.id)], running);
  const byId = new Map(items.map((item) => [item.id, item]));
  const requested = (item: TodoItem) => conditions.requester === null || item.requester === conditions.requester;
  const found = (item: TodoItem) => query === null || filterRecords([item], (one) => todoSearchFields(wording, one, []), query).length > 0;
  const listed = (name: ListName, from: readonly ItemLine[]) =>
    from.filter((line) => listOf(line) === name).flatMap((line) => byId.get(line.id) ?? []).filter(requested);
  const choices: TodoChoices = {
    workers: [...new Set(all.map((line) => line.worker ?? WORKER_AGENT))],
    chats: [],
    requesters: [...new Set(items.map((item) => item.requester))]
  };

  return {
    queue: listed("queue", open).filter(found),
    pending: listed("pending", open).filter(found),
    held: listed("held", open).sort((one, other) => (other.heldAt ?? 0) - (one.heldAt ?? 0)).filter(found),
    merged: listed("merged", open).sort((one, other) => (other.mergedAt ?? 0) - (one.mergedAt ?? 0)).filter(found),
    done: listed("done", finishedFound.slice(0, limit)),
    doneTotal: finishedFound.length,
    standingNotes: all.filter((line) => line.marks.some((mark) => mark.kind === "unfinished")).map((line) => ({ id: line.id, title: line.title })).sort((one, other) => other.id - one.id),
    choices,
    standing: standingOf(all, running),
    paused: holds.paused,
    deployHold: holdOf(holds.deploy_hold)
  };
}
