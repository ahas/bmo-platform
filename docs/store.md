# The store

Agency keeps its rows in one PostgreSQL database, the store. The `store` crate opens it, binds and runs
statements on it, reads the model registry out of it, makes its tables and gives a fresh store its
seeds. The other crates and the server reach the store only through it.

## Its tables

- **Made by the store's migrations, which start at the 1.0.0 baseline:** one migration makes every table,
  column, key, index and sequence the store holds, in the shape and the column order the store keeps them,
  and a later change to the shape is a migration of its own after it. A slot request keeps the number of its
  owner's last ask, drawn from `role_slot_asks`, a sequence of the store's own (`docs/role-slots.md`, 「The
  line」).
- **The record of what ran is `agency_migrations`,** a table of the store's own, so the store may stand
  in a database beside another service's tables without either's record in the other's way.
- **Every process that brings the store to its shape takes one step,** which first reads the record and
  then runs the migrations: the server's settling, and a test's template alike.
- **An install from the last release before 1.0.0 is taken as it stands:** a store whose record names every
  migration of that release, and nothing else, holds the baseline's shape already, so the step rewrites its
  record to name the baseline alone, in one transaction, and touches no table and no row. A store whose
  record names only part of them is from an older release and is refused, the refusal naming the last
  migration it holds: that release's last migration runs on it first. A store with no record, or an empty
  one, is given the baseline whole.
- **A migration is not undone:** a store that must go back is dropped and made again.
- **The store names its own tables,** the ones its migration makes, so what reads every table of the
  store — a room forgotten among them — reads them and no other service's beside them in one database.
