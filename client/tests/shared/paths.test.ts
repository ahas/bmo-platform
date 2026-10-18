import { describe, expect, test } from "bun:test";

import { todoOfQuery } from "../../apps/web/shared/paths";

describe("the item an address names", () => {
  test("reads only an exact safe positive query id", () => {
    expect(todoOfQuery("363")).toBe(363);
    expect(todoOfQuery("0")).toBeNull();
    expect(todoOfQuery("-1")).toBeNull();
    expect(todoOfQuery("12tail")).toBeNull();
    expect(todoOfQuery(" 12")).toBeNull();
    expect(todoOfQuery(["12", "13"])).toBeNull();
    expect(todoOfQuery(String(Number.MAX_SAFE_INTEGER + 1))).toBeNull();
  });
});
