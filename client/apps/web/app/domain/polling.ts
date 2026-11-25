import { POLL_INTERVAL_MS } from "#shared/config";

// Runs the callback on a timer for as long as the calling scope lives.
export function usePolling(run: () => void, intervalMs: number = POLL_INTERVAL_MS): void {
  if (import.meta.server) {
    return;
  }

  const timer = setInterval(run, intervalMs);

  onScopeDispose(() => clearInterval(timer));
}

// How close to an edge of a feed counts as being at it.
export const SCROLL_TOP_THRESHOLD_PX = 120;
export const SCROLL_BOTTOM_THRESHOLD_PX = 120;
