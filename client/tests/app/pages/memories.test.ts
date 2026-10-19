import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";

import { memoryState } from "../../../apps/web/app/domain/memories";
import { SCREENS } from "../../../apps/web/app/domain/screens";

// The screen the memories are read as a table on. What makes it right is what its template binds
// and what its two calls send, so the file that ships is what this reads.

const source = await Bun.file(new URL("../../../apps/web/app/pages/memories.vue", import.meta.url)).text();
const template = source.slice(0, source.indexOf("<script"));
const toggle = await Bun.file(new URL("../../../apps/web/app/components/MemoryActiveSwitch.vue", import.meta.url)).text();
const condition = await Bun.file(new URL("../../../apps/web/app/components/MemoryCondition.vue", import.meta.url)).text();
const detail = await Bun.file(new URL("../../../apps/web/app/components/MemoryDetail.vue", import.meta.url)).text();
const domain = await Bun.file(new URL("../../../apps/web/app/domain/memories.ts", import.meta.url)).text();
const table = template.slice(template.indexOf("<DataTable v-else"), template.indexOf("</DataTable>"));
const list = template.slice(template.indexOf('<ul v-else-if="narrow"'), template.indexOf("</ul>"));

describe("the memories screen", () => {
  test("there is a template to read, so nothing below passes on an empty string", () => {
    expect(template.length).toBeGreaterThan(1000);
    expect(template).toContain("<DataTable");
  });

  test("stands in the sidebar under its own name", () => {
    expect(wording(SCREENS.find((screen) => screen.to === "/memories")!.label)).toBe(wording("screens.label.memories"));
    expect(SCREENS.find((screen) => screen.to === "/memories")).toEqual({
      label: "screens.label.memories",
      to: "/memories",
      icon: "i-material-symbols-rule-folder-outline",
      nested: false,
    });
    expect(SCREENS.find((screen) => screen.to === "/situation-rules")).toBeUndefined();
  });

  test("shows every column a row is read by, with its kind first", () => {
    for (const header of ["memories.label.agent", "memories.label.kind", "memories.label.action", "memories.label.whom", "memories.label.condition", "memories.label.created_at", "memories.label.ended_at", "memories.label.watch", "memories.label.hits", "memories.label.active", "memories.label.retired_reason"]) {
      expect(source).toContain(`header: t("${header}")`);
    }
    // The action, which every row has, is the one column that takes what is left; the kind stands
    // before it.
    expect(source.indexOf('id: "type"')).toBeLessThan(source.indexOf('id: "action"'));
    expect(source).toContain('{ id: "action", header: t("memories.label.action") }');
  });

  // Whose a row is stands on the table and on a phone's list, and the agents the filter offers are the
  // ones the rows name, under what the agents table calls them (docs/memories.md, 「The console screen」).
  test("shows whose each row is and filters by agent", () => {
    expect(table).toContain("row.original.agentLabel");
    expect(list).toContain("memory.agentLabel");
    expect(source).toContain('v-model="agent"');
    expect(source).toContain("memory.agent === agent.value");
    // The library refuses a choice whose value is empty, and the screen would not stand at all.
    expect(source).not.toContain('{ value: "", label');
    expect(detail).toContain("memory.agentLabel");
  });

  // A column dragged wider has to show more of its words, and what one line does not hold goes on
  // to the next rather than being cut (docs/memories.md, 「The console screen」).
  test("a cell of words wraps, and gives up its ceiling once the widths are held", () => {
    expect(source).toContain('const WRAPPED = "py-2 leading-6 whitespace-normal break-words";');
    expect(table.match(/:class="\[WRAPPED/g)?.length).toBe(5);
    // Every ceiling a column stands up to is dropped while the widths are held.
    const ceilings = table.match(/'w-max max-w-[0-9]+'/g) ?? [];

    expect(ceilings.length).toBe(5);
    expect(table.match(/held \? '' : 'w-max max-w-[0-9]+'/g)?.length).toBe(5);
    expect(table).not.toContain("truncate");
    expect(table).not.toContain("h-10");
  });

  // Pressing a row is how its whole is read: the table's row, and at a phone's width the item,
  // through a button laid over it so the switch above it stays a press of its own.
  test("pressing a row, or an item at a phone's width, opens the dialog of the whole row", () => {
    expect(table).toContain('layout="fill" selectable @select="open"');
    expect(list).toContain('<button type="button" class="absolute inset-0');
    expect(list).toContain('@click="open(memory)"');
    expect(list).toContain("memories.aria.open_named");
    expect(list).toContain('<div class="relative z-10 shrink-0">');
    expect(template).toContain('<MemoryDetail v-model:open="detailOpen" :memory="chosen" />');
    // The row is held past the press that closes the dialog, so it keeps what it drew until it is gone.
    expect(source).not.toContain("chosen.value = null");
  });

  test("the dialog holds every field of the row under its label, and leaves out the ones it lacks", () => {
    expect(detail).toContain("narrow ? UDrawer : UModal");
    for (const label of ["action", "kind", "whom", "set_by", "created_at", "ended_at", "event", "state", "watch", "hits", "todo", "condition", "done_text", "retired_reason", "candidate", "message"]) {
      expect(detail).toContain(`memories.label.${label}`);
    }
    for (const field of ["memory.action", "memory.condition", "memory.setByName", "memory.doneText", "memory.downReason", "memory.candidateWhy", "memory.todoId", "memory.message.text", "memory.messageId", "memory.eventMetAt", "memory.watch"]) {
      expect(detail).toContain(field);
    }
    expect(detail).toContain('<div v-if="memory.setByName !== null"');
    expect(detail).toContain('<div v-if="memory.todoId !== null"');
  });

  // At a phone's width the table shows two columns of ten, so the rows stand as items with each
  // field labelled, and a field the row lacks is left out.
  test("at a phone's width the rows stand as items whose fields carry their labels", () => {
    expect(source).toContain("const narrow = useNarrow();");
    for (const label of ["memories.label.whom", "memories.label.condition", "memories.label.created_at", "memories.label.ended_at", "memories.label.watch", "memories.label.hits", "memories.label.retired_reason"]) {
      expect(list).toContain(`$t("${label}")`);
    }
    expect(list).toContain('<template v-if="memory.watch !== null">');
    expect(list).toContain('<template v-if="memory.doneAt !== null">');
  });

  // One table for four kinds of row is only readable if it can be narrowed to one of them.
  test("the kinds are filtered from the toolbar, and every type the table holds is on the list", () => {
    expect(template).toContain('<USelect v-model="kind"');
    expect(template).toContain(`:aria-label="$t('memories.label.kind')"`);

    for (const type of ["promise", "check", "followup", "situation", "state"]) {
      expect(source).toContain(`"${type}"`);
    }
    expect(source).toContain('const KINDS = [t("memories.label.all_kinds"), "promise", "check", "followup", "situation", "state"]');
    expect(source).toContain("kind.value === EVERY || memory.type === kind.value");
  });

  // A dash or a nought standing where there is no value reads as data nobody wrote down.
  test("a cell with nothing to say is blank, and the match count stands only where the matcher is asked", () => {
    expect(template).not.toContain("—");
    expect(condition).not.toContain("—");
    expect(source).toContain('return memory.condition !== "";');
    expect(table).toContain('<template v-if="matched(row.original)">');
  });

  // A row waiting on an event stands its event where a condition would, marked as an event rather
  // than a condition, with when it came where it has (docs/memories.md, 「The console screen」).
  test("a row waiting on an event shows the event, marked as one, and when it came", () => {
    expect(condition).toContain('v-if="memory.event !== null"');
    expect(condition).toContain(`:label="$t('memories.label.event')"`);
    expect(condition).toContain("eventText(memory.event)");
    expect(domain).toContain("export function eventText(");
    expect(condition).toContain("memories.label.event_came");
    expect(table).toContain('<MemoryCondition :memory="row.original" wrap />');
  });

  // Being finished and being taken down are two different closings, and the screen keeps them apart.
  test("a row that was checked off shows when it ended and how", () => {
    expect(template).toContain("formatStamp(row.original.doneAt)");
    expect(template).toContain("row.original.doneText");
  });

  // Checking off leaves the flag on, so the flag alone would count every finished errand as open.
  test("a row is open only while its flag is on and nobody has checked it off", () => {
    expect(memoryState({ active: true, doneAt: null })).toBe("open");
    expect(memoryState({ active: true, doneAt: 1_790_666_000_000 })).toBe("done");
    expect(memoryState({ active: false, doneAt: null })).toBe("down");
    expect(memoryState({ active: false, doneAt: 1_790_666_000_000 })).toBe("down");
  });

  test("the switch says which row it is for, spins while the write is out and waits on it", () => {
    expect(toggle).toContain("<USwitch");
    expect(toggle).toContain("memories.aria.retire_named");
    expect(toggle).toContain("memories.aria.restore_named");
    expect(toggle).toContain(':loading="busy === memory.slug"');
    expect(toggle).toContain(':disabled="busy !== \'\'"');
    // Both layouts draw the one switch.
    expect(table).toContain("<MemoryActiveSwitch");
    expect(list).toContain("<MemoryActiveSwitch");
  });

  test("taking a row down asks why on the row's own cell, and putting one back is one press", () => {
    expect(toggle).toContain("<UPopover");
    expect(toggle).toContain("memories.label.retire_reason");
    expect(toggle).toContain('@submit.prevent="takeDown"');
    expect(toggle).toContain('@update:model-value="restore()"');
    expect(template).toContain(':restore="() => putBack(row.original)"');
    // Nothing goes out until a reason is written.
    expect(toggle).toContain(':disabled="why.trim() === \'\'"');
  });

  test("the write is the one POST, and it carries the slug, the flag and the reason", () => {
    expect(source).toContain('$fetch<MemoryChanged>("/api/memories"');
    expect(source).toContain('method: "POST"');
    expect(source).toContain("body: { slug: memory.slug, active, why: reason }");
  });

  test("the rows are read from the one GET, and a row changed is put back in place of its own", () => {
    expect(source).toContain('$fetch<MemoriesView>("/api/memories")');
    expect(source).toContain("one.slug === memory.slug ? changed.memory : one");
  });

  test("finished and down rows stay behind the state filter, which opens on the open ones", () => {
    expect(source).toContain('const state = ref<MemoryState | "all">("open")');
    expect(template).toContain('v-model="state"');
    expect(source).toContain("memoryState(memory) === state.value");
    // Nothing on this screen deletes a row.
    expect(source).not.toContain("DELETE");
    expect(template).not.toContain("common.action.delete");
  });

  test("a row raised as a candidate wears a mark and nothing takes it down by itself", () => {
    expect(template).toContain(`:label="$t('memories.label.candidate')"`);
    // The only thing that changes a flag on this screen is a person pressing the switch.
    expect(source.match(/setActive\(/g)?.length).toBe(3);
  });

  test("times and counts are read in tabular figures, and a count carries its separators", () => {
    expect(template).toContain("formatStamp(row.original.lastHitAt)");
    expect(template).toContain("formatStamp(row.original.setAt)");
    expect(template).toContain("countText(row.original.hits)");
    expect(template.match(/tabular-nums/g)?.length).toBeGreaterThanOrEqual(5);
  });

  test("the place the rows land is kept while they are read, and a refusal stands in their stead", () => {
    expect(template).toContain("<DataTableSkeleton");
    expect(template).toContain('v-if="!ready && !failed"');
    expect(template).toContain("<UAlert");
  });
});
