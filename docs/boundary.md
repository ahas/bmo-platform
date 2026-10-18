# The boundary

This document is bmo-web's boundary: every route its server serves, every call its server and its console
make on another unit, the settings that name those units, and the codes its language packs render. A unit
reaches bmo-web only across it, and bmo-web reaches another unit only by the calls below: never by its code,
its tables, its files or its process.

bmo-web stands in two processes. The **server**, under `server/`, is the account service on sumida with the
language packs: the users, the senders linked to them, the grants they hold, the account model they stand in,
and the sentences a person reads. The **console**, under `client/`, is the Nuxt app a person signs in to; its
own server is the only caller that faces a browser, and it reaches the server and every other unit over HTTP.

## Versioning

- **The boundary carries the repository's one version, in semver,** the version `VERSION` holds.
- **A change to this document and a change to the boundary go in one commit.** A change that makes a route,
  a call or a setting different repairs this document in the same commit, stating what is true now.
- **The version moves by what changed:** the major on a change to a route or a code an existing caller reads;
  the minor on an addition of one; the patch on anything else.

## Wire conventions

These hold for every route the server serves, and the console's clients hold every unit's own conventions as
that unit's document states them.

- **A body is JSON, both ways,** with snake_case keys. A time is epoch milliseconds, a count a whole number,
  and a field written with a question mark may be null.
- **Every route but `GET /health` takes `Authorization: Bearer <token>`** with the server's token, and answers
  401 `unauthorized` without it. sumida's own session routes (below) take the session cookie instead and are
  reached only through the console's server.
- **A path segment is an id as it stands,** percent-encoded where it needs to be.
- **An answer with no body is 204;** one with a body is 200, or 201 where the call creates something. A read
  of one thing by its id answers 404 where it does not exist.
- **A refusal carries a body of exactly two keys:** `error`, one English sentence for a person, and `code`, a
  stable snake_case code a caller decides on. The statuses are 400 for a request wrong as asked, 401 for a
  missing or wrong token, 403 for one the asker may not make, 404 for what does not exist, 409 for a conflict
  with the present state, 500 for a failure on the server's side (code `database` for its database), 502 for a
  call on another unit that failed, and 503 while the account model is not in force.

## What the server serves

### Health

| Route | Answer |
|---|---|
| `GET /health` | `ok`, `version`, `in_force` |

### The account model

| Route | Request | Answer |
|---|---|---|
| `GET /accounts/model` | | `in_force`; `why?`, where it is not in force |
| `POST /accounts/move` | `owner_senders` (each a sender), `rooms` (each a room the deployment answers), `rules` (each `room`, `subject`, `text`) | `moved`: true where this call wrote the move, false where it stood already |

- **The model is in force** once the server stands on its database, sumida's tables stand, its rules are
  registered and the move has been written. Until then every route but `GET /health` and
  `GET /accounts/model` answers 503 naming why, so no caller mistakes an absent model for a refusal; a caller
  reads that no sender is linked to a user and no grant stands.
- **The move** is written once, in one transaction, and a second call writes nothing (`docs/accounts.md`, 「The
  data move」).

### Users and senders

| Route | Request | Answer |
|---|---|---|
| `GET /senders/{sender}/user` | | `user`: the user the sender is linked to; 404 where none is |
| `GET /users/{user}` | | `user`, `name`, `administrator` (boolean), `senders` |
| `GET /users/{user}/senders` | | `senders`: the sender numbers linked to the user |
| `POST /users/{user}/password` | `current`, `next` | 204; 401 `wrong_password` where `current` does not match |
| `POST /users/{user}/name` | `name` | 204; 409 `name_taken` where another user holds it |

- **A user** is named by its id as the server gives it, a string a caller hands on as it stands.
- **A sender** is a whole number, as the receiver names a message's sender.

### Checks

| Route | Request | Answer |
|---|---|---|
| `POST /checks/room` | `room`, `sender?`, `relation` | `allowed`, `grant`, `user?` |
| `POST /checks/message` | `room`, `message`, `relation` | `allowed`, `grant`, `user?`, `sender?` |
| `POST /checks/object` | `kind`, `object`, `relation`, `user` | `allowed`, `grant` |

