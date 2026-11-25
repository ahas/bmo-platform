#!/usr/bin/env python3
"""Builds the console's interface faces from their upstream releases (docs/web.md, 「Fonts」).

Two families come out, at the four weights the screens ask for:

- Apple SD Gothic Neo, the interface's face, cut to the letters Pretendard's subset build carries
  that Apple SD Gothic Neo has as well — Latin, the common Hangul, the punctuation and symbols a
  screen prints. The release carries no OpenType features at all, so tabular figures are added
  here: a copy of each digit on one shared width, its ink centred on it, put in under `tnum`.
  Its vertical metrics are set so the Hangul's ink stands at the middle of a line, as Pretendard's
  does; the release's own leave every line's letters sitting low.
- Pretendard, standing behind it, cut to exactly what the first lacks — the accented Latin, the
  middle dot, the minus, the narrow spaces — so the two together read what Pretendard alone read.

usage: python3 -I client/scripts/interface-font.py --apple <dir> --pretendard <dir> --out <dir>

`--apple` holds the release's AppleSDGothicNeo-<Weight>.woff2, `--pretendard` Pretendard's
woff2-subset build, Pretendard-<Weight>.subset.woff2.
"""

import argparse
import statistics
from pathlib import Path

from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.subset import Options, Subsetter
from fontTools.ttLib import TTFont

WEIGHTS = ["Regular", "Medium", "SemiBold", "Bold"]
DIGITS = "0123456789"
HANGUL = range(0xAC00, 0xD7A4)


def cut(font: TTFont, unicodes: set[int], features: list[str]) -> None:
    options = Options()
    options.layout_features = features
    options.drop_tables += ["vhea", "vmtx", "FFTM"]
    options.hinting = False
    subsetter = Subsetter(options)
    subsetter.populate(unicodes=unicodes)
    subsetter.subset(font)


def ink(font: TTFont, glyph: str) -> tuple[float, float, float, float]:
    pen = BoundsPen(font.getGlyphSet())
    font.getGlyphSet()[glyph].draw(pen)
    return pen.bounds


def add_tabular_figures(font: TTFont) -> int:
    """A digit each on one width, wide enough for the widest digit's ink with the narrowest
    digit's own margins around it, so a figure that ticks keeps its width."""
    cmap = font.getBestCmap()
    names = [cmap[ord(digit)] for digit in DIGITS]
    bounds = {name: ink(font, name) for name in names}
    hmtx = font["hmtx"]
    margin = min(min(box[0], hmtx[name][0] - box[2]) for name, box in bounds.items())
    width = round(max(box[2] - box[0] for box in bounds.values()) + 2 * margin)

    glyf = font["glyf"]
    order = list(font.getGlyphOrder())
    tabular = []
    for name in names:
        x_min, _, x_max, _ = bounds[name]
        shift = round((width - (x_max - x_min)) / 2 - x_min)
        pen = TTGlyphPen(font.getGlyphSet())
        font.getGlyphSet()[name].draw(TransformPen(pen, (1, 0, 0, 1, shift, 0)))
        copy = f"{name}.tnum"
        glyf[copy] = pen.glyph()
        hmtx[copy] = (width, round(x_min + shift))
        tabular.append(copy)
    font.setGlyphOrder(order + tabular)
    glyf.glyphOrder = font.getGlyphOrder()

    addOpenTypeFeaturesFromString(
        font,
        "languagesystem DFLT dflt;\n"
        f"feature tnum {{ sub [{' '.join(names)}] by [{' '.join(tabular)}]; }} tnum;\n",
    )
    return width


def centred_lines(font: TTFont) -> tuple[int, int]:
    """Ascent and descent split so a line's half-leading lands the Hangul's ink in the middle of
    the line, the two summing to the release's own so a line of normal height keeps its height.
    Read off the regular weight and given to every weight, so a bold word in a line stands on the
    same baseline and asks for no more height than the words around it."""
    cmap = font.getBestCmap()
    middles = [sum(ink(font, cmap[code])[1::2]) / 2 for code in HANGUL if code in cmap]
    middle = statistics.mean(middles)
    hhea = font["hhea"]
    total = hhea.ascent - hhea.descent
    ascent = round((total + 2 * middle) / 2)
    return ascent, ascent - total


def set_lines(font: TTFont, ascent: int, descent: int) -> None:
    cmap = font.getBestCmap()
    hhea = font["hhea"]
    hhea.ascent, hhea.descent, hhea.lineGap = ascent, descent, 0
    os2 = font["OS/2"]
    os2.sTypoAscender, os2.sTypoDescender, os2.sTypoLineGap = ascent, descent, 0
    os2.usWinAscent = max(os2.usWinAscent, ascent)
    os2.usWinDescent = max(os2.usWinDescent, -descent)
    os2.fsSelection |= 1 << 7
    if os2.version < 4:
        os2.version = 4
        os2.sxHeight = round(ink(font, cmap[ord("x")])[3])
        os2.sCapHeight = round(ink(font, cmap[ord("H")])[3])
        os2.usDefaultChar, os2.usBreakChar, os2.usMaxContext = 0, 32, 1


def build(apple_dir: Path, pretendard_dir: Path, out: Path) -> None:
    lines = None
    for weight in WEIGHTS:
        apple = TTFont(apple_dir / f"AppleSDGothicNeo-{weight}.woff2")
        pretendard_file = pretendard_dir / f"Pretendard-{weight}.subset.woff2"
        pretendard = TTFont(pretendard_file)
        carried = set(pretendard.getBestCmap())
        shared = carried & set(apple.getBestCmap())

        cut(apple, shared, [])
        width = add_tabular_figures(apple)
        lines = lines or centred_lines(apple)
        set_lines(apple, *lines)
        apple.flavor = "woff2"
        apple_out = out / f"AppleSDGothicNeo-{weight}.subset.woff2"
        apple.save(apple_out)

        cut(pretendard, carried - shared, ["*"])
        pretendard.flavor = "woff2"
        pretendard_out = out / f"Pretendard-{weight}.gap.woff2"
        pretendard.save(pretendard_out)

        print(
            f"{weight}: {len(shared)} letters, tnum width {width}, ascent {lines[0]} descent {lines[1]},"
            f" {apple_out.stat().st_size:,} B; Pretendard {len(carried - shared)} letters,"
            f" {pretendard_out.stat().st_size:,} B"
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--apple", type=Path, required=True)
    parser.add_argument("--pretendard", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    build(args.apple, args.pretendard, args.out)


if __name__ == "__main__":
    main()
