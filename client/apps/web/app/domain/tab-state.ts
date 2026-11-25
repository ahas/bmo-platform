// A screen's open tabs kept on the server against the logged-in account (docs/web.md, 「Tab bar and
// toolbar」), so the same screen opened in another browser or on another device stands with the same
// tabs. The screen names its own key, says what it keeps and how a kept value is put back; this
// only carries it there and back.

// How long the tabs stand still before they are written, so a drag or a run of openings is one
// write.
const SAVE_WAIT_MS = 400;

export interface KeptTabs<S> {
  // What the screen keeps, read fresh each time it changes.
  read: () => S;
  // The kept value put back into the screen: the screen reads it as far as it holds and merges it
  // with whatever it has already opened. Null where nothing was kept.
  restore: (kept: unknown) => void;
}

export function useKeptTabs<S>(screen: string, kept: KeptTabs<S>): void {
  if (!import.meta.client) {
    return;
  }

  const base = useRuntimeConfig().app.baseURL.replace(/\/$/, "");
  const address = `${base}/api/tab-states/${screen}`;
  let loaded = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pending: S | null = null;

  const send = (keepalive: boolean): void => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }

    if (pending === null) {
      return;
    }

    const body = JSON.stringify({ state: pending });

    pending = null;
    // A lost write leaves the tabs as they were kept before, which is all a failure here can cost.
    void fetch(address, { method: "PUT", headers: { "content-type": "application/json" }, body, keepalive, credentials: "same-origin" }).catch(() => undefined);
  };

  const onHide = (): void => send(true);

  onMounted(async () => {
    window.addEventListener("pagehide", onHide);

    try {
      const answer = await $fetch<{ state: unknown }>(`/api/tab-states/${screen}`);

      kept.restore(answer.state);
    } catch {
      kept.restore(null);
    }

    loaded = true;
    // What was opened before the kept tabs arrived, merged with them, is kept at once.
    later(kept.read());
  });

  const later = (next: S): void => {
    pending = next;

    if (timer !== null) {
      clearTimeout(timer);
    }

    timer = setTimeout(() => send(false), SAVE_WAIT_MS);
  };

  // Nothing is written before the kept tabs have been read, or the first draw would overwrite them.
  watch(
    kept.read,
    (next) => {
      if (loaded) {
        later(next);
      }
    },
    { deep: true }
  );

  onBeforeUnmount(() => {
    window.removeEventListener("pagehide", onHide);

    if (loaded) {
      send(true);
    }
  });
}
