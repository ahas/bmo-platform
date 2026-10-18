import { expect, test } from "bun:test";
import { createError, isError } from "h3";

import { wording } from "../../../apps/web/shared/wording";
import { appManifest } from "../../../apps/web/server/utils/app-manifest";
import { fail } from "../../../apps/web/server/utils/fail";

import { questionPush } from "../../../apps/web/server/utils/pushnews";

import type { JobQuestion } from "../../../apps/web/shared/types";

// The server names a key and answers with its words. What leaves it — the message a refused
// request carries, the lines a notification rings with, the manifest an installed app reads —
// is the pack's Korean, in the shape it always had, and never the key (docs/web.md).

(globalThis as Record<string, unknown>).createError = createError;

// The error a route throws is h3's own, and its message is what the server's error answer puts in
// the body the console reads as `data.message`.
test("a refused request carries the pack's sentence, filled in, as its message", () => {
  const refused = fail(400, wording("common.label.count", { n: 3000 }));

  expect(isError(refused)).toBe(true);
  expect(refused).toMatchObject({ statusCode: 400, message: wording("common.label.count", { n: 3000 }) });
  expect(refused.message).toContain("3000");
  expect(refused.message).not.toContain("{");
});

test("a question's notification rings with the pack's title", () => {
  const question = { id: 7, todoId: 90, question: "what should be the default" } as JobQuestion;

  expect(questionPush(question, null).title).toBe(wording("push.title.question", { id: 90 }));
  expect(questionPush(question, null, "Get push notifications").title).toBe(wording("push.title.question_titled", { id: 90, title: "Get push notifications" }));
});


test("the manifest takes its name and its line from the pack and keeps the rest of its shape", () => {
  const manifest = appManifest("0123456789ab");

  expect(manifest.name).toBe("BMO Chat");
  expect(manifest.short_name).toBe("BMO Chat");
  expect(manifest.description).toBe(wording("app.message.manifest_description"));
  expect(manifest).toMatchObject({ lang: "ko", start_url: "/bmo/", scope: "/bmo/", display: "standalone", theme_color: "#18181b", background_color: "#18181b" });
  expect(manifest.icons).toHaveLength(4);
});
