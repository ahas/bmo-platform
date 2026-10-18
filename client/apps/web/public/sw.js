// The console's service worker. It exists so an installed copy has one and so its own icons come
// off the disk instead of the network. It caches nothing else: this console shows what is happening
// right now, and a cached answer is a wrong answer. Pages, the manifest and API calls are never
// served from here, so a session is exactly what the server says it is.

const ADDRESS = new URL(self.location.href).searchParams;

// The shell version the window registering this worker names in its address, a hash of every icon
// file's bytes (client/modules/shell-version.ts). The cache is named after it and every icon
// address carries it, so redrawn icons are a new address, a new worker and a new cache, and the
// worker taking over drops every cache an older one kept.
const SHELL = ADDRESS.get("shell") ?? "";
const CACHE = `bmo-chat-shell-${SHELL}`;

function shellFile(path) {
  return SHELL === "" ? path : `${path}?v=${SHELL}`;
}

// The two pictures a notification carries. The large one is the console's own icon, shown beside
// the text. The small one is what Android draws in the status bar, and the platform paints every
// opaque pixel of it in one colour of its own choosing: a full-colour square handed over there
// arrives as a solid white block, so the small one is a silhouette whose shape is its alpha.
const NOTIFICATION_ICON = shellFile("/bmo/icons/icon-192.png");

// The small picture is drawn for a status bar showing one, two and three device pixels to a CSS
// pixel, 24, 48 and 72 pixels square, and one drawn for another density lands its edges between
// the screen's pixels. The window registering this worker names its screen's density in the
// worker's address, as a whole number rounded up, so the picture taken is the one drawn for that
// density, or the next finer one, which the platform only ever shrinks.
function badgeFor(density) {
  const drawn = Math.min(3, Math.max(1, Math.ceil(Number(density) || 1)));

  return shellFile(`/bmo/icons/badge-${24 * drawn}.png`);
}

const NOTIFICATION_BADGE = badgeFor(ADDRESS.get("density"));

// What may be kept: the icons the installed copy is drawn with, each under an address carrying the
// shell version, so the same address always means the same bytes. The scripts and the styles are
// not among them: the browser's own cache already holds them by their hashed names, and a script
// kept here is a script that can be handed back for a name that now means something else. The
// manifest is not among them either: its words come out of the language pack, which no icon's
// version follows. Everything under the API, and every page, is left to the network.
function isShell(url) {
  return url.origin === self.location.origin && url.pathname.startsWith("/bmo/icons/");
}

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll([NOTIFICATION_ICON, NOTIFICATION_BADGE]))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) => Promise.all(names.filter((name) => name !== CACHE).map((name) => caches.delete(name))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;

  if (request.method !== "GET" || request.mode === "navigate") {
    return;
  }

  const url = new URL(request.url);

  if (!isShell(url)) {
    return;
  }

  event.respondWith(
    caches.match(request).then(
      (hit) =>
        hit ??
        fetch(request).then((response) => {
          if (response.ok && response.type === "basic") {
            const copy = response.clone();

            void caches.open(CACHE).then((cache) => cache.put(request, copy));
          }

          return response;
        })
    )
  );
});

// A notification the console pushed. It carries the one line to read and where pressing it
// goes; nothing is fetched here, because a service worker woken for a push has no session of
// its own to fetch with and the notification is meant to say enough on its own.
//
// Nothing rings for what a screen is already showing. The console keeps a live stream open in
// every open tab and puts the news on the screen itself, so a second bell for something already
// in front of them is noise — but only for that screen: a visible tab on the siren page knows
// nothing of a question standing in the queue's panel, and that one still rings.
function samePlace(clientUrl, target) {
  try {
    const open = new URL(clientUrl);
    const want = new URL(target, open.origin);

    return open.pathname === want.pathname;
  } catch {
    return false;
  }
}

function alreadyWatched(clients, target) {
  return clients.some((client) => client.visibilityState === "visible" && samePlace(client.url, target));
}

self.addEventListener("push", (event) => {
  if (!event.data) {
    return;
  }

  let payload;

  try {
    payload = event.data.json();
  } catch {
    return;
  }

  event.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then((clients) => {
      if (alreadyWatched(clients, payload.url)) {
        return undefined;
      }

      return self.registration.showNotification(payload.title, {
        body: payload.body,
        tag: payload.tag,
        // A repeat about the same thing replaces the one already there without ringing again.
        renotify: false,
        icon: NOTIFICATION_ICON,
        badge: NOTIFICATION_BADGE,
        data: { url: payload.url }
      });
    })
  );
});

// Pressing one goes where the notification points (docs/web.md, 「Notifications」). A console
// already open is brought forward and moved there rather than opened a second time, so pressing
// three notifications does not leave three consoles behind. A window installed as an app is not
// always one this worker controls, and `navigate` on such a window is refused; the window is then
// focused and told where to go over a message, and a window that answers neither is opened fresh.
function inScope(client) {
  try {
    return new URL(client.url).pathname.startsWith("/bmo/");
  } catch {
    return false;
  }
}

async function openTarget(target) {
  const clients = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
  const open = clients.find(inScope) ?? null;

  if (open !== null) {
    try {
      const focused = (await open.focus()) ?? open;

      if ("navigate" in focused) {
        await focused.navigate(target);

        return;
      }
    } catch {
      // The window refused to be moved from here; it is told instead, and opens the place itself.
    }

    try {
      open.postMessage({ type: "bmo:open", url: target });

      return;
    } catch {
      // Nothing of the open window can be reached, so a new one is opened below.
    }
  }

  await self.clients.openWindow(target);
}

self.addEventListener("notificationclick", (event) => {
  const target = (event.notification.data && event.notification.data.url) || "/bmo/";

  event.notification.close();
  event.waitUntil(openTarget(target));
});

// The rules above are plain functions so they can be read back and exercised without a browser.
self.pushRules = { samePlace, alreadyWatched, inScope, badgeFor };
self.shellRules = { cache: CACHE, notificationIcon: NOTIFICATION_ICON, isShell };
