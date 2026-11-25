# Sessions

How a supervisor's session is kept and resumed, told of documents that changed under it, paced at a
start, folded when its window fills and replaced when its prompt documents or its road move; and how a
role's runs keep and reuse their sessions by role, path, provider and model. Clearing a session on the
owner's ask is `docs/session-resets.md`.

**Every agent session of this build is Claude Code.** No provider names dsh or the Codex CLI; the
statements below about either are kept for a rollback to the road a deployment left.

## One session per chat

- One Claude session per chat is kept and resumed, so the supervisor remembers previous exchanges. Every
  supervisor turn is on the provider, model and effort of its one road (`docs/provider-order.md`), and
  `bmo-agency agent` shows the road each supervisor and each agent runs on.

## A restart hands the session over

- **A deployment waits for agency's own quiet before its flip** (`testbed.toml`, `quiet`; the testbed's
  `docs/deploy.md`): `bmo-agency quiet`, run from the release, asks the running server what it holds that
  the next server could not pick up (`docs/api.md`, 「health」), and answers 1 with a line for each piece of
  that work, 0 where there is none, and 2 where it could not tell.
  - **What is counted:** a turn this server carries, no job's, that no program's record names yet, so the
    pickup below would find nothing to attach to: a turn still in agency's own hands, waiting for its card,
    for the handoff of the session it replaces, or for its program to stand and take it. The pickup's own
    reading of the turn's place decides it, so the count and the pickup never disagree.
  - **What is not:** a turn a program has taken, which the next server picks up and its caller re-attaches
    to; a running job, below; a turn of a supervisor that no longer stands, which ends with its removal
    (`docs/supervisors.md`, 「Adding and removing a supervisor」); the receiver's own work, which an agency
    restart does not cut, since the receiver re-attaches to its turns and its own quiet counts what it holds;
    and a turn waiting for a person's node to be heard (`docs/person-nodes.md`, 「Which node is a person's」),
    which has that node kept beside its record. Such a turn has started nothing, and how long it waits is the person's
    PC's to say, not the deployment's, so a deployment waits on none: the next server ends it failed with why,
    and its caller sends it again.
  - **No server listening is quiet:** a server that is not running carries no turn, and the turns its
    process left are the next start's to pick up. Nor does a server whose build answers no quiet route
    hold a count to wait on. A server that answers with any other refusal could not tell.
- **A running job survives an agency restart.** Its turn in flight is kept across the restart, its run
  goes on, and the lease pass takes the item again in the same checkout; the deploy therefore counts no
  job and waits on none.
- **A job whose turn has not gone out survives it too.** A job waiting for a road, for a slot of its role or
  for its checkout, or still composing its first prompt, is carried by no turn a program holds; the job's row
  keeps the words of the turn it carries until that turn ends, and its slot request stands in the line, so
  the server that starts launches it again from them, its place in the line kept (`docs/api.md`,
  「assignments and jobs」).
- **The server drains on its stop.** Asked to stop by `SIGTERM`, it takes no new call, ends every event
  stream with a `stopping` frame, answers a turn's send still waiting with `503` `stopping` so its caller
  re-attaches once the next server stands, lets every other call in hand finish for the drain's span
  (`policy/drain.toml`), and has every event it published written to the store before it exits, so a reader
  resuming on the next server misses none; the turns it carries stay kept in flight for the next server.
- **A caller re-attaches to a turn by its key.** A send that names a key holds the turn under it: a repeat
  of the send with the same key on the same session, from the same caller or after a restart of either
  side, attaches to the turn standing under it instead of sending another, and once the turn has ended
  answers its end as kept, for the span the turn keys' policy keeps one (`policy/turn-keys.toml`). The
  key goes with the turn kept in flight, so a turn picked up after agency's restart stands under it too
  (`docs/api.md`, 「sessions」).
- **The chat works in a folder whose name a release does not change.** Each program keeps the session
  it stands in by the folder it was stood in, and a program stood again on the same folder takes the
  session up again rather than opening a fresh one beside it:
  - on a program that keeps its session by the folder's path as given — dsh, whose screen records the
    session with the folder in its place — the chat works in the deploy tree's link, whose path is the
    same while its target moves from release to release;
  - on Claude Code, which names a session's records after the folder's real path, every link followed,
    the link would be a new folder with each release, so the chat works in its agent's own home under
    the data root, `supervisor-homes/<agent>`, a real folder, as every supervisor's program does; its prompt, its
    configuration, its settings and its MCP servers are handed by absolute path, so nothing of the
    launch reads the folder it stands in.
- **A deploy moves the link's target, not the chat's folder,** so a program stood after it resumes the
  conversation it held, on Claude Code by `--resume` in the same project folder. The relaunch
  instruction a consumer writes at a deploy is the deployment briefing, not the notice of a closed
  session.