- **The tables, by what they keep:**
  - the supervisors: `supervisors`, one row a supervisor — its name, label, owner account, the words that
    call it, its prompt and brain folders, its session scope and turn shape, whether the deployment seeds it,
    its road — `provider`, `model`, `effort`, `thinking`, `answer_tokens` and `advisor` —, its warming per provider as a
    JSON object, its compaction threshold, its card rank, the repository its items go to, and when it was made and last
    changed in epoch milliseconds (`docs/supervisors.md`, 「What a supervisor row holds」); a room forgotten
    leaves every row of it, since a supervisor is its account's and never a room's;
  - the roles: `roles`, one row a role — its name, its label, its prompt or none, its description or none,
    the bundles it gives as a JSON list, the skills its runs carry as a JSON list or none, its line, what its declaration gave the agents it was seeded with —
    a road list, a count, a sessions setting, a compaction threshold and a callability, which nothing reads
    once its agents stand — whether its agents follow the supervisor that asks a run of them, the place a screen
    lists it at, and when it was made and last changed in epoch milliseconds
    — and `role_seeded`, the name of every role the policy's seed has given (`docs/agent-roles.md`, 「What a
    role row holds」);
  - the repositories: `repositories`, one row a repository — its name, label, owner account or owner unit,
    path, node,
    support count, null for `auto`, prefix documents, preamble, rules folder, dev command, pinned provider
    and model or none, whether it is an umbrella, and the sampling a run on a pinned registration is sent (`docs/repositories.md`,
    「What a repository row holds」); `repository_node_checkouts`, its checkouts on the work nodes; and
    `support_checkouts`, the support checkouts a repository set to `auto` has made, each its repository,
    slot and when it was recorded (`docs/worker-slots.md`, 「Growing a checkout at a launch」);
  - what ran: `agent_log`, `agent_executions`, `execution_attempts`, `branch_agents` and `session_resets`,
    each naming the supervisor or the role and path a line, an execution or a reset is of;
  - the sessions roles keep: `role_sessions`, one row a session a role's run opened — the role, the path,
    the provider, model, effort and login, the command's key, the session, when it opened and last
    answered in epoch milliseconds, how many runs it carried, the tokens its last answered turn carried and
    the share of the window they filled, and whether a run holds it now — taken by a run as it starts and
    written again once it has answered, and read for the role's session history (`docs/sessions.md`, 「A
    role's session」);
  - the base prompts: `base_prompts`, `agent_base_prompts`, and `base_prompt_seeded`, the name of
    every seed the store has been given (`docs/base-prompts.md`, 「The seeds」);
  - the supervisor's own documents the owner keeps a text of: `chat_documents`, one row a document's
    name and its text, with when and by whom it was last saved (`docs/agent-documents.md`,
    「A supervisor of the window shape」);
  - the accounts: `claude_accounts`, one row an account of the Claude Code road — its id, name,
    place, on/off, when it was added and when its login was found gone; its `credential_kind`,
    `directory`, `token`, `endpoint` or `api-key`, and its `credential_path`, the configuration directory, the
    token file or the endpoint or `api-key` account's own configuration home; for an endpoint account its
    `endpoint_url`, its `key_file` and its `endpoint_models`, a JSON object from the family words
    `opus`, `sonnet` and `haiku` to the endpoint's model ids, empty on the other kinds; for an `api-key`
    account its `key_file`, the secret's path under the data root's `env/`; its `user_id`, the person it belongs
    to, or its `unit_id`, the organisation unit it belongs to, or neither for the platform's; its `node`, the
    node a person's login was signed in on and stands on, or none; whether it is `lendable` to another person's
    run; and its
    `base_prompt`, the name of a kept base prompt or none, which a kept prompt cannot be removed
    from under (`docs/cache-fallback.md`, 「The Claude Code road is a list of accounts」,
    `docs/base-prompts.md`, `docs/supply.md`);
  - what people bring and what they spend (`docs/supply.md`): `personal_supply`, one row a person's or an
    organisation unit's own key for a gateway road — the person or the unit, one of them, the road, the secret's name, its place, on/off, when it was added and when
    it was found refused; `gateway_call_keys`, one row an account's call key — the account, the key's SHA-256
    digest, when it was issued and when it was revoked; `gateway_requests`, one row a request the gateway
    carried — when, the account or none, the model, the run name and the program's session or none, its input
    and output tokens and its cost in millionths of a US dollar; `supply_reservations`, one row a reservation of a run on a platform source
    or on organisation supply — the run, the step it was opened at, the account, the source, the unit whose
    supply it is for organisation supply, the consumer's reservation, its amount and the amount it was opened at, what it has covered,
    when it expires, the step last used on it and its state, `open`, `captured` or `released`; and
    `supply_settlements`, the supply outbox, one row an ask owed to the supply hook keyed by its idempotency
    key — the ask as JSON, when it was owed, when it is due next, how many tries it took and the last one's
    failure;
  - the accounts' usage windows: `usage_windows`, one row a window of a slot — its `mode`, its
    `account` (empty on a provider that holds no logins of agency's) and its `name` together naming it,
    the fraction used, when it resets in epoch seconds or none, when it was seen, its length in minutes
    or none and its limit's name or none (`docs/usage-windows.md`); and `usage_balances`, one row a
    slot's balance — its `mode` and `account` naming it, its `amount`, its `currency`, when it was
    seen, and the request allowance read with it, used, ceiling, left and what it covers, each none where its
    provider states none (`docs/usage-windows.md`, 「Balances」);
  - the roles' agents: `role_agents`, one row a seat of a role — its name, its role, the account whose seat it
    is or none, the provider and model
    its runs go out on or none, the effort, thinking, answer ceiling and advisor there, whether it is switched on,
    whether its kept session is kept warm, whether its runs keep their sessions, its compaction threshold or none,
    whether it follows the supervisor that asks its run or none for its role's setting, and the prompt,
    bundles and skills it lays over its role's, each none where the role's own stands
    (`docs/role-agents.md`);
  - the agents' attachments: `agent_attachments`, one row an item attached to a holder — the account, the holder's
    kind, `supervisor` or `agent`, its name, the item's kind, `skill` or `mcp`, its name, and when it was attached
    in epoch milliseconds (`docs/person-nodes.md`, 「Attaching skills and MCP servers」);
  - the accounts' MCP servers: `mcp_servers`, one row a server agency put at home for an account — the account, the
    server's name, the key its home copy stands under, and when the row was written in epoch milliseconds
    (`docs/person-nodes.md`, 「MCP servers」);
  - the accounts' nodes: `person_nodes`, one row an account — its id, the node its runs stand on, whether that
    node is shared with other accounts, and when it was set (`docs/person-nodes.md`, 「Which node is a person's」,
    「Several accounts on one node」);
  - the role slots: `role_slot_requests`, one row a slot request with its lease — the account it belongs to or
    none, the one agent it asks to be
    let in on, `agent_wanted`, where its asker named one, its title and the supervisor that asked it where its
    asker named them, the agent it was let in on, its `seat`, what it is
    bound to, the role session among it, whether the line has seen its bound run standing, and who gave it
    back —;
    `role_slot_reservations`;
    and `place_writes`, the standing branch checkouts' list (`docs/role-slots.md`);
  - the roles' provider prompts: `role_provider_prompts`, one row a role and a provider — the text and when it
    was set — and `role_provider_prompt_seeded`, the role and provider of each one the policy gave
    (`docs/agent-roles.md`, 「A role's provider prompts」);
  - the models and their roads: `model_catalogue`, `model_uses`, `registered_models` — a registration's
    card service, its instances' role files, its languages, a JSON list or none, its owner's `account` or none,
    and whether it is `enabled` among its columns —, `registered_model_addresses` — an address reached through
    its owner's PC keeping the `reach_port` of this machine's loopback it is asked at, unique among them
    (`docs/own-llm.md`, 「A person's local AI」) —, `card_demands`, one row a demand agency asked of the arbiter's master and
    has not yet given back — its id, the service, the registration, its holder, the slot's lease and the
    demand it was asked within where they stand, and when it was asked (`docs/own-llm.md`, 「An engine the
    arbiter stands」) —, and `model_fallbacks`;
  - the work nodes: `node_workplaces`, `node_rotation`, `node_assignments`; the nodes themselves, how
    each is reached among it, are the node service's and read from it (`docs/api.md`, 「The node
    service」);
  - the gate kinds the gate declared: `gate_kinds`, one row a kind's place in the order — its key, the
    part of a turn and the step there, the condition it runs under, and what the gate says of its probe,
    switch and answer — written whole by the declaration `docs/api.md`, 「Gate kinds」 states;
  - the live Claude guard: `live_claude_openings`, `live_claude_runs`; an opening is kept under the run
    it was opened for, `run`, and carries its job and item only where a caller names them, since a
    job's rows are the job unit's (`docs/live-claude-guard.md`);
  - the moves of the cache prefix: `prefix_moves`;
  - the work a harness hands over: `assignments`, one row an item held for a repository — the item, the
    holder, the repository, the role it runs as and its state, and for a loan when its give-back began
    (`docs/worker-slots.md`, 「Loans across a job's review」) — and `jobs`, one row the item's run — the
    item and lease the harness named, the assignment, the repository, the role, the agent its item named where
    it named one, the other repositories its item named to be lent, the checkout, the session and run it opened, the model its turn went out on, every road its turns went out on and why each was left, the
    reason it waits where no road can answer, its state, outcome and report, where its review stands — its
    stages, the stage, round and check under way, its rejections, the owner's question its work waits on and
    whether it gave its checkout up meanwhile, and the words its due check is told with and when it came due
    (`docs/job-review.md`) —, when the wait it has not yet told its orderer of began, whether the notice of its
    end is owed yet, whether its ended job's
    lease is kept standing for the item's close, what its turn spent (its
    input, output, cache-read and cache-write tokens) and their sum, its cost in millionths of a US dollar and
    the supply source it last went out on, the prompt it was launched with and the
    newest of its item's notes that prompt carried, and the words of the turn it carries until that turn ends, which a server that starts launches a job whose turn had not gone out again from (`docs/sessions.md`, 「A restart hands the session over」);
    `job_sessions`, one row a session a turn of a job started, with the job and when; `job_words`, one row a word
    for a job that runs — said to it or a note added to its item — with who said it, the note it came from, when
    it was kept and when a turn of the job took it (`docs/agent-control.md`, 「Words for a running job」); and what a session carries: `session_work`, its item and results, and `session_loads`,
    the documents loaded into it (`docs/api.md`, 「Assignments and jobs」);
  - the run ledger: `runs`, one row a run — its supervisor or its role and path, its title, its room, its unit, how deep
    it runs, its bound, when it started and finished in epoch milliseconds, how it ended, who started it, the
    tokens its context held, what it spent (its input, output, cache-read and cache-write tokens, and its
    cost in millionths of a US dollar) and the supply source it went out on, the
    search it carried, the round it belongs to, the message its answer went out as and whether agency carries its turn — opened and closed by the job unit, and a job turn's, a chat turn's and a role session's turn's by agency (`docs/api.md`,
    「runs」, `docs/run-records.md`, 「The run ledger」); a room forgotten takes its runs with it, and a
    supervisor's reset of its conversation takes the supervisor's and those of the rooms it answers alone
    (`docs/supervisors.md`, 「Starting it afresh」);
  - the clocks runs are held to and the asks past them: `run_clocks`, one row a clocked run — its name,
    the name it runs under, its role or supervisor, what it is doing, when it started, its clock and when
    it ended — and `clock_asks`, one row an ask — the run, the facts it carried, when it was asked, the
    verdict, until when a keep holds, who gave it, why and when it closed
    (`docs/agent-control.md`, 「A clock asks, it never cuts」); a row ended a week ago goes as the next
    clock is opened;
  - the events the stream carries: `stream_events`, one row an event — its number, its kind, when it was
    published in epoch milliseconds and its data as JSON — the newest 50,000 kept, its numbering kept in
    two settings rows (below; `docs/api.md`, 「events」);
  - the life of a turn: `turn_liveness`, one row a turn put into a program's pane, keyed by the turn's
    execution — its run, line and session, its phase, the pane, the program's pid and birth, the record
    files it reads, herdr's last reading of the pane and when, the record's length and when it last grew,
    when the place last took a status-line call and a hook event, when it opened and was last written,
    and its ending, when it ended and the reading the ending stood on — opened as the turn opens, written
    as its wait looks and closed with its ending (`docs/agent-control.md`, 「Telling that a turn's program
    is gone」); a row ended a week ago goes as a turn's next row is opened;
  - the supervisors' room sessions and settings: `room_sessions`, `room_session_turns`,
    `supervisor_settings` (`docs/supervisors.md`, 「Its settings」), and `supervisor_sleep`, one row a supervisor's
    sleep — whether it sleeps, its own hour or none, the day it last slept for and when it last tried
    (`docs/sleep.md`, 「Its setting」);
    the window a room holds open for an agent is the room's owner's, and agency keeps none;
  - the lines one BMO handed another: `relays`, one row a line — the sending and receiving BMOs, the room,
    the line, the kind, `relay` or `chat`, the state it came to, the answer or the reason, and when it was
    sent in epoch milliseconds (`docs/relays.md`, 「The record」);
  - the notices owed to an orderer or the owner: `notices`, agency's outbox, one row a notice keyed by its kind
    — the receiver's route it goes by (an instruction or a withdrawal), the body it is told
    with as JSON, how it is kept once told (let go, kept or standing), the mark it was owed for, the
    receiver's id of the copy told last, when it was owed, when it was last told, when it is due next, how
    many tries it took and the last one's failure — written as a notice is owed and sent until the receiver
    takes it (`docs/agent-control.md`, 「An item's change is told to its orderer」);
  - the settings, below.
- **The run records are the receiver's,** not this store's: what a turn wrote down beside its run is
  kept by the receiver, beside the run it was written under (`docs/api.md`, 「Tables a consumer
  shares」).
- **An account's `user_id`** names the user it belongs to, or none for the platform's; agency reads it and
  never fills it in itself, since who the users are is the caller's.
- **A supervisor's and a repository's `owner_account`** names the id of the user it belongs to, the same id an account's
  `user_id` names, or none for the administrator's; agency writes the one its registration is handed and
  never chooses one itself.
- **A supervisor's `prompts`** names the folder under `prompts/agents/` that holds what that supervisor alone
  is given, one plain name, or none for one given no folder of its own, as a supervisor added on the console
  starts; a repository's names the folder its rules are read from. A pieces-shaped turn's persona is read
  from this folder, never from the agent's name (`docs/harness-machine.md`, 「A supervisor's system
  prompt」). Whoever makes the row writes it, a seed among them; a row written without it names none.

## The settings table

- **One row per key, a text value each,** for the values agency keeps that need no table of their own.
- **The families it holds,** a family being a key or a key with a suffix after its colon:
  - a run's cancellation, `execution_cancel:*`, and the owner's switch on each provider,
    `provider_switch:*` by the provider, which the agent host reads again before it starts a child
    (`docs/agent-control.md`, 「The host」);
  - an agent's sampling, `agent_sampling:*`, and its own gate switches, `agent_gate:*`, which agency keeps
    and a turn's consumer reads;
  - the Claude account chosen to run on and the one in use, `claude_account_chosen` and
    `claude_account_in_use`;
  - a supervisor's session record, `agent_session:*`;
  - the replacements supervisors' sessions owe, kept until they are made: `handoff_owed:*` by the agent, why
    its one session across rooms owes one, and `room_handoff_owed:*` by the agent and the room, as JSON, the
    handover a room's own session owes, why, and what its handoff turn goes out on (`docs/sessions.md`,
    「Session replacement」);
  - when a supervisor's session was last asked for its handoff, `handoff_asked:*` by the agent and the room,
    which paces the asks to one a minute (`docs/sessions.md`, 「A restart hands the session over」);
  - a supervisor's session opened without the handoff it was owed, `handoff_lost:*` by the session's id, kept
    with the moment, until a turn begun after it has answered, and read back on its state as `handoffLost`;
  - a turn in flight, `turn_in_flight:*` by its run, as JSON: the session, the turn's id, the caller's
    key, the agent, the room, the turn's run in the ledger, the job and when it began, kept while the
    turn goes and read by the next server's start (`docs/sessions.md`, 「A restart hands the session over」);
  - the node a turn in flight waits on, `turn_node_wait:*` by its run, the node's name, kept while the turn
    waits for a person's node to be heard and let go with the turn, read by the deployment's quiet and the next
    server's start (`docs/person-nodes.md`, 「Which node is a person's」);
  - a keyed turn's end, `turn_end:*` by the session and the caller's key, as JSON: the turn's id, how it
    ended, its answer and when it ended, kept for the span `policy/turn-keys.toml` names so a send repeated
    with the key is answered with it (`docs/api.md`, 「sessions」), and dropped past it by the
    reconciliation's pass;
  - the event stream's numbering: `event_numbers_reserved`, the number every number a process gave
    stays below, written ahead of the numbers a process gives, and `events_dropped_through`, the
    highest number the store has dropped (`docs/api.md`, 「events」);
  - the supervisor a run of the search or browse line writes for, `research_supervisor` (`docs/api.md`, 「settings」);
  - the console secrets the DeepSeek top-up reads its card from and signs in with, `deepseek_card_secret` and
    `deepseek_login_secret` (`docs/api.md`, 「settings」);
  - a hook registration that outlives a restart, `hook_kept:*` by its point, as JSON: its address and the name of
    the token file under the data root's `env/`, never the token (`docs/api.md`, 「hooks」);
  - a person's supply notice told once, `supply_told:*` by the key or login and the moment it was found refused
    (`docs/supply.md`, 「Running out mid-turn」);
  - a supervisor's reset point, `reset_point:*` (`docs/supervisors.md`, 「What the reset cuts off」);
  - the machine watch's notes: `doctor_standing_failure`, `herdr_leftovers_told`;
  - the logins watch's notes: `claude_node_token_gone:*` and its notice keys;
  - a job's road and its stop: `job_route_block:*`, `job_stop:*`;
  - the queue's notices (`docs/agent-control.md`, 「An idle agent is told to the orderers of the items it could
    take」 and 「An item's change is told to its orderer」): the moment an agent was first seen idle,
    `queue_agent_idle:*` by the agent; the label values last published to the job service, `label_values:*`
    by the label (「A label agency does not hold is refused as it is set」); the marks of a notice owed once, by
    the item or the job it tells of, `queue_paused_told:*`, `queue_unlaunchable_told:*`, `queue_agent_told:*`
    (the item's refused `agent` label), `queue_role_told:*` (the item's unknown `role` label),
    `queue_no_candidate_told:*` (the role and model told as having no candidate), `queue_draft_told:*`,
    `queue_wait_told:*`, `job_moved_told:*` and `job_close_mark:*`; the job service's event cursor,
    `job_events_after`, past which no event's notice is left unwritten; and the deployment watch's
    `deploy_watch`, as JSON, the unit last judged for each deploy root under `judged` and the units seen
    running and not yet judged, each with its deploy root, under `running` (`docs/agent-control.md`, 「A
    deployment that did not go live is told」); and the deploy queue's flush's: the owner's own row of the flush
    minutes, `deploy_flush_minutes`, a positive whole number, `deploy_flush_last`, the epoch milliseconds
    of the last flush, and `deploy_stalls`, as JSON, the moment each waiting commit was first read waiting under
    `since`, keyed by its repository, item and commit, those told as waiting too long under `stale`, and each
    repository told as one nothing deploys under `undeployable`, with the waiting commits it was told for
    (`docs/agent-control.md`, 「The deploy queue is flushed every hour」);
  - the machine's room: its seven limits, `machine_limit:<name>`, the launcher's notice of launches held,
    `machine_launch_held`, its `at` and `held` as JSON; and the job and item a job's run named while it
    goes, `worker_job:*` by the checkout, as JSON (`docs/agent-control.md`, 「Room on the machine」);
  - the rest: `rest_until`, `rest_until:*`, each a moment in epoch seconds, `rest_kind`, and `rest_told`, the
    kind of the silence told as it began, kept until its end is answered; and one model's rest on one login,
    `model_rest:<account>:<model>`, as JSON: its `until` in epoch seconds, none where it waits for the
    login's next reading, and the moment it was `written` (`docs/cache-fallback.md`, 「The account
    judgement」);
  - an account's unread mark, `usage_windows_unread:*` by the account, the moment its reader first found
    its token could not be read with (`docs/usage-windows.md`, 「The Claude account reader」);
  - the data move's mark, `users_moved_at`, and the user who administers the platform,
    `users_administrator`, carried with the rows of a store moved across; agency reads them and never
    writes them, and a store without them reads as not moved and as naming no administrator.

