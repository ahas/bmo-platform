import { afterAll, beforeAll, describe, expect, test } from "bun:test";

import { BROWSER_HOOK_MS, centreOf, openTouch, touchReady } from "./touch";

// A chip under a message, tapped by a real finger while the input box holds the focus a send left
// in it (docs/web.md, the room screen's chip line section). The row turns a finger's press away from the browser, as a message
// row does, so the focus the press would have moved stays where it was unless the tap lets it go;
// a phone raises its keyboard on every tap while a text box holds it. Only a real touch shows it:
// the press a dispatched event or an emulated click makes is a mouse's, which the row leaves alone.
//
// Skipped where the browser build is not on the machine, as every touch check is.

const ready = touchReady();

let script = "";
let touch: Awaited<ReturnType<typeof openTouch>>;

async function mount(): Promise<void> {
  const { page } = touch;

  await page.setContent(`<meta name="viewport" content="width=device-width,initial-scale=1">
    <style>body { margin: 0; font: 16px sans-serif; } button, textarea { display: block; margin: 40px; width: 200px; height: 48px; }</style>
    <div id="row" tabindex="-1"><p>an answer</p><button id="chip" type="button" aria-expanded="false">chip</button></div>
    <textarea id="box"></textarea>`);
  await page.addScriptTag({
    type: "module",
    content: `${script}
      const row = document.getElementById("row");
      const chip = document.getElementById("chip");
      let touched = false;
      row.addEventListener("pointerdown", (event) => {
        touched = event.pointerType === "touch";
        if (touched) event.preventDefault();
      });
      row.addEventListener("click", (event) => {
        if (touched && event.target.closest("button")) releaseFocusOutside(row);
      });
      chip.addEventListener("click", () => chip.setAttribute("aria-expanded", String(chip.getAttribute("aria-expanded") !== "true")));
      window.mounted = true;`
  });
  await page.waitForFunction(() => "mounted" in window);
}

async function focused(): Promise<string> {
  return await touch.page.evaluate(() => document.activeElement?.id || document.activeElement?.tagName || "");
}

describe.skipIf(!ready)("a finger's tap on a chip", () => {
  beforeAll(async () => {
    script = (await (await Bun.build({ entrypoints: [new URL("../../apps/web/app/domain/tapfocus.ts", import.meta.url).pathname], target: "browser" })).outputs[0]?.text()) ?? "";
    touch = await openTouch({ reducedMotion: "reduce" });
  }, BROWSER_HOOK_MS);

  afterAll(async () => {
    await touch?.close();
  }, BROWSER_HOOK_MS);

  test("opens the chip and leaves the input box without the focus, which a tap on the box takes back", async () => {
    await mount();

    await touch.tap(await centreOf(touch.page, "#box"));
    expect(await focused()).toBe("box");

    await touch.tap(await centreOf(touch.page, "#chip"));
    expect(await touch.page.getAttribute("#chip", "aria-expanded")).toBe("true");
    expect(await focused()).not.toBe("box");

    await touch.tap(await centreOf(touch.page, "#box"));
    expect(await focused()).toBe("box");
  }, 60_000);
});
