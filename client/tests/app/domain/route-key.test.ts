import { describe, expect, test } from "bun:test";

import { screenKey } from "../../../apps/web/app/domain/route-key";

describe("which screen an address is", () => {
  test("another path, or another value of a part the screen reads, is another screen", () => {
    expect(screenKey({ path: "/rooms/7", query: {} }, ["at"])).not.toBe(screenKey({ path: "/rooms/8", query: {} }, ["at"]));
    expect(screenKey({ path: "/rooms/7", query: { at: "120" } }, ["at"])).not.toBe(screenKey({ path: "/rooms/7", query: {} }, ["at"]));
    expect(screenKey({ path: "/rooms/7", query: { at: "120" } }, ["at"])).not.toBe(screenKey({ path: "/rooms/7", query: { at: "121" } }, ["at"]));
  });

  test("a part the screen does not read leaves it the same screen", () => {
    const room = screenKey({ path: "/rooms/7", query: { at: "120" } }, ["at"]);

    expect(screenKey({ path: "/rooms/7", query: { at: "120", todo: "1425" } }, ["at"])).toBe(room);
    expect(screenKey({ path: "/gates/laya", query: { todo: "3" } }, [])).toBe("/gates/laya");
  });
});

// The router builds a screen anew whenever its key changes, and an item opened on the TODO panel
// writes itself into whatever address is open. So no screen is named by its whole address.
describe("the screens the console stands", () => {
  test("none is named to the router by its whole address", async () => {
    const pages = new Bun.Glob("**/*.vue");
    const named: string[] = [];

    for await (const file of pages.scan({ cwd: new URL("../../../apps/web/app/pages", import.meta.url).pathname })) {
      const source = await Bun.file(new URL(`../../../apps/web/app/pages/${file}`, import.meta.url)).text();

      if (/definePageMeta\([^)]*fullPath/.test(source)) {
        named.push(file);
      }
    }

    expect(named).toEqual([]);
  });

});