## Its seeds

- **The values are the deployment's,** handed to the settling by whoever settles the store; the store
  names none of them and supplies only when each is written.
- **Agency's server settles the store as it starts** with the values agency's policy declares
  (`docs/policy.md`): the account `policy/seeds.toml` names, its credential the Claude Code login
  directory of the machine the server runs on — `CLAUDE_CONFIG_DIR`, else `.claude` under the home
  folder; the catalogue entries `policy/seeds.toml` declares, of which the repository's own declares none,
  since a model file is a deployment's; the base prompts `policy/base-prompts.toml` declares, each with the
  text of its seed file under the code tree's `prompts/base/`; and the roles `policy/roles.toml` declares,
  each with its prompt and its description as `docs/agent-roles.md`, 「The roles the policy seeds」, states
  them — a prompt file's text under the code tree's `prompts/roles/` and the declaration's description — its
  provider order and its compaction threshold, and each provider prompt the declaration names, its file's text
  under `prompts/roles/`, given once by the role and the provider whether the role was seeded then or before. A
  supervisor and a repository are the deployment's to add; the registry's provider is named on a road by the
  owner.
- **The settling** brings the store to its shape (「Its tables」) and then writes every seed in one
  transaction.
- **The account:** a store holding no account is given the one the seed names, the login the machine
  already has; a list the owner has written to is left alone.
