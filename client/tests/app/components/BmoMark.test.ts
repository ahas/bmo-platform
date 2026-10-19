import { describe, expect, test } from "bun:test";
import { readdirSync } from "node:fs";
import { BMO_MINT, CONTROL_OUTLINE, CONTROL_PAINT, controlElements, ICON_CONTROLS, roundedIcon, SMALL_ICON_FILL } from "../../../apps/web/shared/bmo-controls";

// One picture stands on a tab and at the top of the sidebar, and the notification carries its
// silhouette; what makes them read as one mark rather than several is that all are drawn from the
// same control shapes, colours and frame. Nothing on any screen would ever say they had drifted, so the
// agreement is checked here instead, off the files that ship.

const mark = await Bun.file(new URL("../../../apps/web/app/components/BmoMark.vue", import.meta.url)).text();
const icons = new URL("../../../apps/web/public/icons/", import.meta.url);

describe("the small picture a notification carries", () => {
  test("is the controls' silhouette, every control filled with its outline", async () => {
    const badge = await Bun.file(new URL("badge.svg", icons)).text();

    expect(badge).toContain('viewBox="0 0 24 24"');
    expect(badge).toContain(controlElements(ICON_CONTROLS, () => `fill="#ffffff" stroke="#ffffff" stroke-width="${CONTROL_OUTLINE}"`));
  });
});

describe("the mark the console wears", () => {
  test("is drawn from the same control shapes", () => {
    expect(mark).toContain('from "#shared/bmo-controls"');
    expect(mark).toContain('v-for="control in ICON_CONTROLS"');
  });

  test("is the tab's icon: the same rounded frame and colours, none of its own", () => {
    expect(mark).toContain("roundedIcon(SIDE, SMALL_ICON_FILL)");
    expect(mark).toContain(':fill="CONTROL_PAINT[control.name].fill"');
    expect(mark).toContain(':stroke="CONTROL_PAINT[control.name].outline"');
    expect(mark).toContain(':fill="BMO_MINT"');
    // A colour written into the mark would be one the tab's icon could drift from.
    expect(mark).not.toMatch(/#[0-9a-f]{3,8}\b/i);
  });

  test("whose file the tab shows is drawn in that frame and those colours", async () => {
    const tab = await Bun.file(new URL("favicon.svg", icons)).text();
    const frame = roundedIcon(32, SMALL_ICON_FILL);

    expect(tab).toContain(`rx="${frame.corner}" fill="${BMO_MINT}"`);
    expect(tab).toContain(`<g transform="${frame.transform}">`);

    for (const paint of Object.values(CONTROL_PAINT)) expect(tab).toContain(`fill="${paint.fill}" stroke="${paint.outline}"`);
  });

  test("carries nothing a reader is told to read: the name beside it is what says which console this is", () => {
    expect(mark).toContain('aria-hidden="true"');
  });
});

// A shape rendered with smoothing turned off comes out in steps at an icon's size.
describe("every picture of the mascot is smoothed", () => {
  const unsmoothed = /crispEdges|optimizeSpeed/;

  test("the mark", () => {
    expect(mark).not.toMatch(unsmoothed);
  });

  test.each(readdirSync(icons).filter((name) => name.endsWith(".svg")))("%s", async (name) => {
    expect(await Bun.file(new URL(name, icons)).text()).not.toMatch(unsmoothed);
  });
});
