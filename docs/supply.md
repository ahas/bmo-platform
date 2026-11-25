# Supply

Who pays for a run, and on which credential it goes out. A road names the provider and the model a run goes
out on (`docs/provider-order.md`); a supply source is an axis inside that road: the same provider and the same
model, on a credential and at a payer of its own. A run never changes provider or model for its supply. Agency
measures what a run costs in US dollars at the provider's list price and knows nothing of plans or credits: a
consumer above it answers, through the supply hook, which sources a run may take and whether a source that the
platform pays for takes it.

## The sources

- **Personal supply:** the person's own credential, a Claude login or an API key of theirs, paid by them to the
  provider directly. It runs out when the provider refuses it: a window full, a key refused or a
  balance spent.
- **Organisation supply:** the credential of an organisation unit on the person's path, the keys and Claude
  accounts the unit registers (「Organisation supply」), paid by the unit to the provider and counted as
  organisation credits by the consumer. Its source word is `organisation`.
- **A platform source:** the platform's credential, the Claude logins marked lendable whoever owns them
  (「The lendable accounts」) and the gateway members and registrations no person and no organisation owns
  (「Owned supply」), paid as the consumer answers:
  the plan's included supply, an organisation's credits or the person's own credits are each a platform source
  of the consumer's naming. Every platform source goes out on the same credentials, so moving between them
  changes only who pays.
- **Stop:** nothing left. A run waits or ends saying so (「Running out mid-turn」).
- **The owner's and the deployment's own runs** — a run of no account or of the administrator's account
  (`docs/account-isolation.md`, 「The scope」) — go out as every run went before there were sources: on the
  platform's credentials, their cost recorded and reserved nowhere. Every rule below holds for the runs of the
  other accounts alone.

## What a person brings

| Road | A login | A key |
|---|---|---|
| Claude Code | a subscription login of the person's own, signed in on their node | an Anthropic API key, an account of the kind `api-key` |
| Codex | none | an OpenAI key, a personal member of the gateway |
| DeepSeek | none | a DeepSeek key, a personal member of the gateway |
| OpenRouter | none | an OpenRouter key, a personal member of the gateway |
| The registry's provider | none | a local AI of their own on their PC, an OpenAI-compatible address with an optional key (「A person's local AI」) |

- **A key is the person's secret in the console's secret store,** the file `users/<user>/<name>` under the data
  root's `env/`; agency is handed the name alone and reads the file only where a run or the gateway hands its key
  on. Its value never enters the store, a log line, a response or a command line.
