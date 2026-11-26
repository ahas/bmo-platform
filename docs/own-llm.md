# Own LLM services

Agency reaches models of the deployment's own choosing, outside the Claude, Codex and OpenRouter
roads, as registered models.

- **The registered models** are every road to a model the owner registers — an OpenAI-compatible chat
  completions address, an optional key, the model's name and its sizes — which one provider of
  agency's, the registry's, offers on every line it carries. A registration is the whole declaration
  of such a model: one is added, changed and removed with no commit and no deployment, and no model,
  address, window or answer size of one is written in the code.
  - A run with tools — a supervisor's turn, a role's run — is Claude Code pointed at agency's gateway
    under the registration's name (「How requests go」 below).
  - A request with no tools — a translation's part — is one request to the gateway's chat completions
    path under the same name, asked in the agent's host (「A request with no tools」 below).
- **The model catalogue** holds the model files the deployment's own engines serve, each with where it
  comes from, where it lies and how it is asked by default; choosing one for an engine fetches, checks
  and installs it (`docs/model-catalogue.md`).
- **A gate's run is not agency's.** Agency keeps no gate row, no route and no order for a gate, and
  chooses no provider for one (「Gates are not agency's runs」 below).

## A request with no tools

### What it answers to

- **The call:** every such request goes to the gateway's `/v1/chat/completions` with the
  registration's name as its model, the system prompt as the first message where it is not empty, then
  the input, `max_tokens`, and `enable_thinking` in `chat_template_kwargs` as the run says. No request
  names a sampling setting unless the run carries one, so otherwise the engine's own defaults hold.
- **No line of a run with tools:** such a request is asked with no tools, so the request is the whole
  of the run and no program stands in a pane for it.
- **The model list:** the gateway's authenticated `/v1/models` names every registration, and a
  request's registration must stand in it (「The model list」 below).
- **The context window is its registration's:** every request is sized to the window of the
  registration it names (「The context window」 below).
- **Size refusals** come back as a 400 that names the context, and are read as too large (「Input
  size」 below).
- **The key:** the gateway's own, read for each request (「The keys」 below).
- **No state on agency's side:** no session, no prompt cache, no keepalive. An engine behind a
  registration may keep its own state, such as a per-conversation slot.

### What the code still declares

- **The providers are programs, so they are code:** Claude Code, Codex, OpenRouter and the registry's
  provider are each a program or a client with its own launch line, its own records and its own reading
  of an answer, and a build carries the ones it was written with.
- **Which lines the registry's provider carries:** every line but the browse (「Choosing a registered
  model」 below). A line is a shape of the deployment's own work, so it is the deployment's to say which
  provider carries it.
- **The gateway's address and key file,** read in one place by every caller (「The gateway」 below).
- **Nothing else:** no model, address, window, answer size or engine is named in the code beyond
  the seeds the store starts with (「The seeds」 below), which are the owner's once written.

### The own model and the engine behind it

A registered model is what every line on the registry's provider runs on. Its model refuses nothing of
its own, so what it is told is the line it holds, and the line a run goes out on decides what that is,
never the model's name.

- **On the worker line it is a worker.** A run on the worker line — a job's among them — is handed the
  worker line whole, and what that line appends closes on the rules of the repository it works in
  (`docs/launch-lines.md`, `docs/repositories.md`): it answers as a worker of the deployment's, never as the
  harness it runs on, whichever model that is; and it takes no destructive action and none that harms anyone
  else without the owner's order or permission, which only the owner's own words in the item give.
- **Its system prompt opens on its base prompt** (`docs/base-prompts.md`): where the base prompt is a
  harness's own prompt, that prompt is kept whole in the store and handed to the program as the whole
  of its system prompt, with the worker line's rules after it. The harness, the tools, the rules on
  confirming and reporting, and the context management stay, since the program needs them to run; so
  does the program's own opening line, which the identity layer the rules carry answers. Another prompt
  chosen in its place — one written for another harness — is handed the same way.
- **On the chat line it is the supervisor whose turn it is.** A round of the window shape on a registered
  model is handed that supervisor's system prompt exactly as it is, with nothing of a worker's in it.
- **A turn of the pieces shape,** on the chat line, is given its own system prompt: its base prompt, then
  the layers after it (`docs/prompt-layers.md`) — the platform's rules where the supervisor is the platform
  administrator's, its organisation's instructions, the pieces role whole and its persona — in place of a
  harness's own prompt (`docs/supervisors.md`, 「What a
  turn is given」). Nothing of the window shape's documents or of the worker's rules is among them.
- **The rule against destroying the system is on every line that acts,** since every agent that acts is
  given the self-preservation rule (`prompts/rules/self-preservation.md`): the worker and chat lines,
  a pieces-shaped turn's prompt after its base prompt. The platform's own parts it protects are named in the
  platform's `prompts/platform/preservation.md`, which the worker line and the platform administrator's
  supervisors carry beside it.
- **A repository's rules follow the path, not the model:** a run in a repository's support worktree is
  handed that repository's rules, and a run elsewhere on the same registered model is not.
- **A job's session closes with nothing handed over.** Where a job's runner hands its session over as
  the job ends, the session is closed and the next run at that path opens on its own words alone: no
  note of the closed session is carried (`docs/sessions.md`, 「A role's session」, 「Session
  replacement」).
- **Whether a registration keeps its research is a flag for the caller that sends research out.** Agency
  stores it with the registration and answers it with every read of one; it routes nothing by it, since it
  sends no search or browse out itself.
- **Thinking is the only reasoning setting of a registered model:** each registration carries a
  thinking default, which a run on it follows where its order's entry leaves thinking to the
  registration, and the entry's thinking overrides it where it names one (`docs/provider-order.md`,
  「Effort and thinking」). A registered model takes no effort level.
- **Each run samples by its own settings:** a job on a repository's pinned registration by the
  repository's sampling (「A repository pinned to a registration」 below), a supervisor's pieces-shaped turn by its
  generation settings (`docs/supervisors.md`, 「Its generation settings」); a run that names none — a turn of
  the window shape among them — leaves the engine's defaults to hold.
- **The model behind a registration is the engine's, and the engine's model is a catalogue entry.** The
  deployment's own engine serves the file its placement names — an entry of the model catalogue
  (`docs/model-catalogue.md`) — and every request reaches whatever file that is, since the gateway
  sends the registration's name and the engine serves one model. How the engine is served, the hardware
  it stands on and how fast it reads and writes are the node's own (`docs/work-nodes.md`,
  `docs/model-catalogue.md`), not agency's: agency knows a registration, its address and its sizes, and
  nothing of the weights behind it.

### The model list

- **The gateway's list names every registration** and every node of one, and a request takes from it
  its own registration alone: the list must name it, and nothing asks another.
- **Checked on call:** a request whose registration the list does not name is refused as a changed
  list, and the run returns to the existing run policy.
- **Caching the list:** the list is kept for one minute, and the first request after that minute
  fetches it again, since the list rarely changes and fetching on every request would make each
  request send two.
- **No time limit:** the list fetch and the request each wait for their answer; each ends when the
  service answers or the connection is refused or broken.

## Registered models

### What a registration holds

- **Its name:** lowercase letters, digits and hyphens, beginning with a letter, unique in the registry.
  It is the name the gateway serves it under, the model the registry's provider lists, and what an
  order's entry, a repository's pin and a held session name. It is fixed once made.
- **Its label:** how a screen shows it.
- **Its addresses,** one or more, each a node the same model is served on: an OpenAI-compatible chat
  completions address ending in `/chat/completions`; a short label, unique within the registration,
  that names the node; a key or none (「The keys」 below); and whether the instance behind it keeps role
  files, a role's saved prefix it restores on asking (「Role files」 below), off unless named.
- **Its model:** the model's name as every one of its addresses serves it.
- **Its context window:** a whole number of tokens within the bounds the policy sets (「The context
  window」 below).
- **Its answer size:** the answer ceiling of a run on it where its order's entry names none, a whole
  number of tokens within the policy's bounds and smaller than its window.
- **Its thinking default:** on or off, what a run on it does where its order's entry leaves thinking to
  the registration.
- **Whether it is walked:** on or off, kept with the registration.
- **Its card service:** the arbiter service its engine is, or none, and whether that service's instances
  keep role files. A registration naming one holds no addresses of its own: where its engine stands, and so
  where a run on it goes, is the arbiter's to say at that moment (「An engine the arbiter stands」 below).
  Naming one takes away the addresses the registration held, and no address is added to it while it names
  one.
- **Whether it keeps its research:** on or off, kept for the caller that sends a search or a browse out and
  reads it; agency acts on neither.
- **Its languages:** the languages a run on it writes a result for a person in, each by its English name,
  or none, which means every language. The owner names them for a model whose supported languages lack
  the language a person asks in, or that writes it badly; nothing names them on its own, and no list of
  models or languages stands in the code. A run on the worker line on a registration naming some is told
  them, and writes its results in English where its brief names a language outside them (「A run on the
  worker line is composed as on Claude」 below; `docs/worker-language.md`, 「A model that writes only some
  languages」).
- **Its owner:** the account it belongs to, or the organisation unit it is filed under, or neither for the
  platform's (`docs/supply.md`, 「Owned supply」; `docs/api.md`, 「Who a caller acts for」). An
  owned registration is served to its owner's call key and the gateway's own key alone; a registration filed
  under a unit is no one's personal supply and no platform source — it is the unit's organisation supply, which
  the walk that takes a unit's things takes with its keys, Claude accounts and members, for the people of the
  unit's path alone (`docs/supply.md`, 「Organisation supply」); none is owned until the owner names one.
- **Nothing else of a run's prompt:** what a run on it is told is its line's, and an agent's base prompt is
  the agent's own choice (`docs/base-prompts.md`); the gateway passes every prompt as it stands.

### Where it is kept

- **In the store:** one table of registrations and one of their addresses, each address row belonging
  to its registration and removed with it.
- **At the boundary:** `/models` is the model resource — the list, the create, the read, the change and
  the take-away all read or write these rows, so what is listed is what can be changed and removed. The
  model catalogue is another resource, `/model-catalogue` (`docs/model-catalogue.md`).
