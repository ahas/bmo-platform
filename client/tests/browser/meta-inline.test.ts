import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { chromium } from "playwright-core";

import { BROWSER_HOOK_MS, touchReady } from "./touch";

import type { Browser, Page } from "playwright-core";
import KO from "../fixtures/browser/meta-inline.test.ko.json";

// A meta line carrying only its time and marks stands at the end of the words' last line where that
// line leaves it room, as Telegram lays it, and on a line of its own only where it does not (docs/web.md,
// the room screen's meta line section). What decides it is the blank the words keep at their end, which the room's own rules draw:
// they are read out of the component as it ships, laid on a bubble built to the conversation's
// measures, and the browser says where the line and the words ended up.

const source = await Bun.file(new URL("../../apps/web/app/components/RoomMessage.vue", import.meta.url)).text();
// The component's unscoped rules, which are the blank's.
const rules = source.match(/<style>([\s\S]*?)<\/style>/)?.[1] ?? "";

// A bubble at the conversation's measures: 12px sides, 8px foot, a 16px text on a 24px line, and the
// line laid at the trailing foot as the room lays it, 52px wide.
const ROOM = 52;
const bubbleOf = (words: string) => `<!doctype html><style>
    :root { --spacing: 0.25rem; --chat-meta-gap: 4px; --chat-meta-line: 20px; }
    body { margin: 0; font: 16px/24px monospace; }
    .bubble { display: inline-block; max-width: 240px; }
    .body { position: relative; display: flex; flex-wrap: wrap; align-items: flex-end; padding: 8px 12px; }
    [data-words] { min-width: 0; max-width: 100%; flex-grow: 1; }
    p { margin: 0; white-space: pre-wrap; } ul { margin: 0; padding-left: 20px; }
    [data-meta] { position: absolute; right: 12px; bottom: 8px; width: ${ROOM}px; height: 20px; }
    ${rules}
  </style>
  <div class="bubble"><div class="body" data-meta-room style="--chat-meta-room: ${ROOM}px"><div data-words>${words}</div><span data-meta></span></div></div>`;

interface Laid {
  bubble: { width: number; height: number };
  words: number;
  wordsBottom: number;
  metaTop: number;
  metaBottom: number;
  lastLineBottom: number;
  // Whether the line covers any of the words.
  covers: boolean;
}

async function laid(words: string): Promise<Laid> {
  await page.setContent(bubbleOf(words));

  return page.evaluate(() => {
    const box = (selector: string) => document.querySelector(selector)!.getBoundingClientRect();
    // The lines the words' own letters stand on, read off their text alone.
    const walker = document.createTreeWalker(document.querySelector("[data-words]")!, NodeFilter.SHOW_TEXT);
    const lines: DOMRect[] = [];

    for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
      const range = document.createRange();

      range.selectNodeContents(node);
      lines.push(...range.getClientRects());
    }

    return {
      bubble: { width: Math.round(box(".bubble").width), height: Math.round(box(".bubble").height) },
      words: Math.round(box("[data-words]").height),
      wordsBottom: Math.round(box("[data-words]").bottom),
      metaTop: Math.round(box("[data-meta]").top),
      metaBottom: Math.round(box("[data-meta]").bottom),
      lastLineBottom: Math.round(Math.max(...lines.map((line) => line.bottom))),
      covers: lines.some((line) => {
        const meta = box("[data-meta]");

        return line.bottom > meta.top && line.top < meta.bottom && line.right > meta.left && line.left < meta.right;
      })
    };
  });
}

const ready = touchReady();
let browser: Browser | null = null;
let page: Page;

beforeAll(async () => {
  if (!ready) {
    return;
  }

  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 400, height: 400 } });
}, BROWSER_HOOK_MS);

afterAll(async () => {
  await browser?.close();
}, BROWSER_HOOK_MS);

describe("the blank's rules", () => {
  test("stand unscoped in the room's message, keyed on the room the line asks for", () => {
    expect(rules).toContain("[data-meta-room] > [data-words]");
    expect(rules).toContain("var(--chat-meta-room)");
  });
});

describe.skipIf(!ready)("a meta line of its time and marks alone", () => {
  test("stands at the end of a one-word message's line, so the bubble is one line tall", async () => {
    const one = await laid(KO.line_102);

    expect(one.words).toBe(24);
    expect(one.bubble.height).toBe(8 + 24 + 8);
    expect(one.metaBottom).toBe(one.wordsBottom);
    expect(one.metaTop).toBeLessThan(one.lastLineBottom);
    expect(one.covers).toBe(false);
    // As wide as the words and the line beside them, and no wider.
    expect(one.bubble.width).toBeLessThan(240);
  });

  test("stands at the end of a longer message's last line where that line leaves it room", async () => {
    // A first line of one long word, and a last line of a short one.
    const some = await laid(`<p>${"x".repeat(21)} yy</p>`);

    expect(some.words).toBe(48);
    expect(some.metaBottom).toBe(some.wordsBottom);
    expect(some.metaTop).toBeLessThan(some.lastLineBottom);
    expect(some.covers).toBe(false);
  });

  test("drops to a line of its own where the last line leaves it none", async () => {
    // The most short words one line holds without the blank: one more would wrap.
    await page.setContent(bubbleOf("<p>ab</p>").replace("data-meta-room ", ""));

    const count = await page.evaluate(() => {
      const words = document.querySelector("p")!;
      let held = 1;

      while (held < 100) {
        words.textContent = Array(held + 1).fill("ab").join(" ");

        if (words.getBoundingClientRect().height > 24) {
          break;
        }

        held++;
      }

      return held;
    });

    const full = await laid(`<p>${Array(count).fill("ab").join(" ")}</p>`);

    expect(full.words).toBe(48);
    expect(full.covers).toBe(false);
  });

  test("keeps a line of its own under words that end in a list", async () => {
    const listed = await laid('<div class="markdown"><ul><li>a</li></ul></div>');

    expect(listed.words).toBe(24 + 4 + 20);
    expect(listed.metaTop).toBeGreaterThanOrEqual(listed.lastLineBottom);
    expect(listed.covers).toBe(false);
  });
});
