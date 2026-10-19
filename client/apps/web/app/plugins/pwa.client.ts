import { consoleBase, routeOf } from "#shared/paths";

// The service worker is registered once the page is up, so it never competes with the first
// paint. A browser without one simply does not install the console, which costs the console
// nothing: everything it shows comes from the network anyway.
//
// It also speaks back: a notification pressed while this window is open may reach the worker as a
// window it does not control, which it cannot move by itself, so it says where to go instead and
// the window carries itself there. The address it sends is the browser's own, base and all, while
// the router works below that base — so the base is taken off before the router is handed it.
//
// The worker is told the screen's density in its address, so a notification carries the status-bar
// picture drawn for that density; a window on a screen of another density registers it again. It is
// told the shell version too (client/modules/shell-version.ts), so a build with redrawn icons
// registers a worker at another address, which replaces the one keeping the old icons.
//
// The worker stands at the console's base path, the one the configuration names, with that path as
// its scope; the worker reads its base back from that scope (docs/web.md, 「Installing on a phone」).
export default defineNuxtPlugin(() => {
  if (!import.meta.client || !("serviceWorker" in navigator)) {
    return;
  }

  const router = useRouter();
  const config = useRuntimeConfig();
  const { shellVersion } = config.public;
  const base = consoleBase(config.app.baseURL);

  navigator.serviceWorker.addEventListener("message", (event: MessageEvent) => {
    const said = event.data as { type?: unknown; url?: unknown } | null;

    if (said === null || typeof said !== "object" || said.type !== "bmo:open" || typeof said.url !== "string") {
      return;
    }

    void router.push(routeOf(base, said.url));
  });

  const register = (): void => {
    navigator.serviceWorker.register(`${base}sw.js?density=${Math.ceil(window.devicePixelRatio)}&shell=${shellVersion}`, { scope: base }).catch((error: unknown) => {
      console.error("service worker registration failed:", error);
    });
  };

  // A page reached by a link inside the console is already loaded by the time this runs, and
  // that load event is never coming again.
  if (document.readyState === "complete") {
    register();
  } else {
    window.addEventListener("load", register, { once: true });
  }
});
