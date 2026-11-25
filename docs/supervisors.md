# Supervisors

A supervisor is the conversational axis of agency (`docs/agents.md`, 「Two axes」): an agent that owns
rooms and talks with people. Every conversational agent is a supervisor, the one a deployment seeds and
every one added on the console alike; no supervisor is of a kind apart. This document states what a
supervisor's row and settings hold, how one runner carries every supervisor's turns, how a supervisor of
the `room` scope keeps a session in each room, how a supervisor is started afresh, and what a supervisor
hands to a role. What each supervisor owns apart from the others is `docs/agent-ownership.md`.

## What a supervisor row holds

- **One row of the `supervisors` table is the supervisor's whole declaration.** Nothing in the code lists
  the supervisors or answers a question about one by its name.
- **Who it is:** its name, the key every reader finds it by; its label; its call names, the words beside its
  label that call it in a room (「Its call names」 below); its owner, the login account it belongs to, or none for the administrator's; its mark on a screen; and
  whether the deployment seeds it.
- **What it is told:** its own prompt folder, the documents its system prompt is composed from, and its
  brain folder, whose base persona its system prompt ends with (`docs/agent-documents.md`).
- **How it converses:** its session scope and its turn shape (「One runner」 below).
- **Where it runs — its road:** one provider, one model of it, the effort and the thinking its turns go out
  with there, the advisor its program may consult where the provider takes one (`docs/provider-order.md`, 「The
  advisor」) and, on the registry's provider, the answer ceiling (`docs/provider-order.md`, 「What a road
  is」). Every supervisor names its own, set on its row as an agent's is set on the agent; its turns go out on
  it and on no other, and no other provider or model stands in when it cannot answer.
- **Its card rank:** the rank the demands of every run for it are asked at on an engine the arbiter stands, the
  lower first, or none for the default (`docs/own-llm.md`, 「An engine the arbiter stands」). What the rank
  means is the arbiter's; which supervisor stands at which is the owner's to set here.
- **Its cache warming, switched per provider:** for each provider, whether its resident
  program's prefix is kept warm while it is idle on that provider (`docs/keepalive-schedule.md`). A
  supervisor is the only thing agency warms.
- **Its compaction threshold:** the share of its model's window its session may fill before the session is
  folded or handed over (`docs/sessions.md`, 「Session compaction」).
- **Its upkeep of a turn:** its gate switches, its sampling and its voice, which agency keeps and a turn's
  consumer reads; and the repository its TODO items go to (`docs/repositories.md`).
- **Its draft questions** are the receiver's, kept by the receiver and shown on the supervisor's page in the
  console; agency holds none of them and names the supervisor a session is for, so the receiver reads the
  supervisor's own (「What a session's state names」 below).

## Its call names

- **Its call names are the one list of words beside its label that call it in a room,** kept on its row's
  `aliases` column; no other column holds a word that calls it. The receiver wakes the supervisor on a message
  that holds its label or one of its call names, in either case.
- **They are written by `PATCH /supervisors/{supervisorName}`** as a list (`docs/api.md`, 「supervisors」), the
  whole list each time: at most the supervisors policy's count, each name one to the policy's length of
  characters once trimmed, and none holding a comma (`policy/supervisors.toml`). An empty list or null leaves
  it none. A list outside those bounds is refused whole and nothing is written.
- **They are read as one line,** the names joined by a comma and a space, as `aliases` on `GET /supervisors`
  and `GET /supervisors/{supervisorName}`. Every reader splits that line at its commas, which is why no name
  holds one.
- **A supervisor of the `pieces` shape is told them,** with its label, in its system prompt (「What a turn is
  given」 below). A change of either moves that prompt: its next turn in each room is sent the prompt as it now
  is, and the session is then replaced while it is quiet (`docs/sessions.md`, 「Session replacement」).

## Its settings

- **Every supervisor holds settings beside its row,** a row of the `supervisor_settings` table keyed by its
  name, every default standing where it holds none: its reference switch, its brain switch, its tool limit,
  its persona and its generation settings, with who saved them last and when. Its thinking and the registration it runs on at the
  registry's provider are its road's (「What a supervisor row holds」 above), and a save of the settings
  changes them on the road.
- **What a turn reads of them is its turn shape's.** A turn of the `pieces` shape is composed with the
  persona its settings hold (「What a turn is given」 below) and asked with its generation settings; a turn of
  the `window` shape is composed from the chat documents and its brain's persona (`docs/agent-documents.md`,
  「A supervisor of the window shape」), and its settings change nothing of it.
- **The reference switch** says whether the gate's reading of the supervisor's brain reaches its turns — the
  sheets, memories and rows the gate picks in front of its words, and the index it reads the brain through; the
  supervisors policy names the default where its settings never held it. Off, every place the gate's reading
  reaches a supervisor's turn is closed, and the brain's own tools stay, so the supervisor still recalls and
  learns by its own calls and is handed nothing it did not ask for:
  - its session record holds none of the capabilities the roles policy names as the reference's (`reference`:
    `gate.reference`, the gate's reading of the index and its pieces), on its own turns;
  - the receiver, which reads the switch on the settings page beside the brain switch, composes the room's block
    and the turn's core as it does for a brain switched off: no brain index, hint or due line in the block, and no
    core asked of the gate.
  The brain switch off closes all of this whatever the reference switch says.
- **The brain switch** says whether the supervisor's brain reaches its turns, on where its settings never held
  it. Off, every place the brain reaches a supervisor's turn is closed, and nothing else changes:
  - its session record holds none of the capabilities the roles policy names as the brain's (`brain`:
    `room.chat`, which every entry of the brain's tool asks; `gate.reference`, the gate's reading of the index
    and its pieces; and `brain.lessons`, the lessons form a run working for it holds), on its own turns;
  - the receiver, which reads the switch on the settings page, carries no brain index, hint or due line in the
    room's block and asks the gate for no core for its turns (the receiver's `docs/drafting.md`, 「A supervisor
    whose brain is off」).
  The base persona a `window`-shaped supervisor's system prompt ends with is its identity, not its memory, and
  stays. The brain's memory tool asks only `room.tools`, which other tools share, so the switch does not take
  its entries away; the tool limit below does, where it leaves `room.tools` out.
- **The tool limit** narrows what the supervisor's sessions hold, in the shape a role's session's
  `toolsAllowed` has (`docs/api.md`, 「sessions」): a list of capabilities, or none for everything, as every
  supervisor holds by default. A list is the whole of what its sessions keep, on its own turns, and it
  never widens them (「What each capability stands for」 below):
  - of the tool host's entries, only those asking a capability it names, since the session record keeps only
    those of the chat line's and the supervisor bundles' capabilities it names;
  - of the servers the chat configuration starts outside the tool host, only those standing under a capability
    it names; every other is not in the supervisor's MCP configuration, which is strict, so no server the
    configuration does not name rides on the program either;
  - of the supervisor's own program's built-in tools, only those standing under a capability it names and the
    one the chat line keeps for every limited supervisor; no other built-in tool of the program is there, a tool
    the map below names and one a later version of the program adds alike. A run it hands off holds its
    role's own (「What each capability stands for」 below).
  A save names only capabilities the supervisor could hold; any other is refused.
