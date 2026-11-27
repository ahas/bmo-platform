# The boundary API

This document is agency's boundary: every route it exposes, every event it publishes, every hook a
consumer registers, and the tables whose shape a consumer reads or writes. A consumer reaches agency
only across this boundary and never into a crate or a module behind it; agency reaches a consumer only
by publishing an event or calling a hook the consumer registered. What a turn is told, whom an agent
answers, which role a piece of work runs as, and whether an answer may go out are the
consumer's, decided above this boundary and handed in; agency supplies, carries and controls, and judges
none of it.

`docs/openapi.json` is the machine contract: every route's parameters, bodies, answers and codes, the
events, the tools of the harness plugin, and the removed and moved-out members. It is generated from
the service's own types by `bmo-agency openapi`, and a test fails where the committed document differs
from what the code generates. This document states what the routes are for and how the pieces fit; where
a field's exact shape matters, the OpenAPI document is the one to read.

## The shapes the boundary is made of

- **A route** is an operation agency's server answers: a verb and a path, a request body and a response
  body, and the status codes and kebab-case outcome codes it answers with.
- **An event** is something agency publishes as it happens. Agency does not wait on a subscriber and reads
  nothing back from one.
- **A hook** is a decision agency hands up: a consumer registers one, and agency calls it at the point
  named. Every hook has a default that does nothing.
- **A table** whose contract is shared is one a consumer reads or writes directly beside agency.
- **A client** is the Rust crate and the TypeScript package agency publishes from `docs/openapi.json`.

## Versioning

- The version is the set's (bmo-chat's `docs/conventions.md`, 「Versions」): a release tags the commit
  `v<version>`, a build reads it off the tag and the testbed hands it to the build as `BMO_VERSION`, and the
  MCP server names it. The workspace manifest's version and the OpenAPI document's are a placeholder that
  never moves; a deployment is told apart by its commit.
- A change to this document and a change to the boundary go in one commit. A change to a route, an event, a
  hook or a shared table's contract repairs this document in the same commit.
- Toward the next release, a removed or renamed route, field or event counts as a major step, an addition as
  a minor one, anything else as a patch.

## The control port

- Every route is answered by agency's server, `bin/agency-server`, on the loopback, on the port
  `AGENCY_CONTROL_PORT` names (3220 where it names none), under `Authorization: Bearer <token>` with the
  token `AGENCY_CONTROL_TOKEN` holds. A request without the token is answered 401.
- **A route a run calls for itself also takes the run's session token,** the `BMO_MCP_TOKEN` the tool host
  issued the run's program (`docs/tools.md`, 「The session record」), since a job's program is handed no
  control token. The server asks the tool host for the record the token stands for, with the host's issuer
  token, and the route answers for the record's holder alone. `POST /borrowings` is the one such route. A
  session token on any other route, one the host does not know, or one the host cannot be asked about is
  answered 401.
- Both variables stand in the data root's `env/agency.env`. A verb whose environment names no port reads the
  port there, under the data root `BMO_DATA_DIR` names: a job's own program is handed neither variable
  (`docs/agent-control.md`, 「A run apart is handed the launcher's `HOME` by name」), and a verb it runs reaches
  the server on the deployment's port all the same. The token is read there by `bmo-agency quiet` alone, the
  deployment's own verb; a job asks with its session token instead (above). A program agency starts that
  holds the token is handed it as the file it stands in, `AGENCY_CONTROL_TOKEN_FILE` (`docs/agent-control.md`,
  「A token is handed as the file it stands in」), and every reader of the token reads that file where the
  variable is not set.
- The same file names the variables a line hands the session records of its runs
  (`docs/tools.md`, 「A tool's folder」): for the chat and worker lines, `BMO_GATE_URL`, the gate's address, and
  `BMO_GATE_TOKEN_FILE`, the path of the gate's token file under the data root's `env/`.
- The same file names the tool host's port, `BMO_MCP_HOST_PORT`, with the value the tool host's own
  `env/pocket.env` gives it (3290 where neither names one): every MCP configuration agency writes points
  its tool host servers at that port, and a line whose port is not the host's has no tool of the host's,
  a dsh session refusing to open on it.
- A route takes and answers JSON, keys in camelCase, times in epoch milliseconds, ids as signed 64-bit
  integers. A failure is a non-2xx status with `code` and `error`. A list is a page: `items` and `more`,
  asked with `after` or `before` and `limit`.
- A route that changes state takes a PATCH with the field it changes; an action that leaves a record is a
  POST creating it; a computation that keeps nothing is a POST named for its result.
- **A field a body may leave out is optional in the document and the clients,** and a body that leaves it out
  asks nothing of it: a change leaves it as it stands, an ask takes its default. A caller sends only what it
  means; where a route reads `null` as a value of its own — clearing a field, `auto` — the route says so.
- No path carries a version. A gateway mounts the service under `/api/agency/` and passes the path on
  unchanged.

### Who a caller acts for

- **The console names the signed-in person on a call** with `Owner-Key`, the account's user id, and
  `Owner-Role`, `administrator` or `member`, the same headers it sets on every call it relays.
- **A caller carrying no key is the deployment's own** — a unit, a tool, a job, the shell — and keeps every
  route's present behaviour. `Owner-Role: administrator` is the owner and keeps every access.
- **A member's access to agent management is the console's.** Agent management is five parts, each under the
  console section of the agent management tab that holds it (the web's `docs/accounts.md`, 「Console
  sections」), a role's agents standing under the roles section with the roles:

  | Paths under | Section |
  |---|---|
  | `/supervisors` | `supervisors` |
  | `/roles`, `/role-agents` | `roles` |
  | `/repositories` | `repositories` |
  | `/models` | `models` |
  | `/model-catalogue` | `model-files` |

  On these routes agency asks the account service for that person's console sections, with the person's own
  headers, and goes on only where the route's section allows it: a `GET` or `HEAD` needs its read, and every
  other method its write.
- **The agent list is read under any part of agent management.** The console's agent management screen opens
  on any one of its five parts the reader may read, and its shared reads open on any part read: the agent
  list is the supervisors, the roles and the repositories read together, and an agent's own page stands under
  those three parts. So a `GET` or `HEAD` under `/supervisors`, `/roles`, `/role-agents` or `/repositories` goes
  on where the person may read any one of the five sections; a read under `/models` or `/model-catalogue` needs that
  route's own section, and every change needs its own route's write. What the console reads of the agents
  for its own bookkeeping, outside agent management's screen, carries no key and is the deployment's own.
- A refusal is 403 `forbidden` with one sentence naming what was asked and the section it needs; a permission
  that could not be read because the account service did not answer is 503 `access-unchecked`, so no access is
  guessed. Agency keeps no permission of its own: the console's is the only one.
- **A member's change of a repository needs repository management** on the delegation chain, and a repository
  an organisation unit owns is changed by that unit's heads (`docs/repositories.md`, 「Who may add and change a
  repository」 and 「An organisation's repository」). The same rule stands the other resources an organisation
  unit holds: a supervisor, a role agent's seat and a registered model filed under the unit, and the run
  records of the agents seated on it, belong to that unit's heads alone — heading the unit is the whole of who
  may, the chain's answer of who writes the unit's link of `agency.unit-resources.manage` being the head
  check, and a head of a unit heading the units below it as the chain answers them. **Such a call asks no
  console section:** a member's `POST` naming `ownerUnit` — a `POST /models` naming `unit` — and a change or
  removal of a row a unit owns go past the section's check; the chain's check that follows refuses anyone else.
- **A section's write reaches a member's own rows alone.** Every row of agent management but a role is filed
  under the login account it belongs to, its `ownerAccount`, the user id `Owner-Key` names, or under an
  organisation unit, its `ownerUnit`, or under neither, the platform's; a body naming both owners is refused.
  A member's change — any method but `GET` and `HEAD` — of a supervisor, a repository or a role agent the path
  names goes on only where that row is filed under the member's own account — one filed under another account,
  or one of the platform's, is refused 403 `forbidden`, the sentence naming the row and whose it is, before
  the route reads the body — or, where the row is filed under a unit, where the member heads it. A row a
  member adds — `POST /supervisors`, `POST /repositories`, `POST /role-agents`, `POST /models` — is filed
  under their own account, or, where the body names `ownerUnit` (`unit` for a model), under a unit they head;
  a body naming another account as `ownerAccount` is refused 403 `forbidden`; a member's change of a role
  agent keeps its filing the same way. A role is no account's: the roles are the platform's presets, and a
  member's `POST`, `PATCH` or `DELETE` under `/roles` is refused 403 `forbidden` whatever the section grants. A
  path naming a row the store does not hold goes on to its route, which answers it as it does. The
  administrator and the caller carrying no key change every row and file it where the body says.
- **The lists answer the rows the caller reaches.** `GET /supervisors`, `/role-agents`, `/repositories`,
  `/models` and `/runs` answer a member the rows filed under their own account and those filed under the units
  they head, a head above heading the units below; rows of the platform's and of another account stand in no
  member's list, and the read of a single row the reader does not reach is answered as none stood there. Each
  list takes `ownerAccount`, `unit`, and `unitSubtree=true` with `unit` — the named unit and every unit under
  it, the units below read off the account service — and answers only the rows both named and reached; a
  member may name their own account and a unit they head alone, refused 403 `forbidden` otherwise, and values
  that are not ids or name both axes at once are refused 400. The administrator's and the deployment's own
  calls name any owner and reach every row.
- **The run log and the usage answer the same reach.** A member's read of the log of a run, an agent or a job
  meets the lines written under the agents they reach alone, and a note they write goes into such a log only,
  refused 403 `forbidden` elsewhere; a member's usage figures stand over the work they reach (「log-lines」,
  「accounts, rest and usage」). The administrator's and the deployment's own calls read and write every line
  and group over everything.
- **A person's own supply is theirs alone** (`docs/supply.md`, 「What a person brings」). On `/fallback`,
  `/accounts`, `/claude-logins` and `/personal-supply`, a call naming a person with `Owner-Key` and
  `Owner-Role: member` acts on that person's own Claude accounts and keys alone: what it reads holds no other
  account, an account it adds is pinned to that person, a change naming another's account is answered 404
  `unknown-account` as if it stood nowhere, and the actions that order or choose the platform's accounts are
  refused 403 `forbidden`. The administrator's and the deployment's own calls read the platform's accounts
  and the administrator's own alone in the lists, each carrying its `userId`, and of each person's supply only `personalSupply`: the person and the roads their
  supply serves; `GET /accounts` asked by `userName` or `user` still answers whether that person holds a login,
  as the job unit asks it. A key that is not a person's number is refused 403 `forbidden`.

## Routes by resource

### health

| Method | Path | Does |
|---|---|---|
| GET | `/health` | Readiness and version. |
| GET | `/quiet` | The work the server holds that the next server could not pick up. |

**The quiet** answers `Quiet`: `quiet`, true where the server holds no such work, and `work`, one entry for
each turn it carries that is no job's, whose agent still stands, and that no program's record names yet, each
with its `run`, `session`, `agent`, `turnId` and `since`, the moment the turn began, in epoch milliseconds
(`docs/sessions.md`, 「A restart hands the session over」). `bmo-agency quiet` reads it for the deployment.

**The drain.** Asked to stop by `SIGTERM`, the server stops taking connections and answers any call that
still reaches it `503` `stopping` with `Retry-After`; it ends every event stream with a `stopping` frame,
answers a turn's send still waiting `503` `stopping` with the send's key, and lets every other call in hand
finish within the drain's span, `short_calls_seconds` in `policy/drain.toml`. It then writes every event it
published to the store, given the same span again, and exits. A caller that reads `stopping` asks again once
the server stands; nothing it was carried for is lost by the answer, and a reader resuming its stream there is
sent every event published before the stop.

### events

| Method | Path | Does |
|---|---|---|
| GET | `/events` | Agency's one stream, as server-sent events or a page with `Accept: application/json`. |
| POST | `/events` | A process of agency's own publishes an event. |

An event's SSE name is its kind, `<resource>.<change>` where it tells a member's change, its id is the
service's event number, and its data is `{"id","kind","at","data"}`, the inner `data` the member as a GET
answers it, its key alone for a deleted member, or the fact a kind of its own carries (「Events」 below). A
reader narrows with `kinds`.

- **Numbers only rise.** An event's number is never given twice and keeps rising across the service's
  restarts: a server that starts goes on above every number a process before it gave.
- **What is kept.** Every event the stream carries is kept in the store, the newest 50,000 of them; an
  older one is dropped as newer ones come. A restart drops none.
- **Resuming.** A reader resumes with `Last-Event-ID` or `after`, the last number it read: the stream
  sends every kept event after that number, of the kinds it reads, oldest first, and then the live ones
  as they come, none twice. The page answers the same kept events, `limit` at a time, with `more` where
  there are others after them. A stream opened with no number is sent what is published from then on; a
  page asked with none answers from the oldest kept. A number this numbering never gave is read on from
  the newest number it has given, so what is published after the notice is sent.
- **Told when it is too late.** Where events after the number named are no longer kept — it is older
  than the oldest kept — or the number is one the service never gave, the stream first sends
  `events.missed`, with no id of its own, its data `after`, the number named, and `keptFrom`, the oldest
  number kept or null where none is; the page answers the same as `missed`, null otherwise. What happened
  between is not told again: the reader reads afresh the state it follows, then reads on from the
  events sent after the notice.
- **Told when the server stops.** A server that drains ends every stream it serves with one `stopping`
  frame, with no id of its own, its data `{"kind":"stopping","at"}`, and closes it; the reader opens the
  stream again from the last number it read once the server stands, and loses nothing.

### hooks

| Method | Path | Does |
|---|---|---|
| GET | `/hooks` | The hook points and what is registered at each. |
| PUT | `/hooks/{point}` | Register a downstream's URL at a hook point. |
| DELETE | `/hooks/{point}` | Take a registration away. |
| GET | `/road-verbs` | The verbs a room's road performs, as the consumer answers them. |

The hook points are `typed-line`, `room-title` and `road-verbs`, registered by the receiver, `open-items` and
`pane-claims`, registered by the consumer that owns which items have a job open and which panes it keeps
standing, and `supply`, registered by the consumer that answers who pays for a person's run (「Hook points」).
A point nobody registers answers its default, which composes or claims nothing. A registration takes `url` and,
where the consumer checks a bearer, `token`. A point that declares it outlives a restart — `typed-line` and
`supply` — keeps its address in the store's settings and its token in a file under the data root's `env/`,
`agency-hooks/<point>.token`, and is registered again from them as the server starts; taking it away removes
both. `road-verbs` is what the procedures tool asks a room's road.

### supervisors

| Method | Path | Does |
|---|---|---|
| GET | `/supervisors` | Every supervisor the caller reaches, each with the account or unit that owns it. |
| POST | `/supervisors` | Register a supervisor; it is given its brain. |
| GET | `/supervisors/{supervisorName}` | One supervisor the caller reaches: its row, order, warming, threshold, gates, sampling, voice, and what it stands on now. |
| GET | `/supervisors/{supervisorName}/rooms` | The rooms a supervisor's turns went into, each with its last run there. |
| GET | `/supervisors/{supervisorName}/reset-point` | A supervisor's reset point alone. |
| GET | `/supervisors/{supervisorName}/road` | A supervisor's one road, and whether it can take a turn now. |
| GET | `/supervisors/{supervisorName}/settings` | A supervisor's settings, its system prompt and its room sessions. |
| PATCH | `/supervisors/{supervisorName}/settings` | Change a supervisor's reference switch, persona, generation settings, registration or thinking. |
| GET | `/supervisors/{supervisorName}/sleep` | Whether a supervisor sleeps, at what hour, and its record. |
| PUT | `/supervisors/{supervisorName}/sleep` | Set whether a supervisor sleeps and its own hour. |
| POST | `/supervisors/{supervisorName}/resets` | Start a supervisor afresh in the ranges named; `dryRun` counts it first. |
| PATCH | `/supervisors/{supervisorName}` | Change a supervisor's label, call names, road, warming, threshold, gates, sampling or voice. |
| DELETE | `/supervisors/{supervisorName}` | End a supervisor's registration; `forgetBrain` lets its brain go with it. |

- **Every supervisor is a row** (`docs/supervisors.md`), and these routes add, change, reset and end any of
  them alike.
- **A supervisor is owned by a login account,** `ownerAccount`, the id of the user it belongs to, the same id
  an account's `owner_account` names (`docs/store.md`), **or stands under an organisation unit,** `ownerUnit`,
  never both and neither for the platform's. A member named on the call
  registers, changes, resets and ends the supervisors of their own account and, for a supervisor filed under a
  unit, only where they head it, and registers one under either (「Who a caller acts for」); the platform's own
  call writes the owner it is handed.
- **A supervisor answers its row:** `name`, `label`, `ownerAccount`; `aliases`, its call names as one line
  joined by a comma and a space, or null for none (`docs/supervisors.md`, 「Its call names」); `sessionScope`,
  `turnShape`, `brainFolder`, `repository`, the repository its TODO items go to; its road, `provider`, `model`, `effort`,
  `thinking`, `answerTokens` and `advisor` (below), every turn of it going out there; `cardRank`, the rank of the demands of
  the runs for it, or null; `warming`, for each provider whether its prefix is kept warm there; `foldRatio`, its
  compaction threshold; `gates`, `voice` and `sampling`; `seeded`, true for a supervisor the deployment seeds,
  which `DELETE` refuses 409 `seeded`; and `resetPoint`, the moment in epoch milliseconds
  its last reset cut its turns off at (`docs/supervisors.md`, 「What the reset cuts off」), or null where it
  was never reset. The reset also publishes each move of the point as it makes it (「Events」), so a consumer
  that missed one reads the point again here or at `GET /supervisors/{supervisorName}/reset-point`.
- **`GET /supervisors/{supervisorName}/road`** answers whether the supervisor's road can take a turn now, judged
  as a turn's start judges it (`docs/provider-order.md`, 「Whether a road can answer」): `supervisor`, its
  road's `provider`, `model`, `effort` and `thinking`, `answers`, true where a turn would go out on it now,
  and `reason`, the sentence a turn would fail with where it cannot, null where it can. A consumer that holds a
  room's turn while Claude Code's accounts rest reads it before it lets the turn wait: a supervisor whose road
  is on another provider answers through the rest. 404 `not-found` for no supervisor of the name.
- **`GET /supervisors/{supervisorName}/reset-point`** answers `supervisor` and `resetPoint` alone, the same point
  the full read answers, for a consumer that reads it on every turn it sends; 404 `not-found` for no supervisor of
  the name. The point's one source is agency's store, which the reset writes; the event and both reads are views
  of it, and a consumer keeps no point of its own.
- **`GET /supervisors/{supervisorName}/rooms`** answers `items`, every room the supervisor's turns went into, as
  the run ledger records them (`docs/run-records.md`, 「The run ledger」), the latest first: `roomId`; `runs`, how
  many of its runs the ledger holds there; and `lastRunId`, `lastRunUnit`, `lastStartedAt` and
  `lastFinishedAt` in epoch milliseconds, its latest run there, which a screen opens its log by. Which rooms a
  supervisor answers is the room's owner's — the receiver keeps every room's answerers (`docs/supervisors.md`,
  「Who a send goes out as」) — so this read names where it has spoken, never where it may; a room it answers and
  never spoke in is not listed, and a room forgotten takes its runs with it. 404 `not-found` for no supervisor of
  the name.
- **A road** is `provider`, a provider agency carries, `model`, `effort` (a level of the reasoning policy or
  null), `thinking` (`on`, `off` or null for the provider's own), `advisor` (a model of the provider's catalogue
  or a family word its program takes, on a provider whose `takesAdvisor` is true, or null for none) and, on the
  registry's provider, `answerTokens` or null (`docs/provider-order.md`, 「What a road is」, 「The advisor」). The
  same shape is an agent's.
- **`POST /supervisors` takes the registration,** each field judged in this order and nothing written until
  all have passed (`docs/supervisors.md`, 「Adding and removing a supervisor」): `name`, a lowercase letter,
  then lowercase letters, digits and hyphens, forty characters at most, held by no supervisor yet, 409
  `name-taken` where one holds it; `label`, given; the road, `provider` and `model` with `effort`, `thinking`,
  `answerTokens` and `advisor` where it names them, judged as a road is (below);
  `repository`, a repository the store holds, or none; `ownerAccount`, none where the caller gives none, or
  `ownerUnit` where the body files the supervisor under a unit the caller heads; and
  `foldRatio`, the first provider's declared default where none is given. The row is written as one added
  on the console is: the `room` scope, the `pieces` shape, warmed on its road's provider, the brain folder
  `agents/<name>`, and no prompt folder.
- **A supervisor added on the console is added through the receiver,** which owns the rooms it answers: the
  console asks the receiver, the receiver writes the console's room, the supervisor's number there and the
  room's `supervisors` list, and asks `POST /supervisors` for the supervisor's half. Agency holds no room's
  list.
- **A road is refused** 400 `bad-request` with `field` beside `error`, naming the field: no provider or no
  model, a provider agency does not carry, a provider whose program does not carry the chat line, a model the
  provider does not hold — a catalogue that reads and does not name it, a member its gateway road does not
  hold, a registration with no address —, an effort the model does not take, and an advisor (`field`
  `advisor`) on a provider that takes none, one that is neither a model the catalogue holds nor a family word,
  or one the road's model is not paired with (`docs/provider-order.md`, 「The advisor」). A catalogue that
  cannot be read refuses nothing. A body naming `order`, `quotaShare` or `active` is refused 400 with that field: a
  supervisor holds no order and nothing is forced over its road.
- **The answer is 201,** `supervisor`, as the read answers it, and `brain`, what became of its brain:
  `made`, `kept` where one already stood under its key, `none` where no brain is deployed, or `failed`, the
  failure logged. The registration stands whatever the brain answers; the brain is filed under its owner's
  id as the account key where it names an owner, and under `AGENCY_BRAIN_ACCOUNT` where it names none.
- **`PATCH /supervisors/{supervisorName}` changes the fields the body names:** `label`, `aliases`, the road —
  `provider` and `model`, both or neither, `effort`, `thinking`, `answerTokens` and `advisor` —, `warming`, `foldRatio`,
  `cardRank` (a whole number of zero or more, or null for none), `repository`, `gates`, `sampling` and
  `voice`, each judged as the registration judges it; a road's fields it does not name keep what the road
  holds, except that a changed provider or model drops the effort, thinking, answer ceiling and advisor the body
  does not name, since they were the old model's. `aliases` is the whole list of the supervisor's call names, or
  null or an empty list for none: at most ten, each one to forty characters once trimmed and holding no comma,
  else refused 400 `bad-request` with `field` `aliases`. A body naming `order`, `quotaShare` or `active` is
  refused 400 with that field. A changed road — its provider, model, effort, thinking or answer ceiling —
  reaches the supervisor's next turn, and what its sessions held is replaced on it through their handoff: the
  one session across rooms is asked for its handoff through the receiver and let go, each room's own session
  writes its note in one more turn on the road it was opened on and its next generation opens on it, and the
  program standing on one of them ends with them so the pane no longer shows the old road. The replacement is
  owed, not made under a turn: a turn already going finishes where it went out, and its end makes the
  replacement while the agent is quiet (`docs/supervisors.md`, 「Where a supervisor's turn runs」). The route
  answers once the change is written and the replacement owed, never waiting for a handoff: the replacement
  is made behind the answer.
- **`DELETE /supervisors/{supervisorName}` ends a registration,** taking `force`, `forgetBrain` and `rooms` in
  its query:
  - 404 `not-found` for no supervisor of the name;
  - 409 `seeded` for a supervisor the deployment seeds, whatever `force` says;
  - 409 `running` for a supervisor a turn holds now, unless `force`, which ends it under the turn.
  - Its conversation is reset first (`docs/supervisors.md`, 「Starting it afresh」), each of the `rooms`
    named — a list of room numbers joined by commas — read as a room it shares, so the rooms keep their
    record; its person range is reset with it where `forgetBrain` is true.
  - Then in one write its settings, its base prompt choice, its session reset asks, its gate switches, its
    sampling, its reset point and its row; then what stands for it: its program, its workspace, its place and home folder, its
    execution rows, the agent host's records of it and its lock. What could not go is logged.
  - **Its brain is data and stays,** unless `forgetBrain` is true.
  - The answer is 204.
- **`GET /supervisors/{supervisorName}/settings`** answers a supervisor as its settings page shows it
  (`docs/supervisors.md`, 「Its settings」): `agent` and `label`; `icon`; `builtin`, whether the deployment
  seeds it; `turnShape`; `provider` and `model`, its road; `registration`, the registration its road names
  where it stands on the registry's provider, or null, and `registrations`, every offered registration, each
  `name` and `label`; `entries`, the model files behind that registration, `serving`, the one its engine
  serves now, and `modelUse`, the engine's latest use; `settings` — `referenceOn`, `brainOn`, `toolsAllowed` (a
  list of capabilities, or null for everything), `persona`, `generation`, `updatedAt` and `updatedBy`; `generationSettings`, every generation setting with its bounds, as a
  repository's `samplingSettings` are answered (「repositories」), `max_tokens` held under the registration's
  window; `thinking`, on, off or null for the provider's own; `personaFile`, the persona its prompt folder's
  file holds; `basePrompt` and `basePrompts`; `system`, the system prompt its next turn sends —
  `characters`, `hash` and `text` — where its turn shape is `pieces`, and null for the `window` shape; and
  `sessions`, its room sessions, each `chatId`, the room's `title`, `generation`, `systemResetAt`,
  `resetWaiting` and `lastResetAt`. 404 `not-found` for no supervisor of the name.
- **`PATCH /supervisors/{supervisorName}/settings`** takes `referenceOn`, `brainOn`, a switch, `toolsAllowed`,
  a list of capabilities the supervisor could hold or null for everything (`docs/supervisors.md`, 「Its
  settings」), `persona`, `generation`, an object
  of settings by name, a null letting one go back to the engine's default, `registration`, an offered
  registration its road is moved to on the registry's provider, or null, which leaves the road as it stands,
  since a supervisor always names a road,
  `thinking` and `by`, who saves, each judged before anything is written; a field it does not name is
  refused 400 with `field` beside `error`, and so is a value outside its setting's bounds. A save that
  changes its generation settings, its reference switch, its brain switch or its tool limit asks a reset of
  every room session it holds, each carried out by its handover. One that moves its thinking or its
  registration moves its road, which replaces its sessions through their handoff as the row's route does,
  behind the answer (「`PATCH /supervisors/{supervisorName}`」 above). The answer is the settings read as it then stands.
