// A personal app's screen built as a console bundle (docs/person-nodes.md, 「Personal screens」): the app's
// `screen/` built with bmo-ui's bundle build, on the release this kit vendors, into the `screen/` of the folder
// `BMO_TOOL_OUT` names. The bundle's name, which is its words' namespace, is `my-` and the app's name with each `_`
// written `-`; its words are the app's `screen/words.json`, written beside the module; its routes are the default
// export of `screen/entry.ts`. What the console shares stays an import of the package's own path, which the console
// resolves to its copy, so the screen draws with the console's components and calls through its relay.
//
// Run as `bun /kit/build.mjs` inside the confined build, the app's folder its working folder; `BMO_SCREEN_SOURCE`
// names another app folder where it is run elsewhere.

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import vue from "@vitejs/plugin-vue";
import { consoleBundle } from "bmo-ui/bundle/console-bundle.js";
import { build } from "vite";

const source = process.env.BMO_SCREEN_SOURCE ?? process.cwd();
const into = process.env.BMO_TOOL_OUT;

if (into === undefined || into === "") {
  console.error("BMO_TOOL_OUT names no folder to build the screen into");
  process.exit(2);
}

const tool = Bun.TOML.parse(readFileSync(join(source, "tool.toml"), "utf8")).tool?.name;

if (typeof tool !== "string" || tool === "") {
  console.error("tool.toml names no tool");
  process.exit(2);
}

const name = `my-${tool.replaceAll("_", "-")}`;
const screen = join(source, "screen");
const words = JSON.parse(readFileSync(join(screen, "words.json"), "utf8"));
const out = join(into, "screen");

for (const [locale, tree] of Object.entries(words)) {
  if (typeof tree?.[name]?.title !== "string") {
    console.error(`screen/words.json: the locale ${locale} names no ${name}.title, the name the screen is shown by`);
    process.exit(2);
  }
}

await build({
  root: screen,
  configFile: false,
  cacheDir: join(process.env.TMPDIR ?? "/tmp", "screen-kit-cache"),
  logLevel: "warn",
  plugins: [consoleBundle({ name, words }), vue({ isProduction: true })],
  define: {
    "import.meta.server": "false",
    "import.meta.client": "true",
    "import.meta.dev": "false",
    "process.env.NODE_ENV": JSON.stringify("production"),
    __VUE_OPTIONS_API__: "true",
    __VUE_PROD_DEVTOOLS__: "false",
    __VUE_PROD_HYDRATION_MISMATCH_DETAILS__: "false"
  },
  build: {
    outDir: out,
    emptyOutDir: true,
    target: "es2022",
    minify: true,
    sourcemap: false,
    copyPublicDir: false,
    cssCodeSplit: false,
    lib: {
      entry: join(screen, "entry.ts"),
      formats: ["es"],
      fileName: () => "module.js",
      cssFileName: "style"
    }
  }
});

writeFileSync(join(out, "words.json"), `${JSON.stringify(words)}\n`);
