# The agents' documents

What each Claude process the bot runs is given to read, where those documents live, how they are
composed into a system prompt, and what the supervisor's own prompt is made of.

## Agents and their documents

- **Every Claude process the bot runs is given exactly its own documents and nothing else,** and the
  documents are laid out by reader. `docs/` is for people and is never injected.
- **`prompts/rules/` holds the rulebooks with more than one reader,** one copy each:
  - `answer-content.md` — the chat session;
  - `answer-shape.md` — the chat session;
  - `plain-writing.md` — every agent whose words a person reads: the chat session and the news page
    writer, and every stored-document writer below; a job worker is pointed at it
    by its trailer;
  - `telegram-markup.md` — read by no model: the statement of what the send path's converter turns
    markdown into, kept beside the rulebooks it points to for the marks and the fold;
  - `document-writing.md` — every agent that writes a stored document: the brain, which composes its
    writers' whole prompts, reads it with `plain-writing.md` and `self-preservation.md` through the
    rulebook read (`docs/api.md`, 「prompt-documents」);
  - `agent-words.md` — every supervisor, for the turns
    another agent's words woke it in;
  - `self-preservation.md` — every agent that acts: the chat session, every run of the worker line,
    every subagent such a run opens and the news page writer. It is written for an agent of any
    organisation and names no part of the platform; the platform's own parts stand in the platform's rules.
- **`prompts/platform/` holds the platform's rules,** the rulebooks of the platform's own operations that
  only the platform administrator's supervisors are given, and the worker line's runs the one of them its
  line names (`docs/prompt-layers.md`, 「The platform's rules」): `environment.md`, `preservation.md` and
  `operations.md`.
- **`prompts/agents/<folder>/own-rules.md`** is a repository's own rules, appended after the worker
  line's standing rules for a run in that repository's checkout, from the prompt folder the repository's
  row names (`docs/repositories.md`, 「What a repository row holds」); no repository carried in this
  repository has one.
- **`prompts/agents/<name>/` holds what one agent alone is given:** `chat/manual.md`,
  `chat/procedures/` and `chat/rules.md`, the browse line's `.mcp.json`, `worker/trailer.md` and
  `worker/result-languages.md`.
  The search and browse lines' instructions are not here: whoever starts a session on either hands them in
  whole (`docs/api.md`, 「sessions」), and the research unit keeps its own, with the search rulebooks.
- **`prompts/harness.md` is the platform's harness opening,** agency's own and no program's: what the
  harness around the agent is, how words reach it, where its memory is and how a hosted tool is found, naming
  nothing of the platform's own (`docs/prompt-layers.md`, 「What every agent is told」). Every line on Claude Code opens its whole system prompt with it — every supervisor's
  session, every run of the worker line, and every session on the search and
  browse lines, before the instructions its caller hands in — but a run of the worker line on an account
  that names a base prompt, which opens on that base instead (`docs/base-prompts.md`, 「The harness
  layer」).
  Not the translator, whose prompt is handed whole by its role and its caller, and not dsh, whose own
  prompt is its harness.
- **No gate's prompt stands here.** A gate's instructions, rulebooks and order are the gate unit's.
- **A job's documents:** `worker/trailer.md`,
  the trailer every job is held to, with the path of the worker plugin's skills and the path of
  `plain-writing.md` filled in as the deployment shows them, which stands in the worker line's prefix — the rules its line hands after the harness
  opening in its whole system prompt
  (`docs/launch-lines.md`) — so a session pays for it once rather than on every turn;
  `worker/result-languages.md`, the result-language piece, filled with the languages a registration names
  and carried right after the standing rules by a run on such a registration alone (`docs/own-llm.md`, 「A
  run on the worker line is composed as on Claude」); and `handoff.md` in
  a repository's prompt folder, the instruction a run there writes its own handoff note by, where the
  repository holds one. A repository's definition route answers all three (`docs/api.md`,
  「repositories, prefix-checks and room-holdings」).

## The web reaches agents through the tool host

