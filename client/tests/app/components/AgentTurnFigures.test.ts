import { describe, expect, test } from "bun:test";

// What a turn's figures are drawn as, read off the files that ship: one line per run, how long it
// took in figures that line up, and under it the four kinds as one group of bars named beside their
// figures — the group every other split of tokens on the console is drawn with.

const block = await Bun.file(new URL("../../../apps/web/app/components/AgentTurnFigures.vue", import.meta.url)).text();
const view = await Bun.file(new URL("../../../apps/web/app/components/AgentLogView.vue", import.meta.url)).text();
const template = block.slice(0, block.indexOf("<script"));

describe("a turn's figures", () => {
  test("stand one line per run, with its stage, its road and how it ended where it did not answer", () => {
    expect(template).toContain('v-for="line in props.lines"');
    expect(template).toContain("{{ line.stage }}");
    expect(template).toContain("{{ line.road }}");
    expect(template).toContain('v-if="line.outcome !== null"');
  });

  test("keep the time in figures that line up, and never at the smallest size", () => {
    expect(template).toMatch(/<span v-if="line.took !== ''" class="[^"]*tabular-nums[^"]*">\{\{ line.took \}\}<\/span>/);
    expect(template).not.toContain("text-xs");
  });

  test("draw each run's four kinds as the one group of bars, read, write, cache read and cache write", () => {
    expect(template).toContain('<UsageSplit :tally="line.tally" />');
    expect(template).not.toContain("ShortKinds");
  });

  test("are what a run's chip spreads into, under the line the run ended on", async () => {
    const spend = await Bun.file(new URL("../../../apps/web/app/components/RunSpend.vue", import.meta.url)).text();
    const at = view.indexOf("<RunSpend");

    expect(spend).toContain('<AgentTurnFigures v-if="open" :lines="props.chip.lines"');
    expect(at).toBeGreaterThan(view.indexOf('<AgentLogRow :row="row" />'));
    expect(view).toContain('v-if="chipOf(row) !== null"');
    expect(view).not.toContain("<AgentTurnFigures");
  });
});