- **Agency is a session relay, and its own restart is no session event.** It opens a model session
  where none stands and relays turns into the one that stands; a turn in flight across agency's restart
  is picked up again, not lost:
  - every turn sent is kept in agency's settings while it goes, under its run (`turn_in_flight:<run>`,
    `docs/store.md`), with what its end needs to reach its caller: the session and the turn's id, the
    caller's key where the send named one, the agent, the room, the turn's run in the ledger where agency
    opened one, and the job; while it waits for a person's node to be heard, that node is kept beside it, by
    its run (`turn_node_wait:<run>`). Every turn agency carries is sent
    through the session contract, so every kind is kept: a supervisor's chat turn, a conversation's, a
    gate's, and every turn of a role's thrown-away session — a job's, a co-work branch's, a worker ask's
    and a research run's among them. A turn stops being kept only once its end
    has reached its caller, or the pickup or the pass below has handed its end back or found nothing that
    names it; each removal is written to the log with the run and why;
  - a server that starts reads each turn the process before it left there, before it reads anything else
    that process left, in the place of the pane program that carries it — the agent's place, or, for a
    turn of a role's thrown-away session (a job's among them), the place of
    that session's own, which stands while the turn goes on and is ended once its end is handed back
    (`docs/api.md`, 「sessions」) — and as that program writes its turns down:
    - on a dsh pane, against the program's record, `loop.jsonl`, by the run's name, and its inbox;
    - on a Claude Code pane, against the place's run, `run.json`, and the hooks' calls since that run
      began (`docs/agent-control.md`, 「What a resident turn reads」): a run naming the turn that the
      program took — a `submit` since the run began — and that stands open is going; one the hooks have
      closed, which they do once a stop or a failure ends the round, has ended, and its answer or its
      error is read from the session's own record, the transcript the hooks name, from the run's start.
      A program on a node names the node's path for it, which this machine does not hold, so for a place
      on a node the record read is that session's under the configuration directory and folder of the
      place's program as this machine reads them, where the link brings the node's transcript back
      (`docs/work-nodes.md`, 「The link」), as the live turn reads it;
      a run naming another turn, or one the program never took, does not name this one;
    - on a Claude Code pane of the turn's own, in no agent's place — a role's turn on Claude Code's own
      provider, a job's among them (`docs/agents.md`, 「A run in a pane of its own」) — through the
      execution its run opened: the pane's turn goes in under the run's own name (`docs/launch-lines.md`),
      so the turn's liveness row names the run, and that row holds the pane, the program and the session's
      transcript. The turn is the program's only once the program took it: words handed over stand in the
      transcript since the execution began. A turn it took is going while its program stands and the
      transcript's round from the row's start is still working, has ended once that round has, its answer
      or its error read from it, and ends failed where the program is gone with the round still open; a
      turn it never took is lost, whatever the program is doing. The pane is closed once the turn's end is
      handed back, or once the turn is found lost, and so is the pane of its own of a turn read in a place
      of its run's own — a job's turn, whose hooks write into such a place while its program stands in a
      pane of its own — found through the same liveness row;
  - a turn whose end its program has written is handed back through the path a live turn's end takes:
    what of its record the run log lacks is filed there with its end, its execution row and the place's
    run are closed, a chat turn's road, spend and ledger run are written, its end is published as
    `turn.ended` under the turn's own id, and a job's turn ends its job with its report as the job's launch
    would have (`docs/api.md`, 「assignments and jobs」);
  - a turn the program is still working on, or still holds in its inbox, is attached to: the server
    holds the place's program, so no turn of its own goes in meanwhile, waits on what the program writes
    down, and hands the end back once it is written; a program that goes away meanwhile ends the turn
    failed;
  - **an attached turn no job holds is held to the bound of an adopted turn** (`policy/resident-turn.toml`):
    its caller went away with the process before, so nobody would ever cut it, and one that has not ended
    within the bound from its start is interrupted with its surface's own keys and ended failed with the
    bound's reason, its pane closed where it has one of its own. A job's turn is held to the job's own
    bound below;
  - a picked-up turn is logged as it goes: what its program's record gained is filed into its run log as a
    live turn's is, so a screen following the log follows the picked-up turn rather than standing at the
    last line the process before wrote; a line marks where the log takes the turn up, and where no record
    this process can read names the turn, the log says so on a line of its own instead of standing still;
  - a job's turn picked up this way keeps everything its running job holds until the job ends: the job
    stays `running`, so its checkout reads as held and its slot as taken, its item's lease
    is kept on the harness's behalf as every running job's is, and no launch notice is told for its item
    (`docs/api.md`, 「assignments and jobs」);
  - a picked-up job's turn is held to the same clocks as a live one (`docs/agent-control.md`, 「A job
    turn's tool calls are held to a clock」): its calls are read from the record its program writes, a
    call that stops making progress is cut and the failed result handed to the model, a call with no
    process of its own is cut by words put into the turn, a stop cuts a blocked call, and a turn past
    its bound is cut with its item marked stopped, so the item's lease is given back and its assignment
    released; the turn's tool lines are kept in the run's log and the unit's own;
  - a picked-up turn listens for words as a live one does, where its provider takes words mid-turn: a word
    offered to its run while its program reads working is typed into the program and answered as taken, so
    words said to a job whose turn crossed a restart reach that turn (`docs/agent-control.md`, 「Words for a
    running job」);
  - a turn nothing in its place names is ended failed for its caller, its reason saying it waited for its
    node's link where one is kept beside it; a job's is let go and its job launched
    again from the words its row keeps, since no program took them (`docs/api.md`, 「assignments and
    jobs」). Either way the pane of its own, where it has one, is closed, so no program stands on a turn
    nobody waits for.
  - every turn's liveness row the process before it left open is closed first, with the ending
    `restarted` and what the server then finds of the turn's program, and the same is written to its log,
    before any place that process left is taken down (`docs/agent-control.md`, 「Telling that a turn's
    program is gone」).
  A turn picked up this way answers under its own turn id as it did before the restart: the progress read
  answers it running while it goes, with what its program's record has written of it so far, and ended once
  its end is handed back; its end is published as `turn.ended` with that id and the caller's key; and a
  send repeated with its key attaches to it, or answers its end once it has ended. The state of a
  thrown-away session a picked-up turn still runs on is read by its id while the turn goes on, from the
  execution row the turn left, and the session ends with the turn as any other.