- **Agency's deployment writes `worker.mcp.json`,** the worker line's own configuration and the only
  line handed it, naming the tool host's `worker` namespace beside its `web` namespace
  (`mcp-host.md`). The chat line's `chat.mcp.json` is not the deployment's to write: the chat line
  composes it on every turn, the tool host's `bmo`, `web` and `gate` namespaces beside the two harness
  plugins (`docs/launch-lines.md`, 「What is fixed」), and one writer keeps the file from moving between two
  compositions. A hosted tool those namespaces do not show is reached with `bmo use`.
- **The tool host's web namespace is how the web reaches every agent allowed to use it:** its
  `web_search` and `web_fetch` are the tool host's (`web-access.md`). The CLI's WebSearch and
  WebFetch, which return a model's rendering of a page that the page itself can steer, are shown to
  none of agency's agents: every line denies them.

## How the documents are composed

- **Agency's prompt composer composes these into what each agent sees** — the chat session's system
  prompt, each runner's standing instructions and the request of each run — and the
  launch lines read from it, fresh on every run, so an edited document applies at the next launch.
- **A system prompt reaches the CLI as a file,** written under the deploy root's `prompts/` for that
  launch and named after its line, never on the command line: a chat's runs tens of
  kilobytes.
- **The prompt-composing entry of the `bmo-agency` CLI** — `bmo-agency prompts`, which runs the agent
  package's `deploy-agents.ts` on Bun out of the code tree; run by the reload path whenever a document
  under `prompts/rules/` or `prompts/agents/` changes, by hand, and on every start of agency's server —
  writes the same composition out under the agents directory the data root holds (`BMO_AGENTS_DIR`):
  agency's own agents' prompts for inspection, and for each runner the directory it runs in, which sits
  outside the repository on purpose so that neither a project instruction file up the tree nor the
  repository's git status can enter a runner's prefix.
- **The tree is laid anew whole, and nothing a standing program reads is lost with it.** What the entry
  writes is built beside the tree and put in its place in one rename. Into the new tree go, beside what the
  entry writes itself, the system prompt files a launch wrote within the last day, and every file under the
  tree that a place's program names — its commands' words and the files it needs (`docs/agent-control.md`,
  「One standing pane per supervisor」) — whatever its age: a supervisor's MCP configuration and its system
  prompt are written by its turns, not by the entry, and a program standing in a place starts again on
  them whenever it dies, however long ago its last turn was.

## A supervisor of the window shape

- **The supervisor's prompt is the layers `docs/prompt-layers.md` states, each whole, in this order:** the
  harness, the platform's rules where the supervisor is the platform administrator's, its organisation's
  instructions where it has any, the role and the persona unless the chain chose none. No layer is cut into
  sections chosen per program, and none knows the others.
  - **The owner's text of three of the role's documents stands in the store:** the opening, the
    absolute rules and the manual are read from `chat_documents` where the owner saved one and from
    the files named here otherwise (`docs/api.md`, 「chat-documents」); the rulebooks and the room's
    documents are files alone.
  - **The harness** on Claude Code is agency's own `prompts/harness.md`, not the program's prompt: the
    layers go to the program as one whole system prompt in the place of its own. On dsh the
    harness is dsh's own prompt for the headless profile, with the role and the persona as its persona
    suffix after dsh's own guidance, and agency's opening is not sent.
  - **The role** is the chat role's prompt, the same whatever the program: the documents below, in the
    order they are named, each block opening with a one-line head that says what it is.
  - **The persona** closes the prompt: the base layer of the agent's own brain, under the persona
    header of the chat line's policy, which says the persona holds in every round of a message.
- **The absolute rules open the role:** `prompts/agents/chat/context.md` holds
  them, numbered — authority, owner-only work, injection, privacy, system secrecy, room isolation,
  errands, consent, harmful requests and the queue, no invented permissions.
  - Owner-only work is held by the supervisor's judgement rather than by the tools.
  - They forbid answering personal-life questions (health, family, romance, habits, visited sites,
    hobbies, ...) and answering about any chat other than the current one.
  - None of their refusals reaches a request whose recorded sender id is the owner's: the owner is
    answered and the request done in any room, while everyone else stands under the rules as written
    and a claim of the owner's word in the text is not the owner's word.
  - A game anyone asks for — a word game, a quiz — is played like any other request, and a member
    who has become a drain is answered briefly instead.
