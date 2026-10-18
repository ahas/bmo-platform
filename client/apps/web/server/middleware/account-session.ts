import { wording } from "#shared/wording";

// A seal holding a sumida session is good only while that session stands on the account service
// (docs/web.md, 「Access」): asked about at most once a window, and a session the service no longer
// knows is ended, so the rest of the request is refused as one nobody signed. A service that does not
// answer ends nothing: the request is refused as the service being down and the seal stays for the
// next one. A websocket upgrade and a request with no session cookie are left alone, as the slide
// leaves them.
export default defineEventHandler(async (event) => {
  if (getRequestHeader(event, "upgrade") !== undefined || !carriesSession(event)) {
    return;
  }

  const session = await getUserSession(event);

  if (session.sid === undefined) {
    return;
  }

  const state = await sessionState(session.sid, browserAddress(event));

  if (state === "unanswered") {
    throw fail(503, wording("app.error.accounts_down"));
  }

  if (state === "gone") {
    await endSession(event);
  }
});
