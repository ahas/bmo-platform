# Repositories

A repository is where executing work happens: a code tree with its checkouts, and what is that code
tree's own to hand a run working in it. It is a place, never an agent (`docs/agents.md`, 「Two axes」):
no repository owns a role, a session or a program, and any role may run in any repository's path. How a
repository runs several jobs at once is `docs/worker-slots.md`; what a job holds outside its turns — the
preamble, the workbench, the dev server — is `docs/worker-bench.md`.

## What a repository row holds

- **One row of the `repositories` table a repository,** its whole declaration:
  - **its name,** the key every reader finds it by, the small alphabet a URL, a unit's name and a lock
    file's take without quoting (`policy/workers.toml`); and **its label**, what a screen shows it as;
  - **its owner,** the login account it belongs to, or the organisation unit it belongs to (「An
    organisation's repository」), or neither for the administrator's;
  - **its path,** the base checkout, the directory its jobs work in, which every add names: agency makes no
    checkout and no directory for a repository; and **its node,** where the base checkout stands on a work
    node that takes jobs rather than here (`docs/work-nodes.md`, 「Which nodes take jobs」);
  - **its checkouts:** how many support worktrees stand beside the base, `auto` for as many as its launches
    grow (`docs/worker-slots.md`, 「Growing a checkout at a launch」), and the checkouts it keeps on the
    work nodes that take jobs, set as one list (`docs/worker-slots.md`, 「Checkouts on the nodes」);
  - **its prefix documents,** the paths of the documents a run's prefix carries in this repository, in
    order (`docs/worker-bench.md`, 「Prefix documents」);
  - **its preamble,** the owner's text that stands before every job's prompt in this repository
    (`docs/worker-bench.md`, 「Preamble」);
  - **its rules,** the repository's own rules a run in it is given after the worker line's standing rules,
    from the prompt folder its row names under `prompts/agents/` or none;
  - **its dev command,** the one line its jobs' dev server is started with (`docs/worker-bench.md`, 「Dev
    server」);
  - **its pinned model,** a provider and one model of it, or none (「A pinned model」 below), with the
    sampling a run on a pinned registration is sent: the supervisors' generation settings by the same names
    and bounds, less the answer ceiling, each held or left out; one left out is the engine's own default,
    which agency neither sets nor reads (`docs/own-llm.md`);
  - **whether it is an umbrella,** off unless set: a folder that holds other repositories' checkouts below it,
    whose own tree its jobs never write and whose job does its work in the checkouts lent to it
    (`docs/worker-slots.md`, 「An umbrella repository」);
  - **its downgrade heal,** on unless the owner turns it off, whether a job that fell to a lower model is
    launched on its own model again (`docs/cache-fallback.md`, 「A repository the owner keeps on its fallen
    model」).
- **Every repository is a row alike,** the platform's own services and the owner's other repositories —
  one kept outside the platform's tree among them. Nothing is built in: the operations repository is a row
  like any other.
- **A repository is added, changed and removed on the console's repository tab**, a unit's on the organisation
  screen as well (「An organisation's repository」), or through the repository
  routes (`docs/api.md`, 「repositories, prefix-checks and room-holdings」), with no restart; a removal is refused while a job holds one of
  its checkouts. From a shell, `bmo repository add <name> --path <base checkout>` adds one — `--label` names
  what the console and the agent list show it by, and `--umbrella` marks it an umbrella — and `bmo repository remove <name>` takes one away; each
  asks the same route and prints what it wrote or the route's refusal, and the checkout and the deploy tree
  on disk stand as they were.
- **A repository keeps no log of its own.** It is a place, so what ran in it is its jobs' runs, each with its
  own log; its jobs are listed by repository, newest first, and a screen shows the log of its job going, else
  of its latest.

## Who may add and change a repository

- **A setting of the delegation chain,** the kind `repositories.manage`: a flag, `nearest`, on by default,
  held at the platform's link, a unit's and a person's, with rules; a head writes a unit's and a person's.
  Agency declares it as its server stands (`docs/prompt-layers.md`, 「The settings agency declares」).
- **A member's change of a repository needs it,** resolved for them afresh: off, every add, change and removal
  they ask is refused 403 `repositories-withheld`, whatever their console section grants. Where the account
  service does not answer, the change is refused 503 `access-unchecked`. The owner's and the deployment's own
  calls are bound by none of it.

## An organisation's repository

- **A repository may belong to an organisation unit in place of an account.** A head of the unit adds one
  naming `ownerUnit` where `repositories.manage` resolves true for the unit, and changes and removes it; the
  owner adds and changes any. A head is the person the chain answers the unit's link of `repositories.manage`
  to, which it answers its writers alone. Heading the unit is the whole of it: no console section is asked
  (`docs/api.md`, 「Who a caller acts for」), so a head sets every field of the unit's repository from the
  console's organisation screen as the owner does from the repository tab.
- **Only the people of its path work on it:** a holder's work stands on it only where the holder is a person
  whose chain runs through the unit, read off the chain afresh, so no other account's agents and none of the
  platform's work there; anything else is refused 403 `foreign-repository` as every crossing of sides is
  (`docs/api.md`, 「A repository takes the work of its own side alone」).
- **An account or a unit removed for good takes the repositories filed under it** as a removal takes one, its node
  and support checkouts' rows with it, and is refused while a job holds a checkout of one (`docs/api.md`, 「owners
  and organisation units removed for good」).