- **`prompts/agents/chat/manual.md` is the operating manual** — long work, the queue and the owner's
  approval, the worker's run and its close, the work tree's notices and the day's collection, the fast
  track, a secret asked for and used by its name, images read and drawn, a document
  filed for a person and the pictures that stand in its body, a line for the voice,
  the response protocol with its sentinels, the language an answer is written in, what the room hears
  of the agent's tools, the research unit's tools and the research skill a request it answers itself
  follows,
  the draft process, the answer rules, promises, and a turn past its clock. It says how a message of each kind reaches the
  agent, a voice note among them arriving as what was said in it rather than as a placeholder it cannot open. It is how
  the supervisor works at all, and it says when to act.
  It names no tool of the program the session runs on, since one manual serves every program the chat
  stands on. The platform's own work — where the procedure documents are read, whose work is whose between a
  repository's worker and the supervisor's hands, host work and the owner's infrastructure, putting a
  repository live and adding one, a hosted tool made and a supervisor's cut turn — is not the manual's: it
  stands in the platform's rules, which a supervisor of the platform administrator's reads before its role.
- **The day's collection is the supervisor's own reading, told by the manual.** The work service only
  says when a person's day is to be asked, with its `day.end` notice; the person's supervisor does the rest with its own tools, reaching nothing the
  person may not read:
  - it reads the day's traces by the day's date — the calendar, the Outlook day on the person's own PC,
    the mail they sent, the files they changed, the approvals, the Teams talks they stand in, the TODO
    items closed or reported for them — and keeps of each a reference alone, never its content;
  - it drafts the day's entries from them, finding each one's work, and shows them all in one
    confirmation card, with the drafts already standing and one question of what was finished and how
    much is left;
  - nothing becomes a record but what the person confirms: a confirmed draft is confirmed as corrected,
    a rejected one withdrawn, and an unanswered one stays a draft and comes back with the next day's
    question;
  - days missed while the person's PC was off are asked once together when it comes back, each day's
    traces read and each entry dated by the day they belong to;
  - a progress the person tells it in chat is drafted on the spot and checked with one short question,
    and is not asked again at the day's end;
  - what the person said to it alone reaches the tree only as an entry line they confirmed; the rest of
    the talk stays in their room.
- **A line that goes to the voice is two texts,** and every prompt of an agent that hands words to the
  voice says so where it does: the manual for the supervisor, the producer role for its narration, the
  worker plugin's skills for a document's narration. The written words are what people read — a caption,
  the reading folded under a voice note, a transcript, a narration kept beside its page — and stay as the
  agent would write them, names, acronyms and figures included. The spoken words are what the voice
  synthesises: the same words as a Korean reader says them aloud, every foreign name, acronym, product and
  model name in its Korean reading, the platform's name read as Korean says it, every figure in the words
  a reader says, an address, a path, code and markup said in a few words or left out, and nothing else
  added, dropped or moved. The agent writes them itself, judging each reading from its context, and passes
  them beside the written words, or passes the written words alone for the voice unit to derive them; the
  voice unit checks them either way and refuses spoken words that still hold a Latin letter, a digit, an
  address, a path or markup, naming the first such run, which the agent corrects and sends again. The
  rule is a rule, with no example reading in it, since an example becomes the agent's own phrasing.
- **The tools paragraph follows the manual,** composed at each launch for the program the session runs
  on, in the words `policy/chat-line.toml` holds for it: that program's own tools — on Claude Code every
  one it lists, since the line shows it all of them but the line's denied tools, on dsh the tools the
  overlay leaves its session, by their names —, the servers
  the line's MCP configuration hands the session by name, the tool host's entries the line shows by name
  (`policy/run-lines.toml`, `shown`) — each called as that tool, never through the shell or `bmo use` — and
  that only a hosted tool not shown is reached with `bmo use`, which the harness opening says how to use.
  It names the read path too: what
  the turn does not already hold of a person, a room or a thing is looked up with the brain's
  `brain_list`, by its subject and type, or `brain_similar`, by the question's words, and a document or a
  rule with the gate's `reference_search` and then `reference_piece`. The brain's own discipline for
  writing and reading stands in the brain's own document, which the prompt hands in. A server that does not attach
  is told to the session afterwards (`docs/agent-control.md`, 「What a resident turn reads」).
