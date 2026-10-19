import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { chromium } from "playwright-core";

import { BROWSER_HOOK_MS, touchReady } from "./touch";

import type { Browser, BrowserContext, Page } from "playwright-core";
import type { LiveRun, StatusView } from "../../apps/web/shared/types";

// An agent's screen keeps its title bar whole while the agent runs (docs/web.md, 「Agent screen」):
// at a phone's width the name stays readable, the uptime and the actions fold behind two marks, and no
// control in the bar overlaps another; on a desktop the uptime and the stop stand in the bar as words.
// Each is measured as the browser lays it out.
//
// It drives the console given in BMO_CONSOLE_TEST_URL, the one tests/support/browser-console.ts stands up:
// the development server on a scratch database, with stand-ins for the units it reads and this file's fixture
// account made there (docs/checks.md, 「How the suite runs the tests」). The suite stands up no console, so
// there it skips.

const origin = process.env.BMO_CONSOLE_TEST_URL;
const ACCOUNT = { name: "agent-topbar", password: "fixture-password-1558" };
const AGENT = "worker";
// Two phones held upright — a Galaxy S25 and a wider one — and a desktop.
const WIDTHS = [
  { width: 360, height: 780, phone: true },
  { width: 390, height: 844, phone: true },
  { width: 1280, height: 800, phone: false }
];
// The uptime as it reads, hours, minutes and seconds.
const CLOCK = /\d{2}:\d{2}:\d{2}/;
// The bar's own square, which every mark in it keeps.
const MARK_PX = 32;

// The worker at work for twenty-five minutes, stoppable through its item.
const RUN: LiveRun = {
  id: 1,
  kind: "worker",
  agent: AGENT,
  chatId: null,
  messageId: null,
  startedAt: Math.floor(Date.now() / 1000) - 1_500,
  unit: "fixture-unit",
  capSeconds: null,
  depth: null,
  model: null,
  startedBy: null,
  todoId: 1,
  progress: null,
  stop: { kind: "item", id: "1" }
};

type Part = { name: string; control: boolean; left: number; right: number; width: number; height: number };
type Bar = { left: number; right: number; parts: Part[]; title: { scroll: number; client: number } };

let browser: Browser | null = null;
let cookie = "";

beforeAll(async () => {
  if (!origin || !touchReady()) {
    return;
  }

  const login = await fetch(`${origin}/api/auth/login`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(ACCOUNT)
  });

  expect(login.ok).toBe(true);
  cookie = login.headers.get("set-cookie")!.split(";")[0]!;
  browser = await chromium.launch();
}, BROWSER_HOOK_MS);

afterAll(async () => {
  await browser?.close();
}, BROWSER_HOOK_MS);

async function open(viewport: (typeof WIDTHS)[number]): Promise<BrowserContext> {
  const context = await browser!.newContext({
    viewport: { width: viewport.width, height: viewport.height },
    isMobile: viewport.phone,
    hasTouch: viewport.phone,
    serviceWorkers: "block",
    reducedMotion: "reduce"
  });
  const at = cookie.indexOf("=");

  await context.addCookies([{ name: cookie.slice(0, at), value: cookie.slice(at + 1), url: origin! }]);

  // The picture the screen asks for first carries the run; the stand-in bot sends no other.
  await context.route(/\/api\/status$/, async (route) => {
    const status = (await (await route.fetch()).json()) as StatusView;

    await route.fulfill({ json: { ...status, live: [RUN] } });
  });

  return context;
}

// Every part of the title bar a reader sees: its controls, its title and the marks standing for a
// state, outermost first, as the browser laid them out.
async function bar(page: Page): Promise<Bar> {
  return await page.evaluate(() => {
    const root = document.querySelector<HTMLElement>("[data-slot=root]:has(> [data-slot=left] > [data-slot=toggle])")!;
    const box = root.getBoundingClientRect();
    const title = root.querySelector<HTMLElement>("[data-slot=title]")!;
    const parts = [...root.querySelectorAll<HTMLElement>("button, a, [data-slot=title], [data-slot=icon], [data-slot=base]")]
      .filter((element) => element.getClientRects().length > 0)
      .filter((element, _, all) => !all.some((other) => other !== element && other.contains(element)))
      .map((element) => {
        const part = element.getBoundingClientRect();

        return {
          name: element.getAttribute("aria-label") ?? element.dataset.slot ?? element.tagName,
          control: element.matches("button, a"),
          left: part.left,
          right: part.right,
          width: part.width,
          height: part.height
        };
      });

    return { left: box.left, right: box.right, parts, title: { scroll: title.scrollWidth, client: title.clientWidth } };
  });
}

// Every pair of parts whose boxes cross, past half a pixel of rounding.
function overlaps(parts: readonly Part[]): string[] {
  return parts.flatMap((one, at) =>
    parts.slice(at + 1).filter((other) => one.left < other.right - 0.5 && other.left < one.right - 0.5).map((other) => `${one.name} / ${other.name}`)
  );
}

describe.skipIf(!origin || !touchReady())("an agent's title bar while it runs", () => {
  for (const viewport of WIDTHS) {
    test(`stands whole at ${viewport.width}px`, async () => {
      const context = await open(viewport);

      try {
        const page = await context.newPage();
        const root = page.locator("[data-slot=root]:has(> [data-slot=left] > [data-slot=toggle])").first();

        await page.goto(`${origin}/agents/${AGENT}`, { waitUntil: "load" });
        await page.waitForLoadState("networkidle", { timeout: 5_000 }).catch(() => undefined);

        // The run has reached the bar: the uptime as words on a desktop, the fold's menu on a phone.
        await (viewport.phone ? root.locator("[aria-haspopup=menu]") : root.getByText(CLOCK)).first().waitFor();

        const found = await bar(page);

        expect(overlaps(found.parts)).toEqual([]);
        expect(found.parts.filter((part) => part.left < found.left - 0.5 || part.right > found.right + 0.5).map((part) => part.name)).toEqual([]);
        expect(found.title.scroll).toBeLessThanOrEqual(found.title.client + 1);

        if (viewport.phone) {
          // Folded: no words of the run in the bar, and every control a whole square to press.
          expect(await root.getByText(CLOCK).filter({ visible: true }).count()).toBe(0);
          expect(found.parts.filter((part) => part.control && (part.width < MARK_PX || part.height < MARK_PX)).map((part) => part.name)).toEqual([]);
        } else {
          expect(await root.getByText(CLOCK).filter({ visible: true }).count()).toBe(1);
          expect(await root.locator("[aria-haspopup=menu]").filter({ visible: true }).count()).toBe(0);
        }
      } finally {
        await context.close();
      }
    }, 60_000);
  }
});
