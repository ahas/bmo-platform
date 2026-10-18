import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";

import { emptyRoomStats, hourBars, messageShares, peakHour, peakLabel, spanText, speakerShare } from "../../../apps/web/app/domain/roomstats";

import type { RoomAgentStats, RoomStats } from "../../../apps/web/shared/types";
import { ROOM_ID } from "../../support/ids";
import KO from "../../fixtures/app/domain/roomstats.test.ko.json";

const day = 86_400;

function hours(counts: Record<number, number>): number[] {
  return Array.from({ length: 24 }, (_, hour) => counts[hour] ?? 0);
}

function stats(overrides: Partial<RoomStats> = {}): RoomStats {
  return { ...emptyRoomStats(ROOM_ID), ...overrides };
}

describe("the day a room lives", () => {
  test("the busiest hour is the top of the scale and every other bar stands against it", () => {
    expect(hourBars(hours({ 0: 5, 9: 10, 21: 20 }))[21]).toBe(100);
    expect(hourBars(hours({ 0: 5, 9: 10, 21: 20 }))[9]).toBe(50);
    // An hour with something in it is never invisible, however small its share.
    expect(hourBars(hours({ 3: 1, 21: 1000 }))[3]).toBe(4);
    expect(hourBars(hours({ 3: 1, 21: 1000 }))[4]).toBe(0);
  });

  test("a room with nothing in it has no bars and no hour", () => {
    expect(hourBars(hours({}))).toEqual(Array.from({ length: 24 }, () => 0));
    expect(peakHour(hours({}))).toBeNull();
    expect(peakLabel(wording, hours({}))).toBe("—");
  });

  test("the hour reads as a clock", () => {
    expect(peakHour(hours({ 2: 3, 23: 9 }))).toBe(23);
    expect(peakLabel(wording, hours({ 2: 3, 23: 9 }))).toBe(KO.line_37);
    expect(peakLabel(wording, hours({ 7: 2 }))).toBe(KO.line_38);
  });
});

// An agent of the room as the server hands it, saying as much as the test names.
function agent(name: string, label: string | null, messages: number): RoomAgentStats {
  return {
    agent: name,
    label,
    messages,
    lastAnswerAt: null,
    sessionMode: null,
    model: null,
    lastMode: null,
    lastModel: null,
    lastEffort: null,
    runs: 0,
    contextTokens: null,
    lastRunAt: null,
    weekTokens: 0,
    totalTokens: 0
  };
}

describe("who did the talking", () => {
  test("a share is of what the people said, not of what an agent answered", () => {
    const room = stats({ messages: 100, fromPeople: 40, agents: [agent("bmo", KO.line_64, 60)] });

    expect(speakerShare({ name: "Owner", messages: 30 }, room)).toBe(75);
  });

  test("a room nobody has spoken in divides by nothing", () => {
    expect(speakerShare({ name: "Owner", messages: 0 }, stats())).toBe(0);
  });
});

describe("how long the room has been going", () => {
  test("days while they are few, months once they are many", () => {
    expect(spanText(wording, stats({ firstAt: 1_000_000, lastAt: 1_000_000 + 3 * day }))).toBe(KO.line_76);
    expect(spanText(wording, stats({ firstAt: 1_000_000, lastAt: 1_000_000 + 90 * day }))).toBe(wording("room.label.span_months", { n: "3" }));
  });

  test("a room with no message says nothing about its age", () => {
    expect(spanText(wording, stats())).toBe("—");
  });
});

// Who wrote the room's messages. Every message in the archive was written by an agent or by
// somebody else and the server counts it exactly that way, so the parts close over the whole and
// the card can draw them as one track cut into parts. That closing is the whole reason there is a
// bar here rather than numbers side by side, so it is what these read for.

describe("the parts the room's messages divide into", () => {
  test("are the people and each agent that spoke, closing over the count the card prints above them", () => {
    const room = stats({ messages: 2629, fromPeople: 712, agents: [agent("bmo", KO.line_92, 1900), agent("agent-a", KO.line_92_2, 17)] });
    const shares = messageShares(wording, room);

    expect(shares.map((one) => one.label)).toEqual([KO.line_95, KO.line_95_2, KO.line_95_3]);
    expect(shares.reduce((sum, one) => sum + one.value, 0)).toBe(room.messages);
  });

  test("leave out an agent of the room that has said nothing there", () => {
    const shares = messageShares(wording, stats({ messages: 10, fromPeople: 4, agents: [agent("agent-a", KO.line_100, 6), agent("bmo", KO.line_100_2, 0)] }));

    expect(shares.map((one) => one.label)).toEqual([KO.line_102, KO.line_102_2]);
  });

  test("call an agent whose row carries no label by its name", () => {
    expect(messageShares(wording, stats({ messages: 3, fromPeople: 1, agents: [agent(`chat:${ROOM_ID}`, null, 2)] }))[1]?.label).toBe(`chat:${ROOM_ID}`);
  });

  test("carry the figure the legend reads out, in the console's own grouping", () => {
    expect(messageShares(wording, stats({ messages: 2629, fromPeople: 712, agents: [agent("bmo", KO.line_110, 1917)] })).map((one) => one.text)).toEqual([
      "712",
      "1,917"
    ]);
  });

  test("wear the accent colours the console's other bars name their parts by, one each", () => {
    // A card in a palette of its own would read as a different kind of picture from the bars
    // standing beside it, which are the same kind of picture.
    expect(messageShares(wording, stats({ messages: 10, fromPeople: 4, agents: [agent("bmo", KO.line_119, 5), agent("agent-a", KO.line_119_2, 1)] })).map((one) => one.color)).toEqual([
      "primary",
      "secondary",
      "warning"
    ]);
  });

  test("are nothing at all in a room that has said nothing", () => {
    // A track of noughts is a shape, and a shape says something about a room that has said
    // nothing. There is no bar in that card at all.
    expect(messageShares(wording, stats({ messages: 0, fromPeople: 0, agents: [agent("bmo", KO.line_129, 0)] }))).toEqual([]);
  });
});

describe("a room's sheet before anything has been read for it", () => {
  test("names no agent yet", () => {
    expect(emptyRoomStats(ROOM_ID).agents).toEqual([]);
  });
});