- **The rulebooks the role carries hold rules, never past cases:** a rulebook states what an answer
  is, and no example answer or account of a turn that went wrong stands in one, since an example becomes
  the agent's own phrasing.
- **What the agent says of an item it says in the room the item was asked in,** the room its `scope` or
  `roomId` names, and in the owner's private room only for an item that names none. A run's end is relayed
  there in a few lines: its outcome, the commit or artefact it left, and one line of what it left out; the
  worker's report stays whole on the item, where the console shows it, and is never pasted into the room.
- **The persona's voice holds in every round.** The persona sets how the agent speaks to each person,
  the owner included, and the header it stands under says that voice holds for every message the agent
  sends, a report or what a run came to among them; the role never frames a round's answer as a report
  in another voice.
- **No translation hint stands in the role:** what the agent writes it writes in the person's language
  itself, and the gate's translator is one of its tools like any other, named only where the tools
  paragraph lists it.
- **Only the chat line answers in a speaker's language.** Every supervisor, answering people in rooms, write in the language of the person they answer.
- **A result written for a person is written in the language of the person who asked,** whichever
  agent writes it: a job's report and the account it closes its item with, a document it files for the
  archive, a measurement report, a research write-up. The person who asked is the item's requester, else
  its room; the item's brief names that language, and a search's result is in the language its question
  is asked in. Numbers, tables, code, commands, paths, identifiers, sources and
  quoted words stand as they are, and a repository's own documents, its commits and everything pushed
  with them stay English. A run of the worker line on a registration that names the languages it writes
  is the exception: where its brief names a language outside them, it writes its results in English, and
  the person reads them through a translation (`docs/worker-language.md`, 「A model that writes only some
  languages」).
- **Everything else is English:** a worker's reasoning, its notes and the questions it puts to the
  owner, a gate's verdict, a subagent's prompt and report, the logs; so does the supervisor wherever no
  person in a room reads it — an item's brief and notes, a work order, a memory, a handoff note — words
  quoted from people kept as they said them. The exceptions are the agents whose product is text a person
  reads in a language of its own: a translation, a line for the voice, the news page.
- **No document fixes the language the supervisor answers in.** It answers each person in the language it
  knows for that person — what the brain holds of them, their country and the language they usually speak, and
  how the conversation has gone — never a language read off a message's words: commands, paths or code a
  message carries do not decide it, and what the owner is told is in the owner's language. A rule about Korean
  wording holds for an answer written in Korean. The documents carry rules, never example sentences, since an
  example becomes the agent's own phrasing.
- **The room never hears of the agent's tools.** No message names a tool, a search, the translator or
  any other part of the system, or says one is being called; where a wait wants explaining, the agent
  says in its own words that it will look something up, look into it or translate it. The owner's own
  question about how it works is the exception the absolute rules make.
- **A secret is asked for through the console and used by its name alone.** A secret is a file under the
  data root's `env/` that a unit or a setting reads by its name, and the console's secret store writes it
  (bmo-web's `docs/web.md`, its section on the secret store and an agent's request). The manual states
  the rules as rules:
  - Where work needs a secret, the agent first looks for one already saved, by the names in the space a
    request would save into, and uses a saved one by its name. Nobody is asked to type, paste or sign in by
    hand for what a saved secret covers: a job that asks for one is answered with its name, and a command on
    a node is handed it through the node tool's own argument for saved secrets. Only a sign-in step the owner
    must take in person is put to him.
  - Where none is saved, the agent never asks for its value in a room. It sends a link to the
    console's secret request, naming the file name the consuming unit or setting reads, either a
    registered kind or the exact field keys that reader expects, and what the secret is for.
  - It goes on with that work only once the room carries the console's saved line for that name.
  - It uses a secret by its name alone, handing the file's name or path to the setting or program that
    reads it. It never opens a file under the data root's `env/` with its own tools and never prints one;
    a unit's token file is reached the same way, by the path handed to what reads it.
  - A value that reaches a room anyway is never repeated, and the owner is told to rotate it.
