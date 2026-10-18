import { wording } from "#shared/wording";

import type { VoiceBriefingView } from "#shared/types";

// Whether the voice briefing is on. The receiver sets the switch and reads it by no route the console can ask
// (docs/boundary.md, 「Gaps the console meets」), so the section says it cannot be read.
export default defineEventHandler(async (event): Promise<VoiceBriefingView> => {
  await requireUserSession(event);

  throw fail(503, wording("agents.error.voice_briefing_unavailable"));
});