- **A Claude login or an `api-key` account of a person is a Claude account whose `user_id` is that person**
  (`docs/cache-fallback.md`, 「The Claude Code road is a list of accounts」). A login is signed in on the person's
  node (「Signing a person's login in」 below) and kept in a configuration directory there.
- **A gateway key of a person is a row of personal supply:** the person, the road it serves (`codex`,
  `deepseek` or `openrouter`), the secret's name, its place among that person's keys of the road, an on/off and
  the moment it was found refused. The gateway serves it as a personal member of its own (`docs/own-llm.md`,
  「Personal members」).
- **Several on one road are taken in their order,** as a person's logins and keys were placed; one refused waits
  for its person, and the next is taken.
- **Only the person makes, sees, changes and removes their own,** through the routes the console calls with the
  person's own headers (`docs/api.md`, 「Who a caller acts for」 and 「personal supply」). The administrator sees
  of another person's supply only which roads it serves, and the Claude accounts list the administrator reads
  holds the platform's accounts alone.
- **A person off-boarded has their personal supply switched off:** each of their Claude accounts, keys and local
  AIs is turned off and their gateway call key is revoked. Their secrets stay sealed where the console keeps them.
- **A person removed for good has their supply taken:** their Claude accounts, keys, local AIs, reservations, call
  key and the gateway's records of their requests, and an organisation unit removed for good its keys and Claude
  accounts (`docs/api.md`, 「owners and organisation units removed for good」). Their secrets are the console's to take.

### A person's local AI

- **A local AI of a person's own is a model served where their runs stand:** an OpenAI-compatible chat completions
  address reached from their own PC — a server on the PC's loopback such as Ollama, LM Studio or llama.cpp's, or
  one on the PC's own network — its model's name, and a key where the server asks one, the person's secret by its
  name. Agency keeps it as a registration the person owns (`docs/own-llm.md`, 「A person's local AI」), so it answers
  that person's runs alone and the gateway refuses anyone else's request naming it (「Owned supply」).
- **It is personal supply:** a run of the person's on the registry's provider naming it — an agent of theirs whose
  road they set to it — goes out on it where their order reaches `personal` (「Choosing a source as a run starts」),
  as their own login or key goes out on its road. It is placed in their order wherever personal supply stands
  there, and is never a platform source or an organisation's.
- **It needs a PC of the person's own:** a node their record names, reached through the hub, that no other account
  shares (`docs/person-nodes.md`, 「Several accounts on one node」), since its address is reached from there. One
  added while the person holds no such PC is refused 409 `no-pc`.
- **Their organisation may forbid it:** the delegation chain's `supply.personal-local-ai`, a flag agency declares,
  narrowing down the chain and on unless a link turns it off, for the platform, units and the person, written by a
  unit's heads (`docs/prompt-layers.md`, 「The settings agency declares」). Where it stands off for the person, adding
  one is refused 403 `personal-local-ai-withheld`, its test answers `withheld`, and a run naming one they already
  hold finds no personal supply and ends `withheld` (「Why none is left」). The administrator's account is held to
  none of it. A check the account service did not answer is refused 503 `access-unchecked`.
- **Only the person adds, lists, switches and removes theirs,** through `/personal-models` (`docs/api.md`,
  「accounts, rest and usage」); one switched off is taken by no run and its address is carried by no link.

### Signing a person's login in

- **The sign-in runs on the person's node,** in the node's herdr session as the node's own user: the CLI's
  subscription sign-in with a configuration directory under the node's data root, named after the account. The
  address it prints and the code the person pastes travel as they do for the owner's (`docs/cache-fallback.md`,
  「Signing an account in from the console」), and the login it writes stays on the node.
- **The account names that directory and its node.** A run of the person on that node is handed the directory
  as its configuration home and no token; the account goes out nowhere else, and its usage windows are read off
  its turns' answers as every account's are.
- **The pane is read as herdr joins its soft-wrapped lines,** so an address wider than the pane reads whole, and
  the login is taken once the CLI says it took (`Login successful`), refused once it says it refused, and given up
  at the policy's bound otherwise.
- **Its credentials file stands on the node, not here,** so a login of a person's found waiting for them is
  opened again by their next sign-in, not by a file this machine reads.
- **A person signs their own login in again on their node,** by the account (`account`), where its login has ended
  or is about to: the CLI's same sign-in, run in the account's own directory there, so the login it writes stands
  where the account already names it, and the account's mark of a login waiting for its person is lifted once the
  CLI says it took. Only the person's own account is signed in so: another's, or one that is not there, is refused
  404 `unknown-account`; an `api-key` account, which keeps no login, 422 `not-a-directory`; and a login that stands on
  another node than the person's now, 409 `login-elsewhere`, since its directory is there and not on the PC the
  sign-in runs on: the person signs a new login in instead.
- **A person's new login is named by agency** where the start names none: `claude-NN`, one past the highest
  number among every account's names, the platform's and other people's among them, which the person cannot see.
  A name the person gives that another account holds is refused 409 `name-taken`.
- **A person with no node attached cannot sign in yet:** the start is refused 409 `node-not-attached` where the
  node's link is not heard now, and 409 `node-not-ready` where it has no ready workplace yet, as while it is first
  set up (`docs/person-nodes.md`, 「The workplace on a PC」).
- **The sign-in runs the workplace's own Claude Code,** `<root>/bin/bmo-claude` (`docs/work-nodes.md`, 「The
  workplace on a node」), never a `claude` the person installed for themselves. Where it does not stand yet — the
  node's setup record names no copy, or the pane finds none at its path — the start is refused 409 `no-cli`,
  saying Claude Code is not installed on the PC yet and is being installed, and the node's setup is started with
  `--install` as a unit of its own (`docs/person-nodes.md`, 「The workplace on a PC」), so a start a few minutes
  later finds it.
