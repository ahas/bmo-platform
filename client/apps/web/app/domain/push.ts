import { newDevicePushKinds, PUSH_DENIED, PUSH_NO_WORKER, PUSH_UNCONFIGURED, PUSH_UNSUPPORTED } from "#shared/config";
import { countText } from "./numbers";
import { failureText } from "bmo-ui/app/domain/errors";

import type { PushKind } from "#shared/config";
import type { PushDevice, PushState } from "#shared/types";
import type { Translate } from "#shared/wording";
import type { Ref } from "vue";

// Whether this browser can be rung at all, and what to say where it cannot. The three ways it
// cannot are different facts and each has its own sentence: the browser has no push, the owner
// has refused it here, or this server holds no keys to sign with.
export type PushStanding = "ready" | "unsupported" | "denied" | "unconfigured";

export function pushStanding(input: {
  supported: boolean;
  permission: "default" | "granted" | "denied";
  configured: boolean;
}): PushStanding {
  if (!input.supported) {
    return "unsupported";
  }

  if (!input.configured) {
    return "unconfigured";
  }

  return input.permission === "denied" ? "denied" : "ready";
}

export function standingNotice(t: Translate, standing: PushStanding): string | null {
  if (standing === "unsupported") {
    return t(PUSH_UNSUPPORTED);
  }

  if (standing === "denied") {
    return t(PUSH_DENIED);
  }

  return standing === "unconfigured" ? t(PUSH_UNCONFIGURED) : null;
}

// This browser among the devices the account has. The endpoint is the browser's own name for
// itself, so it is what tells this device from the others on the list.
export function deviceOf(devices: readonly PushDevice[], endpoint: string | null): PushDevice | null {
  return endpoint === null ? null : (devices.find((device) => device.endpoint === endpoint) ?? null);
}

// The devices that are not this one. They are shown so a browser that has been cleared, or a
// phone no longer carried, can be dropped from here rather than ringing into nothing until a
// push service says it is gone.
export function otherDevices(devices: readonly PushDevice[], endpoint: string | null): PushDevice[] {
  return devices.filter((device) => device.endpoint !== endpoint);
}

// The VAPID public key as the subscription call wants it: the server hands it over base64url,
// and this is the same bytes as the buffer the browser asks for.
export function keyBytes(base64url: string): ArrayBuffer {
  const padded = base64url.padEnd(base64url.length + ((4 - (base64url.length % 4)) % 4), "=");
  const binary = atob(padded.replace(/-/g, "+").replace(/_/g, "/"));
  const buffer = new ArrayBuffer(binary.length);
  const bytes = new Uint8Array(buffer);

  for (let at = 0; at < binary.length; at += 1) {
    bytes[at] = binary.charCodeAt(at);
  }

  return buffer;
}

// A subscription as this app sends it up: the endpoint and the two keys the push is sealed to,
// and nothing else the browser's own object carries.
export function subscriptionBody(subscription: PushSubscription, kinds: readonly PushKind[]): {
  endpoint: string;
  keys: { p256dh: string; auth: string };
  kinds: PushKind[];
} {
  const raw = subscription.toJSON();

  return {
    endpoint: subscription.endpoint,
    keys: { p256dh: raw.keys?.p256dh ?? "", auth: raw.keys?.auth ?? "" },
    kinds: [...kinds]
  };
}

// Whether this browser has push of its own. All three are needed and a browser may have the
// service worker without the rest.
export function pushSupported(): boolean {
  return (
    typeof window !== "undefined" &&
    "serviceWorker" in navigator &&
    "PushManager" in window &&
    "Notification" in window
  );
}

export interface PushControls {
  standing: Ref<PushStanding>;
  notice: Ref<string | null>;
  // This browser's own subscription, or null while it has none.
  here: Ref<PushDevice | null>;
  others: Ref<PushDevice[]>;
  busy: Ref<boolean>;
  initial: Ref<boolean>;
  reload: () => Promise<void>;
  // A change that did not go through, with what is on the screen still standing.
  failure: Ref<string | null>;
  // The reading itself having failed, which is a different screen: nothing was read, so there is
  // nothing to show and the only thing to do is read it again. One value for both would be drawn
  // twice by a screen that shows each of them where it belongs.
  readFailure: Ref<string | null>;
  // What this browser last heard back from a test ring, so pressing it says something.
  tested: Ref<string | null>;
  subscribe: () => Promise<void>;
  unsubscribe: () => Promise<void>;
  setKinds: (kinds: PushKind[]) => Promise<void>;
  forget: (endpoint: string) => Promise<void>;
  ringTest: () => Promise<void>;
}

