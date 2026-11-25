import { describe, expect, test } from "bun:test";

import { STOPPING_FRAME } from "../../../apps/web/server/utils/relays";
import { consoleFrame, KEEPALIVE_FRAME, RelayOutput, SseFramer } from "../../../apps/web/server/utils/sse-frames";
import KO from "../../fixtures/server/utils/sse-frames.test.ko.json";

const bytes = (text: string) => new TextEncoder().encode(text);

// The frames a browser would read out of what the relay wrote: each blank-line-ended block, with
// its fields.
function frames(written: string): { event: string | null; id: string | null; data: string | null }[] {
  return written
    .split("\n\n")
    .filter((block) => block.trim() !== "")
    .map((block) => {
      const field = (name: string) => block.split("\n").find((line) => line.startsWith(`${name}: `))?.slice(name.length + 2) ?? null;

      return { event: field("event"), id: field("id"), data: field("data") };
    });
}

describe("the bot's stream cut into whole events", () => {
  test("an event split across chunks comes out whole, once all of it has come", () => {
    const framer = new SseFramer();

    expect(framer.push(bytes("id: 7\nevent: bot\nda"))).toEqual([]);
    expect(framer.holding()).toBe(true);
    expect(framer.push(bytes('ta: {"id":7}\n'))).toEqual([]);
    expect(framer.push(bytes('\nid: 8\nevent: bot\ndata: {"id":8}\n\n: keep\n\n'))).toEqual([
      'id: 7\nevent: bot\ndata: {"id":7}\n\n',
      'id: 8\nevent: bot\ndata: {"id":8}\n\n',
      ": keep\n\n",
    ]);
    expect(framer.holding()).toBe(false);
  });

  test("a character cut between two chunks is held with its event, and CRLF ends an event too", () => {
    const framer = new SseFramer();
    const text = bytes(KO.line_38);
    const cut = text.indexOf(0xed) + 1;

    expect(framer.push(text.slice(0, cut))).toEqual([]);
    expect(framer.push(text.slice(cut))).toEqual([KO.line_42]);
  });
});

describe("what a relay writes", () => {
  test("the console's own events, the keepalive and the stopping word never land inside one of the bot's", () => {
    let written = "";
    const output = new RelayOutput((chunk) => {
      written += new TextDecoder().decode(chunk);
    });
    const bot = (id: number) => `id: ${id}\nevent: bot\ndata: {"id":${id},"type":"sent"}\n\n`;
    const upstream = bytes(bot(1) + bot(2) + bot(3));

    // The bot's bytes arrive cut at every point, with this app's frames written between the cuts.
    for (let at = 0; at < upstream.length; at += 7) {
      output.upstream(upstream.slice(at, at + 7));
      output.own(consoleFrame({ name: "mail", data: { account: "dev" } }));

      if (at % 21 === 0) {
        output.own(": keep\n\n");
      }
    }

    output.own(STOPPING_FRAME);

    const read = frames(written);

    expect(read.filter((one) => one.event === "bot").map((one) => [one.id, one.data])).toEqual([
      ["1", '{"id":1,"type":"sent"}'],
      ["2", '{"id":2,"type":"sent"}'],
      ["3", '{"id":3,"type":"sent"}'],
    ]);
    expect(read.filter((one) => one.event === "mail").every((one) => one.data === '{"account":"dev"}')).toBe(true);
    expect(read.at(-1)).toEqual({ event: "stopping", id: null, data: "{}" });
    expect(read.every((one) => ["bot", "mail", "stopping", null].includes(one.event))).toBe(true);
  });

  test("a relay that passes only some of the bot's events drops the rest whole, and writes its own frames as ever", () => {
    let written = "";
    const output = new RelayOutput((chunk) => {
      written += new TextDecoder().decode(chunk);
    }, (frame) => frame.includes('"type":"briefing"'));
    const bot = (id: number, type: string) => `id: ${id}\nevent: bot\ndata: {"id":${id},"type":"${type}"}\n\n`;
    const upstream = bytes(bot(1, "sent") + bot(2, "briefing") + bot(3, "live"));

    for (let at = 0; at < upstream.length; at += 5) {
      output.upstream(upstream.slice(at, at + 5));
    }

    output.own(KEEPALIVE_FRAME);

    expect(frames(written)).toEqual([
      { event: "bot", id: "2", data: '{"id":2,"type":"briefing"}' },
      { event: "keep", id: null, data: "{}" },
    ]);
  });
  test("a console event carries no id, so it never moves a tab's cursor", () => {
    expect(consoleFrame({ name: "mail", data: { account: "owner" } })).toBe('event: mail\ndata: {"account":"owner"}\n\n');
    expect(frames(consoleFrame({ name: "mail", data: undefined }))[0]).toEqual({ event: "mail", id: null, data: "{}" });
  });

  test("the keepalive is a frame the page hears, and moves no cursor either", () => {
    expect(frames(KEEPALIVE_FRAME)).toEqual([{ event: "keep", id: null, data: "{}" }]);
  });
});