- **The persona** is a text of at most the supervisors policy's length, or none, and a persona left empty is
  none.
- **A save is checked whole before anything is written** and overwrites what it changes, marking the
  settings it changed as saved. A field no save names is refused, as is a value outside its setting's bounds.
  A save that changes the reference switch, the brain switch or the tool limit starts every room afresh, as one
  changing the generation settings does, since each changes what a session is handed at its start.

### What each capability stands for

- **What the chat line holds outside the tool host stands under a capability the run-lines policy declares**
  (`policy/run-lines.toml`, the chat line's `held`), and a limit that leaves the capability out leaves all of it
  out:

  | Capability | Servers of the chat configuration | On a node, in their place | The program's own tools |
  |---|---|---|---|
  | `jobs.file` | `job`, `agency` | the tool host's `agency` namespace | none |
  | `web.pages` | `explorer` | the tool host's `web` namespace | none |
  | `documents.file` | `artifact` | the tool host's `artifact` namespace | none |
  | `program.shell` | none | none | `Bash`, `BashOutput`, `KillShell` |
  | `program.files` | none | none | `Read`, `Grep`, `Glob`, `LS`, `Write`, `Edit`, `MultiEdit`, `NotebookEdit`, `NotebookRead`, `EnterWorktree`, `ExitWorktree` |

  A supervisor whose program stands on a node is handed, for each capability the limit keeps, the namespace of
  the third column in place of the servers of the second, its record holding the capabilities those entries ask
  (`docs/person-nodes.md`, 「A supervisor's tools on a node」).

- **Every other capability stands for the tool host's entries that ask it,** reached through the tool host's
  namespaces in the chat configuration, and its grant lock refuses an entry whose capability the session record
  does not hold. `room.chat` is the brain's tools and the supervisor's records; `room.tools` carries, among
  others, `bmo/worker_ask` and `bmo/app_put`; `web.read` is the tool host's web search and page fetch.
- **The program's own web search and fetch, its subagents and its schedulers stand under no capability.** The
  chat line denies the web tools and the schedulers to every supervisor (`WebSearch`, `WebFetch`, `Monitor`,
  the cron and wake-up tools) and leaves it its subagents (`Agent`, `Task`), for reading and searching
  inside its turn and for work in its own background that ends within the session (「A supervisor's long
  work」 below). A limited supervisor's program is started with none
  of them, nor with `Workflow`, `SendMessage`, `Skill` or any built-in tool the table does not name.
- **A limited supervisor's program keeps one tool no capability gives:** `ToolSearch`, which loads the
  definitions of the MCP tools it holds and reaches nothing else (the chat line's `limited_keeps`).
- **How the program is narrowed:** a limited supervisor's program is started with the program's own list of the
  built-in tools it may have, those it keeps and no other, beside the chat line's denied list with every tool
  under a capability the limit leaves out added to it (`docs/launch-lines.md`, 「What a line's program is allowed,
  by road」). A supervisor with no limit is started with no such list, so its line stays the one every
  supervisor's warm prefix stands on.
- **A limited supervisor's turn goes out on Claude Code alone,** on its own road or on a gateway road. A rollback
  harness, which takes its own tools whole or not at all rather than by name, refuses the turn.
- **A run a supervisor hands off holds its role's tools.** `bmo/worker_ask`, under `room.tools`, starts a run
  of the worker role on the worker line, and that run holds the worker line's own program tools,
  the shell and the file tools among them (`docs/launch-lines.md`). A limit meant to keep a supervisor away from
  files and the shell leaves `room.tools` out with `program.files` and `program.shell`.

### What confines a session allowed files

- **A session holding `program.files` or `program.shell` reads and writes as the user its program runs as,** on
  the machine it runs on, whatever that user may reach; the tool limit narrows nothing below that.
- **A supervisor of an account works on its person's node or its isolation node** (`docs/account-isolation.md`,
  「The scope」): its program runs as that node's own user, and on this machine an isolation node reads and
  writes inside its own home alone, so the data root, its `env/` folder with the units' token and settings files,
  and every other account's home are refused it.
- **A supervisor of no account, or of the administrator's account, runs as the owner's own user on this
  machine.** Allowed files or the shell, it reads everything that user reads: the data root, every unit's token
  and settings files under `env/`, every room's files and every account's data; and what it reads goes to the
  provider its road names. Such a supervisor is set on a road whose engine is not the owner's own on the owner's
  word alone, or with a tool limit that leaves `program.files`, `program.shell` and `room.tools` out.
- **Whether a road's engine is the owner's own is its provider's declaration** (`docs/provider-presence.md`),
  answered as `ownersEngine` on the provider (`docs/api.md`, 「providers, models and model files」): the
  registry's own engines and Claude Code under the owner's accounts declare it, every other provider does not.

### Its generation settings

- **The settings an engine reads as it answers,** each by the name the engine reads it under, a number
  within its bounds or a switch, as the generation policy declares them (`policy/generation.toml`): the
  answer ceiling held under the registration's window, and the rest sent as the run's sampling, a setting the
  chat template reads under the template's own field.
- **A setting it leaves out is the engine's own default,** which agency neither sets nor reads.
- **A save that changes how its answers are generated starts every room afresh:** it asks a reset of every
  room session the supervisor holds, each carried out by its handover (「Clearing it」 below), so no
  conversation goes on under settings it was not begun with and none goes without its note.

## The bundles a supervisor holds

- **Every supervisor's session record holds the chat line's capabilities and the supervisor bundles,** the
  same for every supervisor, read from the roles policy's `supervisor` list: `room.chat`, its own records
  and the TODO; `room.send`, sending into the rooms it answers under its own name; and `web.read`, web
  search and page fetch (`docs/agent-roles.md`, 「The bundles」).
- **The chat line itself holds `gate.translate`, `gate.reference`, `research.ask` and the voice namespace,**
  whatever the supervisor, as `docs/launch-lines.md` declares.
- **The capabilities that operate the platform are the administrator's supervisors' alone:** `deploy` and
  `research.owner` stand in the record of a supervisor of no account or of the administrator's account, and in no
  person's supervisor's (`docs/person-nodes.md`, 「Who reaches a node」).
- **Every supervisor's program is handed the same servers,** whatever its scope and shape: the tool host's
  namespaces, the job service's plugin and agency's own, so every supervisor reads the queue, files items,
  assigns a worker and launches an item's job (`docs/launch-lines.md`, 「What a line's program is allowed,
  by road」). A supervisor whose program stands on a node is handed the tool host's entries that carry that work
  in place of the servers that run with this machine's addresses and secrets (`docs/person-nodes.md`, 「A
  supervisor's tools on a node」).
- **Every tool acts as the supervisor whose turn calls it:** its memories, notes and reminders are its own,
  a send goes out under its own name into a room it answers and nowhere else, and an item it files goes to
  its own repository. The tool host's grant lock judges an acting call the same way for every supervisor.

## Who a send goes out as

- **A send made in a supervisor's turn is that supervisor's.** The turn's run names it, and every sending
  tool and script reads it for every call.
- **Three things are asked before anything goes,** each refusing with a status code and one English
  sentence that names the supervisor: it answers the room — the room names it among the supervisors that
  answer in it, or the room names none and the receiver's setting names it as the supervisor that answers
  such rooms; its row holds `room.send`; and the room's road carries the verb.
- **It is filed under the supervisor's own number and name** on a road that speaks for several speakers;
  which supervisor is the road's own voice, naming no speaker, is the receiver's setting.
- **Only a supervisor's own round sends, and no send carries words.** Words reach a room only as a round's
  message, which its instruction stages review: a line, a spoken line, a voice note file or a caption the round
  asks into its own room is refused, and one it asks into another room is handed to that room's supervisor,
  whose round there writes and reviews it.
- **A supervisor asked to say something aloud says its round's message in its own voice.** Its round asks the
  send tool's `speak`, and the message, reviewed as every message is, goes out as a voice note in the voice its
  row names, the voice unit speaking it as the supervisor speaking as itself, which its face plays (the
  receiver's `docs/voiced-notices.md`, 「An answer said aloud」). Any other sound, another voice or words read in
  a voice not its own, it makes with the voice unit's `voice/generate`, reached through the chat line's `voice`
  namespace, and sends as a file: generated media, which no face plays. The manual's 「A LINE FOR THE VOICE」
  says when each fits and that a voice briefing turn takes no tool call. A send anything else asks — a job, a role's run, a person at a shell, a
  script — goes out nowhere: the receiver hands it to the room's supervisor as a quiet instruction, and that
  supervisor says it as its round's message or lets it go (the receiver's `docs/messages.md`, 「Words handed
  to the room's supervisor」).
- **Another supervisor's room stays closed:** a supervisor never sends into a room it does not answer.

## One runner

- **One runner carries every supervisor's turns.** A caller (the receiver) hands the words that woke a
  supervisor, and the quiet instructions queued for it, to the runner the same way for every supervisor;
  the runner reads the row for every turn and runs it by what the row says.
- **What the runner does the same for every supervisor:**
  - the room's queue: words gathered for the queue's delay, the window that keeps it listening, and quiet
    instructions taken into the next turn; in a turn of the window shape, a wait the session asks for and
    words said while the turn works put into it where its provider takes them;
  - one turn at a time per supervisor, since its turns go into its one program;
  - the session the turn goes on in, kept by the supervisor's session scope;
  - the turn run in the supervisor's resident program, on its road;
  - the answer sent into the room under the supervisor's own identity, and handed back to the receiver as
    any word filed there;
  - the session handed over at its compaction threshold in the way its provider declares (「What a provider
    declares for a conversation」 below).
- **What each row says:**

  | Setting | What it says | A supervisor of one session across its rooms | A supervisor added on the console |
  |---|---|---|---|
  | `session_scope` | `agent`: one session across every room it answers, each turn carrying its room's block; `room`: one session in each room | `agent` | `room` |
  | `turn_shape` | `window`: the chat documents as its system prompt (`bmo-window.md`); `pieces`: the platform's harness opening, the pieces role and its persona as its system prompt (「What a turn is given」 below). Either shape's round runs the receiver's draft process, the same questions, review and revision (`drafting.md`) | `window` | `pieces` |

- **The scope and the shape go together:** a turn of the window shape goes on in the one session of the
  `agent` scope, and one of the pieces shape in the session of its room.
- **Every supervisor is the same kind.** The scope and the shape are settings of its row, and nothing else
  tells one supervisor from another. Which supervisors answer a room is the room's: the receiver keeps
  each room's list of the supervisors that answer in it, and the supervisor that answers a room whose list
  is empty is a setting of the receiver's; agency holds neither.

### The session's scope

- **A supervisor of the `agent` scope keeps one session across every room,** each turn carrying its room's
  block.
- **A supervisor of the `room` scope keeps one session in each room it answers** (「A session in each room」
  below). Its one program goes on in the session of the room the turn is for: a turn for another room changes
  the command's session, and the program is started again on it in the same pane.
- **One turn at a time per supervisor,** whatever its scope.

### What a provider declares for a conversation

- **How a full session is handed over:** `fold`, the program's own fold put into it as a turn (Claude
  Code's `/compact`); or `handoff`, the session asked for a note in one more turn and a new generation
  opening with it (「Its limit」 below). The registry's provider hands over; Claude Code's folds.
- **Its default compaction threshold:** the share a supervisor's row is seeded with where it is added on
  that provider and names none. DeepSeek and OpenRouter declare 0.5; a provider declaring none gives 0.85.
- **The limits that cut a turn going nowhere,** where the provider declares them (「A turn that goes
  nowhere」 below). The registry's provider declares them.
- **The wait for an engine a job holds:** a turn on a registration an open job runs on waits until the job
  ends (「The engine it shares」 below). The registry's provider declares it.
- **Nothing else in a conversation turns on the provider or the model.**

## Where a supervisor's turn runs

- **On its road, judged as the turn starts:** its provider must have quota left and be able to
  answer now (`docs/provider-order.md`, 「Whether a road can answer」). The turn goes out on the model, the
  effort and the thinking the road names.
- **A road that cannot take the turn fails it plainly.** The turn ends failed at once with the sentence that
  says why — the road, its reason, the moment it reopens where a window names one, and what a person does
  where only a person can reopen it — and nothing stands in: no other provider or model answers, and the turn
  does not wait. A road that fails under the turn — a limit met, a credential or balance refused, the
  provider unable to answer — ends the turn failed with that failure in the same way (`docs/provider-order.md`,
  「A road that fails」). On Claude Code the accounts are still walked inside the road
  (`docs/cache-fallback.md`, 「The Claude Code road is a list of accounts」). The next turn judges the road
  again.
- **A road the owner changes replaces the supervisor's sessions through their handoff,** whatever its scope:
  - what moves a road: its provider, model, effort, thinking or answer ceiling changed on the row's route or
    the settings route, a registration pinned anew, and a change of the registered models that leaves the
    program standing on a model its road no longer names or on an address its registration no longer holds;
  - the replacement is owed as the change is written and made while the agent is quiet: the one session
    across rooms is asked through the receiver, its handoff turn going out on the road the session last
    answered on, each room's own session writes its note in one more turn
    on the road it was opened on (「A session in each room」 below), and only then is the session let go and
    the program standing on it taken down, so the next turn stands the road the row names now;
  - a turn running as the road moves finishes where it went out, and its end makes the replacement; no
    change of a road signals a program a turn holds;
  - a fresh session of a supervisor is never opened without the handoff (`docs/sessions.md`, 「A restart
    hands the session over」).
- **No supervisor holds a slot.** A supervisor is not a role and has no cap: its turns are bounded by its
  one program.
- **Its home is its own folder under the data root, `supervisor-homes/<name>`,** kept across deployments:
  its shell starts there, the files it makes for itself go there, and on Claude Code its program's records
  are named after that folder's real path (`docs/sessions.md`).

## A session in each room

- **One session for each supervisor of the `room` scope in each room,** a row of the room sessions table
  keyed by the agent and the room: the harness session its turns there go on in — the id it goes by, the
  provider and model it was opened on and the place it stands on — with its generation and when it opened,
  the last message the agent took, the hash of the system prompt its last turn was sent with, when that
  last changed, and what the runs on the words not yet taken have spent (「A turn that goes nowhere」
  below).
  - It is a table of its own because such a supervisor holds one session in each room it answers, where a
    supervisor of the `agent` scope keeps one session across every room and a role keeps its sessions by
    role, path, provider and model (`docs/sessions.md`, 「A role's session」).
- **Each turn goes on in that session.** The supervisor's program stands on the room's session — the one
  it stood on, or the same program started again on this room's in the same pane where it stood on another
  room's — and the turn's prompt is the round the receiver composes: its new words with the room's block, the
  reference and the draft process's question, never the conversation again. The program keeps that, so the turn
  goes on from the one before and the engine computes only what is new (「The session's scope」 above).
- **One turn at a time in the supervisor's program.** Turns of two rooms take the supervisor's own lock in
  turn, since they go into one program.
- **The system prompt is handed to the harness as a file,** composed for each turn (「What a turn is
  given」 below). It is the same bytes from one turn to the next until what it is composed of changes.
- **A changed system prompt hands the session over,** since the program standing in the pane read the old
  prompt when it started and a resume keeps it:
  - whether it changed is read off the prompt actually sent: the SHA-256 of the system prompt against
    the hash the session keeps from its last turn — a room's session row, or the supervisor's row where
    one session serves every room;
  - a room's own session writes its handoff note in one more turn, as it does at its handoff mark
    (「Its limit」 below), and the generation that opens holds the note ahead of the turn's words;
  - the one session across rooms is asked through the receiver while agency still names it, and the
    receiver's next turn opens with the note the standing program wrote;
  - the handover is owed, not made under the turn that found it: that turn goes on in the session the
    agent holds, sent the new prompt, and the handover is made once the turn has ended and the agent
    is quiet (`sessions.md`, 「Session replacement」); the next turn is composed from the new documents,
    and the conversation goes on from the note;
  - a supervisor whose documents did not change keeps its session, its hash still matching;
  - a session whose prompt was never recorded hands over once, since what it read cannot be known;
  - the turn's log carries a line saying the system prompt changed, which the console shows.
- **A road moved or a reset asked hands a room's session over the same way:**
  - the session writes its note in one more turn on the provider, model and place it was opened on, sent
    the system prompt composed for that road, and the generation that opens holds the note ahead of the
    next turn's words;
  - the handover is owed for every room whose session holds a harness session as the change is written,
    with why — the documents, the road or a reset — and made at once in a room that carries no turn, and
    otherwise as that room's turn ends;
  - a turn that finds a handover still owed on the session it holds makes it first and opens on the note,
    and so does a turn that finds that session on another road than its own with nothing owed, so no
    road move opens a generation without asking.
- **A new generation is a new harness session,** opened by the turn that finds:
  - no session held in the room yet;
  - a reset of the session asked for in the room and not yet carried out, after the handover that carries
    it out (「Clearing it」 below);
  - a prompt whose documents moved since its last turn (「A changed system prompt」 above);
  - the held one opened on another provider or model than its road names now, or on a place
    its registration no longer has, after its handover;
  - the held one no longer there to resume: its program keeps no record of it, which the turn says in
    the program's own words before anything is stood. That turn is asked again once on a fresh session.
- **A generation that opens holds nothing read,** and one that opens because the session it had is gone
  holds no note either: the session it replaced cannot be asked. One whose handoff turn failed or wrote no
  note opens all the same, and its session's state says the handoff was lost until a turn begun after it
  has answered, so the receiver opens that turn on what it keeps of the room's answers (`docs/sessions.md`, 「A
  restart hands the session over」). The documents the ended session had
  loaded are forgotten as the new generation opens, before its first turn is composed. They stand under the session
  id the receiver names the session by: the agent's name where one session serves every room, and
  `<agent>@<room>` where the room keeps its own. A session that goes on in the one it held keeps what it
  loaded — a plain turn, a return to another room by the one session across rooms, and the same session
  resumed after agency's own restart or a deployment.
- **What each turn was given and how it answered is kept,** generation by generation, in the room
  session turns table: the user turn exactly as it went out and the answer as the run wrote it. The
  follow pane and the console read it; no run is built from it.
- **A restart loses nothing.** The session row keeps the last message the agent took, so a message
  that woke the agent and was not yet answered when the bot stopped is taken again at the start. What
  the cut run had spent on it is kept with it, so the restart takes the turn up within the same net
  rather than afresh.

### Its limit

- **The window is its registration's.** The harness is told the registration's context window and
  answer ceiling as the run starts (`docs/own-llm.md`, 「How requests go」).
- **A room's own session hands itself over at the handoff mark when its provider hands a full session
  over by a note,** as the registry's provider does (「What a provider declares for a conversation」
  above). This is the session of a supervisor of the `room` scope, which agency carries. A turn whose
  context — its last request, as its harness reports it — has reached its compaction threshold
  (`docs/sessions.md`, 「Session compaction」), so with a margin of the window left, is followed by one
  more turn of the same session, after its answer has gone out and before the room's next turn:
  - the turn is the session itself asked for its handoff: its prompt is the handoff instruction,
    `prompts/agents/pieces/handoff.md`, sent as an ordinary turn on the very run the room's turns go out
    as, so the engine reads only the instruction past what it already holds and the session writes the
    note with the whole conversation in front of it;
  - the session consolidates its brain first, in the same turn and with its brain tools: the first of its
    unconsolidated episodes the brain answers, as many as the turn can afford and never all, each draft that
    passes the review written with the episodes it rests on as its grounds and every episode read marked
    consolidated, the consolidation the receiver asks of a session before a fold (the receiver's
    `docs/unified-bmo.md`, 「The brain's consolidation」). It calls no `brain_fold`: the receiver reports the
    fold to the brain as it releases the generation that ended. Only then does it write the note, which is the
    turn's answer, so every handover of a room's session — its mark, its documents, its road or a reset —
    lets no session go with its episodes unread;
  - nothing of it goes into the room and no word of the room is taken by it: it is no round of the
    room's, its usage is the supervisor's, its log is a run of its own, `handoff:<room>:<time>`, and the
    instruction and the note are kept as the last turn of the generation they close;
  - the note opens the next generation: the room's session opens a new generation with no harness
    session and the note held on it, and the next turn opens a fresh harness session whose first prompt
    stands the note at its head; the note is let go once that turn answers, and the turn is kept with
    it as it went out;
  - an answer that hands nothing over leaves the session as it was, its log says why, and the next turn
    that ends at or past the mark asks again.
- **The one session across rooms is the receiver's to replace.** A supervisor of the `agent` scope keeps
  one session every room's turns go on in, and the receiver judges when it is full: where its provider
  hands a full session over by a note, the receiver writes the owner's word to start it afresh with the
  reason `full`, and the session's next turn replaces it through the handoff of its own — the same
  handoff note a replacement leaves (`unified-bmo.md`, 「Replacing the session」). Agency runs no handoff
  turn of the mark for this session; the session the receiver asks is the one standing in the pane, and
  the note its successor opens with is the receiver's.
- **Why a turn and not the harness's fold:** the harness's own `/compact` folds on a request of its
  own shape, which the engine has no saved state for, so the engine would read the whole conversation
  again — on a long session, many minutes — where an ordinary turn on the same session reads only its
  new words.
- **The harness's own fold stands past the handoff mark, as a guard.** Claude Code folds a
  conversation unasked only near the end of the window it was told, a margin short of it, on its
  built-in instruction, which no hook or setting of agency's can change. On a wide window that margin
  stands past the mark, so between turns the handoff comes first and the harness's fold is reached only
  by one turn that grows past it in its own tool calls. It is left on: switched off, a session that
  once grew to the end is refused as too long on every turn after. A window narrow enough that the
  margin reaches below the mark has the harness fold first and the handoff never come.
- **The window is the engine's whole context:** when other work takes a card, the engine waits at
  that card's stage rather than running on fewer cards (`docs/work-nodes.md`), so the window does not
  shrink and a session goes on as it was.
- **A turn at the limit fails** with the harness's own line about it (`docs/own-llm.md`, 「A turn at
  the input limit」).

### Clearing it

- **A reset of a room's session is an ask, carried out by the session's handover**
  (`docs/session-resets.md`): the session consolidates and writes its note in one more turn and the new
  generation opens on it — a clean reset's turn, sent `prompts/agents/pieces/handoff-clean.md`, consolidates
  the same way and writes none, and the generation opens on nothing, no handoff lost — at once where the room carries no turn and otherwise once its turn ends, so a turn running when the
  ask comes ends on the session it began on. A room whose session holds no harness session has nothing to
  hand over, and its next turn opens the new generation before anything else.
- **The session cleared is kept:** the harness keeps the old session's record under the supervisor's
  home, and the room session turns table keeps its turns under their generation. The new generation
  starts from the last message the old one took, so nothing said before it is replayed.
- **Who asks:**
  - the owner, with the reset control of a room session on the supervisor's settings;
  - a save of the supervisor's generation settings, which asks it of every room it holds a session in
    (「Its generation settings」 above).
- **The program on the session is taken down once the handover is made** (「The program on an ended
  session」 below), so nothing shows or keeps the cleared session after it.
- **The handover that carries it out** settles the ask, and its run's log says why it handed over.
- **The program's model is the one the supervisor answers on:** the model its road names
  (`docs/provider-order.md`, 「Whether a road can answer」) — on the registry's provider resolved to the
  registration its road names and an address that registration holds now — never the row's
  `last_model`. A road changed or a registration that lost the
  address the program stood on hands the session over and then lets the old program go (「Where a
  supervisor's turn runs」 above); the next turn stands the model the supervisor answers on now.

### The program on an ended session

- **The resident program stands only on a session one of the supervisor's rooms goes on in.** A turn
  stands it on its room's session, and between turns it stays standing there, its pane showing that
  conversation.
- **Ending a session takes its program down at once,** rather than leaving it to the next turn:
  - a turn its guard cut short, for the session the cut ended (「A turn that goes nowhere」 below);
  - a reset of a room's session carried out, for the session that room held, once its handover is made
    (「Clearing it」 above);
  - a session its road moved or its prompt's documents moved, replaced outside a turn (「Session replacement」
    in `docs/sessions.md`): the handoff is asked first where one is had, then the session goes and its program
    with it, and the next turn stands the road the row names now;
  - a reset of the supervisor's conversation, for whatever session it stands on, every one of them the
    conversation's (「Starting it afresh」 below), or, where an overtaken turn holds it, by that turn as
    it ends (「What the reset cuts off」 below).
- **Why at once:** the program holds the ended conversation for as long as it stands. Its pane shows
  it, a person typing there goes on in it, and its loop starts it again on that session if it ends; a
  restart of the service leaves it standing as well. Taken down, the place holds no program, its pane
  serves the host, and the next turn in any room stands the program on the session that turn goes on
  in.
- **It never cuts a turn.** It is done only under the supervisor's own lock, taken without waiting;
  where a turn holds the lock, that turn has stood the program on its own session, and nothing is taken
  down.
- **Only a program on an ended session goes:** one standing on another room's session is left as it
  is. The session token the program carried is taken back from the tool host with it.

## Starting it afresh

- **A reset of a supervisor takes what it has said, what it remembers, or both, out of the system,**
  so it answers again as on its first day in that respect, under the settings it holds now. It is the
  owner's alone: the supervisor's page and `bmo-agency supervisor reset <supervisor>` ask it, and nothing
  else does.
- **It is a route of agency's server,** `POST /supervisors/{supervisorName}/resets`, a dry-run read of
  what it would take and a write that takes it, so it runs on the store wherever it stands and the verb and
  the console only ask it (`docs/api.md`, 「supervisors」).
- **The caller names the rooms the supervisor answers,** those it answers alone and those it shares
  with other supervisors, since which supervisors answer a room is the room's owner's; agency reads no
  room's list of them, and judges only its own rows by the rooms it is handed.
- **What each range takes of agency's is agency's; the rest is its owners',** told by the reset's
  event as it ends (`docs/api.md`, 「Events」, `supervisor-reset.created`), so each unit takes its own
  rows:
  - the conversation range takes the supervisor's room sessions and their kept turns, its run log lines,
    the run log lines of the rooms it answers alone, its runs in the run ledger and those of the rooms
    it answers alone, the records its harnesses keep of the sessions run in its home and the prompts
    their histories kept of them; the rooms' own records — their messages, judgements, usage and the
    windows they hold open for it — are their owners', and so are the records a turn wrote down under
    its run; the event carries the run names, since a chat run's name says its room but not the
    supervisor that ran it;
  - the person range is the brain's: agency asks the brain to let go of the supervisor's own brain whole,
    found by its key, so nothing it remembered comes back (`docs/api.md`, 「Calls agency makes」, the
    brain); agency takes the records its harnesses keep of the sessions run in its brain folder;
  - a harness's records of a session are looked for in every configuration folder its program may have
    run under, since the one a turn ran under is the login's or the road's it went out on: for Claude
    Code the machine's own and the home of every Claude account agency holds, and for a road of
    Claude Code every home under the road's folder; the prompts their histories kept are taken out of
    each of those folders' histories alike;
  - what it keeps, whichever range — its supervisor row and road, its settings, its gate
    switches, the rooms themselves, the registry and the engine's placement, and whatever range was not
    named.
- **A dry run reads the same,** agency's rows counted and the brain asked for a preview, nothing
  written.
- **Once begun it runs to its end,** apart from the request that asked it, so a caller that stops
  waiting — the console's request running out, a verb killed — loses only the answer and never leaves
  it cut between the steps.

### What the reset cuts off

- **Nothing of what the reset took is written again after it,** by a turn that was running or by words
  that were waiting, whichever range it named.
- **The reset point** is the moment the reset cut the supervisor off, kept in the store's settings under
  its name. One point stands per supervisor; the full read and the read of the point alone both answer it
  from the store (`docs/api.md`, 「supervisors」), and a consumer that reads it on every turn it sends reads
  the point alone. The reset moves it to now twice: as it begins, before it takes anything, and again once
  the store holds nothing of what it took, so a turn begun while it ran, which may have read what it took,
  is cut off too. Each move is published with the point on `supervisor-reset.created` (`docs/api.md`,
  「Events」).
- **A turn begun at or before the point is overtaken.** A turn begins as its round takes the words it
  answers; the handoff after a turn is a turn of its own here, begun as it starts.
  - The reset stands a cancellation over every run of the supervisor going as it cuts it off — the runs
    agency's own `supervisor-run-going` named going and not yet ended — so a run under way stops where it
    stands, as a run its guard cuts does.
  - Every call of its tools and every verb its shell runs is refused whole from then on, and the
    refusal tells it to end the turn.
  - It writes nothing of its own: no answer goes into the room, and no kept turn, usage, road or
    handoff is written. The run log it wrote under its run is taken back, so the turn leaves no
    record of itself, its first line included.
  - A turn holds the point off whenever it writes, reading the point again under the hold, so a turn
    writing as the reset begins writes whole before the point moves and the reset takes it after, and
    one reading the point after it moved finds itself overtaken and writes nothing.
  - The program it stood goes as it ends, where no room goes on in that session any more: killed where
    it still stands there, and the records its harness keeps of the session in every configuration
    folder they may stand in (「Starting it afresh」), the prompts its history kept of it and every line
    its place's records hold of the session removed after it, since the reset left them to the turn
    holding them.
- **What waited for its turns is let go unread,** in every room it answers: the words its rooms'
  queues were gathering, those its conversations held or waited on, and its quiet instructions, whose
  rows are settled as delivered so no turn takes them again. Those are the rooms' owner's, which lets
  them go on each move of the point. A run an overtaken turn launched answers nothing either: its result
  is dropped where the run began at or before the point or the reset took its row. A round left with
  nothing to answer runs no turn.
- **The program standing in its place is taken down where the conversation range is named,** once the
  rows are gone, whatever session it stands on, since every one is the conversation's (「The program on
  an ended session」 above), so the pane no longer holds a conversation whose record was removed. It is
  killed rather than asked to end, and waited on until it is gone, so a program asked to end does not
  write its session's record back after the reset removed it. Where a turn holds it, the turn is one
  the reset overtook, and takes it down itself as it ends.

## The engine it shares

- **One engine, one slot, many sessions.** A supervisor's registration may be served by the same
  engine jobs run on and other callers ask. The engine itself keeps each session's state apart,
  saving the slot and loading the matching saved state whenever a request does not go on from what the
  slot holds (`docs/work-nodes.md`), so no request reads another's from the start.
- **While a job holds the engine, the supervisor's turns wait,** as every supervisor's turns on a
  registration do, since the registry's provider declares the wait. The engine is held when an open job
  runs on the supervisor's registration. Both are read from rows: the job's from the run it noted, or
  before it noted one from its repository's pin, and the supervisor's from its road; a run, a pin or a road
  resolves to the registration its model names, where the registry holds one.
  - Every room's turn of that supervisor waits, looking again every few seconds, until no such job is
    open. Words that come in meanwhile, and quiet instructions of its own, join the waiting turn and
    are answered with it.
- **The room shows the wait.** The turn's running line in the room says the agent is waiting for its
  engine and which item's job holds it, from the moment it starts waiting until the turn begins. The
  turn sends a progress of its own in a waiting phase naming the job and its item, and stands in the
  bot's picture with the job it waits on, so a screen opened mid-wait reads the same line.

## What a turn is given

- **A turn of the `pieces` shape is given a system prompt of the layers `docs/prompt-layers.md` states, each
  whole, in this order:** the harness, the platform's rules where the supervisor is the platform
  administrator's, its organisation's instructions where it has any, the pieces role and the supervisor's
  persona unless the delegation chain chose none, as every supervisor's is. No section of it is chosen or
  ordered per supervisor.
  - **The harness** is the platform's harness opening, `prompts/harness.md`, its values filled in. On
    Claude Code the layers are one whole system prompt in the program's own place; on a program
    that is a harness of its own and carries its turns through its inbox, as dsh does, what is composed —
    every layer after the harness — goes as its persona suffix after its own prompt, and the platform's
    opening is not sent.
  - **The platform's rules** are the platform's rulebooks the supervisors policy names for this shape
    (`policy/supervisors.toml`, `pieces_platform`), for a supervisor of no account or of the platform
    administrator's account alone.
  - **The pieces role** is the same for every supervisor of that shape: the rulebook against destroying
    the system, the absolute rules, the frame with its name and its tools, the turn, and the rulebook on
    another agent's words, in that order (`policy/supervisors.toml`, `pieces_role`; `prompts/agents/pieces/`).
  - **Its name** is filled into the frame: the label it stands as in the rooms, its registration name where
    it holds no label, and the call names a person calls it by, its label where it holds none (「Its call
    names」 above). The frame tells it that this is its own name, to answer and speak of itself by, and that
    BMO is the platform's name. A supervisor of the `window` shape is told who it is by its own documents.
  - **Its folder** is filled into the frame's tools: by its path for a supervisor the platform's rules reach,
    and as the folder its shell starts in for any other (`docs/prompt-layers.md`, 「What every agent is
    told」).
  - **The persona** closes it: the persona its settings hold, else its own prompt folder's `persona.md`;
    none where the delegation chain chose none for the person it serves, so it keeps only its name and its
    picture.
- **Its turn** is the round the receiver composes, the same for either shape: its new words, the room's block
  and the draft process's first question, as the session it goes on in keeps the conversation (「A session in
  each room」 above). The round then runs that process to its end — its questions, its review and a revision a
  rejection asks for — as a turn of the window shape does; the shape changes the system prompt and the session
  scope, never the rounds (`drafting.md`).

