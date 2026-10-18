import { describe, expect, test } from "bun:test";

// What a call's fold holds. It is a log, so what the fold stands is the call laid out to be
// read — every argument under its own name — and the record as it was stored is a press further
// in rather than the first thing in the reader's face. Which element holds what, and which
// tokens each wears, is what a rendered screen would name as neither, so the rule is read off
// the file that ships.

const source = await Bun.file(new URL("../../../apps/web/app/components/AgentLogRow.vue", import.meta.url)).text();
const template = source.slice(0, source.indexOf("<script"));

// One fold whole, from its opening tag to the closing one that answers it. The folds nest, so
// the tags are counted rather than searched for: the first closing tag after a call's fold is
// the original's, not the call's.
function foldAt(mark: string): string {
  const at = template.indexOf(mark);

  expect(at).toBeGreaterThan(-1);

  const opened = template.lastIndexOf("<UChatTool", at);
  const tags = /<UChatTool|<\/UChatTool>/g;

  tags.lastIndex = opened;

  let depth = 0;
  let tag: RegExpExecArray | null;

  while ((tag = tags.exec(template)) !== null) {
    depth += tag[0].startsWith("</") ? -1 : 1;

    if (depth === 0) {
      return template.slice(opened, tags.lastIndex);
    }
  }

  throw new Error(`fold never closes: ${mark}`);
}

// The call's own fold, and not the result's below it: the result's fold prints its line straight
// as it should, and reading the whole template would let that <pre> answer for a question it is
// not the answer to.
const call = foldAt("line.kind === 'tool'");

// The original, folded once more inside the call's own fold, and what the call's fold stands
// before anyone opens it. Both are read where they are asserted on rather than once at the head
// of the file: a fold that has gone missing then fails the test that says it should be there,
// instead of bringing the whole file down before a single test has been named.
function original(): string {
  return foldAt(`:text="$t('common.label.original')"`);
}

function reading(): string {
  return call.replace(original(), "");
}

describe("what a call's fold stands", () => {
  test("there is a template to read, so nothing below passes on an empty string", () => {
    expect(template.length).toBeGreaterThan(500);
    expect(call).toContain("<UChatTool");
    expect(original()).toContain("<UChatTool");
    expect(reading().length).toBeLessThan(call.length);
  });

  test("the arguments, each under the name it was sent under", () => {
    expect(reading()).toContain('v-for="(argument, index) in fold.args"');
    expect(reading()).toContain("{{ argument.name }}");
    expect(reading()).toContain("{{ argument.value }}");
  });

  test("and not the stored text, which is no longer the first thing read", () => {
    expect(reading()).not.toContain("{{ line.text }}");
  });

  test("a value clipped for the fold says so in the row's own word", () => {
    expect(reading()).toContain('v-if="argument.cut"');
    expect(reading()).toContain("{{ CUT }}");
  });

  // The two happen together on the very line where the writer's cap fell: the last value is
  // clipped and the record stops. One word for both would read as the screen stuttering, so the
  // fold says each in its own words and every place that reports the writer's cut says the same
  // one thing.
  test("a record that stops short says it in words of its own, not in the value's", () => {
    expect(reading()).toContain('v-if="line.truncated || !fold.whole"');
    expect(reading()).toContain("{{ SHORT }}");

    const words = (name: string): string => new RegExp(`const ${name} = t\\("([^"]+)"\\)`).exec(source)?.[1] ?? "";

    expect(words("CUT")).not.toBe("");
    expect(words("SHORT")).not.toBe("");
    expect(words("CUT")).not.toBe(words("SHORT"));
    // And nowhere on this row does the writer's cut wear the clipped value's word.
    expect(template).not.toMatch(/truncated"[^>]*>\{\{ CUT \}\}/);
  });

  test("the value keeps the newlines it was written with", () => {
    expect(reading()).toContain("whitespace-pre-wrap");
  });
});

describe("the stored text", () => {
  test("is still reachable, behind a fold of its own", () => {
    expect(original()).toContain("{{ line.text }}");
    expect(original()).toContain(`:text="$t('common.label.original')"`);
  });
});

describe("what the fold wears", () => {
  test("nothing of a colour this row does not already read in", () => {
    expect(call).not.toMatch(/text-(primary|secondary|error|warning|success|info)/);
    expect(call).not.toMatch(/bg-(primary|secondary|error|warning|success|info)/);
  });
});

// Every line that is not a call — a prompt among them — stands in the branch after the call's
// fold: words as markdown, a record as the characters it was stored as. A prompt is a record, so
// what that branch gives a record is what a prompt is shown as.
describe("a line that is not a call", () => {
  const rest = template.slice(template.indexOf('<template v-else>'), template.indexOf("</template>", template.indexOf('<template v-else>')));

  test("stands whole as it was stored when it is not words", () => {
    expect(rest).toContain('<MarkdownText v-if="look.words"');
    expect(rest).toMatch(/<pre v-else class="[^"]*font-mono[^"]*"/);
    expect(rest).toContain("{{ line.text }}");
  });

  test("wraps inside the log rather than being clamped, folded or widening it", () => {
    const record = /<pre v-else class="([^"]*)"/.exec(rest)?.[1] ?? "";

    expect(record).toContain("whitespace-pre-wrap");
    expect(record).toContain("break-words");
    expect(rest).not.toContain("line-clamp");
    expect(rest).not.toContain("<UChatTool");
  });
});

