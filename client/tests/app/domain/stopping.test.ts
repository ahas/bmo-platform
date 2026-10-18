import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";
import { computed, ref } from "vue";
import KO from "../../fixtures/app/domain/stopping.test.ko.json";

// What a stop does on a screen, held apart from any screen. Three of these are the ways the first
// pattern let a reader down — a refusal that went away with the section, a question left locked on
// a call that never came back, and nothing saying whose stop was out — so each is read here rather
// than by looking at a screen.

Object.assign(globalThis, { computed, ref });

const { STOP_WAIT_MS, stopKey, useStops } = await import("../../../apps/web/app/domain/stopping");

const runner = { kind: "unit", id: "bmo-search-9" } as const;
const turn = { kind: "execution", id: "bmo-execution-turn" } as const;

function held(): { resolve: (value?: unknown) => void; reject: (error: unknown) => void; promise: Promise<never> } {
  let resolve!: (value?: unknown) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<never>((yes, no) => {
    resolve = yes as (value?: unknown) => void;
    reject = no;
  });

  return { resolve, reject, promise };
}

const settle = async (): Promise<void> => { await new Promise((resolve) => setTimeout(resolve, 0)); };

describe("the key a row is known by", () => {
  test("is the target in the shape every road writes it", () => {
    expect(stopKey(runner)).toBe("unit:bmo-search-9");
    expect(stopKey(turn)).toBe("execution:bmo-execution-turn");
  });
});

describe("asking before stopping", () => {
  test("one question stands at a time, and cancelling closes it without asking the bot", async () => {
    const asked: unknown[] = [];
    const stops = useStops(wording, async (target) => void asked.push(target));

    stops.ask(runner);
    expect(stops.isAsking(runner)).toBe(true);

    stops.ask(turn);
    expect(stops.isAsking(runner)).toBe(false);
    expect(stops.isAsking(turn)).toBe(true);

    stops.cancel();
    expect(stops.asking.value).toBeNull();
    expect(asked).toEqual([]);
  });
});

describe("a stop that was asked and not confirmed", () => {
  test(KO.line_57, async () => {
    const stops = useStops(wording, async () => ({ state: "asked" }));

    stops.ask(runner);
    await stops.stop(runner);

    expect(stops.isStopping(runner)).toBe(true);
    expect(stops.isStopping(turn)).toBe(false);
    // The question is answered, so it closes; what stands in its place is the word on the row.
    expect(stops.asking.value).toBeNull();

    // Still drawn, so still stopping.
    stops.seen([stopKey(runner)]);
    expect(stops.isStopping(runner)).toBe(true);

    // Gone from the picture: the run ended, and the word goes with it.
    stops.seen([]);
    expect(stops.isStopping(runner)).toBe(false);
  });

  test("a stop the bot says it confirmed leaves no word: the row is already gone", async () => {
    const stops = useStops(wording, async () => ({ state: "confirmed" }));

    await stops.stop(runner);
    expect(stops.isStopping(runner)).toBe(false);
  });
});

describe("while a stop is out", () => {
  test("the row whose stop it is says so, and the others are held rather than pressed too", async () => {
    const waiting = held();
    const stops = useStops(wording, async () => await waiting.promise);
    const sent = stops.stop(runner);

    await settle();
    expect(stops.isSending(runner)).toBe(true);
    expect(stops.isSending(turn)).toBe(false);
    expect(stops.busy()).toBe(true);

    // A second press while the first is out is not a second call.
    const asked: unknown[] = [];
    await useStops(wording, async (target) => void asked.push(target)).stop(turn);

    await stops.stop(turn);
    expect(stops.isSending(turn)).toBe(false);

    waiting.resolve({ state: "asked" });
    await sent;
    expect(stops.busy()).toBe(false);
  });
});

describe("what goes wrong", () => {
  test("a refusal stays on the row it was pressed on, and no other row wears it", async () => {
    const stops = useStops(wording, async () => { throw { data: { message: KO.line_111 } }; });

    stops.ask(runner);
    await stops.stop(runner);

    expect(stops.errorOf(runner)).toBe(KO.line_116);
    expect(stops.errorOf(turn)).toBe("");
    // The question stays open on a refusal: the reader may try again or leave it.
    expect(stops.isAsking(runner)).toBe(true);
    expect(stops.busy()).toBe(false);

    // The row is still drawn, so what it was told stays with it — switching sections draws other
    // rows and never touches this one's.
    stops.seen([stopKey(runner), stopKey(turn)]);
    expect(stops.errorOf(runner)).toBe(KO.line_125);

    // Asking again clears what the last press was told.
    stops.ask(runner);
    expect(stops.errorOf(runner)).toBe("");
  });

  test("a call that never comes back gives the row up on its own and says so", async () => {
    const waiting = held();
    const stops = useStops(wording, async () => await waiting.promise, 10);

    stops.ask(runner);
    await stops.stop(runner);

    expect(stops.busy()).toBe(false);
    expect(stops.isSending(runner)).toBe(false);
    expect(stops.errorOf(runner)).toContain(KO.line_141);
    // Nothing is claimed about the run: it was never answered for.
    expect(stops.isStopping(runner)).toBe(false);
  });

  test("a row the picture no longer draws takes its refusal and its question with it", async () => {
    const stops = useStops(wording, async () => { throw new Error("no"); });

    stops.ask(runner);
    await stops.stop(runner);
    expect(stops.errorOf(runner)).toBe(wording("live.error.stop_failed"));

    stops.seen([]);
    expect(stops.errorOf(runner)).toBe("");
    expect(stops.asking.value).toBeNull();
  });
});

describe("the wait", () => {
  test("is long enough for a stop the bot answers and short enough to give a row back", () => {
    expect(STOP_WAIT_MS).toBeGreaterThanOrEqual(10_000);
    expect(STOP_WAIT_MS).toBeLessThanOrEqual(60_000);
  });
});
