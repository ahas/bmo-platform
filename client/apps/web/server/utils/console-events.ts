// The console's own events, as opposed to the bot's: what this server learns on its own account —
// mail arriving in one of its mailboxes — and every open tab should hear (the mail unit's docs/mail.md, 「Reaching
// the open tabs」). A named event carries a small payload and no cursor; a tab that missed one while
// its stream was closed learns the state again when it reopens, so nothing here is kept or replayed.

export interface ConsoleEvent {
  name: string;
  data: unknown;
}

const listeners = new Set<(event: ConsoleEvent) => void>();

export function publishConsoleEvent(name: string, data: unknown): void {
  for (const listener of listeners) {
    try {
      listener({ name, data });
    } catch {
      // One listener's fault is no reason to keep the event from the others.
    }
  }
}

export function onConsoleEvent(listener: (event: ConsoleEvent) => void): () => void {
  listeners.add(listener);

  return () => {
    listeners.delete(listener);
  };
}