- **Read afresh by every run and every screen,** so a saved change reaches the next run with no
  deployment and no restart of the bot. What the gateway carries — the addresses, the keys and the
  model behind — reaches it as 「The gateway」 below says.
- **A registration with no address and no card service** is listed, and offered nowhere: no order, pin
  or run may name it, and the gateway's table leaves it out.

### The context window

- **Each registration's own,** read afresh by every run on it: the window Claude Code is told (「How
  requests go」 below), and the room the answer of a request with no tools is given (「Input size」
  below). A change reaches the next run, with no deployment.
- **Never empty:** a registration holds a window from the moment it is made, and a save that leaves it
  out or gives what it does not take is refused, so what the console and the command show is what runs.
- **The model list's own window sizes nothing:** a `max_model_len` a list carries is not read, so the
  owner may hand a run less than the model serves.
- **A request with no tools run in its agent's host** is handed the window the bot read, since the host
  holds no store.
- **A process holding no store** has no registry to read and refuses a request with no tools, naming
  its registration; nothing stands in for the window.

### The seeds

- **The registrations the deployment's own models start with** are written once, when the registry's
  tables are made, and are then the owner's like any other; a seed removed stays removed.
- **A field the registry gains later** is given each seed's value once, as its column is made, on the
  registration of the seed's name where it still stands; every other registration takes the field's off
  value.
- **The window each seed starts at** is the value its seed holds, or the one an older store's own
  setting held for the same model where it held a window the registration takes.
- **Every node takes its key:** a request to a node's front with no key is refused, so a seed's address
  carries a key even where its model runs on the deployment's own machines.

### A repository's own registration

- **A registration the owner makes** with the registry's command, like any other and not a seed, may be
  the model a repository pins (「A repository pinned to a registration」 below), while a supervisor stays
  on another. What this document says of the worker line holds there alike: a job in the repository reads
  the worker line, and the pinned model answers as a worker of that repository.

### A person's local AI

