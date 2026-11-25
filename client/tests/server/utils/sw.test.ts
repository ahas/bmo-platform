import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { shellVersion } from "../../../apps/web/modules/shell-version";
import { appManifest } from "../../../apps/web/server/utils/app-manifest";

// The service worker is a plain script the browser loads on its own, so it is read here as the
// browser reads it — from the file that ships — and its rules are exercised against a stand-in
// for the one thing it asks of the browser: which console windows are open and which of them
// the owner is actually looking at.

interface WindowClient {
  url: string;
  visibilityState: string;
}

interface Rules {
  samePlace: (clientUrl: string, target: string) => boolean;
  alreadyWatched: (clients: WindowClient[], target: string) => boolean;
  inScope: (client: { url: string }) => boolean;
  badgeFor: (density: string | null) => string;
}

// What a click does with the windows it finds is the browser's to trigger and the worker's to
// decide, so the handler is taken off the file the same way and run against stand-in windows.
interface ShellRules {
  base: string;
  cache: string;
  notificationIcon: string;
  isShell: (url: URL) => boolean;
}

interface Loaded {
  rules: Rules;
  shell: ShellRules;
  clicked: (event: unknown) => void;
  setClients: (clients: unknown) => void;
}

// The worker as the window registers it: at the console's base, with that base as its scope.
async function serviceWorker(href = "https://bmo.example.com/bmo/sw.js?density=2", scope = new URL("./", href).href): Promise<Loaded> {
  const source = await Bun.file(new URL("../../../apps/web/public/sw.js", import.meta.url)).text();
  const listeners = new Map<string, (event: unknown) => void>();
  const self: {
    addEventListener: (name: string, handler: (event: unknown) => void) => void;
    pushRules?: Rules;
    shellRules?: ShellRules;
    location: { origin: string; href: string };
    registration: { scope: string };
    clients?: unknown;
  } = {
    addEventListener: (name, handler) => void listeners.set(name, handler),
    location: { origin: "https://bmo.example.com", href },
    registration: { scope }
  };

  new Function("self", "caches", "fetch", source)(self, undefined, undefined);

  const clicked = listeners.get("notificationclick");

  if (self.pushRules === undefined || self.shellRules === undefined || clicked === undefined) {
    throw new Error("the service worker no longer publishes its rules or its click");
  }

  return {
    rules: self.pushRules,
    shell: self.shellRules,
    clicked,
    setClients: (clients) => {
      self.clients = clients;
    }
  };
}

const worker = await serviceWorker();
const rules = worker.rules;

describe("what counts as the same place", () => {
  test("the path is the place, whatever the address carries besides", () => {
    expect(rules.samePlace("https://bmo.example.com/bmo/rooms/1?todo=90", "/bmo/rooms/1")).toBe(true);
    expect(rules.samePlace("https://bmo.example.com/bmo/rooms/1#note", "/bmo/rooms/1?todo=7")).toBe(true);
  });

  test("another screen of the console is another place", () => {
    expect(rules.samePlace("https://bmo.example.com/bmo/siren", "/bmo/rooms/1")).toBe(false);
    expect(rules.samePlace("https://bmo.example.com/bmo/", "/bmo/rooms/1")).toBe(false);
  });

  test("an address that is not one says nothing rather than throwing", () => {
    expect(rules.samePlace("not an address", "/bmo/rooms/1")).toBe(false);
  });
});

describe("whether a notification is worth showing", () => {
  const target = "/bmo/rooms/10000001?todo=90";

  test("nothing rings for what they are looking at: that screen is already saying it", () => {
    expect(rules.alreadyWatched([{ url: "https://bmo.example.com/bmo/rooms/10000001", visibilityState: "visible" }], target)).toBe(
      true
    );
  });

  test("a tab left on that screen behind another window still rings", () => {
    expect(rules.alreadyWatched([{ url: "https://bmo.example.com/bmo/rooms/10000001", visibilityState: "hidden" }], target)).toBe(
      false
    );
  });

  test("a visible tab on another screen still rings: it knows nothing of this", () => {
    expect(rules.alreadyWatched([{ url: "https://bmo.example.com/bmo/siren", visibilityState: "visible" }], target)).toBe(false);
  });

  test("no console open at all rings, which is the whole point of it", () => {
    expect(rules.alreadyWatched([], target)).toBe(false);
  });
});

