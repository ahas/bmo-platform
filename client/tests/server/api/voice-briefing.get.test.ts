import { describe, expect, test } from "bun:test";

import { asked, routeOf } from "../../support/nitro";
import { answer, standIn } from "../../support/units";

import type { VoiceBriefingView } from "../../../apps/web/shared/types";

// How the voice briefing's switch stands, read from the receiver without moving it.

const receiver = standIn("RECEIVER", () => answer({ on: true, autoplay: false }));

const handler = await routeOf<VoiceBriefingView>("../../apps/web/server/api/voice-briefing.get");

describe("reading the voice briefing's switch", () => {
  test("is the receiver's `GET /voice-briefing`, answered as it stands", async () => {
    receiver.asked.length = 0;

    expect((await asked(handler, {})).answer).toEqual({ on: true, autoplay: false });
    expect(receiver.asked.map((one) => [one.method, one.path])).toEqual([["GET", "/voice-briefing"]]);
  });
});
