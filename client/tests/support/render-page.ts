import { plugin } from "bun";
import { join } from "node:path";
import { compileScript, compileTemplate, parse } from "vue/compiler-sfc";
import { createSSRApp } from "vue";
import { renderToString } from "vue/server-renderer";

import type { Component } from "vue";

// A console page rendered the way the server renders it: its single-file component compiled by Vue's own
// compiler and rendered to HTML by Vue's server renderer. Nuxt's own components are stood in for by the
// components a test hands in, and `$t` reads the console's pack, so what a page says is read as a person
// would read it. A component's `~/` and `#shared/` imports resolve through the app's own tsconfig, as every
// other import from the app's files does under `bun test`.

const APP = join(import.meta.dir, "..", "..", "apps", "web");

plugin({
  name: "console single-file components",
  setup(build) {
    build.onLoad({ filter: /\.vue$/ }, async (args) => {
      const source = await Bun.file(args.path).text();
      const { descriptor, errors } = parse(source, { filename: args.path });

      if (errors.length > 0) throw errors[0];

      const id = Bun.hash(args.path).toString(16);

      if (descriptor.script === null && descriptor.scriptSetup === null) {
        const template = compileTemplate({ source: descriptor.template!.content, filename: args.path, id });

        return { contents: `${template.code}\nexport default { render };`, loader: "ts" };
      }

      const script = compileScript(descriptor, { id, inlineTemplate: true });

      return { contents: script.content, loader: "ts" };
    });
  }
});

/** The app's file at a path under `apps/web/app/`. */
export function appFile(...parts: string[]): string {
  return join(APP, "app", ...parts);
}

/** The HTML a page renders to, with the components it uses that are not its own handed in by name. */
export async function renderPage(file: string, components: Readonly<Record<string, Component>>, translate: (key: string) => string): Promise<string> {
  const page = ((await import(file)) as { default: Component }).default;
  const app = createSSRApp(page);

  for (const [name, component] of Object.entries(components)) app.component(name, component);
  app.config.globalProperties.$t = translate;

  return await renderToString(app);
}
