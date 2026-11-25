import { appManifest, MANIFEST_TYPE } from "../utils/app-manifest";

// The manifest at the address the page's link and the service worker name, with the type a
// browser installs an app from.
export default defineEventHandler((event) => {
  setResponseHeader(event, "Content-Type", MANIFEST_TYPE);
  setResponseHeader(event, "Cache-Control", "public, max-age=0");

  // The shell-version module states the version as a string as the build stands; the base path is
  // the one the console was built under.
  const config = useRuntimeConfig(event);
  const { shellVersion } = config.public as { shellVersion: string };

  return JSON.stringify(appManifest(config.app.baseURL, shellVersion), null, 2);
});
