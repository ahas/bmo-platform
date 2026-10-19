import { versioned } from "#shared/shell-version";
import { consoleName } from "#shared/agent-names";
import { wording } from "#shared/wording";

// The installed app's manifest. Its name and the line that says what it is are words, so they come
// out of the language pack; everything else is the app's shape and stands here as it is.
const ICONS = [
  { src: "/bmo/icons/icon-192.png", sizes: "192x192", type: "image/png", purpose: "any" },
  { src: "/bmo/icons/icon-512.png", sizes: "512x512", type: "image/png", purpose: "any" },
  { src: "/bmo/icons/icon-maskable-192.png", sizes: "192x192", type: "image/png", purpose: "maskable" },
  { src: "/bmo/icons/icon-maskable-512.png", sizes: "512x512", type: "image/png", purpose: "maskable" }
];

export const MANIFEST_TYPE = "application/manifest+json; charset=utf-8";

// Each icon's address carries the shell version, so an installed app reading the manifest sees a
// redrawn icon as an icon at another address and takes it again.
export function appManifest(shellVersion: string): Record<string, unknown> {
  return {
    name: consoleName(wording, "app.label.manifest_name"),
    short_name: consoleName(wording, "app.label.manifest_name"),
    description: wording("app.message.manifest_description"),
    lang: "ko",
    dir: "ltr",
    start_url: "/bmo/",
    scope: "/bmo/",
    display: "standalone",
    orientation: "portrait-primary",
    // The console's dark ground, the colour the page's own theme colour gives the chrome, so the
    // splash an installed app opens on shows the icon's mint shape on the console's ground rather
    // than a screen washed in mint (docs/web.md, 「Installing on a phone」).
    background_color: "#18181b",
    theme_color: "#18181b",
    icons: ICONS.map((icon) => ({ ...icon, src: versioned(icon.src, shellVersion) }))
  };
}
