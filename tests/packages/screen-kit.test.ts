import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

// The screen kit (docs/person-nodes.md, 「Personal screens」): an app's `screen/` built with bmo-ui's bundle build on
// the release the kit vendors, into the `screen/` of the folder `BMO_TOOL_OUT` names, named after the app, its words
// beside the module and the libraries a screen shares with its console left as imports; words naming no title are
// refused. The build runs here as a place's build runs it, the app's folder its working folder, unconfined.

const root = resolve(import.meta.dir, "..", "..");
const kit = join(root, "screen-kit");
const vendored = JSON.parse(readFileSync(join(kit, "package.json"), "utf8")).dependencies["bmo-ui"] as string;

function app(words: Record<string, unknown>): { folder: string; out: string } {
  const scratch = mkdtempSync(join(tmpdir(), "screen-kit-"));
  const folder = join(scratch, "weekly_notes");
  const screen = join(folder, "screen");

  mkdirSync(screen, { recursive: true });
  writeFileSync(join(folder, "tool.toml"), '[tool]\nname = "weekly_notes"\nprotocol = 1\nrun = ["python3", "main.py"]\n');
  writeFileSync(
    join(screen, "entry.ts"),
    'import Notes from "./Notes.vue";\n\nexport default { routes: [{ path: "/my/weekly-notes", name: "my-weekly-notes", component: Notes }] };\n'
  );
  writeFileSync(
    join(screen, "Notes.vue"),
    '<script setup lang="ts">\nimport { useI18n } from "vue-i18n";\nimport UButton from "@nuxt/ui/components/Button.vue";\n\nconst { t } = useI18n();\n</script>\n\n<template>\n  <UButton :label="t(\'my-weekly-notes.title\')" />\n</template>\n'
  );
  writeFileSync(join(screen, "words.json"), JSON.stringify(words));

  return { folder, out: join(scratch, "out") };
}

function built(folder: string, out: string) {
  return Bun.spawnSync(["bun", join(kit, "build.mjs")], { cwd: folder, env: { ...process.env, BMO_TOOL_OUT: out }, stderr: "pipe" });
}

describe("the screen kit", () => {
  test("builds an app's screen as a bundle named after the app, on the release it vendors", () => {
    const { folder, out } = app({ en: { "my-weekly-notes": { title: "Weekly notes" } } });

    try {
      const ran = built(folder, out);

      expect(ran.exitCode, ran.stderr.toString()).toBe(0);

      const module = readFileSync(join(out, "screen", "module.js"), "utf8");
      const layer = JSON.parse(readFileSync(join(out, "screen", "layer.json"), "utf8")).layer as string;

      expect(vendored).toBe(`file:vendor/bmo-ui-${layer}.tgz`);
      expect(module).toContain('from "vue"');
      expect(module).toContain('from "@nuxt/ui/components/Button.vue"');
      expect(module).toContain("/my/weekly-notes");
      expect(JSON.parse(readFileSync(join(out, "screen", "words.json"), "utf8"))).toEqual({ en: { "my-weekly-notes": { title: "Weekly notes" } } });
    } finally {
      rmSync(resolve(folder, ".."), { recursive: true, force: true });
    }
  }, 120_000);

  test("refuses words that name no title under the app's screen", () => {
    const { folder, out } = app({ en: { "my-weekly-notes": { heading: "Weekly notes" } } });

    try {
      const ran = built(folder, out);

      expect(ran.exitCode).not.toBe(0);
      expect(ran.stderr.toString()).toContain("names no my-weekly-notes.title");
      expect(existsSync(join(out, "screen", "module.js"))).toBe(false);
    } finally {
      rmSync(resolve(folder, ".."), { recursive: true, force: true });
    }
  }, 60_000);
});
