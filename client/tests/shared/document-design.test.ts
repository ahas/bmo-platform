import { describe, expect, test } from "bun:test";

import { DARK_PALETTE, DOCUMENT_PALETTE, LIGHT_PALETTE } from "../../apps/web/shared/document-design";

import type { DocumentPalette } from "../../apps/web/shared/document-design";

// The document palette's own promise: every ink and tone reads at 4.5:1 or better on each surface of
// its scheme, a tone on its own soft tint included. Worked out with the WCAG contrast ratio, since
// every colour is a plain hex value.

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((at) => Number.parseInt(hex.slice(at, at + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));

  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}

function ratio(one: string, other: string): number {
  const [light, dark] = [luminance(one), luminance(other)].sort((a, b) => b - a);

  return (light! + 0.05) / (dark! + 0.05);
}

const INKS = ["ink", "inkSoft", "muted", "accent", "ok", "warn"] as const;
const SURFACES = ["paper", "surface", "sunk"] as const;
const TINTS = [
  ["accent", "accentSoft"],
  ["ok", "okSoft"],
  ["warn", "warnSoft"],
  ["inkSoft", "sunk"]
] as const;

describe.each([
  ["light", LIGHT_PALETTE],
  ["dark", DARK_PALETTE]
] as [string, DocumentPalette][])("the %s document palette", (_name, palette) => {
  test("reads every ink at 4.5:1 or better on every surface", () => {
    const short = INKS.flatMap((ink) => SURFACES.map((surface) => ({ ink, surface, ratio: ratio(palette[ink], palette[surface]) }))).filter((pair) => pair.ratio < 4.5);

    expect(short).toEqual([]);
  });

  test("reads every tone at 4.5:1 or better on its own tint", () => {
    const short = TINTS.map(([ink, tint]) => ({ ink, tint, ratio: ratio(palette[ink], palette[tint]) })).filter((pair) => pair.ratio < 4.5);

    expect(short).toEqual([]);
  });
});

test("the console's palette names every colour in both schemes, dark under the console's dark class", () => {
  expect(DOCUMENT_PALETTE).toStartWith(":root{--doc-paper:#f6f5f1;");
  expect(DOCUMENT_PALETTE).toContain(":root.dark{--doc-paper:#141416;");
  expect(DOCUMENT_PALETTE.match(/--doc-ink-soft:/g)).toHaveLength(2);
});
