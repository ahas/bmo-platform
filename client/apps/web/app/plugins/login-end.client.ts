import { askWhetherLoginEnded, holdLoginTab } from "~/domain/login-end";
import { appUrl } from "~/domain/urls";

// Every call this app answers with 401 makes the tab ask whether its login still stands, from any
// screen (docs/web.md, 「Access」). The browser's own fetch is where the asking is hooked: every way a
// screen calls this app — $fetch, useFetch, a plain fetch — goes through it, where a hook handed to
// $fetch's defaults is replaced by any call that names its own.
export default defineNuxtPlugin(() => {
  const router = useRouter();
  const { loggedIn, session } = useUserSession();
  const sessionPath = appUrl("/api/_auth/session");
  const ownCalls = `${window.location.origin}${appUrl("/api/")}`;
  const call = window.fetch.bind(window);

  holdLoginTab({
    signedIn: () => loggedIn.value,
    async asked() {
      try {
        const response = await call(sessionPath, { headers: { accept: "application/json" }, cache: "no-store" });

        if (!response.ok) return null;

        const read = (await response.json()) as { user?: unknown } | null;

        return read?.user !== undefined && read.user !== null;
      } catch {
        return null;
      }
    },
    forget: () => {
      session.value = null;
    },
    here: () => router.currentRoute.value.fullPath,
    go: (to) => router.replace(to)
  });

  window.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const response = await call(input, init);

    if (response.status === 401 && response.url.startsWith(ownCalls)) {
      void askWhetherLoginEnded();
    }

    return response;
  }) as typeof window.fetch;
});