- **`GET` and `PUT /supervisors/{supervisorName}/sleep`** read and set the supervisor's sleep (`docs/sleep.md`,
  「Its setting」). The reading answers `on`, `hour` (its own, 0 to 23, or null for the policy's), `defaultHour`
  (the policy's), `sleptOn` (the day it last slept for, `YYYY-MM-DD`, or null) and `triedAt` (the moment it last
  tried, or null); a supervisor whose setting was never written answers `on` true and `hour` null. The setting
  takes `on`, required, and `hour`, a whole hour from 0 to 23 or null, each refused 400 otherwise, and answers the
  reading as it then stands. A supervisor the store does not hold is answered 404.
- **`POST /supervisors/{supervisorName}/resets`** starts a supervisor afresh (`docs/supervisors.md`,
  「Starting it afresh」): `conversation` and `person`, the ranges, at least one switched on; `alone` and
  `shared`, the rooms the caller says it answers alone and those it shares with other supervisors, each a list
  of room numbers; `dryRun`, true to count what it would take and write nothing; and `by`. It answers 200
  with `agent`, `label`, `ranges`, `alone` and `shared`, each room's `chatId` and `title`, `rows`, what it
  took or would take of each table and range, `files`, the harness records, and `brain`, the brain's
  answer for the person range or null. 400 for a body naming no range, and for a supervisor whose brain
  folder is the brain's root.

### relays

| Method | Path | Does |
|---|---|---|
| POST | `/relays` | Hand a line from one BMO to another person's BMO, recorded for both sides. |
| POST | `/chats` | Open a chat into another person's room and hand it a line, answered with that BMO's answer. |
| GET | `/relays` | The relays one BMO sent or received, the latest first. |

- **A relay is one BMO's word to another** (`docs/relays.md`). The body is `from`, the sending BMO;
  `to`, the receiving BMO; `text`, the line; `room`, the room it is said in; and `person`, the receiving
  person's account id where the sender names one. The receiving BMO's own session is started or taken up and
  the line goes into it as a turn, marked as another BMO's word and never privileged.
- **The answer is the record,** `{ "relay": Relay, "answer": <the receiving BMO's answer or null> }`, 201
  where the line was taken. A receiving BMO that answers the refusal marker is answered 409 `relay-refused`, a
  BMO in a turn 409 `relay-busy`, and a turn that failed 422 `relay-failed`; each is also recorded with the
  state it came to. An unknown receiving BMO is 404 `unknown-supervisor`, an unknown sender 422
  `unknown-sender`, a line to itself 409 `relay-to-self`, a line to another supervisor of the same person 422
  `relay-to-supervisor` (the two meet in a room one of them answers, by the name that wakes the other), and a
  `person` that is not the receiving BMO's account 422 `person-mismatch`.
- **`GET /relays`** takes `agent`, the BMO's name, and an optional `limit`, and answers `items`, the relays it
  sent or received, the latest first: `id`, `from`, `to`, `room`, `kind`, `line`, `state`, `answer`,
  `reason` and `createdAt`.

### roles

| Method | Path | Does |
|---|---|---|
| GET | `/roles` | Every role, every bundle a role may give and every line a role may name. |
| POST | `/roles` | Add a role. |
| GET | `/roles/{roleName}` | One role, with how many of its runs go and wait. |
| PATCH | `/roles/{roleName}` | Change any field of a role. |
| DELETE | `/roles/{roleName}` | Remove a role no run holds or waits for and no default names. |
| GET | `/roles/{roleName}/sessions` | The sessions the role's runs opened, newest first. |
| GET | `/role-agents` | Every agent of every role the caller reaches, by role and name, or one role's. |
| POST | `/role-agents` | Add an agent to a role. |
| PATCH | `/role-agents/{agentName}` | Change any field of an agent. |
| DELETE | `/role-agents/{agentName}` | Remove an agent that carries no run. |
| POST | `/composed-agents/{ownerAccount}` | Make a person's agents as the sign-up composition resolved for them says. |
| GET | `/signup-composition` | The sign-up composition standing for a unit or a person, and the subject's own link. |
| PUT | `/signup-composition/{link}` | Judge a link's sign-up composition and write it on the delegation chain in the caller's name. |

- **The agents an account is given** are `docs/role-agents.md`, 「The agents an account is given」: the
  composition is a setting of the delegation chain agency judges and writes itself, the agents made by the
  deployment's own call alone, and a role off for a member refused their `POST /role-agents` with 403
  `role-off`.

- **The roles are the store's rows** (`docs/agent-roles.md`, 「What a role row holds」), and these routes are
  the one way to change them.
- **A role is answered with every field of its row:** `name`; `code`, its kebab-case word, which is its name
  and what a screen shows the role by (`docs/agent-roles.md`, 「A role's label」); `label`, the English word a
  log keeps; `prompt`, its text or null; `description`, its text or null; `bundles`, each `name`, `code` (the
  name with each dot a hyphen, so `room.chat` is `room-chat`) and `label`; `skills`, the names of the skills
  its runs carry, or null where it sets none and its runs carry the whole plugin the release places for it;
  `line`; `agents`, its agents in the
  order a run looks for a free one, each as 「role agents」 below answers it; `maxConcurrent`, how many of
  them are switched on; `followsAsker`, whether its agents follow the supervisor that asks a run of them, each
  one's own road standing where no supervisor's does or that road cannot answer now (`docs/role-agents.md`, 「An
  agent that follows its asker」), an agent's own setting standing over it; `followsSupervisor`, true while each
  of its switched-on agents names no road or follows its asker, so its runs a supervisor asks go out on that
  supervisor's road (`docs/role-slots.md`, 「A role that follows its supervisor」) and a screen shows its road as
  the supervisor's; `providerPrompts`, each text its runs are given beside its
  prompt by the provider their program stands on (`docs/agent-roles.md`, 「A role's provider prompts」); and
  what stands of it now: `running` and `waiting`, how
  many of its runs go and wait, `provider`, the provider its next run goes out on, or null where none of its
  agents' roads can answer now or none names a road, and `waitingFor`, why its runs wait, or null, null too
  while it follows the supervisor.
- **`GET /roles` answers `roles`, `bundles`, `skills` and `lines`:** every role in the order a screen lists
  them, every bundle the policy declares with its `name`, `code` and `label`, the name of every skill a
  plugin of the release carries, which a role or an agent may name, and every line agency carries a role may
  name.
- **`POST /roles` takes `name`, `label`, `bundles` and `line`,** and may add `prompt`, `description`, `skills`,
  `providerPrompts` and `followsAsker`, false where it names none, and
  answers 201 with the role, which holds no agent yet. It is refused 422 `unprocessable-entity`, with one
  English sentence and nothing written, for a name out of the roles policy's pattern or past its length or
  already held; an empty label or one past the policy's length; a bundle the policy does not declare; a line
  agency does not carry; a prompt or description that is not text; `skills` that is not a list of names or
  names a skill no plugin of the release carries; `providerPrompts` that is not an object naming providers of
  this build, each with text or null; `followsAsker` that is not true or false;
  and `order`, `quotaShare`, `maxConcurrent`, `active`, `sessions` or `foldRatio`, none of which
  a role holds: each is its agent's (「role agents」 below). A role is the platform's preset: a member's `POST`,
  `PATCH` or `DELETE` of one is refused 403 `forbidden` (「Who a caller acts for」).
- **`PATCH /roles/{roleName}` changes the fields the body names** and answers 200 with the role, each judged as
  `POST` judges it; a `prompt`, a `description` or `skills` given as null clears it; `providerPrompts` sets the
  text of each provider it names and takes away each it names as null, and leaves every other provider's as
  it stands. A role removed takes its provider prompts with it. 404 `not-found` for a role the store
  does not hold. A change reaches the role's next run; a run already let in finishes under the row it began
  under.
- **Role agents.** `GET /role-agents` answers `items`, every agent the caller reaches by role and by name,
  narrowed to one role's by `role` and to one owner's by `ownerAccount`, `unit` and `unitSubtree` (「Who a
  caller acts for」); each is its row (`docs/role-agents.md`, 「What an agent row holds」):
  `name`, `role`, `ownerAccount`, `ownerUnit`, `homeNode`, `provider`, `model`, `effort`, `thinking`, `answerTokens`, `advisor`, `enabled`,
  `keepsCache`, `sessions`, `foldRatio`, `prompt`, `bundles`, `skills`, `addedAt`, `followsAsker`, true or false
  where it sets whether it follows its asker and null where its role's setting stands (`docs/role-agents.md`, 「An
  agent that follows its asker」), with `followsSupervisor`, true where it names no road or follows its asker by its
  own setting or its role's, and `carries`, whether a run stands let in on it now. A role's `agents` are answered
  the same way. `POST /role-agents` takes `role` and
  may add any other field, `name` being the role and a number where none is named, and answers 201 with the
  agent; `PATCH /role-agents/{agentName}` changes the fields the body names, a field given as null cleared,
  and answers 200. Both refuse 422 a role the store does not hold (`unknown-role`), a name out of the
  pattern, a provider without a model or a model without a provider, a road judged as a supervisor's road is
  against the role's line, a bundle the policy does not declare, and a `followsAsker` that is not true, false or
  null; `POST` refuses 409 a name
  already taken. `DELETE /role-agents/{agentName}` answers 204, 404 for an agent the store does not hold and
  409 while it carries a run. An agent added, switched or removed holds from the next admission, and one
  more seat lets in at once the reserved slots it frees. A member named on the call adds agents under their
  own account or, naming `ownerUnit`, under a unit they head, and changes and removes those and the seats
  filed under the units they head alone; a body naming both owners is refused 422 (「Who a caller acts
  for」).
- **`DELETE /roles/{roleName}`** answers 204; 404 `not-found` for a role the store does not hold, and 409
  `conflict` while a run of it goes or waits, or for a role the roles policy names as a default.
- **`GET /roles/{roleName}/sessions` answers `items`,** the sessions the role's runs opened, newest first, a
  page asked with `before` and `limit`, each with its `sessionId`, `path`, `provider`, `model`, `effort`,
  `account` (null off a provider that holds logins), `openedAt` and `lastAnsweredAt` in epoch milliseconds,
  `runs`, how many runs it carried, `context`, the tokens its last answered turn carried and the share of the
  window they filled, and `valid`, whether the next run of that key would resume it (`docs/sessions.md`,
  「The session history of a role」).

### own-agents

| Method | Path | Does |
|---|---|---|
| GET | `/own-agents/{ownerAccount}` | The agents supplied to a person, their own changes over them, and what they may change. |
| GET | `/own-agents/{ownerAccount}/models/{providerName}` | The models of a provider a person may stand an agent on, each with whether their supply carries it now. |
| POST | `/own-agents/{ownerAccount}` | Add an agent of the person's own to a role their organisation leaves on. |
| PATCH | `/own-agents/{ownerAccount}/{agentName}` | Lay a person's agent on another road, or switch it on or off. |
| POST | `/own-agents/{ownerAccount}/{agentName}/back` | Stand a seat supplied to the person again as supplied, made where it no longer stands. |
| DELETE | `/own-agents/{ownerAccount}/{agentName}` | Remove an agent of the person's own; a supplied one is switched off instead. |
| GET | `/own-instructions/{ownerAccount}` | A person's own instructions, whether they may give any, and the organisation's, which stand before them. |
| PUT | `/own-instructions/{ownerAccount}` | Replace a person's own instructions, written on their own link of the delegation chain in their name. |

- **What they do is `docs/role-agents.md`, 「A person's own agents」, and `docs/prompt-layers.md`, 「The person's
  instructions」.** A caller naming a person with `Owner-Key` and `Owner-Role: member` acts on that person's own
  account alone, any other account in the path refused 403 `forbidden`; the deployment's own caller acts on the
  account the path names, as the console does for the signed-in person. These paths stand under no part of agent
  management: whether the person may make a change is the delegation chain's word for that person, never a console
  section.
- **`GET /own-agents/{ownerAccount}`** answers `account`; `add` and `road`, each `allowed` and `from`, the link that
  said it where one did; `roles`, every role the store holds, each with `role`, `label`, `description`, `line`,
  `off` and `offFrom`, `most`, the most agents of one role a person holds, and `agents`, the seats supplied in their
  order and then the person's own, each `name`, `agent` (the row as `GET /role-agents` answers it, null for a seat
  supplied that no longer stands), `supplied` (`road`, its `provider`, `model` and `effort` or null to follow the
  asker, and `from`, the link whose composition supplied it or null for the platform's own rule; null for an agent
  of the person's own) and `changed`; and `providers`, every provider the build carries with its `name`, `label`
  and `lines`, each with `off` — `owner` where the owner's switch stands it off, `chain` where the person's chain
  closes it, `from` naming the link that said so, and null where its sources stand open for the person
  (`docs/supply.md`, 「A closed provider is read as closed, not as absent」).
- **`GET /own-agents/{ownerAccount}/models/{providerName}`** answers `provider` and `models`, the provider's offered
  models less any another person owns and any filed under a unit the person's account does not stand on, each
  `id`, `efforts`, `inService` and `supplied`, whether the person's supply
  carries it now — which a provider the chain closes leaves every model of without exception.
- **`POST /own-agents/{ownerAccount}`** takes `role` and, optionally, `provider`, `model` and `effort`; it answers
  201 with the agent. Refusals: 403 `own-agents-withheld`, 403 `role-off`, 422 `unknown-role`, 409
  `too-many-agents`, and for a road named 403 `road-choice-withheld` and 409 `road-not-supplied`.
- **`PATCH /own-agents/{ownerAccount}/{agentName}`** takes a road — `provider` with `model` and `effort`, or both
  null to follow the asker — and `enabled`; 404 `not-found` for no agent of the person's, 403
  `road-choice-withheld` and 409 `road-not-supplied` for a road, 403 `own-agents-withheld` for a supplied agent
  switched off.
- **`POST /own-agents/{ownerAccount}/{agentName}/back`** answers the agent standing as supplied; 404 `not-supplied`
  for a name no seat supplied to the person holds. **`DELETE`** answers 204; 409 `supplied-agent` for a seat
  supplied and 409 `agent-busy` while it carries a run.
- **`GET /own-instructions/{ownerAccount}`** answers `allowed` (`allowed`, `from`), `all`, `agents` (a text by an
  agent's name, the names of agents the person no longer holds left out), `named` (the person's supervisors and role
  agents, each `name`, `label`, `kind` and `role`), `organisation` (every link's text of the organisation's
  instructions, outermost first, each `from` and `text`) and `mostCharacters`. **`PUT`** takes `all` and `agents`
  and answers the same; 403 `instructions-withheld`, 422 `bad-instructions` for a text out of shape, 422
  `unknown-agent` for a name that is no agent of the person's, and the account service's own refusal of the write
  as it answered it.
- Every route answers 503 `access-unchecked` where the account service does not answer.

### prompt-documents

| Method | Path | Does |
|---|---|---|
| GET | `/prompt-documents` | Deployed prompt documents: base prompts, rulebooks, personas, agent definitions, system prompts. |
| GET | `/prompt-documents/{documentName}` | One prompt document with its version. |
| PATCH | `/prompt-documents/{documentName}` | Change a base prompt or its chooser. |
| GET | `/rulebooks` | The shared rulebooks named, as the deploy tree holds them. |

### chat-documents

| Method | Path | Does |
|---|---|---|
| GET | `/chat-documents` | The supervisor's own documents the owner may keep — its opening, the absolute rules and the manual — each with the text it stands as, where that text comes from, and its version. |
| PATCH | `/chat-documents/{documentName}` | Save the owner's text of one document, or reset it to its file. |

- **A document stands as the owner's text where one is saved,** and as its file under the deploy tree
  where none is; `source` says which, and `version` is the moment the stored text was last saved.
- **The three names** are `opening`, `absolute-rules` and `manual`; any other name is refused 404.
- **A save** takes `text` of 1 to the bound the base prompts' policy states, and `by`, the caller's
  name; an empty or oversized text is refused 400. **A reset** takes no text and lets the file stand
  again.
- **The chat line reads them afresh at every launch,** so a save reaches the next turn with no
  restart; a save that changes the composed prompt hands the session over, as a changed document does
  (`docs/supervisors.md`, 「A session in each room」).

### document-moves

| Method | Path | Does |
|---|---|---|
| POST | `/document-moves` | Word that session documents moved, so sessions read them again. |

### sessions

| Method | Path | Does |
|---|---|---|
| GET | `/sessions` | Every session, by agent or scope. |
| POST | `/sessions` | Start a session at a scope: a supervisor's, a role's run at a path, or a conversation. |
| GET | `/sessions/{sessionId}` | A session's state: provider, model, place, holder, why the prefix changed, the fold ratio. |
| GET | `/sessions/{sessionId}/streaming` | The block the session's answer is writing right now, as the model gateway counts it. |
| PATCH | `/sessions/{sessionId}` | Move a conversation's provider and model, or hand a role's kept session over. |
| DELETE | `/sessions/{sessionId}` | End a session, or clear a room's session. |
| POST | `/sessions/{sessionId}/turns` | Send a turn into a session. |
| GET | `/sessions/{sessionId}/turns/{turnId}` | A turn and what it has written so far. |
| PATCH | `/sessions/{sessionId}/turns/{turnId}` | Interrupt a turn. |
| POST | `/sessions/{sessionId}/turns/{turnId}/inputs` | Push words into a working turn. |
| POST | `/sessions/{sessionId}/words` | Say words to a run's session, offered to the turn it is working. |
| POST | `/sessions/{sessionId}/answers` | Answer a question a turn asked. |
| POST | `/sessions/{sessionId}/folds` | Fold a session, compacting its context. |
| POST | `/session-reset` | Ask to clear one agent's session in a room — `agent`, `chatId?`, `by?`, `clean?` (`docs/session-resets.md`) — answered with the session as it stands after; the next round opens a new one. |

**The block a session's answer is writing.** `GET /sessions/{sessionId}/streaming` answers what the model
gateway holds of the answer the session's program is streaming now (`docs/own-llm.md`, 「The gateway」):
`index`, `kind` (`thinking`, `text` or `tool`) and `bytes`, the bytes that block has written so far. It is read
for a supervisor's session, by the program session the supervisor holds; it answers 204 with no body where
the session is no supervisor's, its supervisor holds no program session, its road does not go through the
gateway, or no block is being written now, and where the gateway does not answer. A turn's lines say a block
only once it has ended, so this is the one place a block still being written is read.

**A question in the middle of a turn carries what the screen shows, never a choice among options.** A
turn whose program holds the pane at a permission question, or at another screen the reading does not
know, carries the question on the session's state and publishes it as `question.created`;
`POST /sessions/{sessionId}/answers` answers it with `allow`, `leave` or `refuse` (`Answer`), the
question's `Asks` naming a permission question or an unknown screen. The CLI's own question tool puts
a choice among options, which that shape cannot answer; no run on the worker line is shown it, and a
job asks its supervisor through the job service's question, which the supervisor settles or puts to the owner
(`docs/launch-lines.md`, 「Worker line」).

A job's run is a role's run at its checkout. A session is started at one scope — `agent` or `room` for a
supervisor, `role` for a role's run, or `conversation` — and the scope says what is kept. Its id names its
owner and the scope: the supervisor's name alone for its session, `<supervisor>@<room>` for a room's, and a
number the service gives it for a role's run (`<role>@run:<n>`) and a conversation
(`<supervisor>@conversation:<n>`), so every start of one of those is a session of its own; a number whose
role's session's place the process before left standing for a turn picked up across a restart is passed over.

A conversation is the caller's: its start names the model, the provider (Claude Code where it names none) and,
where it names one, the folder its turns go out on, and its turns are ordinary prompts with the context around
them and no work parts. A provider whose program keeps a session continues the one its last answered turn left.
The session stands in the server's memory from its start to its end.

**There is no `worker` scope.** A run in a repository is a role's run at its checkout's path: its sender asks
the slot and starts the role's session on it. A start naming `worker` is refused 400; no list names a
`<repository>@worker` session, and its state, change, end and fold routes and a turn sent to it answer 404
`unknown`.

A role's session is the session of one run of a role (`docs/agents.md`, 「A run in a pane of its own」). It runs
on the slot its caller was let in on (「slots, reservations and places」): its start names `slot`, that slot
request's token, and the session runs as the slot's role on the provider, model and effort the slot was let
in on, on the role's line.

- **`slot`** must name a request that is `active`; a token no request holds, or one waiting, reserved or given
  back, is refused 422 `slot-not-active`.
- **`role`**, where the start names one, is the slot's role; another is refused 422 `contradicts-slot`, and a
  role the store does not hold 422 `unknown-role`.
- **`provider` and `model`**, where the start names them, are the slot's own; either differing is refused 422
  `contradicts-slot`, so a start never moves a run off the road its slot was let in on.
- **`cwd`** is the path the run works at: the one the start names, else the slot's `path`, else the line's own
  deployed directory.
- **A start naming no `slot`** is refused 400: a role's session runs on a slot and on nothing else.
- **A slot carries several sessions in turn.** A start naming a slot binds its lease to the session it opens,
  whether or not the lease was bound to another before, and that other session's end no longer gives the
  lease back; ending the session the lease is bound to does (`docs/role-slots.md`, 「The line」). A caller
  running sessions one after another on one slot starts the next before it ends the one before.

A role's session's first prompt carries no reference the gate reads: the gate reads a brain only for the
supervisor a call names, and a role's session names none. A job's first prompt carries the reading's
entries (「assignments and jobs」), and a co-work branch's prompt is composed by its parent.

A role's session may be handed its instructions whole, for a consumer that composes its own — the research unit,
which asks for the slots and the sessions its searches, browses, rounds and branches go out in, is one: the
start names `systemPrompt`, the instructions, and `line`, the runner line it goes out on — `search` or `browse`,
whose system prompt is the platform's harness opening and then that text, with the line's standing MCP
configuration and the tools it allows, or `plain`, whose system prompt is that text alone, with no tool and no MCP
server (`docs/launch-lines.md`, 「Search runner」, 「Browse runner」 and 「Plain runner」). A turn into such a session goes out on the role's line whole, never in a
standing branch checkout. Its prompt is the caller's, whole, and its answer is the run's reply text. Its turns
carry no clock: each lasts until it ends or is stopped. A start that hands no instructions opens on the role's
prompt. Beside them its road is checked, and the start may name the rest below:

- **The road's check:** a provider agency does not carry, one the owner switched off, or one whose program does
  not carry the line — the slot's for a role's session, the one named for a conversation — is refused 503
  `line-not-carried`, the sentence saying which (`docs/launch-lines.md`, 「Providers and what a route may
  promise」).
- **`demandId` and `within`:** on a registration naming a card service, a granted demand of the caller's
  own that every turn of the session stands on, with no other asked, or the caller's granted demand each
  turn's own is asked within (`docs/own-llm.md`, 「An engine the arbiter stands」); each a demand's id, a
  positive whole number, refused 400 otherwise. A start naming no `demandId` on a slot whose lease holds a
  demand stands on that demand.
- **`cardRank` and `cardPool`:** the rank a demand its turns ask stands at, a whole number of zero or more, and
  its pool, `key` and `cards`, a text that is not empty and a whole number of one or more; each refused 400
  otherwise (the arbiter's `docs/placement.md`, 「A demand's rank」, 「A pool」). Named by neither, the rank is that of
  the supervisor that asked the session's slot, its `askedBy`, as `docs/own-llm.md` says, and there is no pool.
- **`labels`:** an object the session's record carries, each key and its value written under that key of the
  record's `labels`, for the placed tools that read them; a key agency writes itself — `line`, `checkout`,
  `archive`, `role`, `place` and `reads` — is refused 400. The record's `reads` holds the media the run's
  model reads natively, as `docs/provider-order.md` 「The media a model reads」 judges them from the road the
  run goes out on.
- **`toolsAllowed`:** the capabilities the record keeps, of those the line and the session's role give it; a
  record keeps them all where the start names none, and the list never widens them.
- **`continues` and `continuesElse`:** the program's own session the session's turns continue, the
  `harnessSession` an earlier turn's end named (below), and the words a turn goes out with instead where that
  session cannot be resumed; each a text that is not empty, refused 400 otherwise, and `continuesElse` refused
  400 without `continues`. The named session is resumed while its record stands under the login the turn goes
  out on, whatever the agent's `sessions` setting and its cache's age; where it does not stand, the turn opens
  a new session on the `continuesElse` words, or on its own prompt where the start named none
  (`docs/sessions.md`, 「A role's session」).

**A role's session's turn goes out on the session's road.** Its provider, model and effort are the session's,
its folder the session's `cwd`, and its instructions and bundles its role's; a turn's body adds only what is
the turn's own, every part optional: `name`, the run's name, one agency gives where none is named; `title`,
what a person watching reads over the run, the role's name where none is named; `chat`, the room its run's
lines stand under, none where it names none; `unit`, the unit it runs under; `resume`, which session it goes
on — `{ "kind": "auto" }` where it names none, `{ "kind": "fresh" }`, or `{ "kind": "session", "id": … }`
(`docs/sessions.md`, 「A role's session」); `hearsWords`, whether words pushed into the turn reach it, true
where it names none, so every run stands to be spoken to unless its caller says otherwise; and `variables`,
what its program is started with beside its own. A turn's road is its session's: a turn body naming
`provider`, `model`, `mapping`, `workKey` or `tier` is refused 400.

Each turn of a role's session goes out in a pane of its own, on the session the role gives it: the one kept
for the role at the path on the provider and model where the role keeps sessions and it is valid, a new one
otherwise (`docs/sessions.md`, 「A role's session」). Agency asks nothing of such a session after a turn's
answer: any further turn, a round among them, is its caller's. A server that starts ends every role's session
the process before it left, since a role's session does not outlive the process that started it, except one
carrying a turn in flight, a job's among them: that turn is picked up from the session's place and the
session ends with it (`docs/sessions.md`, 「A restart hands the session over」). Before it takes one down it
writes to its log what it found there — the program the place records, whether that process is alive, and
what the pane's head was last answered — after closing the liveness rows that process left open and picking
up the turns it left in flight. A thrown-away session one of those turns still runs on answers its state by
its id while that turn goes on, and ends with it. The kept session it answered on stays for the next run of
its key.

A turn's context may name `round`, the receiver's name for the round of the room the turn belongs to,
which the turn's run keeps (「runs」 below).

A turn on the chat line ends its system prompt with the base persona: the base layer of the agent's own
brain, read at the turn's launch (「Calls agency makes」). The turn context's `basePersona` is the fallback,
standing in that place only where the brain gives none (`docs/sessions.md`, 「Document reload」).

A room's session is readable by its id from its start, before its first turn has opened a generation:
the read answers its supervisor's road and its place, at generation 0,
and its first turn opens the first generation on that road. A start writes nothing of the session, so the
list of sessions names a room's once its first turn has opened one. A supervisor's session is readable once
the supervisor holds one; a supervisor whose provider's program carries its own turns, dsh's, holds no session
id of its own, its turns going through its resident's inbox, so its session is readable from its start as a
room's is: its supervisor's road, its resident's context and its `foldRatio`.

A conversation moves its provider and model on the change route, each left as it stands where the body
names only the other, and starts its next turn on a fresh session where the road keeps one. A hand-over is a
role's kept session's: where a job's turn last answered on the session, the session is asked for its handoff
first, noted on the job's item (「assignments and jobs」); it then closes the session kept for the role at that
path on that provider and model and drops its record, and the holder the body names is kept for the session's
state. A supervisor's and a room's session go out on the supervisor's one road, and its row is where that
road is changed. `holder` and `prefixChangeReason` are read back on the session wherever
it is listed or read.

A supervisor's and a room's session carry `handoffLost`, true from the moment agency opened the session or its
generation in place of one whose handoff was not had — the receiver's ask failed, or a room's handoff turn
failed or wrote no note — until a turn of that session begun after it has answered, false otherwise. The receiver opens
that first turn on what it keeps of the session's answers (`docs/sessions.md`, 「A restart hands the session
over」).

A session's state carries `foldRatio`: the share of its model's window its context may fill before the
session is folded once the turn that crossed it is over, the compaction threshold of its supervisor or its
role, read afresh with each read, so a change on the console reaches the next read of every session. A room's
and a supervisor's session carry `context` beside it, the context their last turn left — `carried`, its
tokens, and `window`, the model's window — or null where no turn has ended on it since agency's process
started or since it last folded or ended; on a resident program that carries its own turns it is read off the
program's record (`docs/sessions.md`, 「Session compaction」). Beside it stands `foldsItself`: true for a
room's or a supervisor's session on such a program, which compacts itself at the session's `foldRatio` and
takes no fold call, and false for every other session. It carries `supervisor` or `role` beside them, the row
the session runs for, so a consumer whose settings are kept per supervisor matches the session by it.

A turn sent is carried to its end whether or not its sender still waits on the send: a sender that lets
go of the request, or goes down while the turn works, leaves the turn running, and the turn ends as it
would have, its run closed and its end published as `turn.ended`. Agency's own restart is no end of a
turn either: a turn whose program keeps its record in its place is picked up by the server that starts
next, and its end is published as `turn.ended` under the same `turnId`, a job's turn ending its job; a
turn no record names ends failed (`docs/sessions.md`, 「A restart hands the session over」). The progress
read answers a turn picked up after a restart under its `turnId` as it answered it before.

**A turn is re-attached to by its caller's key.** A send may name `key`, a text that is not empty and that
its caller does not give to another turn of the session; anything else is refused 400. The key is kept with
the turn while it goes and with its end once it has ended, and `turn.ended` carries it as `key`:

- **A repeat of the send with the same key on the same session** sends nothing: it attaches to the turn
  standing under the key, the one this server sent or the one it picked up at its start, and is answered
  with that turn's end as the first send would have been, its `prompt` and `context` the repeat's own.
- **A repeat after the turn has ended** is answered at once with its end, as kept: its `id`, `state`,
  `delivery`, `outcome`, `ending`, `reason`, `answer`, `answers` and `harnessSession`, `text` holding the
  answer. An end is kept for
  `kept_seconds` in `policy/turn-keys.toml`; a repeat after that, or one naming a key no turn of the
  session stood under, is a send of a new turn.
- **A send refused before its turn began** — the session in a turn, unknown, or the turn's body refused —
  keeps nothing under its key, and a repeat is asked as a new send.
- **A send that names no key** is carried as before: its caller follows it by the `turnId` on
  `turn.created` and `turn.ended`.
- **A send cut by the server's drain** is answered `503` `stopping` with `key`, the key it named, and
  `Retry-After`: the turn goes on in its program and is kept in flight, and the caller sends again with
  the same key once the server stands.

A turn is named from its start: `turn.created` carries its `turnId`, and the send answers with that same
id once the turn ends. The send's answer carries `harnessSession` beside it: the program's own session the turn
answered on, where its road reads one, and null otherwise; a repeat of a keyed send answers it as kept. The progress read answers the turn named, on every scope that carries turns, with
its `id`, `sessionId`, `prompt`, `context`, `state` (`running` or `ended`), `delivery`, `outcome`,
`answer`, `text`, the last holding every text block the turn has written so far, and `waiting`. The
progress read answers the turn named while it runs, and the session's last turn once it has ended; the
interrupt and the words pushed into a turn act on the turn going now. Each answers `unknown-turn` where the session
carries no turn of that id.

**The interrupt says whether the turn has ended.** It stands a stop over the turn's run, which the turn reads
wherever it stands — waiting for its card or its node, in its program, or after its program's end while the
reading of its price is waited on — and waits for the turn to end, up to
the interrupt's bound (`policy/run-stop.toml`). A turn that has ended within it is answered `200` with `state`
`interrupted` and `outcome` `stopped`; one still going is answered `202` with `state` `running`, `outcome` null
and what it waits for in `waiting`: the stop still stands over it, and its end comes as `turn.ended`. So no
answer says stopped of a turn that goes on. A session's turn that has ended keeps what it wrote until the session's next
turn begins.

**Words said to a run.** `POST /sessions/{sessionId}/words` hands a paragraph, and any files with it, to a
role's run by its session's id (`docs/agent-control.md`, 「Words that arrive while a turn is working」). Its
body is `WordsAsk`: `words` (`text` accepted as its older name), `by`, and `files`, each `name`, `type` and
`data`, the bytes base64-encoded; the files are written where the run's program can read them and named
before the words as said. The answer is `WordsSent`: `said` — `delivered`, `waiting`, `kept`, `refused` or
`started` — with the `sessionId`, `run` and `turnId` it went into or takes, `jobId` where the run is a job's,
and `reason`, why it did not go in, where it did not.

- **A session a job's turn ran in addresses the job** for as long as the job is `starting`, `running` or
  `reviewing`: the `sessionId` the job answers, and any session an earlier turn of the same job ran in, whether
  this process started that session or picked its turn up across a restart. Its words take the job's road
  (「assignments and jobs」, 「Words and notes for a running job」): never refused as ended while the job runs,
  and answered 410 `run-ended` only once the job has ended.
- **Who may say:** the owner, `by` naming `owner` or nothing, and the supervisor that asked for the run, the
  `askedBy` of the slot it runs on; for a job, its holder and the
  `askedBy` of its seat. Any other name is refused 403 `not-the-sender`; the control port's own bearer is what
  stands behind a name.
- **A run's session is a role's, never a supervisor's or a room's:** any other scope's session is refused 422
  `not-a-run`. A room's words go by the room's own push (`POST /sessions/{sessionId}/turns/{turnId}/inputs`).
- **The words are offered to the turn the run carries now,** and taken only where its provider declares it
  takes words mid-turn (`docs/agent-control.md`, 「Words that arrive while a turn is working」). For a run that
  is no job's, a turn that ends before it took them answers them refused, and agency keeps nothing: they
  belong to whoever said them, which may say them again as a turn.
- **A run that is no job's, on a provider that takes no words mid-turn,** answers them refused, with `reason`
  naming the provider; they are not kept for a later turn, since a run's next turn is its caller's to send.
- **A session that is no job's and stands with no turn working** answers 409 `no-turn`; **one whose run has
  ended** answers 410 `run-ended`; **an id no run stands on** answers 404 `unknown-session`.
- **The run's record says what was sent and what became of it:** a `prompt` line named `word` under the
  run, and a `note` line under it saying what became of them. A job's report ends with the same words, read
  off its run's log, so what a job was told mid-turn stands beside its answer (`docs/run-records.md`, 「The
  run ledger」).

**A running turn says what it waits for.** `waiting` is null while nothing holds the turn back, and while
the turn waits before its program is put to work it names the wait, typed: `for`, what it waits for —
`card`, a card the arbiter's master has not yet granted the run on a registration naming a card service
(`docs/own-llm.md`, 「An engine the arbiter stands」), or `node-program`, a program on a node that has stood
past its notice without taking the turn, or whose node's far end could not listen on a port the program reaches
home through (`docs/work-nodes.md`, 「When a node drops」), or `node-link`, a person's node whose link is not
heard, on which a turn of its account waits before it starts (`docs/person-nodes.md`, 「Which node is a
person's」) —, `service`, the card
service or the node, `reason`, why it waits as it stands now — the master's reason, the master unreached, the
gateway taking the granted card's entry into its table, or the program on the node not yet taking the turn, or the port its far end could not listen on, or the node's link not heard —,
and `since`, the moment in epoch milliseconds the wait began. A `node-program` wait the bound overtakes ends
the turn `unsubmitted`, its `reason` saying how long the program stood on which node, or which port its far end
could not listen on; a `node-link` wait its bound overtakes ends the turn failed, its `reason` naming the node
and how long its link went unheard, and a stop standing over its run ends it `stopped`. The wait is
published as `turn.changed` — `session`, `turnId`, `run` and `waiting` — as it begins, each time its reason
changes, and once more with `waiting` null as it clears, on the grant or on the wait's end, so a consumer
shows it on the room's working line from the event or from the read and never from the run's log. A turn
that ended waits for nothing.

A turn's end — the send's answer and `turn.ended` alike — carries `ending` beside `outcome`: the word the
turn's wait ended on, typed, and `reason`, why a turn with no answer has none, or null. A turn put into a
program's pane ends `answered`, `silent` (back at rest, its record closing the turn with no answer),
`errored` (the program's own error in the answer's place, or the failed request its program reported through its failure hook), `card-taken` (a demand of a better rank took the card of the demand the turn stood on, so the turn was stopped; what it wrote is unfinished and `reason` names who took it), `vanished` (herdr says its pane is gone),
`program-lost` (the program's process is gone and its record never closed the turn), `silent-peer` (no
program can be named for the turn, the pane shows none, and nothing the program writes moved between two
looks), `blocked`, `untrusted`, `unsubmitted`, `overran` or `stopped`
(`docs/agent-control.md`, 「Telling that a turn's program is gone」); a turn that went out on a road with no
such pane ends `answered`, `blocked`, `stopped` or `failed`, and a worker's turn put back before anything
went in `reprepared`. No turn whose program cannot answer is left waiting: each of these is an observed
event, and none is a clock's. A role's session's turn cut because the slot its session is bound to was given
back — by its holder's finish or its owner's release (「slots, reservations and places」) — ends `stopped`
with the reason `released`.

A turn's end carries `answers` beside `answer`: the answer of each turn of its program the round spanned, in
order, each `text`, that turn's last response, and `opened`, what opened that turn — `handed` for words handed
over, the round's own prompt or words pushed into it, and `program` for a turn the program opened on its own, a
background task's notice. `answer` is the round's answer built from them (`docs/agent-control.md`, 「The round's
answer」); agency judges none of them, and a consumer that weighs what a later turn answered against an earlier
one reads them here. A turn whose road reads no such turns carries `answers` empty, and `answer` alone stands. A
turn on a line that carries its background apart, the chat line, spans no turn the program opened on its own, so
its `answers` hold no `program` entry (`docs/agent-control.md`, 「Background work on a line that carries it
apart」).

Every route that takes a body names the request type it reads, holding only the fields that route reads;
`docs/openapi.json` names that type, and no route's request is a response type reused.

### turn-contexts

| Method | Path | Does |
|---|---|---|
| POST | `/turn-contexts` | The context the submit hook would add to a typed line. |

### runs

| Method | Path | Does |
|---|---|---|
| GET | `/runs` | Runs newest first, of the owners the caller reaches: open or closed, by agent, room, round, names or the search they carried; with the tokens spent so far. |
| POST | `/runs` | Open a run in the ledger, under the title a body names. |
| GET | `/runs/{runId}` | One run the caller reaches, its clock among its fields. |
| PATCH | `/runs/{runId}` | Close a run, or stop it, or attach the message its answer went out as. |

A run is a row of the ledger (`docs/run-records.md`, 「The run ledger」): a unit the job unit opens and
closes, a job's turn, which agency's job runner opens and closes, and a chat turn's run and a role session's
turn's run, which agency opens as the turn starts and closes as it ends. Each
answers `title`, what the run was asked to do: a role session's turn's title, or the title the body opening
it named, else null; `roomId`, the room it ran for; `round`, the round the receiver named in the turn's context; and
`messageId`, the message its answer went out as once the receiver attaches it, the latest message the
turn answers until then, or null where the turn named neither. A chat turn's, a role session's turn's and a
job turn's run answer
what the turn spent once it is closed: `usage`, its `inputTokens`, `outputTokens`, `cacheReadTokens` and
`cacheWriteTokens` summed over the models that answered, and `spentTokens`, the four together; both are
null on a run whose road reported no figures, and on an open run. Every turn of a round, each question,
item and check of the draft process among them, is a run of its own under the round.

The list answers the newest runs first, `limit` of them (50 where it names none, at most 500), and pages
by a run's id: with `before`, the runs before that id; with `after`, the runs after it nearest to it; with
neither, the newest. `more` says whether more runs lie beyond the page in the direction it was asked —
older ones, or with `after` newer ones. It narrows by `live` — `true` the runs still open, `false` the
closed ones —, by `agentName`, by `roomId`, by `round`, and by `names`, a comma-separated list each
matched against a run's work unit and its round, every filter named holding at once; `search` answers instead
every run that carried that search, newest first. By `ownerAccount`, `unit` and `unitSubtree=true` it narrows
to one owner's runs — a unit and, with the subtree, every unit under it (「Who a caller acts for」) — and a
member sees only the runs standing under the accounts and units they reach. A query naming anything else, or a
value that does not read as its filter's kind, is refused 400. Each run answers `ownerAccount` and `ownerUnit`,
the owners its agent was seated on when it opened, and `unitName`, the name its log lines are written under —
its work unit, which names no organisation unit. A change naming `messageId` alone attaches the message, on an open or a closed run alike, and
leaves the run's closing as it stands. A closed run answers `counts.tools`, how many tool calls its log
lines held as it closed; an open run's `counts` is empty.

### background-work

| Method | Path | Does |
|---|---|---|
| GET | `/background-work` | The background work every supervisor's program still runs after the turn that launched it, oldest first; by room. |

A supervisor's background work is a shell command or a subagent its program put in its own background and that
outlived the turn that launched it (`docs/agent-control.md`, 「Background work on a line that carries it apart」).
Each answers `task`, the program's id for it; `kind`, `shell` or `subagent`, or whatever else the program names
it; `description`, what it was launched as; `command`, a shell's command, else null; `roomId`, the room the
launching turn was in, null for work launched in no room; `agentName`, the supervisor whose program runs it, or
null; `launchedAt`, the moment its launch was written down, which is the stop of the turn that launched it; and
`status`, `running` on the list. The list holds the work still running and nothing that ended, was stopped, was
lost with its program or was dropped by it, and nothing the background watch already settled; it narrows by `roomId`, and a value that does not read as a whole number is refused
400. It is read from every supervisor's background record as it stands at the call, so it answers a launch within a
moment of its being written, ahead of the watch's own look; an end the record does not hold — a notice the
program took up without a submit, or work it dropped — leaves the list on the watch's look that settles it.

### log-lines

| Method | Path | Does |
|---|---|---|
| GET | `/log-lines` | Log lines across runs: by run, job or agent, kinds, before or after. |
| POST | `/log-lines` | Add a note in the bot's own words to a run's log, beside the run's own lines. |
| GET | `/log/agents/{agentName}/lines` | One agent's log across its runs: the newest page, a page before or after an id, or the page at a moment. |

`GET /log-lines` reads the lines that meet every filter it is given: `runId`, `jobId` and `agentName` (the
agent the lines stand under), at least one of them, else it is refused 400; and `kinds`, a comma-separated
list of line kinds. With no cursor, or with `after`, it reads the page after that line id; with `before`
alone, the page just before that line id; with both, the lines between them. Each page stands oldest line
first and answers `items`, with `more` saying further lines wait the way it was read; `limit` is 500 where
left out and at most 500.

A signed-in member's read of these routes, and of a job's lines, stands over the agents they reach — one's own
account's and those of the units they head — alone: a line written under an agent outside that reach goes from
every page and count, the reached names standing inside the query so a page keeps its cursors whole, and a log
whose every agent lies outside reads as none stands there. A role's name carries no owner of its own, so its
page goes on over the agents of it the reader reaches. A note a member posts `POST /log-lines` goes into the
log of a reached agent only, refused 403 `forbidden` elsewhere; the deployment's own call and the
administrator's read and write every line (「Who a caller acts for」).

`runId` names a run as its lines were written under it, or a run of the ledger by its id, whose lines
are the ones written under its unit. Each line answers the ledger's `runId` where the ledger holds its run, and
that run's `roomId`, `round` and `messageId`, null where it holds none.

A run's tool calls are its `tool` lines, each naming the tool and carrying a `ref`, and the `tool-result`
line with the same `ref` saying how the call ended in its `state`, `succeeded` or `failed`. A run on a
road whose program carries its own turns has them too, read off that program's record: there a call's
line carries no arguments and its result's text says how long the call ran (`docs/run-records.md`,
「Agent log」).

A line's `metadata` is the JSON object the writer kept beside it, null where it kept none. A line read off
an assistant message of a Claude Code program's record carries `request` there, the message's `id` and the
`usage` the provider stated for that request — `input_tokens`, `output_tokens`, `cache_read_input_tokens`
and `cache_creation_input_tokens`, each where the record states it — so a reader of the run as it goes
counts each request once by its `id` and needs no end of the turn for its figures. A job's lines
keep the metadata their writer filed, and a pane program's shell call carries Claude Code's
`toolUseResult.bashEditDiff` there, with the files, their hunks, the count of files whose hunks were left
out and the paths the call changed. A `note` the call clock wrote about a job's tool call carries
`toolClock` there, the call's id, tool, one-line command, process id and elapsed time and what befell it;
a clock note filed before its fields were kept is answered with them read back out of its words
(`docs/agent-control.md`, 「A run's tool calls are held to a clock」).

- **One agent's log across its runs** is `GET /log/agents/{agentName}/lines`, in the same line shape. A role's
  name reads the lines under it and the lines of every agent of that role, a job's lines standing under the agent
  its item names (`docs/run-records.md`, 「Agent log」):
  - no cursor reads the newest page, oldest line first, and `hasMore` says older lines wait;
  - `before` reads the page before that line id, oldest line first, and `hasMore` says older lines wait;
  - `after` reads the page after that line id, oldest line first, and `hasMore` says newer lines wait;
  - `at` opens the page where the line at or shortly after that moment stands, beginning at the head of
    its run (a run's name is not unique for ever); `chat` narrows the moment to one room. Such a page
    answers `hasOlder` for the lines before its head, and `hasMore` for newer lines; a moment already
    left out of the log reads the newest page.
  - The page answers `items`, `hasMore` and `hasOlder`; `limit` is 500 where left out and at most 500.
  - Every read of it that goes by line id walks each agent's own lines apart, the named agent's and each
    agent's of the role, in the order the page goes, and merges what they gave by id. A page therefore costs
    the same however many lines other agents wrote since the agent's last one: an agent that last wrote a
    million lines ago opens as quickly as one writing now, and so does the question whether any line stands
    before a page. Finding the line at a moment is narrowed by the moment itself.

### executions

| Method | Path | Does |
|---|---|---|
| GET | `/executions` | Executions, latest first. |
| POST | `/executions` | Launch a run's program in the agent host. |
| GET | `/executions/{executionId}` | One execution. |
| PATCH | `/executions/{executionId}` | Cancel an execution or take the cancel back. |

### overtime-requests

| Method | Path | Does |
|---|---|---|
| GET | `/overtime-requests` | Runs asking for more time. |
| PATCH | `/overtime-requests/{requestId}` | Answer an overtime request. |

### stop-marks

| Method | Path | Does |
|---|---|---|
| GET | `/stop-marks/{jobId}` | Why a job is to stop, if it is. |
| PUT | `/stop-marks/{jobId}` | Mark a job to stop: its turn's end writes it `stopped` with the mark's reason, and it goes out on no further road. |
| DELETE | `/stop-marks/{jobId}` | Clear a job's stop mark. |

### slots, reservations and places

| Method | Path | Does |
|---|---|---|
| GET | `/slots` | Every role's slots: the runs let in, reserved and waiting. |
| GET | `/slot-requests` | Open slot requests, by role, by owner or waiting. |
| POST | `/slot-requests` | Ask for a slot of a role; its provider and model are those of the agent it is let in on. |
| DELETE | `/slot-requests` | An owner's release: every request, reservation and bound role session the owner holds given back. |
| GET | `/slot-requests/{token}` | One slot request. |
| PATCH | `/slot-requests/{token}` | Move a slot request: admitted, bound to its launched run, finished. |
| GET | `/reservations` | Held slot reservations. |
| POST | `/reservations` | Reserve slots of a role for an owner. |
| PATCH | `/reservations/{token}` | Release a reservation with its outcome. |
| GET | `/places` | Branch places, by node and checkout. |
| POST | `/places` | Take a branch place and its folder. |
| GET | `/places/{placeId}` | One place. |
| PATCH | `/places/{placeId}` | Prepare, remember or retire a place, or record its last write. |

A slot is asked under a role the store holds (「roles」), with the source, the owner and the request key its
caller names and the path its run will work at, and is refused 422 `unknown-role` under any other and 400
where it names none, whatever its source (`docs/role-slots.md`, 「Who takes a slot」). An ask may name `provider` and `model` together, the road the run
goes out on: it is let in on an agent of the role that stands on that road alone, and is refused 422 with
the roads the role's agents hold where none does (`docs/role-agents.md`, 「Which agent a run takes」); or
`model` alone, whose
provider is that of the role's first agent naming that model; a `provider` named alone is refused 400. It
may name `placeKey`, the key only one active request holds at once. A slot let in on an agent that names no
road goes out on the road of the supervisor its `askedBy` names; asked under no supervisor the store holds,
it is refused 422
`unprocessable-entity` saying the role follows the supervisor that asks it. A request answers `seat`, the
agent it is let in on, null until it is.

A request key names one request for good (`docs/role-slots.md`, 「Request keys」). Asked again with the same key
while that request stands — `waiting`, `reserved` or `active` — the ask answers 201 with that request and
changes nothing of it; asked again once the request was given back, `released` or `cancelled`, it is refused
422 `unprocessable-entity` and opens nothing, and a new run asks under a new key.

A slot request answers its `state` — `waiting` in its role's line, `reserved` under a reservation, `active`
once let in, and `released` or `cancelled` once given back — and the `role`, `path`, `provider`, `model` and
`effort` it is let in on, `effort` null where the order names none; `heldFor`, why it waits where its
provider has no quota, or null; `endedBy`, who gave it back — `holder`, `owner` or `run-ended` — or null while
it stands; and `session`, the role session it is bound to or continues, or null. A request is asked for
admission by a `PATCH` naming no `bound` or `finished` state, which answers `request`, `state`, `endedBy` and
`outcome`: `request` is the request, `active`, where it is let in now or was already, and null otherwise;
`state` is the request's state now, so `waiting` and `reserved` say its turn has not come or a reservation lets
it in rather than its token, and `released` or `cancelled` say it was given back, by `endedBy` and with
`outcome`, the words it was given back with; a caller asks again only while it is `waiting`. A request's
admissions are its owner asking: the
line keeps the order requests were asked in, and a slot goes to the first waiting request whose owner is
asking now and whose provider has quota (`docs/role-slots.md`, 「The line」). A `PATCH` with `{ "state":
"bound", "childId": … }` binds an active request to the run it launched and answers the request: a unit's, a
pane's or an execution's name, or the id of a role session agency holds, to which the lease is then bound
(`docs/role-slots.md`, 「The line」). A role's start naming an active slot binds it to the session it opens,
whether or not it was bound before; a later binding naming another child leaves it bound to that session and
keeps the child as the run's name until the next start on the slot. A request is
given back with `{ "state": "finished", "outcome": … }`, and cancelled, where it never ran, with `{ "state":
"finished", "cancel": true, "outcome": … }`; either answers `{ "held": <bool>, "stoodDown": <bool>, "note":
<text or null> }`. A request given back while bound to a role session ends that session, a turn running in it
cut first and the answer given once the session has ended (`docs/role-slots.md`, 「The line」).

A slot on a registration naming a card service asks its demand once the line lets it in (`docs/own-llm.md`,
「An engine the arbiter stands」): the admission answers `demand` beside `request` — its `id`, `state`,
`nodeName` once a card is granted and `reason` while it waits, null on any other slot — and answers `request`
null until the card is granted, the caller asking again, with no clock of agency's. An admission may name
`cardRank` and `cardPool`, as a session's start names them, the demand a slot asks of its own; `demandId`, a granted demand of the caller's own the slot stands on, refused 400 `demand-gone` where the
master no longer holds it; or `within`, the asking run's granted demand the slot's own is asked within. A
demand the slot asked is given back as the slot is finished, or on the line's next minute whatever ended its
lease.

A slot request may carry `title`, one line naming the work it is asked for, and `askedBy`, the supervisor
that asked for it, each kept as given; a `PATCH` binding it may name the run its child is. Each is null where
none was named. A request answers `account`, the account it belongs to: its asking supervisor's owner account,
or null for the deployment's own (`docs/role-slots.md`, 「Accounts」).

`GET /slots` answers `items`, for every role its `role`, `maxConcurrent`, `active`, `reserved` and `waiting`, and
`runs`, each active run: its `token`, `role`, `path`, `provider`, `model`, `source` and `since` in epoch
milliseconds; `agent`, the agent of the role it was let in on (`docs/role-agents.md`), or null where none is
named; `session`, the role session it is bound to, or null; `runName`, the run a binding named where one
did since the slot's last start, else for a slot bound to a role session the run name of that session's
running turn, null while no turn runs, and for any other slot the child it is bound to, null until bound;
`jobId`, the job whose run it is where its source is `job`, else null; and `title`. A screen opens a tile's
log by its job's where `jobId` is named, else by its `runName` where it stands, else by its `session`.

`DELETE /slot-requests` takes `owner` in its query and gives back everything that owner holds
(`docs/role-slots.md`, 「An owner's release」): each of its requests that waits or stands reserved, cancelled; each
that is let in, released; each reservation it holds, released; and each role session one of its leases is bound
to, ended: a turn running in such a session is cut first, ending `stopped` with the reason `released`, and the
session is ended once that turn has ended. Every one is given back as `owner`. It answers as soon as the
requests and reservations are given back, 200 with `cancelled`, `released` and `reservations`, the counts given
back, and `sessionsEnded`, the ids of the bound sessions it ends; their ending is carried on by agency after
the answer, whether or not the caller waits, since a turn's end is an event of its program and may be long in coming,
and the caller is never held on it. 400 where no owner is named. An owner holding nothing answers zero counts.

A reservation is asked under a role, an owner, a key and a count, and answers its token and the requests that
stand for its slots; reserving again with the same key changes only the count (`docs/role-slots.md`,
「Reserving slots」).

A place's id is its path, percent-encoded in the path segment. A list without a node names the standing branch
checkouts under the data root — one for each place of each pool, its number, the role that last ran in it,
whether a run holds it now (`taken`) or a branch last answered in it (`remembered`), and the write it last
took. `checkout` narrows the list to that checkout's pool; `nodeName` with `checkout` names the shadow the
node's link keeps of that checkout instead.

### providers, models and model files

| Method | Path | Does |
|---|---|---|
| GET | `/providers` | Every provider agency carries, with its switch and what it has left. |
| GET | `/providers/{providerName}` | One provider. |
| PATCH | `/providers/{providerName}` | Throw the owner's switch on a provider. |
| GET | `/models` | The registered models the caller reaches. |
| POST | `/models` | Register a model. |
| GET | `/models/{modelKey}` | One registered model the caller reaches, and its health. |
| PATCH | `/models/{modelKey}` | Change a model's registration. |
| DELETE | `/models/{modelKey}` | Take a model's registration away. |
| PUT | `/models/{modelKey}/addresses/{node}` | Set one address of a model's registration, its key named by a file. |
| GET | `/provider-models/{providerName}` | The models a provider holds, for an order's chooser. |
| GET | `/model-catalogue` | The model files the deployment's engines serve, and each engine's latest choice. |
| POST | `/model-catalogue` | Save or remove a model file's entry, or choose one for its engine. |
| GET | `/fallback` | The Claude accounts, the account in use and the chosen one. |
| PATCH | `/fallback` | Add, change, order, switch or choose the Claude accounts. |
| POST | `/claude-logins` | Start a Claude account's sign-in through the CLI: an account there by `account`, or a new one by `name` or named by agency where the start names neither; answers the sign-in's `login`, the `url` to approve it at and the account's `name`. |
| POST | `/claude-logins/{login}/code` | Hand the sign-in the code its page showed; answers the accounts as the fallback's change does once the login is written. |
| DELETE | `/claude-logins/{login}` | Give a sign-in up, ending the CLI it holds. |
| GET | `/gateway-members` | The gateway's members, each with its road, label and owner. |
| PATCH | `/gateway-members/{memberName}` | Set or take away a member's owner. |

Each provider answers `name`, `code` (its name, the kebab-case word a screen words it by), `label`,
`enabled`, `registered`, `gateway`, its `lines`, and `quota`: what it has left as the order reads it
(`docs/provider-order.md`, 「Quota: what a provider has left」) — `left`, a fraction from 0 to 1 or null where
it reports none, `window`, the name of the window that counted, `balance` for a provider that keeps one,
with `currency` and `balanceSeenAt`, the currency it is in and when it was seen in epoch seconds,
`requests`, the request allowance read with it — `used`, `limit`, `left` and `scope`, what it covers or
null — or null where its provider
states none (`docs/usage-windows.md`, 「Balances」), and `resetsAt` in epoch seconds where the counting
window names one. The quota is the provider's as a whole and names no model, so a spent balance reads as
no quota left although a run on a model the provider says costs nothing still goes out on it
(`docs/provider-order.md`, 「Quota: what a provider has left」). `gateway` says its models are served by
agency's model gateway — the registry's registrations and the member roads of Codex, OpenRouter and
DeepSeek — and `registered` says the models are the registry's registrations alone. `ownersEngine` says its
engine is the owner's own, so what a session on it reads stays with the owner: the provider's own declaration
(`docs/provider-presence.md`), true for the registry's registrations and Claude Code on the owner's logins and
false for every road that hands what it reads to an outside service (`docs/supervisors.md`, 「What confines a
session allowed files」). A consumer judging where a session's reading goes reads it, never `registered` or
`gateway` in its place, since a provider may be neither and still not the owner's own.
`takesAdvisor` says a road of it may name an advisor, the provider's own answer (`docs/provider-order.md`, 「The
advisor」): true for Claude Code on its own logins, false for every provider carried through the gateway; and
`advisorWords` the family words such a road may name its advisor by beside a model of the provider's own, empty
where it takes none. A provider that cannot answer now says why twice over (`docs/provider-presence.md`): `reason`, the English sentence, and
`reasonCode`, its kebab-case code, both null on a provider that can answer. Each provider answers
`foldRatioDefault` as well: the compaction threshold a supervisor or an agent whose road is on it is given
where its add names none — the provider's own declaration, 0.85 where it declares none (`docs/supervisors.md`,
「What a provider declares for a conversation」) — so a screen marks the threshold equal to it as the default.
A provider that reads its own key from a file answers `keyFile`, that file's name under the data root's `env/`
(`docs/provider-presence.md`): OpenRouter's and DeepSeek's, null on every other, so a console that writes the
deployment's secrets there sets the key where the provider reads it. Its content is never reported.

The gateway's members are the members file's (`docs/own-llm.md`, 「The gateway's members」): each `name`, `road`
or null, `label`, `provider`, `account`, the account it belongs to or null, and `unit`, the organisation unit it
is filed under or null — never both owners, and neither for the platform's; no key file and no
key is answered. The list answers what the caller reaches: the administrator's and the deployment's own read
every member, and a member reads its own members and those filed under the units they head, the members of the
platform and of other accounts and units standing in no such list. A member's change takes `account` or `unit`,
a whole number or null, one owner at most, and writes it into the file, every other
field and member left as it stands; it answers the member as it now stands, 404 `unknown-member` for a name the file
holds none of, 400 with `field` for an owner that is not a whole number or for a change naming both, 503
`members-unread` where the file
cannot be read and 500 `members-unwritten` where it cannot be written. The change is the administrator's and the
deployment's own: a member's `PATCH` is refused 403 `forbidden`. A registration's owner is its `account`, a user
id or null, or, filed under an organisation unit, its `unit` — never both, answered with the registration and
changed with `PATCH /models/{modelKey}` (`docs/supply.md`, 「Owned supply」). The account is the owner's alone to
set, a member's `PATCH` naming it refused; the unit is the platform's call's or a head of that unit's, and a
head may change and remove the registrations of their unit alone (「Who a caller acts for」).

A provider's change throws the owner's switch on it; the last provider left on is not thrown off, since
nothing would answer. A provider that is off answers on no road (`docs/provider-order.md`, 「The owner's
switch on a provider」).

`GET /provider-models/{providerName}` answers `provider`, `models` and `staleReadAt`, the moment of a stale
read in epoch seconds or null, with the models an order's entry may name on that provider
(`docs/provider-order.md`, 「Models a provider holds」). Each model is `id`; `efforts`, the levels it takes, null
in the list standing for the provider's own default; `free`, whether it costs nothing where the catalogue
prices it, else null; `inService`, whether it is served rather than superseded where the catalogue tells them
apart, else null; and `contextTokens`, `answerTokens` and `thinking`, its window, its answer ceiling and its
thinking default, named for a gateway road's member and the registry's registration and null for a model of
the Claude CLI's catalogue, which states none of them. Claude Code's are the CLI's catalogue; a gateway
road's are its members; the registry's are its offered registrations. A catalogue that cannot be read answers
the provider's refusal, never an empty list.

A registered model is one registration of the registry's provider: the list, the create, the read, the change
and the take-away all read or write the registrations, so the list holds exactly what can be changed and
removed. A model's key names its provider and the model apart — `provider::model`, or the provider as a query
and the model in the path. Each answer carries `name` beside its `key`, the registration's own name, which is
what a road and a repository's pin name on the registry's provider; and the registration's own run
settings, `contextTokens`, its window, `answerTokens`, its answer ceiling, and `thinking`, its thinking
default — what a run on it does where its road leaves thinking to the provider (`docs/own-llm.md`,
「Registered models」), so a screen marks the entry's thinking equal to it as the default. A create starts a registration
under its own name as its label, at the registry's smallest window and answer and with no address, filed under
a unit where the body names `unit` and its caller heads one, and is
refused where the name already stands. `healthy` and `lastAnsweredAt` are read back: `healthy` is whether the
engine answers now, and `lastAnsweredAt` the moment it last answered a probe, in seconds. Each answer carries
`usedBy`, every supervisor whose road names it, every role an agent of which stands on
it and every repository whose pin names it, and a take-away is refused 409
while any does. Each answer carries `cardService`, the arbiter service the registration's engine is or null,
and `roleFiles`, whether that service's instances keep role files; the create and the change take both, and a
change names at least one of `model`, `cardService`, `roleFiles`, `languages`, `account` and `unit`. Each answer carries
`languages`, the languages a run on the registration writes a result for a person in, each by its English
name, empty where it names none, which means every language; the create and the change take it as a list,
an empty list or null naming none (`docs/own-llm.md`, 「What a registration holds」). Naming a card service takes the
registration's addresses away, and its `healthy` is the arbiter master's (`docs/own-llm.md`, 「An engine the
arbiter stands」). A registration's addresses are the registry's, set by the address route and with
`bmo-agency agent --register-address`.

`PUT /models/{modelKey}/addresses/{node}` adds the address of the node the path names, or changes the one standing
there in place, as the command's `--register-address` does, its key named rather than given: it takes `url`, held to
the registry's rule (an http or https address ending in `/chat/completions`), and `keyFile`, one plain file name
under the data root's `env/` — the platform's space of the console's secret store — or null or absent for no key.
Agency keeps the name alone: it neither writes, reads back nor removes that file, and a key file it wrote for the
address before is taken away (`docs/own-llm.md`, 「The keys」). The node is a node label by the registry's rule. It
answers 200 with the model as the read answers it, and stands every program on a pin naming the model again as a
change does. It is the administrator's and the deployment's own: a member's call is refused 403 `forbidden`. A key
naming no registration of the registry's provider is 404, and a registration naming a card service, a node label,
an address or a key file the registry refuses, 400 with the registry's sentence and the `field` it names.

The model catalogue is a resource of its own, never of `/models`: the files the deployment's own engines
serve, each with its sizes, its source, its nodes and the registration it stands behind, and the latest
choice for each engine (`docs/model-catalogue.md`). Its entries are not models, and `/models` neither lists
nor changes them.

The fallback's view answers `accounts`, every Claude account with its kind, path, place, switch, login
standing, `onToken` — whether its runs here go out on its token for the work nodes now
(`docs/cache-fallback.md`, 「A directory account carries on by its token」) —, whether it is chosen and in
use, its rest and its own `session`, `week` and `balance`, the same the
accounts list answers (「accounts, rest and usage」); and `active`, the account runs on Claude Code go out on
now. Its change takes the account actions `add-account`, `update-account`, `remove-account`, `set-account-order`,
`set-account-enabled`, `choose-account` and `clear-account-choice` (`docs/cache-fallback.md`, 「The Claude
Code road is a list of accounts」); no road and no route action is any part of it.

A sign-in (`docs/cache-fallback.md`, 「Signing an account in from the console」) starts with `account`, a
directory account's id, or `name`, a new account's name, and answers 201 with `login`, the sign-in's identifier,
`url`, the address the owner approves it at, and `name`, the name of the account it signs in. It is refused 400
where it names neither or both, 404 where the account is none, 422 `not-a-directory` for an account that keeps no
login of its own, 409 `directory-taken` where a new account's directory already holds anything, and 422 `no-link`
with what the CLI said where the CLI gave no address within the bound. The code takes `code`, a non-empty string, and answers 200 as the fallback's change does,
`{ok, accounts}`; a sign-in that is none, or has ended, is 404 `no-sign-in`, and a code the CLI did not take is
422 `sign-in-refused`, saying what the CLI said with the code taken out, and ends the sign-in. Giving it up
answers 204, and 404 `no-sign-in` for one that is none.

### road-blocks and launch-room

| Method | Path | Does |
|---|---|---|
| GET | `/road-blocks` | Roads blocked now, by model or role; none means open. |
| POST | `/road-blocks` | Block a road until a time. |
| DELETE | `/road-blocks/{blockId}` | Open a road again. |
| GET | `/launch-room` | Whether a launch may start, why not, and the jobs and browsers running. |
| PATCH | `/launch-room` | Record that launches are held, and why. |

A block is a resource with an id: its selection — `provider::model` — the moment it opens again in epoch
milliseconds and the reason it was closed. A list returns the blocks whose moment is still ahead, narrowed to
the roads a named model or a role's agents would take, and a deletion by that id opens the road again
(`docs/provider-order.md`, 「A road that fails」). Agency writes a block itself as a job's turn leaves its
road for its provider's failure, for the span the failure's kind holds, its reason
naming the kind and the failure's words; a block written by the route and one written by a walk are one
resource, listed and opened alike.

The launch room answers `held`, every reason it is held, the jobs at work and, of those, the ones whose place
runs a browser. The launcher's own notice is kept beside the machine's reasons and read back with them; a
notice that holds nothing is withdrawn.

### gate kinds

| Method | Path | Does |
|---|---|---|
| GET | `/gate-kinds` | The gate kinds the gate declared, each with the part of a turn it stands in and its step there. |
| PUT | `/gate-kinds` | The gate declares its kinds and their order, over the declaration standing. |

- **The gate declares its kinds; agency keeps no copy of the gate order.** A gate kind is a judgement the
  gate service runs on its own, and its order — which of them stand before a reply, which after it and which
  outside any turn — is the gate's own policy; agency runs no gate and holds no row for one. The agency reads `GET /gates` from the gate
  as it starts and once a minute after, and keeps what the gate answers; `PUT /gate-kinds` is the route the
  gate declares through when it has more to say than that page carries. The declaration is written over
  the one standing whole: a kind the gate no longer names is no kind of a turn.
- **What a turn reads.** A supervisor's gate switches (`GET /supervisors/{supervisorName}`) are read from the
  declared kinds. Every supervisor's turn passes the declared kinds; agency keeps no subset of its own.
- **A declaration names its gates,** each as the gate publishes it: `gateName`, `part` (`pre`, `post` or
  `apart`), `step`, `condition` or null, `switchable`, `layaProbe`, `defaultLayaMode` and `answerTokens`.
  An empty declaration, or one whose kind stands in no turn part, is refused 400 `bad-request`.
- **What a read keeps.** An empty order the gate answers through `GET /gates` is kept as one, so no kind
  stands and no supervisor shows a gate switch. A deployment that names no gate address, and a gate the
  address does not answer, leave the declaration already kept as it stands.

### pull request reviews

| Method | Path | Does |
|---|---|---|
| POST | `/github/deliveries` | One GitHub delivery of a `pull_request` event, its body as GitHub sent it, handed on by the console's gateway with its `X-GitHub-Event` and `X-Hub-Signature-256` headers; answers 202 `{taken}` at once and acts after (`docs/pull-request-reviews.md`). |

- **Signed or refused:** the signature is checked against the deployment's webhook secret
  (`BMO_GITHUB_WEBHOOK_SECRET` or `BMO_GITHUB_WEBHOOK_SECRET_FILE`); one that does not match is 401
  `bad-signature`, and a deployment naming no secret answers 503 `no-webhook-secret`. An event or an action it
  does not act on is answered 202 as well.
- **What it does after:** an open pull request on a repository agency holds is filed once as a review item in the job service,
  in the unattended lane only where its author is the repository's `OWNER` or a `MEMBER` and its head stands in
  the same repository, and otherwise as a draft that waits for the owner's approval and is never merged by the
  review; a push is told on that item; a merge opens the held items labelled with the same pull request, and a close is
  told on them.

### translations

| Method | Path | Does |
|---|---|---|
| POST | `/translation-runs` | Translate one text on the `translate` road: `purpose` (`document`, `worker` or `record`), `language` (the target's name as the instructions ask for it), `text`, an optional `note` and optional `terms`; answers 200 with `text`, the translation whole, and the `provider` and `model` it went out on. |

- **One text, one run** (`docs/launch-lines.md`, 「Translate runner」): no session is kept, nothing is
  queued and nothing is cached here; the caller cuts a long text into parts, keeps the translations it
  wants kept, and bounds how many parts it has out at once.
- **A caller that stops waiting** ends its translation there, as one that failed: the card the
  translations out on a registration share stays with the others still out and is given back once none
  is, whether its demand was granted yet or not (`docs/own-llm.md`, 「An engine the arbiter stands」).
- **A note** is the caller's word on this one part — what an earlier answer for it got wrong, or that a
  one-line title is wanted — carried in the run's system prompt after the translator's own, as an
  instruction the model reads and does not render; the input is the language line and the text, and the
  terms where the caller names any, since a translation model renders every line of its input.
- **Terms** are the words of this one part the translation renders as the caller says: each a `source`, as
  the text writes it, and a `target`, how the translation writes it, the source itself where none is
  named, so a name, a figure with its unit, code, an address or a fixed heading the caller lists stays as
  written. They stand at the head of the input as the translation model's own reference translations, one
  line each, and the instruction after them asks for the translation alone (`docs/launch-lines.md`,
  「Translate runner」). An empty list, or none, asks with the language line alone.
- **Refusals:** 400 `bad-request` for a purpose it does not know or an empty text or language; 413 `too-large` where the
  model refused the part as longer than its window takes, so the caller cuts it smaller; 502 `no-answer`
  where the road answered with nothing or failed otherwise, naming why. A translation none of whose role's agents has
  quota left waits, as every run does (`docs/provider-order.md`, 「Whether a road can answer」).

### accounts, rest and usage

| Method | Path | Does |
|---|---|---|
| GET | `/accounts` | Accounts: held, kind, home, usage window, rest. |
| PATCH | `/accounts/{accountId}` | Record an account's usage and its rest. |
| GET | `/rest` | Whether the deployment rests, until when, of what kind. |
| PATCH | `/rest` | Set the rest, or clear a rest that has passed. |
| DELETE | `/rest` | End the rest by hand. |
| GET | `/usage` | Usage by model, checkout, repository, holder, room, round, account or supply source over a span, with session contexts. |
| GET | `/personal-supply` | A person's own gateway keys, or each person's roads for the administrator. |
| POST | `/personal-supply` | Add a key of the signed-in person's for a gateway road. |
| PATCH | `/personal-supply/{supplyId}` | Change one of the person's keys: its place or its switch. |
| DELETE | `/personal-supply/{supplyId}` | Remove one of the person's keys. |
| GET | `/personal-models` | The signed-in person's own local AIs, and whether their chain lets them add one. |
| PUT | `/personal-models/{name}` | Add or change a local AI of the signed-in person's own. |
| PATCH | `/personal-models/{name}` | Turn one of the person's local AIs on or off. |
| DELETE | `/personal-models/{name}` | Remove one of the person's local AIs. |
| GET | `/organisation-supply/{unit}` | An organisation unit's own keys and Claude accounts, and whether its supply stands open. |
| POST | `/organisation-supply/{unit}` | Add a key of the unit's for a gateway road, or an Anthropic API key. |
| PATCH | `/organisation-supply/{unit}/{supplyId}` | Turn one of the unit's keys or accounts on or off. |
| DELETE | `/organisation-supply/{unit}/{supplyId}` | Remove one of the unit's keys or accounts. |
| GET | `/supply-sources` | The supply sources the order the signed-in person's runs walk holds. |
| POST | `/supply-test` | Test whether a supply source of a road answers now: the person's own, or the platform's. |
| POST | `/people/{user}/off-boarding` | Switch a person's supply off and revoke their call key. |

An account is reached one of four ways: a directory holding a login, a token file, an endpoint — an
Anthropic-compatible base URL with a key file and an optional map from the `opus`, `sonnet` and `haiku`
family words to the endpoint's model ids — or an Anthropic API key, `api-key`, its secret under the data
root's `env/`. Each carries `userId`, the person it belongs to or null for the platform's, `unitId`, the
organisation unit it belongs to where a unit owns it, `node`, the node a
person's login stands on or null, and `lendable`, whether the account goes out for another person's run on a
platform source (`docs/supply.md`, 「The lendable accounts」); `update-account` takes `lendable` for any account,
a person's, a unit's or the platform's, from the administrator alone, a person's call naming it refused 403
`forbidden`; and takes `unitId` from the owner alone, a unit's number giving an account of no person's to that
organisation (`docs/supply.md`, 「Organisation supply」), null giving it back to the platform, either leaving its
`lendable` as it is, a person's account refused 422 `persons-own`. It may also name a base prompt that a worker-line run on it opens
with. The list reports each account's kind, its home and, for an endpoint account, its base URL, key-file
path and model map; a key's content is never reported. Each directory and token account carries its
two headline windows, `session` and `week`, as the usage-window store holds them under the account
for the login answering for it — none read at or before the account's unread mark or its mark of waiting
for a person, and none read before its home was pointed elsewhere (`docs/usage-windows.md`, 「What reads
the windows」): each a window's `name`, `utilization` (0 to 1), `resetsAt` and `seenAt` in
epoch seconds, `durationMinutes` and `limitName`, or null where the account holds no such window; an
endpoint account holds none. An endpoint account whose endpoint is a provider's that offers a balance
carries `balance`, what it has left as its balance reader last read it (`docs/usage-windows.md`,
「Balances」): `amount`, `currency`, `seenAt` in epoch seconds and `requests`, the request allowance read
with it or null, or null where none has been read; a directory or token account's is null. `window` is the usage a report named, answered by the report's
change and null on the list. A report's usage rests its login only where a window of it reads full,
`utilization` or `weeklyUtilization` at 1: until the five-hour window's `resetsAt`, or the week's
`weeklyResetsAt`, or the rest policy's fallback from now where the week names none; the answer's
`restUntil` is that moment, or null where no window reads full. Accounts are added and changed through
`PATCH /fallback`'s `add-account` and `update-account` actions, and with `bmo-agency agent --account-add`
and `--account-set`.

The deployment rests while the service's own outage rest stands, or while every enabled Claude account the
platform's own work runs on (`docs/cache-fallback.md`, 「The account judgement」, the logins a run of no person
walks) is closed for its limit — a rest written under it, a limit's or a refusal's, or a window of its own
read full — until the soonest of them reopens. A person's or an organisation's own login neither holds the
rest nor lifts it. `GET /rest` answers `resting`, `until` in epoch seconds and `kind`, `outage` where the
service's own rest stands and `limit` otherwise, both null where it does not rest. `PATCH /rest` with `until`
in epoch seconds, `kind` (`limit` or `outage`) and `account` writes a rest as a run's failure would, and
answers what then stands: `resting`, whether the deployment rests now; `until`, the moment the rest it wrote
or found stands until, `kind` and `accountId`, the login it stands under, null for the outage; `written`,
false where a rest already standing there was left as it stood; and `silenced`, whether it was the rest that
silenced the deployment. `PATCH /rest` with `clearPassed` clears every rest whose moment has passed and
answers `kind`, the kind of the silence that has just ended, once: the silence told as it began where one was,
else one the passed rests held; null while the deployment still rests or where no silence ended.

A moment is kept in epoch seconds, wherever it comes from: one written in milliseconds — a figure past the
seconds of any date the deployment can meet — is read and kept as the second it names, so a reset reported in
milliseconds never rests a login for ever. That holds for a rest written through either route, for a usage
report's reset moments and for a rest already kept.

`DELETE /rest` is the administrator's end of the rest, for a quota the owner knows has come back before the
moment kept for it: it clears the service's outage, every rest written under the platform's own logins, a
limit's or a refusal's, and every model's rest on them, and sets aside the full windows kept for those logins
until their reader reads them again (`docs/cache-fallback.md`, 「The account judgement」). It answers
`ended`, whether the deployment rested before it, with `resting`, `until` and `kind` as the deployment stands
after it: a login whose next reading finds its window still full closes again, and the silence it makes is
told again as any silence is. The end is answered once to whoever clears the passed rests, as a rest that
passed on its own is.

The silence is published once as it begins, as `rest.changed` (「Events」) with `account`, the login whose
limit closed it or null, `kind`, `until` and `notice`, the rest policy's words: by whatever writes the limit's
rest or the outage that closes the last open login, and otherwise on the rest policy's clock
(`lift_check_seconds`), where the outage or a window read full closed it with nothing written. A silence held
by refusals alone is never published as a limit; the login notice tells it.

Usage groups the jobs whose turns started in the span by the key `groupBy` names, `model` where it names
none: the model the job's turn went out on (`model`), the checkout it ran in (`agent`), the repository that
checkout belongs to (`repository`), the job's holder (`user`), its room (`room`), or, over the runs of the
ledger instead, the person a run was for, `none` for the deployment's own (`account`), or the supply source it
went out on (`supply`). Another word is refused 400. Every group carries `costUsd`, what its jobs or runs cost,
or null where none reported a cost. A checkout belongs to the repository whose name it is, or whose name it carries before `-w<n>` for a
support checkout or before `@<node>` for a node checkout (`docs/worker-slots.md`), so a repository's group
sums its base, support and node checkouts; a checkout no repository holds now is a group of its own under
its own name. Each group carries `tokens`, what the jobs' models spent; `jobs`, how many jobs are behind the
figure; `firstAt` and `lastAt`, the span they cover; `inputTokens`, `outputTokens`, `cacheReadTokens` and
`cacheWriteTokens`, the spend apart summed over the jobs whose turns reported all four kinds, each null
where no job of the group did; `unsplitJobs` and `unsplitTokens`, the jobs that spent tokens without
reporting all four kinds and the `tokens` those jobs spent, a job that spent nothing counting in neither; and
`contextTokens`, the tokens the roles' sessions answered in the span carry. A `model` group's are those of
the sessions on its model; an `agent` group's those of the sessions opened in the directory its checkout
stands at now (`docs/worker-slots.md`); a `repository` group's the sum over the distinct directories of
every checkout of it standing now. A session records its directory and not its machine, so a node checkout
standing at the same directory as another checkout carries the same sessions, and its repository counts
that directory once. It is null where no such session answered in the span, for a checkout that
stands nowhere now, and for every `user` and `room` group, since a session records neither the holder nor
the room of the jobs it served. `round` groups the
chat turns' runs instead, those started in the span that name a round and report the four kinds, by that
round: each group's `tokens` is what its turns spent, `turns` how many there were, a field only a round's
group carries, the four kinds the spend apart, and `firstAt` and `lastAt` the span they cover; its `jobs`,
`unsplitJobs` and `unsplitTokens` are 0 and its `contextTokens` null. Every group has the `UsageGroup` shape
of `docs/openapi.json`. The span is two moments in epoch milliseconds, yesterday to now where none is given.

A signed-in member's usage stands over the work they reach: for the ledger's groupings, the runs filed under
their own account and under the units they head, and for the job's, the jobs whose run is so filed — a job
standing on no run by its holder's filing. The platform's own work, filed under neither owner, stands in no
member's figures, and a member's read answers no `contextTokens`, whose sessions are summed over every
reader's work (「Who a caller acts for」).

**Personal supply** (`docs/supply.md`, 「What a person brings」) is a person's own keys for the gateway's roads.
`GET /personal-supply` answers, for a person's call, `items`, each key's `id`, `road` (`codex`, `deepseek` or
`openrouter`), `secret`, `place`, `enabled`, `refusedAt` and `addedAt`; and for the administrator's or the
deployment's own, `people`, each person's `user` and the `roads` their keys serve, and no key's secret. `POST`
takes `road` and `secret`, one plain file name under the person's `users/<user>/` space, and answers 201 with
the key, last of that person's keys of the road; a key the person already holds for the road is refused 409
`conflict`, a road no gateway member serves 422 `unknown-road`,
a secret that is not one plain name 400 with `field`, and a call naming no person 403 `forbidden`. `PATCH`
takes `place` or `enabled`; turning a refused key on clears `refusedAt`. `DELETE` answers 204. A key of another
person is 404 `unknown-supply`.

**Organisation supply** (`docs/supply.md`, 「Organisation supply」) is a unit's own keys and Claude accounts,
for a head of the unit and the owner: a person is taken as a head where the delegation chain answers them the
unit's link of `supply.organisation`, anyone else refused 403 `forbidden`, and a check the account service did
not answer 503 `access-unchecked`. `GET /organisation-supply/{unit}` answers `unit`, `open`, whether
organisation supply stands open for the unit on the chain, and `items`, each `id` (`key-<n>` for a gateway key,
`account-<id>` for a Claude account), `road`, `secret`, `name` for an account, `enabled` and `refusedAt`. `POST`
takes `road` and `secret`, one plain file name in the unit's `units/<unit>/` space: a gateway road adds a key
last of the unit's keys of that road, `claude-code` an `api-key` Claude account of the unit's with a
configuration home of its own under the data root; it answers 201 with the item, and is refused 409
`organisation-supply-closed` while the unit's supply stands closed, 409 `conflict` for a secret the unit holds
already as a key of the road, and 422 `unknown-road` as a person's is. `PATCH` takes `enabled` and answers the
page; turning a refused key on clears `refusedAt`. `DELETE` answers 204. An item of no such unit's is 404
`unknown-supply`. `GET /supply-sources` answers a person's call `personal`, `platform`, `organisation`,
`organisationSupply`, `personalOnly`, `ownSupply`, `localAi` (`docs/supply.md`, 「The sources a run walks」) and
`providers`, every provider the build carries as the person's chain and the owner's switch leave it — each
`provider` with `off`, `owner` for the owner's switch or `chain` for the chain's
`provider.<code>.offered`, null while its sources stand open, and `from`, the link that said so, where the chain
closed it (「A closed provider is
read as closed, not as absent」 in `docs/supply.md`); a
call naming no person is refused 403.

**A person's local AI** (`docs/supply.md`, 「A person's local AI」) is a registration they own, served on their own
PC, through the person's own headers alone; a call naming no person is refused 403 `forbidden`. `GET
/personal-models` answers `allowed`, whether the chain's `supply.personal-local-ai` lets them add one, and `items`,
each local AI's `name` as they gave it, `model`, the registration's key on the registry's provider (`registered::<the
registration's name>`), `url`, `modelName`, the model the server serves, `keyName`, the name of their secret or
null, `contextTokens`, `enabled` and `addedAt`. `PUT /personal-models/{name}` takes `url`, an `http` chat
completions address, `modelName`, `keyName`, one plain name in their `users/<user>/` space or null, and
`contextTokens` where they name one, and answers 201 for one made and 200 for one changed; refused 403
`personal-local-ai-withheld` where the chain forbids it, 409 `no-pc` where the person holds no PC of their own, 400
with `field` for a name, an address, a model, a key's name or a window out of their rules, and 503
`access-unchecked` where the account service did not answer. `PATCH` takes `enabled`; `DELETE` answers 204 and is
refused as the registry refuses the removal of a registration something names. A name of no local AI of theirs
is 404 `unknown-model`.

**A supply test** (`POST /supply-test`, `docs/supply.md`, 「Testing a source」) takes `road`, one of `claude-code`,
`codex`, `deepseek`, `openrouter` and `registered`, `model`, the registration to test on `registered`, by its name
or its key, and for a person's call on `registered`, `local` in place of `model`, a local AI not saved yet, its
`url`, `modelName` and `keyName`; it
answers 200 with `road`, `ok`, whether the source answers now, and where it does not `reasonCode`, its kebab-case
code, and `reason`, the English sentence, both null where it does: `off` where the owner switched the road's
provider off, `no-source` where no credential of the road is on (a person's call on `registered` among them),
`login-needed` where Claude logins are on and none of them is signed in, `not-ready` where the gateway does not
serve the key's member yet and the test is worth asking again in a few seconds, `refused` where the provider
refused the key, and `failed` where the test reached no answer; a person's local AI answers besides `withheld`
where their chain forbids it, `no-pc` where they hold no PC of their own, and `unreachable` where their PC or the
address on it could not be reached. A person's call tests their own supply, the
administrator's and the deployment's own the platform's. A road that is none of these is refused 400 with the
`field` `road`; on `registered`, a platform's call that names no model, or one that is no registration, 400 with
the `field` `model`.

**Off-boarding** (`POST /people/{user}/off-boarding`) is the account service's, the deployment's own call: it
turns off each Claude account, key and local AI of the person, revokes their call key and answers `{accounts,
keys, localAis}`, how many it turned off. A person's own call is refused 403 `forbidden`.

**A person's sign-in** (`POST /claude-logins` from a person's call) starts on the person's node, named by its
record (「person-nodes」): a person with no node record is signed in on their isolation node, and one whose node
is not attached now is refused 409 `node-not-attached`, one with no ready workplace yet 409 `node-not-ready`, and
one whose workplace holds no Claude Code of its own yet 409 `no-cli`, the node's setup started to install it
(`docs/supply.md`, 「Signing a person's login in」). It takes `account` or `name`, not both, 400 `account-or-name`:
- `account` signs the person's own login in again in its directory on their node, and the code's answer lifts its
  mark of a login waiting for its person (`docs/supply.md`, 「Signing a person's login in」). An account that is not
  the person's own, or not there, is 404 `unknown-account`; an `api-key` account 422 `not-a-directory`; and a login
  that stands on another node than the person's now 409 `login-elsewhere`.
- `name` names a new login, and may be left out: agency then names the account `claude-NN`, one past the highest
  number any account's name holds, since account names are one namespace across every owner and a person sees only
  their own. A name another account already holds is refused 409 `name-taken`, and one that cannot be a name 400,
  each with `field` `name`.

A person's login that stands on a node is signed in again there alone: the owner's sign-in naming it by `account`
is refused 422 `not-a-directory`, as an account that keeps no login here is. The owner's sign-in takes `account` or
`name` as a person's does, 400 `account-or-name` for both, an account not there 404 `unknown-account`, and names a
new login the same way where it names neither, a name another account holds 409 `name-taken`.

### owners and organisation units removed for good

| Method | Path | Does |
|---|---|---|
| GET | `/owners/{account}/removal` | Count what removing a login account for good would take, changing nothing. |
| DELETE | `/owners/{account}` | Remove everything agency keeps of a login account, for good. |
| GET | `/org-units/{unit}/removal` | Count what removing an organisation unit for good would take, changing nothing. |
| DELETE | `/org-units/{unit}` | Remove everything agency keeps of an organisation unit, for good. |

- **The administrator's alone.** Each route takes the control token with `Owner-Role: administrator`, the console's
  call for the administrator; any other caller — a member, or the deployment's own caller carrying no key — is
  refused 403 `forbidden` before anything is read.
- **The numbers:** `{account}` is a login account's user id, the number `Owner-Key` carries and an owner column
  holds, never a Claude account's id; `{unit}` is an organisation unit's id, and `company`, a query parameter, the
  unit's company where the caller names it. Each is a positive decimal whole number, anything else refused 400
  `bad-request`. Agency keys nothing by a company and reads `company` no further.
- **Agency decides nothing of whether the account or the unit may go:** the console's account service decides that
  and calls these routes in its cascade. Agency removes what is keyed to the number it is handed.
- **A count changes nothing** and answers `{"counts": {<kind>: <n>}}`, exactly what the removal would take now; the
  removal answers `{"removed": {<kind>: <n>}}`. Every kind is present in both, 0 where there is none.
- **An account's kinds:**
  - `supervisors`: the supervisors filed under the account, each removed as `DELETE /supervisors/{supervisorName}`
    removes one with every room read as shared and its brain kept, since the brain's own removal takes it: its
    conversation reset and its turns cut off, its rows, its program, workspace, place and folders, its execution
    rows, its agent host records and lock, and its plugin of attached skills;
  - `slot-requests`: the role slot requests that belong to the account, each still open given back as an owner's
    release gives one back, the role session bound to it ended, and then every row of them;
  - `role-agents`: the role agents seated for the account, with their own plugins of attached skills;
  - `attachments` and `mcp-servers`: the account's rows of each;
  - `skills`: the skills standing in the account's own folder under the data root, taken with the folder;
  - `repositories` and `repository-checkouts`: the repositories filed under the account, as
    `DELETE /repositories/{repositoryName}` removes one, and their node and support checkouts' rows with them;
  - `registered-models`: the registrations the account owns, its local AIs among them, as the registry removes one;
  - `claude-accounts`: the Claude accounts whose `userId` is the account, with their usage windows, balances,
    rests and marks;
  - `personal-supply`: the account's own keys for the gateway's roads; its secrets stay where the console keeps
    them, for the account service to take;
  - `supply-reservations`: the account's runs' reservations, each still open captured through the supply outbox
    with what it last covered, as an expired one is, then every row of them; the asks owed in the outbox are
    still sent;
  - `call-keys` and `gateway-requests`: the account's gateway call key, revoked with its file and its row taken,
    and the gateway's records of the requests it carried;
  - `person-nodes`: the account's node record;
  - `workplaces`: the workplace of the account's isolation node, and of the PC its record names where no other
    account's record names that node, each with its units stopped and its folder under the data root's `nodes/`;
    that PC is also taken off the node service's list (its `DELETE /nodes/{nodeName}`, with no leave), before the
    record goes, so its workspace in herdr and the pane in it end on the next pass over the list
    (`docs/work-nodes.md`, 「In herdr」); a PC whose hub agent is still attached comes back on the node service's
    next pass as a node of no account;
  - `isolation-users`: the system user of the account's isolation node, taken away with its home and its
    firewall table by the isolation node's teardown (`docs/account-isolation.md`, 「Taking it down」).
- **An organisation unit's kinds:**
  - `supervisors`: the supervisors filed under the unit, each removed as the account's removal above takes one,
    its conversation reset first and its brain kept for the brain's own removal;
  - `role-agents`: the seats filed under the unit, with their own plugins of attached skills;
  - `registered-models`: the registrations filed under the unit, as the registry removes one;
  - `repositories` and `repository-checkouts`: the repositories the unit owns, as above;
  - `organisation-supply`: the unit's own keys for the gateway's roads;
  - `claude-accounts`: the Claude accounts given to the unit, as above;
  - `supply-reservations`: the reservations naming the unit's supply, each still open captured as above and every
    one of them taken off the unit, since each is a person's run's own.
- **Nothing of no account's and nothing of another account's is touched:** every row is found by the number handed,
  so a row of no account — the platform's — and a row of any other account or unit stand as they stood.
- **Refused 409 `conflict`** while a job holds a checkout of a repository the account or the unit owns, naming
  it, before anything is taken.
- **Again and again alike:** a removal asked twice takes nothing the second time and answers every kind at 0; one
  broken half-way is finished by asking again. Once begun it runs to its end apart from the request.

### settings

| Method | Path | Does |
|---|---|---|
| GET | `/settings` | Agency's own switches: the research supervisor, the downgrade return, the DeepSeek top-up's card and login, and the deploy queue's flush minutes. |
| PATCH | `/settings` | Change agency's switches. |

- **These four switches stand on the delegation chain** (`docs/prompt-layers.md`, 「The settings agency
  declares」): the read answers the platform link's value in the administrator's name, and the write puts it
  there, the administrator's account being the writer of the platform's link. Where no account service is
  named, each stands in the settings row it stood under before the value moved, and the row is read and
  written in its place; a value found in a row as the server stands is moved onto the platform's link once and
  the row given up. A write the chain refuses — a lock above the platform's link — is refused; one the account
  service cannot answer is failed, nothing guessed.
- **`researchSupervisor`** names the supervisor whose brain a run of the search or browse line writes into
  (`docs/agent-roles.md`, 「The voyager and the seeker roles」), and the one whose brain a run of the worker
  line reads and writes its lessons into (「The bundles」 there), or null for none. The kind
  `agency.research-supervisor` stands at the platform and at units: for a run, the nearest link of the person
  the run stands for names the supervisor; the read here answers the platform's word. `PATCH /settings` takes a
  supervisor the store holds, or null; another name is refused 400 with `field`. `downgradeReturn` is always
  null.
- **`deepseekCardSecret`** names the console secret, of the kind `card`, the DeepSeek top-up reads its card
  from at each call (`docs/tools.md`, 「The DeepSeek top-up」), or null for none, in which case the tool refuses
  every call. It is the owner's to change at any time, with no code change and no deployment; the kind
  `agency.deepseek.card-secret` stands at the platform alone, the name naming the platform's secret space.
  `PATCH /settings` takes a secret's name, one plain file name with no directory in it, or null; anything else
  is refused 400 with `field`. Agency keeps the name alone and never reads the secret.
- **`deepseekLoginSecret`** names the console secret, of the kind `account-login`, the DeepSeek top-up signs in to
  DeepSeek's platform with where DeepSeek asks a call for a sign-in, or null for none, in which case the tool
  refuses every call. It is the
  owner's to change as `deepseekCardSecret` is, its kind `agency.deepseek.login-secret` standing at the
  platform alone as the card's does, and `PATCH /settings` takes and refuses it the same way, with its own
  `field`. Agency keeps the name alone and never reads the secret.
- **`deployFlushMinutes`** is the minutes between two flushes of the deploy queue in force
  (`docs/agent-control.md`, 「The deploy queue is flushed every hour」): the platform link's word on the kind
  `agency.deploy-flush-minutes`, else the policy's, sixty; the deploy queue is the platform's one, so the kind
  stands at the platform alone. It is the owner's to change at any time, with no code change and no
  deployment, and the next read of the flush's clock takes it. `PATCH /settings` takes a positive whole
  number, or null to return to the policy's; anything else is refused 400 with `field`.

### repositories, prefix-checks and room-holdings

| Method | Path | Does |
|---|---|---|
| GET | `/repositories` | Every repository the caller reaches. |
| POST | `/repositories` | Add a repository. |
| GET | `/repositories/{repositoryName}` | One repository the caller reaches: checkouts, node, prefix documents, preamble, rules, pin, jobs. |
| PATCH | `/repositories/{repositoryName}` | Change a repository: fields, checkouts, node placement, pin, sampling, ready its bench. |
| DELETE | `/repositories/{repositoryName}` | Remove a repository no job holds a checkout of. |
| GET | `/repositories/{repositoryName}/definition` | The definitions a run there is handed: trailer and handoff. |
| GET | `/repositories/{repositoryName}/workbench` | The repository's workbench and scratch folders. |
| DELETE | `/repositories/{repositoryName}/servers` | Take down what the ended runs in the checkout left running (`docs/run-groups.md`). |
| POST | `/repositories/{repositoryName}/words` | Say words to a repository's worker, into its job's turn or a turn of its own. |
| POST | `/prefix-checks` | Which of these files are in the prefix of a run at a repository's path. |
| GET | `/room-holdings/{roomId}` | What the agency holds of a room. |
| DELETE | `/room-holdings/{roomId}` | Let a room's data go; `dryRun` says what would go. |

- **Every repository is a row** (`docs/repositories.md`, 「What a repository row holds」): `name`, `label`,
  `ownerAccount`, `path`, `nodeName`, `supportCount`, `prefixDocuments`, `preamble`, `rules`, `devCommand`,
  `pin` — `{provider, model}` or null — `umbrella`, `downgradeHeal`, and `sampling`, the sampling a run on a
  pinned registration is sent: each setting it holds by name, a number or a switch.
- **`umbrella`** is true for a folder holding other repositories' checkouts below it, whose own tree its jobs
  never write (`docs/worker-slots.md`, 「An umbrella repository」), false where an add names none. An umbrella
  takes no support checkouts and no node checkouts: an add of one naming no `supportCount` takes `0`, and an
  add or a change that would leave an umbrella with a `supportCount` other than `0` or with node checkouts is
  refused 400 `umbrella`.
- **A repository answers its sampling's settings,** `samplingSettings`, every setting its sampling may hold,
  each `name` and either `switch`, true for an on-or-off setting, or its bounds — `min`, `max`, `aboveMin`, true
  where the least value itself is refused, and `whole`, true where only whole numbers are taken. A setting
  left out of `sampling` is the engine's own default, which agency neither sets nor reads, so no default value
  is answered: a screen shows a setting the repository leaves out as the default.
- **`supportCount` is `"auto"` or a whole number from 0 to 8** (`docs/worker-slots.md`, 「The base checkout and the
  support checkouts」). An add naming none takes `"auto"`; a change takes either, and refuses anything else 400
  `supportCount`.
- **A repository answers its checkouts,** `checkouts`, each `name`, `path`, `nodeName`, the node it stands on or
  null, `kind` — `base`, `support` or `node` — and `job`, the job holding it, `{jobId, itemId, role}`, or null, so
  a screen reads which of its checkouts are free. A repository set to `"auto"` lists the support checkouts it has
  made and keeps.
- **A repository keeps no log of its own.** What ran in it is its jobs': `GET /jobs?repositoryName=…` lists
  them newest first, and a job's run is read by the job's own log; a screen shows the log of the job going in
  the repository, else of its latest.
- **An add names its path,** the base checkout's directory, and is refused 400 `path` without one: agency makes
  no checkout and no directory for a repository. It may name `ownerAccount`, the account it is filed under, or
  `ownerUnit`, the unit it stands under, neither being the platform's; a member named on the call adds, changes
  and removes the repositories of their own account and those filed under the units they head (「Who a caller
  acts for」, `docs/repositories.md`, 「An organisation's repository」).
- **A repository takes the work of its own side alone.** A side is one account, or the platform's: what is
  filed under no account or under the administrator's own; a repository an organisation unit owns, `ownerUnit`,
  takes the work of the people of that unit's path alone (`docs/repositories.md`, 「An organisation's
  repository」). Work stands on a repository only where its holder —
  the supervisor that files or holds it, or the role agent's seat, by its `ownerAccount` — stands on the
  repository's side, so a member's supervisor works on its account's repositories alone and the platform's
  supervisors on the platform's. An item filing, an assignment, a launch with every repository it lends, and a
  borrowing that cross the sides are refused 403 `foreign-repository` (「assignments and jobs」).
- **`prefixDocuments`** is the paths of the documents a run's prefix carries at the repository's path, in order
  (`docs/worker-bench.md`, 「Prefix documents」); the add and the change take the list whole, an empty list
  clearing it, and refuse an empty path or one that climbs with `..`.
- **A pin** names a provider agency carries whose program carries the worker line and a model it holds,
  judged as an order's entry is (「supervisors」); null clears it. A changed pin reaches the repository's next
  job; a job already going finishes where it went out.
- **A change that names a node other than the one the repository stands on places it there,** and one that
  names none brings it back: the node must be one the node service keeps and that takes jobs (「nodes」), refused
  400 `nodeName` otherwise, and the path is written as the node reads it. The checkout this machine reads the
  repository through follows the placement.
- **A change may name `nodeCheckouts`,** the repository's checkouts on the work nodes, the list whole, each
  `nodeName` and `path`, a path written from the node's home or absolute, or null for the default beside the
  base's folder (`docs/worker-slots.md`, 「Checkouts on the nodes」). A node the list adds must be one that takes
  jobs, and the repository's base must stand on this machine; one it leaves out is removed, refused 409
  `conflict` while a job holds it, and its checkout on the node stays on the node. An empty list removes every
  one; a body naming none leaves them as they stand.
- **A change is judged whole before anything is written:** its fields, its placement, its node checkouts and its
  sampling each judged first, and a refusal of any of them leaves the repository as it stood.
- **A removal** answers 204, and is refused 409 while a job holds one of its checkouts; 404 for a repository
  the store does not hold.

**Words said to a repository's worker.** `POST /repositories/{repositoryName}/words` hands a paragraph, and
any files with it, to the worker of a repository (`docs/agent-control.md`, 「Words said to a worker」). Its
body is the sessions road's `WordsAsk`, and `worker` may name one of the repository's checkouts; the answer is
the same `WordsSent`, its `said` `started` where a turn of the worker's own was begun rather than taken.

- **The owner's alone:** any `by` but `owner`, or none, is refused 403 `not-the-sender`.
- **A job starting, running or reviewing in one of the repository's checkouts** takes the words by the job's
  road (「assignments and jobs」, 「Words and notes for a running job」): into the turn it works now, or kept for
  its next turn; the answer names its `sessionId`, `run`, `turnId` where a turn took them, and `jobId`, and its
  report and log keep them. A checkout's job whose turn works is chosen before one whose turn does not.
- **Where no job runs there** the words are carried in a turn of the worker's own on the session its role
  keeps at the checkout, under the checkout's own lock and the clock `policy/worker-words.toml` names, and the
  answer is `started` before that turn ends; its answer, or what ended it, stands under the word's run in the
  worker's log.
- **The checkout the words go to** is `worker` where it names one of the repository's, else the checkout of the
  job going in the repository, else its base. A checkout on a work node is refused 409 `worker-on-node` where
  no turn of its own can stand here; a turn already working at it makes the words wait, answered `refused`.
- **A repository the store does not hold** answers 404 `unknown-repository`, and a worker no checkout of it
  is 422 `unknown-worker`. A file over the bound in `policy/worker-words.toml` is refused 413 `too-large`.

`bmo-agency run say <target> --words <text> [--file <path>]… [--by <name>]` reaches these roads and a job's
(「assignments and jobs」) from a shell, its target `session:<sessionId>`, `job:<jobId>`, `repository:<name>` or
`worker:<checkout>`; its `by` is `BMO_TURN_AGENT`
where the shell stands in an agent's program, else the owner.

### nodes

| Method | Path | Does |
|---|---|---|
| GET | `/nodes` | The nodes the node service keeps, each with whether it takes jobs. |

- **The nodes are the node service's,** read afresh and handed on in the node list's order: each `name`,
  `label`, `kind` — `ssh`, or `jake` for a node reached through a hub — `role`, the services the node service
  reserves it for or null, `state` and `checkedAt` in epoch seconds or null.
- **Whether a node takes jobs is agency's,** `takesJobs`, with `jobsRefusal`, the English sentence saying why
  not, or null where it takes them (`docs/work-nodes.md`, 「Which nodes take jobs」). A repository's placement and
  its node checkouts name only a node that takes jobs (「repositories, prefix-checks and room-holdings」).
- **A node service that cannot answer** is 502 `upstream-failed`.

### person-nodes

| Method | Path | Does |
|---|---|---|
| GET | `/person-nodes` | Every account's nodes, one row a node. |
| PUT | `/person-nodes/{ownerAccount}` | One node an account stands on set, beside the ones its other records name: `node`, `shared` and `own`, each false where left out. |
| DELETE | `/person-nodes/{ownerAccount}` | An account's node records, all of them, taken away; `?node=` takes one away. |
| GET | `/nodes/{nodeName}/access` | Whose a node is, and whether a caller reaches it: its owners, the caller's account and the administrator's grant. |

- **An account's nodes are where its runs stand** (`docs/person-nodes.md`, 「Which node is a person's」): an account
  holds one record a node it stands on — its **own node**, the machine bound to it, marked `own` and one, and any
  number of designations onto nodes it may share, many accounts a node and many nodes an account — each row
  `ownerAccount`, `node`, `shared`, whether the node is declared shared for the account, `own`, whether it is the
  account's own machine, `setAt` in epoch milliseconds, and `linked`, whether the node's link is up now: connected
  and heard from in the last twenty seconds (`docs/work-nodes.md`, 「The link」), the judgement a run of the
  account waits on before it starts. Where an account stands on several nodes, its own node places its role runs,
  its newest designation where it has no own node; a job's run stands on its agent's home node or at its worker,
  whatever its agent's account (`docs/worker-slots.md`, 「Names and places」). The administrator's account needs no
  record: its runs are the owner's own and stand on this machine. The service that binds an account to a machine
  sets its record once the machine is proven, and again when the account is bound to another; setting one node
  leaves the account's other records standing.
- **The node is one the node service keeps as a hub node reserved for no service:** another is 422
  `not-a-hub-node`, and a node service
  that cannot answer is 502 `upstream-failed`. Taking away a record no account holds is 404 `not-found`.
- **A node another account holds as its own machine is 409 `node-taken` to every other account, the
  administrator's call included** (`docs/person-nodes.md`, 「Which node is a person's」). A node that is no one's
  own takes further accounts as designations; two records naming one node without either shared are 409
  `node-taken` as before (`docs/person-nodes.md`, 「Several accounts on one node」). An account whose record is
  shared works on the node under a workplace root of its own, `<node root>/accounts/<account>`.
- **These records and the session records agency issues are what the node service's tool judges a caller by**
  before it runs a command on a node (`docs/person-nodes.md`, 「Who reaches a node」). Whether an account owns a
  node is this list: the node's owners are the accounts whose record names it, and the owner of an isolation node,
  `account-<id>`, is the account it is named for (`docs/account-isolation.md`); a node neither names is no
  person's. The account a caller acts for is the `ownerAccount` of the supervisor its record is held under, on
  `GET /supervisors/{supervisorName}`, else of the role agent of that name, on `GET /role-agents`; a caller neither
  names an account of is the deployment's own.
- **`GET /nodes/{nodeName}/access` answers the same judgement in one read,** for a caller that would rather ask
  agency than read the lists. The caller is named as its tool session's record names it: `agent`, the record's
  holder, required, and `role` and `checkout`, the record's labels, where it holds them. The answer is `node`;
  `owners`, the accounts whose record names the node or whose isolation node it is, none for a node neither names; `agent`; `account`, the
  account the caller acts for, null for the deployment's own, read off the seat at its role and folder as well;
  `administrator`, whether it holds the administrator's grant, which a supervisor of no account or of the
  administrator's account holds, and no other caller does; and `reaches`, true where it holds that grant or its
  account is among the owners; and `root`, the node path of the workplace root of the caller's account where that
  account's record names the node shared and the node's workplace is set up, null otherwise, which the node
  service's tool runs that caller's command inside. The node is not asked of the node service: a node it does not
  keep is answered as one no record names. `agent` may stand out, and the node service's owners' read leaves it
  out: the answer then holds no caller — `account` and `agent` null, `administrator` and `reaches` false — and the
  `owners` alone say whose the node is.
- **The administrator's grant is written into the records agency issues** (`docs/tools.md`, 「The session record」):
  a record held under the name of a supervisor of no account or of the administrator's account holds each grant
  `policy/person-nodes.toml` names, `node.admin`, which the tool reads as reaching every node, and
  `platform.payment`, which agency's DeepSeek top-up declares (`docs/tools.md`, 「The DeepSeek top-up」), and every other
  record holds none: another account's supervisor's, a role agent's, a seat's run's and a job's alike, judged as the access read judges. So the tool
  reads the grant off the caller's record and asks agency nothing for it.

### personal-apps

| Method | Path | Does |
|---|---|---|
| GET | `/personal-apps/{ownerAccount}` | An account's personal apps, as the tool host holds them and the PC keeps their versions. |
| POST | `/personal-apps/{ownerAccount}/{name}/put` | The app's folder on the account's PC built into a new version, put at home and pointed at. |
| POST | `/personal-apps/{ownerAccount}/{name}/back` | The app sent back to a version its PC keeps: `version`, the newest older than the current where left out. |
| DELETE | `/personal-apps/{ownerAccount}/{name}` | The app taken out at home and on the PC. |
| POST | `/personal-apps/{ownerAccount}/screen-session` | A short-lived tool session acting for the account, which a console reads and calls the account's personal screens under. |

- **What each does is `docs/person-nodes.md`, 「Personal apps」:** the runner the account's records name is asked
  over its passage, and the tool host's registry through `bmo-pocket`, the binary `BMO_POCKET_BIN` names in agency's
  environment, else `bmo-pocket` on the path.
- **A caller naming a person with `Owner-Key` acts on that person's account alone;** another account in the path is
  403 `forbidden`. A caller of the deployment's own names the account, as the console does for the person signed in
  and agency's app put does for the account its record names.
- **The listing** answers `items`, one per app: `name`; `kind`, `mcp` for an MCP server (`docs/person-nodes.md`, 「MCP
  servers」) and `app` for every other; `version`, `putAt` in epoch milliseconds, `entries`, each
  `{name, title, description}` by its full name, `permissions`, `{network, secrets}`, and `kept`, the versions its
  PC keeps older than the current, newest first, null where the PC was not reached; and `reached`, whether it was.
- **The screen session** is issued at the tool host for the account and nothing else (`docs/person-nodes.md`,
  「Personal screens」): its record names the namespace `my` alone, no capability and no grant, the account as
  `ownerAccount`, the runner its records name and the file of the apps the person does not reach, lasting
  `SCREEN_SESSION_MS`, ten minutes. It answers 201 `{token, expiresAt}`, the token the tool host issued and when it
  lapses in epoch milliseconds; the body is empty. An account with no runner is 409 `no-runner`, and a tool host that
  does not issue it 503.
- **A put, a going back and a removal** answer `name`, `version`, the version now in place (null after a removal),
  `entries` and `said`, the lines the tool host and the build said.
- **Refusals:** an account with no runner, its PC shared or absent, is 409 `no-runner`; a PC that cannot be reached
  is 502 `runner-unreached`; the runner's refusal, a build that failed or the tool host's refusal is 422
  `app-refused` with its words; a person whose chain does not let them put an app is 403 `personal-apps-withheld`,
  and one whose app asks for a permission their chain leaves off is 403 `permission-withheld`, naming it; an
  account service that cannot be asked is 503 `access-unchecked`; going back with no older version kept is 409
  `no-older-version`; an app the account holds none of is 404 `not-found`; a name outside an app's alphabet is 400;
  an MCP server asking for a secret the account's secret space holds no file of is 409 `secret-missing`, naming it.

### catalogue

| Method | Path | Does |
|---|---|---|
| GET | `/catalogue/{ownerAccount}` | The apps of the account's company's catalogue the account sees, what each lets it do, its notices and its share ceiling; `company` names the company for the administrator. |
| GET | `/catalogue/{ownerAccount}/apps/{appId}/source` | The newest version's source, checked against its hash, for a reader who sees the app; `version` names another. |
| POST | `/catalogue/{ownerAccount}/apps` | Share one of the account's items: `name`, `targets`, `kind` and `language`. |
| POST | `/catalogue/{ownerAccount}/apps/{appId}/install` | Install the app's newest version for the account. |
| POST | `/catalogue/{ownerAccount}/apps/{appId}/update` | Move the account's copy onto the newest version, confirming what it asks for. |
| DELETE | `/catalogue/{ownerAccount}/apps/{appId}/install` | Drop the account's copy. |
| POST | `/catalogue/{ownerAccount}/apps/{appId}/clone` | Adopt the account's copy as its own personal app. |
| POST | `/catalogue/{ownerAccount}/apps/{appId}/units/{unit}` | Turn the app on or off for a unit the account heads: `on`, and `lock`. |
| POST | `/catalogue/{ownerAccount}/apps/{appId}/recall` | Recall the app at a unit the account heads, `unit`, or at the platform for the administrator. |
| POST | `/catalogue/{ownerAccount}/apps/{appId}/withdraw` | Withdraw the app, for its owner or maker. |
| POST | `/catalogue/{ownerAccount}/apps/{appId}/approve` | Approve a version, `version`, for a head of the maker's home unit. |
| POST | `/catalogue/{ownerAccount}/apps/{appId}/promote` | Ask for the app to be made a platform app, with a `note`. |
| POST | `/catalogue/{ownerAccount}/apps/{appId}/text` | Set one locale's text, `locale`, `title` and `description`, or clear it with both null, for the maker or the owner. |
| POST | `/catalogue/{ownerAccount}/apps/{appId}/translate` | Fill each locale holding no text by the translator role, for the maker, the owner or a head of the maker's home unit. |
| POST | `/catalogue/{ownerAccount}/seen` | Keep that the account has looked at what was shared with it. |
| GET | `/catalogue-promotions` | Every company's requests to make an app a platform app, for the administrator. |

- **What each does is `docs/person-nodes.md`, 「Shared apps」:** the company's catalogue through `bmo-pocket
  catalogue`, the account service's delegation chain in the account's name, and the runner its records name.
- **Who acts:** as for 「personal-apps」, a caller naming a person with `Owner-Key` acts for that person's account
  alone; a caller of the deployment's own names the account, as the console does for the person signed in. The
  administrator's account reads any company's catalogue by `company`, an empty one where it names none, and recalls
  at the platform; in the company its home unit stands in it is a member as well, installing, updating, dropping and
  adopting a copy there as any member does, and in any other company it holds no copy, 403 `forbidden`;
  `GET /catalogue-promotions` answers the administrator's account and the deployment's own callers
  alone, 403 otherwise.
- **The listing** answers `company`; `ceiling`, the targets the account may share with; `apps`, each `id`, `kind`
  (`app`, `skill` or `mcp`), `key`, `name`, `title` and `description` (the maker's language's), `language` (the
  maker's, or null), `titles` and `descriptions` (by locale, each locale written), `machine` (the locales a
  translation wrote), `server` (for an MCP server, how it is reached, read off its newest version's `mcp.toml`:
  `transport`, `local` or `remote`, and `url`, `auth`, `header`, `secret` for a remote one or `command` for a local
  one, each null where it names none; null for any other kind), `maker`, `owner`, `targets`, `version` (the newest), `versions` (each `version`,
  `sharedAt`, `sourceHash`, `permissions` and `review`), `permissions` and `review` of the newest, `approval`
  (`required`, `approved`), `installs` (how many), `withdrawn`, `promotion`, `installed` (the account's copy,
  `version` and `how`, or null), `updateWaiting` (a newer version stands), `permissionsGrown` (it asks for more than
  the copy's), and `may`, what the account may do with it — `install`, `update`, `uninstall`, `clone`, `withdraw`,
  `approve`, `promote`, `text` (set a locale's text), `translate` (fill the other languages), `recall` and `turnOn`, the last two the units it heads on the account's path, or `platform`
  for the administrator; `notices`, each `kind` (`shared`, `update` or `approval`), `app` and `at`; and `seenAt`.
- **A share** names the item by `name`, and `kind`: `app` where left out, and for an app or an MCP server the
  folder of 'my apps' its PC places, the kind read off its source; `skill` for one of the account's own skills at
  home. `language`, one of `ko`, `en`, `ja` and `zh-CN`, names the language its plain text is written in, `en` where
  left out. Nothing is translated at share. It answers the app's entry as the listing answers it; each target is `{"team": true}`, `{"heads":
  true}`, `{"company": true}`, `{"person": <user>}`, `{"group": <id>}` or `{"unit": <id>}`, and none given shares
  with `team` and `heads`.
- **The source** answers `version`, `sourceHash` and `files`, by path.
- **Every change** answers the app's entry as it then stands; a removal of a copy answers it too.
- **The text** sets `locale`'s `title` and `description` with no machine mark, or clears the locale where both are
  null. **The translation** translates the maker's title and description into each of `ko`, `en`, `ja` and `zh-CN`
  whose title the item does not hold, on the translator role's road as a document's part is (「translations」), and
  writes each with the machine mark; it answers once every locale is written, and a locale whose translation failed
  is left out and the rest written.
- **Refusals:** a target the ceiling leaves out is 403 `scope-withheld`, and one of another company 403
  `other-company`; a person who may not make personal apps is 403 `personal-apps-withheld`, and one who may not
  install received apps 403 `install-withheld`; an app the account does not see is 404 `not-found`; installing a
  withdrawn app is 409 `withdrawn`, one waiting for a head's approval 409 `awaiting-approval`, one not offered to
  the account 403 `not-offered`, and over an app of the account's own of the same name 409 `name-taken`; dropping or
  adopting a copy a lock above keeps on, or turning an app on where a lock above holds it, is 409 `locked`; a unit
  the account does not head is 403 `forbidden`; text or a translation asked by anyone the route does not name is 403
  `forbidden`, and a locale outside the four is 400; installing a skill over one of the account's own of the same
  name is 409 `name-taken`; and the personal apps' own refusals (「personal-apps」) stand as they are.

### personal-skills

| Method | Path | Does |
|---|---|---|
| GET | `/personal-skills/{ownerAccount}` | The account's skills at home, its own and its installed copies. |
| GET | `/personal-skills/{ownerAccount}/{name}` | One skill's files. |
| PUT | `/personal-skills/{ownerAccount}/{name}` | Save one of the account's own skills whole: `files`, each file by its path as text. |
| DELETE | `/personal-skills/{ownerAccount}/{name}` | Remove one of the account's own skills and every attachment of it. |

- **What each does is `docs/person-nodes.md`, 「Skills at home」;** who acts is as for 「personal-apps」.
- **The listing** answers `skills`, each `name`; `description`; `title` and `summary`, its front matter's text in its
  own language, else null; `titles` and `summaries`, by locale; `language`, or null; `version`; `origin`, `own` or
  `installed`; `app`, the catalogue id an installed copy is of, else null; and `attachedTo`, the names of the holders
  it is attached to.
- **One skill** answers `name`, `version`, `origin` and `files`, by path. **A save** answers the skill as the listing
  answers it; **a removal** answers `{name}`.
- **Refusals:** a name outside a skill's alphabet, a `SKILL.md` that does not read, holds no `description` or names
  another name, a path that climbs out or names a mark, a file that is not text and a whole larger than the bundle
  size are 400; saving over an installed copy is 409 `name-taken`, and removing one is 409 `installed-copy`, since a
  copy is dropped through the catalogue; a person whose chain does not let them make personal apps is 403
  `personal-apps-withheld`; a skill the account holds none of is 404 `not-found`.

### attachments

| Method | Path | Does |
|---|---|---|
| GET | `/attachments/{ownerAccount}` | The account's holders, what each line takes and what is attached to each, and the skills and MCP servers that may be. |
| PUT | `/attachments/{ownerAccount}/{holderKind}/{holder}` | Set one holder's attached skills and MCP servers whole: `skills` and `mcp`, each a list of names. |

- **What it keeps is `docs/person-nodes.md`, 「Attaching skills and MCP servers」;** who acts is as for
  「personal-apps」.
- **The listing** answers `holders`, each `kind` (`supervisor` or `agent`), `name`, `label`, `role` (an agent's role,
  null for a supervisor), `line`, `takes` (`{skills, mcp}`, whether its line takes attached items of each kind),
  `skills` and `mcp`, the names attached to it; supervisors first, the oldest first, then agents by name. `skills`
  and `mcp` list what may be attached, each `name`, `title` (or null), `titles` by locale and `origin`, `own` or
  `installed`.
- **A change** answers the holder as the listing answers it.
- **Refusals:** a non-empty list of a kind the holder's line takes none of is 409 `line-takes-none`, naming the kind;
  a holder or an item the account does not hold is 404 `not-found`; a holder of another account is 403 `forbidden`;
  a holder kind other than `supervisor` and `agent` is 400.

### assignments and jobs

| Method | Path | Does |
|---|---|---|
| GET | `/assignments` | Assignments, by holder, repository or state. |
| POST | `/assignments` | Assign an item to a repository and the role it runs as, for the harness holding it, and hold it until its job starts and ends, the harness releases it, or the session its holder names ends; no clock ends a hold. |
| GET | `/assignments/{assignmentId}` | One assignment. |
| PATCH | `/assignments/{assignmentId}` | Release an assignment. |
| POST | `/borrowings` | Lend a running job a checkout of another repository on its own machine, put on the job's branch and held in the job's name until the job gives its checkout up; with `wait`, wait for one to come free. |
| GET | `/jobs` | Jobs newest first: by state or running, by item, room, repository, role or holder. |
| POST | `/jobs` | Run an item as its assignment's role in its repository: the harness hands over the item's content and the lease it holds, and agency launches the run once a slot of the role, a checkout and a checkout of every other repository its `repositories` label names are free; never refused for quota, it waits. |
| GET | `/jobs/{jobId}` | One job and what its turn spent, `usage`. |
| PATCH | `/jobs/{jobId}` | End a job with its outcome, or stop it, its turn asked to end first; its assignment is released and its slot given back. |
| GET | `/jobs/{jobId}/prompt` | The prompt the job was launched with. |
| POST | `/jobs/{jobId}/words` | Say words to a job that is starting, running or reviewing: into the turn it works now, or kept for its next turn. |
| POST | `/instruction-texts` | The work-order step's line composed from an instruction. |
| POST | `/item-filings` | File an item in the job service for the turn an agent is carrying now: its room as the item's `scope`, the repository it names as its `assigneeName`, and `asked-by=owner` where every message of that turn is the owner's. |
| POST | `/job-stops` | Stop a job, by its id or by its item, for the turn an agent is carrying now, only where every message of that turn is the owner's. |
| POST | `/job-reviews` | Examine a job's report under review, for the turn an agent is carrying now: ask the worker, send the work back, pass the check due, or put the work to the owner. |
| POST | `/pane-deaths` | A head pane died: every job its worker (`agent`) is running ends failed with the exit `code` and the `last` stderr line as its report. |

The list answers the newest jobs first, `limit` of them (50 where it names none, at most 500), and pages by a
job's id as the runs' list does: with `before`, the jobs before that id; with `after`, the jobs after it nearest
to it; `more` says whether more lie beyond the page in the direction it was asked. It narrows by `state`, one
state or several comma-separated; by `running`, `true` the jobs starting, running or reviewing and `false` every other;
by `itemId`; by `roomId`; by `repositoryName`, the jobs in any checkout of that repository; by `workerName`,
the jobs in that one checkout; by `roleName`; and by `holderName`, every filter named holding at once. A query
naming anything else, or a value that does not read as its filter's kind, is refused 400.

- **A job's `state` is one of five,** and every route that answers a job — the list, one job, a launch, a
  change, a stop and a review — answers whichever it stands in: `starting`, launched and its turn not yet
  going; `running`, a turn of its work going; `reviewing`, its turn answered and its report under the
  orderer's review (`docs/job-review.md`); `ended`, finished with its outcome; and `stopped`, cut short by a stop,
  a bound or the loss of what ran it. `PATCH /jobs/{jobId}` takes only the two that end a job, `ended` and
  `stopped`, and refuses any other 400.

- **A job names the agent that does its work** as `agentName`, the agent its item's `agent` label named or the
  one its launch took for its item's `model` label, none for a job launched before items named one; its log's lines stand under that name (`docs/run-records.md`,
  「Agent log」).
- **A job and an assignment name where they run twice:** `workerName`, the checkout — the repository's base,
  a support worktree or a checkout on a node — and `repositoryName`, the repository that checkout is of, or
  null for a checkout no repository holds any longer. `GET /assignments` narrows by `repositoryName` and by
  `workerName` alike.

- **`POST /assignments` names the repository by its `assigneeName`,** exactly as `GET /repositories` lists it,
  or a checkout of one: an assignee no repository holds a checkout by is refused 422 `unknown-repository`,
  naming the assignee and, where they are few, the repositories' names, else pointing at `GET /repositories`;
  a repository of another side than its holder's — the account of the supervisor or the seat the holder is
  (「repositories」, 「A repository takes the work of its own side alone」) — is refused 403
  `foreign-repository`, nothing held; a repository whose checkouts are all held now is refused 422
  `no-worker-free`, and the item waits for one to come free. A repository set to `"auto"` grows a support
  checkout instead and answers it held
  (`docs/worker-slots.md`, 「Growing a checkout at a launch」): refused 409 `agent-busy` where no agent of the
  item's `agent` or `model` label is left free, and 422 `no-worker-free` where the machine has no room, its
  sentence naming each reason. A checkout is held now while an assignment of it is `held` or `running` or a job is at work in
  it, `starting`, `running` or `reviewing` unparked; the queue's notices count a checkout free by the same
  reading (`docs/agent-control.md`, 「An idle agent is told to the orderers of the items it could take」).
- **An assignment naming no `assigneeName` goes where its item goes,** never to a checkout of another
  repository: the `assigneeName` of the `item` it hands over, else the item's `assigneeName` as the job
  service answers it by `itemId`, and the checkout named `worker` for an item naming no repository. A job
  service that cannot be read is 502 `upstream-failed`, and nothing is held.
- **An ask for the same item by the same holder answers its standing hold,** 200 with that assignment
  instead of `no-worker-free`, so a lost answer asked again is no busy worker. An ask naming another worker
  or role releases the standing hold first. A hold the harness will not run is released by
  `PATCH /assignments/{assignmentId}` with `state` `released`, which is the harness's `release_worker`.
- **An item is held on one checkout at a time.** An ask for an item releases first every `held` assignment of
  it another holder left, and a `held` assignment of an item that a newer assignment of the same item follows,
  under any holder, is released before anything reads the assignments, so a hold left under a name no session
  ends with goes once its item is asked for again. A job's own loan, below, is no assignment of its item in
  this sense, on either side.
- **A hold named for a session ends with it.** An assignment held under a session's id, or under the agent's
  own name where it keeps one session across its rooms, is released when that session ends — by its end
  route, by a supervisor reset that takes it, or by a turn its guard cut — unless a `starting` or `running`
  job owns it. So a hold a turn took and never started its job for does not outlive the session. An
  assignment a job runs on is the job's: its end releases it.
- **A job's own loan of a checkout is an assignment held in its name,** its `holderName` `job:<jobId>`
  (`docs/worker-slots.md`, 「A checkout lent to a job」). No job starts on it. Once that job gives its checkout
  up — ended, stopped, failed, or its review paused — any read of the assignments gives the loan back: parked
  onto the job's branch first on this machine, reading held until it is released (`docs/worker-slots.md`,
  「Loans across a job's review」).
- **A launch lends the repositories its item names.** `POST /jobs` reads the `item`'s label `repositories`,
  repository names comma-separated, and keeps the names other than the job's own repository on the job; they
  are held fixed on the lease with the launch's other labels (below). It refuses, nothing written: a name no
  repository holds 422 `unknown-repository`; a repository with no checkout on the machine the job's checkout
  stands on 422 `no-checkout-here`; more checkouts than `job_checkouts_most` (`policy/workers.toml`), the
  job's own among them, 422 `too-many-checkouts`; and a repository, the job's own or one it lends, of another
  side than its holder's 403 `foreign-repository`. Before the job's first turn a checkout of each is lent
  together, the job waiting `starting` with its `heldFor` naming them while any has none to give, and its
  prompt opens with each lent repository, its path and its branch (`docs/worker-slots.md`, 「A checkout lent
  to a job」). A checkout that cannot be put on the job's branch fails the job with what git said.
- **`POST /borrowings` lends a job a checkout,** the route under `bmo job borrow` (`docs/worker-slots.md`,
  「A job writes only in its own checkout」). Its body names the job by `runName`, the run it works in, the
  repository by `repositoryName`, and `wait`, true to wait for a checkout rather than be refused for none.
  - **Who may ask:** the control token's holder for any running job, or the job's own program by its session
    token: a record whose holder is not the checkout the job runs in is refused 403 `not-this-job`.
  - **What it takes:** a checkout of the repository on the machine the job's own checkout stands on, chosen as
    every loan is — the one standing on the job's branch where one does, else a free support worktree first
    and its base checkout last — held as the assignment `job:<jobId>` for the job's item. Where none is free,
    a repository set to `"auto"` grows one, bounded by the machine's room alone. A support worktree not made
    yet is made before the answer, as a launch makes it; one that cannot be made lets the loan go and is
    refused 500 `checkout-not-made`, saying why. The checkout is then put on the job's branch; one that cannot
    be is given back and refused 500 `checkout-not-placed`, with what git said. It answers 201 with
    `workerName`, `cwd` and the assignment's `assignmentId`, or 200 with the loan the job already holds, asked
    again.
  - **What it refuses:** a run no starting or running job works in 404 `no-job`; a repository of another side
    than the job's holder's 403 `foreign-repository`; the job's own repository 422 `own-repository`; a
    repository with no checkout on that machine 422 `no-checkout-here`; a loan that would
    take the job past `job_checkouts_most` 422 `too-many-checkouts`; one whose every checkout there is at work
    and that grows none 422 `no-worker-free`, naming them, and the machine's reasons where its room held the
    growth.
  - **With `wait`,** a repository with no checkout to give holds the ask, looking again whenever a job or an
    assignment changes, for at most `borrow_wait_seconds` (`policy/workers.toml`), and answers 201 once one is
    lent, or 202 with `still-waiting` as its `code` and the sentence naming what holds every checkout, for the
    verb to ask again. Every other refusal stands as above.
- **The role a job runs as is its assignment's.** `POST /assignments` takes `roleName`, a role the store
  holds, `worker` where it names none, and refuses another 422 `unknown-role`; an assignment and its job
  answer it as `roleName`. The job's run is a run of that role at the checkout: it holds a slot of the role
  keyed by the checkout, goes out on its item's model, the repository's pin or the road of an agent of the role
  (「A job goes out on the road of an agent of its role」 below), in a session of the role's
  started for each turn and ended with it, and resumes the session the role keeps at that checkout on that
  provider and model where it keeps one (「sessions」 above; `docs/role-slots.md`, 「The line」).

A job's `itemId` and `leaseId` are the harness's references, kept as given. The harness reads the job's
progress, questions and outcome from its fields and events, and reports and closes the item in the job
service.

- **A running job keeps its item's lease, whoever launched it.** While a job is `starting`, `running` or
  `reviewing`,
  agency renews its item's lease in the job service on the harness's behalf on its own clock
  (`policy/job-leases.toml`), so the lease holds however long the job runs and whether or not the harness
  that took it still stands — its session replaced, its plugin restarted, agency itself restarted under the
  job. A lease the job service no longer holds, run out before a renewal reached it, is taken again on the
  same item for the same holder and assignee, and the job keeps the new lease's id, which its end keeps or
  gives back as below. An item another holder leased meanwhile, or one no longer open — held, dropped or
  merged — is left as it stands, said in agency's log and told once to the item's orderer
  (`item-moved-under-job`, `docs/agent-control.md`, 「An item's change is told to its orderer」). The harness
  may renew the lease as well; a renewal of either holds it. Agency keeps no job store; the calls it makes
  there are the ones a job's end states below, the filing of an item, and the reads 「Calls agency makes」
  names.
- **A running job's lease holds fixed what its launch read** (`docs/repositories.md`, 「An agent an item
  names」): as the job is written, and with every renewal and every lease taken again while it is `starting`,
  `running` or `reviewing`, agency sets the lease's `frozen` in the job service to the item's labels `agent`,
  `role`, `model` and `repositories` and its fields `assigneeName` and `scope`, with the reason naming the job and the agent it
  runs on, so the job service refuses a change of any of them while the job runs. A renewal of a lease whose
  job has ended sets `frozen` to null, so what the item's next launch reads is changed freely. A lease the
  job service could not be asked to hold is said in agency's log, and the next pass asks again.

- **An item a supervisor files goes through agency,** `POST /item-filings`, so what the item says of where it
  was asked and who asked is agency's reading of the turn rather than anything the model writes. It takes the
  `agentName` its harness plugin was started for — never one the model names — and the item's `title`,
  `brief`, `queueName`, and where they apply `repository`, `priority`, `assigneeName`, `requesterName` and
  `labels`, and raises it in the job service (`POST /items`) on the chat's own caller token where the
  deployment names one:
  - its `scope` is the room of the turn that agent is carrying now;
  - `repository` names the repository the work is in, one the store holds (`GET /repositories`), and is
    written as the item's `assigneeName`, the one field a launch reads for where an item goes
    (`POST /assignments`), so the item is taken only in that repository's checkout; a repository the store
    does not hold is 422 `unknown-repository`, and a call naming an `assigneeName` other than its
    `repository` is 400 `bad-request`. An item for work in no repository, host work, names none and goes to
    whichever worker its launch names;
  - a label `repositories` names the repositories a change reaching two or more of them is lent at its launch
    (「A launch lends the repositories its item names」 above), each one the store holds, refused 422
    `unknown-repository` otherwise, and no more than `job_checkouts_most` with the item's own repository among
    them, refused 422 `too-many-checkouts`;
  - the repository and every repository the label names stand on the filing supervisor's side
    (「repositories」, 「A repository takes the work of its own side alone」); one of another side is refused
    403 `foreign-repository`, nothing filed;
  - its labels gain `asked-by=owner` where the receiver marked that turn privileged — every new message in it
    the owner's, said under the owner's own sender, which the account service links to the owner's account, whatever
    surface it came through, the console's included (`STRUCTURE.md`, 「In the console a person is their account」) — and lose any `asked-by` the call names, so the job service's unattended labels open an item
    the owner asked for at once while one the agent raised itself, or anyone else asked for, stays a draft
    until an approver approves it: the owner, or the supervisor itself where the deployment marks the chat's
    caller an approver in the job service, for the routine work its own instructions let it judge; agency
    tells the orderer of each such draft once (`docs/agent-control.md`, 「An item's change is told to its orderer」);
  - an agent carrying no turn now files no item: 409 `no-turn`;
  - **it names its agent, or a model and a role** (`docs/repositories.md`, 「An agent an item names」): a call
    whose `agent` and `model` labels are both missing, empty or `auto` is refused 409 `agent-unnamed`, nothing
    filed, its sentence listing the agents of the role it would run as — its `role` label's, else the role a
    job runs as —, each with its road and whether it is free or carries a run, and asking the supervisor to
    name one from what it has learned; an agent the agency does not register is refused 422 `unknown-agent`;
    a call naming a model and no agent is filed only where that role holds an agent switched on, of the
    filing supervisor's account, whose road runs the model, and is otherwise refused 422 `no-candidate`,
    nothing filed, its sentence naming the role, the model and the role's agents with their roads;
  - **its steps are checked before it is filed** (`docs/job-review.md`, 「Stages」): a call whose
    `stepsCheck` is not the mark the check answers is refused 409 `steps-unchecked`, nothing filed, its
    sentence naming the agent the item runs on, the skills and the tools that agent holds, what the steps are
    checked against and the mark; an item naming a model is checked against the first of its candidates by
    name, its sentence naming them all. The mark is read off the item's title and brief and that agent, or
    the role and the model, alone, so the steps may change before the call that names it;
  - `steps`, where it names any, are the stages the item's job runs in (`docs/job-review.md`, 「Stages」):
    once the item is raised, its work order is saved in the job service with the item's title as its purpose,
    its brief as its directive and these steps, `saved` (`PUT /items/{itemId}/work-order`); a work order the
    job service refuses leaves the item raised, and the answer's `workOrderRefused` carries the job service's
    sentence, null where the work order was saved or no step was named.
  It answers 201 with the item's `id`, `state`, `scope` and `labels`, as the job service answered them. The
  job service's refusal of the item itself, a 4xx, keeps its status and its code with its sentence, a label
  naming what agency does not hold among them (422 `label_value_unknown`, `docs/agent-control.md`, 「A label
  agency does not hold is refused as it is set」); any other refusal of the job service's is 502
  `upstream-failed` with its sentence.

- **A supervisor stops a job on the owner's word,** `POST /job-stops`. It takes the `agentName` its harness
  plugin was started for, never one the model names; exactly one of `jobId`, the job, and `itemId`, the item
  whose `starting` or `running` job it stops; and `reason`, a short line that becomes the job's report. The
  turn's privilege decides, as it does for a filing, so no one else in a room stops a job through the chat
  agent:
  - an agent carrying no turn now stops nothing: 409 `no-turn`;
  - a turn the receiver did not mark privileged stops nothing: 409 `not-owner`;
  - an item with no job `starting` or `running` is 404 `no-job`, and a job already ended or stopped 409.
  The stop is the one below, and its answer is that stop's.

- **A supervisor examines a job's report,** `POST /job-reviews` (`docs/job-review.md`). A job's turn that
  ends answered does not end the job: the job stands `reviewing`, its report is written onto the item's review
  in the job service, and its orderer is told the check due now (`job-review`). The call takes the `agentName`
  its harness plugin was started for, never one the model names; `jobId`; `action`, one of `ask`, `reject`,
  `pass` and `escalate`; `text`, the words the action hands on, required for every action but `pass`; and, for
  `reject` alone, `stage`, the stage counted from 1 the work goes back to, one the job has reached
  (`docs/job-review.md`, 「Stages」), else 400 `bad-request`.
  - an agent carrying no turn now acts on nothing: 409 `no-turn`;
  - a job that is not `reviewing` is 409 `not-reviewing`, and one naming no job 404 `no-job`;
  - `escalate` while the job's work was sent back fewer times than the review policy's ceiling is 409
    `escalation-early`, and every action while the owner's question on it waits is 409 `awaits-owner`;
  - it answers 200 with where the job stands after the move (`JobReviewed`): its `id`, `itemId` and `state`,
    and `review`, its review's position — `stage`, `stages` (how many there are), `round`, `check`,
    `rejections` and `question` as below — and nothing of its report, its steps or its spend, which the
    review's instruction told or a read of the job answers; a turn the move asks for runs apart from the
    answer, as a launch does.
  Every job answers `review`: `stages`, the steps it runs in order, empty for a job of one stage; `stage`, the
  stage under way counted from 1, or null for the final report; `round`; `check`, the check due now, kept
  while a question of it is out and null while a work turn runs; `rejections`; `question`, the owner's question while the work waits on it, else null; and
  `parked`, whether the job gave its checkout up while that question waited.
- **A job is stopped by asking its turn to end.** `PATCH /jobs/{jobId}` with `state` `stopped` and
  `POST /job-stops` stop a job alike:
  - the reason, the `report` the stop names, is kept as the job's stop mark, and the job's run is cancelled:
    the cancellation the turn's door reads in the run's program, which puts the stop words in — commit,
    push, write the handoff, exit — and leaves the turn to end on them, a turn picked up across a restart
    included; the turn's answer to them is the job's handoff, noted on its item as above, and a turn that
    ends with no answer leaves one written from its run log. A tool
    call blocked on a read or a write is stopped on the stop, so the words are read at the next boundary,
    and a turn that has not ended within the stop's bound is interrupted and ends stopped;
  - the answer waits for that turn to end. The turn's end writes the job
    `stopped`, outcome `stopped`, with the reason as its report, releases its assignment, gives its slot
    back, returns its lease and tells its room, so its slot and its checkout take the next launch as soon as
    the stop answers;
  - a job no turn carries is written so at once, its assignment released and its slot given back alike, its
    handoff written from its run log.
  A job's end never writes over a job already ended or stopped.

- **A job that ends keeps its item's lease for the item's close.** A job written `ended` — its run
  finishing or failing, or the harness ending it — gives no lease back: the lease it held stands, and agency
  goes on renewing it on the same clock as a running job's, until the job service holds it no more. The
  item stays `leased` from the job's start through its close, never `open` between them: the requester
  writes the final report and moves the item to `done`, which ends the lease (the job service's 「States and
  moves」), or, where the item is not to close, gives that lease back itself, and the next renewal finds it
  gone and stops. A lease the job service no longer holds is not taken again for an ended job. An item the
  lease pass still finds leased at the pass after the one that first found its job ended is owed to its
  orderer once for that lease and kept until the lease ends (`job-awaits-close`), unless its commit waits for
  its repository's batch deployment, which the notice of that commit tells instead.
- **An item whose commit waits to go live stays leased until it is live.** Where the job's close handed a commit
  over, the item's labels say so (the job service's `docs/cli.md`, 「`bmo-job job close`」), and the requester
  does not move it to `done` at the job's end: it puts the commit live, and closes the item once the deployment
  ended live. A deployment that fails closes nothing and files nothing: the requester writes what failed onto
  the item and launches the same item again on the lease that stands, as the same worker and role.
- **A job that is stopped gives its lease back.** A stop — the harness's or the owner's, or the pass below
  ending a job whose process went away — returns the item's lease to the job service as the end is written,
  so the item is open again for a rerun.
- **A job's end reaches the room.** A job's answered turn ends it only once its review has taken the work
  (`docs/job-review.md`, 「The job's end」); however a job ends, agency queues the job's report for the item's
  orderer through the receiver's `POST /instructions`: an order under the sort `job` in the orderer's room,
  naming the requester of the item the orderer was found on, and carrying the item, the repository, the role,
  the outcome and the report, under the kind `job-ended:<item>` and the event `job-ended:#<item>`; an
  ended job's order names the lease that still stands on the item, which the close ends. The report goes
  whole only where no review examined it — a job stopped, failed or blocked; a job that ended `ended`, whose
  report its review passed and so told the orderer already, has its order name that report and the item it
  stands on instead (`docs/job-review.md`, 「The job's end」). Agency owes it in its
  outbox until the receiver takes it, and owes the end of a job whose notice a restart of agency cut off once the
  policy's grace has passed (`docs/agent-control.md`, 「An item's change is told to its orderer」); the receiver
  keeps the instruction until a turn takes it, so a chat session being replaced when the job ended loses nothing.
  The supervisor reads the report and relays the job's end in a few lines in that room, as its own manual
  says, the report itself staying whole on the item. The orderer is the item's room, else its parent's up
  the chain it was raised from, read from the job service (`docs/agent-control.md`, 「An item's change is
  told to its orderer」); where the job service cannot be read or the chain finds no room, the job's own
  `roomId` from the launch, which the harness hands over as the item's `scope`; and where none is found, the
  owner's private room, which is the room the receiver gives an instruction that names none. A job that ends
  `failed` or `blocked` in a room other than the owner's private room is also queued as an instruction in the
  owner's private room, under the kind `job-failed:<item>`.

- **A job whose turn goes on across agency's restart ends as its turn does.** Its turn is kept in flight
  while it goes, and the server that starts next picks it up (「sessions」 above): a turn whose program's
  record holds its end, or comes to hold it, ends the job `ended` with the turn's answer as its report,
  as its launch would have, its run closed, its assignment released, its slot given back, its lease kept for
  the item's close and its end told to the room. The pass below leaves a job whose turn is kept in flight to
  it.
- **A job whose turn had not gone out when agency restarted is launched again.** A job keeps the words of
  the turn it carries until that turn comes back from its session: its first prompt once it is composed, a
  turn its review asks for as the review asks it, and the words a turn carried onto the next road goes out
  with. A server that starts, after it has picked up the turns in flight and before the
  pass below, launches again every job this process does not carry and no turn kept in flight names that is
  still `starting`, or that keeps the words of a turn while it is `running` — waiting for a road or a slot, or
  between one road and the next — or `reviewing` with its review's turn still taking its checkout back. Each goes out as its launch would have, on the road its walk answers
  now, from the words it keeps; a first prompt not yet composed is composed again, its review opened only
  once. Its slot request still waiting in the line is taken up again rather than asked anew, so the job
  keeps its place, and a seat it was let in on stays its own; a request asked on another road than the one
  the job now takes is cancelled and the job asks again. A job's turn the pickup finds no program took is
  one such, its kept turn let go. A `running` job whose checkout stands on another machine is not launched
  again, since a turn that went out there is no record this machine keeps.
- **A job left running with no live execution ends as stopped on the pass.** The process that ran it can
  go away with agency's own restart before the run writes its end, on a program that keeps no record the
  next server can read it from. On start, after the turns in flight are picked up, and on the reconcile
  pass that keeps the execution rows, a job still `starting` or `running` whose turn is not kept in flight
  and that the start did not launch again is read against the records the store already keeps: an open execution row of its run's on its role's line begun at the job's start is a live
  execution, and an open run in the ledger is a live run. A job with neither — or whose run is already
  closed — ends as `stopped` with a report saying the process that ran it went away, its open run is closed,
  its assignment released, its slot given back, and its lease returned and its report told to the room
  through the same delivery as any other end. A job younger than the launch window is left for the next pass, and one whose checkout
  stands on another machine is left, since its execution is no record this machine keeps. A job with a live
  execution is left running, and so is one this process is still carrying — waiting for a road, or between
  one road and the next.
- **A job may carry an instruction,** `instruction`, beside its item or inside it — `repository`,
  `documents`, `sentences` and `checks`, every one required and none empty (`docs/worker-bench.md`, 「A
  small-model instruction」); one that lacks a field is refused 400 naming it. The job keeps it, answers
  it, and its prompt renders it under the item's title. `POST /instruction-texts` answers `text`, the
  one line a work-order step is written as, composed from an instruction.
- **A job's prompt carries its item's newest notes, as many as fit the policy's bytes**
  (`policy/job-prompt.toml`, `[notes]`): the notes that do not fit are left out from the oldest, and one
  line says how many were left out and that they stand on the item. The report trailer every job is held
  to is no part of the prompt; it stands in the run's cached prefix (`docs/launch-lines.md`). The job keeps
  the newest note its prompt was composed with, so a note the item gains later reaches the job by the road
  below rather than twice.
- **Words and notes for a running job.** A job that is `starting`, `running` or `reviewing` takes words from
  outside it, and no word for it is lost (`docs/agent-control.md`, 「Words for a running job」):
  - **They are said to the job,** `POST /jobs/{jobId}/words`, or to any session one of its turns ran in,
    `POST /sessions/{sessionId}/words` (「sessions」 above); both take `WordsAsk`, answer `WordsSent` and hold
    the same rules of who may say. A job that has ended answers 410 `run-ended`, and a job no row holds 404
    `unknown-job`.
  - **A note added to the job's item while it runs is one of them.** Agency reads the job service's
    `note.created` events in its minute pass (`docs/agent-control.md`, 「An item's change is told to its
    orderer」) and hands each note on the item of a job that runs to that job, behind a paragraph naming it a
    note of the item and who added it; a note the job's own turn wrote, the job service's own record, a note
    the job's prompt already carried and a handoff note, which the turn that goes on opens with already (below),
    are left out.
  - **Each is kept on the job before it is offered,** in the store's `job_words`, so a word survives the turn
    it was said into ending, a review between stages and agency's own restart.
  - **The turn the job works now takes them at its next look,** where its provider takes words mid-turn,
    whether this process started that turn or picked it up across a restart; the answer is `delivered`, or
    `waiting` where the turn waits on a person's answer.
  - **Where no turn of the job takes them** — it is starting, waits for its seat or its road, stands under
    review between stages, or its provider takes no words mid-turn — the answer is `kept`, and the job's next
    turn carries every word kept for it behind its own prompt: the first turn, a stage handed over, a
    question of its review or a rework. A turn that goes out on another road because its own stopped
    answering carries them again.
  - **The job's report ends with them,** as with any word said into its turn (「sessions」 above).
- **A job's prompt opens on its item's handoff,** where the item holds one: the newest note the job service
  answers for the item labelled `kind` `handoff`, unless a job of the item ended answered after it was written,
  stands under the item's title, after the preamble bundle and the checkouts lent to the job where the prompt
  carries them (`docs/worker-bench.md`, 「Preamble」), ahead of its instruction and brief, under a heading that
  says it is the last session's own handoff, and the
  item's notes below leave it out (`docs/sessions.md`, 「The handoff a job's session leaves」). Its notes are
  read from the job service as the job is written, so a relaunch of the item on any agent or model opens where
  the last session left off.
- **A job's work that moves to another session leaves a handoff note on its item:** `POST
  /items/{itemId}/notes` of the job service with `text`, the handoff, `actorName` `agency`, and `labels`
  `kind` `handoff`, `job`, the job's id, and `source`, `session` where the session wrote it or `log` where it
  was written from the job's run log. A turn of the job's review that goes out on another road, a turn whose
  road stopped answering and went out on the next, a stop and a session handed over as the job ends each
  leave one, and the turn that goes on opens with it; a note the job service does not take is said in the
  job's log and stops nothing.
- **A job's first prompt carries what the gate's reading attaches to a turn.** As the job is launched,
  while its holder — the supervisor that launched it and whose account the item is filed under: an
  account's own supervisor for its jobs, the owner's supervisor for the owner's — stands with both the
  reference and the brain switch on (`docs/supervisors.md`, 「Its settings」), an envelope is handed to the
  gate's turn reading (`POST /reference/selected` of the gate) with no room, no situation and `agent`
  naming that holder. The gate picks nothing: what it answers is what attaches to a turn with no pick —
  the org sheet, the sheet of the rows the services of the deployment left standing in that supervisor's
  brain, and any due or woken rows that brain holds (the gate's `docs/reference-selection.md`, 「What a
  turn is given」) — attached to the prompt under a `## Reference` heading. The instruction's own lines
  ride in the envelope and choose nothing, so a job with no instruction takes what attaches just as a
  lineless round does. A job naming no holder, one whose holder's switches close the reading, or a gate
  that answers nothing stands on its own words; beyond what the reading hands at its start, a job's run
  reads that brain itself through its memory tools.
- **A job goes out on the road of the agent its item names, or its launch takes for its item's model**
  (`docs/repositories.md`, 「An agent an item names」), let in on that agent alone. The item's `agent`, `model`
  and `role` labels are read off the item `POST /jobs` is handed, its `labels`. An item naming a model and no
  agent takes the first agent by name of its role, switched on, of the holder's account, whose road runs the
  model, and free, chosen in the same transaction that writes the job with every candidate's row held, so two
  launches at once take two different agents; where every candidate is busy it is refused 409 `agent-busy`,
  its sentence naming each candidate and what it carries, and where none exists 422 `no-candidate`, its
  sentence naming the role, the model and the role's agents with their roads, told to the item's orderer
  (`item-no-candidate`). An item that names neither an agent nor a model, or names `auto`, refuses the launch
  422 `agent-unnamed`, the sentence
  naming the role it would run as and listing that role's agents, each with its road and whether it is free,
  so the supervisor names one and launches again; an agent the agency does not register refuses it 422
  `unknown-agent`, and one that names no road 422 `agent-names-no-road`. A refusal writes nothing and leaves
  the assignment held; the item's orderer is told once for that label (`item-unlaunchable`,
  `docs/agent-control.md`, 「An item's change is told to its orderer」), and the mark is let go as a job of the
  item is launched. A launch naming an agent that is busy (`docs/role-agents.md`, 「When an agent is busy」) is
  refused 409 `agent-busy` at once, its sentence naming the agent and the job or run it carries at work, an
  agent whose every job stands in the machine's turn being free: nothing is written,
  the assignment stays held, and no request waits to start it later. The orderer is told once the agent is
  free (`item-awaits-launch`), and launches it then. An item's `model` label beside an `agent` label is not
  read. The job answers `chosenBy` `item`, its agent having
  chosen its provider and model; a job row that holds `forced`, `pin` or `order` answers it as it stands, and
  no job is given one now.
- **A job is never refused for quota:** where its agent's road has no quota or cannot answer, `POST /jobs` takes the job all the same and answers 201. The job
  is written `starting`, holds its assignment, its checkout and its item's lease, and waits, answering
  `heldFor`, its sentence naming every provider's reason — the owner's switch, a spent balance or quota,
  each Claude login's own, with the moment the first of them reopens — and what a person must do for the
  first provider a person can reopen, a sign-in command or where a balance is topped up. It asks its road
  again as each usage reading, account change, rest or road fact comes in and on the policy's clock
  (`policy/roads.toml`, `[job]`), and goes out on the first road that can answer, `heldFor` then null. It
  holds no slot while it waits for a road (`docs/provider-order.md`, 「Whether a road can answer」): its slot
  is asked as its first turn goes out, and once let in it keeps that seat through every later turn and its
  review until the job ends (`docs/role-agents.md`, 「Which agent a run takes」). A job's turn not let in at its first ask for a slot of its role names that
  wait as its `heldFor` — the agent it waits for, where its item names one, else its role — until it is let
  in, so a job with a `heldFor` — still `starting`, or `running` toward a turn its review asked for — waits for its
  turn and works on nothing meanwhile. The slot of a job whose item named an agent belongs to that agent's
  account and asks that agent alone (`docs/role-slots.md`, 「Accounts」); where that agent has left the register,
  stands under another role, or moved to an account the ask cannot belong to, the line can never open the seat
  and the job is ended `failed` at once, its report naming the agent and what became of it — a job is never left
  waiting in `starting` on an impossible seat. A job's turn waiting for its card
  names that wait as its `heldFor` until the
  card is granted (`docs/own-llm.md`, 「An engine the arbiter stands」). A job held so — for a road, for its
  card — or not let in at its first ask for a slot of its role, is told to its item's orderer once as the
  wait begins (`job-waiting`).
- **A job whose turn makes no progress is told.** A running job's turn that has made no progress for the stall
  span while it still works — no new line of the model's, no output grown with a new line, no processor time
  above the idle share — is told to its item's orderer once a stall through the receiver's `POST
  /instructions`, under the kind `job-stalled:<item>` and the event `job-stalled:#<item>`, naming what it waits
  on; the instruction is withdrawn once the turn moves again or ends (`docs/agent-control.md`, 「A run's tool
  calls are held to a clock」).
- **A job that will wait is written waiting.** `POST /jobs` judges, as it writes the job, the wait its first
  turn will begin with, and writes that wait as its `heldFor` in the same write, so the `201` answer, the
  `job.created` event and every read of the job carry it from the start: where no road of its walk answers,
  the road's sentence; where its road answers but its first ask for a slot of its role would not be let in,
  the agent it waits for. The ask is judged by the line's own rule (`docs/role-slots.md`, 「The line」) and
  not made, so the job holds no place in the line before its first turn asks. Where the line has moved by
  the time the turn asks and lets it in, its `heldFor` is cleared as the turn goes out.
- **A job never moves to another road.** A turn that fails for its provider's availability — its quota,
  window, balance or credential refused, its last login closing under it, or its road found unable to answer
  as the turn ends, a provider that answers it cannot serve now (a 5xx such as a service bench's 503, or a
  refused connection) among them — holds the job waiting for its agent's road with the reason while the road
  cannot answer, and is launched again on it once it can: the same job, item, lease and checkout, its prompt
  opened with a line saying it continues work already in the checkout; where the road answers again at once,
  the job ends as that turn ended, but a provider that answered it cannot serve now is waited for, its
  sentence in the job's `heldFor`, until the block that answer wrote opens, however long that takes, and the
  job goes out again on that road, its session kept across the wait where it had one
  (`docs/sessions.md`, 「The warm/cold criterion」).
  A job answers `roads`, every road its turns went out on in
  order — each `provider`, `model`, `account` (null off a provider that holds logins), `at` in epoch
  milliseconds and `reason`, why the road before it was left, null on the first — and its report ends
  saying where it ran and why it moved. A turn the Claude Code walk carried to the next account because
  the one before refused it (`docs/cache-fallback.md`, 「The Claude Code road is a list of accounts」) adds
  a road of the same provider and model on that account, its `reason` the refused account and its own
  words. Where none of its role's agents has quota, the job stays `running`
  and answers `heldFor`, the reason it waits; it goes out again as soon as a provider has quota, and
  `heldFor` is null whenever a turn is out and not waiting for its card. A job on its item's model or
  a pinned model never moves: it waits for that one. A failure of the work itself moves nothing. A job
  whose turn every Claude account refused, with no other provider to take it, ends `failed` on that login
  failure, naming each account a person must sign in to, rather than waiting.
- **A job ends by itself as its turn ends.** A turn that answered ends the job `ended` with its answer as
  the report. An answer that is a tool call written as text instead of a call made — tool-call markup or a
  model's control token outside the answer's quoted code, or a lone JSON object shaped like a call, as
  docs/agent-control.md 「What a resident turn reads」 judges it — is no report: the call never ran, so the
  job ends `ended`, outcome `failed`, with a report saying the turn stopped on a tool call written as text. A turn
  that did not answer ends it `ended` with its outcome (`failed`, `blocked`, `stopped`
  or `reprepared`) and the reason the turn gave as the report. A turn cut at the job's own bound — the
  backstop for a turn its tool clock cannot see — ends it `stopped`, outcome `stopped`, with the bound's
  reason as the report, and its lease is given back so the item is open for a rerun. A launch that never
  reached its turn — no session, no line, no herdr — ends it `stopped`, outcome `failed`, with why as the
  report. Every end that is not an answer is also written as a note into the job's log, so the job's lines
  say why it ended.
- **A job's turn is a run of the ledger.** As the turn goes out, the job's runner opens a run under the
  job's run name, its role, its repository and its room, and the job carries it as `runId`; as the turn
  ends the run is closed with how it ended, the tokens its context held and what it spent. A job answers
  `usage`, what its turn spent: `inputTokens`, `outputTokens`, `cacheReadTokens` and `cacheWriteTokens`,
  each summed over the models that answered it, and `tokenCount`, the four together. `usage` is null, and
  `tokenCount` 0, until the turn ends and where its road reported no figures.
- **A job's end releases its assignment, its slot and its checkout,** whichever way it ended, so the next
  job can take them. Its slot is every request asked under the job's owner, `job-<id>`, given back as that
  owner's release gives it back, whatever its lease is bound to (`docs/role-slots.md`, 「The line」); no
  write of a job's end leaves it standing. A `running` assignment whose job has ended is released as the
  assignments are next read.

### session-work and session-loads

| Method | Path | Does |
|---|---|---|
| GET | `/session-work/{sessionId}` | The item and results a role's session holds. |
| PATCH | `/session-work/{sessionId}` | Record the item a session serves. |
| PUT | `/session-work/{sessionId}/results/{resultKey}` | Record a result sent into a session. |
| DELETE | `/session-work/{sessionId}/results` | Forget a session's results as it folds. |
| GET | `/session-loads/{sessionId}` | The documents loaded into a session. |
| PUT | `/session-loads/{sessionId}` | Keep a session's loads. |
| DELETE | `/session-loads/{sessionId}` | Forget a session's loads. |

## Events

Agency publishes one stream, `GET /events`. The kinds are:

`supervisor.changed`, `role.changed`, `session.created`, `session.changed`, `session.folded`, `session.deleted`,
`turn.created`, `turn.changed`, `turn.ended`, `question.created`, `run.created`, `run.changed`, `run.stopped`,
`execution.changed`, `slot-request.changed`, `reservation.changed`, `repository.changed`, `repository.deleted`,
`prompt-document.changed`, `document-move.created`, `overtime-request.created`,
`overtime-request.changed`, `rest.changed`, `account.changed`, `usage.changed`,
`road.changed`, `launch-room.changed`, `supervisor-reset.created`, `place.changed`, `assignment.created`,
`assignment.changed`, `job.created`, `job.changed`, `job.ended`, `worker.changed`, `road-walked`, `supply.changed`,
`background-work.created`, `background-work.ended`.

A kind agency keeps to its own processes does not cross the stream. `events.missed` is the stream's own
notice to a reader resuming too late (「events」 above), never published as an event and never kept.

- **`slot-request.changed`** carries one slot request's change (「slots, reservations and places」 above) as
  one object: `token`, `source` and `state` — `waiting`, `reserved`, `active`, `released` or `cancelled`;
  `role` and `owner`; `askedBy` and `title` where the ask named them; `provider`, `model`, `childId`,
  `agentName` and `outcome` as far as they are set; and `enqueuedAt`, with `leasedAt` and `endedAt` once
  they are set. It is published when an ask opens a waiting request, when the line lets a request in, raises
  a reserved slot or binds a lease to its run, when a request is given back by its holder, its owner, a job's
  end or the line on evidence its run ended, and when a reservation's seats are raised or given back.
- **`supervisor-reset.created`** carries a supervisor's reset (`docs/supervisors.md`, 「Starting it afresh」) in
  two shapes: each move of its reset point, `agent` and `point` in epoch milliseconds, so the rooms' owner lets
  go what waited for its turns; and the reset once agency's part is taken, `agent`, `ranges`, `alone` and
  `shared`, the rooms, `runs`, the runs its own log lines went under, `by` and `at` in epoch seconds, so each
  unit takes its own rows.
- **`background-work.created`** and **`background-work.ended`** carry one task of a supervisor's background
  work as `GET /background-work` answers it (「background-work」 above). The watch over the background records
  publishes `created` on the look that first reads the task running, a server started afresh reading it again,
  and `ended` once, on the look that finds it over, its `status` then the word its ending is told by: the word
  its notice ended on (`completed`, `failed`, `killed` or `stopped`), `stopped` where a stop of the turn ended
  it, `lost` where its program went without a notice, or `gone` where its program dropped it with none.
- **`turn.changed`** carries what a running turn waits for (「sessions」 above): `session`, `turnId`, `run` and
  `waiting`, null as the wait clears.
- **`road.changed`** carries one fact of a role's road (`docs/provider-order.md`, 「The facts told」),
  published once per move, after the system has acted, and named in its `fact`; it names its row as `role`,
  and `supervisor` is null, since a supervisor's road never moves:
  - `road-moved`: `from` and `to` — each a road, its `provider`, `model` and `account`, the
    account null off a provider that holds logins — and `reasons`, why each provider passed could not
    answer, each a `provider` and its `reason`;
  - `road-returned`: `from`, the road it had moved to, and `to`, the head of its walk it is back on;
  - `road-closed`: `reasons`, every provider's reason, and `command`, what a person does to reopen
    the first road a person can reopen — the sign-in command with the account's own configuration
    directory, or where a spent balance is topped up — or null where every road only waits for a reset,
    with `reopensAt`, the epoch second the first of them reopens, or null.

  Telling the owner is the consumer's: the moved road as what the system switched to, the closed road
  with its command passed over verbatim. No fact is the end of the matter, and none is said twice.
- **`supply.changed`** carries one fact of a person's run and its supply (`docs/supply.md`, 「The facts
  told」), published once, after agency has acted, named in its `fact`, with `run`, `account`, `job` or null,
  `supervisor` or null, `provider` and `model`:
  - `supply-moved`: `from` and `to`, each a source, and `reason`, why the source left could not go on;
  - `supply-closed`: `source`, the last source tried, `why`, the reason none is left as `docs/supply.md`,
    「Why none is left」, words it, `reason`, `reopensAt`, the epoch millisecond the hook named or null, and
    `sentence`, the one sentence the person is told.
- **`road-walked`** carries the road a turn on a supervisor's line walked (`docs/run-records.md`, 「The roads a
  turn walked」), published once as the turn ends: `stage`, `chatId`, `run`, `provider`, `model`, `outcome`,
  `answered` or `error`, `ms`, `inputTokens`, `outputTokens`, `cacheReadTokens` and `cacheWriteTokens`, each
  null where the road reported none, and `contextRatio`, or null. The unit that keeps the road records writes
  each one with its event number, reading on from the last number it wrote.
- **`worker.changed`** carries a job's run that answered on a lesser model than it went out on
  (`docs/cache-fallback.md`, 「Healing differs for a job」): `worker`, the checkout it ran in; `repository`, the
  repository that checkout is of, or null; `role`, the
  role it ran as; `launched`, the model it went out on; `answeredOn`, the models it answered on; `dropped`,
  whether the session its role kept there was dropped, false where the repository's heal is off; and `job`,
  the job, or null.

## Hook points

- **`typed-line`**: asked as the program's submit hook hands a person's line typed into its pane to agency
  (「turn-contexts」), its answer the round context the line goes on with, or null to leave the line as typed.
  Each ask is `{ "point": "typed-line", "ask": { "text", "sessionId" } }`, the answer read from `answer`.
  The receiver owns it: its registration stands again as agency starts, since the line's round is the
  receiver's.
- **`supply`**: asked as a person's run starts and as it spends (`docs/supply.md`, 「The supply hook」). Each ask
  is `{ "point": "supply", "ask": { "op", … } }`, the answer read from `answer`; amounts are US dollars, times
  epoch milliseconds, and `key` the idempotency key `<run>:<step>:<op>`. `run` is the run's supply key, a
  string: its kind (`job` or `turn`), its number and the moment it chose its source in epoch milliseconds, joined
  by hyphens, so no two runs share one across kinds or restarts. `account`, `step`, `job` and `room` are numbers.

  | `op` | Ask | Answer |
  |---|---|---|
  | `order` | `run`, `account`, `provider`, `model`, `kind` (`job` or `turn`), `job`, `supervisor`, `room` | `sources`: in order, each `personal` or a platform source's name |
  | `reserve` | `run`, `step`, `account`, `source`, `key` | `accepted`, `reservation`, `amountUsd`, `expiresAt`, and `reopensAt` where it refuses |
  | `extend` | `run`, `step`, `account`, `source`, `reservation`, `spentUsd`, `key` | `accepted`, `amountUsd` (the whole), `expiresAt` |
  | `capture` | `run`, `step`, `account`, `source`, `reservation`, `costUsd`, `key` | `taken` |
  | `release` | `run`, `step`, `account`, `source`, `reservation`, `key` | `taken` |

  Nobody registered: `order` answers no order, so a person's run takes personal supply alone, and nothing else
  is asked. Registered and not answering, or answering what does not read: `order`
  answers personal supply alone, `reserve` and `extend` are refused, and `capture` and `release` stay owed in
  the supply outbox until they are taken. A capture may exceed its reservation, and one whose reservation the
  consumer let go at its expiry is taken as a charge of that amount.

## Tables a consumer shares

- **`room_windows`** is the receiver's: the rooms' listening windows, and agency keeps none.
- **`run_records`** is the receiver's too: what a turn wrote down beside its run, and agency keeps
  none.
- **`runs`, `agent_log`, `agent_executions`** carry a run's ledger, its log lines and the executions
  agency recorded.

## The command

`bmo-agency` is agency's unit command (bmo-cli's `docs/api.md`, 「The unit command」). Asked `nouns`, it prints
the nouns `bmo` dispatches to it, one a line, each marked `home` after a tab where its commands act on the
operations machine. It answers with nothing of the machine read: no settings, no data root and no standard
input, so `bmo` finds it from any directory.

| Noun | Takes | Calls |
|---|---|---|
| `agent` (`agents`) | the agent flags `--help` prints: the providers' switches and the account order, the accounts and their kinds, an account's endpoint, key file, model map and base prompt, the registered models, the supervisors, roles and repositories with their orders, and a supervisor's gates | the store at the data root, and the service for what it must be told |
| `agent status` (`agent-status`) | `status`, `stop`, `failure`, `submit`, `start` or `shell`, `--place <dir>`, `--hold-tasks`, `--hold-text-tool-call`, `--hold-wording`, `--compose-typed`, `--carry-background`, `--refuse-claude` or `--guard-job`; `shell` alone needs no place and answers a shell call rewritten to close its standard input, refused where `--refuse-claude` is given and the command starts Claude Code, and where `--guard-job` is given and the job guard refuses it (`docs/live-claude-guard.md`), which reads agency's store, asks the testbed's `classify` verb and, for a write of `STRUCTURE.md`, reads the job's item from the job service; with `--guard-job` it also answers a call of the file tools that write, refused where the file lies in another worker's checkout or is `STRUCTURE.md` the job's item does not open, and let through as it came otherwise; with `--carry-background` a stop writes the background tasks it names running into the place's background record, and a submit the notice it is handed (`docs/agent-control.md`, 「Background work on a line that carries it apart」) | the agent's place, and the service for a typed line |
| `model` (`models`) | `add`, `set`, `remove`, `use` or `run-use`, with an entry's file, bytes, hash, engine, registration and settings; `--entry` and `--placed` for an installer; `--json` | the store's model catalogue |
| `session fold` | `--role <role>`, `--path <path>`, `--model <model>`, `--all` or `--dry-run` | the roles' kept sessions, and the store |
| `reload` | `--path <file>`, repeatable, and `--now` | the store, the checkouts and the deploy tree, and the service for the documents moved |
| `node work` (`work-node`) | `setup <node> [--root <dir>] [--install]`, `check`, `sync`, `runs <node> on\|off`, `remove <node>` or `teardown <node>`, the last an account's isolation node alone (`docs/account-isolation.md`, 「Taking it down」); no step lists the workplaces | the store, the node service, herdr, and for a teardown this machine's users, units and firewall |
| `node claude-token` | `<account>`, and `--paste` to take a token from standard input | the store, and the account's program on this machine |
| `node person` | none to list every account's node; `<account> <node>` to set one, with `--shared` where the node is shared with other accounts; `<account> --clear` to take it away | the store, and the node service for the node |
| `node pane` (`node-pane`) | `<node>` | the node service, and the login its pane starts |
| `dev-server` | `start`, `restart` or `stop <checkout>`; `list [--sweep]`, the checkouts' dev servers and the runs' records; `pane <checkout>` | the repository's workbench and the checkout's dev-server pane, the runs' records, and the store |
| `dev-pane` | `<checkout>` | the checkout's dev-server pane, and the store |
| `run-group` | `<run> --record <file> -- <program…>`, the launcher a run's pane starts on; not a verb a person runs | the machine's control groups and proc files, and the run's record |
| `supervisor follow` | `<room> --agent <supervisor> [--version <version>]` | the store, as the room is kept and its turns are appended |
| `supervisor reset` | `<supervisor>`, `--conversation`, `--person`, `--dry-run`, `--alone <rooms>` and `--shared <rooms>` | the service's `/supervisors/{supervisorName}/resets` route |
| `claude batch` | `--model`, `--effort`, `--config-dir`, `--system-file` and `--title`, with the batch on standard input; the run lasts until it ends | the provider's program on this machine |
| `live-claude` | `--purpose <what it measures and why>`, or `--list` | the store's run ledger |
| `job borrow` | `<repository> [--wait]`, run inside a job, which names it by `BMO_RUN_NAME`; it calls with the control token where the process holds one and with the job's session token `BMO_MCP_TOKEN` otherwise, and is refused where it holds neither; on the port `AGENCY_CONTROL_PORT` names, else the one the data root's `env/agency.env` names (「The control port」); `--wait` asks with `wait` and asks again on each `still-waiting`, printing what it waits for | the service's `POST /borrowings` route (`docs/worker-slots.md`, 「A job writes only in its own checkout」) |
| `repository` | `add <name> --path <base checkout> [--label <label>] [--umbrella]`, or `remove <name>` | the service's `POST /repositories` and `DELETE /repositories/{repositoryName}` routes, a removal refused while a job holds one of its checkouts |
| `run list` | `--json`, or `--full` for every row whole | the store, herdr, and the machine's proc files |
| `run say` | `<kind>:<id>` — `session:`, `job:`, `repository:` or `worker:` — `--words <text>`, `--file <path>` repeated, `--by <name>` | the server's words routes |
| `run stop` | `<kind>:<id>` — `unit:`, `execution:` or `item:` — `--why <reason>` for an item, `--all`, or no target to list what is running | the store, herdr and systemd |
| `machine memory` | `--runtime <runtime>` and `--unit <unit>`, each repeatable, and `--json` | the machine's proc root |

- **Every noun prints English** and ends with 0, with 1 where its work failed, and with 2 where its words
  are wrong; `--help` prints a noun's whole usage.
- **`agent status` and `agent-status` are one noun:** the two-word spelling is read as the command's own
  `agent-status`, and both reach the same hooks and shell guard.
- **`quiet` is the deployment's, not a noun `bmo` dispatches** (`docs/sessions.md`, 「A restart hands the
  session over」): it asks the running server's `GET /quiet` on the port and under the token the environment
  names, else the data root's `env/agency.env`, and ends with 0 where the server holds no work its restart
  would cut, no server listens, or the server's build answers no such route (404), with 1 and a line for each
  piece of work where it holds some, and with 2 where it could not tell: no token, another refusal, or an
  answer that does not read.

