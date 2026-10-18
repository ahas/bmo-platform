import { withoutBase } from "ufo";

// A user other than the platform administrator reaches only the routes opened to them, each read
// under its scope (docs/web.md, 「Access」); every other route answers them as if
// nothing stood there. The administrator, a console login and a request nobody signed pass on to
// the route itself, which refuses the last as it always has. A route is named by its path under the
// console's base, as its file names it.
export default defineEventHandler(async (event) => {
  const config = useRuntimeConfig(event);
  const path = withoutBase(event.path.split("?")[0] ?? "", config.app.baseURL);

  if (!path.startsWith("/api/") || !carriesSession(event)) {
    return;
  }

  const session = await getUserSession(event);

  if (session.sid === undefined || session.administrator === true) {
    return;
  }

  const opened = openedRoute(event.method, path);

  if (opened === null) {
    throw createError({ statusCode: 404, statusMessage: "Not Found" });
  }

  event.context.routeScope = opened.scope;
});