// Ringing this browser. Everything it does goes through this app's own routes, and the
// subscription it makes is written against the account that is logged in. A screen that shows none
// of it says so, and nothing is read until it is asked for.
export function usePush(t: Translate, { immediate = true }: { immediate?: boolean } = {}): PushControls {
  const state = ref<PushState>({ configured: false, publicKey: null, devices: [] });
  const endpoint = ref<string | null>(null);
  const permission = ref<"default" | "granted" | "denied">("default");
  const supported = ref(false);
  const busy = ref(false);
  const initial = ref(true);
  const failure = ref<string | null>(null);
  const readFailure = ref<string | null>(null);
  const tested = ref<string | null>(null);

  const standing = computed<PushStanding>(() =>
    pushStanding({ supported: supported.value, permission: permission.value, configured: state.value.configured })
  );
  const notice = computed<string | null>(() => standingNotice(t, standing.value));
  const here = computed<PushDevice | null>(() => deviceOf(state.value.devices, endpoint.value));
  const others = computed<PushDevice[]>(() => otherDevices(state.value.devices, endpoint.value));

  async function read(): Promise<void> {
    state.value = await $fetch<PushState>("/api/push");
  }

  // The worker this browser actually has, or null where it has none.
  //
  // It is asked for rather than waited on. `navigator.serviceWorker.ready` never settles at all
  // while no worker has taken the page — a registration that failed, a browser that refused one,
  // a copy of the console served from somewhere the worker's scope does not cover — so a screen
  // built on it stands at its skeleton for ever with nothing on it saying why. Asking answers
  // either way, and a browser with no worker is then told so.
  async function worker(): Promise<ServiceWorkerRegistration | null> {
    return (await navigator.serviceWorker.getRegistration()) ?? null;
  }

  // What this browser is already subscribed to, if anything. It is asked of the registration
  // rather than remembered here, because the browser is what actually holds it.
  async function readEndpoint(): Promise<void> {
    if (!supported.value || !state.value.configured) {
      return;
    }

    const registration = await worker();
    const subscription = await registration?.pushManager.getSubscription();

    endpoint.value = subscription?.endpoint ?? null;
  }

  async function guarded(work: () => Promise<void>): Promise<void> {
    busy.value = true;
    failure.value = null;

    try {
      await work();
    } catch (error) {
      failure.value = failureText(error, t("push.error.change_failed"));
    } finally {
      busy.value = false;
    }
  }

  async function subscribe(): Promise<void> {
    await guarded(async () => {
      const key = state.value.publicKey;

      if (key === null) {
        throw new Error(t(PUSH_UNCONFIGURED));
      }

      permission.value = await Notification.requestPermission();

      if (permission.value !== "granted") {
        return;
      }

      const registration = await worker();

      if (registration === null) {
        throw new Error(t(PUSH_NO_WORKER));
      }

      const subscription =
        (await registration.pushManager.getSubscription()) ??
        (await registration.pushManager.subscribe({
          // Every push this console sends carries something to show, so none of them is silent.
          userVisibleOnly: true,
          applicationServerKey: keyBytes(key)
        }));

      endpoint.value = subscription.endpoint;
      state.value = await $fetch<PushState>("/api/push", {
        method: "POST",
        body: subscriptionBody(subscription, newDevicePushKinds())
      });
    });
  }

  async function unsubscribe(): Promise<void> {
    await guarded(async () => {
      const registration = await worker();
      const subscription = await registration?.pushManager.getSubscription();
      const gone = subscription?.endpoint ?? endpoint.value;

      await subscription?.unsubscribe();

      if (gone !== null) {
        state.value = await $fetch<PushState>("/api/push", { method: "DELETE", body: { endpoint: gone } });
      }

      endpoint.value = null;
    });
  }

  async function setKinds(kinds: PushKind[]): Promise<void> {
    await guarded(async () => {
      const subscription = (await (await worker())?.pushManager.getSubscription()) ?? null;

      if (subscription === null) {
        return;
      }

      state.value = await $fetch<PushState>("/api/push", {
        method: "POST",
        body: subscriptionBody(subscription, kinds)
      });
    });
  }

  async function forget(gone: string): Promise<void> {
    await guarded(async () => {
      state.value = await $fetch<PushState>("/api/push", { method: "DELETE", body: { endpoint: gone } });
    });
  }

  async function ringTest(): Promise<void> {
    tested.value = null;
    await guarded(async () => {
      const result = await $fetch<{ sent: number; gone: number }>("/api/push/test", { method: "POST" });

      tested.value =
        result.sent === 0
          ? t("push.message.test_nowhere")
          : t("push.message.test_sent", { n: countText(result.sent) });
    });
  }

  async function reload(): Promise<void> {
    initial.value = true;
    failure.value = null;
    readFailure.value = null;
    supported.value = pushSupported();
    permission.value = supported.value ? Notification.permission : "default";

    try {
      await read();
      await readEndpoint();
    } catch (error) {
      readFailure.value = failureText(error, t("push.error.read_failed"));
    } finally {
      initial.value = false;
    }
  }
  if (immediate) {
    onMounted(reload);
  }

  return {
    reload,
    initial,
    standing,
    notice,
    here,
    others,
    busy,
    failure,
    readFailure,
    tested,
    subscribe,
    unsubscribe,
    setKinds,
    forget,
    ringTest
  };
}
