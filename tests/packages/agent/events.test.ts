import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { agencyEventsSince, emitAgencyEvent, subscribeAgencyEvents } from "@agency/agent/events";

const notice = { account: "second", login: "here", stands: "ending", leftMs: 60_000, notice: "sign in again" } as const;

describe("agency's events", () => {
  test("reach every listener until it unsubscribes", () => {
    const seen: string[] = [];
    const unsubscribe = subscribeAgencyEvents((event) => {
      seen.push(`${event.name}:${event.body.account}`);
    });
    const told = emitAgencyEvent("account-login-notice", notice);

    unsubscribe();
    emitAgencyEvent("account-login-notice", { ...notice, account: "third" });

    expect(seen).toEqual(["account-login-notice:second"]);
    expect(told.at).toBeGreaterThan(0);
  });

  test("a listener that attaches late picks up from the id it last saw", () => {
    const first = emitAgencyEvent("account-login-notice", notice);
    const second = emitAgencyEvent("account-login-notice", { ...notice, account: "late" });

    expect(agencyEventsSince(first.id).map((event) => event.id)).toEqual([second.id]);
  });

  test("a listener that throws keeps none of the others from hearing", () => {
    const heard: number[] = [];
    const quiet = subscribeAgencyEvents(() => {
      throw new Error("a listener that breaks");
    });
    const kept = subscribeAgencyEvents((event) => heard.push(event.id));
    const told = emitAgencyEvent("account-login-notice", notice);

    quiet();
    kept();

    expect(heard).toEqual([told.id]);
  });
});
