import { afterEach, describe, expect, test } from "bun:test";

import { askWhetherLoginEnded, holdLoginTab, loginRoute, returnPath } from "../../../apps/web/app/domain/login-end";

import type { LoginTab } from "../../../apps/web/app/domain/login-end";
import type { RouteLocationRaw } from "vue-router";

describe("the road to the login screen and back", () => {
  test("carries the screen asked for, and home carries nothing", () => {
    expect(loginRoute("/rooms/-100?at=5", false)).toEqual({ path: "/login", query: { to: "/rooms/-100?at=5" } });
    expect(loginRoute("/", true)).toEqual({ path: "/login", query: { ended: "1" } });
  });

  test("returns only to this app's own screens", () => {
    expect(returnPath("/rooms/-100")).toBe("/rooms/-100");

    for (const away of [undefined, ["/rooms"], "rooms", "//evil.example/x", "/\\evil.example", "https://evil.example/", "/login", "/login?to=/x"]) {
      expect(returnPath(away)).toBe("/");
    }
  });
});

describe("a refusal for the login", () => {
  afterEach(() => holdLoginTab(null));

  function tabWith(answer: boolean | null, signedIn = true): { tab: LoginTab; went: RouteLocationRaw[]; asks: () => number; forgotten: () => boolean } {
    const went: RouteLocationRaw[] = [];
    let asks = 0;
    let held = signedIn;

    const tab: LoginTab = {
      signedIn: () => held,
      asked: async () => {
        asks += 1;

        return answer;
      },
      forget: () => {
        held = false;
      },
      here: () => "/rooms/-100",
      go: (to) => went.push(to)
    };

    return { tab, went, asks: () => asks, forgotten: () => !held };
  }

  test("a session gone sends the tab to the login screen, saying so, once for a burst", async () => {
    const { tab, went, asks, forgotten } = tabWith(false);

    holdLoginTab(tab);
    await Promise.all([askWhetherLoginEnded(), askWhetherLoginEnded(), askWhetherLoginEnded()]);

    expect(asks()).toBe(1);
    expect(forgotten()).toBe(true);
    expect(went).toEqual([{ path: "/login", query: { to: "/rooms/-100", ended: "1" } }]);
  });

  test("a session standing, or a server that did not say, leaves the tab where it is", async () => {
    for (const answer of [true, null]) {
      const { tab, went, forgotten } = tabWith(answer);

      holdLoginTab(tab);
      await askWhetherLoginEnded();

      expect(went).toEqual([]);
      expect(forgotten()).toBe(false);
    }
  });

  test("a tab holding no login asks nothing", async () => {
    const { tab, asks } = tabWith(false, false);

    holdLoginTab(tab);
    await askWhetherLoginEnded();

    expect(asks()).toBe(0);
  });
});
