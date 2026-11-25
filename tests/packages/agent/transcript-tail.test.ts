import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { appendFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { scratchDir } from "@agency/test-support/scratch";
import { transcriptRecords } from "@agency/agent/transcript-tail";

function file(text: string): string {
  const path = join(scratchDir("bmo-transcript-tail-"), "session.jsonl");

  writeFileSync(path, text);

  return path;
}

describe("transcriptRecords", () => {
  test("answers every JSON line, passing over what is not JSON and an unfinished last line", () => {
    const path = file('{"n":1}\nnot json\n\n{"n":2}\n{"n":');

    expect(transcriptRecords(path)).toEqual([{ n: 1 }, { n: 2 }]);
  });

  test("a look after more was written reads the new lines and keeps the earlier ones", () => {
    const path = file('{"n":1}\n{"n":');

    expect(transcriptRecords(path)).toEqual([{ n: 1 }]);
    appendFileSync(path, '2}\n{"n":3}\n');
    expect(transcriptRecords(path)).toEqual([{ n: 1 }, { n: 2 }, { n: 3 }]);
  });

  test("a last line that already parses is counted without its newline, and not twice once it comes", () => {
    const path = file('{"n":1}\n{"n":2}');

    expect(transcriptRecords(path)).toEqual([{ n: 1 }, { n: 2 }]);
    appendFileSync(path, '\n{"n":3}\n');
    expect(transcriptRecords(path)).toEqual([{ n: 1 }, { n: 2 }, { n: 3 }]);
  });

  test("a file written over under the same path is read again from its start", () => {
    const path = file('{"n":1}\n{"n":2}\n');

    expect(transcriptRecords(path)).toHaveLength(2);
    writeFileSync(path, '{"m":1}\n{"m":2}\n{"m":3}\n');
    expect(transcriptRecords(path)).toEqual([{ m: 1 }, { m: 2 }, { m: 3 }]);
    writeFileSync(path, '{"k":1}\n');
    expect(transcriptRecords(path)).toEqual([{ k: 1 }]);
  });

  test("a look hands back its own copy, which a later look does not grow", () => {
    const path = file('{"n":1}\n');
    const first = transcriptRecords(path);

    appendFileSync(path, '{"n":2}\n');
    transcriptRecords(path);
    expect(first).toEqual([{ n: 1 }]);
  });

  test("a path with no file answers nothing, and a file made there later is read whole", () => {
    const path = join(file(""), "..", "later.jsonl");

    expect(transcriptRecords(path)).toEqual([]);
    writeFileSync(path, '{"n":1}\n');
    expect(transcriptRecords(path)).toEqual([{ n: 1 }]);
  });

  test("text split across looks inside a multibyte character reads whole", () => {
    const path = file("");
    // Korean, since a Hangul syllable is three bytes and the cut below falls inside one.
    const line = Buffer.from('{"t":"h€llo"}\n');

    appendFileSync(path, line.subarray(0, 8));
    expect(transcriptRecords(path)).toEqual([]);
    appendFileSync(path, line.subarray(8));
    expect(transcriptRecords(path)).toEqual([{ t: "h€llo" }]);
  });
});