## What a turn shows

- **The same turn events every supervisor's turn flows,** for the room: the turn started, waiting for the
  engine, the answer being written, sent, or failed with its reason. The start and the ending name the
  supervisor as the turn's speaker, by its number in the room and its name, so no screen tells one
  supervisor's turn apart by anything else.
- **The turn stands in the bot's live picture** for as long as it runs or waits, under the same
  speaker, so a screen that opens or reloads in the middle of it still shows it, and a stop of the bot
  waits on a running one. Its run is an execution of the supervisor's own host, which a stop reaches as
  it reaches any agent's.
- **Its own pane shows each turn as it goes:** the head pane of the agent's workspace is its program's
  own screen, where a person reads its tool calls and its thinking as the program draws them and can
  type into it (`docs/agent-control.md`, 「One standing pane per supervisor」).
- **A standing pane shows each room session.** In the herdr workspace of the agent, named by its row's
  herdr label, each room session has a tab of its own named by the room's title, running
  `bmo-agency supervisor follow <room> --agent <agent>`, the room named by its number, out of the deploy
  tree's binary: every turn as it was given and every answer as it was written, as they are appended. A
  pane started from an older binary is replaced. The pane stands apart from the panes of every role's
  runs, and the console's supervisor page lists the supervisor's room sessions and shows each pane.
  - **The verb** prints every generation the session has had, each under a line naming its number — the
    newest with when it opened, which is all the session row keeps — then waits on the newest and
    prints each turn as it is appended, going on into the next generation when a new harness session
    opens one. A turn is headed by who it is — the room for what the agent was given, the agent's label
    for its answer — and the time it was kept.
  - **The tabs are stood** for every agent holding room sessions as the chat receiver attaches to
    herdr, on the same clock as the heads, and for one agent as a session of its opens. Every pass is
    the same: a tab whose command line is the one wanted stays, one running an older line — the verb's
    script moved, or the room gone from the agent's sessions — is closed, and a missing one is opened,
    nothing focused.
  - **A follow pane is a standing pane,** told apart by the standing mark its command line carries, so
    no sweep of leftovers closes it, as none closes a head or a dev server.
