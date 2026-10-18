import type { RoomAgentStats, RoomSpeaker, RoomStats } from "#shared/types";
import { countText } from "./numbers";

import type { BarColor } from "bmo-ui/app/domain/bars";
import type { Translate } from "#shared/wording";

// How a room's figures read. The counting is the server's; this is the shape they take on
// screen — which hour the room is busiest in, how tall each hour's bar stands, and what share
// of the talking one person did.

// A bar of nothing is not drawn at all, and the busiest hour is the top of the scale, so the
// shape of a day reads whether the room saw ten messages or ten thousand.
export function hourBars(byHour: number[]): number[] {
  const peak = Math.max(0, ...byHour);

  return byHour.map((messages) =>
    peak === 0 || messages === 0 ? 0 : Math.max(4, Math.round((messages / peak) * 100))
  );
}

// The hour the room is busiest in, as a clock reads. A room with nothing in it has no hour.
export function peakHour(byHour: number[]): number | null {
  const peak = Math.max(0, ...byHour);

  return peak === 0 ? null : byHour.indexOf(peak);
}

export function peakLabel(t: Translate, byHour: number[]): string {
  const hour = peakHour(byHour);

  return hour === null ? "—" : t("room.label.peak_hour", { hour: String(hour).padStart(2, "0") });
}

// The room's messages divided into those it is made of: the people's, and each agent's that spoke
// there. Every message in the archive was written by an agent or by somebody else, so the parts
// close over the whole exactly — the server counts each agent's and calls the rest people — and
// that is what makes this a thing to draw as one track cut into parts rather than as numbers
// standing side by side.
//
// A room with nothing in it divides into nothing, and answers with no parts at all: a track of
// noughts is a shape that says something about a room that has said nothing.
export interface MessageShare {
  label: string;
  value: number;
  color: BarColor;
  text: string;
}

// The accent colours the console's other bars name their parts by, so this card stands in the same
// palette as every bar beside it rather than in one of its own. Each differs from the ones before it
// on screen: the theme draws secondary and info in one blue, so info is not among them.
const SHARE_COLORS: readonly BarColor[] = ["primary", "secondary", "warning", "neutral", "success", "error"];

// How an agent is called on the sheet: its own label, or its name where its row carries none.
export function agentLabel(agent: RoomAgentStats): string {
  return agent.label ?? agent.agent;
}

export function messageShares(t: Translate, stats: RoomStats): MessageShare[] {
  if (stats.messages <= 0) {
    return [];
  }

  const parts = [
    { label: t("room.label.share_people"), value: stats.fromPeople },
    ...stats.agents.filter((agent) => agent.messages > 0).map((agent) => ({ label: agentLabel(agent), value: agent.messages }))
  ];

  return parts.map((part, at) => ({ ...part, color: SHARE_COLORS[at % SHARE_COLORS.length]!, text: countText(part.value) }));
}

// What share of the room's own talking one person did. An agent's answers are not talking and
// are not counted, here or in the total.
export function speakerShare(speaker: RoomSpeaker, stats: RoomStats): number {
  return stats.fromPeople === 0 ? 0 : Math.round((speaker.messages / stats.fromPeople) * 100);
}

// How long the room has been going, in the words a person uses for it.
export function spanText(t: Translate, stats: RoomStats): string {
  if (stats.firstAt === null || stats.lastAt === null) {
    return "—";
  }

  const days = Math.max(1, Math.round((stats.lastAt - stats.firstAt) / 86_400));

  return days < 60 ? t("room.label.span_days", { n: days }) : t("room.label.span_months", { n: Math.round(days / 30) });
}

export function emptyRoomStats(chatId: number): RoomStats {
  return {
    chatId,
    messages: 0,
    fromPeople: 0,
    speakers: 0,
    firstAt: null,
    lastAt: null,
    lastAnswerAt: null,
    top: [],
    byHour: Array.from({ length: 24 }, () => 0),
    agents: []
  };
}
