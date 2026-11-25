// The fixed ids a test names the owner, a room, a message, an account, a worker and a unit by, kept
// in one place and the same values the Rust tests read from the `test-support` crate. A test that
// needs a second one of a kind names its own beside these.

// The owner's user id, which is also the chat id of the owner's private chat with the supervisor.
export const OWNER_ID = 42;
// A group room's chat id.
export const ROOM_ID = -100;
// A message's id within its room.
export const MESSAGE_ID = 9;
// A Claude account's id.
export const ACCOUNT_ID = "account";
// A worker's name: the built-in worker's.
export const WORKER = "worker";
// A job's unit name.
export const UNIT = "bmo-task-1";
