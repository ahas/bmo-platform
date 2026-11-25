// A question the CLI in a pane puts to a person in the middle of a turn, read off the screen, and
// the keys that answer it (`docs/agent-control.md`, 「A confirmation in the middle of a turn」).
//
// Claude Code keeps a few safety checks that ask a person even in bypass mode. The screen draws such
// a question below the last rule, as a few lines of what is asked and a numbered choice whose
// cursor row is marked; a `Yes` and a `No` row make it a permission question. Anything else that
// holds the pane blocked is a question this reading does not know, and what is kept of it is the
// screen it stood at.

// A question this reading knows is bounded on both sides — under the rule, above the choices — so
// what is between them is the question and not the screen, and it is what a person is asked about.
// A question it does not know has no bounds: what is kept then is the foot of the screen, which is
// evidence and never words to hand on (`docs/agent-control.md`, 「What a failed turn may say」). The
// two are different things and the type says so, so that nothing can hand the second on by reaching
// for a field that means the first.
export type PaneConfirmation =
  | {
      kind: "permission";
      // The question as a person reads it: the lines above the choices, without the key hints.
      text: string;
      // Whether the refusal row takes words of its own (`Tab to amend`).
      amendable: boolean;
    }
  | {
      kind: "unknown";
      // What stood at the foot of the screen, which is where a question stands. It tells one
      // standing question from the next and goes to the run's log; what a person and the run are
      // told names the state instead.
      screen: string;
      amendable: false;
    };

// What tells one standing question from the next. A question this reading knows is known by its own
// words; one it does not is known by the screen it stood at.
export function confirmationKey(asked: PaneConfirmation): string {
  return asked.kind === "permission" ? asked.text : asked.screen;
}

// What the caller decided. A refusal carries the words the run is given in the command's place;
// leaving it is for a person to answer in herdr, and the same question is not asked again.
export type ConfirmAnswer = { allow: true } | { allow: false; reason: string } | { leave: true };

const cursorMark = "❯";
const optionRow = /^\s*(?:❯\s*)?(\d+)\.\s+(.*?)\s*$/;
const hintRow = /Esc to cancel|Enter to confirm/;
// The line a question left standing counts down on, redrawn every second until the CLI denies the
// command itself. It is not part of the question: kept, it would make every look a new one.
const countdownRow = /\bwill automatically \w+ this request in \d+:\d{2}\b/;
// The longest an unknown question is kept at: the bottom of the screen is where a question stands.
const unknownRows = 15;

const isRule = (row: string): boolean => /^\s*─{10,}\s*$/.test(row);

// The screen as it is read: every row but the countdown.
const screenRows = (screen: string): string[] => screen.split("\n").filter((row) => !countdownRow.test(row));

function dialogRows(screen: string): string[] {
  const rows = screenRows(screen);
  let rule = -1;

  rows.forEach((row, at) => {
    if (isRule(row)) {
      rule = at;
    }
  });

  return rows.slice(rule + 1);
}

function options(rows: readonly string[]): { at: number; text: string; cursor: boolean }[] {
  return rows.flatMap((row, at) => {
    const match = optionRow.exec(row);

    return match === null ? [] : [{ at, text: match[2] as string, cursor: row.trimStart().startsWith(cursorMark) }];
  });
}

const tidy = (rows: readonly string[]): string =>
  rows
    .map((row) => row.trimEnd())
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();

// The question on the screen, or nothing where the screen holds no readable question yet.
export function confirmationOf(screen: string): PaneConfirmation | null {
  const rows = dialogRows(screen);
  const choices = options(rows);
  const amendable = rows.some((row) => /Tab to amend/.test(row));
  const permission = choices.some((one) => /^Yes\b/.test(one.text)) && choices.some((one) => /^No\b/.test(one.text));

  if (permission) {
    const first = choices[0]?.at ?? rows.length;
    const text = tidy(rows.slice(0, first).filter((row) => !hintRow.test(row)));

    return text === "" ? null : { kind: "permission", text, amendable };
  }

  const foot = tidy(
    screenRows(screen)
      .filter((row) => row.trim() !== "" && !isRule(row))
      .slice(-unknownRows),
  );

  return foot === "" ? null : { kind: "unknown", screen: foot, amendable: false };
}

// The keys that move the cursor onto the row whose choice starts with the given word, or nothing
// where the screen has no such row or no cursor.
export function choiceKeys(screen: string, choice: "Yes" | "No"): string[] | null {
  const choices = options(dialogRows(screen));
  const target = choices.find((one) => new RegExp(`^${choice}\\b`).test(one.text));
  const cursor = choices.find((one) => one.cursor);

  if (target === undefined || cursor === undefined) {
    return null;
  }

  const steps = choices.indexOf(target) - choices.indexOf(cursor);

  return Array.from({ length: Math.abs(steps) }, () => (steps > 0 ? "down" : "up"));
}

