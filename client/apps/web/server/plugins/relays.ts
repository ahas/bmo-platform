// Tells every browser this app is relaying to that the stop coming is a restart.
//
// A development server is closed through Nitro's own hook. The served build runs on Bun with no
// signal handling of its own, so there the stop systemd sends is taken here: the relays are told,
// given a moment to carry it, the app's close hook is run for everything else that holds
// something open, and the process ends as it would have.

// How long the word is given to reach the browsers. They read it over this machine's own proxy,
// so this is a flush and not a wait for an answer.
const FLUSH_MS = 200;

export default defineNitroPlugin((nitro) => {
  nitro.hooks.hook("close", () => {
    stopRelays();
  });

  if (import.meta.dev) {
    return;
  }

  let stopping = false;

  const stop = async (): Promise<void> => {
    if (stopping) {
      return;
    }

    stopping = true;
    stopRelays();
    await new Promise((resolve) => setTimeout(resolve, FLUSH_MS));

    try {
      await nitro.hooks.callHook("close");
    } catch (error) {
      console.error("close hook failed:", error);
    }

    process.exit(0);
  };

  process.once("SIGTERM", () => void stop());
  process.once("SIGINT", () => void stop());
});
