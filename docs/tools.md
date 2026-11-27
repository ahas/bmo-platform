# Agency's tools

The tools agency itself provides to the tool host: each a folder of its own under `tools/`, with its
`tool.toml`, which the tool host reads as it reads any other tool's. They hand agents the documents
agency keeps under `prompts/` — the supervisor's procedures — let a supervisor hand a small coding task
to a run of the worker role, put the personal app such a run made for a person, let one BMO hand a line to
another person's BMO and open a chat into that person's room, and let the
platform's supervisor ask for a DeepSeek top-up, and hand a supervisor whose program stands on a node its filing,
stopping and reviewing of work. A tool that belongs to no one service is a repository of its own, not a folder
here.

## A tool's folder

- **`tools/<name>/`** holds the tool's manifest, `tool.toml`, the schema file of each entry it
  declares, and the Rust crate of its executable, a member of agency's workspace.
- **A deployment places each tool** in its folder's `.placed/` inside the release of agency's own
  deploy tree, under `$HOME/.local/share/bmo-agency` (`docs/agent-control.md`, 「The job's release」),
  the folder the tool host reads: the executable the release's `bin/` holds, beside the manifest and
  the schema files it names. The steps that do it stand in the testbed's declaration, `testbed.toml`.
  A tool with entries is then registered with `bmo-pocket put`, whose `--allow`s name exactly the
  capabilities and grants its manifest asks for, since the tool host refuses a tool asking for one no
  `--allow` names; the manifests' tests hold each step to its manifest.
- **The registry holds no tool of agency's that no folder here declares:** a deployment takes the names
  `delegate` and `room_grant`, each where the tool host holds it registered for agency, out with `bmo-pocket
  take`, once the new server stands.
- **The manifest** says the tool's name, the protocol version it speaks, its build and check
  commands, its run command, and either its entries or the resources it serves, each with the
  capabilities a session must hold. It names no timeout, for the tool or an entry: a call is waited on
  until the executable answers. Its tests hold the manifest to what the executable
  answers.
- **The executable answers one request and ends:** the tool host hands it one JSON object on
  standard input and reads one on standard output.
  - The object carries the protocol version, its `kind` — `call`, `resources_list` or
    `resources_read` — what that kind asks (an entry's full name and its arguments, or a resource's
    `uri`), and the session record the request is made under.
  - A call is answered with a tool result: its `content`, text pieces, and `isError`. A call whose
    arguments do not fit the entry is answered as a failure saying so.
  - A listing is answered `{ "resources": [...] }`; a read `{ "contents": [...] }`, or
    `{ "error": "<why>" }` where the document cannot be read.
  - A request that does not read, or a kind the executable does not answer, ends it unclean with why
    on its error stream, which the tool host takes as a failed call.
- **`crates/tool-call`** is agency's side of that protocol, which every tool here shares: the request,
  the parts of the session record agency's tools read, agency's server as every tool reaches it (its
  port and token, read as 「The procedures」 states), the answer, and the tree they read from.
- **The session record** is the tool host's: the holder's name, the session's namespaces,
  capabilities, grants and shown entries, and the variables handed to every call's environment. The
  data root stands in those variables as `BMO_DATA_DIR`. The grants agency writes are the administrator's,
  `node.admin` and `platform.payment`, on the record of a supervisor of no account or of the administrator's
  account, and none on any other record: not another account's supervisor's, a role agent's, a seat's run's or
  a job's (`docs/person-nodes.md`, 「Who reaches a node」). The capabilities that operate the platform, `deploy`
  and `research.owner`, go on the same records alone. A supervisor whose program stands on a node is written the
  namespaces, capabilities and shown entries that carry the work of its line's servers there
  (`docs/person-nodes.md`, 「A supervisor's tools on a node」). Agency's own values stand under the issuer's
  `labels`: the line, the store's address, the agent's place where it is a resident program, and the media
  the run's model reads natively under `reads` — each of `image`, `pdf` and `video` as
  `docs/provider-order.md` 「The media a model reads」 judges them from the road the run goes out on, with no
  `reads` at all where the model reads none; an apart
  session's caller adds labels of its own beside them (`docs/api.md`, 「sessions」), which agency's tools do not read. The turn's own variables stand in `variables`, beside the
  deployment's variables the run's line names in `policy/run-lines.toml` for the placed tools that read
  them, each where the deployment sets it in agency's environment — the chat and worker lines'
  `BMO_GATE_URL` and `BMO_GATE_TOKEN_FILE`, the gate's address and the path of its token file, which the
  gate's translate tool and the job service's queue tools call it with, so no token enters the record. A tool reads only the parts it needs and passes over
  the rest.
- **The apps the record withholds** stand in the file its `withheldAppsFile` names, on the record of every session
  agency issues for a caller acting for a person's account other than the administrator's (「The apps a person
  reaches」); a record of the deployment's own, and one holding the administrator's grants, names none.
- **The platform's base is turned off** for the brain tools of a record issued for a person's account where the
  delegation chain's `brain.global` resolves false for that person (`GET /chain/kinds/brain.global/resolve?person=`;
  the brain's `docs/design.md`, 「The layers a reading takes」): the record's `BMO_BRAIN_BASE` is `off`, and the
  brain's tool then answers `brain_list` and `brain_similar` from the agent's own brain alone. It is resolved as the
  record is issued; where it resolves true, where the kind is not declared, for a record of the deployment's own
  and where the account service cannot be asked, the record names no such variable and the base is read.
- **The turn a request is made in** is read afresh from the record: the run the agent's place names
  in its `run.json` — none for a person's turn — or else the run's own variables. Its variables are
  `BMO_TURN_AGENT`, the agent the turn is; `BMO_TURN_BEGAN`, when it took its words, in epoch
  milliseconds; `BMO_TURN_CHAT`, the room it answers; and `BMO_TURN_VIA`, the road it answers on.
- **The tree a tool reads `prompts/` from** is the nearest folder above the one it runs in that holds
  `prompts/agents/`, agency's own prompt tree, which is the deploy tree the tool is placed in; else the tree
  `BMO_DEPLOY_DIR` names; else agency's deploy link, `$HOME/.local/share/bmo-agency/deploy`. A `prompts/` of
  another shape is passed by: the tool host runs a tool from its copy under the data root, whose own `prompts/`
  holds the personas, so the copy finds agency's deploy link.

## The apps a person reaches

- **An app is a tool of the tool host whose manifest names one** (the tool host's `docs/mcp-host.md`, 「Apps」), and
  each has two settings on the account service's delegation chain, `app.<key>.offered` and `app.<key>.on`, declared
  as the tool is put. An app reaches a person where both resolve true for them; a tool naming no app is the
  system's and is never withheld.
- **Agency resolves them for the person a session acts for:** the account its caller acts for (`docs/person-nodes.md`,
  「Who reaches a node」), where it is a person's account other than the administrator's. A caller of the
  deployment's own, and one holding the administrator's grants, has no app withheld, as the system's own sessions
  and the owner's supervisors have.
- **What it writes:** a JSON object, `apps`, the keys of the apps that do not reach the person, `account`, the
  account they were resolved for, and `personal`, `{network, secrets}`, whether the person's personal tools may reach
  the network and secrets — the chain's `personal-apps.network` and `personal-apps.secrets` resolved for the person,
  each false where it does not resolve (`docs/person-nodes.md`, 「The network and secrets of a call」) — written whole
  into the file the record's `withheldAppsFile` names. The tool host reads it at every listing, description and
  call, so a write holds from the session's next request.
  - A resident program's file stands in its place, `withheld-apps.json`, and so does a run's that names its place;
    a run that names none has a file of its own under the data root's `agency/withheld-apps/`, named by when its
    record lapses, and the files whose records have lapsed are taken away as another is written.
- **An MCP server reaches the record's holder alone** (`docs/person-nodes.md`, 「Attaching skills and MCP
  servers」): the file's `apps` also names the key of every MCP server of the record's account not attached to the
  record's holder — the supervisor a supervisor's record is held by, the seat a role run is let in on — and of every
  one where the record's line takes no attached MCP server or the record names no holder. The account's servers are
  those of agency's own record of them and of the tool host's listing together (`docs/person-nodes.md`, 「MCP
  servers」). The file keeps the holder, `holder` (`{kind, name}`), and the line, `line`, so a turn resolves them
  again. A record of the administrator's account, which the chain withholds nothing from, has a file written as
  well, `apps` those keys alone and `personal` both true, and names it whatever was written.
- **What cannot be known withholds** (`docs/person-nodes.md`, 「Attaching skills and MCP servers」):
  - where the tool host's listing or the holder's attachments cannot be read, `apps` names every MCP server of
    agency's record, the attached ones among them;
  - where agency's own record cannot be read, `apps` is written null, which the tool host reads as a file that does
    not read, every app withheld, until a turn resolves the file again;
  - the file's `attached`, `{"withheld": true, "why"}`, says that the attached servers were withheld and why, and
    is left out where they were not; agency's log says it once, as the file comes to say it. The tool host reads
    past it.
  - A file that cannot be written is taken away, so a record naming it reaches no app; a run's file of its own
    never stood before, so its record names one that does not stand.
- **When it resolves:**
  - as it issues a record, before the record is asked of the tool host;
  - as each turn of a resident program begins, before the turn's words go in: the place's file, where it stands, is
    resolved again for the account it names. So a setting changed while a session stands holds from its next turn,
    and a call in hand runs to its end.
- **How it resolves:** it lists the declared kinds (`GET /chain/kinds`), takes each key `app.<key>.offered`, and
  asks both of each app's settings for the person (`GET /chain/kinds/{key}/resolve?person=`), every ask of one
  resolution side by side. A setting that does not resolve to `true` withholds its app.
- **Where the account service cannot be asked,** the file is left as it stands, so a session keeps what it last
  reached; a record issued with no file yet names one that does not stand, which the tool host reads as every app
  withheld until a turn writes it.
- **The session is told when its apps change.** What its personal tools reach is written again without a word to
  it; where a turn's resolution withholds other apps than the file held before, and the turn is a chat session's in
  a room, a notice is owed in agency's outbox to the supervisor in that room, as the fact the chat line's policy
  words for it, naming the apps turned off and on, so the session does not reach for a tool it no longer has. It
  carries no instruction. A file whose `apps` is null, before the turn or after it, names no apps to tell of, so no
  notice is owed for it.


## The owner and the runner a record names

- **Every record agency issues names the account its caller acts for,** `ownerAccount` (the tool host's
  `docs/api.md`, 「The session record」): the account a person's caller acts for (`docs/person-nodes.md`, 「Who
  reaches a node」), and the administrator's account for a caller holding the administrator's grant, a supervisor of
  no account among them. A caller of the deployment's own that holds no grant names none. The tool host shows a
  session the personal tools of that account alone.
- **Beside it, the runner that account's personal tools run on,** `runner` (`docs/person-nodes.md`, 「Personal tools
  on a PC」):
  - the administrator's, on this machine: the command of this deployment's node link, `bin/node-link runner`, over
    the data root's `personal-tools/` folder, for that account alone;
  - a person's, on their PC: the runner socket of the PC's link, where the link's spec names the person as the one
    account the PC runs personal tools for;
  - none for an account on a shared PC, an account with no PC, or a PC whose link has no spec yet, so a call of a
    personal tool fails saying no runner is named.