## What a run in a repository is handed

- **A run working in a repository's checkout is handed the repository's own,** whatever its role:
  - its system prompt opens on the platform's harness opening and the role's prompt, then the worker line's
    standing rules, the repository's rules and its prefix documents, each headed by its path;
  - a job's prompt carries, under the item's title, the repository's preamble under agency's header naming
    the job's checkout, its workbench and its dev server (`docs/worker-bench.md`, 「Preamble」), then the
    item.
- **What the repository's system prompt holds is part of the prefix,** so a changed prefix document or rule
  moves the prefix of every session kept at that path, and the next run there opens a new session
  (`docs/sessions.md`, 「A role's session」). The preamble is no part of it: it stands in the job's prompt,
  so a changed preamble reaches the next job's prompt and folds no session.
- **A run whose path is no repository's** — a worker ask in a supervisor's scratch folder, a search, a
  translation — is handed nothing of a repository.

## A job

- **A job is one run of an item in a repository.** The item names the repository its work belongs to and
  may name the role it runs as; where it names none, the role is `worker` (`policy/roles.toml`). Any role the
  store holds may be named, free choice, and the run holds that role's prompt, bundles, order and cap. An
  item whose work changes a repository's code names `coder`; one that names no role stays `worker`
  (`docs/agent-roles.md`, 「The coder role」).
- **A job goes out when two things are free:** a slot of its role (`docs/role-slots.md`) and a checkout of
  its repository (`docs/worker-slots.md`, 「The queue with support checkouts」). Jobs on three repositories
  whose role has two agents switched on run two at a time, each in its own repository's checkout on its
  agent's provider and model; the third waits for a slot.
- **It runs on the road of the agent its item names, or its launch takes for its model,** and on no other
  (「An agent an item names」 below): that agent's provider, model and effort, let in on that agent alone
  (`docs/role-agents.md`, 「Which agent a run takes」).
- **It goes on in a kept session** where one is valid for its role at that checkout on that provider and
  model, under the role's session policy (`docs/sessions.md`, 「A role's session」), and its runner may hand
  the session over as the job ends.
- **A job whose work spans repositories** is one item whose label `repositories` names them, comma-separated,
  filed to the repository it runs in, an umbrella where the work has no home of its own
  (`docs/worker-slots.md`, 「An umbrella repository」). Its launch lends it a checkout of each other repository
  named before its first turn, waiting for them as it waits for its seat, and it keeps them through its review
  (`docs/worker-slots.md`, 「A checkout lent to a job」, 「Loans across a job's review」).
- **Its checkout, its branch and its landing** are `docs/worker-slots.md`'s; its workbench and dev server
  are `docs/worker-bench.md`'s.

## An agent an item names

- **Every item names the agent its job runs on, or the model and the role it runs on.** The label
  `agent=<name>` names an agent of any role as the agency registers it (`docs/role-agents.md`); the labels
  `model=<model>` and `role=<role>` name a model and the role whose agents run it. The supervisor that files the
  item names them, and may change them before it launches it, choosing from what it has learned of the work and
  of its agents, unless the owner named them himself. The job runs as its agent's role, goes out on that
  agent's road — its provider, model and effort — and is let in on that agent alone, waiting for it however
  long its provider cannot answer, never on another agent of the role and never on another road. Its slot ask
  belongs to that agent's account (`docs/role-slots.md`, 「Accounts」) and its run stands on that agent's home
  node (`docs/role-agents.md`, 「What an agent is」). Where the named agent leaves the register, moves to
  another role or to an account the ask cannot belong to, the job is ended failed at once with that reason in
  its report, and never left waiting in `starting` on a seat that can never open.
- **A name agency does not hold is refused as it is set.** The job service refuses an item filed, or an
  `agent`, `role` or `model` label changed, naming an agent, a role or a model agency does not hold, from the
  values agency publishes to it (`docs/agent-control.md`, 「A label agency does not hold is refused as it is
  set」); what changes after the label was set is told to the item's orderer at once.
- **An `agent` label stands over a `model` label.** Where an item carries both, the agent it names is the job's
  and the `model` label is not read.
- **A model an item names is taken at the launch.** The `model` label names the model as an agent's road names
  it, `claude-opus-5`, or with its provider before it, `claude-code/claude-opus-5`; the role is the item's
  `role` label's, else the role a job runs as. The launch takes the first agent by name of that role,
  switched on, of the launching supervisor's account, whose road runs that model, and free; the choice and the
  job's writing are one step, so two launches at once take two different agents (`docs/role-agents.md`,
  「Which agent a run takes」). The agent taken is the job's as if the item had named it: nothing about its agent,
  role or road changes after the launch.
