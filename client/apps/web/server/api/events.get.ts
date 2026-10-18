import { relayBotStream } from "../utils/bot-relay";
import { roomStreamFilter } from "../utils/room-stream";

// The bot's live stream, relayed event by event, with the console's own events written between
// them. The browser cannot carry the control token, so the session is checked here and the token
// is added on this side. A bot that is not there ends the stream, and the browser opens it again
// on its own; whether the bot answers at all is what the health route is for. A user who reads only
// some rooms hears only those rooms' news (docs/web.md, 「Access」).
export default defineEventHandler(async (event) => {
  await requireUserSession(event);

  const rooms = await readableRooms(event);

  const query = getQuery(event);
  const after = readCursor(query.after) ?? 0;
  // Which run of the bot the cursor was taken against. The bot sets a cursor from an earlier run
  // aside and hands over its whole buffer instead, so this travels with it.
  const run = typeof query.run === "string" ? query.run : null;
  const aborted = new AbortController();

  // The upstream call outlives this handler, so it ends when the browser lets go.
  event.node.res.on("close", () => aborted.abort());
  event.web?.request?.signal?.addEventListener("abort", () => aborted.abort());

  setResponseHeaders(event, {
    "content-type": "text/event-stream",
    "cache-control": "no-cache",
    connection: "keep-alive",
    "x-accel-buffering": "no"
  });

  return sendStream(event, relayBotStream(after, run, aborted, rooms === "all" ? {} : roomStreamFilter(rooms)));
});