- **A failure says nothing in the supervisor's name,** but for one its engine ended. The turn ends
  failed with its reason, which the console shows on the turn's line. A turn cut short is told to the
  room in the road's own voice (「A turn that goes nowhere」 below).
- **A turn its engine ended with an error carries that error's kind on its ending,** `engineError` on
  `turn-ended` (`docs/api.md`, 「Events」), so the consumer holding the room leaves one line there under
  the supervisor's own number, in plain words chosen by the kind. The kind is the one the program's
  record and its `StopFailure` hook carry (`docs/agent-control.md`, 「What a resident turn reads」), as
  the program names it, or none where it names none.
  - The words the turn took stay untaken, as after any failed run (「A turn that goes nowhere」 below),
    and words that came in while it ran follow as the next turn.

## A turn that goes nowhere

- **A turn is watched as its run streams** where its provider declares these limits, as the registry's
  provider does for every supervisor on it (「What a provider declares for a conversation」 above), through
  the tool calls and their results the turn's log takes, and cut short at the first of these:
  - **the same tool failing six times in a row;**
  - **eight calls in a row that repeat themselves and learn nothing:** each the same tool as one of the
    eight calls before it, with an input at least 90% alike by its pairs of characters, and each
    failing or answering what that alike call answered;
  - **the last net:** 80 tool calls in the turn. No clock cuts a turn: however long its run goes, it is
    cut only by what it does. Each of its tool calls is held to the call clock, as a job's are (「A
    supervisor's long work」 below).
