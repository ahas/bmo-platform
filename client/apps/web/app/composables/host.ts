// What a console bundle's screen calls by name, as bmo-ui's console plugin contract lets it ask of its
// host (bmo-ui's `docs/console-bundles.md`, 「What a console owes」; the bundles' own `host/` folders name
// them): a text put on the clipboard, the markdown renderer, and a screen's kept tabs. Nuxt makes a
// project's composables importable by name, so these stand here rather than being imported per screen.
export { copyText } from "~/domain/copy";
export { loadMarkdownRenderer } from "~/domain/markdown-load";
export { useKeptTabs } from "~/domain/tab-state";
