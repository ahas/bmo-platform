import { describe, expect, test } from "bun:test";

// The mark left of a reference document's badge, saying whether the document was read on that turn
// or was already in the session. Read off the file that ships: what a rendered screen would not name
// is that a finger resting on it asks for its name and never chooses the message it stands in. Where it
// stands in a room's turn actions is the receiver screens' to hold, with the room's own files.

const mark = await Bun.file(new URL("../../../apps/web/app/components/ContextStateMark.vue", import.meta.url)).text();
const logRow = await Bun.file(new URL("../../../apps/web/app/components/AgentLogRow.vue", import.meta.url)).text();

describe("the read-now or read-before mark", () => {
  test("is an icon alone, named by its tooltip and its accessible name, and reachable by the keyboard", () => {
    expect(mark).toContain('<HintTip :text="props.look.label" press');
    expect(mark).toContain(':aria-label="props.look.label"');
    expect(mark).toContain('role="img"');
    expect(mark).toContain('tabindex="0"');
    expect(mark).not.toContain(":label=");
  });

  test("keeps the rest to itself, so the message it stands in is neither chosen nor given a menu", () => {
    expect(mark).toContain("@touchstart.stop");
    expect(mark).toContain("@contextmenu.prevent.stop");
  });

  test("stands left of the badge on the agent's own log", () => {
    expect(logRow.indexOf("<ContextStateMark")).toBeLessThan(logRow.indexOf("<UBadge"));
  });
});