// Where a click lands. The browser fires this and the worker decides between the console window
// already open and a new one; both branches are run here, because the one thing a machine cannot
// do is press an operating system's own notification.
describe("what a click on a notification does", () => {
  interface Opened {
    navigated: string[];
    focused: number;
    told: unknown[];
    openedWindows: string[];
    closed: number;
  }

  // A window of the console, as the browser hands one over: it is focused first and then moved.
  // One installed as an app may be a window this worker does not control, which refuses to be
  // moved from here, so that window is a case of its own.
  async function click(urls: string[], data: unknown, movable = true, on: Loaded = worker): Promise<Opened> {
    const seen: Opened = { navigated: [], focused: 0, told: [], openedWindows: [], closed: 0 };
    const windows = urls.map((url) => {
      const window = {
        url,
        focus: () => {
          seen.focused += 1;

          return Promise.resolve(window);
        },
        navigate: (target: string) => {
          if (!movable) {
            return Promise.reject(new Error("this window is not controlled by this worker"));
          }

          seen.navigated.push(target);

          return Promise.resolve(window);
        },
        postMessage: (said: unknown) => void seen.told.push(said)
      };

      return window;
    });
    const waits: Promise<unknown>[] = [];

    on.setClients({
      matchAll: () => Promise.resolve(windows),
      openWindow: (target: string) => {
        seen.openedWindows.push(target);

        return Promise.resolve(undefined);
      }
    });

    on.clicked({
      notification: { data, close: () => void (seen.closed += 1) },
      waitUntil: (work: Promise<unknown>) => void waits.push(work)
    });

    await Promise.all(waits);

    return seen;
  }

  test("a console window already open is brought forward and sent to the place", async () => {
    const seen = await click(["https://bmo.example.com/bmo/rooms/1"], { url: "/bmo/rooms/42?todo=90" });

    expect(seen.navigated).toEqual(["/bmo/rooms/42?todo=90"]);
    expect(seen.focused).toBe(1);
    expect(seen.openedWindows).toEqual([]);
    expect(seen.closed).toBe(1);
  });

  test("a window this worker cannot move is told where to go instead", async () => {
    const seen = await click(["https://bmo.example.com/bmo/"], { url: "/bmo/mail/owner/inbox/%3Ca%40b%3E" }, false);

    expect(seen.focused).toBe(1);
    expect(seen.navigated).toEqual([]);
    expect(seen.told).toEqual([{ type: "bmo:open", url: "/bmo/mail/owner/inbox/%3Ca%40b%3E" }]);
    expect(seen.openedWindows).toEqual([]);
  });

  test("a window that is not the console is not the one to reuse", async () => {
    const seen = await click(["https://example.com/", "not a url at all"], { url: "/bmo/rooms/42" });

    expect(seen.navigated).toEqual([]);
    expect(seen.openedWindows).toEqual(["/bmo/rooms/42"]);
  });

  test("no console open at all opens one", async () => {
    const seen = await click([], { url: "/bmo/rooms/42" });

    expect(seen.openedWindows).toEqual(["/bmo/rooms/42"]);
    expect(seen.focused).toBe(0);
    expect(seen.closed).toBe(1);
  });

  test("a notification carrying no address goes to the console's own front door", async () => {
    expect((await click([], undefined)).openedWindows).toEqual(["/bmo/"]);
    expect((await click([], {})).openedWindows).toEqual(["/bmo/"]);
  });

  test("under another base path, the console's windows and its front door are that base's", async () => {
    const elsewhere = await serviceWorker("https://bmo.example.com/console/sw.js?density=2");

    expect((await click([], undefined, true, elsewhere)).openedWindows).toEqual(["/console/"]);

    const reused = await click(["https://bmo.example.com/bmo/rooms/1", "https://bmo.example.com/console/rooms/1"], { url: "/console/rooms/42" }, true, elsewhere);

    expect(reused.navigated).toEqual(["/console/rooms/42"]);
    expect(reused.openedWindows).toEqual([]);
  });
});

