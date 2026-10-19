import type { VoiceBriefingView } from "#shared/types";

// Whether the voice briefing is on, and whether a briefing plays by itself: the receiver's `GET /voice-briefing`,
// which reads the switch without moving it.
export default defineEventHandler(async (event): Promise<VoiceBriefingView> => {
  await requireUserSession(event);

  return await receiverRead<VoiceBriefingView>("/voice-briefing");
});
