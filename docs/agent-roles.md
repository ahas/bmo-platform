# Roles

A role is the executing axis of agency (`docs/agents.md`, 「Two axes」): what a run is for, which decides
the prompt it opens on, the tools it is given, the providers and models it runs on, how many of its runs go
at once and whether its runs keep their sessions. This document is the canonical statement of roles and
their bundles. How a role's runs are let in is `docs/role-slots.md`; how a run gets its provider is
`docs/provider-order.md`.

## What a role is

- **Every run holds one role,** the one the work names (「Which role a run holds」 below). A role is a preset —
  a prompt, tools, skills, a line and how its sessions are kept — that every run of it reads as it starts; the
  seats its runs take, each on one road, are its agents (`docs/role-agents.md`).
- **A role gives function bundles, never a run by its name.** Every reader takes the role a run holds and
  reads its row, so a role added as a row is given everything its row says from its first run, with no
  change anywhere else.
- **A bundle is a capability of the tool host** (`mcp-host.md`, 「Namespaces and capabilities」): a named set
  of tools a session is shown once its record holds it, and the right the tools' own refusals read.
- **The workers, the coder, the helper, the producer, the voyager, the seeker, the translator,
  the transliterator, the actor and the note assistant are roles alike.** Nothing tells one role from another
  but what its row says.

## What a role row holds

