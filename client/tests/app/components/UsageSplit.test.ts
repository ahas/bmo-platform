import { describe, expect, test } from "bun:test";
import KO from "../../fixtures/app/components/UsageSplit.test.ko.json";

// What a row's traffic divided into looks like on a screen, wherever the row came from. The
// picture is a track cut into the four kinds with the parts named under it, and the rules of it
// are read off the file that ships: what makes them true is a slot, a fallback and a handful of
// classes, none of which a rendered screen would name.

const source = await Bun.file(new URL("../../../apps/web/app/components/UsageSplit.vue", import.meta.url)).text();
const template = source.slice(0, source.indexOf("<script"));

function tagOf(mark: string): string {
  const at = template.indexOf(mark);

  expect(at).toBeGreaterThan(-1);

  const opened = template.lastIndexOf("<", at);

  return template.slice(opened, template.indexOf(">", at) + 1);
}

describe("the kinds as a bar", () => {
  test("there is a template to read, so nothing below passes on an empty string", () => {
    expect(template.length).toBeGreaterThan(400);
    expect(template).toContain("<UProgressGroup");
  });

  test("the parts are what the row's kinds are, and the legend reads their figures", () => {
    // The bar and its legend both come off the one shape the domain works out, so what the
    // legend names and what the track draws cannot come apart.
    expect(source).toContain("usageBar(t, props.tally)");
    expect(template).toContain(':items="items"');
    expect(template).toContain(':max="bar.max"');

    // The figure in place of the percent the group would otherwise print: a percent of a row is
    // the shape the track is already showing, and the figure is what the reader came for.
    expect(template).toContain('#item-trailing="{ item }"');
    expect(template).toContain("{{ item.text }}");
  });

  // A row from before the kinds were kept apart has nothing to divide, so it keeps the figures
  // it always had: four dashes, and no bar over them saying a row was measured and found empty.
  test("a row that states no kind falls back to the figures, unfolded", () => {
    expect(template).toContain("v-else");
    // The four unfolded, not the folded three: the popover names cache reads and cache writes apart,
    // and a row that wraps has no long line for a fold to answer.
    expect(template).toContain("kindFigures($t, tally)");

    // The word for a kind and its number are separate marks, so the word can be quiet and the
    // number can be tabular; a kind with no figure carries no word, so the word only stands
    // where there is one.
    expect(template).toContain("v-if=\"kind.label !== ''\"");
    expect(tagOf("{{ kind.text }}")).toContain("tabular-nums");
  });

  // The four divide the total exactly, so the widths mean what they look like and the bar has
  // nothing to explain. The bar under a TODO item's account carries a note because its parts
  // overlap; a note here would be a sentence saying what the filled track already says.
  test("and no sentence under it explaining that the widths are shares", () => {
    for (const word of [KO.line_59, KO.line_59_2, KO.line_59_3]) {
      expect(template).not.toContain(word);
    }
  });
});