## The harness plugin

The service publishes `bmo-agency` beside its client: `bmo-agency mcp` serves the MCP server, and each
tool is a thin call of one route through the agency's own client.

- `file_item` calls `POST /item-filings`, naming the agent its server was started for (`bmo-agency mcp
  --agent <name>`); it is the chat line's one road for filing an item: the job plugin is started without its own
  item creation on every program, and no prompt names it. Its description tells the agent to name the
  repository the work is in, none for host work, and to file one change that reaches two or more repositories
  as one item, its repository the one it runs in, an umbrella where it has no home of its own, and its label
  `repositories` naming every repository it reaches.
- `stop_job` calls `POST /job-stops` the same way, naming the agent its server was started for, and is
  listed beside `file_item` only where the server was started for one.
- `review_job` calls `POST /job-reviews` the same way, and is listed beside them on the same terms. Its
  description names the four actions and that `escalate` is refused before the review policy's ceiling.
- `assign_worker` calls `POST /assignments`, naming the repository and the role.
- `release_worker` calls `PATCH /assignments/{assignmentId}` with `state` `released`, giving back a hold the
  harness will not run.
- `run_job` calls `POST /jobs`.

## The clients

`crates/client` is the Rust client (`bmo-agency-client`) and `packages/client` the TypeScript package
`@agency/client`, both generated from `docs/openapi.json`: every body as a type and a typed call for every
route. A caller uses one of them and writes no client of its own.

