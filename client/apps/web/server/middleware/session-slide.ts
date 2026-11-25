// A kept login is counted from the last visit, not from the login: every request that carries one
// reseals it once the slide interval has passed, and the reseal sets both the cookie's expiry and
// the seal's own a full span from now. A websocket upgrade is left alone, since what answers it is
// the socket and not a page that could carry the new cookie. A request with no session cookie is
// left alone too: asking the session layer about it would mint an empty session and set a cookie
// on a visitor who never logged in.
export default defineEventHandler(async (event) => {
  if (getRequestHeader(event, "upgrade") !== undefined || !carriesSession(event)) {
    return;
  }

  const session = await getUserSession(event);

  if (!sessionWantsSlide(session)) {
    return;
  }

  await setUserSession(event, { user: session.user, loggedInAt: session.loggedInAt, remember: true, touchedAt: Date.now() }, sessionLife(true));
});
