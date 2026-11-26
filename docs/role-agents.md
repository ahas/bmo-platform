# A role's agents

A role says what a kind of work is: its prompt, its tools, its skills, its line and how its sessions are kept.
An agent is one seat of a role: a row that names its role and the one road its runs go out on. This document
states what an agent holds, how a run comes to an agent, and how the agents are changed. What a role itself
holds is `docs/agent-roles.md`; how a run waits for its turn is `docs/role-slots.md`.

## What an agent is

- **An agent is one seat of one role.** A run of a role is let in on one agent of that role and on no other
  thing: the agent it is let in on is the seat it holds, and an agent busy with one run takes no other
  (「When an agent is busy」 below). A role with three agents has three of its runs at work at once, and a role
  with none runs nothing.
- **An agent has a home node.** Its runs go out there: the machine that carries the session of the agent,
  chosen by the administrator from the nodes it may place agents on. None names this machine, the
  deployment's own default; a named home is a hub node the node service keeps (docs/person-nodes.md,
  「Which node is a person's」). A person's agent only stands on a node its account stands on, which the
  binding of the person's machine gives; nodes no account stands on — the administrator's machine among
  them — are the platform's own and take an agent only the platform's call writes home to them. A node lends
  its place and no more: it neither owns the agent nor judges what the agent may run, and a place that
  cannot be given ends the run with the reason rather than leaving it to wait.
- **An agent's road is fixed.** It names one provider, one model of it and the effort, thinking and advisor
  its runs go out with there. Its runs go out on that road and never on another: an agent whose provider cannot answer
  now, or has no quota, takes no run until it can, and no run of its moves to another provider. An agent that
  follows its asker is the one exception (「An agent that follows its asker」 below).
- **An agent that names no road follows the supervisor that asks it.** Its run goes out on the asking
  supervisor's own provider, model and effort as they stand when it is asked; it takes no run that names no
  road and that no supervisor asked. It also takes a run whose ask names a road no agent of its role stands
  on — an item's model, a repository's pin — on that road. A role all of whose agents name no road refuses
  a run that names no road and that no supervisor asks, with why. The coder's agents are seeded so
  (`docs/role-slots.md`, 「A role that follows its supervisor」).
- **A role holds no order, no cap and no forced provider, and its agents stand in no order of priority.** How
  many of a role's runs go at once is the number of its agents that are switched on, and which road a run takes
  is its agent's. Which agent does a job's work is named by the supervisor that orders it, or, where its item
  names a model and a role instead, taken by the agency at the launch from that role's free agents on that model
  (「Which agent a run takes」).
- **An agent is a seat of an account or of an organisation unit** (`STRUCTURE.md`, 「Accounts, companies and
  units」): `ownerAccount`, the id of the login account whose seat it is, or `ownerUnit`, the unit whose seat it
  is, or neither for the deployment's own; never both (`docs/api.md`, 「Who a caller acts for」). A run is let in
  only on the agents of its own account — the account of the supervisor that asks it, or none — so one account's
  runs never take another account's seat, and an account's settings of a role are its own agents' prompts,
  bundles and skills laid over the role's preset. A unit's seat is added, changed and removed by that unit's
  heads alone. A unit's seat carries no run yet: letting a unit's people's runs take its seats stands with the
  unit's supply. The role itself, its preset, is the deployment's.

## What an agent row holds