- **Installing the package.** `bun pm pack` in `packages/client` writes `agency-client-<version>.tgz`, and a
  downstream checkout names that file in its dependencies as a `file:` address. The pack carries the
  manifest, the entry point and the generated types; `openapi-fetch` comes with it as the package's one
  dependency.
- **Version and types.** The package carries the repository's version, and a test fails where the
  committed types differ from what `docs/openapi.json` generates.

## Calls agency makes

Agency reaches upstreams through what their repositories publish: brain, node, arbiter,
pocket, testbed, gate and chat through their clients, and cache-warmer through the schedule library
`bmo-cache-warmer-schedule`. These routes below it are called: as a job is written, and with each renewal
while it runs, the job service's lease is set to hold fixed what its launch read; as a job is stopped, the
job service's lease is given back, and while an ended job's lease stands for its item's close it is renewed; as a
job is written, its item's `GET /items/{itemId}/notes` is read for its handoff, and as a job's work moves to
another session, `POST /items/{itemId}/notes` is handed the handoff the old session left; as a job's review puts
its work to the owner, the job service's `GET /questions` by `itemId` is read for a question asking the same that
still waits, and `POST /questions` asks it only where none does (`docs/job-review.md`, 「The owner's word」); as a job's tool
call gone silent and idle is handed over, the same read by `itemId` says whether a question of the job's item still waits,
which holds the ask (`docs/agent-control.md`, 「A run's tool calls are held to a clock」); as a job ends, the
receiver's `POST /instructions` is handed the report for the room's supervisor; as a supervisor's background work
ends or is lost, that same route is handed, through the outbox, an order for the supervisor in the room the work
was launched in, under the sort `background` (`docs/agent-control.md`, 「Background work on a line that carries it
apart」); after a
chat program's first turn, that same route is handed one fact for each of the session's servers its
program did not attach (`docs/agent-control.md`, 「dsh's providers: OpenRouter and DeepSeek」); and before a supervisor's turn opens a fresh
session in place of one it had, the receiver's `POST /handoffs/{agentName}` is asked for the old session's
handoff, with `reason` and `room` where the turn names one, while agency still names the session, and
waited on (`docs/sessions.md`, 「A restart hands the session over」); and every minute the job service's `GET /queues`, `GET /items` with `state`
`open`, `GET /items` with `state` `draft` and `GET /items` with `state` `held` are read, and its events as a page, `GET /events` with `Accept:
application/json` past the cursor agency keeps, narrowed to `item.created`, `item.changed`, `note.created`,
`question.created`, `question.due` and `reply.created`, an item's title asked of the gate as the item is raised or
changed (`docs/worker-language.md`, 「What agency translates on the way in」), a note on the item of a job that runs
handed to that job (「assignments and
jobs」, 「Words and notes for a running job」); an idle agent, and every change of an item or a job that calls
for a judgement, is told to the item's orderer through the receiver's `POST /instructions`, as an order under
the sort `job` with a kind of its own, and a job failed in another room to the owner's private room the same
way, each owed in agency's outbox and sent again until the receiver takes it, a standing one told
again under its kind while its condition holds; and an instruction for a question a settling reply answered,
or for a standing notice whose condition ended, is withdrawn with `DELETE /instructions/{id}` (`docs/agent-control.md`, 「An item's change is told to its orderer」, which tables every
kind, event and code); the question a reply settled on its recommendation is read with `GET
/questions/{questionId}` for its item; the items a held item waits on are read with `GET /items/{itemId}`; a job's shell hook reads the job's own item with `GET /items/{itemId}` where the job would write `STRUCTURE.md`, at the address and with the token the job's environment names (`docs/live-claude-guard.md`, 「The same hook keeps STRUCTURE.md the owner's」); the orderer is found by reading the item and the items it was raised from with `GET
/items/{itemId}`, by their `raisedBy` field, else their `raised-by` label, up the chain; and for pull request reviews the job service's `GET /items` by the `pr` label,
`POST /items`, `POST /items/{itemId}/notes` and `PATCH /items/{itemId}`; every few seconds, for the jobs standing
in the machine's turn (`docs/role-agents.md`, 「When an agent is busy」), the job service's `GET /items` with
`label` `close-phase` and the testbed's lanes as `bmo-testbed status --json` prints them, read through the
types of the testbed's published client `bmo-testbed-client`, and, for a job whose commit waits to go live, its
repository's deployment record `deployed.json` and progress record `deploy-progress.json` under the deploy root
the testbed declaration in its checkout names; every few seconds, for a deployment that did not go live
(`docs/agent-control.md`, 「A deployment that did not go live is told」), the progress record under the deploy
root of every repository agency holds on this machine, the user manager's `systemctl --user is-active` for a
run's unit, and that unit's journal through `journalctl --user`, its last lines and the testbed's event line
for an event no hook took, and, for a run that went live, the deployment record `deployed.json` beside it; at
each flush of the deploy queue (`docs/agent-control.md`, 「The deploy queue is flushed every hour」), the job
service's `GET /items` with `label` `close-phase`, and, in the base checkout of each repository with a commit
waiting, `git ls-remote` for its remote's `main` and the testbed's verb `bmo-testbed deploy --commit <commit>
--message <why>` (the testbed's `docs/boundary.md`, 「`deploy`」), which starts the deployment in a unit of its
own and returns; and once an hour GitHub's search through
`gh api graphql`, a search for the owner's login (`user:`) (`docs/pull-request-reviews.md`). For a control call
on agent management that carries a signed-in person's headers, the web's account service is asked for that
person's console sections with `GET /users/{user}/sections` (「Who a caller acts for」 above), reached at
`WEB_ADDRESS` with `WEB_TOKEN`, or the file `WEB_TOKEN_FILE` names, else from the web unit's `env/web.env`
under the data root where the process's environment names neither. The same account
service is asked, as a record is issued for a person's account and as a resident program's turn begins, for the
declared kinds with `GET /chain/kinds` and for each app's two settings for that person with `GET
/chain/kinds/{key}/resolve?person=` (`docs/tools.md`, 「The apps a person reaches」). As its server stands, and
again where a resolution finds one undeclared, it declares the settings its prompt layers read with `PUT
/chain/kinds/{key}`, `agency.org-instructions`, `agency.persona`, `agency.personal-instructions` and
`agency.personal-instructions.text`, and as a prompt of an agent of an account is composed it asks them for that
person with `GET /chain/kinds/{key}/resolve?person=` (`docs/prompt-layers.md`, 「The settings agency declares」).
A person's own instructions are written on their own link with `PUT /chain/kinds/{key}/links/{link}/value` in their
name, naming agency in `Declared-By` (「own-agents」). Each is the
documented route of the unit that holds what must reach it; agency keeps no client of these units and no
store of their rows. A consumer registered at the `supply` hook is asked at the address it registered and never
at one agency knows of itself (「Hook points」); as a person's run moves to another supply source or finds none
left, the receiver's `POST /instructions` is handed, through the outbox, one fact for the supervisor in the
room the run answers, marked `ridesForGood` so it opens no turn, no source left carrying the fixed line
`supply-closed` as its `line` for the room to hear at once, and no source left is withdrawn with `DELETE
/instructions/{instructionId}` once a run of that supervisor there goes out again (`docs/supply.md`, 「The facts
told」).
Of the brain it asks an agent's own brain made as the agent is registered, found by the agent's key,
let go whole or only found for a preview, the discipline a chat launch's system prompt carries, and the
base layer of the agent's persona that prompt ends with; and for a supervisor's sleep (`docs/sleep.md`), the
newest summary of its sleeps (`GET /brains/{brainId}/memories` by the type `reflection` and the label `sleep`),
the consolidation state since it (`GET /brains/{brainId}/consolidation` with `since`), and the summary written
(`POST /brains/{brainId}/memories`). For the same sleep it reads the arbiter master's picture (`GET /cluster`) for a
card under a claim, and hands the receiver's `POST /lines/{agentName}` the record-only line `slept`. Of the tool host it asks, on its control socket
with the issuer token, a session token for each program and run that holds a capability, which it takes
back, the asynchronous calls still running (`GET /uses`, `state=running`) before it takes back a token a
supervisor's replaced program carried, a namespace's facts, the paragraph on finding and using tools that a chat launch's
system prompt carries, and, after a chat program's first turn, the servers its session token's clients
reached (`GET /tool-sessions/{token}`, its `connected`), where a host that saw none says nothing; and through
`bmo-pocket`'s owner verbs it puts, takes and lists an account's personal tools (「personal-apps」). Of the gate it asks
what the turn reading attaches for a job's first prompt, read over the brain of the supervisor the call names
in its `agent`, and attaches the entries it answers to that prompt; it asks `POST /translations` for the English of what a person wrote for a job's
worker, the owner's part of its preamble and the words handed to a running job, waiting for the answer no longer
than its own bound (`docs/worker-language.md`, 「What agency translates on the way in」); and it
reads the gate kinds and their order as the gate publishes them, so no copy of the gate order stands here. Of the
mail service the DeepSeek top-up asks, while DeepSeek's sign-in waits on a code or a link it mails, `GET
/accounts` for the account carrying the DeepSeek login's address, then `GET /accounts/{accountName}/messages` with
`q` naming DeepSeek's sender and `after` the moment the sign-in began, and `GET
/accounts/{accountName}/messages/{messageId}` for each message's bodies (`docs/tools.md`, 「Where a person is
asked」). It reaches the service at `BMO_MAIL_ADDRESS` with `BMO_MAIL_TOKEN`, acting for `BMO_MAIL_OWNER` with the
role `administrator`, each else from the mail unit's settings file `env/mail.toml` under the data root: its
`server.listen`, `server.token` and `accounts.administrator`.

- **Where the arbiter's master is.** `AGENCY_ARBITER_URL` names its address and `AGENCY_ARBITER_TOKEN_FILE`
  the arbiter deployment's settings file whose `CARD_MASTER_TOKEN` is its token. A run on a registration
  naming a card service asks it `POST /demands` for one card, reads `GET /demands/{demandId}` with its answer
  held open, and gives the demand back with `DELETE /demands/{demandId}`; health reads `GET /cluster`, and
  the gateway's launcher reads it on its beat for each service's standing instances (`docs/own-llm.md`, 「An
  engine the arbiter stands」). With no address, no demand is asked and a run on such a registration is
  refused.
- **The gateway's door asks the master too.** The gateway's front, for a request on a registration naming
  a card service with no instance standing, asks `POST /demands` for one card with the door as its `ref`,
  reads `GET /demands/{demandId}` with `waitMs` until it is granted, and gives it back with `DELETE
  /demands/{demandId}` once the last request waiting on it has ended; as the launcher starts it reads `GET
  /demands` and gives back every demand under the door's reference (`docs/own-llm.md`, 「The gateway」).
  The launcher reads the master's address and token as 「The gateway」 says.
- **Where the gate is.** `BMO_GATE_URL` names its address and `BMO_GATE_TOKEN_FILE` the path of the file
  its token stands in. With no address, no reference is asked and a job carries only its own
  words.
- **Where the brain is.** `AGENCY_BRAIN_URL` names its address and `AGENCY_BRAIN_TOKEN` the token it is
  called with, or `AGENCY_BRAIN_TOKEN_FILE` the file it stands in; the node's token is read the same way
  (`AGENCY_NODE_TOKEN`, `AGENCY_NODE_TOKEN_FILE`). With no address, no agent has a brain: nothing is made, counted or let go.
- **A person's PC's hub.** The node service's target of a hub node names its hub's control address, the hub's
  settings file, and the agent; for a PC's workplace and link agency asks that hub itself — `GET
  /status`, `POST /exec`, `POST /upload` and `POST /listen` — and runs the hub's
  executable `AGENCY_JAKE_HUB` names in its `connect` mode for each byte stream, each with a control credential
  scoped to that agent alone, made from the hub's `JAKE_CONTROL_KEY` and never the control token
  (`docs/person-nodes.md`, 「Who reaches a node」).
- **The account a brain is filed under.** The brain keeps one brain per agent, keyed by an account key
  and the agent's key, and never makes one by itself. A brain agency makes is of the kind `agent`, and an
  agent's brain is found by its key among the brains of that kind alone, so a unit's org brain filed under
  the same key never answers for an agent. A brain agency makes carries, as the account key,
  the id of the login account owning the agent where it names one, and otherwise the key
  `AGENCY_BRAIN_ACCOUNT` names; as the agent's key, the agent's registered name — the key the brain tool
  and the gate look a brain up by. The brain reads no meaning into the account key. With no account to
  file it under, agency makes no brain and logs that it made none.
- **The base persona.** At a chat launch the agent's brain, found by its key, is asked for its persona
  rendered of the layer named `persona` alone; a brain that holds no such layer gives none.
- **The persona's values.** A persona piece keeps placeholders: two opening braces, a key of lowercase ASCII
  letters, digits and hyphens joined by dots, and two closing braces, such as `{{owner.name}}`. The
  deployment keeps their values outside every repository, in a TOML file whose path
  `AGENCY_PERSONA_VALUES` names; a key's dots walk the file's tables to a string, so `owner.name` is the
  `name` of its `[owner]` table, and nothing else of the file is read. With no file named, a file that does
  not read, or a key it holds no string for, the placeholder stays unfilled and the brain's persona is not
  used.
- **Making an agent's brain.** As a supervisor is registered (「supervisors」), the brain is asked for the agent's key first; where it answers one, nothing is made. A brain
  that cannot be reached, or refuses, leaves the agent registered and the failure logged.
- **The tool discipline.** At a chat launch, the tool host's `GET /discipline` is read through its client
  and its text, telling the session how to find and use a hosted tool with `bmo use`, is put into the
  system prompt whole; agency writes nothing of its own about it. The control socket is the one
  `BMO_MCP_HOST_SOCKET` names, else `bmo/mcp-host.sock` under the user's runtime directory. A host that
  cannot be reached, or refuses,
  leaves the paragraph out and the launch logs one sentence saying so.

Each surface is a crate of its own repository, declared in the workspace manifest and allowed by
the declaration's `base_libraries`, which the testbed's `dependencies` check holds; every other crate of another repository, a downstream's client among them,
stays outside the boundary.

It calls no downstream but the job service's lease, item and event routes and the receiver's instruction
and owner-notice routes: the lease renewed or, for a stopped job, returned; the items, queues and events
read; the report handed over as a job ends, an orderer told of each change of its items and jobs and the
owner of what only he can act on, a chat session told of a server it lacks, and a room's supervisor told of
background work it launched there that ended or was lost. The gate and the receiver
reach it through the hook point above for everything else.

- **A test's ground reaches no peer.** A server a test stands is handed an environment built for it, with
  no deployment's peer address or token inherited from the worker that runs the suite; it names a peer
  only where the test itself names one, and it marks itself a test's. Every call in this section — the
  job service's lease, the receiver's instruction, notice and handoff, the job service's queue, item and event reads — is then not made, and the line saying so is logged. A test
  that wants a stand-in calls the call itself with a map of its own, never through a test server's
  ground.

## Not on this boundary

- **No agent rows, worker rows or tiers stand behind this boundary,** so these are no routes of it: `/agents`
  and `/agents/{agentName}` (a supervisor is `/supervisors`, a role `/roles`); `/agent-roles` (`/roles`);
  `/workers` and every route under it (`/repositories`); `/seat-requests` (`/slot-requests`); the pool's
  `GET /pool`, `GET /roles` of the pool's roles and `PATCH /pool/limits` (how many of a role's runs go at once
  is the count of its agents);
  `/tiers`, `/work-keys`, `/fold-ratios` and `/pin-checks` (an order's entries and a row's compaction
  threshold); and the route table, a supervisor's provider order and the per-key choices of `/fallback`, which
  keeps the Claude accounts alone; and `/stand-in`, since nothing stands in for a supervisor's road.
- **No warming route:** agency's settings carry no keepalive switch; a supervisor's warming is a field of its
  row.

`GET/POST /digests`, `GET/POST /searches`, `GET /searches/{searchId}` and `GET/PATCH /research-settings`,
with the tools `browse`, `remote_browser`, `research_branch`, `unread_pages`, `song` and `news_due`, are
gone: each only wrapped a call into an upstream the caller reaches directly. A digest, a research, a
browse and the morning news are ordinary sessions started with `POST /sessions` and the instructions
their caller hands in, and the session calls the upstreams it needs through their own MCP servers.

The agency's job client and its lease taking are gone; the harness takes the lease and hands the item's
content to `POST /jobs`. The hooks the job service and the brain registered here are gone; agency calls
the brain's client itself. The word-chain procedure and its mention among the agent documents are
retired.
