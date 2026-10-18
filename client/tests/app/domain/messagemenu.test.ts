import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";

import { messageMenu } from "../../../apps/web/app/domain/messagemenu";

import type { CopyItem } from "../../../apps/web/app/domain/room-copy";
import type { MessageMenuActions } from "../../../apps/web/app/domain/messagemenu";
import KO from "../../fixtures/app/domain/messagemenu.test.ko.json";

function item(text: string): CopyItem {
  return { senderName: "Owner", senderId: 10000001, messageId: 7, sentAt: 1_757_000_000, text, judgements: [] };
}

function spies(): MessageMenuActions & { called: string[] } {
  const called: string[] = [];

  return {
    called,
    reply: () => called.push("reply"),
    copy: () => called.push("copy"),
    copyBody: () => called.push("copyBody"),
    copyRef: () => called.push("copyRef"),
    select: () => called.push("select")
  };
}

describe("the menu on one message", () => {
  test("offers the five in the order they are read: answering, taking, choosing", () => {
    const groups = messageMenu(wording, item(KO.line_29), spies());

    expect(groups.map((group) => group.map((entry) => entry.label))).toEqual([
      [KO.line_32],
      [KO.line_33, KO.line_33_2, KO.line_33_3],
      [KO.line_34]
    ]);
  });

  test("each entry does its own thing", () => {
    const actions = spies();

    for (const entry of messageMenu(wording, item(KO.line_41), actions).flat()) {
      entry.onSelect?.(new Event("select"));
    }

    expect(actions.called).toEqual(["reply", "copy", "copyBody", "copyRef", "select"]);
  });

  test("turns the words alone off where there are no words, and nothing else", () => {
    const empty = messageMenu(wording, item(""), spies()).flat();
    const said = messageMenu(wording, item(KO.line_50), spies()).flat();

    expect(empty.map((entry) => entry.disabled === true)).toEqual([false, false, true, false, false]);
    expect(said.every((entry) => entry.disabled !== true)).toBe(true);
  });
});
