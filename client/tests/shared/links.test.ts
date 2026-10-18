import { describe, expect, test } from "bun:test";

import { linkParts, messageLinks } from "../../apps/web/shared/links";
import KO from "../fixtures/shared/links.test.ko.json";

describe("the addresses in a message", () => {
  test("an address runs to the next space and leaves the sentence's punctuation behind", () => {
    expect(linkParts(KO.line_7)).toEqual([
      { text: KO.line_8, href: null },
      { text: "https://a.example/x?y=1", href: "https://a.example/x?y=1" },
      { text: KO.line_10, href: null },
      { text: "https://b.example/p_(q)", href: "https://b.example/p_(q)" },
      { text: KO.line_12, href: null }
    ]);
  });

  test("a markdown link's address is the pressable part and the preview's, with nothing after its parentheses", () => {
    const text = KO.line_17;

    expect(linkParts(text)).toEqual([
      { text: KO.line_20, href: null },
      { text: "https://bmo.example.com/bmo/archive/83", href: "https://bmo.example.com/bmo/archive/83" },
      { text: KO.line_22, href: null }
    ]);
    expect(messageLinks(text)).toEqual(["https://bmo.example.com/bmo/archive/83"]);
  });

  test("words with no address are one plain run, and a scheme alone is no address", () => {
    expect(linkParts(KO.line_28)).toEqual([{ text: KO.line_28, href: null }]);
    expect(linkParts(KO.line_29)).toEqual([{ text: KO.line_29, href: null }]);
    expect(linkParts("")).toEqual([]);
  });

  test("the previews take each address once, in order, up to the limit", () => {
    const text = "https://a.example https://b.example https://a.example https://c.example https://d.example";

    expect(messageLinks(text)).toEqual(["https://a.example", "https://b.example", "https://c.example"]);
    expect(messageLinks(text, 1)).toEqual(["https://a.example"]);
    expect(messageLinks(KO.line_38)).toEqual([]);
  });

  test("an address inside code gets no preview, and one in the words around it keeps its own", () => {
    const text = [
      KO.line_43,
      "",
      "```shell",
      'curl -s "https://code.example/api?x=1"',
      "```",
      "",
      KO.line_49,
      "",
      KO.line_51,
      "| --- | --- |",
      KO.line_53,
      KO.line_54
    ].join("\n");

    expect(messageLinks(text)).toEqual(["https://words.example/docs_(a)", "https://linked.example/p", "https://cell-words.example"]);
    expect(messageLinks(KO.line_58)).toEqual(["https://a.example"]);
  });
});
