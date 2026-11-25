// The renderer, fetched once for the whole console. The first text to be shown asks for it and
// every one after that is built the moment it stands, so a screen of fifty messages downloads it
// once and waits for it once.
//
// Whether this is a browser is asked outright. The answer is known while the build is being made,
// so the server's copy of this function ends at that line and the renderer is in no bundle but the
// browser's.

type Renderer = typeof import("./markdown");

let loaded: Renderer | null = null;
let loading: Promise<Renderer | null> | null = null;

// The renderer when it has already arrived, and null until it has.
export function markdownRenderer(): Renderer | null {
  return loaded;
}

// The renderer, fetched if nobody has asked for it yet. Null on the server, and null where the
// fetch failed; a failed fetch is forgotten so the next text asks again.
export function loadMarkdownRenderer(): Promise<Renderer | null> {
  if (!import.meta.client) {
    return Promise.resolve(null);
  }

  loading ??= import("./markdown").then(
    (renderer) => (loaded = renderer),
    () => {
      loading = null;

      return null;
    }
  );

  return loading;
}
