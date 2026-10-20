import { unsealSession } from "h3";

import { unitAt, unitHeaders } from "../utils/units";

import type { ConsoleBundleView } from "#shared/console-bundles";

// A console bundle's sockets (bmo-ui's `docs/console-bundles.md`, 「How a bundle mounts」; `docs/boundary.md`,
// 「Calls the console makes」): a screen's socket is opened at the relay base, the service's name and the
// path the service's own API writes, and the console hands the upgrade on there with the service's
// token, behind its own sign-in. A frame goes each way as it came, so the bundle's protocol is the
// service's own.

interface Held {
  upstream?: WebSocket;
}

// bun adds a `headers` option to the WebSocket constructor; the DOM type does not name it.
interface SocketOptions {
  headers?: Record<string, string>;
}

const OPEN = 1;

// Whether the upgrade carries a live session. The upgrade never reaches the console's own middleware
// (Nitro's server hands it to this handler first), so the seal is read here through the same module the
// routes read it with.
async function signedIn(request: { url: string; headers: Headers }): Promise<boolean> {
  const config = useRuntimeConfig();
  const name = config.session?.name ?? "nuxt-session";
  const cookies = new Map(
    (request.headers.get("cookie") ?? "")
      .split(";")
      .map((part) => part.trim().split("="))
      .filter((pair) => pair.length === 2)
      .map(([key, value]) => [key!, value!] as const)
  );
  const sealed = cookies.get(name);

  if (sealed === undefined || sealed === "") return false;

  try {
    const unsealed = await unsealSession(undefined as never, config.session as never, decodeURIComponent(sealed));
    const data = unsealed.data ?? {};

    return typeof data.user === "object" && data.user !== null;
  } catch {
    return false;
  }
}

function serviceOf(path: string): { bundle: ConsoleBundleView; rest: string; search: string } | null {
  const url = new URL(path, "http://console.invalid");
  const parts = url.pathname.split("/").filter((part) => part !== "");
  const at = parts.indexOf("api");
  const service = parts[at + 1] ?? "";
  const rest = parts.slice(at + 2).map(encodeURIComponent).join("/");
  const bundles = (useRuntimeConfig().consoleBundles ?? []) as ConsoleBundleView[];
  const bundle = bundles.find((one) => one.serviceName === service);

  return at === -1 || bundle === undefined || !bundle.websocket ? null : { bundle, rest, search: url.search };
}

export default defineWebSocketHandler({
  async open(peer) {
    const asked = serviceOf(peer.request.url);

    if (asked === null) {
      peer.close(1008, "no such relay");

      return;
    }

    if (!(await signedIn(peer.request))) {
      peer.close(1008, "unauthorized");

      return;
    }

    const unit = unitAt(asked.bundle.serviceName.replaceAll("-", "_").toUpperCase());

    if (unit === null) {
      peer.close(1011, "the service is not reachable");

      return;
    }

    const target = new URL(`${unit.address}/${asked.rest}${asked.search}`);
    target.protocol = target.protocol === "https:" ? "wss:" : "ws:";

    const Socket = WebSocket as unknown as new (url: URL, options: SocketOptions) => WebSocket;
    const socket = new Socket(target, { headers: unitHeaders(unit) });
    const held = peer.context as unknown as Held;
    held.upstream = socket;

    socket.addEventListener("message", async (message) => {
      const data = typeof message.data === "string" ? message.data : await new Response(message.data as Blob).arrayBuffer();

      if (peer.websocket.readyState === OPEN) peer.send(data);
    });
    socket.addEventListener("close", (close) => peer.close(close.code, close.reason));
    socket.addEventListener("error", () => peer.close(1011, "the service failed"));
  },

  message(peer, message) {
    const socket = (peer.context as unknown as Held).upstream;

    if (socket !== undefined && socket.readyState === OPEN) socket.send(message.rawData as ArrayBuffer);
  },

  close(peer) {
    (peer.context as unknown as Held).upstream?.close();
  }
});
