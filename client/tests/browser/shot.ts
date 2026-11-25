import { mkdirSync } from "node:fs";
import { dirname } from "node:path";

import sharp from "sharp";

import type { Page } from "playwright-core";

// A screenshot a job can read back without paying for the screen twice.
//
// A check of a screen ends in a picture, and the picture is read back into the session that took
// it. What that costs is the whole file, base64'd: a full-resolution shot of a console screen is
// around 180,000 bytes, which arrives as roughly 250,000 characters and stays in the session for
// the rest of the job. Most of those characters are pixels nobody looked at — the screen was
// checked at a width, and the shot was taken at the device's.
//
// So a shot taken here is bounded: the raster is scaled to a ceiling on its width. What is lost is
// resolution nobody was reading; what is kept is the layout, the spacing and the text, which is
// what a check of a screen is about.
//
// It is encoded both ways and the smaller file is the one kept. Neither format wins on every
// screen: a dense screen with photographs, gradients and antialiased text is far smaller as JPEG,
// and a flat one — a table of text on one background, a forced-colours screen — is smaller as
// PNG, by as much as three quarters. A shot is taken once and read once, so the second encode
// costs a fraction of the read it saves, and the alternative is a helper that makes some screens
// worse than taking them by hand.
//
// The capture is the driver's own screenshot, because the driver's protocol session is the one
// holding the page's emulation. A clipped capture asked of a second session lays the page out under
// that session's emulation and clears the driver's when it ends — a phone page loses its pixel
// ratio and its mobile layout and reflows narrower — so the bound is applied to the captured raster
// instead (the agency's docs/worker-bench.md).

/** How wide a shot may be, in pixels. The same ceiling the browse agent's own browser carries. */
export const shotWidth = 1280;

/** How hard the JPEG is squeezed. Enough that text stays legible and a gradient does not band. */
export const shotQuality = 72;

/** Which encoding a shot was kept in, being whichever of the two came out smaller. */
export type ShotFormat = "jpeg" | "png";

/** What one shot came to, so a caller says the numbers without asking the file system. */
export interface Shot {
  /** Where it was written, under the extension of the encoding that was kept. */
  path: string;
  bytes: number;
  /** The raster's own size, after the ceiling. */
  width: number;
  height: number;
  /** What the raster was multiplied by; 1 where the screen was already inside the ceiling. */
  scale: number;
  format: ShotFormat;
  /** What each encoding of those same pixels weighed, so the choice can be read rather than trusted. */
  considered: { jpeg: number; png: number };
}

// How often a refused capture is asked for again, and how long the first pause between asks is.
const captureTries = 4;
const captureWaitMs = 100;

/** The name a shot is written under, which says which of the two encodings it holds. */
export function shotPath(path: string, format: ShotFormat): string {
  return `${path.replace(/\.(png|jpe?g|webp)$/i, "")}${format === "jpeg" ? ".jpg" : ".png"}`;
}

// The screen at one pixel to a CSS pixel, losslessly, so both encodings are made from the same
// pixels. Chromium refuses a capture outright — 「Unable to capture screenshot」 — when its renderer
// is busy enough not to have a frame to hand over, which under a whole test suite it sometimes is.
// It is a moment and not a state, so the ask is repeated a few times before it is believed.
async function captured(page: Page, fullPage: boolean): Promise<Buffer> {
  for (let attempt = 0; ; attempt += 1) {
    try {
      return await page.screenshot({ type: "png", fullPage, scale: "css" });
    } catch (error) {
      if (attempt >= captureTries - 1 || !/Unable to capture screenshot/.test(String(error))) throw error;

      await new Promise((wake) => setTimeout(wake, captureWaitMs * (attempt + 1)));
    }
  }
}

/**
 * One screenshot, bounded and written.
 *
 * `fullPage` takes the whole document rather than what is on screen. It is the more expensive of
 * the two by far — a long page is a tall raster — so it is asked for where the whole page is the
 * question and not by habit.
 */
export async function shot(
  page: Page,
  path: string,
  options: { fullPage?: boolean; width?: number; quality?: number } = {},
): Promise<Shot> {
  const ceiling = options.width ?? shotWidth;
  const whole = await captured(page, options.fullPage === true);
  const { width: wholeWidth = 0 } = await sharp(whole).metadata();
  // A screen already inside the ceiling is not scaled up to meet it: the ceiling is a bound, not
  // a size.
  const scale = wholeWidth <= ceiling ? 1 : ceiling / wholeWidth;
  const { data: asPng, info } = await (scale === 1 ? sharp(whole) : sharp(whole).resize({ width: ceiling }))
    .png()
    .toBuffer({ resolveWithObject: true });
  const asJpeg = await sharp(asPng).jpeg({ quality: options.quality ?? shotQuality }).toBuffer();
  const considered = { jpeg: asJpeg.byteLength, png: asPng.byteLength };
  const format: ShotFormat = considered.jpeg <= considered.png ? "jpeg" : "png";
  const bytes = format === "jpeg" ? asJpeg : asPng;
  const written = shotPath(path, format);

  mkdirSync(dirname(written), { recursive: true });
  await Bun.write(written, bytes);

  return {
    path: written,
    bytes: bytes.byteLength,
    width: info.width,
    height: info.height,
    scale,
    format,
    considered,
  };
}

/** What a shot cost a session that reads it back: the file as base64, which is what a Read is. */
export function readCost(bytes: number): number {
  return Math.ceil(bytes / 3) * 4;
}
