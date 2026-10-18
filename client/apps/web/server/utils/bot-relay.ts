import { STREAM_KEEPALIVE_MS } from "#shared/config";

import { onConsoleEvent } from "./console-events";
import { unitAt, unitHeaders } from "./units";
import { holdRelay } from "./relays";
import { consoleFrame, KEEPALIVE_FRAME, RelayOutput } from "./sse-frames";

import type { ConsoleEvent } from "./console-events";

// What one relay passes on: which of the bot's whole events, and which of the console's own. The
// console's stream passes all of both; a listener's stream passes only what it plays and stops.
// A relay may also write frames of its own worked out of the bot's: `derived` makes one reader
// for each stream, which sees every whole bot event — passed on or not — and answers with the
// frames to write after it.
export interface RelayFilter {
  frame?: (frame: string) => boolean;
  consoleEvent?: (event: ConsoleEvent) => boolean;
  derived?: () => (frame: string) => string[];
}

// The receiver's live stream from a cursor, the run and the numbered events it states (the receiver's
// `docs/boundary-api.md`, 「The receiver's own routes」), relayed event by event, with this app's own
// frames — the keepalive every few seconds, the word that it is stopping, the console's events —
// written between them. The server this app runs on drops a request that says nothing for long enough
// and the receiver says nothing until it has something to say, so the keepalive is a frame the page
// hears: a page that has heard nothing for several of them knows its connection is gone. A receiver
// that is not there ends the stream, and the reader opens it again on its own.
export function relayBotStream(after: number, run: string | null, aborted: AbortController, filter: RelayFilter = {}): ReadableStream<Uint8Array> {
  let keepalive: ReturnType<typeof setInterval> | null = null;
  let reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
  let release: (() => void) | null = null;
  let unlisten: (() => void) | null = null;

  function stop(): void {
    release?.();
    release = null;
    unlisten?.();
    unlisten = null;

    if (keepalive !== null) {
      clearInterval(keepalive);
      keepalive = null;
    }

    void reader?.cancel().catch(async () => undefined);
    reader = null;
    aborted.abort();
  }

  return new ReadableStream<Uint8Array>({
    start(stream) {
      function write(chunk: Uint8Array): void {
        try {
          stream.enqueue(chunk);
        } catch {
          stop();
        }
      }

      // Everything written here is whole: the bot's events only once all of each has come, and
      // this app's own frames at any moment between them.
      const output = new RelayOutput(write, filter.frame, filter.derived?.());

      keepalive = setInterval(() => output.own(KEEPALIVE_FRAME), STREAM_KEEPALIVE_MS);
      // Held while it is open, so a stop of this app is said on it before it ends.
      release = holdRelay({ write: (text) => output.own(text), stop });
      unlisten = onConsoleEvent((event) => {
        if (filter.consoleEvent?.(event) ?? true) {
          output.own(consoleFrame(event));
        }
      });

      void (async () => {
        try {
          const unit = unitAt("RECEIVER");

          if (unit === null) throw new Error("the receiver's address is not set");

          const asked = `/events?after=${after}${run === null ? "" : `&run=${encodeURIComponent(run)}`}`;
          const upstream = await fetch(`${unit.address}${asked}`, {
            headers: { ...unitHeaders(unit), accept: "text/event-stream" },
            signal: aborted.signal
          });

          // A receiver that answered and refused is a fault of this pair's settings — most often a
          // token that no longer matches — and the reader sees only a stream that ends and opens
          // again. So it is written here, where the reason is known. A 503 is the receiver not ready,
          // which is no refusal.
          if (!upstream.ok || upstream.body === null) {
            if (upstream.status !== 503) console.error(`events: the receiver refused the stream this app relays (${upstream.status})`);
            throw new Error(String(upstream.status));
          }

          reader = upstream.body.getReader();

          for (;;) {
            const { done, value } = await reader.read();

            if (done) {
              break;
            }

            if (value !== undefined) {
              output.upstream(value);
            }
          }
        } catch {
          // The bot is not there, it let go, or it refused and that was written above; either way
          // there is nothing left to relay.
        }

        stop();

        try {
          stream.close();
        } catch {
          // Already closed by the reader leaving.
        }
      })();
    },
    cancel() {
      stop();
    }
  });
}