- **Kept in agency's store,** the `role_agents` table (`docs/store.md`), one row an agent:
  - **its owners,** `ownerAccount`, the login account whose seat it is, or `ownerUnit`, the organisation unit
    whose seat it is, neither for the deployment's own and never both;
  - **its name,** its key, kebab-case, which a screen and a log show the agent by; an agent added with no
    name is named after its role and a number, `coder-01`;
  - **its role,** one the store holds;
  - **its road:** `provider` and `model`, both or neither, with `effort`, `thinking` (`on`, `off` or the
    provider's own), `answerTokens` and `advisor` (the model its program may consult, or none;
    `docs/provider-order.md`, 「The advisor」), each where it is set;
  - **whether it follows its asker,** `followsAsker`: true or false, or none where its role's setting stands
    (「An agent that follows its asker」 below);
  - **whether it is switched on,** `enabled`; an agent switched off takes no run and counts for nothing;
  - **whether its kept session is kept warm** between runs, `keepsCache`: the session its last answered run
    kept is pinged before its cache cools, so its next run there resumes it warm
    (`docs/keepalive-schedule.md`, 「An agent's kept session」);
  - **whether its runs keep their sessions,** `sessions`, `keep` or `fresh` (`docs/agent-roles.md`, 「A
    role's sessions」); an agent added with none named keeps them;
  - **its compaction threshold,** `foldRatio`, the share of its model's window a run's session may fill
    before it is folded; where it sets none, the ratio its provider declares;
  - **what it lays over its role's preset,** each none where the role's own stands: `prompt`, which then
    stands in place of the role's prompt; `bundles`, which then are the bundles its runs are given in place
    of the role's; and `skills`, the names of the skills its runs carry in place of the role's.
- **A role's preset is the default of every agent of it.** A run reads its agent first and its role for
  whatever the agent leaves unset, so an agent that sets nothing runs exactly as its role says.

## An agent that follows its asker

- **An agent may follow the supervisor that asks its run while it names a road of its own.** A run of it that a
  supervisor asks goes out on that supervisor's own provider, model and effort as they stand when it is asked, as
  an agent that names no road does; the agent's own road is its fallback, named and kept on the agent, and the
  run goes out on it where no supervisor asked, where the asking supervisor names no road, or where that
  supervisor's road cannot answer now. The asker's road is taken first, and only a run that road cannot take
  goes out on the agent's own. A run let in on its own road in place of its asker's is logged once as it is let
  in, naming the agent's own road it went out on and the asking supervisor, its road and the sentence that road
  answered.
- **The setting is the role's, and each agent's own stands over it.** A role says whether its agents follow
  their asker, `followsAsker` on the role, false unless set; an agent says it for itself, true or false, or
  none for the role's. An agent that names no road follows its asker whatever either says, and has no fallback.
- **The supervisor a run is asked for is the one its slot names as `askedBy`** (`docs/role-slots.md`, 「Who
  takes a slot」). A caller that names none, a unit asking for its own work among them, has its run go out on
  the agent's own road.
- **What a run of such an agent goes out with on each turn** is the asking supervisor's entry on that
  supervisor's provider and the agent's own entry on any other, so a run that fell back to its own road keeps
  that road's model, effort and thinking.
- **A job of a role whose agents follow their asker** walks the asking supervisor's road first and its agents'
  own roads after it (`docs/repositories.md`, 「A job」).
- **The transliterator follows its asker** (`docs/agent-roles.md`, 「The translator and the transliterator
  roles」): a narration's spoken words go out on the road of the supervisor whose narration asks them, and on
  its agents' own roads where none is named.

## What a run reads off its agent

- **A run finds its agent by the slot it holds.** A turn of a role's session takes the seat of the slot its
  session was started on (`docs/api.md`, 「sessions」), whatever folder it works in, so a slot asked with no
  path still runs as its seat, on its seat's account and node; everything the turn reads off its agent, below,
  reads that seat. Any other run takes the seat of the slot request of its role that stands
  active at the folder the run works in — a job's checkout, a worker ask's folder, a place — the newest
  where more than one stands there. A run that names no folder, a translation among them, takes the seat of
  its role's only active request. A run whose seat neither tells stands on its role's preset alone.
- **Its prompt:** the agent's `prompt` stands in the place the role's prompt takes in the run's system
  prompt (`docs/agent-roles.md`, 「A role's prompt」). A system prompt is a word of the run's command, so an
  agent with a prompt of its own keeps a session of its own (`docs/sessions.md`, 「A role's session」).
- **Its bundles:** the agent's `bundles` are the bundles the run's token is issued with and its session's
  record carries, in the place of the role's (`docs/agent-roles.md`, 「Where the role is read」); the line's
  own capabilities stand whatever the agent names.
- **Its skills,** on the worker line: the agent's `skills` where it names them, else its role's `skills`
  where the role names them, else the whole plugin the release places for the role. A list is handed as a
  plugin of its own, a folder under the agents root — `role-agents/<agent>/plugin` for an agent's list and
  `role-skills/<role>/plugin` for a role's — holding the manifest of the role's plugin and the named skills
  alone, each from the role's plugin where that carries it, else from the first other plugin of the release
  that does. The folder is written whole whenever what it should hold differs from what it holds — the list
  changed, or a release moved a skill's text — and stands at the same path run after run. A named skill no
  plugin of the release carries any longer is left out and said in the log. A list that names none hands
  the run a plugin with no skill.
- **Its attached skills and MCP servers,** for an agent of an account (`docs/person-nodes.md`, 「Attaching skills
  and MCP servers」): each skill of the account's attached to the agent is added to the skills above, copied from the
  account's skills at home, and the plugin is then the agent's own, `role-agents/<agent>/plugin`, whichever list
  its role's skills came from; a skill whose name the role's skills hold already is left out and said in the log.
  Each MCP server of the account's attached to the agent is reached by its runs, and every other one is withheld
  from their records. A line whose carries take neither (`docs/launch-lines.md`, 「What a line carries」) carries
  none of them.
- **Its sessions:** the agent's `sessions` says whether the run resumes the session the last answered run
  left at its key and keeps its own (`docs/sessions.md`, 「A role's session」).
- **Its compaction threshold:** the agent's `foldRatio`, where it sets one, is the share a run's kept
  session is folded at as the run ends; where it sets none, the ratio its provider declares
  (`docs/sessions.md`, 「Session compaction」).

## Which agent a run takes

- **A job names its agent, or the model and the role it runs on.** Its item names the agent with its `agent`
  label, or a model with its `model` label beside its `role` label, set by the supervisor that files and
  launches it from what it has learned of the work and of its agents, or by the owner's own word; a job whose
  item names neither is refused at its launch (`docs/repositories.md`, 「An agent an item names」). Where the
  item names an agent, nothing in the agency picks one; an `agent` label stands over a `model` label.
- **A job whose item names a model takes a free agent of its role on that model.** Its candidates are the
  agents of its role that are switched on, of the account of the supervisor that launches it or of a unit of
  that account's path (「A unit's seats run their people's work」 below), and whose road
  runs that model. At the launch the agency takes the first candidate by name that is free (「When an agent is
  busy」), in the same step as it writes the job: every candidate's row is held while they are read and the job
  written, so two launches at once never take one agent. Where every candidate is busy the launch is refused,
  and where the role holds no candidate at all it is refused with the agents and roads the role holds; the job
  is never let in on another agent in its place.
- **A run that names an agent takes that agent alone.** An ask naming an agent of the role by name is let in
  only on it, on its own road, and waits for it however long it is busy or its provider cannot answer;
  it is never let in on another agent of the role. Every job asks so, the agent its launch took for a model as
  much as one its item names, and a job's launch is refused while its agent is busy (「Agency starts no job by
  itself」 below).
- **A job that moves to another agent, model or session carries its handoff.** The session it leaves writes
  what is done, found, under way and left, and the next session, a relaunch of its item on another agent
  included, opens on it (`docs/sessions.md`, 「The handoff a job's session leaves」).
- **An agent carries one job from its first turn to its end.** The seat a job's first turn is let in on stays
  the job's through every turn after it and every wait between them — its review, a stage passed and the next
  handed over — and is given back only as the job ends, stopped, failed or done, or as its review pauses it
  (`docs/job-review.md`, 「The owner's word」). Its next stage, asked by its supervisor's review, runs on that
  seat at once, before any other run of the role (`docs/role-slots.md`, 「The line」).
- **Agency starts no job by itself.** A launch naming an agent that is busy, or naming a model whose every
  candidate is busy, is refused at once (`docs/repositories.md`, 「An agent an item names」), never queued to
  start later, and no turn of work goes out unless a supervisor's launch or review move asks for it now. When
  the agent, or one of the candidates, comes free its supervisor is told, and the supervisor launches
  (`docs/agent-control.md`, 「An item's change is told to its orderer」).
- **A run that names no agent is let in on any agent of its role that is switched on, that its account may
  take, is free, and whose road can answer now,** taken by name where several are free; the agents hold no
  order of priority, and a unit's seat stands in the pool beside the account's own. A run a
  supervisor asks may take an agent that names no road, or one that follows its asker, on that supervisor's
  road, and a run no supervisor asks passes the agents that name no road over and takes one that follows its
  asker on that agent's own road.
- **A run that names a road takes an agent of that road alone.** An ask naming a provider and a model, or a
  model alone, is let in only on an agent whose road is that one, and waits for such an agent however long
  it takes; where no agent of the role stands on that road it is let in on an agent that names none, and
  where the role holds no such agent either it is refused with the roads the role's agents hold.
- **A translation holds no seat.** It is asked of no slot and waits in no line: it goes out on the first road of
  its role's agents that can answer, with the effort and advisor of the agent that road was taken from — the
  first agent standing on it that carries no run, else the first that does (`docs/launch-lines.md`, 「Translate
  runner」). A second agent of the translator role on the same road as an earlier one is never the one a
  translation goes out as, so its effort and advisor reach no translation.
- **A run waits where no agent of its role is free for it,** in its role's line, in the order asked
  (`docs/role-slots.md`, 「The line」). Where the only agents free are ones whose provider cannot answer, the
  request says why it waits, with the provider's own sentence.
- **The slot request names its agent,** as its `seat`, from the moment it is let in, with the provider, model
  and effort it goes out on, and keeps them to its end.
- **A reservation holds seats of a role by count,** not agents by name: a reserved seat is one fewer agent a
  run may be let in on, and the request that takes the reservation's place is let in on any agent then
  free.
- **A unit's seats run their people's work.** A seat filed under an organisation unit is let in on the runs
  of the accounts whose path stands on that unit — the unit's own people and those of the units below it —
  as a unit's supply is taken by them (`docs/supply.md`, 「Organisation supply」). Its road stands as any
  agent's, its runs are the account's runs for the line and the cap (the account's cap counts the seat, and
  the seat holds one run at a time), and its filing, changes and removal stay the unit's heads alone
  (`docs/api.md`, 「Who a caller acts for」). The run stands under the seat: it carries the unit's filing on
  the ledger row, is handed the unit's space of the console's secret store
  (`docs/tools.md`, 「The owner and the runner a record names」), keeps no seat's composition settings, and is
  composed as the run of an agent of no account is — its role's prompt alone, since none of the asker's own
  agent settings stand over it (`docs/prompt-layers.md`, 「A role's agents of an account」) — for
  it is the unit's thing, not any person's. A run asked for no
  account, the deployment's own, takes no unit's seat; neither does an account whose path does not stand on
  the unit, whatever else it may read of it.

## When an agent is busy

- **An agent is busy while a run it carries is at work.** It carries a run while a slot request let in on it
  stands active, and any run but a job's is at work for as long as it stands.
- **A job is at work except while it stands in the machine's turn,** in one of four states and in no other:
  - testing, or waiting to test: its checkout holds a lane of the testbed's turn or its repository's close
    line, or waits in line for one, as the testbed's own record names the checkout's root;
  - deploying, or waiting for its deployment: its close has handed its commit over and waits for it to go
    live, the deployment of that commit under way or not yet begun, whether the close still runs in the job's
    turn or the turn has ended under review.

  Writing, reading, a review round, a question waiting on the owner and every other state are work, and keep
  its agent busy as long as the job stands.
- **An agent whose every job stands in the machine's turn is free for another job.** The queue's notice offers
  it its next awaiting item, one whose checkout is free (`docs/agent-control.md`, 「An item's change is told to
  its orderer」), and a launch onto it goes through. The new job is a session of its own in its own checkout,
  on the same agent's road.
- **Each job keeps its agent, its seat, its session, its role and its road throughout.** When a job's test
  ends, or its deployment ends not live and it goes back to work, it simply goes on, so an agent may carry two
  jobs or more at once; it is offered nothing new until every job it carries stands in the machine's turn
  again. A job standing in the machine's turn counts against no cap (`docs/role-slots.md`, 「The cap」).
- **Which jobs stand in the machine's turn is read, never guessed** from a job's tool lines or its command
  words. Agency reads it every few seconds (`policy/machine-turn.toml`): the testbed's lanes as
  `bmo-testbed status --json` prints them, each holder and waiter matched by its checkout to the job working in
  that checkout or under it; the job service's items carrying the label `close-phase`, under a close in any
  repository; and, for a running job whose item's close is testing, the close line the same status prints run
  in the job's checkout.
- **An item carries a pair of labels for each repository its work was closed in** (the job service's
  `docs/cli.md`, 「`bmo-job job close`」): `close-phase:<repository>`, which the close writes `testing` while it
  checks and `waiting-batch` once its commit is merged and waits for that repository's batch deployment, and
  `close-commit:<repository>`, that commit. The repository is the name its testbed declaration gives. The pair
  `close-phase` and `close-commit` with no repository sums them up, and is read alone only on an item that
  carries no repository's pair, where `waiting-live` stands for a commit an older close left. A label alone
  marks no job, since one outlives the close that wrote it: `testing` in any pair only sends agency to the
  close line, and every pair reading `waiting-batch` or `waiting-live` sends it to the deployment of the commit
  that pair names. The job stands in the machine's turn while any of them is not live.
- **A handed-over commit waits while its repository's deployment says it is not live.** Agency reads the
  repository's deploy root from the `[deploy]` table of the testbed declaration in a checkout of that
  repository, and under it the deployment record, which names the commit that runs, and the progress record,
  which names the run under way or the last one ended, with its outcome (the testbed's `docs/deploy.md`, 「The
  places」). The checkout is the job's own where its declaration gives that repository's name, else the base
  checkout of the repository row on this machine that goes by that name: the name the declaration in its base
  checkout gives, else the row's own (`docs/repositories.md`). The summing pair is read in the job's own
  checkout. The commit no longer waits once the record's commit is that commit or one that holds it, as git
  answers in that checkout, and no longer once a run of that very commit ended other than live, since the close
  then sends the job back to work. A repository with no checkout here, a checkout with no deploy root, a record
  that does not read and a git that cannot tell leave that commit waiting no longer.
- **Each job's mark is kept on its row until the next reading.** A reading that fails marks nothing, so a job
  no reading names is at work.

## Changing the agents

- **`GET /role-agents`** lists every agent, by role and name, each with every field of its row and the run
  it carries now, or none; `role` narrows it to one role's.
- **`POST /role-agents`** adds an agent to a role: `role` required, every other field optional. It refuses a
  role the store does not hold, a name out of the pattern or already taken, a provider agency does not
  carry, a model its provider does not hold, a provider without a model or a model without a provider, a
  bundle the policy does not declare, a skill no plugin of the release carries, and an advisor its provider
  does not take or its model is not paired with (`docs/provider-order.md`, 「The advisor」).
- **`PATCH /role-agents/{agentName}`** changes any field of the row, each judged as the add judges it; a
  field set to null is cleared, which for an override gives the role's own back.
- **`DELETE /role-agents/{agentName}`** removes an agent, and is refused 409 while it carries a run.
- **An account removed for good takes every agent seated for it,** each slot request of the account given back first
  and the role session bound to it ended, so no run is left carrying one (`docs/api.md`, 「owners and organisation
  units removed for good」).
- **A change holds from the next admission.** A run already let in finishes on the agent as it stood when
  it was let in; an agent switched off or removed while it carries a run takes no further run.
- **A role's own routes carry no road.** `POST /roles` and `PATCH /roles/{roleName}` take no order, no cap
  and no forced provider, and refuse one named; `GET /roles` answers each role's agents beside it.

## The agents an account is given

- **A setting of the delegation chain,** the kind `signup.composition`: a JSON object, `nearest`, held at the
  platform's link, a unit's and a person's, a head writing a unit's and a person's, a lock keeping the links
  below from changing it. Agency declares it with its other kinds (`docs/prompt-layers.md`, 「The settings
  agency declares」) and judges and writes its value itself.
- **A composition names roles under `roles`,** each with `off`, whether the people under the link may not use
  the role, and `agents`, the roads of the agents a person is given of it, each a `provider`, a `model` and an
  `effort`, or none of them for the road of the role's agent of no account, at most the policy's number of them (`policy/organisation.toml`). A role named nowhere stands as the
  platform's own rule gives it: one agent on the road of the role's agent of no account that is on, none where
  the role holds no such agent. With nothing set anywhere, every person is given what that rule gives.
- **What stands for a person:** a role is off where any link of their chain still read says so; its agents are
  those of the nearest link naming them. A lock cuts the chain as for every kind.
- **`POST /composed-agents/{ownerAccount}`**, the deployment's own call, makes them as the account is made
  ready (the web's `docs/web.md`, 「The join page」): for each role the person holds no agent of and that does
  not stand off, the agents it names, named `<role>-u<user>` and then numbered, each carrying the settings of the
  role's agent of no account where there is one and on the road the composition names, else on that agent's. A
  road the role's line refuses is said to the log and left out. It answers the agents made; asked again, it
  makes none twice.
- **A role off for a person takes no agent of theirs:** a member's `POST /role-agents` for it is refused 403
  `role-off`. Where the account service does not answer, the role stands on.
- **`GET /signup-composition`** with `unit` or `person` answers what stands for the subject, each role with
  `off`, `agents` and `from`, the link that said them, and, for a caller the console names, the subject's own
  link as the chain answers it to them. **`PUT /signup-composition/{link}`** with `value` judges a composition
  — its shape, every role one the store holds, every road against its role's line — and writes it on the chain
  in the caller's name, null following the link above; the chain's answer, its refusal among them, is the
  route's.

## A person's own agents

- **The organisation supplies a person's agents, and the person lays their own changes over that set.** The set
  supplied is the sign-up composition resolved for the person now (「The agents an account is given」): for each role
  that does not stand off, the seats it names, each `<role>-u<user>` and then numbered in the composition's order, on
  the road the composition names, else on the road of the role's agent of no account that is on, else on none, which
  follows the asker. Each seat is supplied by the link whose composition named it, or by the platform's own rule
  where none did. The person's own layer is everything else of their agents: an agent of their own added beside the
  seats, a road chosen for an agent, a supplied agent switched off.
- **Each change a person may make is a setting of the delegation chain,** a flag, `nearest`, allowed by default, held
  at the platform's link, a unit's and the person's, with rules for a person, a group or a unit, a head writing a
  unit's and a person's; a unit that sets it and locks it decides it for everyone under it. Agency declares them with
  its other kinds (`docs/prompt-layers.md`, 「The settings agency declares」):
  - `agency.own-agents`: whether the person may change the set — add agents of their own to a role that does not
    stand off, at most the policy's number of one role's agents (`policy/organisation.toml`) counting the seats, and
    switch a seat supplied to them off;
  - `agency.road-choice`: whether the person may choose an agent's road — a provider and a model on it with an
    effort, or none to follow the asker.
  A person's own instructions are a third, `agency.personal-instructions` (`docs/prompt-layers.md`, 「The person's
  instructions」).
- **A road a person chooses is one their supply carries now:** a source of the order their run would walk (the
  hook's order, `docs/supply.md`, 「Choosing a source as a run starts」) that
  holds a credential for the road, judged as a run's start judges it but reserving nothing. A model another person
  owns is offered to nobody else, and a road is judged against its role's line as every agent's is. A road naming
  none is always open, since the agent's runs then go out on the road of the supervisor that asks them.
- **Putting a seat back takes the person's own change away:** the seat stands on, on the road supplied now, made
  again where it no longer stands. It is open whatever the chain withholds, since it only returns to the set the
  organisation supplies. A seat supplied is never removed, only switched off; an agent of the person's own is
  removed, and is refused while it carries a run.
- **The settings are judged as a change is asked.** Withholding a change later keeps the person from making another;
  the agents keep the roads and the switches they stand at until the person puts them back.
- **Everything the organisation supplies is set by its heads on the organisation's own settings** (the web's
  `docs/web.md`, 「Organisation」, the agents tab): the composition's roles and their agents' roads, the organisation's
  instructions, and whether its people may make each change; none of it needs the platform's agent management.
- **The routes** are `docs/api.md`, 「own-agents」: each acts on one person's account, the person's own caller on
  theirs alone.

## The roles the policy seeds

- **A role the policy seeds is seeded with its agents:** as many as its declaration's count, each on the road
  the declaration names, or on none where it names none. An agent is given once, with its role; a role the
  store already holds is given no agent by the policy again.
