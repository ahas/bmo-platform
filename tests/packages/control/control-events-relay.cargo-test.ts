import "@agency/test-support/setup";

import { afterAll, beforeAll, describe, expect, test } from "bun:test";

import { agencyEventsRelayed, controlServer, emitAgencyEvent } from "@agency/agent/events";
import { emptyDatabase } from "@agency/test-support/store";

import { frontAlone, frontBinary } from "./control-contract-base";

import type { ControlBase } from "./control-contract-base";

// An event a TypeScript process of agency's publishes reaches the server's event stream (docs/api.md,
// 「Events」): the process posts it to agency's server, found on the port and with the token its
// environment names, and a consumer reads it off `GET /events` by its kind.

const token = "secret";
// The binary is read as the file loads, so no hook spends its own time on it.
const binary = frontBinary();
// Made at the file's top, as a scratch store must be.
const store = await emptyDatabase();
const named = { port: process.env.AGENCY_CONTROL_PORT, token: process.env.AGENCY_CONTROL_TOKEN };

let front: ControlBase | null = null;

beforeAll(async () => {
  front = await frontAlone(binary, token, { BMO_ARCHIVE_URL: store });
});

afterAll(async () => {
  await front?.close();
  process.env.AGENCY_CONTROL_PORT = named.port;
  process.env.AGENCY_CONTROL_TOKEN = named.token ?? "";
});

describe("an event a TypeScript process publishes", () => {
  test("is relayed to the server and read off its event stream by its kind", async () => {
    // The test home names no server, so nothing is relayed until the process names one.
    expect(controlServer()).toBeNull();

    process.env.AGENCY_CONTROL_PORT = String(front!.port);
    process.env.AGENCY_CONTROL_TOKEN = token;

    expect(controlServer()).toEqual({ port: front!.port, token });

    const reading = new AbortController();
    const stream = await fetch(`http://127.0.0.1:${front!.port}/events?kinds=account-login-notice`, {
      headers: { authorization: `Bearer ${token}` },
      signal: reading.signal,
    });

    expect(stream.status).toBe(200);

    const notice = { account: "second", login: "here" as const, stands: "gone" as const, leftMs: null, notice: "Sign in again." };

    emitAgencyEvent("account-login-notice", notice);
    await agencyEventsRelayed();

    const reader = stream.body!.getReader();
    const decoder = new TextDecoder();
    let read = "";
    const until = Date.now() + 10_000;

    while (!read.includes("Sign in again.") && Date.now() < until) {
      const { done, value } = await reader.read();

      if (done) break;
      read += decoder.decode(value, { stream: true });
    }

    reading.abort();

    expect(read).toContain("event: account-login-notice");

    for (const [field, value] of Object.entries(notice)) {
      expect(read).toContain(`${JSON.stringify(field)}:${JSON.stringify(value)}`);
    }
  });
});