- **A session id names one session across restarts.** A thrown-away session's id and a conversation's carry a
  number the server gives as each starts, and a server that starts gives numbers past every one the store still
  names — a job's session, a session a turn of a job ran in, a slot's session and a turn kept in flight — before
  it starts any session. So a session picked up across the restart keeps its id to itself, and words said to a
  job's session reach that job (`docs/agent-control.md`, 「Words for a running job」).
- **A program is stood afresh only between turns** (`docs/launch-lines.md`, 「What is fixed」). A
  program whose command changed while one of its turns works — a deployment that moved its line, its
  prompt or its environment — is left standing until the record says that turn ended: the turn that
  found it changed goes into the program as it stands, waiting in its inbox behind the working turn, and
  the next turn's stand replaces it. A program that goes down while a turn runs and is stood again
  resumes the same session; the turn it was on is the caller's to ask again from its own round, which
  holds the item, the message it answers and the report in hand. One session answers a room: the
  folder's record is what tells a program which session that is, so no second one is opened while the
  first stands.
- **A caller that names a folder works in it,** as a conversation opened on one does; only the chat's
  own launches, which name none, stand in the folder above.
- **A fresh session of a supervisor opened in place of one it still holds is never opened without the
  handoff.** Where a chat-line turn — one of the `agent` scope across rooms, or one of the `room` scope
  in a room — is about to open a fresh session for an agent that had one, the session going away writes
  what the next one needs before anything replaces it. A room's own session whose documents moved, whose
  window filled, whose road moved or whose reset was asked writes it in one more turn agency runs on it
  (`docs/supervisors.md`, 「Its limit」, 「A session in each room」).
  The one session across rooms is asked through the receiver's `POST /handoffs/{agentName}`, and the ask
  is waited on:
  - **what counts as replacing a session:** a session the turn does not resume — its prompt documents
    moved, the owner asked a reset, its provider, model, effort or login moved — and a session the agent
    had whose kept harness is gone, which is asked while the agent's program still stands in its pane;
  - **what does not:** a turn that resumes the session it had, and a program stood again on the same
    folder after agency's own restart or a deployment, which takes the same session up again, so
    neither asks anything;
  - **the order:** the ask goes out while agency still names the session and the old program still stands
    in its pane under herdr, before the session is let go and the turn's stand replaces it. The turn that
    waits for the handoff lends the session to the receiver for as long as the ask stands, so every turn
    the receiver sends for it, the notes the session owes and then the handoff itself, one after another,
    runs on the very session going away instead of being refused as a second turn while the waiting turn
    goes. The waiting turn takes the session back once the ask is answered, never between two of the
    receiver's turns, and a waiting turn that ends while a turn of the receiver's still goes leaves nothing
    of itself behind to hold the session busy. The old
    program usually outlives agency's own restart, so a turn after a restart that opens a fresh session
    asks as any other does;
  - **a turn of the handoff asks for none:** a turn the receiver sends on a session lent to it is the
    handoff's own, so it never asks for a handoff of its own, whatever it finds of the session; an ask made
    from inside it would wait in the receiver's line behind the very ask it answers;
  - **the road the handoff goes out on:** a turn of the handoff goes out on the road the session last
    answered on, the provider and model the supervisor's row keeps as its last answer, where the row now
    names another, so the program that holds the session writes the note; on Claude Code it resumes that
    session. A fresh session on Claude Code never resumes a session another road left, since no Claude
    Code login holds it;
  - **once a minute at most, and never skipped:** an ask for the same session within a minute of the last
    one (`policy/handoff.toml`, `again_seconds`) waits out the rest of that minute and is then made, so a
    turn that keeps failing does not ask in a loop and no replacement lets a session go without asking.
    While an owed replacement waits so, the turn that comes waits for it as for any replacement being made;
  - **the answer:** 200 with the handoff file's path and its byte count where the old session wrote one,
    204 where there was nothing to ask or it wrote nothing; a 409, a handoff already under way, is waited
    on and asked again a few seconds later (`policy/handoff.toml`); the ask waits for the receiver's
    answer up to the policy's ceiling (`wait_seconds`, thirty minutes), long enough for any note a session
    writes, and a stop asked of the waiting turn ends the wait at once and the turn with it, as any stopped
    turn ends. An error answer, a refusal, a broken connection or the ceiling lets the turn go on — the
    fresh session is opened all the same, and the run log says the handoff was not had and why. A
    deployment that names no receiver asks nothing.
- **What the ask carries:** `reason`, an English sentence for the receiver's journal; `room` where the turn
  is one room's; `cause`, why the session goes as the receiver's room line names it — `reset` for a reset
  of it, `documents` for its prompt's documents moved, `road` for its road moved or a fresh session opened
  in place of one on another road, `full` for its window filled, `other` for a session that is gone — and
  `clean` for a clean reset (`docs/session-resets.md`), which has the session consolidate and write no
  note, and whose fresh session opens on nothing and is no handoff lost.
