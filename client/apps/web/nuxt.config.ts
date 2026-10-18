import { fileURLToPath } from "node:url";

import { parseYAML } from "confbox";

// The server's language pack (docs/language-pack.md), whose parts the console's own code reads where
// it shows a line a unit answers with as a code. A part is imported from its file under the server's
// `lang/`, read into a module holding the part's tree. The build runs on Node, where the runtime's own
// YAML import is not at hand, so the parser is the one Nuxt's own configuration loading carries. Only
// the server pack's files are read here: the console's pack is the i18n module's to read.
const serverPackFolder = fileURLToPath(new URL("../../../server/lang/", import.meta.url));
const serverLanguagePack = {
  name: "bmo-web-server-language-pack",
  enforce: "pre" as const,
  transform(code: string, id: string) {
    return id.startsWith(serverPackFolder) && id.endsWith(".yaml")
      ? { code: `export default ${JSON.stringify(parseYAML(code))};`, map: null }
      : null;
  }
};

// The base path every page and road stands under; the deployment's reverse proxy hands it on as it is
// (docs/web.md, 「Where it opens」).
const baseURL = process.env.BMO_WEB_BASE_URL || "/bmo/";

export default defineNuxtConfig({
  // The archive screen's bundle, which brings the bmo-ui layer under it; bmo-ui is named too, as the
  // base every screen of the console stands on (bmo-ui's `docs/boundary.md`, 「How a host takes it」;
  // bmo-artifact's `docs/layout.md`, 「The console plugin bundle」).
  extends: ["bmo-artifact-client", "bmo-ui"],
  modules: ["nuxt-auth-utils", "@nuxtjs/i18n"],
  compatibilityDate: "2025-09-01",
  devtools: { enabled: false },
  ssr: true,
  // A worker's standing dev console serves a copy under a path of its own and builds into a directory
  // of its own, so a typecheck in the same checkout does not rewrite what it is serving. Nothing named,
  // the console is what it always is.
  buildDir: process.env.BMO_WEB_BUILD_DIR || ".nuxt",
  app: {
    baseURL,
    head: {
      htmlAttrs: { lang: "ko" },
      meta: [
        // The chrome of an installed copy takes the console's own background.
        { name: "theme-color", content: "#18181b" },
        // The standard name is what Chromium reads; the Apple one stays for Safari's home screen.
        { name: "mobile-web-app-capable", content: "yes" },
        { name: "apple-mobile-web-app-capable", content: "yes" },
        { name: "apple-mobile-web-app-status-bar-style", content: "black-translucent" }
      ],
      link: [
        { rel: "manifest", href: `${baseURL}manifest.webmanifest` },
        // A tab is drawn at 16 or 32 pixels, each with a picture drawn for that size (docs/web.md,
        // 「The console's pictures」); the installed app takes its own from the manifest.
        { rel: "icon", type: "image/png", sizes: "16x16", href: `${baseURL}icons/favicon-16.png` },
        { rel: "icon", type: "image/png", sizes: "32x32", href: `${baseURL}icons/favicon-32.png` },
        { rel: "apple-touch-icon", sizes: "180x180", href: `${baseURL}icons/apple-touch-icon.png` }
      ]
    }
  },
  // The zone every time is shown in, the locale figures are written and letter case folded in, and the
  // dayjs locale dates' words are written in, all stated since the console renders on the server as well
  // as in the browser (bmo-ui's `docs/boundary.md`, 「What the host owes」). The dayjs locale is loaded
  // by the app's own plugin.
  appConfig: {
    bmoUi: { zone: "Asia/Seoul", locale: "ko-KR", dateLocale: "ko" }
  },
  // A dev server answers only the hosts it is told of: those `BMO_WEB_DEV_HOSTS` names, comma-separated.
  vite: {
    server: { allowedHosts: (process.env.BMO_WEB_DEV_HOSTS ?? "").split(",").filter((host) => host !== "") },
    plugins: [serverLanguagePack],
    build: {
      // Rolldown's notice of how much of a build its plugins took is off. Most of a Nuxt build is plugin
      // work by its nature, so the notice stands on every build and names nothing a change could act on.
      rolldownOptions: { checks: { pluginTimings: false } },
      // Every chunk past the default 500 kB is a library loaded only by the screen that needs it, the
      // code editor and the diagram renderer's layout engine among them. None can be cut smaller from
      // here, so the warning stands just above the largest of them.
      chunkSizeWarningLimit: 7000
    }
  },
  nitro: {
    // A deployment builds a release of its own beside the one being served and only then moves onto it,
    // because nitro splits the build by route and a running server reads a route's chunk off disk the
    // first time that route is asked for (docs/web.md, 「Running as a service」). Whoever is deploying
    // names the directory here; with nothing named the build writes where it always does.
    output: { dir: process.env.BMO_WEB_OUTPUT_DIR ?? ".output" },
    // The runtime's own modules stay imports, not bundled files.
    rollupConfig: { external: [/^bun:/], plugins: [serverLanguagePack] },
    typescript: { tsConfig: { compilerOptions: { types: ["@types/bun"] } } }
  },
  $production: {
    nitro: { preset: "bun" }
  },
  // One language, one pack: every word the console shows is a key into `i18n/locales/ko.yaml`, so a
  // wording fix is an edit to that file alone (docs/web.md, 「Wording and the language pack」).
  // `language` is what the module writes into the document's `lang`.
  i18n: {
    strategy: "no_prefix",
    defaultLocale: "ko",
    detectBrowserLanguage: false,
    locales: [{ code: "ko", language: "ko-KR", file: "ko.yaml" }]
  },
  ui: {
    // Registers ContentSearch and ContentSearchButton, which the sidebar's search is; the console has no
    // @nuxt/content, and the palette is handed its groups directly.
    content: true
  }
});