- **The net bounds the words, not one run of them.** Words a run did not answer stay untaken, and the
  next turn in the room asks them again: after a restart cut the run in flight, or after a run that
  failed. Each of those runs is the same turn spending again, so what they spend adds up:
  - the room's session row keeps the tally of the words not yet taken: the tool calls made and the
    minutes run, the wait for the engine not counted;
  - a run writes its tally as it goes, at each tool call and each result and once a minute, so a
    restart loses at most a minute of it;
  - a run starts with the tally the runs before it left: its calls count on from there;
  - a turn whose words already stand at the net is cut before its run starts, as any cut is;
  - a turn that takes its words, answered, skipped or cut, clears the tally;
  - repetition is read within one run alone, since a fresh run's calls are not the ones before it.
- **Repetition is read first and the net last,** since a turn going nowhere repeats itself long
  before it reaches it.
- **What a cut does:**
  - the run is stopped, and the turn ends failed with the reason, as any failure does;
  - the consumer holding the room tells it in one short line in the road's own voice, naming the
    supervisor and the reason;
  - the words it took stay taken, that line with them: the turn is kept on its generation with the
    reason in place of an answer, so neither the room's next turn nor a restart asks them again;
  - the room's session opens a new generation, so the next turn opens a fresh harness session rather
    than going on in the one that looped, which loops again when resumed;
  - the resident program still standing on the session the cut ended — the one the run went on in, or
    the one the room held where the cut came before any run — is taken down at once (「The program on an
    ended session」 above);
  - the consumer hands a quiet instruction naming the supervisor, its agent name, the room and chat, the
    reason and the turn's log to the supervisor its own setting names; the chat manual asks nothing for
    one cut and says where a repeated one is filed.

