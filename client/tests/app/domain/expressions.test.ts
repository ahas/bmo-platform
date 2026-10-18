import { expect, test } from "bun:test";
import { expressionParts } from "../../../apps/web/app/domain/expressions";
import KO from "../../fixtures/app/domain/expressions.test.ko.json";
const attachment = { id: 1, kind: "sticker" as const, name: "emoji.webp", bytes: 4, type: "image/webp", available: true };
test("custom emoji uses UTF-16 offsets without losing escaped text or adjacent characters", () => {
  const parts = expressionParts(KO.line_5, [{ offset: 6, length: 4, attachment }]);
  expect(parts.map(p => p.text)).toEqual([KO.line_6, "👍🏽", KO.line_6_2]);
  expect(parts[1]?.attachment).toBe(attachment);
});
test("invalid and overlapping entities cannot drop source text", () => {
  const parts = expressionParts("😀x", [{ offset: -1, length: 1, attachment }, { offset: 0, length: 2, attachment }, { offset: 1, length: 2, attachment }, { offset: 3, length: 99, attachment }]);
  expect(parts.map(p => p.text).join("")).toBe("😀x");
  expect(parts.filter(p => p.attachment)).toHaveLength(1);
});
