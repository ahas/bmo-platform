// Draws every picture of BMO the console, the Android app and the machine they run on carry: the
// installed app's icons, the tab's icons, the account picture and the Android app's launcher icon
// out of BMO's controls, the yellow cross pad, the blue triangle, the small green button and the
// large red one, on BMO's body mint; the notification's small picture out of the same controls as a
// silhouette; and the login screen's logo out of one drawing of BMO's whole front (docs/web.md, 「The
// console's pictures」).
//
//   bun client/scripts/icons.ts
//
// The front is held in units of BMO's own front, 266 wide and 350 tall, laid out after BMO's profile
// picture with its perspective flattened: the parts at the photo's sizes, one unit to a pixel of it,
// and the body at the proportion BMO reads at there with its side and top. The controls an icon
// carries are the front's own (client/shared/bmo-controls.ts), and each picture places them in a
// frame of its own and is written as an SVG and as the PNGs of the sizes it ships in, each PNG
// rendered from the vector at its own pixel size so every edge is smoothed there.

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import sharp from "sharp";
import {
  BMO_MINT,
  centredOn,
  CONTROL_OUTLINE,
  CONTROL_PAINT,
  type ControlShape,
  controlElements,
  FRONT_CONTROLS,
  fitted,
  ICON_CENTRE,
  ICON_CONTROLS,
  ICON_REACH,
  roundedIcon as roundedFrame,
  SMALL_ICON_FILL
} from "../shared/bmo-controls";

const OUT = join(import.meta.dir, "../public/icons");
const ANDROID_RES = join(import.meta.dir, "../../android/app/src/main/res");

const FRONT = { width: 266, height: 350 };
const LOGO = { width: 478, height: 200 };

const COLOUR = {
  ink: "#12201f",
  body: BMO_MINT,
  screen: "#c6e1d0",
  slot: "#0b2c2a",
  dot: "#3850a8",
  bars: "#1a163f"
};

// BMO's front, straight on: the body and everything the profile picture shows on it.
function front(): string {
  return `
  <rect x="2.5" y="2.5" width="261" height="345" rx="24" fill="${COLOUR.body}" stroke="${COLOUR.ink}" stroke-width="5"/>${parts()}`;
}

// What stands on BMO's body: the screen and the face in it, the slot under the screen, the small
// blue dot, the cross pad, the three buttons and the two bars, every button flat, since a front view
// shows no side of one. The smile is the photo's wide, flat-bottomed curve, its ends rising steeply
// and its middle level.
function parts(): string {
  return `
  <rect x="27" y="27" width="212" height="158" rx="14" fill="${COLOUR.screen}" stroke="${COLOUR.ink}" stroke-width="4.5"/>
  <ellipse cx="71" cy="86" rx="8" ry="11.5" fill="${COLOUR.ink}"/>
  <ellipse cx="195" cy="86" rx="8" ry="11.5" fill="${COLOUR.ink}"/>
  <path d="M 114 120 C 117 131, 149 131, 152 120" fill="none" stroke="${COLOUR.ink}" stroke-width="4" stroke-linecap="round"/>
  <rect x="31" y="197" width="144" height="15" rx="2" fill="${COLOUR.slot}"/>
  <circle cx="209" cy="205" r="8" fill="${COLOUR.dot}" stroke="${COLOUR.ink}" stroke-width="3.5"/>${controls(FRONT_CONTROLS)}
  <rect x="43" y="308" width="26" height="11" rx="5.5" fill="${COLOUR.bars}"/>
  <rect x="86" y="308" width="26" height="11" rx="5.5" fill="${COLOUR.bars}"/>`;
}

// BMO's controls, each in its own colour and the darker shade of it its outline is drawn in. On the
// front the buttons stand higher than the photo's, the red button's bottom edge level with the bottom
// edge of the bars.
function controls(shapes: readonly ControlShape[]): string {
  return `\n  ${controlElements(shapes, (name) => `fill="${CONTROL_PAINT[name].fill}" stroke="${CONTROL_PAINT[name].outline}" stroke-width="${CONTROL_OUTLINE}"`)}`;
}

// The small picture a phone's status bar paints in a colour of its own is 24dp square with the
// controls standing 2dp in from its sides, the margin Android keeps clear around one,
// rendered at 24, 48 and 72 pixels for one, two and three device pixels to a CSS pixel.
const BADGE = { side: 24, margin: 2, densities: [1, 2, 3] };

// Each badge is named by its side in pixels, which the service worker picks by.
const badgeName = (density: number) => `badge-${BADGE.side * density}.png`;

// A picture's SVG at its own size in units, which a PNG is rendered from at any size.
interface Picture {
  width: number;
  height: number;
  source: string;
}

function svg(width: number, height: number, body: string, comment = ""): Picture {
  return {
    width,
    height,
    source: `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">${comment}${body}\n</svg>\n`
  };
}

// How far a picture a platform cuts to a shape of its own keeps the controls in from its edge: the
// circle of 32% of the side they stand in, well inside the 40% every such shape leaves whole.
const CUT_FILL = 0.32;