// The small icon Android draws in the status bar is painted in one colour of the platform's own,
// so it must be the silhouette rather than the console's own square: a picture with no alpha to
// read arrives there as a white block.
describe("the pictures a notification carries", () => {
  test("the small one is its own file and not the large one", async () => {
    const source = await Bun.file(new URL("../../../apps/web/public/sw.js", import.meta.url)).text();

    expect(worker.shell.notificationIcon).toBe("/bmo/icons/icon-192.png");
    expect(source).toContain("badge: NOTIFICATION_BADGE");
    expect(source).toContain("icon: NOTIFICATION_ICON");
  });

  test("the small one is the one drawn for the screen's density, or the next finer one", () => {
    expect(rules.badgeFor("1")).toBe("/bmo/icons/badge-24.png");
    expect(rules.badgeFor("2")).toBe("/bmo/icons/badge-48.png");
    expect(rules.badgeFor("3")).toBe("/bmo/icons/badge-72.png");
    expect(rules.badgeFor("2.625")).toBe("/bmo/icons/badge-72.png");
    expect(rules.badgeFor("4")).toBe("/bmo/icons/badge-72.png");
    // A worker registered without a density, or with nonsense, takes the coarsest.
    expect(rules.badgeFor(null)).toBe("/bmo/icons/badge-24.png");
    expect(rules.badgeFor("x")).toBe("/bmo/icons/badge-24.png");
  });

  test("the window names its density in the worker's address", async () => {
    const plugin = await Bun.file(new URL("../../../apps/web/app/plugins/pwa.client.ts", import.meta.url)).text();

    expect(plugin).toContain("${base}sw.js?density=${Math.ceil(window.devicePixelRatio)}");
  });

  test("every silhouette it may pick ships at its own size, and its shape is its alpha", async () => {
    for (const side of [24, 48, 72]) {
      const badge = new Uint8Array(await Bun.file(new URL(`../../../apps/web/public/icons/badge-${side}.png`, import.meta.url)).arrayBuffer());

      // The PNG header: the signature, then IHDR with the size and the colour type, 6 being RGBA.
      expect([...badge.slice(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
      expect(new DataView(badge.buffer).getUint32(16)).toBe(side);
      expect(new DataView(badge.buffer).getUint32(20)).toBe(side);
      expect(badge[25]).toBe(6);
    }
  });
});

describe("which windows are the console's", () => {
  test("a window under the console's own base, and nothing else", () => {
    expect(rules.inScope({ url: "https://bmo.example.com/bmo/mail/owner/inbox" })).toBe(true);
    expect(rules.inScope({ url: "https://bmo.example.com/bmo-dev/" })).toBe(false);
    expect(rules.inScope({ url: "not a url at all" })).toBe(false);
  });
});

// The base path is the console's configuration, so a console built under another one installs,
// registers and notifies under that one: the manifest is written under the base it is handed, the
// window registers the worker at that base with it as the scope, and the worker reads its base back
// off its scope.
describe("the install and push roads follow the configured base path", () => {
  for (const base of ["/bmo/", "/console/"]) {
    test(`under ${base} the manifest opens, scopes and draws itself there`, () => {
      const manifest = appManifest(base, "0123456789ab") as { start_url: string; scope: string; icons: { src: string }[] };

      expect(manifest.start_url).toBe(base);
      expect(manifest.scope).toBe(base);
      expect(manifest.icons.map((icon) => icon.src)).toEqual([
        `${base}icons/icon-192.png?v=0123456789ab`,
        `${base}icons/icon-512.png?v=0123456789ab`,
        `${base}icons/icon-maskable-192.png?v=0123456789ab`,
        `${base}icons/icon-maskable-512.png?v=0123456789ab`
      ]);
    });

    test(`under ${base} the worker names, keeps and claims only that base's`, async () => {
      const there = await serviceWorker(`https://bmo.example.com${base}sw.js?density=2&shell=0123456789ab`);
      const other = base === "/bmo/" ? "/console/" : "/bmo/";

      expect(there.shell.base).toBe(base);
      expect(there.shell.notificationIcon).toBe(`${base}icons/icon-192.png?v=0123456789ab`);
      expect(there.rules.badgeFor("2")).toBe(`${base}icons/badge-48.png?v=0123456789ab`);
      expect(there.shell.isShell(new URL(`https://bmo.example.com${base}icons/icon-192.png`))).toBe(true);
      expect(there.shell.isShell(new URL(`https://bmo.example.com${other}icons/icon-192.png`))).toBe(false);
      expect(there.rules.inScope({ url: `https://bmo.example.com${base}rooms/1` })).toBe(true);
      expect(there.rules.inScope({ url: `https://bmo.example.com${other}rooms/1` })).toBe(false);
    });
  }

  test("a manifest handed a base without its slashes still writes it as a scope", () => {
    expect(appManifest("console", "0123456789ab")).toMatchObject({ start_url: "/console/", scope: "/console/" });
  });

  test("no file of these roads writes a base path of its own", async () => {
    const files = ["server/utils/app-manifest.ts", "app/plugins/pwa.client.ts", "shared/paths.ts", "public/sw.js"];

    for (const file of files) {
      const source = await Bun.file(new URL(`../../../apps/web/${file}`, import.meta.url)).text();

      expect({ file, literal: source.includes("/bmo") }).toEqual({ file, literal: false });
    }
  });
});

// A redrawn icon has to reach a phone that installed the app before it: the worker there keeps the
// icons it cached, and uninstalling the app clears neither. So the icons' version is read off their
// bytes, and every address of the shell and the cache holding it follow that version.
describe("the shell follows its icons", () => {
  const iconsDir = new URL("../../../apps/web/public/icons/", import.meta.url).pathname;
  const icons = readdirSync(iconsDir).map((name) => ({ name, bytes: new Uint8Array(readFileSync(join(iconsDir, name))) }));

  test("changing one icon's bytes gives another version", () => {
    const redrawn = icons.map((icon) =>
      icon.name === "icon-192.png" ? { name: icon.name, bytes: Uint8Array.from(icon.bytes, (byte, at) => (at === icon.bytes.length - 1 ? byte ^ 1 : byte)) } : icon
    );

    expect(shellVersion(icons)).toBe(shellVersion([...icons].reverse()));
    expect(shellVersion(redrawn)).not.toBe(shellVersion(icons));
  });

  test("another version is another cache and another address for every picture the worker names", async () => {
    const before = await serviceWorker(`https://bmo.example.com/bmo/sw.js?density=2&shell=${shellVersion(icons)}`);
    const after = await serviceWorker("https://bmo.example.com/bmo/sw.js?density=2&shell=0123456789ab");

    expect(before.shell.cache).toBe(`bmo-chat-shell-${shellVersion(icons)}`);
    expect(after.shell.cache).not.toBe(before.shell.cache);
    expect(before.shell.notificationIcon).toBe(`/bmo/icons/icon-192.png?v=${shellVersion(icons)}`);
    expect(after.rules.badgeFor("2")).toBe("/bmo/icons/badge-48.png?v=0123456789ab");
  });

  test("the manifest names its icons at their versioned addresses", () => {
    const manifest = appManifest("/bmo/", "0123456789ab") as { icons: { src: string }[] };

    expect(manifest.icons.map((icon) => icon.src)).toContain("/bmo/icons/icon-192.png?v=0123456789ab");
    expect(manifest.icons.every((icon) => icon.src.endsWith("?v=0123456789ab"))).toBe(true);
  });

  test("the window names the version in the worker's address", async () => {
    const plugin = await Bun.file(new URL("../../../apps/web/app/plugins/pwa.client.ts", import.meta.url)).text();

    expect(plugin).toContain("&shell=${shellVersion}");
  });

  test("only the icons are kept, never the manifest, whose words follow no icon", () => {
    expect(worker.shell.isShell(new URL("https://bmo.example.com/bmo/icons/icon-192.png?v=0123456789ab"))).toBe(true);
    expect(worker.shell.isShell(new URL("https://bmo.example.com/bmo/manifest.webmanifest?v=0123456789ab"))).toBe(false);
    expect(worker.shell.isShell(new URL("https://elsewhere.example/bmo/icons/icon-192.png"))).toBe(false);
  });
});