## A supervisor's long work

A supervisor answers every round in its own session, and its one program holds one turn at a time, so a
turn that runs long holds every room it answers. Work longer than moments therefore never holds a reply:
the supervisor answers the room first, in a few words saying what it will do, and puts the work behind the
reply, where the result comes back to the same room when it is done. The chat manual tells it so (`prompts/agents/chat/manual.md`); what follows is what agency holds that
rule to.

- **Two roads behind the reply, chosen by how long the work must live.**
  - **A call made apart, for work that must outlive the session.** A hosted tool called with
    `bmo use … --async`, and a command launched apart with its output in a log and `bmo/wait` called with
    `--async` and a `check` on that log, run under the tool host's record of the session token the program
    carried. Their wake is queued for the supervisor and the room it was asked in, never for a session, so it
    reaches the supervisor in that room after the turn has ended, after the program folded its conversation and
    after its session was stood afresh: the token is taken back only once no such call under its record still
    runs (`docs/sessions.md`, 「A restart hands the session over」).
  - **The program's own background, for work that ends within the session.** A shell command or a subagent the
    program puts in its own background goes on behind the reply. The turn that launched it ends at its stop
    with the work still running, so the round takes its answer as its draft at once and runs its draft process
    on it, and the program takes the process's later turns and other rooms' rounds meanwhile. The work's
    finished notice comes back to the room the turn was in as a wake of that room's own: agency hands the
    receiver an order for the supervisor in that room carrying what the notice said, and the round it opens
    answers there. Until then the work stands as running in that room: agency lists it and tells its launch and
    its end on its stream (`docs/api.md`, 「background-work」). The turn the program itself opens on the notice is no round's, and nothing of it reaches a
    room (`docs/agent-control.md`, 「Background work on a line that carries it apart」).
