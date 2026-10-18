import data from "../i18n/data/ko.yaml";

// The marks a message's text opens with where it stands for something other than words, read from the data
// file beside the language pack, since the words are the language's and not the code's.

interface TextMarks {
  marks: { voice: string };
}

const MARKS = (data as TextMarks).marks;

function escaped(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** A voice message's mark: the voice word in brackets, with the recording's length where one is known. */
export const VOICE_MARK = new RegExp(`^\\[${escaped(MARKS.voice)}(?: \\d+:\\d{2})?\\]`);

/** The words a voice message says, as its line holds them after the mark; null for a line that is not one. */
export function spokenWords(text: string): string | null {
  const found = VOICE_MARK.exec(text);

  return found === null ? null : text.slice(found[0].length).trim();
}