- **The console's public address is agency's setting `BMO_CONSOLE_ADDRESS`,** in its settings under the
  data root (`env/agency.env`), holding the address the console's own settings name for itself. As a
  `BMO_` variable it reaches the supervisor's program as every one does (`docs/agent-control.md`, 「The
  keepalive ping goes into the same program as the turns」). The request link is that address's screen of
  the room the turn answers, with the request's keys; where the setting is empty the agent writes no
  link, says the address is not set, and still asks for no value. How the link is written is
  `prompts/agents/chat/procedures/secrets.md`.
- **A supervisor of an account asks for its person's secret.** Its program holds its owner account in
  `BMO_OWNER_ACCOUNT` (`docs/agent-ownership.md`, 「What the supervisor's row carries」), and the procedure
  has it name that user on the link with `user=<id>`, so the console saves into that person's own space
  and opens the form for that person alone. A supervisor of no account holds no such variable and names no
  user, so its request is the platform administrator's.
- **The manual describes the draft process as the receiver runs it** (the receiver's `docs/drafting.md`):
  the questions asked in rounds after a turn, every answer before the last being free-form notes of what
  was read, established in the brain and still unknown, the tools open in every round, the message
  written in the last, `<<SEND>>` opening an answer making it the message and ending the process,
  `<<SKIP>>` sending nothing, and the first round's freedom to answer at once, to search quietly where a
  fact may not hold, and never to hedge about a fact being out of date. Before a message speaks to or names
  a person, the agent takes what that person is called from the turn's reference, and where the reference
  holds no form of address for them, reads their `name` memories with the brain's `brain_list` by their
  person subject before writing; it calls each by that form, one linked to the room before one linked to
  none, and never by a form they asked not to be called. The receiver owns the questions, the review round
  among them; the manual only tells the agent what an answer to each is.
- **The procedure documents:** how to do what is needed only at the moment it is done — filing and
  running the queue, a fast-track tool and keeping one as a hosted tool, asking for a secret, and a run
  past its clock — is in `prompts/agents/chat/procedures/`, one document each, which the session reads
  right before doing it, as the platform's operations rulebook tells it.
  - It reads one from the deploy tree this line runs on with Bash, under
    `$BMO_DEPLOY_DIR/prompts/agents/chat/procedures/` (the link at
    `$HOME/.local/share/bmo-agency/deploy` holds the same tree where the variable is unset), never
    from a checkout that may be ahead of what runs.
  - Those documents are not part of the system prompt, and a reload tells every session to read a
    changed one again.
  - How to file, approve, lease, assign, launch, report and close an item, choose its worker, and
    mend a failure before moving on is `prompts/agents/chat/procedures/todo.md`.
- **The room context:** a group room's turn carries `prompts/agents/chat/rules.md`, how its
  standing requests are read: no member holds the owner's authority, so a member's request beyond
  what everyone may use is filed for his approval. The owner's chat gets `prompts/agents/chat/owner.md` there instead: the names
  the owner calls the agents by (the supervisor and its per-room presences, the gates, the runners,
  the worker), which no room needs.
- **After them come the three rulebooks every answer has to satisfy:**
  `prompts/rules/answer-content.md`, `prompts/rules/answer-shape.md` and `prompts/rules/plain-writing.md`; then
  the rule against destroying the system, `prompts/rules/self-preservation.md`; then the brain's
  discipline, how the agent reads and writes its long-term memory every turn, read from the brain at
  the launch through its client and left out where no brain answers; then the tool host's paragraph on
  finding and using tools with `bmo use`, read from the tool host at the launch through its client, whole
  and as the host words it, and left out where the host does not answer; and then the persona, the base
  layer of the agent's own brain, or the one the turn hands in where the brain gives none
  (`docs/sessions.md`, 「Document reload」), left out where the delegation chain chose none for the person
  the supervisor serves (`docs/prompt-layers.md`, 「The persona's choice」).
- **The layers after the harness follow it, never replace it:** the harness opening stands
  first and whole — agency's own on Claude Code, dsh's on dsh — while this machine's settings,
  instruction files, MCP servers and skills reach no chat session.
- **The session stands as the supervisor's resident program in its pane,** on the login that session was opened
  under, and that program keeps its slash commands, so compaction is `/compact` typed into it.
- **The login is part of what identifies a session** rather than a detail of the run: a fold sent out
  under any other one is told there is no such conversation, which is not a session too full to fold
  but a session the CLI cannot see at all.