- **The model catalogue:** its entries are written once, as its table is made (`docs/model-catalogue.md`,
  「Where it is kept」).
- **The roles:** each is given once by its name and recorded as given, a role of its name already held
  left as it stands and one the owner removed left removed; the order the policy declares them in is the
  order a screen lists them, and a role added later is listed after them.
- **The base prompts:** each is given once by its name and recorded as given, a prompt of its name
  already kept left as it stands and one the owner removed left removed; the agents its declaration
  names are given the choice of it only where its prompt is written (`docs/base-prompts.md`, 「The
  seeds」).

## Connecting

- **The store's address** is the one `BMO_ARCHIVE_URL` names: in the process's own environment, else in
  the server's environment file, the data root's `env/agency.env`, else the one the data root's
  `env/archive-url` holds; a process with none of them names no store.
- **The address stays with agency's own processes.** A program agency starts is not handed it: the
  variable names agency's store, not the archive that program reads. A program reads its own archive
  from the data root's setting, and a command of agency's that runs inside a program's session finds
  the store in `env/agency.env`, which the deployment keeps and the session does not replace.
- **The server's store is the canonical one.** The server reads `env/agency.env` as its unit's
  environment, and the command and the model gateway, which run without it, read the address out of the
  same file, so the registry the command writes, the one the gateway serves and the one the server runs on
  are one. `env/archive-url` names another service's database where both files stand, and agency reads it
  only on an installation whose `env/agency.env` names no store.
- **The service opens a pool,** which never hands out a connection opened before the store's shape last
  moved under the process: a statement met with PostgreSQL's code for a plan whose table moved is run
  once more on a fresh connection.
- **A tool or a host handed the store's address opens one read-only connection,** but for the model
  gateway's request records, which its launcher writes on a connection of their own, one row a statement:
  every transaction on a read-only connection is read-only, and an opening the server refuses while it restarts is tried again with a doubling
  pause until the server answers, however long its restart takes.
- **Every number is bound at its widest,** a whole number as a 64-bit integer and a fraction as a
  double, so a statement prepared once takes every later value.
- **The TypeScript side's transactions write one at a time:** each takes one transaction-scoped
  advisory lock as it begins and holds it until it ends, whichever process of agency's opened it, so a
  transaction reads what the one before it wrote.
