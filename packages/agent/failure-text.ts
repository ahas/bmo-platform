// What a failed run leaves behind for somebody to read.
//
// A run that died says why on its last lines, and everything above them is the program starting
// up: a welcome banner, an ASCII drawing, a rule drawn across the terminal, a warning about the
// mode it was launched in. So a record that keeps the head of what was printed keeps the part
// that is the same on every run and throws away the one part that differs — which is the cause.
// The tail is kept instead, and the decoration is dropped before the tail is measured so the
// room it is given is spent on words.
//
// Nothing here decides whether a run failed or why. What was printed is read whole where that is
// judged — a login gone, a session the CLI no longer holds — and this is only the shortening
// applied afterwards, on the way into a row, a log line or a message.

/**
 * How much of a failed run's output is kept, everywhere it is kept. One value, because a record
 * cut to 200 characters in one place and 500 in another is read as two different failures.
 */
export const failureTextLimit = 2000;

const csiEscape = /\[[0-9;:?]*[ -/]*[@-~]/g;
const osEscape = /\][^]*(?:|\\)/g;
const loneEscape = /[@-Z\\-_]/g;

/** A line carrying no letter and no digit is a drawing, a rule or a row of dots, never a cause. */
function saysSomething(line: string): boolean {
  return /[\p{L}\p{N}]/u.test(line);
}

function stripped(raw: string): string {
  return raw
    .replaceAll(csiEscape, "")
    .replaceAll(osEscape, "")
    .replaceAll(loneEscape, "")
    .replaceAll("\r\n", "\n")
    .replaceAll("\r", "\n");
}

function tailOf(text: string, limit: number): string {
  if (text.length <= limit) {
    return text;
  }

  const tail = text.slice(text.length - limit);
  const lineBreak = tail.indexOf("\n");

  // The cut lands mid-line, so that half line is dropped rather than shown as though it were
  // whole — unless dropping it would leave nothing, in which case the half line is the record.
  return lineBreak === -1 || lineBreak === tail.length - 1 ? `…${tail}` : `…\n${tail.slice(lineBreak + 1)}`;
}

/**
 * What a failed run printed, shortened for a record: escape sequences gone, decoration lines
 * gone, blank runs closed up, and the last {@link failureTextLimit} characters kept with a `…`
 * where the head was dropped. Output that is decoration and nothing else keeps its own tail, so
 * a record is never left empty by the filtering.
 */
export function failureText(raw: string, limit: number = failureTextLimit): string {
  const plain = stripped(raw);
  const said = plain
    .split("\n")
    .map((line) => line.trimEnd())
    .filter(saysSomething)
    .join("\n")
    .trim();

  return tailOf(said === "" ? plain.trim() : said, limit);
}