- **And the folder that account's secrets stand in,** `secretSpace`, which a call of a personal tool naming secrets
  is handed them from (`docs/person-nodes.md`, 「The network and secrets of a call」): the console's secret store's
  space of the account, the data root's `env/users/<account>/`, and the data root's `env/` itself for the
  administrator's (bmo-web's `docs/accounts.md`, 「The secret store」). A caller whose agent row stands filed under
  an organisation unit — the row's unit, no account — is handed that unit's space, `env/units/<unit>/`, whatever
  account it otherwise acts for.
- **It is judged as the record is issued,** as the grants are, so a session issued again reads the account and its
  PC afresh.
## The procedures

- **`tools/procedures`** serves each document under `prompts/agents/chat/procedures/` as a resource,
  one per document, under the `bmo` namespace, to a session holding `room.tools`. A document's address
  is `bmo://procedure/<name>`, its name its file's without `.md`, and it is `text/markdown`.
- **A listing** names every procedure. **A read** answers one document:
  - an address that is no procedure's, or a name no document has, is refused with the names there
    are, before anything else is asked;
  - outside any room's turn, the document is read whole;
  - in a room's turn, it is read as that room's road may use it: a part marked for a verb the road
    does not perform is left out, and the marks are taken off what is left. Its trailing blank lines
    go either way.