// BMO's front set into a frame at a height of the frame's own choosing, centred on (cx, top).
function placed(drawing: string, unitHeight: number, cx: number, top: number, height: number): string {
  const scale = height / unitHeight;
  const width = (FRONT.width / FRONT.height) * unitHeight * scale;

  return `\n  <g transform="translate(${cx - width / 2} ${top}) scale(${scale})">${drawing}\n  </g>`;
}

// The app's icon, shown uncut, and the tab's: a rounded square of BMO's mint, transparent outside
// its corners, with the controls filling the given share of its width.
function roundedIcon(side: number, fill: number): Picture {
  const frame = roundedFrame(side, fill);

  return svg(side, side, `\n  <rect width="${side}" height="${side}" rx="${frame.corner}" fill="${COLOUR.body}"/>\n  <g transform="${frame.transform}">${controls(ICON_CONTROLS)}\n  </g>`);
}

// A picture a platform rounds itself has no ground of its own: the whole square is BMO's mint, so
// whatever shape it is cut to is the mint and nothing else shows around it. The controls fill the
// given share of its width, or, cut to a shape of the platform's own, stand in the circle that
// shape leaves whole.
function squareIcon(fit: "box" | "cut"): Picture {
  const side = 512;
  const transform = fit === "box" ? fitted(side, 0.7) : centredOn(side, (CUT_FILL * side) / ICON_REACH, ICON_CENTRE);

  return svg(side, side, `\n  <rect width="${side}" height="${side}" fill="${COLOUR.body}"/>\n  <g transform="${transform}">${controls(ICON_CONTROLS)}\n  </g>`);
}

// The controls as a silhouette, each filled with its outline in one colour.
const silhouette = (colour: string) => controlElements(ICON_CONTROLS, () => `fill="${colour}" stroke="${colour}" stroke-width="${CONTROL_OUTLINE}"`);

// The small picture Android draws in the status bar: the controls' silhouette, its shape carried by
// the alpha channel alone, since the platform paints every opaque pixel in its own colour.
function badge(): Picture {
  const side = BADGE.side;
  const comment = `
  <!-- The small icon Android draws in the status bar, 24dp square: BMO's controls as a
       silhouette. Drawn by client/scripts/icons.ts. -->`;

  return svg(side, side, `\n  <g transform="${fitted(side, (side - 2 * BADGE.margin) / side)}">\n  ${silhouette("#ffffff")}\n  </g>`, comment);
}

// The login screen's logo: BMO beside its name, on no ground, drawn at a small height. The letters
// are strokes, so the logo reads the same on every machine whatever fonts it has; the O is a little
// narrower than the M and wider than the B, and the gaps between the letters are even. Each letter
// is black in the ink of BMO's outline, laid over a wider stroke of BMO's body mint, so it reads by
// its black on a light login screen and by its mint edge on a dark one.
function loginLogo(): Picture {
  const figure = placed(front(), FRONT.height, 78, 0, LOGO.height);
  const top = 55;
  const cap = 110;
  const shapes = [
    `<path d="M 187 ${top} V ${top + cap} M 187 ${top} H 217 A 25 25 0 0 1 217 ${top + 52} H 187 M 217 ${top + 52} A 29 29 0 0 1 217 ${top + cap} H 187"`,
    `<path d="M 282 ${top + cap} V ${top} L 320 ${top + 70} L 358 ${top} V ${top + cap}"`,
    `<rect x="394" y="${top}" width="68" height="${cap}" rx="34"`
  ];
  const stroke = (colour: string, width: number) =>
    `fill="none" stroke="${colour}" stroke-width="${width}" stroke-linecap="round" stroke-linejoin="round"`;
  const layer = (colour: string, width: number) => shapes.map((shape) => `\n  ${shape} ${stroke(colour, width)}/>`).join("");
  const letters = `${layer(COLOUR.body, 28)}${layer(COLOUR.ink, 18)}`;

  return svg(LOGO.width, LOGO.height, `${figure}${letters}`);
}

// A PNG of a picture at the given size, rendered from its vector at that size rather than resized
// from a picture rendered at another, so its edges are smoothed for the pixels it ships in.
async function png(picture: Picture, size: { width: number; height: number }, name: string): Promise<void> {
  const density = (72 * size.width) / picture.width;
  const image = await sharp(Buffer.from(picture.source), { density }).resize(size.width, size.height).ensureAlpha().png({ compressionLevel: 9 }).toBuffer();

  writeFileSync(join(OUT, name), image);
}

// The Android app's launcher icon is an adaptive icon: a background layer of BMO's mint and a
// foreground of the controls, each layer 108dp square of which the launcher shows the middle 72dp
// cut to its own shape. The controls stand in the same share of what the launcher shows as the
// maskable icon's controls do of its square; the monochrome layer, which a launcher theming its icons
// paints in its own colour, is their silhouette.
const ADAPTIVE = { side: 108, shown: 72 };

const androidColour = (hex: string) => `#FF${hex.slice(1).toUpperCase()}`;