- **A running job holds fixed what its launch read of its item.** Agency has the job service hold fixed, on the
  item's lease (bmo-job's `docs/api.md`, "While leased"), the labels `agent`, `role`, `model` and
  `repositories`, the item's
  `assigneeName`, the repository its job runs in, and its `scope`, the room the job answers to, with a reason
  naming the job and the agent it runs on (`docs/api.md`, 「assignments and jobs」). A change of any of them while
  the job is `starting`, `running` or `reviewing` is refused by the job service with that reason, and the
  supervisor makes it once the job is stopped, for the item's next launch. The item's words, its title and
  brief, are the work rather than the conditions it runs under and are not held. Every screen and answer that
  names a running job's agent names the job's own `agentName`, never the item's label.
- **A launch whose agent is busy is refused** 409 `agent-busy` at once, with the agent and what it carries at
  work, or, for a model, with every candidate and what each carries; nothing queues it to start later. An
  agent whose every job stands in the machine's turn — testing, waiting to test, deploying or waiting for its
  deployment — is free, and the launch goes through (`docs/role-agents.md`, 「When an agent is busy」). A busy
  agent's next job is launched by the supervisor once the agent, or for a model any candidate, is free, as the
  idle agent's notice tells it (`docs/agent-control.md`, 「An idle agent is told to the orderers of the items it
  could take」).
- **A model no agent of the role runs is refused** 422 `no-candidate`, at the filing (`docs/api.md`,
  「assignments and jobs」) and at the launch alike, its sentence naming the role, the model and the role's agents
  with their roads; the item is never launched on another agent in its place, and its orderer is told
  (`item-no-candidate`, `docs/agent-control.md`, 「An item's change is told to its orderer」).
- **A launch whose item names neither an agent nor a model is refused** 422 `agent-unnamed`, an `agent` or a
  `model` label that is `auto` or empty alike, with the agents of the role it would run as and the item's
  orderer told once (`docs/agent-control.md`, 「An item's change is told to its orderer」). An agent that names
  no road of its own cannot carry a job, since no supervisor asked it; the launch is refused 422
  `agent-names-no-road` with the agent's name. An agent the agency does not register refuses the launch 422
  `unknown-agent` with the label's text.
- **A role an item names** with its label `role=<name>` must be a role the store holds, and one it does not
  hold refuses the launch 422 `unknown-role` with the label's text. Beside an `agent` label the agent's own
  role is the job's role, so the label decides nothing more; beside a `model` label it is the role the
  candidates are of.
- **What chose a job's provider and model is its agent,** named by its item or taken for its item's model,
  which the job says (`chosenBy` `item`, `docs/api.md`, 「assignments and jobs」).

## A pinned model

- **A repository may pin a model:** a provider and one model of it — a model the Claude CLI's catalogue
  names on Claude Code, a member of a provider's own road, or an offered registration on the registry's
  provider (`docs/provider-order.md`, 「Models a provider holds」).
- **A pin carries the runs in the repository that name no agent,** its worker's session and standby,
  whatever role they run as: the run goes out on the pinned provider and model, let in on an agent of its role
  that stands on that road alone, at the effort that agent names. A pin no agent of the role stands on refuses
  the run's slot with the roads the role's agents hold (`docs/role-agents.md`, 「Which agent a run takes」). A
  job goes out on its agent's road, so no pin carries a job.
- **A pinned model that is unavailable makes the repository's work wait.** Where the pinned provider has no
  quota, cannot answer now, or the pinned registration's engine is down, the run is
  held with the reason and goes out once the pin can answer again; it never falls back to another
  provider or model.
- **A pinned run still holds a slot of its role,** the agent on the pinned road it is let in on.
- **The pin is set and cleared on the repository's row;** the next run reads it.

## The words said to a repository

- **A supervisor's worker ask** runs a `worker` run in the folder it names, the asking supervisor's scratch
  folder where it names none; a repository's checkout may be named only in the owner's turn, and the run is
  then handed that repository's own (`docs/tools.md`, 「The worker ask」). It takes a slot of the worker role,
  and a repository's checkout like a job, and waits for both however long it takes.
- **A person typing into a run's pane** speaks to that run while it stands.
- **A word said to the worker** goes by the repository, `POST /repositories/{repositoryName}/words`: into the
  job's turn going in one of its checkouts where the provider takes it, and otherwise a turn of its own on
  the session the worker role keeps at the checkout, under the checkout's lock and the worker-words clock
  (`docs/agent-control.md`, 「Words said to a worker」). It is the owner's alone, and the words and their
  answer stand in the worker's log.