- **A local AI a person adds is a registration they own** (`docs/supply.md`, 「A person's local AI」), made through
  `/personal-models` in their name, never through the owner's routes or the command:
  - its name is `u<account>-` and the name the person gave, so no two people's collide and the registry's rule
    holds; its label is the name they gave;
  - its one address stands on their PC, node label `pc`: the address as they gave it, plain `http`, its key the
    file of their own secret, `users/<account>/<name>` under `env/`, which agency neither writes nor removes;
  - its window is the one the person names within the registry's bounds, else the policy's
    (`policy/supply.toml`, `[local_ai]`), and its answer size the policy's;
  - it is on or off, the person's switch: one off offers no personal supply and its address is carried by no link.
- **Its address is reached through the person's PC, never from here.** The address row keeps a port of this
  machine's loopback, the lowest free in the policy's range as the address is made, and the gateway asks that port
  in place of the address's own host and port, keeping its path; the PC's link listens on the port and carries each
  connection to the address as the PC reaches it (`docs/person-nodes.md`, 「A local AI on a PC」). An address on the
  loopback of the PC is the PC's own, so nothing here is ever asked in its name. While the PC's link stands
  down, the port refuses, and the run fails as a registration whose engine does not answer does.

### Where it is set

- **The console:** the registered-models tab of the agent management screen lists every registration
  and adds, changes and removes one. The model files behind them are a tab of their own beside it
  (`docs/model-catalogue.md`, 「Where it is set」). An order's entry on the registry's provider, on the
  supervisor and role tabs, and a repository's pin, on the repository tab, offer every registration where
  a model is chosen.
- **The command:**
  - `bmo-agency agent --registered` lists every registration and its addresses, each address with
    whether its key is usable, and the languages of a registration naming some.
  - `bmo-agency agent --register <name> [--label <label>] [--model <model>] [--context <tokens>]
    [--answer <tokens>] [--thinking on|off] [--walked on|off] [--research on|off]
    [--card-service <service>|none] [--role-files on|off] [--languages <list>|none]` makes a
    registration, every field but the label, the card service, the role files and the languages named,
    or changes the named fields of one; `--role-files` here says whether the card service's instances keep
    role files, and `--languages` takes the languages as a comma-separated list, or `none` for every
    language.
  - `bmo-agency agent --register-address <name> <node label> <address> [--key-stdin | --no-key]
    [--role-files on|off]` adds an address or changes one, and is refused on a registration naming a card
    service; `--key-stdin` reads the key from standing
    input, and a key is never taken on the command line; `--role-files` says whether the instance behind
    it keeps role files (「Role files」 below), left as it stands where it is not named. The list marks
    such an address.
  - `bmo-agency agent --unregister <name> [<node label>]` removes a registration, or one of its
    addresses.
- **The address route:** `PUT /models/{modelKey}/addresses/{node}` adds or changes one address as
  `--register-address` does, its key named by a file in the platform's secret space rather than given
  (「The keys」 below; `docs/api.md`, 「providers, models and model files」).
- **What a removal is refused for:** a registration a supervisor's road, an agent of a role —
  switched on or off — a repository's pin or a model catalogue entry names is not removed, and the refusal names each of them
  (`docs/provider-order.md`, 「Models a provider holds」); each of those answers for itself whether it
  names the registration. The last address of a
  registration something names is not removed either.
- **The rules a save is held to** — what a name, a node label, a model, a key file and a language may
  be, how many languages a registration may name, the bounds of the window and the answer size, and how
  long a label may be — are agency's policy
  (`policy/`), so the console's save is checked alike whichever side answers it.

### Nodes and sessions

- **Why a session keeps to one node:** a node saves the state of the conversation it last answered to
  its own disk and loads it back for the conversation's next request (`docs/work-nodes.md`). A request
  that reaches another node is computed from its first token, which on a long conversation is minutes
  of reading where the saved state loads in a fraction of a second.
- **The gateway serves a registration twice over:** under its name, spread over all its addresses, for a
  request that belongs to no session — a translation's part, a model list; and under `<name>@<node label>`,
  one address alone, for each address of a registration that has more than one.
- **A program is placed on one node:** a Claude Code program started for a new session on a
  registration with several addresses is given the name of one node, and every request of that session
  asks that node alone.
- **Which node:** the one given a new session least recently, which the registry keeps for each
  address, so sessions spread over the nodes. No node is probed first: a card another service holds
  shows nothing a probe could read until a request waits on it.
- **A new session its node refuses goes to the next node:** where the first run of a new session is
  refused by its node before any answer — a 502 or a 503, as a node answers when another service holds
  its card past the wait — the same run is placed again, by the same rule, until one answers or every
  address has been tried once in that run. The refused node was just given a session, so the next ones
  pass it by too. A resumed session is never moved, since its state is on its node, and a run its node
  began answering is not placed again, since another node would have to compute it from the start.
- **A resumed session keeps its node:** the record of a held session keeps the node beside its model,
  as it keeps its login, and a program started to resume the session, or to fold it, is placed there
  again. A session whose node has left the registration is not resumed; the run opens a new one.
- **A registration with one address** has no nodes to choose from, and its runs name it alone.

### An engine the arbiter stands

- **Routing is automatic.** A run names the model it needs, its registration; where that model runs and
  on which card is decided by the arbiter's master and this registry at that moment, never by a setting,
  a node list or an assignment a person makes. The arbiter's master is asked through the same path every
  caller of a service standing on several nodes uses (the arbiter's `docs/placement.md`, 「Demands」), and
  its address and token file are agency settings, read from the files the arbiter's deployment keeps:
  `AGENCY_ARBITER_URL` names the master's address and `AGENCY_ARBITER_TOKEN_FILE` the arbiter deployment's
  settings file, whose `CARD_MASTER_TOKEN` is the token every call carries. The server reads both from its
  environment, and the gateway's launcher from its own or else from the server's settings file. A run on a registration naming a card service where no master
  is named is refused, the sentence naming the setting.
- **A registration naming a card service is served wherever the master grants it.** Its instances are the
  ones the master reports standing, each with the address callers reach it at and the file its key is
  read from, as the engine's own deployment reports them; whether they keep role files is the
  registration's.
- **Every run on it asks a demand first:** a supervisor's turn, a role's run — a translation's request among
  them — alike ask the master for one card of the card service, with the
  session's key as the work it continues and agency's holder as the demand's shown reference. A request at
  the gateway's door on a registration with no instance standing asks one too (「The gateway」 below). A
  caller that holds a granted demand of its own hands its id in with its slot or session request, and no
  other is asked for that run. A session started on a slot whose lease holds a demand stands on that demand
  the same way, so a run holds one card however many turns it takes. A translation shares one with those out
  beside it (below).
- **The run goes where the demand is granted.** A new session is placed on the card granted; a resumed
  session asks with its key, which brings it back to the card holding its state wherever that card is
  open, and is computed from its first token where another card answers. The run names the card's entry,
  `<name>@<node>`, once the gateway serves it, an engine the grant started being taken into the gateway's
  table on the launcher's next beat. A program standing with nothing put in, a supervisor's between its
  turns, names the registration alone.
- **A run waits for its grant however long it takes,** the master's answer saying why — the engine
  starting, every card in use, a bench or a claim. Each reason is written into the run's log as it changes,
  and the turn the run goes out for carries the wait as its typed state: its `waiting`, a card wait of the
  card service with the master's reason, on the turn's read and in `turn.changed` as it begins, changes and
  clears on the grant (`docs/api.md`, 「sessions」), so the receiver shows it on the room's working line. A
  supervisor's turn waits so as it waits for an engine a job holds (`docs/supervisors.md`, 「The engine it
  shares」); a job's turn waits so as its job's `heldFor`, told to the item's orderer once
  (`docs/api.md`, 「assignments and jobs」); a role's run waits for its grant as it waits for its slot
  (`docs/role-slots.md`, 「On a registration the arbiter stands」). Agency adds no clock to that wait. A stop ends it: a run whose cancellation stands over its
  name — the interrupt of the turn it goes out for — gives back the demand it asked and goes out on nothing.
- **A demand's rank and pool are the caller's settings, handed on as they are** (the arbiter's
  `docs/placement.md`, 「A demand's rank」, 「A pool」). A caller names them on its slot or session ask
  (`docs/api.md`, 「sessions」); where it names no rank, the demand is asked at the rank of the supervisor the
  run is for: the supervisor whose turn it is, the one that asked the run's slot (its `askedBy`), or the one that
  ordered the job. Work asked within a run takes the run's rank and pool from the master.
- **The supervisor that ordered a job** is found from the job's item by the one resolver of its orderer
  (`docs/agent-control.md`, 「An item's change is told to its orderer」), which answers a room: the run ledger then
  names the supervisors whose turns went into that room. Exactly one names that supervisor. None, or more than
  one, or a job service that cannot be read, is no answer: the job asks at the default rank, and the server's
  log says why. Agency does not guess between supervisors of one room.
- **The demand is held for the run's turn and given back at its end.** The card can be taken from it by a
  bench or a claim, which stops the engine as the arbiter's lower tiers always have: the turn's request in hand is
  then cut, and its next request waits for its demand to be granted again. A demand of a better rank can take it
  too, stopping the engine only where it wants the other one, so nothing else would cut the turn: agency watches the
  demand of every turn in flight, its answer held open with the master's `watch`, and once a better rank has taken
  the card the turn is stopped as an interrupt stops it and ends `card-taken` (`docs/api.md`, 「sessions」). Its
  session's next turn waits for its demand to be granted again; asking the turn again is its caller's. A demand whose run
  has ended without giving it back is given back by agency at its start and with its lease: every demand
  agency asks is kept in the store's `card_demands` with its holder, the slot's lease where one holds it
  and the demand it was asked within, until it is given back; the server gives back every demand kept
  there as it starts, and every minute every demand of a lease no longer active (`docs/role-slots.md`,
  「The line」).
- **The translations out at once on a registration share one card.** Each translation is one request
  of its own (`docs/api.md`, 「translations」), and its caller sends a document's parts together, so what
  holds the card is the translations agency has out on the registration at that moment: the first asks a
  demand, with `translator` as its holder; every one asked while that demand waits or stands
  waits on the same grant and is sent to the granted card's entry, the parts reaching the engine together
  as its slots take them. The demand is the line's, not the first translation's: a translation ends when it
  is answered, when it has failed, or when its caller goes away before its answer, and whichever ends, the
  rest keep waiting on the same demand, none asking another. Once the last of them has ended the demand is
  given back, granted or still waiting, so a line with no translation out holds no card and no place in the
  master's line. A demand the master refuses fails every translation waiting on it. A translation asked
  after the last has ended asks a new one.
- **Work a run asks holds the run's card as its own.** A search branch or a session asked from a run whose own demand is granted asks its demand within the run's. Work on the
  model the run already stands on is granted on the run's own card, beside the run, which waits on it.
  Work on another registration naming a card service goes to any other card with that engine idle, or a
  card free or idle, and only where none is does the run hand its own card over: the self-swap. The run's
  engine then stands down, its session saved, while the asked work runs on its card, and comes back once
  the work's demand is given back. A run holding no card that finds none open waits in the master's line
  like any caller.
- **The run comes back before its next request, and is told.** After such an ask returns, the run's next
  request is held until the run's own demand is granted again and its engine answers health, however long
  that takes. What the ask hands back says that the run's own engine stood down for the work and came back,
  so the agent knows what its ask cost: "Your own model stood down while this work ran on its card, and it
  has come back." A subagent the run's program opens is the run's own requests, on the run's card. A slot
  asks within the demand its admission names, and the slot's finish answers the sentence.
- **The gateway never sends to an engine that is down.** Its table holds, for such a registration, one
  entry for each instance the master reports standing, and the entry under the registration's name alone
  spread over exactly those; the launcher reads the master's picture on its beat, and a change starts a
  new child as a changed registry does. Each run names the entry of the card its demand was granted.
- **Health is the master's, not an address's.** An instance answers health only while its engine stands,
  and one that is stopped is no outage. The registration is `healthy` while its card service is placed on
  a node no claim and no lower tier holds, and a walk that would pass it for an outage asks the master
  (`docs/cache-fallback.md`, 「Automatic walks only on providers, in order」).
- **A stopped engine never closes the road.** Such a registration is offered whether or not any instance
  stands at the moment, so the registry's provider can answer for an order or a pin that names it with no
  engine up anywhere (`docs/cache-fallback.md`, 「Availability: whether a provider can answer now」). The
  provider order's choice never reads the instances standing, nor the gateway's table: the run goes out on
  the registry, asks its demand, the grant starts the engine, and the run sends once the gateway serves the
  granted card's entry. A run leaves the registration only on a real refusal — the master refusing the
  demand, or the engine failing the request — and then as a registered model that does not answer (「When a
  registered model does not answer」 below), never because nothing stood when it asked.

## What leaves the machine

- **A registration sends its runs to its addresses,** wherever the owner points them: an address off
  the machine is a service that receives everything its runs carry — a request with no tools' system
  prompt and input, and a run with tools' prompt and every file and command output its tools read.
  - Where that address is an external operator's service, its retention is the operator's word, not a
    boundary agency enforces locally, and the owner is warned of it where the service asks for no
    personal data.
- **A registration on the deployment's own machines stays on them:** its requests go to loopback — the
  gateway, and behind it the engine on its node and the cards it uses on the internal network — and
  what each hop logs of a request is the node's own concern (`docs/work-nodes.md`).
- **A person's local AI is sent their own runs alone,** on their own PC or the network it reaches: what a run of
  theirs carries goes to the server they named, and nothing of anyone else's.
- **What a translation on an external registration sends about people:** parts of documents the console
  asked to translate, a person book's text as it is where that is the document.

## The keys

- **Where they live:** under the data root's `env/`, outside the store and the checkout, each file at
  permission `0600`.
- **A registration's key** is one per address. The address row holds the name of the file under `env/`,
  never the key.
  - A key given on the console or the command is written by agency into a file of its own under
    `env/registered/`, named for the registration and the node, at `0600`, and a key given again
    replaces it.
  - A seeded address names a file that was already there — a node's token.
  - An address set through `PUT /models/{modelKey}/addresses/{node}` may name a key file the console's secret
    store wrote in the platform's space, one plain file name under `env/`. Agency keeps the name alone and
    neither writes, reads back nor removes that file.
  - A person's local AI names the file of the person's own secret, `users/<account>/<name>`, which agency keeps
    the name of alone, as a named key file (「A person's local AI」).
  - A removed address or registration takes away the files agency wrote for it, and leaves a file it did
    not write where it is.
- **Who handles the files:** the harness-machine crate writes, takes away and checks them, for agency's
  server (`docs/harness-machine.md`, 「The keys」).
- **Where a key never goes:** the store, logs, API responses, the browser, a command line. The console
  and the command receive, for each address, only whether its key is usable; a key reaches the bot once,
  in the body of the save that gives it, and is never sent back.
- **How Claude Code is handed one:** its settings name a key helper that prints a key file — the call key
  of the account the run is for, or the gateway's own key for a run of no account or of the administrator's
  account — which the program runs for its requests, so the key is read out of the file each time and stands
  in no command, environment or record of agency's.
- **How the gateway is handed the registrations' keys:** its launcher reads each file into its child's
  environment alone (「The gateway」 below).
- **The gateway's own key is the platform's door:** `llm-gateway.key` under `env/`, which the deployment's
  own callers present as `Authorization: Bearer <key>`: its own runs here, its translations, its gates. It
  never goes to a node that is a person's.
- **Every other account holds a call key of its own,** issued by agency's server the first time a link or a
  launch needs one for the account, and the only key that account's nodes are sent:
  - its value is written once, at `0600`, as `llm-gateway-calls/<account>.key` under `env/`, and the store
    keeps only its SHA-256 digest, with when it was issued and when it was revoked, in `gateway_call_keys`;
  - a revoked key stays refused, and the account's next need issues a fresh one;
  - the payer of a request is the account its call key is, so a header a caller writes moves no cost.
- **The front checks one of the two on every request** before anything reaches LiteLLM, so a reverse proxy
  standing a public door in front of the gateway adds no check of its own and needs none (「The gateway」
  below).

## The gateway's members

- **A deployment serves providers of its own beside the registry.**
  `llm-gateway/members.json` under the data root names each member; the launcher adds one
  `model_list` entry per member beside the registrations.
  - `road`: the agent provider the member belongs to, such as `deepseek`, `openrouter` or `codex`. A
    member with no road is served by the gateway alone, for a caller that names it by hand.
  - `name`: the model name a caller asks for. It is unique among the members and against every
    registration's name and `<name>@<node>`.
  - `label`: what a screen shows in place of the name.
  - `provider`: the LiteLLM provider, one of `anthropic`, `deepseek`, `openrouter` and `openai`.
  - `model`: the id the provider's API knows, with its `api_base` and its `key_file` under the data
    root's `env/`.
  - `context_tokens`, `answer_tokens`, `thinking`: the window, the answer ceiling and the default
    thinking of a run on the member. A member of a road holds all three; a member of no road may
    leave them out.
  - `account`: the account the member belongs to (`docs/supply.md`, 「Owned supply」), the person whose own key
    it is. `unit`: the organisation unit the member is filed under instead — one member names at most one of
    the two, none for the platform's. Either owner is set, moved or taken away with
    `PATCH /gateway-members/{name}` (`docs/api.md`, 「providers, models and model files」), which writes the
    file and leaves every other field as it stands; the next beat serves it. The setting is the platform's
    alone; the read of the list is narrowed to what the caller reaches.
  - `free`: whether a run on the member costs nothing, so its road's spent balance leaves it answering
    (`docs/provider-order.md`, 「Quota: what a provider has left」). Where the member leaves it out, its
    road's provider reads it off the member: OpenRouter reads a member whose `model` ends in `:free`, its
    own mark of a model it serves at no cost, as free, and every other provider reads a member as costing.
- **A member's key is named by its file and never written in the file.** The launcher reads each key
  file into its child's environment alone, as it does for a registration's address.
- **A name two entries hold refuses the table.** The launcher answers with the refused table, and the
  last good table stands; nothing is served under a name that would answer for another model.
- **The file is read again on each beat.** A changed file starts a new child as a changed registry
  does, so a member added, changed or removed reaches the gateway without a restart. A file that does
  not read is logged and leaves the last good table standing.
- **The deployment seeds the file once.** The repository's `deploy/llm-gateway-members.json` is placed
  where the data-root file is absent, and a file the deployment already holds is never overwritten:
  the members are the deployment's, not the repository's.
- **A member's model name is what Claude Code's `/model` asks for.** Health, a streamed answer and a
  tool call are asked through the Messages API at `/v1/messages`, behind the gateway's key like every
  other request.
- **`/v1/models` names every member** beside every registration, under the gateway's key. `/health`
  remains LiteLLM's own and can fail on an installation with no Prisma database. A member whose
  provider refuses its key is named by its provider's error, never answered by another member.
- **Claude's subscription OAuth is no member.** A local endpoint is a registration or a member whose
  provider is `openai` and whose `api_base` is the endpoint's address.
- **An owned member or registration answers its owner alone:** a request naming it, or `<name>@<node>` of
  it, under its owner's call key goes on, and under the gateway's own key where its owner is the administrator,
  whose supply the deployment's own runs go out on (`docs/supply.md`, 「Owned supply」); under any other key it is
  answered 403 with the code `not-your-member` and the type `permission_error`, as a personal member is
  (「Personal members」). **What stands filed under a unit answers the unit's people's runs:** a request naming
  it, or `<name>@<node>` of it, goes on under the call key of an account holding an open reservation naming that
  unit, as the unit's key variants do (「Personal members」 below), and under no other key, the gateway's own
  among them — a unit's model is its organisation supply, never a platform source (`docs/supply.md`, 「Owned
  supply」). Every route that lists or describes models — `/v1/models`, `/models` and LiteLLM's
  model information routes — leaves it out under a call key it does not answer: a name filed under a unit stands
  in the lists of the keys holding an open reservation naming that unit alone, and a person's key variant in its
  own key's list alone. The gateway's own key lists the
  whole table, since the deployment's own reads of it name what stands, not what they may run. An owner set or
  taken away reaches the front on the next beat, read from the members file and the registry whether or not the
  table itself was built again.
- **The administrator's call key is asked for no reservation,** where one is issued: the administrator's runs
  are the deployment's own. It is otherwise a call key like any other: it reaches the request routes and the
  model lists alone, and the platform's names with the administrator's own, personal members among them.
- **A call key's request whose model cannot be read is refused** 400 before it reaches LiteLLM, on every route
  that names a model, since its owner and its supply cannot be judged.

## Personal members

- **A person's own key for a gateway road is a member of its own** (`docs/supply.md`, 「What a person
  brings」): for the first open row of personal supply a person holds for a road, the launcher lays beside
  each member of that road, whoever owns it, one more under `<member>@u<account>`, its `key_file` the person's secret,
  `users/<account>/<name>` under `env/` — a secret that is not one plain file name lays none — and no road of its
  own, so no road's model list names it. Its key
  reaches the child's environment alone, as every member's does.
- **The store's personal supply is read on each beat,** as the registry is, so a key added, turned off or
  found refused reaches the gateway without a restart.
- **Only that person's call key reaches it:** a request naming `<member>@u<account>` under any other key,
  the gateway's own among them, is answered 403 with the code `not-your-member` and the type `permission_error`.
- **An organisation unit's key is laid the same way** (`docs/supply.md`, 「Organisation supply」): for the
  first open row a unit holds for a road, one more beside each member of that road under `<member>@o<unit>`,
  its `key_file` the unit's secret `units/<unit>/<name>` under `env/`. A call key reaches it only while its
  account holds an open reservation naming that unit, as the store says at the request, so only a person of the
  unit's path whose run took the unit's supply reaches it; every other key, the gateway's own among them, is
  answered 403 `not-your-member`.
- **A member a unit itself owns is laid by its own name** (`docs/supply.md`, 「Organisation supply」): its
  `key_file`, named in the members file beside its `unit`, is already the unit's secret, so the unit's people's
  run on it goes out under the member's plain name, and it is answered, like the variant laid on the unit's key
  row, under a call key holding an open reservation naming that unit and under no other.
- **A run of the person on personal supply** is sent under that name in place of the member's own, and a run
  on organisation supply under the unit's — under its plain name where the member itself is the unit's, and on
  the registry's provider under the registration's own name, which needs no variant at all.


## The gateway

- **What it is:** a LiteLLM proxy behind a front of agency's, standing as its own user unit and listening
  on loopback. It serves the OpenAI-compatible API and Anthropic's Messages API in front, and asks an
  OpenAI-compatible backend at its chat completions path behind, translating the Messages API both ways:
  tool-use and tool-result blocks against tool calls, the streamed events, and an error in the shape the
  caller reads.
- **Where its callers find it:** every caller — a run with tools and a request with none alike — reads
  the gateway's address in one place, an environment variable where the process names one and a loopback
  default otherwise. The bot names none. A check that stands a launcher of its own on another port names
  it for its own runs, so they reach that launcher and never the one serving the bot. A run on a work node, a
  person's PC or an isolation node reads the same address there, which the node's link carries to the gateway
  here, and its key in a file the link sends beside it (`docs/work-nodes.md`, 「The model gateway on a node」).
- **Who goes through it:** every request on a registered model.
  - A run with tools, on the Messages API: Claude Code speaks nothing else, and a registration is an
    OpenAI-compatible address.
  - A request with no tools, on the chat completions path and the model list.
  - A request for vectors, on the embeddings route: a caller that searches by meaning embeds its texts on a
    registration whose engine is an embedding model.
  - A caller off the machine, through a public door: a reverse proxy may stand in front of the gateway's
    loopback port — the deployment's nginx serves it under `/llm/` — and hands every request through as
    it stands. The proxy checks nothing; the gateway's own key, checked at the front, is the only door.
- **What a public caller may ask:** the OpenAI-compatible routes `/v1/models` and
  `/v1/chat/completions`, and Anthropic's Messages route `/v1/messages`, each under the public door's
  prefix. A model is named by its registration's name, such as `flash-next`, which LiteLLM serves from
  any of its addresses, or pinned to one node as `<name>@<node>`, such as `flash-next@n3`; a member of the
  gateway is named by its own name (「The gateway's members」 above).
- **Its table is written from the registry:** the unit's process is agency's gateway launcher, the
  `llm-gateway` binary. It reads the registry out of the store the server runs on, at the address found
  as every reader of the store finds it (`docs/store.md`, 「Connecting」), opened read-only for each
  reading, so it never holds a lock the bot waits on; the request records alone it writes, one row a statement,
  on a connection of their own. It writes the table under the data root, private
  and renamed into place, copies the hooks beside it from the gateway crate's `hooks/` folder in the tree it runs from,
  and starts LiteLLM on it as a child on a free loopback port. A port picked free stands open to any process
  until the child binds it, so the child is not taken where another process, outside the child and its
  descendants, is seen holding the port's listener: the child is ended or has ended for want of the port, and
  is started again on another free port, a few times at most. Where no listener's holder can be read, the
  answer is taken as the child's. The table names each key by a variable
  the launcher sets from the key's file in that child's environment alone, and never a key; the child's
  environment holds nothing else but its `PATH`, its `HOME` and the front's own address, `LLM_GATEWAY_FRONT`, which
  the costs hook hands each request's cost to.
  - A registry not settled yet reads as nothing to serve for now, and is read again at the next beat.
    A launcher with no store address named at all does not start.
  - A child is ready once its readiness check answers, however long it takes to come up; one that ends
    first is given up, and the child serving goes on serving.
  - A child is stopped by asking it to end and waiting until it has.
  - Tables left by a launcher that ended without stopping its children are removed as it starts.
- **The launcher is the front:** it listens on the gateway's loopback port itself and streams every
  request through to the child it serves from, and every answer back, as they stand. Of a request on the
  chat completions path, the embeddings route or the Messages route it reads the model the body names, and
  nothing else; of an answer it reads only whether it is streamed, by its content type
  `text/event-stream`. It counts the requests in hand through each child.
  - The hop-by-hop headers are not handed on either way, a request asks for its answer unencoded, and an
    answer's content-encoding is not handed back.
  - Before any table stands it answers `503`, and where the child cannot be reached `502`, each with a
    line saying so.
  - A streamed answer whose bytes have stopped for the stall span is cut (「A streamed answer whose bytes
    stop is cut」 below).
  - A caller that leaves closes the front's connection to the child and its place among the requests in
    hand; LiteLLM still holds the node until the node has answered.
- **A request on an engine standing nowhere asks for its card,** as a translation's request does (「An
  engine the arbiter stands」 above). A request on the chat completions path, the embeddings route or the
  Messages route that names a registration naming a card service, or `<name>@<node>` of one, under a name
  the table it would be served from holds no entry for, is not handed to LiteLLM, which would answer it
  with an unknown model:
  - The front asks the master for one card of the registration's card service, with the gateway's door as
    the demand's shown reference and nothing more; for a pinned `<name>@<node>` the master still chooses
    the node.
  - It holds the caller's connection open while the demand waits, reading the demand with its answer held
    open and asking again as the master answers, however long that takes; the front keeps no wait of its
    own.
  - Once the demand is granted it waits until the launcher's beat has laid the granted card's entry,
    `<name>@<node>`, into the table a child serves, and sends the request to that child as it sends every
    other. A pinned name the granted card does not match is sent under the registration's name alone.
  - A request names who it is for where its run's composition knows, its rank in `x-bmo-card-rank` and its
    pool in `x-bmo-card-pool`, as `<cards> <key>`, and the door's demand is asked with them; a request naming
    neither is asked at the default rank in no pool.
  - The demand is the door's line's, one for each card service and each such naming, not the first request's:
    every request on the same service and naming that comes while it waits or stands waits on the same grant,
    none asking another,
    and each holds it until its answer has been carried whole or its caller has gone. Once the last of
    them has ended the demand is given back, granted or still waiting. The engine then stays on its card
    until another service's demand takes it, which is the master's own rule, so the next request finds
    its entry served and asks nothing.
  - A master refusing the demand, such as one placing the service on no node, is answered `503` with the
    master's sentence, to every request waiting on it; the next request asks anew.
  - A model that is no registration and no member is handed to LiteLLM as it stands and answered `400`,
    and no demand is asked for it; nor for a registration naming no card service. With no master named, a
    registration naming a card service is served nowhere, as the table says, and no demand is asked.
  - The demands the door asks are given back as the launcher starts: every demand the master holds under
    the door's reference is one a launcher that ended left, since a demand lives no longer than the
    requests waiting on it.
- **What the table holds for a registration:** one entry under its name for every address, and, for a
  registration of several addresses, one more under `<name>@<node label>` for each (「Nodes and
  sessions」 above). A registration naming a card service holds the instances the arbiter's master reports
  standing in place of addresses, each under `<name>@<node>` even where one stands alone and all of them
  under its name, its instances' role files the registration's; a beat whose picture cannot be read leaves
  the last good table standing, and with no master named, or a picture placing no such service, the
  registration is served nowhere. The launcher reads the master's address and token file from its
  own environment, else those two entries alone of the server's settings file under the data root's
  `env/`. Each entry asks its address's chat completions path, with the address's key, for the
  registration's model, and sends `enable_thinking` off in `chat_template_kwargs` where a request names
  nothing. No entry holds a wait (「No request is given up on a clock」 below).
- **No request is given up on a clock.** A request is waited for until its backend answers or its
  connection is refused or broken, and a streamed answer that has sent no byte for the stall span counts
  as broken (the one exception, below).
  - A node sends nothing, not even its response headers, until it has read the whole prompt, so a long
    prompt is most of an hour of silence.
  - LiteLLM has no value that means no wait — an entry naming none falls to the proxy-wide default, and
    an infinite one fails the connection — so a hook of the gateway's own, run after the entry is chosen,
    takes the wait off every request. The gateway-wide request wait is not read on the Messages path, so
    the table sets none, and no entry names one.
  - The gateway does not pass on a caller that leaves: a request whose caller has gone goes on holding
    the backend, a node's card among them, until the backend has answered it.
- **A streamed answer whose bytes stop is cut.** The owner decided that a stream whose bytes have
  stopped counts as broken, and this is the one judgement of time the platform makes on a request. It
  judges whether the peer is alive, not how long the work may take: a stream that has stopped sending will
  never answer, and a backend that answered `200` and then sent nothing, its connection kept alive, would
  otherwise hold the turn with no event to end on.
  - What is watched: an answer the child streams, told by its content type `text/event-stream`, from its
    headers on — the wait for its first byte and every gap between two bytes. The wait for the headers is
    not watched, since a node sends none until it has read the whole prompt. A non-streamed answer has no
    bytes to watch and ends with its body, and a request waiting at the door for its card is not a stream
    and keeps no clock.
  - The stall span: `stall_seconds` in the gateway's policy part, `policy/gateway.toml`, 180 seconds. It
    outlasts a reasoning model's silent prefix at a long prompt: the deployment's engines stream their
    reasoning tokens and DeepSeek streams its `reasoning_content`, so a stream silent that long has
    stalled. A check that stands a launcher of its own may name a shorter span in the launcher's
    environment, `LLM_GATEWAY_STALL_MS`, in milliseconds.
  - What is done: the front ends the caller's answer with an error in the middle of its body, which the
    caller reads as a broken connection — Claude Code retries it, and once its retries are spent reports
    it as a server error, a failure of the provider (`docs/provider-order.md`, 「A road that fails」) — and closes its connection to the child, which ends LiteLLM's stream of that answer and its
    request to the backend. It writes one line naming the model, the child by its table and port, and how
    long the stream was silent.
- **A streamed answer's open block is counted as it passes.** The front reads the Messages events of a
  streamed answer whose request names the program's session, Claude Code's `x-claude-code-session-id`, as
  they go by, and holds for that session the block being written: its place in the answer, its kind
  (thinking, text or a tool call), and the bytes its deltas have written so far. A block that ends, an
  answer that ends and a stream that is cut each let go of it, so a session holds a block only while one is
  open. The bytes are counted and never weighed as tokens: what they come to is the reader's estimate.
  - `GET /gateway/streams/{session}` behind the gateway's key answers the open block of the session the
    path names, as `index`, `kind` and `bytes`, or 404 where none is open. The path stands under
    `/gateway/`, which no route of the child's takes.
  - The count rides on the bytes going back: nothing is held back for it and nothing is written down.
- **What holds for every entry:**
  - An OpenAI-compatible backend is asked at its chat completions path; left to itself the gateway asks
    the Responses API, which an engine's front may answer with a 404.
  - A parameter a backend does not take is dropped, and a failed request is not retried there: the run
    that asked reads the failure.
  - Thinking off where a request names nothing, and a request that names `enable_thinking` in
    `chat_template_kwargs` — a request with no tools', as its run says — is asked as it named it. Left to
    the template's own default a model may think for minutes even on a turn that asked for none, so the
    default is never the template's.
  - **The caller decides thinking on the Messages path, and the gateway carries it.** LiteLLM translates
    a Messages request's `thinking` into an OpenAI reasoning effort the template does not read, so a hook
    of the gateway's own rewrites a Messages request before it is routed: `thinking` of type `enabled` or
    `adaptive` becomes `enable_thinking` on, `disabled` becomes it off, and a request with no `thinking`
    keeps the table's off. The `thinking` field and the effort are taken out, the Anthropic form of a
    choice already carried, and the model's thinking comes back as the answer's thinking block, streamed
    or not.
  - **The system prompt passes as it stands.** What a run is told is composed before the program starts
    — its line's prompt, or the agent's base prompt with its rules (`docs/base-prompts.md`) — and no hook
    reads or rewrites it.
  - **A system turn inside the conversation is folded into the person's turn beside it.** Claude Code
    sends some of what it tells a run as system-role messages among the conversation's own, and LiteLLM
    turns each into a user message, since a backend's chat template takes a system message only at the
    head; standing after the person's words, such a message is the last thing the model reads, and a
    model may answer it by repeating it. So a hook rewrites a Messages request before routing: a run of
    system-role messages right after a user message becomes text inside that message, each wrapped in
    reminder tags, ahead of the person's words and after any tool results it opens with; a run with no
    user message before it goes into the one right after; a run between two answers is left to LiteLLM's
    own turning. The hook reads nothing but the request's own order, so the conversation sent again on
    the next request is folded the same way and the engine finds the state it saved.
  - **A run's sampling settings are carried from a header into the request.** Claude Code has no word for
    most of what the backend samples with, so a run that names sampling settings sends them on every
    request in a sampling header — a JSON object of the backend's own parameter names and numbers, with
    the chat template's words under `chat_template_kwargs` — and a hook writes each into the request body
    before routing, over the program's own: every numeric entry so named is written; the template words
    are laid over the request's own and those over its table entry's, word by word, since LiteLLM sends a
    request's template words in place of its entry's whole; which settings are sent and their bounds are
    the caller's (`docs/supervisors.md`, 「Its generation settings」), and the hook keeps no list. A request with no such header is
    asked with the program's own values.
  - **Past thinking goes where the template is told it goes.** A request whose template words name
    `preserve_thinking` off has the thinking of every answer before its last question taken out before
    routing, so the program's history carries no more of it than the template renders; a request naming
    it on, or naming nothing, keeps the history whole.
  - **A run's long tools are carried from a header into the request.** A run whose program names the
    tools that wait long sends them in a long-tools header (「How requests go」 below), and a hook copies
    the header's value into the request body as one string of comma-joined names; the engines on the node
    pool take the field out before the request goes on to the model server behind them. LiteLLM's own
    forwarding of a caller's headers is left off, since it hands the backend every such header with no
    way to name one alone and a caller's key may stand in one.
- **A request that names a role starts from that role's saved prefix** on an instance that keeps role
  files (「Role files」 below).
- **A saved change reaches it within seconds, and no request is cut:** the launcher reads the registry
  every few seconds. Where the table it would write differs from the one it serves, it starts a new child
  on a fresh port with the new table and waits until that child answers its readiness check; it then
  sends every new request to the new child, and stops the old child once the requests in hand through it
  have ended, however long they take. A change that leaves the table as it stands starts nothing.
- **What the console is told:** the launcher writes a small status file under the data root — which
  table it serves, whether a new child is starting, and whether an old one is draining — and the
  registered-models section says the gateway is settling while a new child is starting, and nothing once
  it serves.
- **A new table is a new child** because LiteLLM loads the models of a table only as it starts; it takes
  models in while running only from a database, which this gateway does not keep.
- **Why LiteLLM:** it is a translator and nothing more, serves both APIs from one table in one file, and
  runs as one process with no database.
- **Its own key:** the key file `llm-gateway.key` under the data root's `env/`, read once as the launcher
  starts. Without that file, or with one that is empty or not `0600`, the launcher does not stand. The key
  read then is the one the front checks and the one handed to each child it starts as LiteLLM's own key,
  so the two never disagree; a key given again reaches the gateway with its restart.
- **The front checks the key on every request,** whatever its path, before the request reaches a child
  or is told no table stands yet. A request must carry `Authorization: Bearer <key>`, the gateway's own key
  or a call key the store holds unrevoked; one with no such header, another scheme or another key is
  answered `401` with `WWW-Authenticate: Bearer` and an error in the OpenAI shape — an `error` object
  holding a `message`, the `type` `authentication_error` and the `code` `"401"` — and nothing of it is
  handed on. The gateway's key is compared in constant time, and a call key by its digest. The call keys'
  digests are read from the store on each beat, and a key the last reading does not hold is looked up in the
  store once before it is refused, so a key issued a moment ago is taken. A request on a call key is handed
  on under the gateway's own key, which LiteLLM's own check, standing behind, holds; one on the gateway's own
  key is handed on as it came.
- **A call key reaches the request routes and the model lists alone:** a request to any other route of
  LiteLLM's — making, changing or removing a model, a key, a team or a setting, its spend and its health
  reads — is refused 403 to a call key before it reaches the child, and stays the gateway's own key's.
- **The front names the payer to the child:** it sets `x-bmo-payer` to the account a call key is, or
  `platform` for the gateway's own key, in place of any the caller sent.
- **A person's call key reaches platform supply only on a reservation.** A request on a call key naming a
  model that is neither one of that person's personal members nor a member or registration they own is
  answered `402` with the code `no-supply` and the type `insufficient_quota` unless the account holds an open
  platform reservation in the store, read as the call keys are: on the beat, and once more on a miss. With no
  supply hook registered no reservation is ever opened (`docs/supply.md`, 「Choosing a source as a run
  starts」), so a call key reaches nothing of the platform's.
- **Each reservation opens what it names and nothing else.** A reservation naming a unit opens its account's
  runs of that unit's names and no others: it is no platform reservation, so a run of that account's on a
  platform name is still refused for want of supply, and a platform reservation is no unit's, so it opens no
  run of a unit's name (「An owned member or registration answers its owner alone」 above).
- **What a request costs is recorded.** A hook of the gateway's own, `costs`, reads the cost of each request
  that ends, answered or failed — the provider's own word where its answer's usage names one, as OpenRouter's
  does, else LiteLLM's price of it — with its model, its tokens, its payer, the run name its `x-bmo-run`
  header names and the program's session its `x-claude-code-session-id` header names, and hands it to the front at `POST /gateway/costs` under the child's own key, at the address
  the launcher hands the child as `LLM_GATEWAY_FRONT`. The front writes one row of `gateway_requests` for it:
  when, the account or none for the platform, the model, the run name and the session or none, the input and
  output tokens, and the cost in millionths of a US dollar, and answers 204, or 400 to a body that does not
  read. A route under `/gateway/` takes no caller's key but the gateway's own,
  so no caller writes a record.
- **How it stands:** the gateway's unit is linked into the user's systemd directory and enabled by
  the deployment, and carries the binary mark, so a deployment restarts it only where it placed a
  different `llm-gateway` binary; a deployment of agency that leaves the launcher as it stands leaves the
  gateway serving.
- **It drains on its stop.** Asked to stop by `SIGTERM` or `SIGINT`, the front takes no new connection and
  lets every request in hand end, however long it takes, as an old child is drained at a change of table;
  a stream whose bytes stop is still cut at the stall span. Only then are the children stopped and the
  launcher ends, so a restart cuts no request in hand.
- **A changed hook needs no restart:** the table's digest covers the hooks' contents, so the next
  reading of the registry starts a new child on them and hands over the way a changed registry does. The
  launcher reads the hooks from the tree it was started by, which for its unit is the deploy link, so a
  deployment that leaves the launcher running still brings it the hooks of the release it made live.

### Role files

- **What a role file is:** an instance of an engine that saves and restores its conversation state —
  a llama.cpp-style server's `POST /slots/0?action=save|restore`, a file named in a JSON body — keeps one file per role,
  the state of that role's system prompt read once. Restoring it is a fraction of a second where reading
  the prompt again is many; the file is named `role-<role>.bin`, the role's name being its whole key.
  Which files stand and whether one is still valid are the engine deployment's own: it builds them and
  builds them again when the prompt or a setting bound into them changes, so a file that stands is the
  role's. Agency never saves one.
- **Which requests name a role file:** the supervisor's runs send `chat` in the role header on every
  request they make through the gateway. A request naming no role file — a translation's among them, and
  every role's run — is asked as it stands. An engine deployment may build files under other names, such
  as `gate` or `session-summary`, which no run of agency's names.
- **Which instances keep them:** an address of a registration says so (「What a registration holds」
  above), and the gateway's table carries that on each entry the address stands behind. An address that
  does not say so is never asked to restore.
- **What the gateway does:** a hook of its own, run after the instance a request goes to is chosen and
  before the request is sent, keeps for each instance the role it last restored or served. A request that
  names a role on an instance not on that role has the instance restore `role-<role>.bin` first, under
  the address's key, and is then sent; a request that names none leaves the instance on no role, since
  what it reads replaces the role's state. Each restore is a line in the gateway's log, with the
  instance, the role and how long it took.
- **A restore that fails is a plain request:** a file the instance does not hold (`404`), another
  refusal or a refused or broken connection leaves the request to be sent as it stands, with a line in the
  log saying why, and that role is not asked of that instance again for ten minutes. What the hook keeps
  is the running gateway's: a new child starts with every instance on no role, and the first request of
  each role restores it.

### Session files

- **Why:** an instance that keeps role files holds one conversation at a time. A request of another session
  takes the place of the one it held, and that one's next request is read again from its first token, which
  on a long session is minutes. The instance parks conversations in its own memory and brings a parked one
  back by itself, but a long session is larger than what it parks.
- **What the gateway does:** the same hook keeps for each such instance the session it last served, as the
  program's session header names it. Before a request of another session, or of none, is sent, the instance
  saves the session it holds into a file; and a request whose session has a file on that instance has it
  restored first, in place of the role's file, since the session's state stands past the role's prefix. A
  save and a restore of a session near the context's end take a few seconds each.
- **Which sessions are saved:** one whose last request read at least 32,000 tokens, as the instance's own
  figures say when it is left. A shorter one is left to the instance's memory, and a file it had is no
  longer its own.
- **The files are a fixed few:** three for each instance, named `session-<n>.bin`. A further session takes
  the file least recently used, so the files' space on the node does not grow with the sessions served.
- **A save or a restore that fails is a plain request:** the session is left without a file, with a line in
  the log saying why, and its next request there is read as it stands. What the hook keeps is the running
  gateway's: a new child knows no instance's session and no file, and writes the files over as sessions
  come.
- **Requests that reach an instance at once** are sent in the order the instance takes them, not the order
  the hook saw them, so a session whose request waits behind another's may be read again once.

## Where a registration is named

- **A road on the registry's provider names one registration,** with the answer ceiling and the thinking a
  run on it goes out with, or none for the registration's own (`docs/provider-order.md`, 「What a road is」).
  A supervisor's or an agent's runs reach it where their road stands on the registry (「The registry on a
  road」 below).
- **A repository's pin may name one,** and every job in the repository runs on it (「A repository pinned to
  a registration」 below).
- **A session whose caller names one** goes out on exactly that registration, as a named road
  (`docs/provider-order.md`, 「Runs on another's road」).
- **A registration named is checked against the registry on saving:** one the registry does not offer is
  refused. One that has left the registry, or holds no address, by the time a run reads it is refused
  there, never replaced by another.
- **What shows it:** the console and the `bmo-agency agent` listing show each supervisor's and each role's
  order beside the provider and model its last run went out on.

## Limits, retries and walking

### Request limits and retries

- **Limits:** a request with no tools is limited in input size, output tokens, the byte count of success
  and failure responses, retry count, and time.
- **Retried:** only transient network errors, 408, 429 and 5xx.
- **The time is the request's own:** each attempt is waited for the time the request has left, and for
  no less. The runtime's own cut of a connection silent for minutes is switched off for it, since an
  engine reading a long prompt sends nothing for longer, and the gateway is told the time left
  (「The gateway」 above).
- **No body in the log** for a missing key or permission error, 401/403, timeout, overflowing response,
  empty answer, or malformed JSON.
- **A retry asks the same registration again;** it is not a move to another model.

### When a registered model does not answer

- **A run on the registry's provider that does not answer fails on its road,** as on any provider: no other
  provider or model is tried (`docs/provider-order.md`, 「A road that fails」, `docs/cache-fallback.md`,
  「Automatic walks only on providers, in order」).
- **A registration that answers it cannot serve now holds the job.** A 5xx from its gateway, a service bench's
  503 among them, or a refused connection is the registration's answer for the turn: the job waits on its
  road, its `heldFor` naming the answer, until the block that answer wrote opens, and goes out again keeping
  the session it had; nothing moves to another registration or provider.
- **A job in a repository that pins a registration does not move:** it waits until the registration can
  answer again (`docs/repositories.md`, 「A pinned model」).
- **A session whose caller named the registration** never moves off it; a run that does not answer there
  fails.
- **A provider the owner switched off** answers on no road, and a job on a pin to one of its registrations
  waits.
- **A job's turn goes out on one road** and is never moved to another.

### The registry on a road

- **The registry's provider is a provider like any other:** a supervisor's or an agent's road stands on it,
  on the registration the road names. The owner's switch on it governs every road on it and every repository
  that pins one of its registrations.
- **It always has quota:** a registered model runs on the deployment's own engine
  (`docs/usage-windows.md`, 「What reads a provider's quota」), so a road on it is answered whenever it can
  answer now, sharing the engine's slot with everything else on it.
- **How it answers there:** with the registration, the thinking and the answer ceiling the road names.
- **The rules:** roads and quota are in `docs/provider-order.md`, availability and the accounts in
  `docs/cache-fallback.md`.
- **Nothing to keep warm:** a registered model has no known cache lifetime, so it sends no keepalive and
  produces no cost or account window.

## A request with no tools on an engine

### How the services behave

- **One request is one HTTP POST.** There is no session and no prompt cache, so the request's system
  prompt is carried whole every time.
- **The request is its caller's alone:** its system message is the caller's instructions and its one user
  message the caller's input. Nothing of a conversation goes with it — no persona, no room context, no
  supervisor's settings — and it joins no conversation on the same registration.
- **An engine with one slot answers one request at a time,** shared by everything on it — supervisors'
  turns, roles' runs and every request with no tools. A request waits behind whatever holds the slot, a
  long answer being written holds it for minutes, and a request that takes the slot from a conversation
  leaves the front to save that conversation's state and load it back on its next request. The engine
  stays up between requests; one that finds it stopped waits for its start.

### A request waits for its answer

- **Its one ask waits for its provider however long it takes:** there is no clock on it. It ends when the
  provider answers, when the provider fails it, or when its caller stops it.

### Which road answered is recorded

- **One line per provider a run went out on,** in the road record.
- **What a line holds:** the supervisor or the role it ran for, room and message, provider and model,
  outcome (answered, timed out, size refused, error, none), the time taken, and the input, output,
  cache-read and cache-write tokens the run reported (empty where it reported none), and, on a line of a
  session's own run, the share of its context window that run ended carrying (empty on a request with no
  tools and wherever the session reported none). The room screen's context chip reads it.
- **What a registration reports:** the chat completions path answers with its token usage, and that is
  kept.
- **A supervisor's turn is a line,** under its supervisor, with the room and the last message the turn
  read, so a turn's figures are read from one record. A second asking the round makes of itself is a line
  the same way, under the name of the kind of second asking it is. A turn whose call failed still writes
  its line, with the outcome error and no tokens.
- **The round and the run:** a line written inside a round of a room's session names that round, so
  everything one answer cost is read by that name, and names the run its log lines are kept under; a line
  written outside any round names none.
- **How many lines:** one for each provider the run went out on; a request with no tools goes out on one,
  so it has one line.

## Gates are not agency's runs

- **A gate's run is the gate service's own.** No gate is a supervisor or a role (`docs/agents.md`, 「Two
  axes」): agency keeps no gate row, route, order, sampling profile or answer size, and chooses no provider
  for a gate.
- **What a gate is told, how it samples, how it splits a long text and what its caller does when it has no
  answer or is stopped** are the gate service's to state, not this document's.

## Input size

### The limit is the context

- **The model runs only within its context limit,** counting system prompt, prompt and answer together.
- **The limit is the context window of the request's registration** (「The context window」 above), read
  at call time.
- **Token-heavy text fills context fast:** some scripts are one or two characters per token.

### Sizing the answer

- **The answer slot** (the request's `max_tokens`) is sized to the input every time.
- **Estimating input tokens from characters:** a non-ASCII character counts as 0.8 tokens, an ASCII
  character as 0.3, which comes out slightly above what the service actually counts, so nothing overruns
  from an underestimate.
- **Which answer size:** for a request with no tools, the size its run names; for a supervisor's turn or a
  role's run, the answer ceiling its order's entry names on the registry's provider; and if nobody chose,
  a floor for a request with no tools and a larger floor for the rest.
- **Fitting the context:** if the slot is larger than what remains of the context it shrinks to what
  remains, and if too little remains the request is not sent.

### Too large

- **What counts as too large:** requests not sent because of size — an input with too little room left —
  and requests the model returned for size, a 400 for exceeding context.
- **A request with no tools that is too large** sends with the answer slot that remains, and if it is
  still too large ends refused for size; agency splits nothing, and its caller cuts the text smaller — a
  translation's caller is answered 413 `too-large` (`docs/api.md`, 「translations」).

### What does not split

- **Runs with tools** (a supervisor's turn, a role's run): the whole conversation is one request, so they
  cannot split; a turn that meets the limit ends as 「A turn at the input limit」 below says.
- **A run that fell over onto the registry** carries the prompt its run assembled, not a request with no
  tools, so it does not split either.
- **Answer slots** are sized as above. A supervisor's or a role's answer ceiling is its order's entry's on
  the registry's provider, and is the registration's answer size where the entry names none, never below
  the floor its own work declares: a work that writes a whole document in one tool call declares a floor
  that holds one. A run that thinks writes its thinking out of the same room, so a work may declare a
  larger floor for such a run, given as far as a fraction of the registration's window allows.
- **Only registered models limit input.** Claude Code, Codex and OpenRouter runs have no place where
  agency blocks by request size.

## Supervisors and roles on a registered model

### Choosing a registered model

- **The registry's provider carries every line but the browse:** the chat line every supervisor runs on,
  the worker line and the search line as a Claude Code road, and the translate line as requests with no
  tools (「A request with no tools」 above). A supervisor's or a role's order may name it on any of those
  lines; the browse line, which needs a browser the registration's window does not carry, stays off it,
  so the seeker role's order cannot name it (`docs/provider-order.md`, 「What a road is」).
- **Jobs:** a job goes out on its role's order, reaching the registry where the order names it, or on the
  registration its repository pins (「A repository pinned to a registration」 below). A manual job run, an
  item or a work order that names a model of its own names a registration as `registered::<registration>`.

### A repository pinned to a registration

- **A repository may pin a registration** (`docs/repositories.md`, 「A pinned model」): every job in the
  repository, whatever role it runs as, goes out on it instead of its role's order. A pin to a
  registration the registry does not hold, or one with no address and no card service, is refused.
- **Its answer ceiling and thinking** are those the role's order names on the registry's provider where
  its entry names that same registration, and the registration's own answer size and thinking default
  otherwise.
- **What runs on it:**
  - Every job of the repository runs on the pin alone, and its relaunch tries no other: a pin that cannot
    answer — its registration's engine down, the registry's provider switched off — makes the
    repository's work wait, and nothing falls back (`docs/repositories.md`, 「A pinned model」).
  - A job that names its own `<provider>::<model>` still asks that first.
  - A pinned job holds a slot of its role like any other run (`docs/role-slots.md`).
- **Its sampling is the repository's,** apart from any supervisor's on the same registration: a repository
  holds sampling settings of its own (`docs/repositories.md`, 「What a repository row holds」), and every run
  on its pinned registration names them (「How requests go」 below). They are the supervisors' generation
  settings, by the same names and bounds, less the answer ceiling, which stays the registration's
  (`docs/supervisors.md`, 「Its generation settings」); one left empty is the engine's own default. Only a run on a registration carries
  them, since only the gateway reads them; a run in a support worktree runs on its repository's; a save
  checks each against its bounds, refuses one out of them, and lets the next run carry the new values; it
  is set in the sampling block of the repository's settings, shown while its pin is a registration.
- **Where it is set:** the pin on the repository tab, which lists every registration beside the
  providers, or the repository routes (`docs/api.md`, 「repositories, prefix-checks and room-holdings」).
- **What shows it:** the repository's pin in the repository list, and beside it the provider and model its
  last job went out on.

### How requests go

- **A request with no tools** is one request to the gateway's chat completions path, asked in the
  agent's host (「A request with no tools」 above).
- **Runs with tools go to Claude Code,** the program every line was written for, pointed at the gateway
  under the registration's name (「The gateway」 above). The line is the one a Claude run of it takes — the
  chat line for a supervisor, a role's own line (「A run on the worker line is composed as on Claude」
  below). Its program carries the search line, and an apart session on that line goes out in a place
  of its own as a member road's does, its program composed as a worker's job on the registration is
  (`docs/api.md`, 「sessions」); the session's caller names the registration, since a registration's window
  decides whether the instructions it hands in fit at all. The road changes only what
  follows, each value read from the registration the run's model names as the run is started.
  - **The endpoint:** the Anthropic base URL is the gateway, and every model Claude Code asks for a call
    of the run's own — its Opus, Sonnet and Haiku defaults and its subagent model — is the registration's
    gateway name, with its
    node where the run is placed on one (「Nodes and sessions」 above), so nothing the run itself does
    asks a model the gateway does not serve and no request of the run's own session leaves its node.
  - **No login of the Claude road:** the run's records go under a configuration home of the
    registration's own under the data root, which holds no login, and no account's token or directory is
    handed over. It is the data root's since a session resumed across a deployment needs its record
    there, and the agents root is laid anew at every start of the bot. The key is the gateway's, from the
    key helper (「The keys」 above).
  - **The window and the answer:** the context-tokens variable is the registration's context window, and
    the output-tokens variable the answer ceiling the run's order's entry names on the registry's
    provider, or the registration's answer size where the entry names none (「A repository pinned to a
    registration」 above).
    Claude Code sizes a model it does not know at a large default window and answer, which a backend
    refuses.
  - **The fold point leaves the whole answer:** the auto-compact override is the share of the window the
    answer ceiling leaves (「A turn at the input limit」 below).
  - **No wait cuts a request:** the program gives a request up after a wait of its own, and its runtime
    drops a connection silent for minutes, and a node reading a long prompt keeps it silent far longer.
    So both are told the longest wait their timers hold, about twenty-four days, which no request
    reaches: a request ends when its answer comes or its connection is refused or broken.
  - **Its retries are Claude Code's own:** after a refused or dropped connection, a 408, 429 or 5xx. A
    retry reads the prompt again from its start. The gateway retries nothing. A new session its node refuses before any answer
    is placed on the next node (「Nodes and sessions」 above).
  - **The prefix stays still:** the attribution header is off, since the block Claude Code otherwise puts
    at the head of every request changes each time and defeats the backend's prefix cache. The traffic
    Claude Code sends to its own services beside the model is off.
  - **Thinking is an effort word:** a run with thinking on is sent `medium`, which Claude Code sends as
    `thinking` of type `adaptive`; one with it off is sent `low` with its thinking tokens at zero, so it
    names no thinking at all. The gateway carries either to the backend. Whether a run thinks is its
    order's entry's thinking where that names one, and the registration's thinking default otherwise.
  - **Sampling is the run's own:** a run that names sampling settings — a supervisor's pieces-shaped turn, from
    its generation settings, and a job on a repository's pinned registration, from the repository's
    sampling — sends them in the Anthropic custom
    headers, as the header the gateway writes into each request; a run that names none
    sends no such header, and the backend samples with what the program sent and its own defaults.
  - **The long tools go with every request:** the program's adapter names the tools whose calls wait long
    (`docs/agent-control.md`), and a run sends them in the Anthropic custom headers as the long-tools
    header, the names joined by commas, which the gateway writes into each request. An engine reads it to
    give up its cards at once once an answer calls one of them, and to keep them while a short tool runs.
    A program whose adapter names none sends no such header.
- **What comes back** is Claude Code's own record — its transcript in a pane, a supervisor's standing one
  or a role's run's own — read as a Claude run's is. An apart session's turn has each line of its record
  written into its run log as it is written; a request with no tools in the host prints one object at its
  end, whose answer text is all its log is given; a job's turn in the pane has its transcript written
  into the job's run log as the program writes it, so a long turn on a slow engine is never a job
  standing silent. Each is filed under the registry's provider with the registration's name as its
  model, and the money Claude Code works out for a model it does not know is not kept: a registered model
  costs none.
  - **A run that resumed a session is counted by its own figures.** The result's model-by-model usage is
    the session's running total restored as the session is resumed, while its usage beside it is the
    run's own, so a resumed run on a registration is filed under the latter, under the one model it
    names.
  - **What a kept session holds is read off its transcript.** The run's own usage sums every request of a
    turn that called tools, and the stream carries no request's count of its own on this road, so the
    context a session ends a run holding is its last request's, as the transcript under the
    registration's home records it, against the registration's window.
- **A request the backend refuses or cannot answer** ends the run failed with Claude Code's own line
  about it, whether for size (a 400) or anything else, and work that could not be done is never treated
  as success. A refused request is the run's own failure, never the road's (「A turn at the input limit」
  below).
- **A run naming a registration the registry no longer holds** is refused before it starts, with a line
  naming the registration; nothing stands in for it.

### No keepalive

- **The registered models run no keepalive.**
- **Nothing else is warmed either:** a supervisor standing on the registry's provider warms no earlier
  session of another provider, and nothing of a role is warmed on any provider.
- **Calls only for real work:** a registered model is called only when there is a real work request.

### Claude Code in the pane

- **The registry's provider declares Claude Code's interactive program over the gateway as its
  interactive surface,** and nothing outside the provider names a registration for that. A request with
  no tools stands no program; every supervisor's turn goes into its own program.
- **The top of its screen names the registration by its label:** the program's settings carry a model
  picker row for the model it is started on, the node placement included, labelled with the
  registration's label, so Claude Code shows that label where it would show the gateway's name and its
  node.
- **Which program is whose:** a Claude Code program standing in a pane is a registration's where its
  records home is that registration's, and the Claude road's own only where it is pointed at no endpoint
  of another's. So a supervisor's move between Claude Code and a registration, or from one registration
  to another, is a different program in the pane.
- **Who stands it:** a supervisor stands it in its head pane for as long as its turns go out on a
  registered model, and a role's run stands it in a pane of its own for the run (`docs/agents.md`, 「A run
  in a pane of its own」). A supervisor's turn and a role's run are each one turn the bot puts into it,
  and a person attached to the console types into it as into any Claude Code program.