// What a turn was handed is a line of this kind too, and it takes the same row as everything else:
// the badge carries the word its own sort has, and the piece it names stands as it was stored.
describe("what a turn was handed", () => {
  const rest = template.slice(template.indexOf("<template v-else>"), template.indexOf("</template>", template.indexOf("<template v-else>")));

  test("wears its badge off the one look every line of this screen takes", () => {
    expect(template).toContain(':label="look.label"');
    expect(template).toContain(':color="look.color"');
    expect(source).toContain("lineLook(t, line.value)");
  });

  test("stands as the record it is rather than inside a call's fold", () => {
    expect(call).toContain("line.kind === 'tool'");
    expect(rest).toContain("{{ line.text }}");
  });
});

// A result is drawn one of two ways, and which one is decided by whether it carries a change.
describe("a call's results", () => {
  const results = template.slice(template.indexOf('v-for="result in results"'));

  test("a result that carries a change reads as its diffs under its summary, the tool's words one press in", () => {
    const diff = foldAt('v-if="result.changes !== null"');

    expect(diff).toContain(':text="result.changes.summary"');
    expect(diff).toContain('v-for="(file, index) in result.changes.files"');
    expect(diff).toContain('<AgentLogDiff :view="file" />');
    expect(diff).toContain("{{ result.changes.more }}");
    expect(diff).toContain(`:text="$t('common.label.original')"`);
    expect(diff).toContain("{{ result.line.text }}");
  });

  test("a shell command's files, and a patch of several, each stand under their own head", () => {
    const diff = foldAt('v-if="result.changes !== null"');

    expect(diff).toContain('v-if="file.byCommand || result.changes.files.length > 1"');
    expect(diff).toContain("{{ file.head }}");
    // A command keeps its own line; only a file tool's change heads the call.
    expect(source).toContain(".find((file) => !file.byCommand)");
  });

  test("any other result keeps its length, its first line and its whole text behind the fold", () => {
    const plain = foldAt("v-else\n        :text=\"resultLabel(result.line)\"");

    expect(plain).toContain(':suffix="preview(result.line).head"');
    expect(plain).toContain("{{ result.line.text }}");
    expect(plain).not.toContain("AgentLogDiff");
  });

  test("a call whose result drew a change is headed by that change", () => {
    expect(call).toContain(":text=\"changed !== null ? changed.head");
    expect(results.length).toBeGreaterThan(0);
  });
});

// The diff itself: a line's numbers, its mark and its text, and a long line wrapping in place.
const diff = await Bun.file(new URL("../../../apps/web/app/components/AgentLogDiff.vue", import.meta.url)).text();

describe("the diff", () => {
  test("stands in monospace with numbers that line up", () => {
    expect(diff).toContain("font-mono");
    expect(diff.match(/tabular-nums/g)?.length).toBe(2);
  });

  test("wraps a long line instead of scrolling the log sideways", () => {
    expect(diff).toContain("break-words whitespace-pre-wrap");
    expect(diff).not.toContain("overflow-x");
  });

  test("an added line and a removed one wear the success and error tones", () => {
    expect(diff).toContain('"bg-success/10"');
    expect(diff).toContain('"bg-error/10"');
  });
});
