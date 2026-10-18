import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";
import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

import type { VoiceBriefingChanged } from "../../../apps/web/shared/types";

// What the route refuses before the receiver is ever called, and what it hands the receiver when it does; a
// refusal of the receiver's is said by the pack's sentence for its code.

let switchedOff = false;

const receiver = standIn("RECEIVER", (one) =>
  switchedOff
    ? answer({ code: "switched-off", error: "The voice briefing is switched off." }, 409)
    : answer({ ok: true, on: one.body?.on ?? true, autoplay: one.body?.autoplay ?? false })
);

const handler = await routeOf<VoiceBriefingChanged>("../../apps/web/server/api/voice-briefing.post");

describe("switching the voice briefing", () => {
  test("hands the receiver the one value it is asked to set", async () => {
    receiver.asked.length = 0;

    expect((await asked(handler, { body: { on: false } })).answer).toEqual({ ok: true, on: false, autoplay: false });
    expect((await asked(handler, { body: { autoplay: true } })).answer).toEqual({ ok: true, on: true, autoplay: true });
    expect(receiver.asked.map((one) => [one.path, one.body])).toEqual([
      ["/voice-briefing", { on: false }],
      ["/voice-briefing", { autoplay: true }]
    ]);
  });

  test("a request that says neither, or both, is refused here, in words, and never reaches the receiver", async () => {
    receiver.asked.length = 0;

    for (const body of [{}, { on: "off" }, null, { on: true, autoplay: true }]) {
      expect(await asked(handler, { body })).toMatchObject({ status: 400, message: wording("agents.error.voice_briefing_direction") });
    }

    expect(receiver.asked).toEqual([]);
  });

  test("the receiver's refusal is said by the pack's sentence for its code", async () => {
    switchedOff = true;

    try {
      expect(await asked(handler, { body: { autoplay: true } })).toMatchObject({ status: 409, message: wording("room.error.briefing_switched_off") });
    } finally {
      switchedOff = false;
    }
  });
});
