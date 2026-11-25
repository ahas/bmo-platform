// The supervisors as their rows declare them (docs/supervisors.md, 「One runner」): the scope of each one's
// session and the shape of its turns. Every reader that needs to know how a supervisor talks reads it here,
// so nothing chooses a path by a supervisor's name or by a kind of its own.

// Where a supervisor's session stands: one across every room it answers, each turn carrying its room's
// block, or one in each room it answers.
export type SessionScope = "agent" | "room";

// What a supervisor's turn is made of: the chat documents, the window's round prompt and the re-asks after
// an answer, or its settings' pieces, its new words and no re-ask.
export type TurnShape = "window" | "pieces";

/** A supervisor's settings, as its row holds them. */
export interface Conversation {
  scope: SessionScope;
  shape: TurnShape;
}

// The columns of a supervisors row the settings are read from; the number is PostgreSQL's bigint as the
// driver hands it back.
export interface ConversationColumns {
  session_scope: string | null;
  turn_shape: string | null;
}

/** The settings a row holds, or null where its scope is none the runner carries. */
export function conversationOf(row: ConversationColumns): Conversation | null {
  const scope = scopeNamed(row.session_scope);

  if (scope === null) return null;

  return {
    scope,
    shape: shapeNamed(row.turn_shape) ?? (scope === "agent" ? "window" : "pieces"),
  };
}

function scopeNamed(word: string | null | undefined): SessionScope | null {
  return word === "agent" || word === "room" ? word : null;
}

function shapeNamed(word: string | null | undefined): TurnShape | null {
  return word === "window" || word === "pieces" ? word : null;
}
