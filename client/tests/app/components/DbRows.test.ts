import { describe, expect, test } from "bun:test";

// The table of rows and the shared table it stands on, read from the files that ship, because what
// makes the column names stay overhead is a handful of classes and nothing a rendered DOM would
// report: a name that has scrolled out of sight is still in the document.

const rows = await Bun.file(new URL("../../../apps/web/app/components/DbRows.vue", import.meta.url)).text();
const table = await Bun.file(new URL("../../../apps/web/node_modules/bmo-ui/app/components/DataTable.vue", import.meta.url)).text();
const handle = await Bun.file(new URL("../../../apps/web/node_modules/bmo-ui/app/components/ColumnResizeHandle.vue", import.meta.url)).text();

describe("the rows of a table on the DB screen", () => {
  test("stand on the shared table, dense, each row pressed as a whole", () => {
    expect(rows).toContain('<DataTable :data="props.rows" :columns="columns" layout="dense" selectable');
    expect(rows).toContain("emit('open', at)");
    expect(rows).toContain("emit('sort', column.name)");
  });

  test("scroll in both directions inside their own box, with the names held overhead", () => {
    // Sideways because a table is wider than a phone, and down because a stuck cell holds itself
    // against the nearest box that scrolls.
    expect(table).toContain('dense: { root: "min-h-0 overflow-auto"');
    expect(table).toContain("sticky");
    expect(table).toContain('thead: "bg-elevated');
  });

  test("draw no line twice: none under the head row beside the table's own separator, none under the last row", () => {
    expect(table).toContain('thead: "bg-elevated [&>tr]:border-b-0"');
    expect(table.match(/border-b border-default(?:\/60)? last:border-b-0/g)).toHaveLength(2);
  });
});

describe("a column's width", () => {
  test("is moved from a separator on the right edge of its name, by pointer or by arrow key", () => {
    expect(handle).toContain("cursor-col-resize");
    expect(handle).toContain("end-0");
    expect(handle).toContain("@pointerdown.prevent.stop=\"emit('start', $event)\"");
    expect(handle).toContain("@keydown.left.prevent.stop=");
    expect(handle).toContain("@keydown.right.prevent.stop=");
    expect(table).toContain('<ColumnResizeHandle');
  });

  test("is held for every column once one moves, so the table lays out by the widths it was given", () => {
    expect(table).toContain('"table-fixed w-(--data-table-width)"');
    expect(table).toContain("style: { ...meta.style, th: { width: `${width}px` } }");
  });
});