- **Kept in agency's store,** the `roles` table (`docs/store.md`), one row a role:
  - **its name,** its key, kebab-case as the policy's pattern says, which a screen shows the role by, and
    **its label**, the English words a log keeps of it (「A role's label」 below);
  - **its prompt,** written in English, which every run of the role opens with after the platform's harness
    opening; a role carrying none gives none;
  - **its description,** one sentence saying what the role is for, which a screen shows of it;
  - **its provider prompts,** each a text a run of the role is given beside its prompt where its program
    stands on that provider, kept by the role and the provider (「A role's provider prompts」 below);
  - **its bundles,** the function bundles its runs are given, each one the policy declares;
  - **its skills,** the names of the skills its runs carry on the worker line, or none set, which leaves its
    runs the whole plugin the release places for the role (`docs/role-agents.md`, 「What a run reads off its
    agent」);
  - **its line,** the launch line its runs go out on — the worker line, the search line, the browse line, the
    plain line or the translate line (`docs/launch-lines.md`);
- **A role carries its line's declaration and its own row, and nothing else:** what its line's program carries
  — the pieces of its system prompt, its setting sources, where its skills come from, its own commands and the
  program's subagent types it keeps (`docs/launch-lines.md`, 「What a line carries」) — filled with the role's
  prompt, provider prompts, bundles and skills. A test per seeded role holds the launch composed for it to that.
- **A role holds no road, no cap, no sessions setting and no compaction threshold.** Whether a run keeps its
  session and the share at which that session is folded are each its agent's (`docs/role-agents.md`, 「What an
  agent row holds」).
- **A role holds no road and no cap.** The roads its runs go out on are its agents', one each, and how many
  of its runs go at once is the number of its agents that are switched on (`docs/role-agents.md`). A role
  with no agent runs nothing.
- **A role whose agents name no road follows the supervisor that asks it.** A run of such a role a
  supervisor asks goes out on that supervisor's own provider, model and effort as they stand when it is
  asked; one no supervisor asks is refused with why (`docs/role-slots.md`, 「A role that follows its
  supervisor」). The role's
  row answers `followsSupervisor` true while each of its agents names no road or follows its asker
  (`docs/api.md`, 「roles」).
- **A role may have its agents follow their asker while they name roads of their own,** `followsAsker` on the
  role, each agent's own setting standing over it: a run a supervisor asks goes out on that supervisor's road,
  and the agent's own road is its fallback where none asked or that road cannot answer now
  (`docs/role-agents.md`, 「An agent that follows its asker」).
- **An agent may lay its own prompt, bundles and skills over its role's,** each standing in place of the
  role's for that agent's runs alone (`docs/role-agents.md`, 「What an agent row holds」).
- **The owner adds, changes and removes roles on the control port** (「Changing the roles」 below), and every
  reader takes a change at its next read, with no deployment and no restart.

## The bundles

- **The policy keeps what is not a role's own,** `policy/roles.toml`, read by the bun side and the Rust side
  alike: the bundles a role may give, `[[bundles]]`, each with its label; the supervisor bundles every
  supervisor holds (`docs/supervisors.md`, 「The bundles a supervisor holds」); the role a job runs as where
  its item names none, `worker`; the role each of agency's own runs that stands for no one's work holds, `upkeep`; the bundle that opens
  sending; and the shape of a role's name.
- **A role gives only bundles the policy declares,** and the control port refuses a role naming another.
- **`brain.lessons` reads the brain of the supervisor a run works for and writes the run's lessons there:**
  the brain's `brain_list`, `brain_similar` and `brain_learn` in the narrower form that bundle holds (the
  brain's `docs/boundary-api.md`, 「Its tool」), reached with `bmo use`. The run's record names that
  supervisor as `BMO_BRAIN_AGENT` — the one agency's `researchSupervisor` setting names (`docs/api.md`, 「settings」), none where it names none — while the
  turn's agent stays the run's own, so every other tool it calls acts as the run. Every role on the worker
  line holds it but `upkeep`, which stands for no one's work, and `actor`, which reaches nothing itself.
- **The policy names the capabilities through which a session reaches a brain,** `brain`: `room.chat`,
  `gate.reference` and `brain.lessons`. A supervisor whose brain switch is off holds none of them on its own
  turns (`docs/supervisors.md`, 「Its settings」).
- **The policy names the capabilities through which a session reaches the gate's reading of a brain,**
  `reference`: `gate.reference`. A supervisor whose reference switch is off holds none of them on its own turns
  and keeps the rest of the brain's (`docs/supervisors.md`, 「Its settings」).
- **A role may list the tool entries its runs are shown by name,** by full name in its `shown` list; every
  other entry its record holds is reached with `bmo use`. No role lists any now: each line's declaration
  lists what its sessions are shown.
- **A screen is handed each role and bundle with its kebab-case code beside the label** (`docs/api.md`,
  「roles」): it shows a role by its name (「A role's label」 below) and words a bundle by its code; the label
  is the English a log keeps.

## A role's label

- **A role's label is its row's,** never empty: every seeded role is given the label the policy declares
  for it, and an add or a change refuses an empty one. It is the English word a log keeps of the role, and
  agency's API answers it with the row.
- **A screen shows a role by its name alone,** the kebab-case key agency holds it under — `worker`,
  `coder`, `helper`, `voyager`, `seeker`, `producer` and every other — on every screen that names
  one: never the row's label and never a word translated from the name. A role the owner adds is shown by
  its name from its first listing.

## The roles the policy seeds

- **The policy seeds them,** `policy/roles.toml`: every role it declares under `[[roles]]` is given to the
  store once by its name, the first time the store is settled with it, with the prompt and the description
  its declaration gives. A role naming a prompt file under `prompts/roles/` takes that file's text as its
  prompt and the one sentence its declaration states as its description; a role naming none takes no
  prompt. Every seeded role carries a description. A role the store holds is never touched by the policy
  again, and one the owner removed stays removed, so a prompt or a description the policy gives a role the
  store already holds reaches the row only through `PATCH /roles/{roleName}`. A role whose prompt file
  cannot be read is not given and waits for the next settling that can read it. A role's provider prompts
  are given apart (「A role's provider prompts」).
- **The seeded roles:**

  | Role | Label | Line | Bundles | Cap | Prompt | Provider prompts | Description | Sessions |
  |---|---|---|---|---|---|---|---|---|
  | `worker` | Worker | worker | `queue.read`, `web.read`, `brain.lessons` | 3 | none | none | policy | keep |
  | `coder` | Coder | worker | `queue.read`, `web.read`, `brain.lessons` | 3 | `coder.md` | `claude-code`: `coder-claude-code.md` | policy | keep |
  | `helper` | Helper | worker | `queue.read`, `web.read`, `brain.lessons` | 2 | `helper.md` | none | policy | keep |
  | `producer` | Producer | worker | `queue.read`, `web.read`, `room.send`, `brain.lessons` | 2 | `producer.md` | none | policy | keep |
  | `voyager` | Voyager | search | `web.read`, `web.sites`, `web.branches`, `web.rulebooks` | 4 | none | none | policy | fresh |
  | `seeker` | Seeker | browse | `web.read`, `web.sites`, `web.branches`, `web.rulebooks` | 4 | none | none | policy | fresh |
  | `translator` | Translator | translate | none | 4 | `translator.md` | none | policy | fresh |
  | `transliterator` | Transliterator | search | none | 4 | none | none | policy | keep |
  | `upkeep` | Upkeep | worker | none | 1 | none | none | policy | fresh |
  | `actor` | Actor | worker | none | 1 | `actor.md` | none | policy | fresh |
  | `note-assistant` | Note assistant | plain | none | 2 | none | none | policy | fresh |
  | `app-reviewer` | App reviewer | translate | none | 2 | `app-reviewer.md` | none | policy | fresh |

  A prompt and a provider prompt named by file are files under `prompts/roles/`. A `policy` description is
  the sentence the role's declaration states.

- **Each seeded role's agents are the policy's,** given with the row: as many agents
  as its declaration's count, each on the road it declares and with the sessions setting, the compaction
  threshold the declaration states (`docs/role-agents.md`, 「The roles the policy
  seeds」), changed by the owner afterwards like any agent. The `coder` declares no road,
  so its agents follow the supervisor that asks them until the owner sets an agent a road.
- **Whatever role writes it, a result written for a person is written in the language of the person who
  asked** (`docs/agent-documents.md`): a job's report and the documents it files, in the language the item's
  brief names for its requester, else its room; a voyager's result, in its question's language. What
  roles read of each other stays English.

## The worker role

- **The role a job runs as where its item names none.** Its run works in a repository's checkout, on what
  the repository hands it — its prefix documents, the prompt that stands before the job's and its rules
  (`docs/repositories.md`) — under the worker line's standing rules.
- **It keeps the job whole:** the brief and its notes, the work order and its tasks, the checkout and the
  branch, the checks the repository declares, the close and the report. The coding passes are not its own
  (「The coder role」 below).
- **It carries no prompt:** a job's run opens on the harness opening and the worker line's rules, with
  what its repository hands it after them.
- **Its bundles:** `queue.read`, the worker's own namespace, the queue's two reading verbs
  (`docs/worker-mcp.md`); `web.read`, web search and page fetch; and `brain.lessons`, the
  brain it reads before the work and writes its lessons into once the work is done. The site tools are the
  voyager's and the seeker's alone.
- **Any job may name another role,** whichever the owner chose for the item (`docs/repositories.md`, 「A
  job」), and its run holds that role's prompt and bundles, on one of that role's agents, in the same
  repository's checkout.

## The coder role

- **The role a coding job runs as.** Its run works in the same repository's checkout, on what the
  repository hands it, and holds the coding work the item asks for: the code, its design, its tests, the
  documents that cover it, and the commit. The item, the close and the report stay the job's, as on the
  worker line.
- **Its line is the worker line,** and its prompt, seeded from `prompts/roles/coder.md`, stands first in
  every run after the harness opening, before the line's standing rules and the repository's own.
- **Its coding skills are its own bundle's,** the `coder` bundle placed at `prompts/agents/coder/plugin/`:
  `ahas-code-design-patterns` first where a structure is under question, `ahas-code-refactoring` second,
  `ahas-code-clean` third and last, `ahas-code-test-writing` before a test is written or changed,
  `ahas-docs-git` and `bmo-docs-conventions` for the documents and the commit, and `ahas-ux-design-rules`
  for a screen. Its prompt names them; the worker's run carries the `worker` bundle, which holds none of
  them.
- **Its plugin holds the worker bundle's skills as well,** the ones every job needs whatever its work —
  closing a job, asking the owner, reaching beyond the checkout — placed beside its own, so a coder's job
  ends the way any job does. The producer's plugin holds them the same way.
- **Its bundles are the worker's:** `queue.read`, `web.read` and `brain.lessons`.
- **Its agents are seeded on no road,** so a run of it a supervisor asks goes out on that supervisor's own
  provider, model and effort until the owner sets an agent a road of its own.
- **It is seeded with three agents, as the worker is,** so a repository's coding jobs run as many at once as
  its jobs did.
- **Which jobs name it:** an item whose work changes a repository's code names `coder`; one whose work is
  not code names `worker`, and an item naming no role stays `worker` (`docs/repositories.md`, 「A job」).
  An item already raised keeps the role it named, so every existing job runs as it did.

## The helper role

- **The role for a hard problem that wants a stronger model** — a design question, a stubborn bug, a plan that
  has to hold, an analysis that needs care. Its prompt, seeded from `prompts/roles/helper.md`, stands first in
  every run.
- **A run of it is asked as any role's is:** a job whose item names `helper` (`docs/repositories.md`, 「A job」),
  or a slot or a session a caller asks naming it (`docs/api.md`, 「sessions」). No tool of agency's hands it a
  task of its own accord.
- **Its runs go out on the helper role's own order, the strongest road the deployment names,** and hold a slot
  of the helper role, like any role's runs.
- **Its runs work in the folder their caller names,** so a run that follows another at the same folder on the
  same provider and model goes on in the same session while it is valid (`docs/sessions.md`, 「A role's
  session」).
- **Its bundles** are the worker's: `queue.read`, `web.read` and `brain.lessons`.

## The producer role

- **The producer is a role whose items make videos.** Its runs work in the video repository's checkout,
  where its projects and its studio live; its line is the worker line, and its role prompt, seeded from
  `prompts/roles/producer.md`, stands first in every run: what the studio is (the video-shotcraft
  skill bundle driving Remotion, rendering on the card under the card arbiter's ticket), how an item is made, how the file goes out and the studio's
  licence rules. The worker plugin's `ahas-video-shotcraft` skill carries the setup, the render and the
  checks.
- **Its bundles:** `queue.read` for its items, `web.read` for the pages and assets an item names, and
  `room.send` for the finished file, which goes into the item's room through the receiver's file send tool
  as a job's own material (the receiver's `docs/room-files.md`, 「A job's sends」); and `brain.lessons`, as every
  role of the worker line.

## The voyager and the seeker roles

- **`web.read`, `web.sites`, `web.branches` and `web.rulebooks` — the whole of the web namespace:** search
  and fetch, the site tools, the branch tool and the rulebooks, the rulebook reader standing in the research
  unit's namespace. The look at a picture is the eye service's `eye/see`, which the search and browse lines
  hold through their `eye` namespace (`policy/run-lines.toml`).
- **The voyager runs on the search line, the seeker on the browse line.** A session's caller names the
  role and narrows the bundles (`docs/api.md`, 「sessions」): an apart session keeps only the capabilities
  its start's `toolsAllowed` names, so the research unit decides whether a search, a browse, a round or a
  branch holds the branch tool and the rulebooks.
- **Neither carries a prompt:** a run on the search or the browse line opens on the harness opening and
  the instructions its caller hands in, and no role prompt is read there (`docs/launch-lines.md`, 「Search
  runner」). The voyager answers a question by searching; the seeker visits and reads the sites and pages it
  is named.
- **The search and browse lines hold the shared brain's writing tools** (`docs/launch-lines.md`, 「Search
  runner」): `brain_learn` and `brain_similar` of the `bmo` namespace, on a supervisor's brain: the brain of the
  supervisor agency's `researchSupervisor` setting names (`docs/api.md`, 「settings」), none where it names none. The
  line's session record names that supervisor as the turn the brain tools run for, so a run writes what it
  establishes into the same brain the supervisor reads, while the run itself stays the role's.

## The translator and the transliterator roles

- **The translator is asked on demand** — a document's part, a text a job reads, a record the owner reads
  — and goes out on the translate line (`docs/launch-lines.md`, 「Translate runner」). Its prompt, seeded
  from `prompts/roles/translator.md`, stands before the instructions of the purpose it is asked for. It holds
  no bundle: no web, no tool.
- **The transliterator is the voice unit's,** asked for a slot under its name with the search line, and
  holds no bundle (the voice's `docs/boundary-api.md`). It carries no seeded prompt: the voice unit hands
  every session its own instructions, and the search line reads no role prompt.
- **The transliterator follows its asker,** as its declaration seeds it: a slot asked for a supervisor's
  narration naming that supervisor as `askedBy` goes out on that supervisor's road, and one naming none, or
  whose supervisor's road cannot answer now, goes out on its agent's own road (`docs/role-agents.md`, 「An
  agent that follows its asker」).

## The upkeep role

- **Held by agency's own runs that stand for no one's work,** the usage reader's renewal turn among them. It
  gives no bundle and carries no prompt: what such a run holds of a role is one of its agents and its
  sessions setting.

## The actor role

- **The role a rehearsal's actor runs as:** one person of a mock
  company, played from that person's roster line and the goal a scene hands them. Its prompt, seeded from
  `prompts/roles/actor.md`, stands first in every run.
- **It reaches nothing itself.** Each turn hands it the person's roster line, their language, their goal, the
  console's screens they may use and what its calls so far answered; it answers one JSON object, `calls`, the
  console calls the person would make next — each its `service`, `screen`, `method`, `route` and `body`, an
  empty list once it is done — and `did`, what the person has done so far as facts a read can check. Its
  caller makes those calls through the person's own sign-in, never through a service token, and judges the
  goal by its own reads, never by the actor's words. It holds no bundle, and its prompt tells it to use no
  tool and that none of a job's rules reach it.
- **It is the person, never the person's supervisor.** What a supervisor is for — a meeting's minutes draft,
  handing tasks out, tidying a gathering, filing archive documents, raising work items — the actor asks the
  person's own supervisor for, in the person's voice, through the roads a person has to it (the private chat, a
  call by name in a group room where the room's rules allow it, the side talk beside a meeting), and then waits for
  the supervisor to act; it never makes those calls itself, and a supervisor that does not act is reported as a
  fact, never covered.
- **Its line is the worker line,** the line that reads a role's prompt into a session a caller starts.
- **Its runs stand under the person's account.** A slot belongs to the account of the supervisor its ask names
  as `askedBy` (`docs/role-slots.md`, 「Accounts」), so its caller asks under the person's own supervisor: the
  run is let in only on an actor agent of that account, counted against that account's agents, and goes out
  on that account's node (`docs/person-nodes.md`, 「Which node is a person's」).
- **Its road is its agent's.** The policy seeds one agent of no account on DeepSeek's flash model at low
  effort, since a cheap general model plays a person through the screens; the deployment changes it as any
  agent's road. That agent carries no person's run, since a slot of an account is let in on that account's
  agents alone; it is the agent the console copies for an account it makes ready (the web console's
  `docs/web.md`, 「The join page」). An account that holds no actor agent is given one with
  `POST /role-agents`, naming the role, the account as `ownerAccount` and the road its caller chooses.
- **Its agent is seeded `fresh`:** every turn hands it the whole of what it knows, so no turn resumes another's
  session.

## The note-assistant role

- **The note unit's Agent** (the note's `docs/design.md`, 「Agent」): one passage of a note, or the whole note,
  worked for the person asking — rewritten, translated, summarized, continued or answered over — and handed back
  in the note's own line grammar for the person to place.
- **Its line is the plain line, and it carries no prompt:** the note unit starts each session naming the
  instructions it opens on, `systemPrompt`, and those instructions are the whole of the run's system prompt — no
  harness opening, no persona and nothing of the platform's (`docs/launch-lines.md`, 「Plain runner」). Its runs
  are shown no tool and no MCP server and read no CLAUDE.md, memory, setting or skill, so a run reads the passage
  it is handed and nothing else (`docs/api.md`, 「sessions」). It holds no bundle.
- **Its runs stand under the person's account.** The note unit asks its slot under the person's own supervisor,
  their BMO, so a run is let in on an agent of that account alone (`docs/role-slots.md`, 「Accounts」), and the
  platform administrator's under no supervisor, on the deployment's own agents; an account that holds no agent
  of the role is given one by the note unit with `POST /role-agents`, on the road of the role's agent of no
  account.
- **Its road is the one the deployment's default answerer stands on as the owner set it:** the policy seeds two
  agents of no account on Claude Code's Opus 5.5 at medium effort with thinking on, changed afterwards like any
  agent's road. They name their road and do not follow their asker, so an ask under no supervisor is let in.
- **Its agents are seeded `fresh`, and agency resumes none of their sessions by itself:** one agent carries the
  asks of every note and every talk of its account, so a session kept for the agent would carry one note's talk
  into another's. The caller names the session a run continues instead (`docs/api.md`, 「sessions」, `continues`):
  the note unit names, for a talk's next round, the session that talk's last round answered on, and hands the
  round's whole words beside it for where that session can no longer be resumed (the note's `docs/design.md`,
  「The talk panel」). An ask over a passage opens a session of its own.

## The app-reviewer role

- **It reads a shared item of any kind and writes its review report** (`docs/person-nodes.md`, 「Shared apps」):
  what the item does, in a few sentences a receiver reads before installing it, and what it reaches for, each named
  with the file it stands in:
  - an app: anything in its code that reaches for more than its manifest declares — the network, a secret, a file
    outside its own folders, another program;
  - a skill: what it instructs an agent to do, any hidden or dangerous instruction (a destructive command, a check
    disabled, a permission bypassed), any ask to send data, secrets, files or the conversation anywhere, and any
    prompt injection — text overriding the agent's rules, impersonating the system or the owner, or hidden in
    comments, encodings or reference files;
  - an MCP server: what it reaches — a remote server's address, the auth it asks and what its secret grants; a local
    server's command, what its code does, the network and secrets it asks, and anything beyond its permissions.
- **Its line is the translate line,** one turn with no tools and no session, the way a translation goes out
  (`docs/launch-lines.md`, 「Translate runner」): its prompt, `prompts/roles/app-reviewer.md`, is the system prompt,
  and the input's first line names the item's kind, then the permissions it asks for and every file of its source.
  It holds no seat and no bundle, and its runs stand under no account.

## A role's provider prompts

- **A role may give its runs a text beside its prompt by the provider their program stands on:** a run of
  the role whose program is that provider's carries the text after the role's prompt (or the agent's own
  prompt where it lays one over the role's), on every line that composes the role's prompt into the run's
  system prompt; a run on any other provider carries none of it. It is how one provider's way of working is
  told to the runs on that provider alone: the coder's Claude Code runs are told how the program's own
  subagents are used, which a run on another provider has no use for.
- **Kept by the role and the provider,** the `role_provider_prompts` table (`docs/store.md`), each read at the
  run's start, so a change holds from the next run; set and taken away through `PATCH /roles/{roleName}`'s
  `providerPrompts`, a provider of this build each, and taken away with the role.
- **The policy gives each one once,** its declaration's `provider_prompts` naming a file under
  `prompts/roles/` by provider: given the first time the store is settled with it, to a role the store
  already holds as much as to one seeded then, and never again, so one the owner changed stands as he left
  it and one he took away stays away.

## Subagents

- **A run's program may open subagents of its own,** a provider's own facility such as Claude Code's agent
  tool: they stand inside the run, on its road, and are no role's runs. Agency hands no subagent definition,
  takes no slot for one and holds no record of one beyond what the run's own log says; whether and how a run
  opens them is the run's own judgement about reaching its work's goal.

## A role's sessions

- **Whether a run goes on in the session an earlier run left is its agent's setting,** the agent row's
  `sessions`: `keep` or `fresh`. Nothing reads a line's name or a run's caller to decide it; a run whose
  agent cannot be told keeps none.
- **`keep`: a run resumes the session its own agent's last answered run left at the same path, on the same
  provider and model, while that session is valid** — inside the cache lifetime the provider declares
  for the model, and on the prefix it was opened on — and opens a new one otherwise (`docs/sessions.md`,
  「A role's session」).
- **`fresh`: every run opens a session of its own,** which no later run resumes.
- **A changed setting holds from the next run;** a run already going keeps the session it started on.

## Which role a run holds

- **A job's run:** the role its item names, `worker` where it names none (`docs/repositories.md`, 「A job」);
  an item whose work changes a repository's code names `coder` (「The coder role」).
- **A worker ask:** `worker` (`docs/tools.md`, 「The worker ask」).
- **A session a caller starts:** the role the start names (`docs/api.md`, 「sessions」): the research unit
  names `voyager` or `seeker`, the voice unit `transliterator`, a translation's caller `translator`, the
  rehearsal `actor`, the note unit `note-assistant`.
- **A shared app's review:** `app-reviewer`, asked by agency itself as an app is shared (「The app-reviewer role」).
- **A run standing for no one's work:** `upkeep`.
- **A role the store does not hold gives nothing,** and every one of these refuses it with the name.

## Where the role is read

- **The session record.** Every record is issued for the run and holds its line's capabilities with the
  bundles its role gives, read from the store as the record is issued (`mcp-host.md`, 「The session
  record」). A run names its role among its turn's variables, which a standing program's own environment
  does not hold, so the record of a program standing in a place for a run is issued from the program's
  environment with the run's variables laid over it.
- **The catalogue a call is served.** The system's own MCP server serves a record the whole catalogue where
  it holds `room.tools`, and otherwise the catalogue's tools whose bundles it holds.
- **The send guard,** for every sending tool and script.
- **The slot,** the agents of the run's role, read from the store as every run asks to be let in: the run
  is let in on one of them, which is its seat and its road (`docs/role-agents.md`, 「Which agent a run
  takes」).
- **The launch,** which reads the role's prompt from the store as it composes the run's prompt.
- **The console's role tab,** which lists, adds, changes and removes the roles, and shows each role's
  session history (`docs/api.md`, 「roles」).

## Changing the roles

- **`GET /roles`** lists every role the store holds, in the order a screen lists them, each with every field
  of its row, how many of its runs go now and how many wait; beside them, every bundle a role may give and
  every line a role may name (`docs/api.md`, 「roles」).
- **`POST /roles`** adds a role, and refuses a name out of the pattern or already taken, a bundle the policy
  does not declare, a line agency does not carry, and an
  order, a cap, a forced provider, a sessions setting, a compaction threshold or a callability, none of
  which a role holds. A role added holds no agent, so it runs
  nothing until one is added (`docs/role-agents.md`, 「Changing the agents」).
- **`PATCH /roles/{roleName}`** changes any field of the row, each judged as the add judges it.
- **`DELETE /roles/{roleName}`** removes a role with its agents, and is refused while a run of it goes or
  waits, and for a role the policy names as a default — `worker` and `upkeep`.
- **A role added or changed is read at its next use;** nothing running is stopped for it, and a run already
  let in finishes under the row it began under.
