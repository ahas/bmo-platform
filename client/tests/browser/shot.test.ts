import { statSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, test } from "bun:test";

import { chromium } from "playwright-core";

import { scratchDir } from "../support/scratch";
import { BROWSER_HOOK_MS, touchReady } from "./touch";
import { readCost, shot, shotPath, shotWidth } from "./shot";

import type { Browser, BrowserContext, Page } from "playwright-core";
import KO from "../fixtures/browser/shot.test.ko.json";

// That a shot comes back bounded. A worker's screenshots are the largest single thing its tools
// return — the 459 of them still on this machine average 186,473 bytes, which is 248,632
// characters once a session reads one back — and nearly all of those characters are resolution
// nobody was reading.
//
// Skipped where the browser build is not on the machine, so a checkout without the cache runs its
// tests and says nothing about shots rather than failing over a missing download.

// A screen wide enough to be over the ceiling, and dense enough that its two encodings differ:
// text, a gradient and a photograph's worth of noise.
const dense = `<style>
  body { margin: 0; font: 13px sans-serif; }
  .band { height: 300px; background: linear-gradient(90deg, #123, #f80, #0af, #8f0); }
  .grid { display: grid; grid-template-columns: repeat(8, 1fr); }
  .cell { padding: 6px; border: 1px solid #999; }
</style>
<div class="band"></div>
<div class="grid">${Array.from({ length: 240 }, (unused, at) => `${KO.line_30}${at} value ${at * 7919}</div>`).join("")}</div>`;

// And a flat one: a handful of solid colours with hard edges and nothing shaded. This is the
// shape that is smaller as PNG than as JPEG — a run of identical pixels is a few bytes to PNG and
// a block to encode for JPEG — and it is why a shot is encoded both ways rather than always as
// the JPEG.
const flat = `<style>body { margin: 0 } div { height: 160px }</style>
${["#fff", "#000", "#fff", "#000", "#fff"].map((colour) => `<div style="background:${colour}"></div>`).join("")}`;

const wide = { width: 2560, height: 900 };
let browser: Browser;
let context: BrowserContext;
let page: Page;
// Asked for here rather than in beforeAll, whose scope closes before the tests that read it.
const out = scratchDir("bmo-shot-");

beforeAll(async () => {
  if (!touchReady()) {
    return;
  }

  browser = await chromium.launch();
  context = await browser.newContext({ viewport: wide });
  page = await context.newPage();
}, BROWSER_HOOK_MS);

afterAll(async () => {
  await browser?.close();
}, BROWSER_HOOK_MS);

describe("the name a shot is written under", () => {
  test("says which encoding it holds, whatever the caller asked for", () => {
    expect(shotPath("/tmp/one.png", "jpeg")).toBe("/tmp/one.jpg");
    expect(shotPath("/tmp/one.png", "png")).toBe("/tmp/one.png");
    expect(shotPath("/tmp/one.jpg", "png")).toBe("/tmp/one.png");
    expect(shotPath("/tmp/one", "jpeg")).toBe("/tmp/one.jpg");
    // A dot inside the name is not an extension, so it survives.
    expect(shotPath("/tmp/622-s25.wide", "jpeg")).toBe("/tmp/622-s25.wide.jpg");
  });
});

describe("what a Read of a shot costs", () => {
  test("is the file as base64, which is four characters for every three bytes", () => {
    expect(readCost(3)).toBe(4);
    expect(readCost(186_473)).toBe(248_632);
  });
});

describe.skipIf(!touchReady())("a shot of a screen wider than the ceiling", () => {
  test("comes back scaled to the ceiling, not at the width it was taken at", async () => {
    await page.setViewportSize(wide);
    await page.setContent(dense);

    const took = await shot(page, join(out, "wide.png"));

    expect(took.width).toBe(shotWidth);
    expect(took.scale).toBeCloseTo(shotWidth / wide.width, 3);
    // And the file on disk is the file the answer named.
    expect(statSync(took.path).size).toBe(took.bytes);
  });

  test("and a screen already inside the ceiling is left at its own size", async () => {
    await page.setViewportSize({ width: 900, height: 600 });
    await page.setContent(dense);

    const took = await shot(page, join(out, "narrow.png"));

    // The ceiling is a bound, not a size: nothing is scaled up to meet it.
    expect(took.scale).toBe(1);
    expect(took.width).toBe(900);
  });

  test("the ceiling can be asked for narrower, for a check that needs less", async () => {
    await page.setViewportSize(wide);
    await page.setContent(dense);

    const took = await shot(page, join(out, "narrower.png"), { width: 640 });

    expect(took.width).toBe(640);
  });

  test("scaling to the ceiling is most of what it saves", async () => {
    await page.setViewportSize(wide);
    await page.setContent(dense);

    const bounded = await shot(page, join(out, "bounded.png"));
    const whole = await shot(page, join(out, "whole.png"), { width: wide.width });

    expect(bounded.bytes).toBeLessThan(whole.bytes / 2);
  });
});