- **`grant`** is what was asked for, `kind:object#relation` (`bmo_room:<room>#<relation>` for a room), as a
  caller names it when it says what a user lacks.
- **A room check** builds the sender's user where their identity is linked, and the room's members, always.
- **A message check** asks the receiver for the message's sender (「Calls the server makes」) and checks that
  sender in the room; a message the receiver does not hold in that room is refused 404 `no_such_message`.
- **An object's `kind`** is one the server's rules declare: `bmo_worker` (relation `run`), `bmo_repository`
  (relation `change`) or `bmo_tools`. A kind or a relation the rules do not declare is refused 400.
- **The platform administrator passes every check.**

### Grants

| Route | Request | Answer |
|---|---|---|
| `GET /grants?room=` | | `grants`: each `relation`, `subject_key` (`user:{user}` or another key), `expires_at?`, `words?` |
| `POST /grants` | `room?`, or `kind` and `object`; `relation`; `to` (`sender?`, `user?`, `members`: true for every member of the room); `words?`; `expires_at?`; `message` (`room`, `id`) | `grant` |
| `DELETE /grants` | as `POST /grants`, without `words` and `expires_at` | `grant` |

- **The granter is the sender of the message a grant is made on,** and sumida checks that their user may grant
  what they give, so nobody gives what they do not hold.
- **A sender with no user yet is given one,** with no way to sign in, and linked.
- **Granting again** moves the expiry and the words.

### Phone keys

| Route | Request | Answer |
|---|---|---|
| `POST /phones` | `user` | `key`, once, and `hash`, the hash the key is kept by |
| `POST /phones/adopt` | `token`, `user` | `hash`: the earlier token kept as a key of that user |
| `POST /phones/who` | `key` | `user`; 401 where it is no live key of a user who may sign in |
| `DELETE /phones/{hash}` | | 204 |

### A room's holdings

| Route | Answer |
|---|---|
| `GET /rooms/{chat}/holdings` | `holdings`: each `code` and its fields — `room_grants` with `count`, the grants standing on the room, and `room_senders` with `count`, the senders whose link was made by a grant in the room |

- **The server declares itself a holder** to the receiver as it starts (「Calls the server makes」), with the
  holdings `room_grants` (`effect` `warns`) and `room_senders` (`effect` `kept`), so the receiver shows what a
  room's removal would take before it removes the room.

### Fixed lines

| Route | Request | Answer |
|---|---|---|
| `POST /words/render` | `code`, the fields the code carries beside it, `language` | `text` |