- **A new login that does not take leaves nothing on the node:** given up, refused, past its bound or never shown an
  address, its pane closes and the shell holding it takes the new login's directory away where the CLI wrote no
  login into it. A login signed in again keeps its directory whatever the sign-in came to.
- **A pane herdr cannot read is said as such:** a start whose pane herdr refused to read until the bound ends 502
  with herdr's own words, never as a CLI that said nothing.

## The sources a run walks

- **Which sources a person's run may take is the consumer's word alone:** the order the supply hook answers names
  them, in order — `personal`, the organisation's own supply (`organisation`) and the platform sources the consumer
  names — and agency walks it as it stands, shaping it no further (the web's `docs/accounts.md`, 「Supply」). Every
  setting that opens or closes a source — the supply policies, the monthly limit of organisation credits, and
  whether organisation supply stands open, the delegation chain's `supply.organisation` — is the consumer's, declared
  and judged there.
- **Where organisation supply stands in the order,** its units are those of the person's path, the nearest first,
  read off the delegation chain's answer for the person; the run takes the nearest of them that holds an open
  credential for the road (「Organisation supply」).
- **`GET /supply-sources`** answers the signed-in person what the order their runs walk holds — `personal`,
  `platform` (any platform source), `organisation` — whether a unit of their path holds supply of its own that is
  on, `personalOnly`, whether their own login or key is all that can carry their runs, `ownSupply`, whether they
  hold one of their own that is on, a local AI among them, and `localAi`, whether they may add a local AI of their
  own now — their chain allows it and they hold a PC of their own (「A person's local AI」) —, which the console reads
  to ask a new person for one before they start (the web's `docs/web.md`, 「The join page」). With no hook
  registered, or one that does not answer, the order is personal supply alone.

## Testing a source

- **`POST /supply-test`** answers whether a source of one road answers now (`docs/api.md`, 「accounts, rest and
  usage」), so the console can tell a person, or the administrator setting the platform up, that the login or key
  just added works before any run needs it. A road whose provider the owner switched off answers `off` first.
- **For a person it tests their own supply on the road,** as personal supply is taken (「What a person brings」):
  - on Claude Code, their own Claude accounts that are on: it answers where one's login stands — signed in and not
    ended, or an `api-key` account whose key file stands — `no-source` where none of theirs is on, and
    `login-needed` where every one waits to be signed in. A window full or a rest does not fail it, since the
    credential stands;
  - on a gateway road, their first open key of the road: one request of a single answer token through the gateway,
    under their own call key, naming the road's first member as their personal member (`docs/own-llm.md`,
    「Personal members」). The launcher lays that member on its next beat, so a gateway that does not serve it yet
    answers `not-ready`, to be asked again in a few seconds; a key the provider refuses answers `refused`;
  - on the registry's provider, a local AI of theirs (「A person's local AI」): the one the test names as its
    `model`, or one not saved yet that the test describes as `local` — its address, its model and its key's name —
    so the console tries it before it is kept. One request of a single answer token goes to the address from the
    person's PC, the link carrying it there (`docs/person-nodes.md`, 「A local AI on a PC」), with the key their
    secret holds. It answers `withheld` where their chain forbids personal local AI, `no-pc` where they hold no PC
    of their own, `unreachable` where their PC or the address on it cannot be reached, `refused` where the server
    refuses the key, and `failed` where it answers otherwise; a person's test naming neither answers `no-source`.
- **For the platform** — the administrator's call or the deployment's own — it tests the platform's supply:
  - on Claude Code, the accounts no other person owns, with those marked lendable (「Owned supply」 and 「The
    lendable accounts」), judged as a person's are;
  - on a gateway road, the road's first member owned by no person but the administrator, by the same one request
    under the gateway's own key;
  - on the registry's provider, the registration the test names, by the same request, a registration naming a
    card service going through the gateway's card as any request on it does; a registration with no address and
    no card service answers `no-source`.
- **Nothing is reserved or recorded as a run's supply by it:** no reservation is asked of the hook, no run of the
  ledger opened, no key marked refused and no `supply.changed` told. The gateway records its one request as it
  records every request (「The ledger」), under the call key it went out on.

## Organisation supply

- **An organisation unit owns supply as a person does:** a key for a gateway road, a row of the store's
  `personal_supply` naming the unit in place of a person, its secret in the unit's space of the console's secret
  store, `units/<unit>/<name>` under the data root's `env/`; and a Claude account naming the unit in place of a
  person, an `api-key` account its heads register on its Anthropic API key, or any account of no person's the
  owner gives the unit (`docs/api.md`, 「accounts, rest and usage」).
- **Only the people of its path take it.** A run of a person whose chain leaves organisation supply open takes
  the supply of the nearest unit of their path that holds an open credential for the road's provider: a Claude
  account of the unit's that can answer now on Claude Code, an open key of the unit's on a gateway road; the
  registry's provider has none. Nobody else's run goes out on it as organisation supply, the platform's own
  runs among them; a Claude account of the unit's that the administrator marked lendable goes out for others
  as a platform source (「The lendable accounts」).
- **It is reserved through the hook as a platform source is,** under the source `organisation`, the
  reservation naming the unit, so the consumer counts its cost and the gateway lets the person's call key reach
  that unit's members while it is open (`docs/own-llm.md`, 「Personal members」). A Claude account of the unit's
  goes out on the person's node with the run, as a person's `api-key` account does.
- **Running out:** a key of the unit's its provider refuses waits for the unit's heads, as a person's waits for
  them, and the run goes on to the next source of its order in a new session; a reservation of it whose extension
  is refused hands the run to no platform source, since that goes out on another credential, and the run takes
  its unit of grace and ends there. The next run asks its order afresh.
- **Its heads manage it** through `/organisation-supply/{unit}` (`docs/api.md`, 「accounts, rest and usage」): a
  person is taken as a head where the chain answers them the unit's link of `supply.organisation`, which it
  answers its writers alone, and a key is added only while organisation supply stands open for the unit: the
  platform's own link opens it for the unit and no link of its path closes it. The kind is the account service's;
  agency reads it and declares none of the supply settings.

## The lendable accounts

- **A Claude account goes out for another person's run only where it is marked lendable,** and the mark is
  the one switch: an account of no one's, a unit's or a person's own, the administrator's among them, is lent
  once it is marked and never otherwise. None is marked by default, and only the administrator marks or
  unmarks one; a person's call naming it is refused, their own account's mark among them.
- **Lending an account sends its login to the borrower's node.** A lent login goes out on the node the
  borrower's run stands on, its token for the nodes with it (`docs/person-nodes.md`), so marking an account
  lendable is the administrator's explicit choice to hand that login to whoever the supply order lets take a
  platform source, and the account's owner is not asked.
- **A lent login is never sent to a node several accounts share.** Where the borrower's node is one another
  account's record names too (`docs/person-nodes.md`, 「Several accounts on one node」), every program there runs as
  the node's one user and reads the others' places, environments and records, so a token placed there would be
  every sharing account's to take. So:
  - such a borrower walks no lendable account at all: a platform source on Claude Code is passed over, and a run
    left with no other source ends `shared-node` (「Why none is left」), telling the person to add their own login
    or ask for a PC of their own;
  - the links' pass, once a minute, lets go every program standing on such a node on a login lent to its
    account — the place's program file removed here, which the link says gone on the node, and the program there
    ended through the link — between that program's turns, once the end of the turn it last carried has reached
    its caller, so a token already placed leaves the node without a person's hand (`docs/work-nodes.md`, 「The link」). The configuration directory the setup marked for the login
    on the node holds no token, since a program signs in from its environment and writes no login there
    (`docs/work-nodes.md`, 「The logins on a node」);
  - the person's own login, signed in on their node by their own choice, and their organisation's are not lent
    and stay as they are.
- **Who borrows it is the supply order's:** a lendable account goes out only for a run whose order reaches a
  platform source and whose reservation the hook accepts (「Choosing a source as a run starts」), as every
  platform source does.
- **An account is never lent to its own owner.** The owner's runs carry it as their own supply, never as a
  platform source, so a platform source of theirs walks the lendable accounts of others alone.
- **Marking leaves ownership as it stands:** a lent account keeps its owner, and its owner's runs and its
  unit's people's runs take it as before. Giving an account to a unit, or back, leaves its mark as it is.
- **A lendable `api-key` account is never sent to a borrower's node,** as an endpoint account is not: its key
  stays on this machine.

## Owned supply

- **Each credential a road goes out on belongs to a person, to an organisation unit or to no one,** and the
  record of the credential says which:
  - a Claude account by its `userId`, or its `unitId` (`docs/cache-fallback.md`, 「The Claude Code road is a list
    of accounts」);
  - a gateway member by the `account` the members file names for it (`docs/own-llm.md`, 「The gateway's
    members」);
  - a registration by its own `account` (`docs/own-llm.md`, 「What a registration holds」).
- **What no person and no organisation owns is the platform's,** and a platform source goes out on it and on the
  Claude accounts marked lendable (「The lendable accounts」) alone. What a unit owns is its organisation supply,
  and a source of no one else's unless it is marked lendable (「Organisation supply」).
- **What a person owns is that person's personal supply:** their run takes it as personal supply, and nobody
  else's run goes out on it, as a platform source or any other, unless it is a Claude account the
  administrator marked lendable. The gateway holds to the same line on every request (`docs/own-llm.md`,
  「The gateway」).
- **What the administrator owns is the owner's own supply,** and the deployment's own runs go out on it as on
  the platform's: a run of no account or of the administrator's walks the administrator's Claude accounts with
  the platform's, and is sent on the gateway's own key, which reaches the administrator's members and
  registrations. Another person's run never does, save on a Claude account of the administrator's marked
  lendable, and a deployment run naming a model another person owns is refused by the gateway as any
  stranger's request is.
- **A model a person owns that refuses their run** is passed over as a refused personal key is (「Running out
  mid-turn」): the run goes out on the next source of its order after personal supply, and no key of theirs is
  marked refused for it, since none was used. A job that finds no source left waits for supply, and on the next
  beat of its wait asks the model it owns again.
- **Who owns a member or a registration is set by the owner** through its route (`docs/api.md`, 「providers,
  models and model files」), and taken away with null, which gives it back to the platform.

## Choosing a source as a run starts

- **A run whose account is a person other than the administrator** asks the supply hook for its order as it
  starts (「The supply hook」), then walks the order within its road (「The sources a run walks」):
  - **personal supply** is taken where the person holds an open credential for the road's provider: a Claude
    login or `api-key` account that can answer now for a Claude Code road, an open key of the road or a member
    of the road they own for a gateway road, a registration they own for the registry's provider, a local AI of
    theirs only while it is on and their chain allows it (「A person's local AI」). It goes out on that credential.
  - **organisation supply** is taken where the order holds it and a unit of the person's path holds an open
    credential for the road and the hook accepts a reservation for it first. It goes out on that credential.
  - **a platform source** is taken where the hook accepts a reservation for it first (「Reservations」). It goes
    out on the platform's credential: a lendable Claude account, the gateway's member no person owns, or a
    registration no person owns, under the person's call key. A model another person owns offers no platform
    source.
  - **with nothing left,** the run is stopped (「Running out mid-turn」), saying why (「Why none is left」).
- **The order is asked afresh for every run,** so a personal login that reopens is taken again from the next
  run, and the platform's rule of using the login in use to its end does not hold for a person's run.
- **With no hook registered, or one that does not answer,** the order is personal supply alone: the platform's
  supply is off for other people until the consumer answers for it, and a person's run never goes out on it
  without a reservation.
- **What the run went out on is its source:** `personal`, `organisation`, the platform source the hook named, or
  `platform` for a run that reserved nothing. It is kept on the run and its job (「The ledger」).

## The supply hook

- **The point is `supply`,** registered by the consumer with `PUT /hooks/supply` (`docs/api.md`, 「hooks」 and
  「Hook points」). Its registration outlives agency's restart: its address is kept in the store's settings and
  its token in a file under the data root's `env/`, never in the store.
- **Five asks,** each posted as `{ point, ask }` with the ask's `op`, answered under `answer`:
  - `order`: the run, its account, its provider and model, whether it is a job's or a supervisor's turn, and
    the job, the supervisor and the room where they stand. Answered with `sources`, in order, each `personal`,
    `organisation` or a platform source the consumer names.
  - `reserve`: the run, the step, the account, the source and the idempotency key. Answered with whether it is
    `accepted`, the `reservation` it opened, its `amountUsd` and `expiresAt`. The size is the consumer's.
  - `extend`: the run, the step, the reservation, the source, what the run has spent so far as `spentUsd` and
    the key. Answered with `accepted`, the reservation's whole `amountUsd` now and its `expiresAt`.
  - `capture`: the run, the step, the reservation, the source, the actual cost as `costUsd` and the key.
    Answered with `taken`. It may exceed the reservation; how far is the consumer's, at most one reservation
    unit. A reservation the consumer already let go, past its expiry, is taken as a charge of that amount.
  - `release`: the run, the step, the reservation and the key. Answered with `taken`.
- **A run is named to the hook by its supply key,** a string: its kind, `job` or `turn`, its number — the job's
  id, or the ledger run a supervisor's turn opened — and the moment it chose its source, joined by hyphens. A job
  and a turn of the same number, and a run chosen again after a restart, never share one.
- **Every ask that moves money carries its idempotency key,** `<run>:<step>:<op>`, the step a number that
  rises with each reservation, extension, capture and release of the run, so an ask sent again is taken once.
- **Agency never calls the account service by its own address:** the hook is the consumer's, and its default
  does nothing.

## Reservations

- **A platform source is reserved before the run goes out on it,** at the size the hook answers, and kept in the
  store: the run, the step, the account, the source, the reservation, its amount, what it covers so far, its
  expiry and its state.
- **The running cost is read while the run goes,** on the meter's beat (`policy/supply.toml`,
  `meter_seconds`): a gateway road's from the gateway's request records of the run's program, by the run name its
  launch sends, since the run began, a Claude Code road's from the status line's cumulative cost of the turn.
- **At 80% of the amount, or as its expiry comes within two beats, the reservation is extended** (`extend_at`,
  the fraction). An accepted reservation names its amount; one that names none is taken as refused. An extension refused hands the run over (「Running out mid-turn」).
- **As the run ends,** its reservation is captured with the actual cost and the rest released.
- **Capture and release go through an outbox,** the store's `supply_settlements`, one row an ask keyed by its
  idempotency key, sent until the hook takes it, as it is written and then every few seconds, the wait between
  tries doubling up to the policy's ceiling. A row the hook takes is let go.
- **A reservation that expired while agency was down** is captured as agency starts, through the same outbox,
  with the cost the store last recorded it covered: the meter writes what the run has spent on each beat.

## Running out mid-turn

- **Personal supply the provider refuses** — a 429, a 401 or a 402, read off the run's ending by the one failure
  reading (`docs/provider-order.md`, 「A road that fails」) — is passed over as a refused Claude account is: the
  key or login waits for its person, who is told once, and the same run goes out on the next source of its
  order, in a new session that opens with the line that it continues the work already in the checkout. A failure
  of personal supply blocks no road.
- **A platform source whose extension is refused** hands the run to the next platform source of the order that
  accepts a reservation. The credential does not change, so the session goes on; what it covered so far is
  captured to the source it left, and the next pays from there.
- **Where none accepts,** the run goes on for one more reservation unit, the amount its last reservation
  answered, and then its turn is interrupted through the session contract:
  - a job waits for supply, as it waits for a road, holding no slot, and asks its order again on each beat of
    its wait (`policy/supply.toml`, `wait_seconds`); it goes out once a source takes it;
  - a supervisor's turn ends with one sentence (「Why none is left」).

## Why none is left

- **A run that finds no source left names why,** by what kept the last source it tried from taking it, and the one
  sentence it ends with, and a job waits with, is the policy's for that reason (`policy/supply.toml`,
  `[words.closed]`), each saying what the person can do:
  - `no-order`: the order named no source — add a login or key of their own, or ask the platform's administrator;
  - `withheld`: the order named sources and the person's chain closed every one, or the run's model is a local AI
    of theirs their chain forbids (「A person's local AI」) — ask their organisation's administrator to turn one on;
  - `no-personal`: no open login or key of their own answers the road's provider — add one in their supply
    settings;
  - `own-refused`: the provider refused their own key for the road — add it again;
  - `no-organisation`: no unit of their path holds an open credential for the road — ask their organisation;
  - `organisation-refused`: the hook refused a reservation of the organisation's supply — its credits ran out,
    with when they reopen where the hook names a moment;
  - `no-lendable`: no Claude account is marked lendable (「The lendable accounts」) — add their own Claude login, or ask
    the platform's administrator; no credits ran out, so nothing says to top up;
  - `shared-node`: the person's node is shared with other accounts, where no login is lent (「The lendable
    accounts」) — add their own Claude login, or ask the platform's administrator for a PC of their own;
  - `owned`: the run's model is another person's own — choose another model;
  - `refused`: the hook refused a reservation of a platform source — the source ran out, with when it reopens where
    the hook names a moment, and to top up or change the supply order;
  - `ran-out`: the source the run went out on ran out mid-run and no other took it.
