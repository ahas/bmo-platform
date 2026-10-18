import {
  APPEND_LIMIT,
  isAgentName,
  NOTE_LIMIT,
  RAISE_LIMIT,
  RAISE_TITLE_LIMIT,
  RESOLUTION_LIMIT,
  STOP_REASON_LIMIT,
  STOP_REASON_NEEDED
} from "#shared/config";
import { wording } from "#shared/wording";

import type { TodoAction, TodoActionResult } from "#shared/types";

interface TodoBody {
  action?: unknown;
  id?: unknown;
  direction?: unknown;
  order?: unknown;
  note?: unknown;
  noteId?: unknown;
  why?: unknown;
  title?: unknown;
  said?: unknown;
  text?: unknown;
  sources?: unknown;
  worker?: unknown;
  cowork?: unknown;
}

const ACTIONS: readonly TodoAction[] = [
  "approve",
  "unapprove",
  "move",
  "top",
  "bottom",
  "drop",
  "done",
  "reorder",
  "resolve",
  "raise",
  "append",
  "unappend",
  "hold",
  "stop",
  "release",
  "worker",
  "cowork"
];

// Changing one row of the TODO. The table is bmo-job's to write, so the console asks and
// bmo-job writes; a bmo-job that is not there changes nothing. Filing from nothing is not among the
// verbs — an item carries the exchange it came from, and that happens in a chat — but raising
// one out of a unfinished note is, because there the exchange is the note itself and the line he writes
// about it, and a unfinished note whose every strand is done or standing as its own row is the only way one
// is ever resolved. Adding words to a row that already stands is here for the same reason the
// raising is: what he thinks of an hour into the work is said at the screen he is watching it
// from, and the only other way to say it is a shell.
export default defineEventHandler(async (event): Promise<TodoActionResult> => {
  // Who is signed in goes to bmo-job with the change, which keeps it where a change is recorded
  // under its hand — an approval given or taken back (docs/todo.md, 「Approval」).
  const signed = await signedInUser(event);

  const body = await readBody<TodoBody>(event);
  const action = body?.action;
  const id = body?.id;

  if (typeof action !== "string" || !(ACTIONS as readonly string[]).includes(action)) {
    throw fail(400, wording("todo.error.bad_action"));
  }

  if (typeof id !== "number" || !Number.isInteger(id)) {
    throw fail(400, wording("todo.error.bad_item_id"));
  }

  const direction = body?.direction;

  if (action === "move" && direction !== "up" && direction !== "down") {
    throw fail(400, wording("todo.error.bad_direction"));
  }

  const order = body?.order;

  // Dragging saves the whole queue, so the order is the request: a list of item numbers, and
  // whether it is still bmo-job's own queue is bmo-job's to say.
  if (action === "reorder" && !isOrder(order)) {
    throw fail(400, wording("todo.error.bad_order"));
  }

  const note = body?.note;

  if (note !== undefined && (typeof note !== "string" || note.length > NOTE_LIMIT)) {
    throw fail(400, wording("todo.error.bad_note"));
  }

  const title = body?.title;
  const said = body?.said;
  const sources = body?.sources;

  // A row raised out of a unfinished note needs a name and a reason. Neither is guessed at from the note.
  if (action === "raise") {
    if (typeof title !== "string" || title.trim() === "" || title.length > RAISE_TITLE_LIMIT) {
      throw fail(400, wording("todo.error.raise_title_empty"));
    }

    if (typeof said !== "string" || said.trim() === "" || said.length > RAISE_LIMIT) {
      throw fail(400, wording("todo.error.raise_body_empty"));
    }

    // The other notes he named beside the one he is standing on. Whether each of them left a
    // unfinished note to raise out of is bmo-job's to say; what is checked here is that they are numbers.
    if (sources !== undefined && !isOrder(sources)) {
      throw fail(400, wording("todo.error.bad_source_ids"));
    }
  }

  const text = body?.text;

  // Words added to an item that already stands. There is nothing to add where nothing was
  // typed, and what is added is the next condition of the work rather than a document of its
  // own, so both ends are held here rather than reaching bmo-job.
  if (action === "append" && (typeof text !== "string" || text.trim() === "" || text.length > APPEND_LIMIT)) {
    throw fail(400, wording("todo.error.note_empty"));
  }

  const noteId = body?.noteId;

  // Which of the appended words are being taken back. There is no last one and no implied one: the
  // press is on a particular lot of words, so the number comes with it.
  if (action === "unappend" && (typeof noteId !== "number" || !Number.isInteger(noteId))) {
    throw fail(400, wording("todo.error.note_pick"));
  }

  const why = body?.why;

  // What makes a unfinished note no longer outstanding is the whole of what resolving writes, so a resolving
  // with nothing to say is refused here rather than reaching bmo-job.
  if (action === "resolve" && (typeof why !== "string" || why.trim() === "" || why.length > RESOLUTION_LIMIT)) {
    throw fail(400, wording("todo.error.resolve_reason_empty"));
  }

  // A stop cuts work in flight, so its reason is the one thing the item carries until it is taken
  // up again, and a stop with nothing to say is refused here rather than reaching bmo-job.
  if (action === "stop" && (typeof why !== "string" || why.trim() === "" || why.length > STOP_REASON_LIMIT)) {
    throw fail(400, wording(STOP_REASON_NEEDED));
  }

  const worker = body?.worker;

  // Which worker is to do it. Null takes the assignment off; a name is a name, and whether that
  // worker exists is bmo-job's to say, since the list of them is the archive's.
  if (action === "worker" && worker !== null && !isAgentName(worker)) {
    throw fail(400, wording("todo.error.bad_worker"));
  }

  const cowork = body?.cowork;

  // Whether its job fans out into branches. It is on or it is off, and nothing else is a state
  // the switch can be in.
  if (action === "cowork" && typeof cowork !== "boolean") {
    throw fail(400, wording("todo.error.bad_cowork"));
  }

  // Raising answers with the number of the row it made, and the console names it back to him: a row he
  // cannot find is a row he has to go looking for. A refusal — an order read before the queue moved, a row
  // that no longer stands as the press read it — is said in the pack's words for its code. The note a taking
  // back names goes by its number where an approval's note is words.
  const answer = await jobPost<{ moved: boolean; stopped?: number | null; raised?: number | null }>(
    "/todo",
    {
      action,
      id,
      direction,
      order,
      note: action === "unappend" ? noteId : note,
      why,
      title,
      said,
      text,
      sources,
      worker,
      cowork,
      by: signed.name
    },
    action === "stop" ? { timeoutMs: JOB_VERB_TIMEOUT_MS } : {}
  );

  return { ok: true, raised: answer.raised ?? undefined, stopped: answer.stopped ?? undefined };
});

function isOrder(value: unknown): value is number[] {
  return Array.isArray(value) && value.every((entry) => typeof entry === "number" && Number.isInteger(entry));
}