- **What lives only inside the program is lost with it.** The program's background work has no record outside
  the program that could carry it on: a session stood afresh, a program ended or stood again on another
  command, and a stop of the supervisor's turn end it. Where work a room waits on is lost that way, the room is
  told once, by the same kind of wake, rather than left waiting. So work that must outlive the session is a
  call made apart, and the program's background is for work that ends within it; the chat manual says so.
- **Background work is held to the call clock.** A background shell, or a shell call or background shell of a
  background subagent, whose processes burn no processor time, begin no process and write no output past the
  clock's stall, or that writes no output and stands idle across the silent window — a wait that only
  polls — is cut as a call in the turn is, with no ask opened, and the wake carried to its room says it was
  cut and why. A shell that waits on a process by a
  `pgrep -f` pattern its own command line also holds counts itself and never ends; the chat manual tells the
  supervisor to wait on its own command's output file or process id instead.
- **A stop of the supervisor's turn ends its program's background work,** within the bound
  `policy/background-work.toml` sets: the subagents by ending the program, which the next turn stands again on
  the same session, and the shells by their processes. The stop names what it ended, and every room whose work
  it ended is told once.
- **The session is free for the next round** as soon as a round's answer is handed over, whatever its
  program's background still runs.
- **A subagent inside the turn** stays the road for reading and searching whose conclusion the reply needs:
  the chat line leaves the program its subagent tool (「What each capability stands for」 above), and a subagent
  hands back only its conclusion, so what it read stays out of the supervisor's context.
- **Work whose result is code or a document is a job item,** filed and launched as the todo procedure
  says, its report coming back to the supervisor as every job's does.
- **How a turn that runs long is noticed.** A prompt line does not keep a model from holding its one
  session on long work, so each tool call of a supervisor's turn is held to the call clock's first rule,
  as a job's and a role's run's are (`docs/agent-control.md`, 「A run's tool calls are held to a clock」):
  a call whose processes burn no processor time, begin none and write no output past the call's minimum
  is stopped, a call with no process of its own is cut past the clock's bound, and the program hands the
  model a failed result and the turn goes on. No ask is opened on a supervisor's call, and its turn has no hard bound:
  a turn that keeps working is bounded by the turn guard (「A turn that goes nowhere」 above), and its room
  reads it as thinking while it goes.

