import type { RoomStats } from "#shared/types";

// What one room adds up to: the receiver's figures for it (its `docs/boundary-api.md`, 「A room's figures」), its
// messages by the hour counted in the zone the console shows every time in.
export default defineEventHandler(async (event): Promise<RoomStats> => {
  await requireUserSession(event);

  const chatId = requireChatId(event);

  await requireReadableRoom(event, chatId);

  const { zone } = useAppConfig().bmoUi as { zone: string };
  const figures = await receiverCall(receiver().GET("/rooms/{roomId}/figures", { params: { path: { roomId: chatId }, query: { zone } } }));

  return {
    chatId: figures.room,
    messages: figures.messages,
    fromPeople: figures.fromPeople,
    speakers: figures.speakers,
    firstAt: figures.firstAt,
    lastAt: figures.lastAt,
    lastAnswerAt: figures.lastAnswerAt,
    top: figures.top,
    byHour: figures.byHour,
    agents: figures.agents.map((agent) => ({
      agent: agent.agentName,
      label: agent.label,
      messages: agent.messages,
      lastAnswerAt: agent.lastAnswerAt,
      sessionMode: agent.sessionMode as RoomStats["agents"][number]["sessionMode"],
      model: agent.model,
      lastMode: agent.lastMode as RoomStats["agents"][number]["lastMode"],
      lastModel: agent.lastModel,
      lastEffort: typeof agent.lastEffort === "string" ? agent.lastEffort : null,
      runs: agent.runs,
      contextTokens: typeof agent.contextTokens === "number" ? agent.contextTokens : null,
      lastRunAt: agent.lastRunAt,
      weekTokens: agent.weekTokens,
      totalTokens: agent.totalTokens
    }))
  };
});