- **A fixed line** is one of the few lines no agent's session says: a code a unit answers with, rendered in
  the room's language from the server's pack. The codes it renders are the receiver's `FixedLine` codes
  (the receiver's `docs/codes.md`, 「Fixed lines」), each to the sentence it renders as.
- **A code the pack does not hold, or a language it does not have,** is refused 404 `unknown_code` or
  `unknown_language`; the caller then sends the line as its code and fields, so a room is never silent.

### The document wording file

The server's release carries the document wording file bmo-artifact's `[wording] file` setting names: every
key of the artifact's 「Wording」 table, rendered from the server's pack, and no other key (`docs/language-pack.md`,
「What other units read」).

### sumida's own routes

The server mounts sumida's router as sumida builds it. The console's server is their one caller, and hands
on its own caller's address in `X-Real-IP`, which sumida's bans read.

| Route | What it is |
|---|---|
| `POST /auth/session` | sign-in, with the name and the password; answers the session cookie, `bmo.sid`, or 401 |
| `DELETE /auth/session` | sign-out, with the cookie |
| `GET /me` | who a session is, with the cookie; 401 where it is no live session |

## Calls the server makes

Each goes to the unit's base address with that unit's token as a bearer, as the unit's document states.

| Unit | Call | Route | Why |
|---|---|---|---|
| the receiver | declare a holder | `PUT /room-holders/{unit}` (`address`, `holdings`) | once as the server starts, so a room's removal asks the server's holdings |
| the receiver | read a message | `GET /rooms/{chat}/messages/{message}` | a message check's sender |

- **A call that does not reach the receiver** is a failure of the call: the start goes on and declares again
  on its next start; a message check answers 502 `unit_failed`.

## Calls the console makes

The console's own server makes every call; no browser reaches another unit. Each sibling's document is the
contract, and the console holds its conventions — its casing, its auth and its refusal body — as that document
states them.

| Unit | What the console reaches it for | Routes |
|---|---|---|
| the server (this repository) | sign-in, the session, users, phone keys, grants, checks, the model | every route above |
| the agency (`docs/api.md`) | the agents and their gates, reasoning, roles and sampling; base prompts; companions and their reset; sessions and a session's reset; the event stream; the co-worker pool; a run's log and records; the models, the registry, the tiers and the roads; the stand-in; the rest; a worker's node | `GET /agents`, `GET /rulebooks`, `POST /agent-gate`, `POST /agent-reasoning`, `POST /agent-role`, `GET`/`POST /agent-sampling`, `GET`/`POST /base-prompts`, `GET`/`POST /companions`, `GET`/`POST /companions/reset`, `GET /sessions`, `POST /session-reset`, `GET /events`, `GET /coworker-pool`, `GET /run-log`, `GET /run-records`, `GET`/`POST /models`, `/registry`, `/model-tiers`, `/fallback`, `GET /model-tiers/models`, `/fallback/codex-models`, `/fallback/openrouter-models`, `GET /stand-in`, `GET`/`POST /rest`, `POST /worker/node`, `POST /runs/stop`, `GET /health` |
| bmo-job (`docs/api.md`) | the TODO panel and an item, its talk and its work order; the questions; the queue's holds and pause; the deploy hold; a launch; the workers and a word to one; a room's jobs; the event stream | `GET /todos`, `GET /todos/{item}`, `GET /todos/worker-lines`, `GET /todos/{item}/worker-lines`, `POST /todo`, `GET`/`POST /todo-talk`, `GET /work-orders/{item}`, `GET /work-orders/{item}/current`, `GET /work-orders/{item}/tasks/{task}`, `POST /work-order`, `POST /question`, `POST /question-pause`, `GET /queue/holds`, `POST /queue-pause`, `POST /deploy-hold-release`, `POST /launch`, `POST /task`, `GET /workers`, `GET /workers/{name}`, `POST /worker`, `POST /worker-word`, `GET /rooms/{chat}/jobs`, `POST /run/stop`, `GET /events`, `GET /health` |
| the receiver (`docs/boundary-api.md`) | the room list, a room's title and messages; the owner and the owner's private chat; the owner's words and files; the event stream and the live board; removing a room; what opened a turn; the voice briefing's switch and a line's reading; companions with a room; a talk about a document | `GET /console/rooms`, `GET /console/rooms/{chat}/title`, `GET /console/rooms/{chat}/agent-message/{message}`, `GET /console/rooms/{chat}/messages`, `GET /console/rooms/{chat}/day-start`, `GET /console/rooms/{chat}/messages/after`, `GET /console/rooms/{chat}/messages/{message}`, `GET /owner`, `POST /say`, `POST /say-file`, `POST /instruct`, `GET /events`, `GET /live`, `GET`/`POST /rooms/remove`, `GET /rounds/openings`, `GET /rounds/instruction`, `POST /voice-briefing`, `POST /voice-briefing/line`, `POST /companions/add`, `POST /companions/remove`, `POST /talks`, `GET /health` |
| the brain (`docs/boundary-api.md`) | the memories screen; a fragment's standing in the document viewer | `GET /memories`, `POST /memories/active`, `GET /fragments`, `POST /fragments/active`, `GET /health` |
| bmo-artifact (`docs/api.md`, `docs/layout.md`) | the archive screen of its console plugin bundle, the share page, the research settings | every road under `/api/archive` and `/api/share/`, and `/searches` and `/collected` as moved screens, handed on whole behind the console's sign-in (the share page's roads take none); `GET`/`POST /research-settings` |
| the cache siren (`docs/api.md`) | the cache siren screen | `GET /stats`, `GET /events?limit=` |
| the cache warmer (`docs/api.md`) | the keepalive view: the round, the targets, a subject's switch and pause | `GET /round`, `PUT /round`, `GET /targets`, `PUT /consumers/{consumer}/subjects/{subject}/switch`, `POST /consumers/{consumer}/subjects/{subject}/pause` |
| the Telegram service (`docs/boundary-api.md`) | the profile photos of rooms and people | `GET /avatars/{kind}/{id}` |
| the testbed (`docs/deploy.md`) | the deployment in the title bar | the progress record, `deploy-progress.json`, read at the path a setting names; the testbed writes it and a reader needs only the file and the run's process |

