import type { Archive } from "@agency/archive/archive";

// The owner's word on the warming round as a whole: one setting, because it is one answer and not a
// list. What it stops is upkeep alone — no room, no gate and no job is refused anything by it, and
// every prefix simply goes cold on its own clock.
const keepaliveStoppedKey = "keepalive_stopped";

export async function keepaliveStopped(archive: Archive): Promise<boolean> {
  return await archive.settings().getSetting(keepaliveStoppedKey) === "1";
}

export async function setKeepaliveStopped(archive: Archive, stopped: boolean): Promise<void> {
  if (stopped) {
    await archive.settings().setSetting(keepaliveStoppedKey, "1");
  } else {
    await archive.settings().deleteSetting(keepaliveStoppedKey);
  }
}