## What a supervisor hands to a role

- **A worker ask:** a small coding task as a run of the worker role in a repository's checkout or the
  supervisor's scratch folder (`docs/tools.md`, 「The worker ask」).
- **A TODO item:** filed to the supervisor's repository, naming the role its job runs as, `worker` where it
  names none (`docs/repositories.md`, 「A job」).
- **An unknown role is an error:** an ask naming a role the store does not hold is refused with the name
  and starts nothing.

## What it hands another BMO

- **A line to another person's BMO, and a chat into that person's room** (`docs/relays.md`): a turn calls
  `bmo/relay` or `bmo/chat`, and agency carries the line into the receiving BMO's own session as a turn.
- **The receiving BMO answers it as another agent's word, never its person's:** the line's prompt names the
  sender, the turn is never privileged whatever the sender's turn held, and the receiving BMO may refuse it,
  which is recorded and reported to the sender.
- **Both sides read the record:** `GET /relays?agent=<name>` answers the lines the supervisor sent or
  received, and the receiving supervisor's own session keeps the turn and its answer.

## The rooms it has spoken in

- **Which rooms a supervisor answers is the receiver's,** which keeps each room's list of the supervisors
  that answer in it; agency holds no list of them (「Who a send goes out as」).
- **Where it has spoken is agency's:** every turn it runs is a run of the ledger naming its room
  (`docs/run-records.md`, 「The run ledger」), so agency answers each room its turns went into with its latest
  run there, the run a screen opens the supervisor's log at for that room (`docs/api.md`, 「supervisors」).

## What a session's state names

- **The supervisor it is for,** so a consumer whose own settings are kept per supervisor — the receiver's
  draft questions among them — reads the supervisor the session runs for; and its compaction threshold as
  `foldRatio`, read afresh from the row as the state is read (`docs/sessions.md`, 「Session compaction」).
- **The supervisor's one road,** `provider` and `model`: the road its row names, which every turn of it goes
  out on, read afresh as the state is read; a room's session that stands on a harness session reads the road
  that session was opened on, which is the row's road unless the owner changed it since. No other provider or
  model is ever named for a supervisor's session.

## Adding and removing a supervisor

- **A supervisor is seeded by the deployment or added on the console.** A seeded one stands again at
  every start, so it is never removed: its row says `seeded`. One added on the console
  answers the rooms the receiver names it in; the console asks the receiver, which writes the room of the
  console's own, the supervisor's number there and the room's list of the supervisors that answer in it,
  and asks agency's `POST /supervisors` for the supervisor's half (`docs/api.md`, 「supervisors」).
- **`POST /supervisors` writes the supervisor's row,** once every field has passed, in the order the
  console's dialog shows them, nothing written until all have passed:
  - the name follows the supervisors policy's pattern, and no supervisor holds it yet;
  - the label is given;
  - the road names a provider that carries the chat line and a model it holds — on the registry's
    provider, an offered registration — with an effort the model takes (`docs/provider-order.md`, 「What a
    road is」);
  - the repository its items go to, where it names one, is a repository the store holds
    (`docs/repositories.md`).
- **The row it writes:** the `room` scope, the `pieces` shape, the label, the
  supervisors policy's mark for one added on the console, the road it is added with, warmed on its
  provider, the repository, its owner, the brain folder `agents/<name>`, no prompt folder and not seeded.
  The owner changes the road on the supervisor's page like any supervisor's. Its gate options
  are the gate's. Nothing is restarted: the runner reads the row and its settings for every turn, and its
  place and program are stood by its first turn.
- **Then its brain is made.** Once the row is written, the brain is asked whether it keeps a brain
  under the supervisor's name, and to make one where it keeps none, filed under its owner's id as the
  account key, or the deployment's where it names none (`docs/api.md`, 「Calls agency makes」). The
  supervisor stands whatever the brain answers: a brain that cannot be reached, or refuses, is logged, and
  no later step makes the brain in its place. A brain made this way holds no memory; only the install's first
  chat agent is given lessons to start from (below).
- **A room forgotten never takes a supervisor.** A supervisor is its owner account's, never a room's, and only
  its own removal below ends it. Its row carries the room its session last ran in, so the
  forgetting of a room (`DELETE /room-holdings/{roomId}`, `docs/api.md`) declares every row of `supervisors`
  as one that stays: the room's turns and runs go, the supervisor answering there does not.
- **`DELETE /supervisors/{supervisorName}` takes everything of its own,** and only of a supervisor the
  deployment does not seed:
  - first a reset of its conversation, every room the caller names read as one it shares, so each keeps
    its record (「Starting it afresh」), and of its person range where the caller lets its brain go;
  - then in one write its settings, its base prompt choice, its session reset asks, its gate switches,
    its sampling, its reset point and its row;
  - then what stands for it: its program killed, so it writes no record of what the removal took, its
    workspace closed, its place and home folder taken away, its execution rows with their envelopes —
    an open row waited on until it closes, however long that takes, since its collector writes it as it
    ends — the agent host's view and head records of it and its agent lock. What could not go is logged
    and refuses nothing, since the removal is written.
  - **Its brain is data and stays** unless the caller asks it let go.
  - **Every turn carried for it ends with it.** A turn waiting for its node reads at every ask whether the
    supervisor still stands (`docs/person-nodes.md`, 「Which node is a person's」), and a turn queued behind
    another for its one program reads it again once it takes the program; one that finds it gone ends failed,
    said to the receiver as the turn's end so its round closes, and stands nothing for it. A turn its program
    carries ends as the program is killed. The deployment's quiet counts no turn of a supervisor that no longer
    stands (`docs/sessions.md`, 「A restart hands the session over」).
  - Once begun it runs to its end apart from the request, as a reset does.
- **An account removed for good takes every supervisor filed under it** the way this removal takes one, its brain
  kept for the brain's own removal (`docs/api.md`, 「owners and organisation units removed for good」).

### A fresh brain's starting lessons

- **The install's first chat agent starts with the lessons agency ships,** `deploy/first-lessons.json`: general
  rules for situations a supervisor meets — how it reports, researches, draws, queues work and tunes models —
  written for any owner, with nothing of one owner's machines, people or projects. Each is a `situation`
  memory with its words and the situation that recalls it, labelled `source` `seed`, about no room and no person.
- **They are written once, by the install, and only into an empty brain.** The install's `first-lessons`
  after-step, run once the first chat agent is registered and its brain made, finds that brain as agency does,
  by the agent's key among the brains of the kind `agent`, and writes each lesson through the brain's own
  `POST /brains/{brainId}/memories` where the brain holds no memory of any status. A brain that already holds one,
  kept from an earlier install or written since, is left as it is, and so is every brain of a supervisor added
  later. No brain deployed means nothing is written.
- **What the owner teaches later is the brain's own:** the lessons are rows like any other, confirmed,
  replaced or closed as the supervisor's discipline says; nothing writes them again.
