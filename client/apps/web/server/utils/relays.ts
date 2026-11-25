// The streams this app is relaying to browsers right now, so that a stop of this app can say so
// on each of them before it lets go. A browser reads a stream that ends after this word as a
// restart rather than a fault, the same word the bot itself sends when it stops on purpose.

export const STOPPING_FRAME = "event: stopping\ndata: {}\n\n";

export interface OpenRelay {
  // Writes text into the stream the browser is reading.
  write: (text: string) => void;
  // Ends the stream and lets go of what it holds upstream.
  stop: () => void;
}

const open = new Set<OpenRelay>();

// Holds a relay for as long as it is open; the answer lets go of it again.
export function holdRelay(relay: OpenRelay): () => void {
  open.add(relay);

  return () => {
    open.delete(relay);
  };
}

export function openRelayCount(): number {
  return open.size;
}

// Every open relay is told this app is stopping and then ended. A relay that fails either step
// is one whose browser has already gone, and the rest are told all the same.
export function stopRelays(): void {
  const told = [...open];

  open.clear();

  for (const relay of told) {
    try {
      relay.write(STOPPING_FRAME);
    } catch {
      // Its reader left already.
    }

    try {
      relay.stop();
    } catch {
      // Nothing is left to let go of.
    }
  }
}
