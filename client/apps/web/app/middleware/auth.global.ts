import { MEMBER_HOME, MEMBER_LAYOUT, isAdministrator } from "~/domain/access";
import { loginRoute, returnPath } from "~/domain/login-end";

// Nothing but the login page is readable without a session; the API refuses on its own side. The
// login screen carries the address asked for, so the next login returns there, and says the login
// ended where a page drawn on the server found its session gone (docs/web.md, 「Access」).
export default defineNuxtRouteMiddleware((to) => {
  // A share link is the one road with no session behind it. Each bundle's share page stands below its
  // own screen path, and the page reaches what it opens by the token in the address alone; the relay
  // lets that one read through without a session.
  const bundles = useAppConfig().consoleBundles ?? [];

  if (bundles.some((bundle) => to.path.startsWith(`${bundle.screenPath}/share/`))) return;

  const { loggedIn, session } = useUserSession();

  if (to.path === "/login") {
    return loggedIn.value ? navigateTo(returnPath(to.query.to)) : undefined;
  }

  if (!loggedIn.value) return navigateTo(loginRoute(to.fullPath, import.meta.server && useRequestEvent()?.context.sessionEnded === true));

  if (isAdministrator(session.value)) return;

  // A member opens only the screens that say a member may, each in the member's own shell; any
  // other address would open onto routes that answer them as absent (docs/web.md, 「Access」).
  if (to.meta.member !== true) return navigateTo(MEMBER_HOME);

  setPageLayout(MEMBER_LAYOUT);
});