- **The marks:** a line ending in `[CAP:<verb>,<verb>]` needs those verbs. A level-two heading marked
  so takes its whole section with it; any other marked line takes itself, the lines it wraps onto and
  every line indented under it. An unmarked line is kept, and what stands under it is held to the
  verbs line by line.
- **The verbs a road performs** are the consumer's that holds the rooms. The tool asks agency's server
  `GET /road-verbs` with the room and the road the turn names, and the server asks the consumer's
  `road-verbs` hook (`docs/api.md`, 「Hooks」). With no consumer registered the road performs no verb,
  and every marked part is left out. Where the server cannot be asked, the read is refused with why.
- **The tool reaches agency's server** on the port and with the token `AGENCY_CONTROL_PORT` and
  `AGENCY_CONTROL_TOKEN` name, the token read from the file `AGENCY_CONTROL_TOKEN_FILE` names where the
  variable is not set. A placed tool's environment holds neither, since the tool host hands it a
  clean one, so it reads them from `env/agency.env` under the record's data root, the file the server's
  unit takes its environment from; a variable its own environment holds stands first.
- **A turn a supervisor's reset overtook** — one naming an agent and a moment at or before that agent's
  reset point in agency's store (`docs/supervisors.md`, 「What the reset cuts off」) — is listed nothing and
  refused every read, saying the agent was reset after the turn began.

