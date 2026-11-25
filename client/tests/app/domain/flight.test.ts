import { GlobalRegistrator } from "@happy-dom/global-registrator";

GlobalRegistrator.register();

import { afterAll, describe, expect, test } from "bun:test";

import { bezier, easing, flightLayer, riseFrame, riseOutOf, riseStart, seconds, settleInto, settleStart } from "../../../apps/web/app/domain/flight";
import KO from "../../fixtures/app/domain/flight.test.ko.json";

afterAll(async () => {
  await GlobalRegistrator.unregister();
});

// The rise reads the page's own motion tokens rather than numbers of its own.
describe("the rise out of the input bar", () => {
  test("reads a length and an easing as the stylesheet writes them", () => {
    expect(seconds(" 320ms")).toBe(0.32);
    expect(seconds("0.2s")).toBe(0.2);
    expect(seconds("")).toBeNull();
    expect(bezier("cubic-bezier(0.2, 0.8, 0.2, 1)")).toEqual([0.2, 0.8, 0.2, 1]);
    expect(bezier("ease-in")).toBeNull();
  });

  test("eases as the stylesheet's curve does, from nothing to the whole way", () => {
    const enter = easing([0.2, 0.8, 0.2, 1]);
    const linear = easing([0, 0, 1, 1]);

    expect(enter(0)).toBe(0);
    expect(enter(1)).toBe(1);
    // An arrival comes at speed and settles: most of the way is covered in the first half.
    expect(enter(0.5)).toBeGreaterThan(0.85);
    expect(linear(0.3)).toBeCloseTo(0.3, 3);
  });

  test("grows the ground from the input box to the bubble, and only moves the contents", () => {
    const start = { box: { left: 0, top: 800, width: 700, height: 60 }, origin: { x: 40, y: 812 } };
    const bubble = { left: 300, top: 500, width: 300, height: 80 };
    const inset = { x: 12, y: 8 };

    expect(riseFrame(start, bubble, inset, 0)).toEqual({ ground: start.box, contents: { x: 28, y: 804 } });
    expect(riseFrame(start, bubble, inset, 1)).toEqual({ ground: bubble, contents: { x: 300, y: 500 } });

    // Half way the ground is half way in every measure, and the contents carry no size at all: a
    // frame says where they stand, never how large.
    const half = riseFrame(start, bubble, inset, 0.5);

    expect(half.ground).toEqual({ left: 150, top: 650, width: 500, height: 70 });
    expect(Object.keys(half.contents)).toEqual(["x", "y"]);
  });

  test("a sent bubble starts at its own size where its first line was typed, and only moves", () => {
    const bubble = { left: 400, top: 500, width: 300, height: 120 };
    const inset = { x: 12, y: 8 };
    const start = riseStart({ x: 40, y: 812 }, bubble, inset);

    expect(start.box).toEqual({ left: 28, top: 804, width: 300, height: 120 });

    // Every frame keeps the bubble's size, and the ground and the words stand together.
    for (const come of [0, 0.3, 0.7, 1]) {
      const drawn = riseFrame(start, bubble, inset, come);

      expect([drawn.ground.width, drawn.ground.height]).toEqual([300, 120]);
      expect(drawn.contents).toEqual({ x: drawn.ground.left, y: drawn.ground.top });
    }
  });
});

// An answer settles out of the box the turn's words stood in, and that box rides with the list.
describe("the settle into a turn's place", () => {
  test("starts from where the turn stood, its words at their own place in it, and follows the list", () => {
    const from = { left: 56, top: 615, width: 238, height: 88 };
    const first = { left: 56, top: 615, width: 238, height: 112 };
    const inset = { x: 12, y: 8 };

    expect(settleStart(from, first, first, inset)).toEqual({ box: from, origin: { x: 68, y: 623 } });
    // The list glides up by 24px: the box goes with it, keeping its own size.
    expect(settleStart(from, first, { ...first, top: 591 }, inset)).toEqual({ box: { ...from, top: 591 }, origin: { x: 68, y: 599 } });
  });
});

// The archived message takes the place of the one on its way while it may still be flying: the rise
// goes on to the bubble standing in its place, hidden under it as the first was, and lands there.
describe("a rise whose bubble is taken over", () => {
  test("goes on to the bubble that took its place, and ends where none did", async () => {
    document.body.innerHTML = KO.line_84;
    const layer = document.querySelector<HTMLElement>("[data-flight-layer]")!;
    const frame = (): Promise<number> => new Promise((done) => requestAnimationFrame(done));
    const flying = riseOutOf(document.querySelector<HTMLElement>("[data-bubble]")!, { x: 0, y: 600 }, { duration: 1e7, ease: [0, 0, 1, 1] }, () =>
      document.querySelector<HTMLElement>('[data-row="m9"] [data-bubble]')
    );

    layer.querySelector('[data-row="out1"]')!.remove();
    layer.insertAdjacentHTML("afterbegin", KO.line_92);
    const landing = layer.querySelector<HTMLElement>('[data-row="m9"] [data-bubble]')!;
    await frame();

    expect(landing.style.visibility).toBe("hidden");
    expect(document.querySelectorAll("[data-rise-ground], [data-rise-contents]")).toHaveLength(2);

    landing.parentElement!.remove();
    await flying;

    expect(document.querySelectorAll("[data-rise-ground], [data-rise-contents]")).toHaveLength(0);
    expect(landing.style.visibility).toBe("");
  });
});

// The conversation marks its own layer, an isolated stacking context under everything laid over it,
// and a flight is drawn there with no number of its own; the flight-layer browser test reads the
// paint order that gives.
describe("the layer a flight is drawn in", () => {
  test("is the list's marked layer, carries no number, and the flight leaves nothing behind", async () => {
    document.body.innerHTML = KO.line_112;
    const list = document.querySelector<HTMLElement>("[data-flight-layer]")!;
    const bubble = document.querySelector<HTMLElement>("[data-bubble]")!;

    expect(flightLayer(bubble)).toBe(list);

    const flying = settleInto(bubble, { left: 0, top: 300, width: 240, height: 120 }, { duration: 60, ease: [0, 0, 1, 1] });
    const layers = [...document.querySelectorAll<HTMLElement>("[data-rise-ground], [data-rise-contents]")];

    expect(layers).toHaveLength(2);
    expect(layers.every((layer) => layer.parentElement === list && layer.style.zIndex === "")).toBe(true);

    // A bubble gone from the page ends its flight at the next frame.
    bubble.remove();
    await flying;

    expect(document.querySelectorAll("[data-rise-ground], [data-rise-contents]")).toHaveLength(0);
  });

  test("is the place the layer keeps for it where it keeps one, after every row", () => {
    document.body.innerHTML = "<div data-flight-layer><div data-list><div data-bubble></div></div><div data-flight-host></div></div>";

    expect(flightLayer(document.querySelector<HTMLElement>("[data-bubble]")!)).toBe(document.querySelector<HTMLElement>("[data-flight-host]")!);
  });

  test("is the body for a bubble in no marked list", () => {
    document.body.innerHTML = '<div id="__nuxt"><div data-bubble></div></div>';

    expect(flightLayer(document.querySelector<HTMLElement>("[data-bubble]")!)).toBe(document.body);
  });
});
