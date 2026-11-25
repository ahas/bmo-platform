import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { defineNuxtModule } from "nuxt/kit";

import { versioned } from "../shared/shell-version";

// The installed app's shell version (docs/web.md, 「Installing on a phone」).
//
// An installed copy takes its icons by address, and the service worker keeps them by address, so an
// icon redrawn under the same address reaches neither: the worker keeps handing back the bytes it
// cached, and uninstalling the app clears neither the worker nor its cache. So the build reads every
// icon file into one version, and every address naming the shell carries it — the page's links here,
// the manifest's icons and the service worker's, both through the public runtime config.

declare module "nuxt/schema" {
  interface PublicRuntimeConfig {
    shellVersion: string;
  }
}

// The version of a set of files: their names and bytes, in name order, hashed. Any file added,
// removed, renamed or changed gives another one.
export function shellVersion(files: { name: string; bytes: Uint8Array }[]): string {
  const hash = createHash("sha256");

  for (const file of [...files].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
    hash.update(file.name).update("\0").update(file.bytes).update("\0");
  }

  return hash.digest("hex").slice(0, 12);
}

function iconFiles(dir: string): { name: string; bytes: Uint8Array }[] {
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => ({ name: entry.name, bytes: readFileSync(join(dir, entry.name)) }));
}

export default defineNuxtModule({
  meta: { name: "bmo-shell-version" },
  setup(_options, nuxt) {
    const version = shellVersion(iconFiles(join(nuxt.options.rootDir, "public", "icons")));

    nuxt.options.runtimeConfig.public.shellVersion = version;
    // Every file the page's head links to on its own origin is part of the shell: the manifest and the
    // tab's and home screen's pictures.
    nuxt.options.app.head.link = (nuxt.options.app.head.link ?? []).map((link) =>
      typeof link.href === "string" && link.href.startsWith("/") ? { ...link, href: versioned(link.href, version) } : link
    );
  }
});
