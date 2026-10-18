import { expect, test } from "bun:test";
import { CONTROL_OUTLINE, CONTROL_PAINT, FRONT_CONTROLS, ICON_BOX, ICON_CENTRE, ICON_CONTROLS, ICON_CROSS_GAP, ICON_REACH, outerBox } from "../../apps/web/shared/bmo-controls";

// Every icon is sized by the box or the reach the controls fill, so both must be the controls' own
// outer edge, outline included, or an icon would cut a control or leave it short of where it is set.

const half = CONTROL_OUTLINE / 2;
const named = (name: string) => ICON_CONTROLS.find((control) => control.name === name)!;

test("the box is the icon's controls' outer edge: the cross's left arm and top, the green's right, the red's bottom", () => {
  const cross = named("cross");
  const green = named("green");
  const red = named("red");

  if (cross.kind !== "polygon" || green.kind !== "circle" || red.kind !== "circle") throw new Error("unexpected shapes");

  expect(ICON_BOX.x).toBeCloseTo(Math.min(...cross.points.map(([x]) => x)) - half, 3);
  expect(ICON_BOX.y).toBeCloseTo(Math.min(...cross.points.map(([, y]) => y)) - half, 3);
  expect(ICON_BOX.x + ICON_BOX.width).toBeCloseTo(green.cx + green.r + half, 3);
  expect(ICON_BOX.y + ICON_BOX.height).toBeCloseTo(red.cy + red.r + half, 3);
});

test("the reach holds every corner of the box the controls touch", () => {
  const cross = named("cross");

  if (cross.kind !== "polygon") throw new Error("unexpected shape");

  const corners = cross.points.map(([x, y]) => Math.hypot(x - ICON_CENTRE.x, y - ICON_CENTRE.y) + half);

  expect(ICON_REACH).toBeGreaterThanOrEqual(Math.max(...corners) - 0.01);
  expect(ICON_REACH).toBeLessThan(Math.hypot(ICON_BOX.width, ICON_BOX.height) / 2);
});

test("an icon draws the cross in beside the buttons at its own height, and moves nothing else", () => {
  const buttons = ICON_CONTROLS.filter((control) => control.name !== "cross");
  const front = FRONT_CONTROLS.find((control) => control.name === "cross")!;
  const icon = outerBox([named("cross")]);

  expect(buttons).toEqual(FRONT_CONTROLS.filter((control) => control.name !== "cross"));
  expect(outerBox(buttons).x - (icon.x + icon.width)).toBeCloseTo(ICON_CROSS_GAP, 3);
  expect(icon.y).toBeCloseTo(outerBox([front]).y, 6);
  expect(icon.width).toBeCloseTo(outerBox([front]).width, 6);
});

// The hue and lightness of a colour, from its hex.
function hueAndLightness(hex: string): { hue: number; lightness: number } {
  const [r, g, b] = [1, 3, 5].map((at) => Number.parseInt(hex.slice(at, at + 2), 16) / 255) as [number, number, number];
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const spread = max - min;
  const hue = spread === 0 ? 0 : max === r ? ((g - b) / spread + 6) % 6 : max === g ? (b - r) / spread + 2 : (r - g) / spread + 4;

  return { hue: hue * 60, lightness: (max + min) / 2 };
}

test.each(Object.entries(CONTROL_PAINT))("the %s's outline is a darker shade of its own colour, never black", (_, paint) => {
  const fill = hueAndLightness(paint.fill);
  const outline = hueAndLightness(paint.outline);
  const turn = Math.abs(fill.hue - outline.hue);

  expect(Math.min(turn, 360 - turn)).toBeLessThan(10);
  expect(outline.lightness).toBeLessThan(fill.lightness - 0.15);
  expect(outline.lightness).toBeGreaterThan(0.2);
});