- **The reason is carried, not only its words:** the `supply.changed` fact names it as `why`, and the line the room
  hears names it, so the console says it in the reader's language (「The facts told」).

## The ledger

- **A run's and a job's cost and source are kept:** `cost_micros`, the cost in millionths of a US dollar summed
  over the models that answered, and `supply_source`. A Claude Code turn's cost is the status line's
  `total_cost_usd` the turn added; a gateway road's is the sum of its request records.
- **The gateway records every request it carries:** the account its call key is, none for the platform's key,
  the model, the run name and the program's session its request named, its tokens and LiteLLM's cost (`docs/own-llm.md`, 「What a request
  costs」).
- **Usage groups by person and by source:** `GET /usage` with `groupBy` `account` or `supply` (`docs/api.md`,
  「accounts, rest and usage」).

## The facts told

- **`supply.changed`** carries a person's run moving to another source (`supply-moved`) or finding none left
  (`supply-closed`), published once each after agency has acted (`docs/api.md`, 「Events」).
- **The person's own supervisor room is told, and the telling never opens a turn.** Each fact goes through the
  receiver's instruction route by the outbox to the room the run answers, for its supervisor, as a row that rides
  that room's rounds for good (the receiver's `docs/bmo-window.md`, 「Rows that ride」): while no source is left a
  turn of that supervisor can only fail, so a fact that opened one would only fail it. The supervisor reads it at
  the next round that goes out. The owner is not told.
