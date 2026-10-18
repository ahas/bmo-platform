import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";

import { PUSH_DENIED, PUSH_UNCONFIGURED, PUSH_UNSUPPORTED } from "../../../apps/web/shared/config";
import { deviceOf, keyBytes, otherDevices, pushStanding, standingNotice } from "../../../apps/web/app/domain/push";

import type { PushDevice } from "#shared/types";

// What the console may say about ringing this browser, and how it tells this device from the
// others the account has. A device is known by the endpoint its own browser gave it.

function device(endpoint: string): PushDevice {
  return { endpoint, kinds: ["question"], createdAt: 1_756_800_000, lastSentAt: null };
}

describe("whether this browser can be rung", () => {
  test("a browser without push is told so, whatever the server holds", () => {
    expect(pushStanding({ supported: false, permission: "granted", configured: true })).toBe("unsupported");
    expect(standingNotice(wording, "unsupported")).toBe(wording(PUSH_UNSUPPORTED));
  });

  test("a server with no keys cannot ring anybody, however willing the browser", () => {
    expect(pushStanding({ supported: true, permission: "granted", configured: false })).toBe("unconfigured");
    expect(standingNotice(wording, "unconfigured")).toBe(wording(PUSH_UNCONFIGURED));
  });

  test("a refusal in the browser is its own fact and says how to undo it", () => {
    expect(pushStanding({ supported: true, permission: "denied", configured: true })).toBe("denied");
    expect(standingNotice(wording, "denied")).toBe(wording(PUSH_DENIED));
  });

  test("nothing is in the way, so nothing is said", () => {
    expect(pushStanding({ supported: true, permission: "default", configured: true })).toBe("ready");
    expect(pushStanding({ supported: true, permission: "granted", configured: true })).toBe("ready");
    expect(standingNotice(wording, "ready")).toBeNull();
  });
});

describe("this device among the account's", () => {
  const devices = [device("https://push.example/a"), device("https://push.example/b")];

  test("is the one wearing this browser's own endpoint", () => {
    expect(deviceOf(devices, "https://push.example/b")?.endpoint).toBe("https://push.example/b");
    expect(otherDevices(devices, "https://push.example/b").map((one) => one.endpoint)).toEqual([
      "https://push.example/a"
    ]);
  });

  test("a browser that has subscribed to nothing is none of them, and they are all the others", () => {
    expect(deviceOf(devices, null)).toBeNull();
    expect(otherDevices(devices, null)).toHaveLength(2);
  });

  test("an endpoint the account does not hold is nobody", () => {
    expect(deviceOf(devices, "https://push.example/gone")).toBeNull();
  });
});

describe("the key a browser subscribes with", () => {
  test("comes back as the bytes it was, whatever the padding", () => {
    // base64url of the five bytes below, with the padding a url-safe key never carries.
    expect([...new Uint8Array(keyBytes("AQIDBAU"))]).toEqual([1, 2, 3, 4, 5]);
  });

  test("reads the url-safe alphabet rather than the ordinary one", () => {
    expect([...new Uint8Array(keyBytes("-_8"))]).toEqual([251, 255]);
  });
});
