import { appManifest, MANIFEST_TYPE } from "../utils/app-manifest";

// The manifest at the address the page's link and the service worker name, with the type a
// browser installs an app from.
export default defineEventHandler((event) => {
  setResponseHeader(event, "Content-Type", MANIFEST_TYPE);
  setResponseHeader(event, "Cache-Control", "public, max-age=0");

  // The shell-version module states the version as a string as the build stands.
  const { shellVersion } = useRuntimeConfig(event).public as { shellVersion: string };

  return JSON.stringify(appManifest(shellVersion), null, 2);
});
