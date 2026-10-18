import { describe, expect, test } from "bun:test";

// What keeps a reader at the end of an agent's log. Lines arrive while it is being read, and the
// window the log is read through changes size under it too: the box the owner types work into
// stands at the foot of an agent's screen and appears once the catalogue has answered whether
// this agent is one that can be launched into. Either way the end of the log moves, and a reader
// who was standing on it and is then left a box's height above it has had the screen taken out
// from under them for no reason they can see.
//
// The rule is the one the room's own feed is held to: watch the two heights rather than a list of
// the things that change them, because a list is the shape that leaves out whatever is put at the
// foot of the screen next. It is read off the file that ships — a rule about when a line runs is
// something a rendered screen would name as neither.

const source = await Bun.file(new URL("../../../apps/web/app/components/AgentLogView.vue", import.meta.url)).text();

describe("what keeps a reader at the end of a log", () => {
  test("there is source to read, so nothing below passes on an empty string", () => {
    expect(source.length).toBeGreaterThan(500);
    expect(source).toContain("function onContentGrew()");
  });

  test("watches the two heights themselves rather than a list of what changes them", () => {
    expect(source).toContain("new ResizeObserver(() => onContentGrew())");
    // The lines and the window the lines are read through.
    expect(source).toContain("for (const box of [feedRef.value?.$el, element])");
    expect(source).toContain("grewWatch.observe(box)");
  });

  test("carries only a reader who was standing at the end", () => {
    const at = source.indexOf("function onContentGrew()");
    const body = source.slice(at, source.indexOf("}", source.indexOf("{", at) + 1));

    expect(body).toContain("if (stick.value)");
    expect(body).toContain("scrollToBottom()");
  });

  // A worker's box arrives after the log has been read to its end, and on a phone it is taller than
  // the distance still counted as the end: the scroll the browser makes as the window shrinks under
  // it would otherwise be read as the reader leaving, and the next line would arrive out of sight.
  test("a scroll that comes with the window or the lines changing size keeps a reader at the end", () => {
    const at = source.indexOf("function onScroll()");
    const body = source.slice(at, source.indexOf("\n}\n", at));

    expect(body).toContain("element.clientHeight !== measured.window || element.scrollHeight !== measured.content");
    expect(body).toContain("if (resized && stick.value)");
    expect(body).toContain("scrollToBottom()");
  });

  test("and stops watching with the view that asked", () => {
    expect(source).toContain("grewWatch?.disconnect()");
  });
});
