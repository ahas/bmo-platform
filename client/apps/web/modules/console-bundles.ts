import { existsSync, readFileSync, readdirSync } from "node:fs";
import { basename, join } from "node:path";
import { pathToFileURL } from "node:url";

import { defineNuxtModule } from "nuxt/kit";

import type { ConsoleBundleView } from "../shared/console-bundles";
import type { ConsoleBundle } from "bmo-ui/shared/console-bundle";

// The console's bundles (bmo-ui's `docs/console-bundles.md`): every layer the console extends that
// declares itself with a `console-bundle.json` is a console bundle, and the console attaches it by
// that declaration alone — its screen, the service relayed under the relay base, its words, and the
// event kinds a notice is raised for. Nothing here names a bundle or a service: a bundle is whatever
// its declaration says it is.

interface LoadedBundle extends ConsoleBundle {
  // The package's root, where its words stand.
  root: string;
}

interface RegisteredWords {
  [key: string]: unknown;
}

declare module "nuxt/schema" {
  interface AppConfig {
    consoleBundles: ConsoleBundleView[];
  }

  interface RuntimeConfig {
    consoleBundles: ConsoleBundleView[];
    consoleBundleWords: Record<string, RegisteredWords>;
  }
}

function wordsOf(root: string, languagePack: string): RegisteredWords {
  const folder = join(root, languagePack);
  const words: RegisteredWords = {};

  for (const file of readdirSync(folder).filter((name) => name.endsWith(".json"))) {
    words[basename(file, ".json")] = JSON.parse(readFileSync(join(folder, file), "utf8")) as unknown;
  }

  return words;
}

// The layer roots the configuration resolved, each bundle's package root among them. A bundle is a
// layer that carries the declaration at its root; a host that names the same screen or the same
// bundle twice is refused rather than attached over the other.
function bundlesOf(roots: readonly string[], declarationFile: string, read: (value: unknown) => ConsoleBundle): LoadedBundle[] {
  const found: LoadedBundle[] = [];

  for (const root of roots) {
    const file = join(root, declarationFile);

    if (!existsSync(file)) continue;

    const readBundle = read(JSON.parse(readFileSync(file, "utf8")) as unknown);

    for (const other of found) {
      if (other.name === readBundle.name) throw new Error(`Two console bundles are named ${readBundle.name}; the one at ${root} is not attached.`);
      if (other.screenPath === readBundle.screenPath) throw new Error(`Two console bundles stand at ${readBundle.screenPath}; the one at ${root} is not attached.`);
    }

    found.push({ ...readBundle, root });
  }

  return found;
}

interface RegisteredLocale {
  code: string;
  language: string;
  file: string;
}

// What the console carries of each declaration: what a screen, the sidebar, the relay and a notice
// read. The words themselves stay in the bundle's pack, which the i18n module merges; the notice
// ringer reads them from the private runtime configuration, since it writes them on the server.
export default defineNuxtModule({
  meta: { name: "bmo-console-bundles" },
  async setup(_options, nuxt) {
    const roots = (nuxt.options._layers ?? []).map((layer) => layer.config.rootDir);
    // bmo-ui's own contract module, read from the layer's folder: a host's package resolution is not
    // guaranteed to reach a layer's internal path by name.
    const contractRoot = roots.find((root) => existsSync(join(root, "shared", "console-bundle.ts")));

    if (contractRoot === undefined) throw new Error("The bmo-ui layer is not among the console's layers.");

    const contract = (await import(pathToFileURL(join(contractRoot, "shared", "console-bundle.ts")).href)) as {
      CONSOLE_BUNDLE_FILE: string;
      consoleBundle: (value: unknown) => ConsoleBundle;
    };
    const bundles = bundlesOf(roots, contract.CONSOLE_BUNDLE_FILE, contract.consoleBundle);

    const declarations = bundles.map(({ root: _root, ...declaration }) => declaration);

    nuxt.options.appConfig.consoleBundles = declarations;
    nuxt.options.runtimeConfig.consoleBundles = declarations;
    nuxt.options.runtimeConfig.consoleBundleWords = Object.fromEntries(bundles.map((bundle) => [bundle.name, wordsOf(bundle.root, bundle.languagePack)]));

    // Every bundle's words are merged into the host's pack by the locale's own code. The hook is the
    // i18n module's, so this module stands before it in the configuration's `modules`.
    nuxt.hook("i18n:registerModule" as never, ((add: (locale: { langDir: string; locales: RegisteredLocale[] }) => void) => {
      for (const bundle of bundles) {
        const langDir = join(bundle.root, bundle.languagePack);
        const locales = readdirSync(langDir)
          .filter((name) => name.endsWith(".json"))
          .map((file) => {
            const code = basename(file, ".json");

            return { code, language: code === "ko" ? "ko-KR" : code, file };
          });

        add({ langDir, locales });
      }
    }) as never);
  }
});