function androidPath(control: ControlShape): string {
  if (control.kind === "polygon") return `M${control.points.map(([x, y]) => `${x},${y}`).join("L")}Z`;

  const { cx, cy, r } = control;

  return `M${cx - r},${cy}a${r},${r} 0 1,0 ${2 * r},0a${r},${r} 0 1,0 ${-2 * r},0Z`;
}

function adaptiveLayer(what: string, paint: (name: ControlShape["name"]) => { fill: string; stroke: string }): string {
  const scale = (CUT_FILL * ADAPTIVE.shown) / ICON_REACH;
  const offset = (point: number) => ADAPTIVE.side / 2 - scale * point;
  const paths = ICON_CONTROLS.map((control) => {
    const { fill, stroke } = paint(control.name);

    return `
        <path
            android:fillColor="${fill}"
            android:strokeColor="${stroke}"
            android:strokeWidth="${CONTROL_OUTLINE}"
            android:strokeLineJoin="round"
            android:pathData="${androidPath(control)}" />`;
  }).join("");

  return `<?xml version="1.0" encoding="utf-8"?>
<!-- ${what} Drawn by client/scripts/icons.ts. -->
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="${ADAPTIVE.side}dp"
    android:height="${ADAPTIVE.side}dp"
    android:viewportWidth="${ADAPTIVE.side}"
    android:viewportHeight="${ADAPTIVE.side}">
    <group
        android:translateX="${offset(ICON_CENTRE.x)}"
        android:translateY="${offset(ICON_CENTRE.y)}"
        android:scaleX="${scale}"
        android:scaleY="${scale}">${paths}
    </group>
</vector>
`;
}

const ANDROID_LAUNCHER = {
  "drawable/ic_launcher_foreground.xml": adaptiveLayer("The launcher icon's foreground: BMO's controls, each in its own colour and a darker shade of it as its outline.", (name) => ({
    fill: androidColour(CONTROL_PAINT[name].fill),
    stroke: androidColour(CONTROL_PAINT[name].outline)
  })),
  "drawable/ic_launcher_monochrome.xml": adaptiveLayer("The launcher icon's monochrome layer: the controls' silhouette, which a themed launcher paints in its own colour.", () => ({
    fill: "#FFFFFFFF",
    stroke: "#FFFFFFFF"
  })),
  "values/ic_launcher.xml": `<?xml version="1.0" encoding="utf-8"?>
<!-- The launcher icon's background layer: BMO's body mint. Drawn by client/scripts/icons.ts. -->
<resources>
    <color name="ic_launcher_background">${androidColour(COLOUR.body)}</color>
</resources>
`,
  "mipmap-anydpi-v26/ic_launcher.xml": `<?xml version="1.0" encoding="utf-8"?>
<!-- The app's launcher icon: BMO's controls on BMO's mint, cut by the launcher to its own
     shape. Drawn by client/scripts/icons.ts. -->
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@color/ic_launcher_background" />
    <foreground android:drawable="@drawable/ic_launcher_foreground" />
    <monochrome android:drawable="@drawable/ic_launcher_monochrome" />
</adaptive-icon>
`
};

mkdirSync(OUT, { recursive: true });

const square = (side: number) => ({ width: side, height: side });
const pictures = {
  "bmo.svg": svg(FRONT.width, FRONT.height, front()),
  "icon.svg": roundedIcon(512, 0.7),
  "icon-maskable.svg": squareIcon("cut"),
  "icon-square.svg": squareIcon("box"),
  "favicon.svg": roundedIcon(32, SMALL_ICON_FILL),
  "badge.svg": badge(),
  "login-logo.svg": loginLogo()
};

for (const [name, picture] of Object.entries(pictures)) writeFileSync(join(OUT, name), picture.source);

for (const [name, source] of Object.entries(ANDROID_LAUNCHER)) {
  mkdirSync(join(ANDROID_RES, name, ".."), { recursive: true });
  writeFileSync(join(ANDROID_RES, name), source);
}

await Promise.all([
  png(pictures["icon.svg"], square(512), "icon-512.png"),
  png(pictures["icon.svg"], square(192), "icon-192.png"),
  png(pictures["icon-square.svg"], square(180), "apple-touch-icon.png"),
  png(pictures["icon-maskable.svg"], square(512), "icon-maskable-512.png"),
  png(pictures["icon-maskable.svg"], square(192), "icon-maskable-192.png"),
  png(pictures["icon-maskable.svg"], square(512), "account.png"),
  png(pictures["favicon.svg"], square(32), "favicon-32.png"),
  png(pictures["favicon.svg"], square(16), "favicon-16.png"),
  ...BADGE.densities.map((density) => png(pictures["badge.svg"], square(BADGE.side * density), badgeName(density))),
  png(pictures["login-logo.svg"], LOGO, "login-logo.png")
]);

console.log(`drew ${Object.keys(pictures).length} SVGs and their PNGs into ${OUT}, and the launcher icon into ${ANDROID_RES}`);
