import { brainClient } from "bmo-brain-client";

import { CONTROL_UNREACHABLE } from "#shared/config";
import { wording } from "#shared/wording";

import { unitAt } from "./units";
import { answered } from "./unit-answer";

import type { BrainClient } from "bmo-brain-client";
import type { RefusalBody, UnitAnswerCall } from "./unit-answer";

// The brain's fixed API as its published package states it (the brain's `docs/boundary-api.md`): JSON in camelCase,
// a page for a list, every call with the brain's token, and a refusal answered with its code and an English sentence.
// A refusal the brain carried out as an answer to a row's state holds its own code in `refusal` beside the answer's,
// and that code is the one said. The console says each by the pack's sentence for it (「Codes the brain answers
// with」).

export const BRAIN_CODES: Readonly<Record<string, string>> = {
  // Every route's.
  "bad-request": "memories.error.brain_bad_request",
  unauthorized: "memories.error.brain_unauthorized",
  "no-route": "memories.error.brain_no_route",
  "not-found": "memories.error.brain_not_found",
  "not-ready": "memories.error.brain_not_ready",
  conflict: "memories.error.brain_conflict",
  failed: "memories.error.brain_failed",
  // A row changed under the console's hand.
  missing: "memories.error.row_missing",
  "other-room": "memories.error.row_other_room",
  "already-up": "memories.error.row_already_up",
  "already-down": "memories.error.row_already_down",
  // The memory rows'.
  "no-agent": "memories.error.no_agent",
  "no-room": "memories.error.no_room",
  "why-missing": "memories.error.why_missing",
  "action-missing": "memories.error.action_missing",
  "state-missing": "memories.error.state_missing",
  "situation-unpaired": "memories.error.situation_unpaired",
  "situation-unfinished": "memories.error.situation_unfinished",
  "event-unknown": "memories.error.event_unknown",
  "event-takes-no-item": "memories.error.event_takes_no_item",
  "event-bad-item": "memories.error.event_bad_item",
  usage: "memories.error.usage",
  "memory-refused": "memories.error.memory_refused",
  // The fragments'.
  "no-desk": "documents.error.fragment_no_desk",
  "no-names": "documents.error.fragment_no_names",
  "out-of-place": "documents.error.fragment_out_of_place",
  "no-body": "documents.error.fragment_no_body",
  "unnamed-slots": "documents.error.fragment_unnamed_slots",
  "title-shape": "documents.error.fragment_title_shape",
  keywords: "documents.error.fragment_keywords",
  repeats: "documents.error.fragment_repeats",
  "nothing-stands": "documents.error.fragment_nothing_stands",
  "bad-merge": "documents.error.fragment_bad_merge",
  "bad-split": "documents.error.fragment_bad_split",
  "body-stands": "documents.error.fragment_body_stands",
  "row-refused": "documents.error.fragment_row_refused",
  // The documents', the digests' and the agents'.
  "brain-library": "memories.error.brain_library",
  "out-of-room": "memories.error.out_of_room",
  "not-owner": "memories.error.not_owner",
  "one-to-one": "memories.error.one_to_one",
  "no-memory": "memories.error.no_memory",
  "not-answered": "memories.error.not_answered",
  "no-brain": "memories.error.no_brain",
  "too-few-batches": "memories.error.too_few_batches",
  "brain-root": "memories.error.brain_root",
  "not-theirs": "memories.error.not_theirs",
  "no-watch": "memories.error.no_watch",
  ended: "memories.error.watch_ended",
  "no-owner": "memories.error.no_owner",
  "reminder-refused": "memories.error.reminder_refused",
  "no-reminder": "memories.error.no_reminder"
};

/** The brain at the address its setting names, asked with its token. */
export function brain(): BrainClient {
  const unit = unitAt("BRAIN");

  if (unit === null) throw fail(503, wording(CONTROL_UNREACHABLE));

  return brainClient(unit.address, unit.token ?? "");
}

// The refusal a refusal body holds: the brain's own, where it carried one beside the answer's code.
function heldRefusal(body: RefusalBody): RefusalBody {
  return body.refusal ?? body;
}

/** One call on the brain, its refusals said in the pack's words. */
export async function brainCall<T, E extends RefusalBody>(
  asked: Promise<{ data?: T; error?: E; response: Response }>,
  call: UnitAnswerCall = {}
): Promise<T> {
  return await answered(asked, { ...call, codes: BRAIN_CODES, refusal: heldRefusal });
}