describe.skipIf(!touchReady())("which encoding a shot is kept in", () => {
  test("a dense screen is kept as the JPEG, which is far smaller for one", async () => {
    await page.setViewportSize(wide);
    await page.setContent(dense);

    expect((await shot(page, join(out, "dense.png"))).format).toBe("jpeg");
  });

  // Without this a helper meant to make screenshots cheaper made some of them dearer: over the
  // 459 real screenshots on this machine, five of a sample of 115 came back larger as JPEG than
  // they had been as PNG, one of them by three quarters.
  test("a flat screen is kept as the PNG, because for one that is the smaller file", async () => {
    await page.setViewportSize({ width: 1000, height: 800 });
    await page.setContent(flat);

    expect((await shot(page, join(out, "flat.png"))).format).toBe("png");
  });

  test("so whichever it keeps is the smaller of the two, whichever that turns out to be", async () => {
    await page.setViewportSize(wide);
    await page.setContent(dense);

    const heavy = await shot(page, join(out, "kept-dense.png"));

    await page.setViewportSize({ width: 1000, height: 800 });
    await page.setContent(flat);

    const plain = await shot(page, join(out, "kept-flat.png"));

    for (const took of [heavy, plain]) {
      expect(took.bytes).toBe(Math.min(took.considered.jpeg, took.considered.png));
      expect(took.bytes).toBe(took.considered[took.format]);
      // And the file it wrote is that many bytes, under the extension of what it kept.
      expect(statSync(took.path).size).toBe(took.bytes);
    }

    // The two fixtures disagree about which encoding wins, which is the whole reason for the choice.
    expect(heavy.format).not.toBe(plain.format);
  });
});

// A room on a phone: a scroller keeping its gutter, as the console's room does, and a sent bubble
// whose width is a share of it. A phone draws its scrollbars over the content, so the gutter takes
// nothing; a page that has lost its phone emulation gets the desktop bar, and the bubble narrows.
const room = `<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
  body { margin: 0; font: 15px sans-serif; }
  .scroll { height: 100vh; overflow-y: auto; scrollbar-gutter: stable; scrollbar-width: thin; }
  .sent { max-width: 70%; margin: 8px 8px 8px auto; padding: 8px; background: #cde; }
</style>
<div class="scroll">${Array.from({ length: 40 }, (unused, at) => `<div class="sent">${at}${KO.line_172}`).join("")}</div>`;

describe.skipIf(!touchReady())("a shot of a phone screen", () => {
  test("leaves the page laid out as it found it", async () => {
    const phone = await browser.newContext({
      viewport: { width: 360, height: 640 },
      deviceScaleFactor: 3,
      isMobile: true,
      hasTouch: true,
    });

    try {
      const held = await phone.newPage();

      await held.setContent(room);

      const bubble = () => held.locator(".sent").first().evaluate((sent) => sent.getBoundingClientRect().width);
      const before = await bubble();
      const took = await shot(held, join(out, "phone.png"));

      expect(await bubble()).toBe(before);
      // And the picture is the phone's width, one pixel to a CSS pixel.
      expect(took.width).toBe(360);
    } finally {
      await phone.close();
    }
  });
});

describe.skipIf(!touchReady())("the whole page against what is on screen", () => {
  test("a full-page shot takes the document, and is the dearer of the two", async () => {
    await page.setViewportSize(wide);
    await page.setContent(dense);

    const seen = await shot(page, join(out, "seen.png"));
    const all = await shot(page, join(out, "all.png"), { fullPage: true });

    expect(all.height).toBeGreaterThan(seen.height);
    expect(all.bytes).toBeGreaterThan(seen.bytes);
    // Both are still bounded on their width; the page being long is not licence to be wide.
    expect(all.width).toBe(shotWidth);
  });
});
