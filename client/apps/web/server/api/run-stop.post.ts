import { wording } from "#shared/wording";

import type { RunStopAll, RunStopped, StopTarget } from "#shared/types";

interface RunStopBody {
  target?: unknown;
  unit?: unknown;
  all?: unknown;
  why?: unknown;
}

const kinds = ["execution", "unit", "item"];

function target(value: unknown): StopTarget | null {
  const asked = value as Partial<StopTarget> | null | undefined;

  return typeof asked?.id === "string" && asked.id.trim() !== "" && kinds.some((kind) => kind === asked.kind)
    ? { kind: asked.kind as StopTarget["kind"], id: asked.id.trim() }
    : null;
}

// bmo-job's refusals of a stop, each with the pack's key for its sentence (bmo-job's `docs/api.md`, 「Stopping
// runs: `RunStopRefusal`」).
const STOP_CODES: Readonly<Record<string, string>> = {
  stop_target_wrong: "live.error.stop_target_wrong",
  reason_language: "live.error.stop_reason_language",
  stop_target_missing: "live.error.stop_target_needed",
  stop_target_absent: "live.error.stop_target_absent",
  stop_target_ended: "live.error.stop_target_ended",
  stop_coworker: "live.error.stop_coworker",
  stop_item_number: "live.error.stop_item_number"
};

// What bmo-job answers a stop with (bmo-job's `docs/api.md`, 「Stopping runs: `RunStopAnswer`」).
interface StopAnswer {
  code: "nothing_running" | "nothing_nameable" | "stop_confirmed" | "stop_asked" | "all_asked";
  target?: StopTarget;
  runs?: number;
  targets?: { target: StopTarget; state: "asked" | "confirmed" }[];
}

// Ending what is running, from the live board. Nothing here decides what a stop means or which kind is reached
// by what: the target travels to bmo-job, whose one stop verb has the whole of that, and the row's own ending —
// the room it tells, the record it closes — is the stopped run's. A body naming `unit` alone is still taken;
// `all` is the owner's one press for everything running.
export default defineEventHandler(async (event): Promise<RunStopped | RunStopAll> => {
  await requireUserSession(event);

  const body = await readBody<RunStopBody>(event);
  const asked = target(body?.target)
    ?? (typeof body?.unit === "string" && body.unit.trim() !== "" ? { kind: "unit" as const, id: body.unit.trim() } : null);

  if (body?.all !== true && asked === null) {
    throw fail(400, wording("live.error.stop_target_needed"));
  }

  const answer = await unitFetch<StopAnswer>(
    "JOB",
    "/run/stop",
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body?.all === true ? { all: true } : { target: asked, ...(typeof body?.why === "string" ? { why: body.why } : {}) })
    },
    { codes: STOP_CODES }
  );

  if ((answer.code === "stop_confirmed" || answer.code === "stop_asked") && answer.target !== undefined) {
    return { ok: true, target: answer.target, state: answer.code === "stop_confirmed" ? "confirmed" : "asked", runs: answer.runs ?? 1 };
  }

  return { ok: true, stopped: (answer.targets ?? []).map((one) => ({ target: one.target, state: one.state, runs: 1 })), refused: [] };
});
