import { describe, expect, test } from "bun:test";

import { keysFor } from "../../../apps/web/shared/wording";
import KO from "../../fixtures/app/components/ResearchSettings.test.ko.json";

// The pool and research controls as they ship. Their behaviour behind the form is the bot's
// (server/research-controls.test.ts) and the domain's (domain/research.test.ts); these checks hold the
// screen to what docs/web.md promises it shows and never offers.

// The sources are read where the app keeps them.
const read = async (name: string) => await Bun.file(new URL(name, new URL("../../../apps/web/app/components/", import.meta.url))).text();
const templateOf = (source: string) => source.slice(0, source.indexOf("<script"));

const settings = await read("./ResearchSettings.vue");
const limits = await read("./AgentsCoworkerLimits.vue");
const engines = await read("./EngineHealthList.vue");
const page = await read("../pages/agents/index.vue");

describe("one model source", () => {
  test("a searcher's model comes from its tier, and nothing on these tabs writes a mapping", () => {
    for (const source of [settings, limits, engines]) {
      expect(source).not.toContain("set-mapping");
    }
    // The tier is the key's own assignment; no profile carries a model or an effort.
    expect(settings).toContain('body: { action: "assign", key, tier }');
    expect(settings).not.toContain("effort:");
    expect(templateOf(settings)).toContain("tierRunText($t, row.original)");
  });

  test("stand in their tabs of the agent page and reread with it", () => {
    expect(page).toContain("<AgentsCoworkerLimits");
    expect(page).toContain('<ResearchSettings v-else :query="query"');
    expect(page).toContain("rereadResearch()");
  });
});

describe("states", () => {
  test("loading, failed first read with retry, and stale values that lock saving", () => {
    for (const source of [settings, limits]) {
      const template = templateOf(source);

      // The table's head is the screen's own and stands while the rows are read.
      expect(template).toContain("<DataTableSkeleton");
      expect(template).toContain(':headers="tableHeaders(columns)"');
      expect(template).toContain("label: $t('common.action.refresh')");
      expect(template).toContain('v-if="stale"');
      expect(template).toContain(':disabled="stale');
    }
  });

  test("every profile setting is its own cell, checked like the bot checks it before it is sent", () => {
    const template = templateOf(settings);

    for (const field of ["childRole", "maxChildren", "waitLimitMs", "strategy"]) {
      expect(template).toContain(`#${field}-cell`);
    }
    expect(template).toContain('v-for="field in INTERVAL_FIELDS"');
    expect(template).toContain('v-for="field in CLOCK_FIELDS"');
    expect(template).toContain("fieldFault($t, settings!, row.original,");
    expect(template).toContain(":disabled=\"stale || row.original.key === 'search:basic'\"");
    expect(template).toContain('>{{ $t("research.label.no_limit") }}<');
    expect(settings).toContain('action: "set-profile", key: profile.key, values: withField(profile, field, value)');
    expect(settings).toContain('action: "reset-profile", key: resetKey.value');
    for (const key of keysFor(KO.line_62)) {
      expect(template).not.toContain(key);
    }
  });

  test("engine state is written in words and the icon is decorative", () => {
    const template = templateOf(engines);

    expect(template).toContain("engineStateLabel($t, engine)");
    expect(template).toContain('aria-hidden="true"');
    expect(template).toContain(`:label="$t('research.label.korean_first')"`);
  });
});