## The worker ask

- **`tools/worker_ask`** declares `bmo/worker_ask` to a session holding `room.tools`: a supervisor hands a
  small coding task to a run of the `worker` role, with no queue item, lease or report.
- **It carries no room check** (`docs/launch-lines.md`, 「What a room's words may make a supervisor do」): what a
  room's words may have the supervisor ask is the supervisor's rules'. A person asking
  their own BMO for an app of their own is held by the chain's `personal-apps.make` at the app's put
  (`docs/person-nodes.md`, 「Personal apps」).
- **Its arguments** are the words and an optional folder or app. The folder is the asking supervisor's scratch
  workspace under the data root where neither is named, made on first use; a repository's checkout may be named
  only in the owner's turn, and the run is then handed that repository's own (`docs/repositories.md`, 「What a
  run in a repository is handed」).
- **An app names a personal app of the person the session acts for** (`docs/person-nodes.md`, 「Personal apps」):
  its name, a lower-case letter, then lower-case letters, digits and `_`, which is the tool's and its entry's. The
  run works in that app's folder of 'my apps', `my-apps/<name>` under the data root, which the person's PC reads as
  its own, and its words are led by the procedure `make-app`, read from the tree the tool reads `prompts/` from, so
  the run writes the app as the guide says. An app and a folder together, and an app asked under a record naming no
  account, are refused.
- **It takes a slot of the worker role,** as the source `ask`, and a repository's checkout where it names one,
  and waits for both however long it takes: a busy role or checkout is waited for, never refused
  (`docs/role-slots.md`, 「The line」). The run goes out on the worker role's active provider
  (`docs/provider-order.md`), and on the session kept for the worker role at that folder on that provider
  and model where one is valid (`docs/sessions.md`, 「A role's session」).
- **It waits for the run to end,** however long the run takes: no bound of its own and no timeout on its
  wait for agency's answer. Its run hears words while its turn works, as every run's does, though the call
  that started it is held, so a supervisor says more to it from another turn or a shell, with `bmo-agency run
  say session:<id> --words …`.
- **Its answer** is the run's answer text and the paths of the files the run created or changed in the
  folder, read from the run's recorded edit diffs. The run is logged like any other, so the console shows it.
- **It refuses only what cannot be asked at all:** a folder on no machine the worker role can run on and a
  checkout named outside the owner's turn. A task worth keeping and reviewing is a
  queue item, filed with `file_item`, not asked this way.

## The app put

- **`tools/app_put`** declares `bmo/app_put` to a session holding `room.tools`: a supervisor puts the personal app a
  worker ask wrote for the person it serves, sends it back to an earlier version, or removes it
  (`docs/person-nodes.md`, 「Personal apps」).
- **The person is the record's,** `ownerAccount` (「The owner and the runner a record names」), read afresh at every
  call; a record naming none is refused, so no session puts an app for anyone but the person it acts for. Like the
  worker ask, it carries no room check.
- **Its arguments:** `name`, the app's; `action`, `put` where left out, `back` or `remove`; and for `back` an
  optional `version`, the newest kept older than the current where none is named.
- **It reaches agency's server** as every tool of agency's does and asks `POST /personal-apps/{account}/{name}/put`,
  `…/back` or `DELETE /personal-apps/{account}/{name}` (`docs/api.md`, 「personal-apps」), waiting however long the
  build on the PC takes. Its answer says the version now in place, the entries the person's sessions reach from their
  next turn, `my/<entry>`, and what the tool host said; a refusal is a failed call saying why.

## The skill put

- **`tools/skill_put`** declares `bmo/skill_put` to a session holding `room.tools`: a supervisor saves a skill of
  the person it serves, or removes one (`docs/person-nodes.md`, 「Skills at home」).
- **The person is the record's,** `ownerAccount`, read afresh at every call, and a record naming none is refused; like
  the app put, it carries no room check.
- **Its arguments:** `name`, the skill's; `action`, `put` where left out, or `remove`; and for `put`, `files`, each
  file of the skill by its path, `SKILL.md` among them, as text. The whole folder is what `files` names.
- **It reaches agency's server** and asks `PUT /personal-skills/{account}/{name}` or `DELETE
  /personal-skills/{account}/{name}` (`docs/api.md`, 「personal-skills」). Its answer says the version now saved and
  that the skill reaches only the agents it is attached to; a refusal is a failed call saying why.

## The job tools on a node

- **`tools/jobs`** declares `agency/file_item`, `agency/stop_job` and `agency/review_job` to a session holding
  `jobs.file`, which agency writes into the record of a supervisor whose program stands on a node alone
  (`docs/person-nodes.md`, 「A supervisor's tools on a node」). Each is the tool of the same name agency's own server
  serves a supervisor at home, with the same description and arguments, under the namespace that server stands
  under, so the program calls it by the same name.
- **The agent is the record's holder,** the supervisor agency issued the record for, read afresh at every call: a
  filing, a stop and a review move are asked for it as its own server asks them for the agent it was started for,
  and no argument names another. A record held by nobody is refused before agency is asked.
- **It reaches agency's server** as every tool of agency's does, at home, and asks `POST /item-filings`, `POST
  /job-stops` or `POST /job-reviews` (`docs/api.md`, 「assignments and jobs」). So the turn's room and whether the
  owner asked are read off the supervisor's place here, as for a filing from home. Its answer is the route's answer;
  a refusal is a failed call carrying the route's status, code and sentence.

## The relay

- **`tools/relay`** declares `bmo/relay` and `bmo/chat` to a session holding `room.tools`: one BMO hands a
  line to another person's BMO, and opens a chat into that person's room (`docs/relays.md`).
- **The sender is the calling turn's agent,** read from the record afresh; a call standing in no agent's turn
  is refused, so a person or a program outside a BMO's turn speaks in no BMO's name.
- **`bmo/relay`** takes the other BMO's name, the line, and the room the line is said in, the turn's own room
  where none is named; an optional `person` names the receiving person's account. **`bmo/chat`** takes the
  same and opens the chat at once, its room the person's room and named. Each answers the record it wrote and,
  for a chat, the receiving BMO's answer; a refusal or a failure is a failed call that names the receiving BMO.
- **It reaches agency's control port as every tool of agency's does,** the address and token from its own
  environment or the server's env file under the data root, and it reaches no other unit. A call waits for the
  receiving BMO's turn to end, however long it takes: no timeout of the tool's own cuts it.

## The DeepSeek top-up

- **`tools/deepseek_top_up`** declares `bmo/deepseek_top_up` to a session holding `room.tools`: the supervisor
  tops the DeepSeek balance up by the amount the owner names, paying with the platform's card on DeepSeek's own
  web platform, in the owner's own Firefox on this machine, on his own profile and its sign-in, out of his sight;
  the platform's DeepSeek login signs in only where DeepSeek asks for a sign-in. Its words, the kinds and fields of its two
  secrets, its caps, its waits and the words it finds the platform's controls by stand in the `top_up` table of
  `policy/providers/deepseek.toml`. Which secrets hold the card and the login are agency's settings
  `deepseekCardSecret` and `deepseekLoginSecret` (`docs/api.md`, 「settings」), the owner's to change with no
  deployment.
- **A call runs only under the administrator's grant,** `platform.payment` (「A tool's folder」 above): the card and
  the DeepSeek account are the platform's own. The tool host refuses any other caller before the tool runs.
- **Nothing but a call tops the balance up.** No clock, reading or closed road calls it, and its description
  tells the supervisor to call it only when the owner asks for a top-up. A call pays at most once: no step of it
  submits a payment a second time, whatever the platform answers.
- **Its one argument is `amount`,** a number above zero in the currency DeepSeek keeps the balance in. A call
  naming none, or naming anything else, is refused. Neither the card nor the login is ever taken from a call.

### Its secrets

- **Both are secrets of the console's secret store** (bmo-web's `docs/accounts.md`, 「The secret store」), files
  of the platform administrator's space, the data root's `env/`, each one `KEY=VALUE` line to a field, written as
  typed:
  - the card, in the console's kind `card`, with the fields `CARD_NUMBER`, `CARD_EXPIRY`, `CARD_CVC` and
    `CARD_HOLDER_NAME`, in the file `deepseekCardSecret` names;
  - the login, in the console's kind `account-login`, with the fields `USERNAME`, the address or number the
    DeepSeek account signs in with, and `PASSWORD`, in the file `deepseekLoginSecret` names.
- **The tool asks agency's `GET /settings` for both names at each call,** so a change to a setting holds from
  the next call. A setting naming none is refused with the setting to fill and the kind its secret is saved in.
  A secret standing nowhere is refused with the name and the kind to ask for
  (`prompts/agents/chat/procedures/secrets.md`); a secret lacking a field, or holding it empty, is refused with
  the missing keys.
- **No secret value leaves the tool.** A value is read into the tool's memory and typed into the platform's
  page, and nowhere else: no answer, refusal or line on its error stream carries one, no screenshot is taken,
  and the owner's profile keeps no card, password or login of what was typed: Firefox's offers to save one are
  never accepted, and the session it would keep form fields in is the owner's own, given back as it stood. A refusal of a step that handles a value
  is said by a fixed sentence, never by the browser's or the page's own words, and a call that does not read is
  refused by a fixed sentence, never by the reader's own words.

### The checks before anything is paid

- **In this order, each refusing the call before the next is asked:** the amount; the card's setting and secret;
  the login's setting and secret; the balance; the cap. A refused call starts no browser and touches no profile.
- **One call is held to a cap,** the one the policy's `top_up.cap` holds for the currency the balance is kept
  in. An amount over it is refused with the cap and its reason, and a larger top-up is a person's own. The currency
  is the balance reader's (`docs/usage-windows.md`, 「Balances」), read from `GET /providers/deepseek` on agency's
  control port, reached as every tool of agency's reaches it. Until the balance has been read the currency is
  unknown, and the call is refused. A currency the policy holds no cap for is also refused.
- **Only a balance kept in a currency the card road serves is paid by card.** DeepSeek's platform takes a card
  for a balance kept in US dollars; a balance kept in yuan is paid only by Alipay, WeChat Pay or a bank's own
  app, which a person scans, so a call for it pays nothing and says so.

### How it pays

- **DeepSeek has no payment API:** its top-up is made only on its web platform by a signed-in account, behind a
  bot check that meets a browser it does not know, or one marked as automated, with a human check or a check of
  the account by mail. So the tool pays in the owner's own Firefox, the browser he signs in to DeepSeek with and
  DeepSeek knows as his device: the default profile of the Firefox the policy's `top_up.platform` names, as its
  `profiles.ini` in the policy's profiles folder names it, used in place, never copied, so its cookies, its device
  mark and its sign-in are his. No other profile is ever used.
- **Nothing reaches the owner's screen, keyboard or mouse.** Firefox is started out of his sight:
  - on a virtual screen of its own (Xvfb, through `xvfb-run`), its authority file in a private folder the call
    makes in the profiles folder, where a confined Firefox reads it;
  - in an environment of its own: told away from the owner's Wayland session onto that screen, with a runtime
    folder of the call's own and a session bus address that leads nowhere, so no window, notification or portal
    dialog of it reaches his desktop;
  - with `--no-remote`, so it neither hands itself to a Firefox he opens nor takes one of his over;
  - for the snap's Firefox, in a user scope named as the snap's launcher expects, made first through the user's
    service manager, since the launcher otherwise asks the session bus for one; the bus is cut only inside it.

  It is driven over WebDriver BiDi on a loopback port it chooses itself and announces on its own output, which
  the tool reads, so the endpoint is the call's alone. Every press and key is dispatched inside the browser, into
  the page or the payment processor's frame it is meant for, never through the screen. Every page is told, before
  its own scripts run, that the browser is not automated, as a person's Firefox tells it.
- **The call's Firefox starts as an ordinary start of the owner's own,** with a few preferences laid over his
  for that start alone, in the profile's user file:
  - the recommended preferences Firefox's remote agent would otherwise lay over the default branch, off. While
    they stand, Firefox leaves out of what it writes as it ends every setting of the owner's that equals one of
    them, and they turn telemetry upload off and point Sync and telemetry at servers that do not exist, which
    resets his telemetry identity and queues a request to delete his telemetry data;
  - no form entry, login or card kept from what the call types;
  - its window, unfocused on a screen nobody looks at, behaving as focused, as a payment processor's fields
    expect of a person's window.

  Sync and telemetry run as they do whenever he starts Firefox, and the top-up page the call opens stands in his
  history as a visit of his own would.
- **The owner's profile is held for the call and handed back as it stood.**
  - A call pays nothing while the owner's Firefox has the profile open, found by the record lock Firefox holds on
    the profile's `.parentlock`, and says so: Firefox opens a profile once at a time. A call made while another
    call drives the profile is refused the same way, by a whole-file lock on the same file, which never meets
    Firefox's own.
  - The owner's saved session, `sessionstore.jsonlz4` and `sessionstore-backups/`, is set aside in the call's
    private folder before Firefox starts, so his tabs neither load out of sight nor are overwritten. His settings,
    `prefs.js` and `user.js`, are copied there too, and the call's preferences are added to the user file. Once
    the call's Firefox has ended, whatever it left in their place, and the note of its endpoint Firefox keeps in
    the profile while it listens, are removed and his session and settings are put back as they stood, a user
    file he did not keep removed, under Firefox's own lock so no Firefox opens the profile meanwhile. Where a
    Firefox has the profile open by then, they stay in the call's folder, which is kept and named on the tool's
    error stream, rather than being written under a running Firefox.
  - The call's tab is opened and closed by the call, and Firefox is closed through its endpoint, so what DeepSeek
    stored in the profile stays for the next call; whatever has not ended is then ended with everything it
    started.
  - Opening Firefox at the desktop while a call runs, for the minute or three a payment takes, shows Firefox's
    own notice that it is already running, and the call is not disturbed by it.
- **The steps, each waited for within the policy's step wait:**
  1. Open the platform's top-up page. The owner's sign-in usually stands; where the page sends the browser to
     sign in, sign in with the login: the identity and the password typed into the fields the policy's words
     name, then the sign-in pressed. A sign-in DeepSeek refuses ends the call, naming the login's secret to save
     again.
  2. On the top-up page, choose a custom amount and type the amount; choose the card, the way to pay that shows
     the card networks' marks the policy's words name, which is the platform's own card processor; go on to the
     checkout where the page asks for it. The way to pay labelled for a debit or credit card is PayPal's guest
     checkout, which asks for a mail address, a billing address and a phone behind a reCAPTCHA, and is never
     chosen.
  3. Type the card's fields into the fields the payment processor shows, found in any frame by their
     autocomplete purpose or the policy's words for each; the holder's name only where a field asks for it.
  4. Press pay, once.
  5. Watch the page for the outcome: the policy's words for a payment made, or for one refused.
- **The card is charged what the page totals:** the amount and the tax DeepSeek adds to it. The cap holds the
  amount the balance gains.

### Where a person is asked

- **The sign-in's check is approved from the platform's own mailbox.** Where DeepSeek holds the sign-in for a
  check of the account it mails, the tool approves it itself from the mail service, bmo-mail, as the API
  document's 「Calls agency makes」 says it is reached:
  1. Where the check offers to send a code, the tool asks for it, once.
  2. It reads the mail account carrying the DeepSeek login's address for mail from DeepSeek's sender that
     reached it after the sign-in began, within the policy's mail wait, looking again at the policy's mail
     pace.
  3. A mail's bodies are read by the browser's own HTML reader. A mail whose text holds one run of digits of the
     policy's code length gives the code, which the tool types into the check's code field before pressing its
     confirmation. A mail with a link on one of DeepSeek's hosts whose address or words name an approval gives
     the link, which the tool opens in a background tab of the same profile, then signs in again, once.
  4. Where no such mail comes within the wait, or no mail account carries the login's address, or the mail
     service cannot be asked, the call ends before any card is entered, as a person's step below.
  - No code or link leaves the tool: none is answered, written to its error stream or kept.
  - The tool needs no capability or grant of the tool host for it: it reaches the mail service directly, with
    the mail unit's own token.
- **The tool does nothing a person must do, and stops where the platform asks for one:**
  - **a human check, or a sign-in check no mail approved.** Where DeepSeek shows a human check, or holds the
    sign-in for a check the mailbox did not approve, the call ends before any card is entered, saying so. The
    owner signs in to DeepSeek in his own Firefox at the machine's desktop, which the console's remote desktop
    reaches, and closes it; the sign-in then stands in his profile for later calls.
  - **the card issuer's confirmation.** Where the issuer asks the cardholder to confirm the payment (3-D Secure:
    an approval in the card's app, or a code it sends), the tool stops there, touches nothing more, and waits
    within the policy's confirmation wait for the page to show the outcome. The issuer's own prompt reaches the
    person; the answer says what the person does. A payment confirmed within the wait is answered as paid; one
    not confirmed within it is answered as not paid, with the sentence the person acts on, and the browser
    closing ends the payment's session.
  - **an identity check or any request the policy's words do not know.** The call ends, saying what the page
    showed by the policy's own sentence, never the page's words.

### What it answers

- **Paid:** the amount paid and the currency, and the balance after: the tool asks agency again until the
  balance reader has read the balance since the payment, within the policy's balance wait, and answers that
  reading with its moment. Where the reader has not read it again within the wait, it answers the last reading
  and says it was read before the payment.
- **Not paid:** a refused payment, a confirmation not given within the wait, a sign-in that did not go through
  or a page the tool could not read each answer zero paid, the sentence a person acts on, and the balance as the
  reader last read it. None moves the balance, and none is tried again on its own: a person checks DeepSeek's
  billing page before asking for another top-up.
- **A browser that cannot be started** answers that nothing was paid, with why it could not start; a profile
  the owner's Firefox has open, or another call holds, answers that nothing was paid and when to call again.
