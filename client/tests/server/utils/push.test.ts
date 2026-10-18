import { expect, test } from "bun:test";

import { scratchWebDatabase } from "../../support/web-database";

// Which devices a notification reaches (docs/web.md, 「Notifications」): a device stores the kinds it
// refused, so a kind added after it subscribed reaches it; the account screen's test reaches every device
// of that account whatever it switched off.

await scratchWebDatabase();

const { useWebSql } = await import("../../../apps/web/server/utils/db");
const { askedKinds, deviceHears, devicesOf, devicesToRing, receivedKinds, refusedKinds, saveDevice } = await import("../../../apps/web/server/utils/push");
const { newDevicePushKinds } = await import("../../../apps/web/shared/config");

(globalThis as Record<string, unknown>).useWebSql = useWebSql;

// Four devices, each with the kinds it refused: one refusing two kinds, one refusing nothing, one refusing
// every kind it was offered when it subscribed, and another account's.
for (const [endpoint, user, refused, at] of [
  ["https://push/phone", "owner", "report,mother", 1],
  ["https://push/desk", "owner", "", 2],
  ["https://push/quiet", "owner", "question,report,mother", 3],
  ["https://push/other", "guest", "question,report", 4]
] as const) {
  await useWebSql().run(
    "INSERT INTO web_push_devices (endpoint, user_name, p256dh, auth, kinds, refused, created_at) VALUES ($1, $2, 'k', 'a', '', $3, $4)",
    endpoint,
    user,
    refused,
    at
  );
}

const endpoints = (rows: { endpoint: string }[]) => rows.map((row) => row.endpoint).sort();

test("a device hears every kind it has not refused, a kind added since it subscribed among them", async () => {
  // A refusal of a kind since retired is no longer named, and the room kind came after all of them.
  expect((await devicesOf("owner")).map((device) => device.kinds)).toEqual([["question", "room", "mail"], ["question", "room", "mail"], ["room", "mail"]]);
});

test("a kind reaches every device that has not refused it", async () => {
  expect(endpoints(await devicesToRing({ kind: "mail" }))).toEqual(["https://push/desk", "https://push/other", "https://push/phone", "https://push/quiet"]);
  expect(endpoints(await devicesToRing({ kind: "question" }))).toEqual(["https://push/desk", "https://push/phone"]);
  expect(deviceHears("mail,report", { kind: "mail" })).toBe(false);
  expect(deviceHears("mail,weather", { kind: "question" })).toBe(true);
});

test("the test ring reaches every device of the account and no other, whatever they refused", async () => {
  expect(endpoints(await devicesToRing({ kind: "test" }, "owner"))).toEqual(["https://push/desk", "https://push/phone", "https://push/quiet"]);
  expect(await devicesToRing({ kind: "test" })).toEqual([]);
  expect(deviceHears("question,report,room,mail", { kind: "test" })).toBe(true);
});

test("a device saying what it wants is stored as what it refuses", async () => {
  await saveDevice({ user: "owner", endpoint: "https://push/phone", p256dh: "k", auth: "a", kinds: ["mail", "room"], now: 5 });

  expect(await useWebSql().get("SELECT refused FROM web_push_devices WHERE endpoint = $1", "https://push/phone")).toEqual({ refused: "question" });
  expect((await devicesOf("owner"))[0]!.kinds).toEqual(["room", "mail"]);
});

test("the kinds asked for, heard and refused", () => {
  expect(askedKinds(undefined)).toEqual(newDevicePushKinds());
  expect(newDevicePushKinds()).toEqual(["question", "room", "mail"]);
  expect(askedKinds(["mail", "weather", "mail"])).toEqual(["mail"]);
  expect(receivedKinds(["question", "mail"])).toEqual(["room"]);
  expect(refusedKinds(["question", "mail"])).toEqual(["room"]);
});
