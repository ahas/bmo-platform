import type { MemoryRowView } from "#shared/types";
import type { Translate } from "#shared/wording";

// How the Memories screen words a memory where the table, its phone list and the dialog of one row all
// say the same thing (`docs/memories.md`, 「The console screen」).

// Whom a row is for: its room by name, or the mark that it is global, and the person where it is
// for one.
export function whomOf(t: Translate, memory: MemoryRowView): string {
  const room = memory.room ?? t("memories.label.global_reach", { reach: memory.reach });

  return memory.forName === null ? room : t("memories.label.whom_person", { room, person: memory.forName });
}

// An event as the row and the memory script write it, which is the table's own value and stands as
// it is written, as a type does.
export function eventText(event: NonNullable<MemoryRowView["event"]>): string {
  return event.item === null ? event.kind : `${event.kind}:${event.item}`;
}

// The three states a row stands in: open while its flag is on and nobody has checked it off, which
// is what the agent itself holds as standing; finished once checked off with its flag still on; and
// down once its flag is off, whether or not it was checked off first.
export const memoryStates = ["open", "done", "down"] as const;
export type MemoryState = (typeof memoryStates)[number];

export function memoryState(memory: Pick<MemoryRowView, "active" | "doneAt">): MemoryState {
  if (!memory.active) {
    return "down";
  }

  return memory.doneAt === null ? "open" : "done";
}
