// The mascot's controls, the shapes every icon of the mascot is drawn from (docs/web.md, 「The console's
// pictures」): the yellow cross pad at the left, and at the right the blue triangle, the small green
// button at its lower right and the large red one under both, as they stand on the mascot's front, in units
// of the front's own drawing, 266 wide and 350 tall. They are vector shapes and nothing else, so
// whatever draws them smooths their edges at the size it shows them.

export type ControlName = "cross" | "triangle" | "green" | "red";

export type ControlShape =
  | { name: "cross" | "triangle"; kind: "polygon"; points: ReadonlyArray<readonly [number, number]> }
  | { name: "green" | "red"; kind: "circle"; cx: number; cy: number; r: number };

// The outline each control wears; half of it stands outside the shape.
export const CONTROL_OUTLINE = 3.5;

// What each control is painted in: its own colour, and its outline in a darker shade of that colour,
// the same hue at a lower lightness, so the outline sets it off from the mint without a black edge.
export const CONTROL_PAINT: Readonly<Record<ControlName, { fill: string; outline: string }>> = {
  cross: { fill: "#f9e000", outline: "#a68800" },
  triangle: { fill: "#6dcdf7", outline: "#086791" },
  green: { fill: "#5db851", outline: "#346d2c" },
  red: { fill: "#ef3538", outline: "#a50d10" }
};

// The mascot's body mint, the colour the face project samples off the official design's pictures
// (the face tool's docs/expressions.md, 「The colours」), which the controls stand on.
export const BMO_MINT = "#5fbb9c";

// A plus sign `extent` across with arms `arm` thick, centred on (cx, cy).
function cross(cx: number, cy: number, extent: number, arm: number): Array<readonly [number, number]> {
  const e = extent / 2;
  const a = arm / 2;

  return [
    [cx - a, cy - e], [cx + a, cy - e], [cx + a, cy - a], [cx + e, cy - a], [cx + e, cy + a], [cx + a, cy + a],
    [cx + a, cy + e], [cx - a, cy + e], [cx - a, cy + a], [cx - e, cy + a], [cx - e, cy - a], [cx - a, cy - a]
  ];
}

// The controls where they stand on the mascot's front.
export const FRONT_CONTROLS: readonly ControlShape[] = [
  { name: "cross", kind: "polygon", points: cross(70, 247.25, 54, 18) },
  { name: "triangle", kind: "polygon", points: [[181, 240.25], [164, 266.25], [198, 266.25]] },
  { name: "green", kind: "circle", cx: 222, cy: 268.25, r: 10 },
  { name: "red", kind: "circle", cx: 187, cy: 298.25, r: 19 }
];

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

// Points far enough round each control's outer edge, outline included, to find its extremes: a
// circle's rim, and round each corner of a polygon the arc its rounded join draws.
function outerEdge(controls: readonly ControlShape[]): Array<readonly [number, number]> {
  const half = CONTROL_OUTLINE / 2;
  const round = (cx: number, cy: number, r: number) =>
    Array.from({ length: 360 }, (_, step) => [cx + r * Math.cos((step * Math.PI) / 180), cy + r * Math.sin((step * Math.PI) / 180)] as const);

  return controls.flatMap((control) =>
    control.kind === "circle" ? round(control.cx, control.cy, control.r + half) : control.points.flatMap(([x, y]) => round(x, y, half))
  );
}

// The box the controls fill, measured to the outer edge of their outlines.
export function outerBox(controls: readonly ControlShape[]): Box {
  const edge = outerEdge(controls);
  const xs = edge.map(([x]) => x);
  const ys = edge.map(([, y]) => y);
  const [x, y] = [Math.min(...xs), Math.min(...ys)];

  return { x, y, width: Math.max(...xs) - x, height: Math.max(...ys) - y };
}

// How far an icon keeps the cross from the buttons, outline to outline: the front's gap between them
// is where the bars and the slot stand, and an icon carries none of those, so it draws the cross in
// to stand beside the buttons, at the height it has on the front.
export const ICON_CROSS_GAP = 14;

function iconControls(): readonly ControlShape[] {
  const buttons = FRONT_CONTROLS.filter((control) => control.name !== "cross");
  const crossBox = outerBox(FRONT_CONTROLS.filter((control) => control.name === "cross"));
  const shift = outerBox(buttons).x - ICON_CROSS_GAP - (crossBox.x + crossBox.width);

  return FRONT_CONTROLS.map((control) =>
    control.kind === "polygon" && control.name === "cross" ? { ...control, points: control.points.map(([x, y]) => [x + shift, y] as const) } : control
  );
}

// The controls as every icon carries them.
export const ICON_CONTROLS = iconControls();

// The box the icon's controls fill, and its centre.
export const ICON_BOX = outerBox(ICON_CONTROLS);
export const ICON_CENTRE = { x: ICON_BOX.x + ICON_BOX.width / 2, y: ICON_BOX.y + ICON_BOX.height / 2 };

// How far the icon's controls reach from that centre: what a picture a platform cuts to a circle sizes
// them by.
export const ICON_REACH = Math.max(...outerEdge(ICON_CONTROLS).map(([x, y]) => Math.hypot(x - ICON_CENTRE.x, y - ICON_CENTRE.y)));

// The transform that sets the controls into a square of `side` units, scaled by `scale` with the given
// point of theirs on the square's centre.
export function centredOn(side: number, scale: number, at: { x: number; y: number }): string {
  const offset = (point: number) => side / 2 - scale * point;

  return `translate(${offset(at.x)} ${offset(at.y)}) scale(${scale})`;
}

// The transform that sets the icon's controls on a square of `side` units, their wider extent filling
// the given share of it.
export function fitted(side: number, fill: number): string {
  return centredOn(side, (fill * side) / Math.max(ICON_BOX.width, ICON_BOX.height), ICON_CENTRE);
}

// The rounded square of the mascot's mint the app's icon, the tab's icon and the console's mark are: its
// corners rounded by 22.5% of its side, and the controls filling the given share of its width. A
// picture shown as small as a tab's sets them at 88%, since there every pixel of them counts.
export const SMALL_ICON_FILL = 0.88;

export function roundedIcon(side: number, fill: number): { corner: number; transform: string } {
  return { corner: 0.225 * side, transform: fitted(side, fill) };
}

// The polygon's points as an SVG attribute takes them.
export const svgPoints = (points: ReadonlyArray<readonly [number, number]>) => points.map(([x, y]) => `${x},${y}`).join(" ");

// The controls as SVG elements, each painted by `paint` with the attributes its fill and outline
// take, joined into one fragment.
export function controlElements(controls: readonly ControlShape[], paint: (name: ControlName) => string): string {
  return controls
    .map((control) =>
      control.kind === "polygon"
        ? `<polygon points="${svgPoints(control.points)}" ${paint(control.name)} stroke-linejoin="round"/>`
        : `<circle cx="${control.cx}" cy="${control.cy}" r="${control.r}" ${paint(control.name)}/>`
    )
    .join("");
}
