import spanWords from "@agency/policy/span-words.toml";
import { fillMarks } from "@agency/support/marks";

// The words a run's spend is said in.

// The four kinds of token, as one run reported them or as a window's rows add up to. Null is
// not nought: it says whatever this was read from was written before the kinds were kept apart,
// and nothing at all is known about how the traffic divided.
export interface TokenSplit {
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
  cacheWriteTokens: number | null;
}

// The four kinds a run reports, in the words a person reads them in. A model reads what it is
// sent and writes what it answers, and the cache is those same two verbs against the store the
// session keeps; so the kinds are read, write, cache read and cache write, and each one stands as its own
// figure wherever a run's cost is said out loud. They are never added into a single figure
// standing for the lot: which of them the reader is asking about is the reader's question, and a figure
// that answered it for them would be this module deciding which traffic counts.
//
// The console says the same words from its own copy of them: it reads the archive itself and
// cannot import this module, so the two say the same thing in two places, and a test holds them
// to it.

// One figure as it stands on a line: the word for what it counts, and the count. A kind the rows
// say nothing about carries no word at all, only a dash — a word in front of a dash would put a
// name to a figure that is not there, and a nought would claim the run burnt none.
export interface TokenFigure {
  label: string;
  text: string;
}

// How a caller says a number. A shell dump shortens to thousands and millions and a report marks
// the thousands, so the words are settled here and the counting stays with whoever prints it.
export type SayTokens = (tokens: number) => string;

const NOTHING: TokenFigure = { label: "", text: "—" };

function figure(label: string, tokens: number | null, say: SayTokens): TokenFigure {
  return tokens === null ? NOTHING : { label, text: say(tokens) };
}

// The four, in the order a run makes them: what went in, what came back, and the cache behind
// both.
export function kindFigures(split: TokenSplit, say: SayTokens): TokenFigure[] {
  return [
    figure(spanWords.kinds.input, split.inputTokens, say),
    figure(spanWords.kinds.output, split.outputTokens, say),
    figure(spanWords.kinds.cache_read, split.cacheReadTokens, say),
    figure(spanWords.kinds.cache_write, split.cacheWriteTokens, say),
  ];
}

// One figure as a single run of text, for a line rather than a layout.
export function figureText(one: TokenFigure): string {
  return one.label === "" ? one.text : `${one.label} ${one.text}`;
}

// The four as one line, which is what a shell has.
export function kindsText(split: TokenSplit, say: SayTokens): string {
  return kindFigures(split, say).map(figureText).join(" · ");
}

// What the account saw go past, whichever kinds it divided into. It stands with the kinds rather
// than in place of them: the kinds are the answer to what a run cost, and this is the answer to
// how much traffic there was.
export function totalText(tokens: number, say: SayTokens): string {
  return fillMarks(spanWords.total, { tokens: say(tokens) });
}