- **A call that does not reach its unit,** or an answer that does not read as its shape, is that screen's
  failure: the region shows the failure and a way to ask again (`docs/skeleton-loading.md`), and every other
  region stands.

## Settings

The server reads its own settings file, overridden by the environment; the console reads its runtime
configuration from the environment. No address, token or name is written into the code.

| Process | Setting | What it names |
|---|---|---|
| server | `listen` | the socket address the server binds |
| server | `token` | the bearer token every caller presents |
| server | `database` | the server's PostgreSQL database |
| server | `valkey` | the Valkey sumida keeps its sessions in |
| server | `receiver.base`, `receiver.token` | the receiver |
| server | `language` | the language the pack renders in where a call names none |
| console | `WEB_SERVER_ADDRESS`, `WEB_SERVER_TOKEN` | the server |
| console | `AGENCY_ADDRESS`, `AGENCY_TOKEN` | the agency |
| console | `JOB_ADDRESS`, `JOB_TOKEN` | bmo-job |
| console | `RECEIVER_ADDRESS`, `RECEIVER_TOKEN` | the receiver |
| console | `BRAIN_ADDRESS`, `BRAIN_TOKEN` | the brain |
| console | `ARTIFACT_ADDRESS`, `ARTIFACT_TOKEN` | bmo-artifact |
| console | `CACHE_SIREN_ADDRESS` | the cache siren |
| console | `CACHE_WARMER_ADDRESS` | the cache warmer |
| console | `TELEGRAM_ADDRESS`, `TELEGRAM_TOKEN` | the Telegram service |
| console | `DEPLOY_PROGRESS_FILE` | the testbed's progress record |
| console | `WEB_DATA_DIR` | the console's own data folder, inside no checkout |
| console | `NUXT_SESSION_PASSWORD` | the key the console seals its session cookie with |

- **A unit whose address is not set** is a unit the console does not reach: its screens stand as they do for a
  unit not yet split (below), and no call is made.

## Screens of units not yet split

The screens of the mail, the drive, the code review, the remote access, the code graph, the nodes and the
monitor, the card arbiter, the gates and the voice briefing's player belong to units whose own repositories
are not yet split. Until each unit's own item carries its screen as a plugin bundle, the console shows a
placeholder in that screen's place: its navigation entry and its route stand, and the page says, through the
language pack, that the feature is not available in this console. The placeholder makes no call.
`docs/inventory.md` lists every file of those screens.

## The codes the packs render

Every other unit answers in English messages and codes; the language packs here hold the sentence each code is
shown as.

| Unit | Codes | Where they are listed |
|---|---|---|
| the receiver | its refusals, facts, fixed lines and kinds; the fixed lines through `POST /words/render` | the receiver's `docs/codes.md` |
| bmo-job | its refusals, answers, facts and kinds, where a screen shows one | bmo-job's `docs/api.md`, 「Codes」 |
| the brain | the codes it answers with, where a screen shows one | the brain's `docs/boundary-api.md`, 「Codes the brain answers with」 |
| bmo-artifact | the document wording file's keys | bmo-artifact's `docs/api.md`, 「Wording」 |
| bmo-ui | the 76 keys the layer reads | bmo-ui's `docs/boundary.md`, 「Wording keys」 |

## The public address

The console answers under one base path, the one its settings name, behind the deployment's proxy. The proxy
hands on the base path as it is, and flows the event streams unbuffered; every road, the artifact bundle's
`/api/archive` and `/api/share/` included, is the console's to hand on.
