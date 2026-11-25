import { TRANSLATION_PART_TOKENS, shownPart, textLanguage, translationParts } from "./translation-parts";

import type { DocumentLanguage, DocumentTranslationFailure } from "./types";

// How the console cuts a text for the translator and joins what comes back. Two texts go through it: a
// document the viewer shows, which is markdown, and a work order's plan or step result, which is either
// prose or the JSON a builder turn answers in. The translator behind both is one, so only the cutting
// differs.
//
// Nothing here touches the disk or the network: the server cuts a text into the parts it hands over,
// the browser joins the parts that came back, and both read the same rules from here. Which language a
// text is in and how prose is cut stand in `translation-parts` and are handed on from here.

export { TRANSLATION_PART_TOKENS, shownPart, textLanguage, translationParts };

export function translationLanguage(language: DocumentLanguage): DocumentLanguage {
  return language === "en" ? "ko" : "en";
}

// What the translator's unit said of the last part it could not have translated, with the road it was asked on;
// empty where it reports nothing.
export function translationFailureLine(failure: DocumentTranslationFailure | null | undefined): string {
  if (failure === null || failure === undefined) {
    return "";
  }

  const road = [failure.provider, failure.model].filter((part) => part !== null).join(" · ");

  return `${failure.reason}${road === "" ? "" : ` (${road})`}`;
}

// A text's parts and the way back: the parts are handed to the translator in this order, and joining
// them — each one translated or still in the original — gives a text of the same shape as the one cut,
// which is what the screen draws.
export interface TranslationCut {
  parts: string[];
  join(translated: ReadonlyArray<string | null>): string;
}

// A JSON answer read as data, a fence around it allowed; null where the text is prose. This is how a
// builder turn's answer is told apart from everything else, and it reads the text the same way the
// console's own reading of that answer does.
export function parsedAnswer(text: string): Record<string, unknown> | null {
  const trimmed = text.trim().replace(/^```(?:json)?\s*/, "").replace(/\s*```$/, "");

  if (!trimmed.startsWith("{")) {
    return null;
  }

  try {
    const data: unknown = JSON.parse(trimmed);

    return data !== null && typeof data === "object" && !Array.isArray(data) ? (data as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

// Whether a string in a JSON answer is something a person reads rather than something the machine
// keys on. A role, a model, an id, a path — the values that must survive the round trip untouched —
// are single words, and a sentence is not; so a string is prose when it has a space in it and a
// letter beside it. Nothing here names a field of any one answer, so an answer of a shape nobody has
// written yet is cut by the same rule.
function isProse(text: string): boolean {
  const trimmed = text.trim();

  return /\s/.test(trimmed) && /\p{L}/u.test(trimmed);
}

type Place = Array<string | number>;

function proseStrings(value: unknown, at: Place, into: Array<{ place: Place; text: string }>): void {
  if (Array.isArray(value)) {
    value.forEach((one, index) => proseStrings(one, [...at, index], into));

    return;
  }

  if (value !== null && typeof value === "object") {
    for (const [key, one] of Object.entries(value)) {
      proseStrings(one, [...at, key], into);
    }

    return;
  }

  if (typeof value === "string" && isProse(value)) {
    into.push({ place: at, text: value });
  }
}

function writeAt(data: unknown, place: Place, text: string): void {
  let held = data as Record<string | number, unknown>;

  for (const step of place.slice(0, -1)) {
    held = held[step] as Record<string | number, unknown>;
  }

  held[place[place.length - 1] as string | number] = text;
}

// A JSON answer cut at the sentences inside it: every string a person reads becomes parts of its own,
// and joining writes the translations back into their places and serialises the answer again. What
// comes back is JSON of the same shape, so whatever reads the answer reads the translation the same
// way, and a string still waiting stands in its original.
function answerCut(data: Record<string, unknown>, budget: number): TranslationCut {
  const found: Array<{ place: Place; text: string }> = [];

  proseStrings(data, [], found);

  const slots = found.map((one) => ({ place: one.place, pieces: translationParts(one.text, budget) }));

  return {
    parts: slots.flatMap((slot) => slot.pieces),
    join(translated) {
      const written = structuredClone(data);
      let at = 0;

      for (const slot of slots) {
        const whole = slot.pieces.map((piece, index) => shownPart(piece, translated[at + index] ?? null)).join("");

        at += slot.pieces.length;
        writeAt(written, slot.place, whole);
      }

      return JSON.stringify(written, null, 2);
    }
  };
}

// How one text is cut and put back together: a builder turn's JSON answer at the sentences inside it,
// anything else at its headings and paragraphs.
export function translationCut(text: string, budget = TRANSLATION_PART_TOKENS): TranslationCut {
  const answer = parsedAnswer(text);

  if (answer !== null) {
    return answerCut(answer, budget);
  }

  const parts = translationParts(text, budget);

  return {
    parts,
    join: (translated) => parts.map((part, index) => shownPart(part, translated[index] ?? null)).join("")
  };
}