- **No source left is heard in the room at once.** No round goes out while none is left, so its instruction
  carries the receiver's fixed line `supply-closed` with `why`, the reason (「Why none is left」), and `until`, the
  epoch second the source reopens where the hook named a moment (the receiver's `docs/codes.md`, 「Fixed lines」):
  the receiver says it into the room as its own line as it writes the row, and the console draws it as a system
  line in the reader's language. It is said as often as the fact is told, once while it stays so.
- **No source left is told once while it stays so.** It is kept in the outbox under the supervisor and the room;
  while it stands, a later run of that supervisor there that finds none left tells nothing again. The first run of
  that supervisor in that room that goes out on a source takes it away, withdrawing the row where no round has
  taken it yet, so the next time none is left it is told afresh.

## What stands and what is left

| Part | Stands | Left |
|---|---|---|
| call keys and request records | a call key per account on its nodes, the platform key here alone, each request recorded with its account, model, run and cost | — |
| personal supply | personal members, `api-key` accounts, a person's own accounts and keys through their headers, the sign-in on their node, a person's local AI on their PC under the chain's `supply.personal-local-ai`, the test of a source, a local AI's tried from the person's PC before it is kept, the off-boarding switch | — |
| owned supply | a Claude account's, a gateway member's and a registration's owner, the walk and the gateway holding to it, the administrator's owned supply carrying the deployment's own runs | — |
| organisation supply | the three source settings on the chain, the order's shape, a unit's keys and Claude accounts, the walk to the nearest unit, its reservation and the gateway's members of it | a Claude subscription login of a unit's own, signed in from its screen |
| sources and reservations | the hook, the walk at a run's start, reservations, extension, the settlement outbox, the capture after a restart | — |
| running out | the personal pass-over, the platform hand-over, the grace unit, the job's wait, the supervisor's sentence for each reason none is left and the room's line saying it | — |
