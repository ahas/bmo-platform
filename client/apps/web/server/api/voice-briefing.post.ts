import { wording } from "#shared/wording";

import type { VoiceBriefingChanged } from "#shared/types";

// One of the voice briefing's two switches moved: the whole briefing, or its playing by itself. The receiver sets
// it and has the owner's session say so where the whole briefing moved; what is refused here is only a request
// that names neither, or both.
export default defineEventHandler(async (event): Promise<VoiceBriefingChanged> => {
  await requireUserSession(event);

  const body = await readBody<{ on?: unknown; autoplay?: unknown }>(event);
  const on = typeof body?.on === "boolean" ? body.on : undefined;
  const autoplay = typeof body?.autoplay === "boolean" ? body.autoplay : undefined;

  if ((on === undefined) === (autoplay === undefined)) {
    throw fail(400, wording("agents.error.voice_briefing_direction"));
  }

  return await receiverPost<VoiceBriefingChanged>("/voice-briefing", on === undefined ? { autoplay } : { on });
});