- **A supervisor's turn goes on in its scope's session,** whatever its scope, on the
  node that session stands on: the supervisor's one session, or the room's where its scope is the room, a
  turn for another room then starting the program again on that room's session in the same pane
  (`docs/supervisors.md`, 「The session's scope」). A full session is handed over by the handoff this
  provider declares rather than folded (`docs/supervisors.md`, 「Its limit」).
- **A role's run goes on in its role's kept session:** where the role holds a valid session on the
  registration at the run's path — on its model, effort and login, on a node the registration still has
  — the run resumes it on its node (`docs/sessions.md`, 「A role's session」); otherwise it opens a
  session of its own, placed on a node as a new session is (「Nodes and sessions」 above). Claude Code folds
  the session as it nears the registration's window (「A turn at the input limit」 below). A person's lines
  typed after a turn go on in its session, on its node, while the run's pane stands.
- **Words mid-turn are taken:** words for the agent that arrive while a turn works go into that turn, as
  Claude Code's pane takes them on the claude-code provider; a registration's window carries them.
- **A line a person types into the supervisor's program is composed by the program, not the gateway:**
  the chat's submit hook hands it what a round of the owner's chat would carry, on this road as on
  Claude's, which never passes the gateway. The gateway rewrites no prompt.
- **What stays in the host:** a request with no tools is one request and one answer, asked in the agent's
  host (「How requests go」 above). An apart session on a runner line on a registration goes out in a place
  of its own (`docs/api.md`, 「sessions」), and every other role's run on a registration in a pane of its
  own (`docs/agents.md`, 「A run in a pane of its own」).
- **Nothing stands with nothing to do but a supervisor's program:** a role keeps no program between its
  runs, so no pane of a role or a repository shows a registration's program before a run comes.

### A run on the worker line is composed as on Claude

- **Every run on the worker line is handed one line, whatever its provider:** a run on a registered model
  is the worker line (`docs/launch-lines.md`) — the platform's harness opening and the role's prompt with
  the line's rules after it (the owner's standing rules, the rule against destroying the system and the
  repository's own where it works in one), or a base prompt with the same rules after it where one is
  chosen for the run (`docs/base-prompts.md`), the worker plugin and its skills, the worker's MCP server, the project and local setting sources, so the checkout's
  `CLAUDE.md` is read in, every tool the line shows, and the worker settings with their hooks, the job
  guard among them.
- **What the road changes is the model and the endpoint,** the gateway's address and key, the
  registration's name, window and answer ceiling (「How requests go」 above), beside the
  effort word and the records home every run on a registration has; **and the languages it writes
  results in,** where the registration names some: the line's rules carry the result-language piece,
  `prompts/agents/worker/result-languages.md`, filled with that list, right after the standing rules, so
  a session on the registration holds it in its prefix. The piece says that where the brief names a
  result language outside the list, every result written for a person is written in English, and that
  the person reads it in their own language through the translation shown beside it
  (`docs/worker-language.md`, 「A model that writes only some languages」). A registration naming none
  carries no piece. A run whose road is chosen before its model is known, a pinned repository's standing
  program among them, is composed on the model its pin or its role's order names, so the piece is there
  from its first turn.
- **Its subagents are its own registration's:** a subagent the run's program opens asks the gateway under
  the run's own gateway name, so it runs on the run's registration (「How requests go」 above).
- **Its first request** is the whole line — the tool definitions, the MCP server among them, the system
  prompt, and the opening lines that carry the checkout's `CLAUDE.md` and the skill listing — plus a
  job's prompt where there is one, read by the engine before its first byte. A job writes nothing to its
  log while its request is read, so it can stand quiet for that long (`docs/agent-control.md`, 「A job gone
  quiet」).
- **A window smaller than the line is not refused:** the run is started as on any registration. Claude
  Code sends its first request whatever its window says, and a model that holds it answers. Past the
  first step the conversation is already over the point Claude Code compacts at, so it compacts and goes
  on. Only a request past the model's own window is refused (「A turn at the input limit」 below).

### A turn at the input limit

- **Within a turn every request carries the whole conversation** — the prompt, every tool call and every
  result so far — and the tool definitions, so a turn that reads much grows towards the registration's
  window.
- **A request holds the conversation and the whole answer ceiling.** Every request names the answer
  ceiling as its `max_tokens`, and an engine refuses one whose prompt and ceiling together pass its
  window rather than cut either, answering a 400 that names the prompt's tokens, the ceiling and its
  context.
- **Claude Code folds before the limit:** it takes the window from the context-tokens variable and
  compacts its conversation as it nears it, as it does on the Claude road. A supervisor's session on a registration is
  handed over earlier, in a turn of its own that writes its state for the next session (`docs/supervisors.md`,
  「Its limit」).
- **Its own fold point keeps room for a bounded answer,** which falls short where the answer ceiling is
  larger, so a request carrying the whole ceiling could still pass the window.
- **So every run on a registration is told to fold at the share the ceiling leaves** (「How requests go」
  above). Claude Code folds at that share of the room it counts, or at its own point where that comes
  first, so a conversation is folded before it holds the window less the whole ceiling, and the room it
  keeps for its summary stands as the margin for its estimate.
- **At the limit the turn ends, failed, on the request's own refusal.** A request the backend refuses for
  size comes back as a 400 naming the size, Claude Code ends the turn on that error, and the turn is
  recorded as failed with it. A role's run on it is a run that did not answer, never a success.
- **A refused request is the work's failure, never the road's.** A 400, a 413 or a 422 is the backend
  answering, so a job whose turn ended on one ends failed with that line as its reason: its registration's
  road is not closed for an outage, and the run does not move on to the next provider of its order, where
  the same conversation would be refused again (`docs/cache-fallback.md`, 「Automatic walks only on
  providers, in order」).
- **The next turn is a new session:** a turn on the held session that ended without an answer lets that
  session go, so the words said next do not go on in it either.

## Settings of an order's registry entry

### What an entry holds for the registry

- **Three settings:** the registration, the answer ceiling and whether thinking is on
  (`docs/provider-order.md`, 「What a road is」).
- **Where:** the road of a supervisor on the registry's provider on the supervisor tab, or of an agent on the
  agents tab; the road's dropdown offers every registration by its name.
- **Who reads them:** every run of that supervisor or that role that goes out on the registry's provider.
  A job on a repository's pinned registration reads the pin's registration, and the entry's ceiling and
  thinking only where the entry names that same registration (「A repository pinned to a registration」
  above).
- **The entry may leave two of them to the registration:** an empty answer ceiling is the registration's
  answer size, and the default thinking choice is the registration's thinking default; the on and off
  choices override it.

### Answer size is the ceiling of the answer slot

- **Choices:** a small set of token sizes, or none.
- **None means the work's default:** a floor for a request with no tools, and for a supervisor's or a
  role's run on a registered model the registration's answer size.
- **The translator's own floor:** a translation never runs below a size that holds a part, whatever its
  entry says.
- **The rules of 「Input size」 above still hold:** a ceiling larger than what remains of the context
  shrinks to what remains, and with too little remaining nothing is sent.
- **So:** raising the ceiling leaves less room for a long input; lowering it shortens answers.

### Thinking is on or off

- **A request with no tools says it either way:** `enable_thinking` in `chat_template_kwargs`, on or off,
  as the run's thinking is.
- **The response** carries the thinking apart from the answer in `content`; agency uses only `content` as
  the answer, and keeps the thinking in neither logs nor records.
- **Thinking eats the answer's budget:** it is spent within the same `max_tokens` as the answer, at the
  same speed, so a request with thinking on takes longer to reach its answer and may spend its answer size
  before it does, and an entry with thinking on should have its answer ceiling raised too.
- **A run with tools** is Claude Code's, and thinking reaches it as the effort word 「How requests go」
  above names.
- **The record:** an answer to a request with no tools, with thinking on, is recorded with effort
  `thinking`; one with it off has no effort.

## No wait on a request

- **Every request is carried until it answers.** A request with no tools and each request of a run with
  tools (Claude Code on a registered model) is waited for until it answers or its connection is refused or
  broken, or its caller interrupts it: agency keeps no wait, no table of them and no default to fall back
  to.
- **The one exception is a stream whose bytes have stopped.** The gateway cuts a streamed answer that has
  sent no byte for its stall span, as the owner decided, since a stream that has stopped sending will
  never answer: a judgement that the peer is alive, not a bound on the work (「The gateway」, 「A streamed
  answer whose bytes stop is cut」).