- **Every handover is told to the room.** The room shows one line as a supervisor's session is handed
  over or reset (the receiver's `docs/unified-bmo.md`, 「The room's line」), whichever way it went:
  - the ask above tells it for the one session across rooms, whether or not a program still stood to be
    asked;
  - where a session the agent had is gone and its program no longer stands, so nothing is asked, agency
    tells the receiver all the same: `POST /handoffs/{agentName}` with `ask` false asks no session and only
    writes the line, with the cause `other`;
  - a room's own session, whose handover agency makes itself, is told the same way once its new generation
    opens — with its note, without one, or clean — with `room`, the cause its handover was owed for
    (`full`, `documents`, `road` or `reset`), and `by`, the account that asked a reset where its ask names
    one. A handover its turn wrote no note for and left the session as it was tells nothing.
- **A handoff that was not had is said on the session's state.** Where the ask for the one session across
  rooms failed, or a room's own handoff turn failed or wrote no note, and the reset was not clean, the fresh
  session or generation opens all the same and its state answers `handoffLost` until a turn begun after it has answered (`docs/api.md`,
  「sessions」). The receiver opens that turn on its transcript of what the session answered — the room's
  own answers for a room's session — and, where it keeps none, on the word that memory could not be
  recovered, telling the owner once (the receiver's `docs/unified-bmo.md`, 「Replacing the session」).
- **A room's session opened because the session it had is gone carries nothing:** there is no session
  left to ask. A room's session cleared by the owner's reset is handed over as any replacement is
  (`docs/supervisors.md`, 「Clearing it」).
- **A supervisor's asynchronous calls go on into its next session.** A call a session started with
  `bmo use --async` — a deployment, a wait, a worker ask — runs under the tool host's record of the session
  token its program carried, and the tool host cancels it the moment that record is taken back. Its wake is
  queued for an agent and a room, never for a session, so the session that follows is the one it wakes. So where a
  supervisor's program is replaced or taken down — stood afresh on a new session, or taken down with a session
  that ended — the token it carried is taken back only once no asynchronous call asked under its record still
  runs, as is the token of any resident program a place stands afresh: agency reads the calls running
  (`GET /uses`) and, while one under that record runs, leaves the record standing and looks again every
  `calls_look_seconds` (`policy/handoff.toml`), taking it back after the last has ended. A host that does not
  answer the read is taken to hold none. A record left waiting when agency's own process ends lapses at its
  backstop, a day after it was issued.

## Document reload

- A session is resumed, so what it remembers of a document is the version it read then; sessions
  drift apart when the documents move on without them. A document a session reads with a tool — a
  knowledge chunk, a person document, the lore — is one it has to be told about: every session
  concerned is told quietly to re-read it, as one queued quiet instruction per session — through the
  first of its chats the file concerns — which wakes nobody: it rides the next round the session
  opens, in any of its chats, for something else, and a reload still waiting there is folded into
  the next one for that chat, so a quiet chat holds one notice naming every file changed since the
  last turn (`bmo-window.md`, 「Rows that ride」).
- The notice names the changed files and carries their current text (a very long file is named and
  left for the session to open), saying that the memory it holds of them is stale, that the text
  given is current and outranks that memory, and that nothing needs saying in the room.
- A person-book document is the exception: it is named without its text, and the notice says its
  current version will be attached the next time a turn needs it — the version has changed, so the
  reference attaches it in full exactly once.
- Where several chats share one session, a changed file is announced to that session once rather
  than once per chat; a room's own document is announced only to that room.
- A reload that names the agent whose documents moved and the rooms they concern is told only to that
  agent's sessions in those rooms; the brain names both for its own documents, having worked out which
  rooms each changed document concerns (`docs/api.md`, 「Sessions」).
- The session stays as it is: no restart, no `--resume` into a fresh session, no break in the
  conversation.
- **Common system-prompt documents are reloaded without waking chats.** They mark the shared chat
  prefix as changed. Room-specific instructions, tones and traits are carried in a turn's leading
  context block instead; changes are picked up on the next turn without invalidating the common
  prefix or waking the session. Agency remembers the last successfully applied room context per
  provider session in its store (`room_sessions`). See `unified-bmo.md`.
- **Agency names what moved; the consumer holding the rooms tells them.** Agency's reload verb sorts
  the files it is given by their place:
  - a document that is part of a chat session's system prompt marks every awake conversational
    agent's prompt as moved (`prompt_changed_at`, one `prefix_moves` row per file), and is not
    announced: the next turn composes the prompt afresh, and a session whose composed prompt differs
    from the one it was last sent hands itself over (`docs/supervisors.md`, 「A session in each
    room」);
  - a document a chat turn carries is neither marked nor announced: the next turn carries its text;
  - every other document is handed to the running server (`POST /documents/reload`), which publishes
    `documents-moved` with the paths and whether the notice is due at once (`docs/api.md`);
  - a file under `prompts/rules/` or `prompts/agents/` first has the agents' documents written again
    by the prompt-composing entry (`docs/agent-documents.md`);
  - a file a role's prefix is built from is answered with the advice to let the sessions of that
    prefix go, since a run reads its prefix only when a session opens.
- Which chats a file concerns, the hour a chat is told at most once in, and the gathering of what
  changes within that hour are the consumer's: it holds the rooms, their people and the queued
  instructions. It hears `documents-moved` and tells only chats that already hold a session — a chat
  without one has no memory to go stale.
- Who reloads follows from where the document lives. A brain document is runtime data: the brain
  calls `POST /documents/reload` for its own changes whenever a run succeeds, and work that edits one
  by hand runs `bmo-agency reload --path <file> [--path <file>...]` afterwards; with `--now` the
  notice is due at once rather than after the hour.
- A document that ships with the code — a rulebook under `prompts/rules/`, an agent document under
  `prompts/agents/`, or the platform's harness opening `prompts/harness.md`, which is part of the chat
  session's system prompt and of a role's prefix — is read out of the deploy tree: the next turn of every
  supervisor composes its prompt from the tree as it now stands, and a session whose composed prompt
  differs from the one it was last sent hands itself over (`docs/supervisors.md`, 「A session in each
  room」). The reload verb refuses such a document from a checkout, since the tree a checkout reads is not
  the one the service runs (`deploy.md`, 「What goes live with the documents」).
- Agency holds no lore and keeps no persona of its own. The base persona a chat session's system prompt
  ends with is the base layer of the agent's own brain, the layer named `persona`, rendered by the brain
  and read through its client once at each launch, with its placeholders filled from the deployment's
  values file (`docs/api.md`, 「Calls agency makes」). The brain renders the same bytes until a piece
  changes, so the prompt's prefix holds across launches, and a changed piece is carried by the next
  launch and is no document of agency's to reload.
- Where the brain gives no base persona — the agent has no brain, no brain is named, the brain does not
  answer, its rendering is empty, or a placeholder stays unfilled — the base persona the turn hands in
  as part of its context (`docs/api.md`, 「Sessions」) stands in its place, and the launch's run log says
  in one sentence why the brain's was not used. Neither case fails the launch.

## Launch pacing

- After a start, the first run of each chat session is spaced from the first run of any other chat
  by at least ten seconds, in the order they came (agency's chat line); a chat's later runs
  go straight through.
- Several sessions starting in the same instant is the shape of abuse and can cost the sessions
  themselves, and a restart is exactly when that would happen: the backlog of every chat, the TODO
  instruction and reload notices all fall due together.
- Instructions that wake rooms are held to the same gap, one cold wake at a time, by the instruction
  pass (`bmo-window.md`, 「Quiet instruction」). Everything else the bot starts — gates, runners,
  compactions — comes one at a time by construction and is not paced.
- The upkeep still waits behind the real work: the keepalive check's first launch comes ten minutes
  after start. The going-down line is a fixed message and starts no session.

## Session compaction

- After every chat turn the program's status line reports how many tokens the session's context
  holds and how large the model's context window is. When a reply leaves the session past its fold
  ratio, the supervisor announces in that
  chat (and only there, when that chat's rest notices are on) that it is going to sleep for a
  moment, types `/compact` into its resident program, and announces that it woke up.
- **The fold ratio is the compaction threshold of the session's supervisor or role,** a field of its row
  changed on the console (`docs/supervisors.md`, `docs/agent-roles.md`), never a constant of a policy file:
  - a session's is its row's threshold, read afresh as the session's state is read (`foldRatio`), so a
    change reaches the next turn of every session of that row;
  - a row is given its threshold as it is added: the one it names, else the default its first provider
    declares — 0.5 for DeepSeek and OpenRouter — and 0.85 where that provider declares none;
  - **a session's state carries the context its last turn left,** `context` with `carried` and `window`,
    read off that turn's answer and held while agency's process stands, and let go as the session folds
    or ends; a fold that did not happen leaves it as it stood; a session no turn has ended on since then
    carries none, and the next turn's end fills it;
  - **a session whose agent's resident program carries its own turns** — dsh's, one session across the
    agent's rooms, read alike by the agent's own session and each of its rooms' — carries instead what the program's record says of its last answer: `carried` the
    `used` that answer's context names, or where it names none, as dsh writes it only on a turn of
    several requests, the input, cache-read and output tokens of the answer's one request; `window` the
    `size` the record's latest context names. The held context stands only where the record holds no
    answer or names no size;
  - the receiver compares the context ratio a turn ends on — the state's `context`, carried against
    window — with the session's `foldRatio`, and the harness compacts after the turn that crosses it; a
    provider that hands its session over rather than folding it is judged by agency itself at the turn's
    end against the same share;
  - **a chat session on a resident program that carries its own turns folds itself.** dsh takes no
    command through the inbox its turns arrive by, every line put there reaching the model, so the fold is
    its own compaction: the overlay its program stands on names the session's fold ratio as dsh's
    compaction threshold (its `compaction-basic` row's `thresholdRatio`), and dsh compacts inside the turn
    that would carry its context past that share. The session's state says so, `foldsItself` true, and a
    consumer makes no fold call for it; a change of the fold ratio changes the program's command, which
    stands the program again on the same session. Every other session's `foldsItself` is false, and the
    fold route refuses a supervisor's session across rooms whose `foldsItself` is true;
  - the session's state names the supervisor or the role it runs for, so a consumer whose own settings
    are kept per supervisor, the receiver's draft questions among them, reads the same row the fold ratio
    was read from.
- Both lines are the session's own words: the full session is asked for its going-to-nap line and
  the compacted one for its waking line, each by a quiet instruction, and the fixed lines, which agency
  names by status code and the caller's language pack words, are only the fallback when the session cannot be asked. Each line is a full
  run of that session, so a chat that would not be shown it — its rest notices off — is not asked
  for it either. A `/compact` the CLI declines to run is a failed compaction, not a silent success.
- **A fold that fails is said out loud.** The room that was just told the supervisor was going for a nap is
  told the nap did not happen, in the fixed line named by its status code, and the owner is told
  which room and what stopped it — a fold nothing can complete leaves the context exactly as full as
  it was, so the next reply asks for another one, and a console log is not where anyone would find
  out. Both are said once per cause: while the same failure stands, the attempts go on in silence
  and the going-to-nap line is not said again, and a different failure says its own piece. A fold
  that works again in that silence comes back in it, since a room told nothing about a nap is not
  told it woke from one.
- Nothing is launched beside a compaction: what the session has read and not yet kept it writes into
  its brain itself, as the brain's discipline asks (`docs/agent-ownership.md`, 「Who writes long-term
  memory」).
- The session id is unchanged and the compacted session keeps its memory of the chat. A fold has no
  clock: it lasts until the program's record says how it came out (`docs/agent-control.md`, 「The order
  for putting in one turn and waiting for it to end」).
- **A supervisor's session across rooms folds in the program standing on it.** The fold word goes into
  that program on the command it stands on, with no room's block, no lead and no road walked, so nothing
  of it restarts the program or moves the session. The fold route answers once the word has come out:
  success only where the record holds the compacted marker, and a fault with the program's own words
  otherwise. A session whose program stands on another session than the one its supervisor keeps, or
  whose supervisor has no program standing, is a fault too, and nothing is typed.
- Compaction runs detached after the reply has been sent, never in the path a chat is waiting on; a
  message that arrives mid-compaction is answered as soon as the compaction finishes. A compaction
  failure leaves the session running as it was.
- **A role's session compacts against its model's own window,** as every other session does:
  - inside a turn, its Claude Code is launched with no compaction setting of the deployment's, so the
    program compacts at its own default share of the model's window while the job goes on;
    compacting a large session early costs more than reading its cached context again;
  - between turns, a run folds the session it keeps once its turn has left the session at or past its
    agent's compaction threshold, the agent row's `foldRatio`, or the ratio its provider declares where the
    agent sets none, read afresh as the run ends — the rule a
    supervisor's session is folded by. The fold is one more turn into the same session on the same command,
    the program's fold word, made by the run before it gives its slot back, so it runs inside the run's slot
    and holds the kept session while it goes; a fold that went through leaves the session kept and valid,
    its context unread until the next run answers on it, and one that did not leaves it as it stood. A run
    of a `fresh` role keeps nothing and folds nothing.
  - A run on a registered model's road is the one exception: it is launched with the share
    its registration leaves for the answer (`docs/own-llm.md`, 「A turn at the input limit」).
  - Whatever a consumer sent the session to hold — a co-work result among it — is folded away with
    the detail, so `session-folded` is published once the fold has reached the session, or may have
    (`docs/api.md`, 「Events」); a fold that never reached it, held off by a busy pane or a session too
    short to fold, publishes nothing.

## A turn whose program dies

- **A turn whose program is gone is closed by the queue's pass.** Every kept turn is read in the place
  whose program carries it — the agent's, or the place of the thrown-away session it went on — and only
  where that place's run is the turn's own; a place whose run is another's, an earlier turn of the agent's
  own place among them, says nothing of it. A turn this process still carries is its own to close and is
  left to it: a turn's walk goes from one Claude account to the next under the same run, and the place's run
  ends between them while the turn goes on, its run in the ledger closed by the turn's own end. Any other
  turn whose run there has ended is no longer kept. Otherwise
  it is read against the execution rows its run put in; where a row's recorded pid and birth are gone and
  the run has no end, the pass closes the run with the moment and a reason naming the room, run, pid and
  that it died silent, and hands its end back as a finished turn does. The room's one turn is freed, so the next
  instruction opens at once and the words the dead turn did not answer come to that next turn.
- **No replacement session is started where a pane is kept:** the pane is left as it stands, and the
  next turn opens the session it needs.
- **A turn still being watched reads the same death itself.** Its wait asks on every look whether the
  process its liveness row names is alive, and ends `program-lost` once it is not and its record has not
  closed the turn (`docs/agent-control.md`, 「Telling that a turn's program is gone」); the queue's pass
  closes what no live wait is left to close.

## A role's session

- **A session is the agent's, kept by the agent and the path together,** with the role, the provider and the
  model the agent's run stands on beside them. An agent is one worker, not a kind of seat
  (`docs/role-agents.md`, 「What an agent is」): a run of a `keep` agent (`docs/agent-roles.md`, 「A role's
  sessions」) resumes the session that same agent's last answered run left at the same path, while it is
  valid, and opens a new one otherwise, so the agent takes up its own work where it left it there. Two agents
  never share a session, whatever role and road they hold in common; a run whose agent cannot be told keeps
  no session (`docs/agent-roles.md`, 「A role's sessions」).
- **The path is the folder the run works in:** a repository's checkout for a job, a support worktree for a
  job there, the task's folder for a worker ask, a place for a co-work branch, the line's own
  directory for a search. A session is kept per folder by the program itself, so a folder that moved is a
  new key.
- **The provider and the model are the ones the run went out on,** with the effort and the login, and every
  word of its command but its session's and its advisor's, the system prompt's text standing for its file: a
  run on another effort, login, tool list or system prompt is another command and opens its own session, so a
  resumed session always stands on the road and the prefix it was opened on. The advisor stands outside the
  prefix the model's cache keeps, so a run with another advisor, or none, resumes the same session
  (`docs/provider-order.md`, 「The advisor」); the prefix digest a job's change is read by leaves it out too.
- **A session is valid while all of these hold:**
  - its cache is warm: the time since its last answer is under the cache lifetime the provider declares for
    the model (the criterion below); a provider that declares none leaves it nothing to outlive, so it stays
    valid however long ago it answered;
  - its last answered turn stayed under the model's context limit;
  - its record still stands: no reset, lost session or hand-over has taken it away, and the session's own
    record file is still there.
  A session failing any of them is dropped with its record, and the run opens a new one, carrying the note
  the replacement rules already leave (「Session replacement」 below).
- **One run holds a kept session at a time.** A run takes the record as it starts and gives it back, with
  the session it answered on and the moment of that answer, once it has answered. A run of the same key
  starting meanwhile finds no record and opens a new session, so runs at once never share one, and whichever
  answers last leaves the record the next run takes. A run that ends with no answer gives nothing back.
- **A resumed run is handed nothing beyond its own words.** What it reads of the earlier runs is what its
  session already holds; in a co-work, the stage results its prompt opens.
- **A `fresh` role keeps no record:** each of its runs opens a session no later run resumes.
- **A role's session may continue a session its caller names,** the start's `continues` (`docs/api.md`,
  「sessions」): the program's own session an earlier turn answered on, which the caller read off that turn's
  end and holds itself. Its turns resume it while its record stands under the login the turn goes out on,
  whatever the agent's `sessions` setting and the cache's age, and otherwise open a new session on the words
  the start's `continuesElse` gives, which carry what the named session held. Agency keeps no record of such a
  session: the caller's naming is the whole of its continuity, so one agent may carry many such sessions, each
  continued by whoever named it.
- **No pane stands between runs.** A run is spawned when its work comes and closed when it ends; the session
  it leaves is the program's record on disk, which the next run of the same key resumes by its id.
- **Nothing keeps a role's session warm.** No ping goes into a role's session between runs
  (`docs/keepalive-schedule.md`); a session whose cache has gone is simply not resumed.
- **A job's runner may hand the session over as the job ends,** naming it across the boundary (`docs/api.md`,
  「assignments and jobs」): a session handed over is asked for its handoff first (「The handoff a job's session
  leaves」 below), then closed with its record, and the next run at that path opens a new one. A session not handed over stays valid for the next run of its key while its cache is
  warm.
- **Which session a job's turn goes on is its runner's to ask.** The turn names whether it resumes the kept
  session, opens a fresh one or continues one it names; agency carries that choice through the rules above.
  A turn whose runner named a session that a change of its road, a reset or the judging of the kept
  session has moved off is put to the runner again before anything goes in, since its prompt was composed
  for what that session has read.
- **A turn a job's review asks for continues the job's own session** (`docs/job-review.md`, 「The turns the
  review asks for」): the session the job's last turn answered on, named by the job and resumed whatever its
  agent's `sessions` setting and its cache's age, while its record stands and the road and the prefix are the
  ones it was opened on.
- **What a job's change moved of the prefix is read by the files a session's prefix is built from:** the
  instructions and the tool declaration at a checkout's head, the platform's harness opening, the role's
  prompt and its provider prompts, the worker line's settings, standing rules, report trailer, plugin, the self-preservation rulebook and the platform's `environment.md` and `preservation.md`, and the repository's own rules and prefix
  documents. A runner asks which of its own commits' files those are, to say why it hands its session over.
- **A session's generation and its context are kept for its state to read:** how many times a key's session
  was opened fresh rather than resumed, and on the record the tokens its last answered turn carried and the
  share of the window they filled.

### The warm/cold criterion

- **Warm while the time since the session's last answer is under the cache lifetime, cold from the lifetime
  on.**
- **The lifetime is the one the provider declares for the model,** each provider answering for its own
  models out of its policy file: Claude Code's is 60 minutes for every model, the subscription's one-hour
  lifetime, and Codex and OpenRouter name theirs per model and per answering upstream; agency keeps no cache
  measurement of its own. A provider that declares none, a registered model on this machine among them,
  leaves a kept session nothing to outlive: the next run of its key resumes it however long the wait before
  it, a job's wait for its road included.
- **The judgement itself,** the lifetime in milliseconds, the warm/cold line and a record's own life, is the
  `cache-siren` crate's, which is handed the declared lifetime and knows no provider.
- **A kept session's record goes cold with its cache.** A record whose last answer is past the cache lifetime
  its provider declares, where it declares one, is retired as it is read;
  `BMO_WORKER_SESSION_TTL_MINUTES`, where the deployment sets it, retires a record sooner, never later.

### The session history of a role

- **Every session a role's runs opened is listed under the role,** newest first: its path, its provider and
  model, when it opened and last answered, how many runs it carried, its context and whether it is still
  valid (`docs/api.md`, 「roles」). The console's role tab shows it in place of a list of every agent's
  sessions.

### The handoff a job's session leaves

- **A job's work that moves to another session takes the old session's handoff with it:** what is done, what
  it found, what it was doing, what is left and where, in the session's own words. The handoff is written
  onto the job's item in the job service as a note of its own kind, labelled `kind` `handoff` with the job's
  id and where its words came from (`docs/api.md`, 「assignments and jobs」), and the next session opens on it.
- **Where the work moves, and how the handoff is had:**
  - **a turn of the job's review goes out on another road or another prefix** — its agent's provider, model
    or effort changed, the road the job's session stands on cannot answer it, or the prefix the session was
    opened on moved: the role's prompt, its bundles, its skills, its tools and MCP servers, its thinking or
    any other word of its command, read off the command key its last turn went out under, which the turn's
    line puts back to the runner before anything goes in — or the session's record is gone, as it is under a
    login whose records home does not hold it: the session the job's last turn answered on is asked one more
    turn on its own road, at the effort it answered on, resumed by its id and never in its place a session of
    another run, with the handoff instruction (`policy/job-handoff.toml`), and the handoff is noted before
    the turn goes on; the turn of the review then opens a fresh session with the handoff ahead of what it
    carries;
  - **the job's road stops answering in the middle of a turn** and the turn goes out again on the next road
    (`docs/cache-fallback.md`): a road that stopped answering cannot be asked, so the handoff is written from
    the last lines of the job's run log, and the turn on the next road opens with it ahead of its words;
  - **the job is stopped:** the stop words its turn reads ask for the handoff as the turn ends, and the turn's
    answer to them is the handoff; a turn that ends with no answer, and a job stopped while no turn carries
    it, leave the handoff written from the last lines of its run log;
  - **the job's session is handed over as the job ends:** the session is asked one more turn for its handoff
    before it is closed, as a review's moved session is.
- **A session that cannot be asked leaves its run log instead:** an ask whose session's record is gone, whose
  program does not start or dies, which answers nothing or the word `none`, or whose answer comes from a session
  other than the one asked leaves a handoff written from the last lines the job's run logged, said to be that.
  The ask goes out under the job's run, so a stop of the job ends it as it ends any turn of the job.
- **A launch of the job's item opens on its handoff,** on whatever agent or model it goes out: the newest
  handoff note on the item stands under the item's title, after the preamble bundle and the checkouts lent to
  the job where the prompt carries them (`docs/worker-bench.md`, 「Preamble」), ahead of the rest of the job's
  prompt, under a heading that says it is the last session's own handoff, unless a job of the item ended with its work answered after that note was written.
  A job's turn that goes out again after agency's restart carries the words it was composed with, the
  handoff among them.

## Session replacement

- Folding a session is what happens when its window fills. Replacing it is what happens when the next
  turn should not go on in the session the agent holds: the documents its prompt is composed from
  moved, its road moved (another provider, model or effort, another login, or a place its registration
  no longer has; another advisor is no move, its program being started again on the same session with it),
  the kept harness session can no longer be resumed, the owner asked for a fresh
  start, or its provider's cache lifetime passed. A full session on a provider that hands a full
  session over by a note is replaced at its compaction mark (「Session compaction」,
  `docs/supervisors.md`, 「Its limit」).
- **A replacement never stands between a question and its answer.** Agency compares the SHA-256 of
  the system prompt it composes with the hash the session keeps from its last turn — a room's session
  row, or the supervisor's row where one session serves every room — at the head of a turn and on
  its own, outside any turn. A session whose prompt moved owes a replacement; one whose prompt did not
  is resumed; one whose prompt was never recorded owes one once, since what it read cannot be known.
  - **A turn that finds the prompt moved goes on in the session the agent holds,** sent the prompt as
    it now is, and writes the replacement down as owed: the person asking waits for no handoff. The
    session's one cache miss on that turn is the same read a fresh session would make.
  - **An owed replacement is made while the agent is quiet,** never under a turn: the moment the turn
    that found it ends and no other turn of the agent runs, when a reload of its prompt's documents is
    told, when the server starts after a deployment, and on the keepalive clock as a catch-all. The old
    session is asked for its handoff then, let go, and the next turn opens a fresh session on the
    note, as below. A turn that arrives while the replacement is being made waits for it and then
    opens on the fresh session.
  - **Which sessions owe one is read off the documents, not waited for:** agency composes the agent's
    system prompt on its own, so a deployment that moved a document is found at the server's start
    and settled before anyone asks.
  - **What is owed is written down, never held in memory alone,** since the session already records the
    new prompt's hash and a restart would otherwise forget the debt. The one session across rooms owes
    its replacement under the agent (`handoff_owed:<agent>`); a room's own session owes its handover under
    the agent and the room (`room_handoff_owed:<agent>:<room>`, `docs/store.md`), with why — its
    documents, its road or a reset — and what its handoff turn goes out on: the supervisor, the provider,
    model and system prompt the session was last sent, the generation's settings, the session and its
    place. A room's handover is made at once where the room carries no turn, once the turn running there
    ends, at the head of the room's next turn, and otherwise at the server's start and on the keepalive
    clock, one picked up across a restart included; one whose room no longer holds the session it was
    owed on is let go, since that session was replaced already.
- **A session going away is asked for its handoff before it is let go.** No summary of a session's
  conversation is made or kept for its successor: the successor opens on the note the old session
  wrote as it closed. A room's own session whose prompt moved, whose window filled, whose road moved or
  whose reset was asked writes that note in one more turn agency asks of it before the new generation
  opens. The one session across rooms is
  asked through the receiver's `POST /handoffs/{agentName}`, while agency still names the session and
  the standing program can answer; the receiver asks that session for its file, and the next session's
  first turn opens on the receiver's note (`docs/supervisors.md`, 「Its limit」; the receiver's
  `docs/unified-bmo.md`, 「Replacing the session」). A room's session opened because the session it had is
  gone carries nothing: there is no session left to ask.
- Where the old session wrote nothing, or its handoff could not be asked, the next session opens on the
  receiver's transcript of what the session answered, or, where neither stands, on a word that memory
  could not be recovered, and the owner is told. That fallback is the receiver's, which agency's
  `handoffLost` on the session's state tells of where agency made the replacement.
- The session id is cleared and a new session opens on the next run; the room hears nothing.
- Who the people are reaches the new session by the brain's index, whose people group its first turn
  walks, and the person books, as on every turn.
- A session is never replaced under a turn that is already going, and never ahead of one that is
  asked: a replacement found at a turn's head is owed and made once the turn has ended.

