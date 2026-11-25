// How a watch is shown. A watch carries a short name and a note of the whole of what it promised, and every
// line that shows one shows the name alone; a watch carrying no name is shown by the head of its note
// instead, so a watch set without one still says which watch it is.

// A name is read at a glance on a line that has the room, the pace and the time left on it too, so it is held
// to a few words; a longer one is cut where it is shown.
export const maxNameLength = 40;

// A note brought to one line of at most `limit` characters, cut with an ellipsis.
function oneLine(text: string, limit: number): string {
  const line = text.trim().replace(/\s+/g, " ");

  return line.length > limit ? `${line.slice(0, limit - 1)}…` : line;
}

// The short name a watch is shown by: the one it was given, or the head of its note where it was given none.
export function watchName(watch: { name: string | null; note: string }): string {
  const given = watch.name?.trim() ?? "";
  const shown = given === "" ? (watch.note.split("\n").find((line) => line.trim() !== "") ?? "") : given;

  return oneLine(shown, maxNameLength);
}
