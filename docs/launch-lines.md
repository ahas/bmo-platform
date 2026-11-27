# The Claude launch lines

## What this document holds

- **How each agent's Claude process is started:** its model and effort, the tools it is shown, its
  permission mode and the tools it is denied or allowed, what of this machine's configuration it reads,
  how its system prompt is delivered, and its environment.
- **Where the lines are:** agency's server's `lines` module — the chat line every supervisor stands on,
  a pieces-shaped supervisor's turn, and the lines a role names: the worker line, the search, browse, plain and
  translate lines. The interactive program a supervisor keeps standing in its pane, and the one each run of a role
  stands in a pane of its own, are built from those same lines by the Claude Code surface in
  `packages/agent/claude-code.ts` (`docs/agent-control.md`, 「The interactive surface a provider
  declares」).
- **Two shapes of one line.** A supervisor's line is started once as the interactive program standing
  in the supervisor's pane: the same flags without print mode and without a prompt, the session named by
  `--session-id` for the first start and by `--resume` for every start after it, and `--settings` naming
  the place's settings file — the status line and the hooks — placed after the denied tools so that list
  flag never swallows it. Every turn is words put into that program. A role's line runs each run apart:
  the same interactive program in a pane of its own, opened for the run on the session its role gives it
  — a new one, or the one the role keeps at that path while it is valid — and closed with it
  (`docs/agents.md`, 「A run in a pane of its own」). No line has a print-mode
  shape: the code refuses a run that asks Claude Code for its answer on the program's output
  (`docs/agents.md`, 「No print mode」).
- **A line is this repository's code, changed by the work that needs it.** A job whose item is
  served by a line carrying something more or something else — a tool, a server, a setting — changes
  that line as part of the item, in the same commit as the code that uses it. What holds a line
  steady is the cache (「Why a line is held steady」 below): a change is made whole and once, never as
  a side effect of other work and never to a line the item does not need.
- **Which provider carries a piece of work is a different question** and is not settled here: every
  supervisor and every agent of a role stands on one road in agency's store, the owner sets it in the
  console, the program that runs the work follows from the road's provider (`docs/provider-order.md`), and a
  provider whose program cannot carry that work's line is refused as a road rather than silently standing in
  for it.
- **The model, the effort and the thinking are the road's.** A supervisor's turn and a role's run go out with
  the model, the effort level and the thinking its road names (`docs/provider-order.md`, 「Effort and
  thinking」). On Claude Code the level is the
  `--effort` word and thinking off is `MAX_THINKING_TOKENS=0` in the environment, so no flag moves that
  is not already the line's. Where the entry names nothing, each line below is as written.
- **The advisor is the road's too, on Claude Code's own road alone.** A road naming one puts `--advisor` and
  its model right after `--effort` on the chat line, the worker line and the runner lines, as the server's lines
  compose them; a road naming none, and every road the gateway serves, carries no such word, so the line of a
  road that names none is the line as written. The flag goes out only where the program would attach the
  advisor to the model the run really goes out on, judged by the provider's declared pairings, since the program
  refuses to start on one it cannot pair; one dropped so is said in the server's log (`docs/provider-order.md`,
  「The advisor」). A role's run takes its seat's advisor, or the asking supervisor's for a seat that follows it; a
  translation, which holds no seat, takes the advisor of the agent its road was taken from (「Translate runner」).
  The CLI's own `/advisor` command, which writes the login's user settings every session on the login reads, is
  never used, and no line reads the user settings it would write.

## Harnesses

- A launch line is one harness's answer to a description of a run. What a run is gets said once, in
  agency's own words and no program's (`packages/agent/harness.ts`): the model and the effort, the
  system prompt as a path to a file (both the CLI and any second harness take it that way, and a
  chat's prompt is tens of kilobytes no command line would hold), the tools shown, the tools allowed
  and the tools kept back, the MCP configuration, the prompt and whether it is written to the run's
  input or stands in the command, the session resumed and whether one is kept at all, whether the
  harness's own commands are reachable, whether the model thinks, the shape the answer comes back
  in, the directory and the environment.
- Not one of those fields is named after a flag, because the whole point is that another program
  answers the same questions in different words.
- A resumed harness reads that file again, so the runtime prompt files survive the atomic rebuild of
  the deployed agent-document tree; a restart must not leave a stored thread pointing at a path the
  rebuild erased before its first keepalive.
- A harness answers two: the command for a run, and the reply read out of what the finished process
  left behind — and both halves hold, so what a run printed is turned into a reply by an
  implementation and by nobody else.
- Whatever a caller wants of a finished run — the text a judgement answered with, the session a fold
  folded, the account's windows, what the run cost — it asks the harness that built the command for
  the reply and reads it off that; a run carried by one program and read in another's vocabulary
  would throw on its first answer.
- **Where the stream while it is still arriving says something in the program's own words, that too
  is read behind the interface**: a watcher learns which session a run is answering in, and what the
  run's events say about the subagents it opened, by handing each event to the harness rather than
  reading those words itself, and a harness whose program streams neither answers neither.
- What is still outside the interface is the reading no second program has yet wanted: a run's own
  log, what it is doing this minute, the spans a job's time is broken into and what a run killed at
  its ceiling had written are all read off the bytes as they come, in Claude Code's own stream
  vocabulary, because there is no reply until the process has ended — so a second harness that
  streamed anything of its own would want that reading behind the interface as well.

### The harnesses this build carries

- **Claude Code is the one harness every provider's runs go out on** (`packages/agent/claude-code.ts`), and
  every flag the CLI is spoken to with is written in that one file — the lines (a chat round, a run of the
  worker line, a search, a browse, a translation) and the two folds are its answers, each in its fixed
  order.
- **dsh — the DeepSeek Harness — remains in the build** (`packages/agent/dsh.ts`) for a rollback to the road
  this deployment left, and no provider of this build names it. Its program takes no flags to speak of, so a
  run becomes a patch overlay written fresh for it, naming rows of dsh's own composition — the run's own prompt
  as the persona suffix after dsh's own, the model and the provider's API it runs on as a route, the tools it
  may have — and the command is the pinned `@deepseek-ai/dsh` at its headless profile with the task standing in
  the line. The task stands there as words headless joins back with one space, each short of the kernel's limit
  on one argument, and it reaches dsh out of a file of its own rather than through the string npx hands a shell,
  because a task can run to hundreds of kilobytes and that string is refused whole long before then.
- **dsh reads every `{{name}}` in a persona as a variable of its own** and refuses the whole prompt on a name
  it does not hold, so each `{{` of the text agency hands it is broken by a word joiner, which the model reads
  past; the headless opening line's own `{{model}}` is left for dsh to fill.
- **What a run may do to the files around it is not one of those rows alone**: dsh keeps how far a write may
  reach and whether the run stops to ask as two knobs, and names only the pairs they make together, so a run
  that set the reach and left the asking behind would be a composition dsh refuses outright — before a word
  goes anywhere. The mode therefore travels in the run's environment as the one name dsh resolves both ends
  of, as well as on the row that states it.
- **The Codex CLI remains in the build** (`packages/agent/codex.ts`) for a rollback, and no provider of this
  build names it: `codex exec` on the machine's own login, its JSON events read back into the same reply, a
  thread id kept per scope, model and effort.
- **A harness after those is one more file.** What it has to write is the whole of the interface: a name to be
  registered under, the lines it can really carry, the word its own program folds a session with, a command
  for each of those lines, a reader, and — where its program has them — the session one of its streamed events
  names, what those events say about the children a run opened, and the tail of the record it kept for a
  session it keeps.
- What it does not write is everything above it — it composes no documents, decides no policy about who may
  use which tool, records no usage, judges nothing about the account, and never reads the route that chose it.
- **What a harness cannot answer it says so about rather than pretending**: dsh's own prompt preamble stands in
  front of whatever prompt it is given, its tools are its own and can be had whole or not at all rather than by
  name, it keeps no session agency could resume, and it reports nothing about what a run cost — so a reply off
  it names no session and carries no figures, and nothing downstream is told a number that was never reported.

### Providers and what a route may promise

- **The owner chooses a provider, never a harness.** The providers are Claude Code, Codex, OpenRouter, the
  registry's and DeepSeek.
- Each is declared beside the program that carries it and answers for itself whether it can be used on this
  machine right now and why not, where its models come from, which effort words an entry of it may hold and
  whether one of its runs may go out with an effort or without, whether an entry of it carries a ceiling on
  the answer's tokens, whether agency chooses the login it goes out under, and how its own reply reports what
  one turn cost — so whoever wants one of those facts asks the provider, and no file outside those programs
  writes the providers' names out to answer it.
- `packages/agent/harnesses.ts` is the one file that names the providers this build carries: it composes them
  into the agent base's provider registry (`packages/agent/agent-providers.ts`), and every other file reaches
  a provider through that registry.
- Claude Code is carried by Claude Code, Codex, OpenRouter and DeepSeek by Claude Code pointed at agency's
  gateway under a member of their road (`packages/agent/member-provider.ts`), and the registry's by Claude Code
  pointed at the gateway under a registration, with a request that needs no tools asked of the gateway
  directly. The models of each member road are the members the deployment names, which the owner adds and
  changes with no commit (`docs/own-llm.md`). On every line their program in the agent's pane is Claude Code,
  and a headless run of any of them is the same program on the same gateway.
- Which provider runs a piece of work is its road's, judged for quota and whether it can answer now
  (`docs/provider-order.md`, 「Whether a road can answer」), so a program is picked in exactly one place.
- **What a route may promise is what the program really carries.** Each harness says what its
  program offers — a gate on every tool call, a tool set made of the MCP servers it is handed alone
  — and each line what it needs of its program: Claude Code offers both and so carries every line;
  Codex and dsh offer no gate on a tool call and carry a chat round and a job, each as a portable run the
  fallback runner hands over; dsh with every tool row of its own off also carries the search line,
  whose tools are the run's MCP servers alone, and the browse line stays Claude Code's.
- **A runner line's apart session goes out on any road whose program carries the line.** A consumer
  that composes a runner's instructions itself starts an apart session on the line with them
  (`docs/api.md`, 「sessions」) and names the provider and model: on Claude Code each turn is a run apart in a
  pane of its own; on a road whose program carries its own turns — a member road's, the registry's — each turn
  goes into a place of the session's own, composed as a worker's job on that road is (the member's or the
  registration's home, window, answer ceiling and wait), with the line's instructions as its system prompt,
  the line's MCP servers as its tools and nothing of the program's own, and the session token its place
  carries issued with the labels and the capabilities the session named. A provider whose program does not
  carry the line refuses the start.
- A role names the one line its runs go out on and a supervisor stands on the chat line, so a provider
  whose program does not carry that line, or that cannot be used here (no Codex executable, no gateway
  key, no OpenRouter key, no DeepSeek key file), is offered on the console as unavailable with the reason
  beside it and refused by the control API as an entry of the order.
- The lines themselves are the owner's alone: a flag added, removed or reordered rewrites the prefix
  every warm session of that line stands on, so what an implementation may not do is move one.
## What is fixed

- **Every line on Claude Code is handed one whole system prompt** with `--system-prompt-file`, in the
  CLI's own place; none appends to the CLI's prompt. A supervisor's chat session, a run of the worker
  line, a pieces-shaped turn, and every turn on the search and browse runner lines each open theirs on the
  platform's harness opening, `prompts/harness.md` read out of the deploy tree, but a run of the worker
  line on an account that names a base prompt, which opens on that base (`docs/base-prompts.md`); the
  translator's is its role's and purpose's, whole.
- **Chat session** (`chatArgs`): the model, effort and thinking the supervisor's road names, read out of
  agency's store at every launch (`docs/provider-order.md`). None is written in code and no constant stands
  behind any — a supervisor always names a road, and a provider that cannot carry the chat line is refused as
  the road is written. The console is the one place
  they are chosen, and the line, the record of the session, the keepalive's cache policy and the check for a
  runtime downgrade all read that one answer. The system prompt is the layers `docs/prompt-layers.md` states,
  each whole (`docs/agent-documents.md`, 「A supervisor of the window shape」): the platform's harness opening,
  `prompts/harness.md`; then, for a supervisor of no account or of the platform administrator's, the platform's
  rulebooks the chat line's policy names; then the organisation's instructions the delegation chain gives the
  person the supervisor serves, where it gives any; then the chat role's documents with the tools paragraph composed for the program the
  session runs on, the three answer rulebooks, the rulebook against destroying the system
  (`prompts/rules/self-preservation.md`), the brain's discipline and the tool host's paragraph on finding
  and using tools (`docs/api.md`, 「Calls agency makes」); then the persona — the base layer of the agent's
  brain, or the one the turn hands in where the brain gives none (`docs/sessions.md`, 「Document reload」),
  unless the chain chose none. On Claude Code the layers are handed over as one file, the whole system prompt
  in the CLI's own place (`--system-prompt-file`); on dsh, whose own prompt is its harness, every layer after
  the harness goes as the persona suffix of its system prompt and the platform's opening is not sent. No settings sources, no
  skills, no Chrome, and the chat line's MCP servers, named in `chat.mcp.json` beside the strict flag that
  keeps every other server out: the tool host's namespaces the chat line names — `bmo`, `gate`, `eye`,
  `ear`, `testbed`, `voice` and `research` (`policy/run-lines.toml`, the chat line's `namespaces`) —
  each a streamable HTTP server behind the run's session token (the tool host's `mcp-host.md`), which is
  where the supervisor's own tools stand — sending, its brain's memory tools, and the gate's translator
  among them; the artifact and explorer services as first-class servers of the line, `artifact` and
  `explorer`, which publish a document and read a page; the job service's harness plugin, `job`, which
  leases an item from a queue, started on the chat's own caller token where the deployment names one
  (`BMO_JOB_CHAT_TOKEN`, or the file `BMO_JOB_CHAT_TOKEN_FILE` names), so the job service records what the
  chat does under the chat's name — an approval among it, which opens a draft where the deployment marks the
  chat's caller an approver and stands as a recommendation where it does not — and on the deployment's
  shared token otherwise — either handed as the file it stands in and read by a shell in front of the
  plugin, which takes its token from its environment alone (`docs/agent-control.md`, 「A token is handed as
  the file it stands in」) — subscribed to no queue and so named no holder, since the plugin's `mcp` verb takes a
  holder only beside the queues it subscribes for it and refuses to start on one alone (`docs/agent-control.md`,
  「An item's change is told to its orderer」) — its own item creation left out of what it serves (`--without job_item_create`) and, on Claude Code, among the line's denied tools
  as well; and the agency's own, `agency`, started for the supervisor the configuration is
  composed for (`--agent`, one configuration a supervisor), which files an item through agency and launches
  the item's job (`docs/api.md`, 「assignments and jobs」). A supervisor whose program stands on a node is
  handed none of the job, agency, artifact and explorer servers, which run with this machine's addresses and
  secrets, and the tool host's `agency`, `web` and `artifact` namespaces in their place, each where its tool limit
  keeps the capability the server stands under (`docs/person-nodes.md`, 「A supervisor's tools on a node」). A
  hosted tool those namespaces do not show is reached with `bmo use`. The chat line names no `web` of the tool host's: the CLI's own `WebSearch` and
  `WebFetch` are denied, and the explorer server is the search and page reader of the line. The search and
  browse lines keep their `web` tools, since a search or a browse is the open web's work and they have no
  explorer server of their own. The permission mode is bypass-permissions, the same words a role's run is
  started with, and the chat line's denied list is its only restriction (`policy/run-lines.toml`, the chat
  line's `tools_denied`): the CLI's web search and fetch, its Artifact tools, the claude.ai Docs connector,
  the tools that bring a session back after its turn has ended — `Monitor`, `ScheduleWakeup` and the
  session cron tools, since a supervisor's turns are the ones its runner puts into its program — and the
  job plugin's own item creation — and the program's own tools no supervisor is asked to use: its design
  sync, plan mode, phone notifications, remote triggers, findings report, feedback and the sends to its own
  projects. The CLI's own subagent tool stands, for reading and searching inside the supervisor's turn
  (`docs/supervisors.md`, 「A supervisor's long work」), with the subagent types the line's carries keep and
  no other (「What a line carries」 below). The program is
  shown every other tool of its own, no `--tools` list
  narrows it and no allow list is written, and every call of every tool it holds goes ahead unasked, every
  server of its configuration among them, in every run of every supervisor, whoever wrote the turn's
  messages. The place's settings pass over the CLI's warning on the bypass mode, as every place's do, so
  the program comes up with no person to answer it. A line that names fewer servers than these leaves the
  supervisor with no tool of its own: a configuration that drops the tool host drops sending and memory
  with it. What is the owner's alone — approving, launching or finishing work, deploying, reading another
  room — is held by the session's own judgement under its rules (「What a room's
  words may make a supervisor do」 below), not by a permission list. The chat stands as the
  supervisor's resident program, whose line keeps its slash commands — `/compact` is one, typed into the program to fold the
  session in place, on the same prefix its turns stand on. There is no separate fold line for the chat. It
  works in a folder whose name a release does not change — the deploy tree's link on dsh, the agent's own
  home `supervisor-homes/<agent>` on Claude Code, which names its records after the folder's real path — so
  the program's own record of its session is taken up again after a restart or a deploy (docs/sessions.md,
  「A restart hands the session over」). A turn that finds no pane to stand the program in, herdr down, fails
  as a transport failure and is run in no other shape.
- **Worker line**: the line the `worker` role names and every role that names it — the coder, the
  helper, the producer and upkeep (`docs/agent-roles.md`, 「The roles
  the policy seeds」). A run on it goes out on the model, effort and thinking its role's order names for the
  provider the run goes out on (`docs/provider-order.md`), or a job's on its repository's pinned model
  (`docs/repositories.md`, 「A pinned model」), in
  bypass-permissions mode with WebSearch and WebFetch disallowed, and with them every tool that
  publishes a page outside the system's own archive — the CLI's Artifact tools and the claude.ai
  Docs connector — since a document a job shows the owner goes into that archive (`archive.md`, 「A
  document a job shows the owner」). The tools that bring a session back after its turn has ended are
  disallowed too — `Monitor`, `ScheduleWakeup` and the session cron tools `CronCreate`, `CronDelete`
  and `CronList` — because a job has no later: its turn's end is its answer, and a run those tools
  keep re-invoking never reaches its own ending, so nothing it wrote is recorded and the unit burns a
  model run on every re-invocation until somebody stops it. The CLI's own question tool,
  `AskUserQuestion`, is disallowed as well: it stops the pane on a choice among options, a job's pane
  has nobody to pick one, and the session contract's question has no shape for that answer
  (`docs/agent-control.md`, 「A confirmation in the middle of a turn」). So are the program's own tools no job
  is asked to use, the same the chat line denies beside its own, and every subagent type the line's carries
  do not keep (「What a line carries」 below). A job asks its supervisor with
  `bmo job ask`, which waits on the answer the supervisor gives or the owner gives once the supervisor puts it
  to him. The claude.ai account's connectors — Gmail, Google Calendar,
  Google Drive, Claude Docs and whatever else the login has connected — are not loaded at all:
  the worker settings set the CLI's `disableClaudeAiConnectors` to `true`. To turn them back on,
  set that key to `false` in `prompts/agents/worker/settings.json` and let the commit go live; it
  moves the worker prefix like any change to the line (「Why a line is held steady」). The Claude
  Docs server stays on the disallowed list either way, so turning the connectors on brings back
  the others and never the one that publishes. The program runs with permissions bypassed, so the worker
  settings also set `skipDangerousModePermissionPrompt` to `true`: a run on the line in a pane of its own,
  which reads those settings as they stand rather than a place's, would otherwise stop at the CLI's
  warning dialog until a person answered it. A run works in the path its work names: a job in its
  repository's checkout, a worker ask in the folder it names (`docs/agents.md`, 「A
  run」). No persona: its whole system prompt, given with `--system-prompt-file` in the CLI's own place,
  is the platform's harness opening, `prompts/harness.md`, then the role's prompt where the role carries
  one, then the line's rules, as one file named by what it holds. The rules are one file the line
  composes for the run — the owner's standing rules for every job,
  `prompts/agents/worker/standing-rules.md`, then, on a registration that names the languages it writes,
  the result-language piece `prompts/agents/worker/result-languages.md` filled with them (`docs/own-llm.md`,
  「A run on the worker line is composed as on Claude」), then the rulebook against destroying the system and the
  platform's `prompts/platform/environment.md` and `prompts/platform/preservation.md`, the platform around the
  run and the platform's own parts the rule protects
  (`docs/prompt-layers.md`, 「The platform's rules」), then the
  repository's own rules where the run works in a repository's checkout and the repository names a
  document of its own, `prompts/agents/<folder>/own-rules.md` from the prompt folder its row names (a
  co-work branch in a standing checkout takes none), then the report trailer every job is held to,
  `prompts/agents/worker/trailer.md` filled in as the deployment shows it (a co-work
  branch takes none),
  then the documents the repository names as its prefix documents (`docs/worker-bench.md`, 「Prefix
  documents」; `docs/repositories.md`, 「What a run in a repository is handed」). A run whose path is no
  repository's is handed nothing of a repository. The trailer stands in the whole prompt, the
  session's cached prefix, so a session pays for it once; the job's prompt carries no trailer, and of
  its item's notes only as many as fit the policy's bytes (`docs/api.md`, 「assignments and jobs」). A
  run that goes out on a Claude account naming a base prompt of its own
  (`docs/cache-fallback.md`, 「The Claude Code road is a list of accounts」) takes the account's base in
  the harness opening's place, with the role's prompt and the same rules after it, as one file named by
  what it holds and given with `--system-prompt-file`; so a run on an endpoint account opens on the base
  its endpoint's harness runs on rather than on the platform's opening (`docs/base-prompts.md`). The base
  is composed with the account the run goes out on, so the line's prefix is the account's as well as the
  role's. Where the role gives a provider prompt for the provider the run goes out on, it stands after the
  role's prompt (`docs/agent-roles.md`, 「A role's provider prompts」). A job carried by another provider whose program is a
  harness of its own is handed
  the rules file alone, in the place that program keeps beside its own default prompt, never in place of
  it, and the platform's opening is not sent: on Codex as the run's developer instructions (`developer_instructions`), with Codex's own
  coding-agent prompt and the checkout's `AGENTS.md` left as they are; on OpenRouter and DeepSeek as the
  persona suffix of dsh's system prompt, after dsh's own prompt whole, with its instruction-file row left
  as it is. A run on the worker line handed no such file is
  refused on every provider rather than started without it. What
  the run is shown beyond the prompt comes from this repository and not from the machine: the
  machine's own CLAUDE.md, which every checkout under the home directory would walk up to, is named
  in the worker settings' `claudeMdExcludes`, the user scope this machine
  registers its own skills, Claude Code subagent definitions and MCP servers in is shut off on the
  line, and in its place the line hands over the skills as a plugin directory — the role's bundle: the
  skills repository's `worker` bundle at `prompts/agents/worker/plugin/` of the release for every run but
  the coder's and the producer's, the `coder` bundle at `prompts/agents/coder/plugin/` for the coder, and
  the `producer` bundle at `prompts/agents/producer/plugin/` for the producer, each placed afresh with
  that repository's own placer (`bmo-skills place --bundle worker|coder|producer`) from the skills
  checkout that stands beside agency's in its workspace, or the one `BMO_SKILLS_DIR` names, before the
  deploy tree flips, so a skill the skills repository gains reaches the next job after the next
  deployment; the copy a checkout of this repository carries stands for a tree no deployment placed — and
  the settings the run is
  held to — which is also where the Bash tool's wait for a job's command is set, its default and its
  most both the largest the program takes, so a command is waited on until it ends, and where the hook
  stands that reads every shell command a job runs
  before it runs and refuses one that would open a Claude Code session or write into the CLI's
  session records until the job has said what for with `bmo-agency live-claude --purpose`,
  which asks nobody and keeps a count of every run it lets through for the item's report
  (`docs/live-claude-guard.md`) —
  and two namespaces of the tool host in one configuration: `web`, which every agent that may use
  the web is given, and `worker`, which serves the queue's two reading verbs as
  tools (`docs/worker-mcp.md`), shown to a run by its role's bundles (`docs/agent-roles.md`, 「The
  bundles」). The line's record names a third, `bmo`, which no configuration names: the brain's lessons form
  the `brain.lessons` bundle holds is reached there with `bmo use`, on the brain the record's
  `BMO_BRAIN_AGENT` names, the line's policy marking it a line that works for a supervisor's brain. No other line is handed that file, so what is added to it reaches runs on the worker line
  and nothing else. So exactly one copy of each reaches a run, a checkout on another
  machine runs the same job, and changing any of it is a commit here rather than an edit to the
  machine. The deploy unit's `deploy.md` says what the whole of it adds up to and what to do when it moves.
  Every one of those files — the settings and their hooks, the plugin and the rules — is read out of the release the job was launched on rather than through the deploy
  tree's link: the job carries that release in `BMO_DEPLOY_DIR` (`docs/agent-control.md`, 「The
  job's release」), so a deployment during the job changes none of them under it.
  The skills and plugins the CLI carries of itself reach no run of the line (「The CLI's own skills and
  plugins are switched off on every line」 below), so a job is listed its role's bundle and nothing else.
  A run on the line is the interactive CLI in a pane of its own, opened when its slot is let in and
  closed with the run (`docs/agents.md`, 「A run in a pane of its own」): the program is started with no
  prompt, on `--resume` of the session its role keeps at that path on that provider and model where one
  is valid and on a new one named by `--session-id` otherwise (`docs/sessions.md`, 「A role's session」),
  told its work over herdr's socket once it is idle, and read back
  out of the record the CLI keeps for that session — so a word added to the item reaches it while
  it works, and the owner can watch it in herdr or type into it while its pane stands. Nothing of the
  line stands between runs and nothing warms a kept session; the resume adds nothing to the cached
  prefix, so the line itself is the same line. A session whose context has passed its role's compaction
  threshold is folded by `/compact` typed into a run's program between turns, so the fold stands on the
  prefix the runs stand on (`docs/sessions.md`, 「Session compaction」). Each run's turn goes into the
  program under the run name its runner gives it, carried as `BMO_RUN_NAME` beside `BMO_TURN_AGENT` and
  `BMO_TASK_LOCKED=1` where the turn has a job, so the verbs the job runs find their job; the variables
  the turn names stand beside them, and the turn's title is what the pane shows over it (`docs/api.md`,
  「sessions」, a work turn's parts). A run of an account with a node stands the same program, on the same
  line and session, in a place of its own on that node rather than in a pane here (`docs/person-nodes.md`,
  「Which node is a person's」). A co-work branch is a run in the standing checkout it works in
  (`docs/role-slots.md`, 「Standing branch checkouts and their list」), on an apart session of its own
  that it keeps nowhere; an apart session handed a runner line's instructions goes out on that runner
  line rather than this one (`docs/api.md`, 「sessions」). Any role on the line may run in any
  repository's path: what differs between runs is the path the run works in, the session it resumes (one
  per role, path, provider and model) and the checkout it holds (one run per checkout, so two jobs never
  share a checkout and two checkouts never wait on each other). No clock ends a run: what a kept
  session is lost to is its cache going cold, a moved prefix, the owner's word, a disaster, or a Claude
  API error.
- **A run of the worker line on a registered model**: Claude Code pointed at agency's gateway under the
  registration's name (`docs/own-llm.md`, 「How requests go」), on the worker line above whole —
  the platform's harness opening with the role's prompt and the line's rules after it, the plugin, the
  the worker MCP server, the project and local setting sources and every tool the line
  shows — and the settings the place writes over the line's own. What differs is the registration's
  gateway name, with its node where it has several, at `medium` effort with thinking on and `low` with it
  off — thinking as the order's entry names it, else as the registration's default — and its endpoint:
  the gateway's address, the registration's context window, the answer ceiling and the request wait,
  each read from the registry as the program is started. Its records and first-run answers stand
  under a home of the registration's own, and the gateway's key is read from its file by the
  settings' key helper. Each run is that program in a pane of its own, on the session its role gives it
  (`docs/own-llm.md`, 「A run on the worker line is composed as on Claude」).
- **A run of the worker line on a DeepSeek, OpenRouter or Codex model**: Claude Code on the gateway under
  the member the role's order entry or the repository's pin names, exactly as a run on a registration
  stands (`docs/own-llm.md`) — a worker's standing program and a role session's turn in a place of its own
  alike: the worker line whole — the platform's harness opening with the role's
  prompt and its rules after it, its plugin, the worker MCP server, its settings and
  hooks — with the member's home, window, answer ceiling and request wait, and the gateway's key read
  from its file by the settings' key helper. The member's effort word goes out in the effort header; no
  login of the Claude road goes with it.
- **A turn of the pieces shape** (agency's server's pieces turn): a turn of the supervisor's resident
  program, which stands on the chat line above as every supervisor's does (`docs/supervisors.md`, 「One
  runner」) — the same permission mode and denied tools, the same configuration with agency's own
  plugin among it, so it files items and launches jobs as every supervisor does, the same mode and the
  same shell guard — with the system prompt its own
  settings compose (`docs/supervisors.md`, 「What a turn is given」) in place of the window shape's — one
  whole prompt given with `--system-prompt-file`, opening on the platform's harness opening — its own
  home, the session of the room the turn is for, resumed or opened under the id the turn names, and its
  program's own commands reachable. The room, the supervisor, the run,
  the messages the turn answers and whether each is the owner's stand in the place's run as its own
  variables. On a registered model it goes out as every run on one does, the thinking its order's entry
  names or else the registration's, with its generation settings as the run's sampling.
- **Search runner**: the line a search goes out on, carried by agency and composed by whoever starts a apart
  session on it (`docs/api.md`, 「sessions」) — the research unit for its searches, their rounds and their
  branches. Its instructions are handed in whole by that caller; agency composes none and asks nothing of a
  run after its answer. On Claude Code: the model and effort the session names, else those the voyager
  role's order names for the provider it goes out on, don't-ask mode, no built-in tool shown, the web MCP
  server's tools and the brain's `bmo` namespace, and nothing else allowed; any other hosted tool it holds
  is reached with `bmo use`; no settings sources, no other MCP, no Chrome, run in its deployed directory
  unless the session names a folder. It is handed the standing `search.mcp.json`, and the run's session
  record carries the labels and keeps the capabilities the session's caller named, so the web tools read
  what that caller told them; the record also names the supervisor — the agent whose row answers every room
  that names nobody — as the turn its brain tools run for and as `BMO_BRAIN_AGENT`, so a fact the run
  establishes is written into the supervisor's brain while the run itself stays the runner's. The voyager role's bundles show it
  bmo-explorer's `research` tool, which opens a branch through the research unit, and the research unit's
  `rulebook` reader where the record holds `web.rulebooks`; the line's `eye` namespace shows it `eye/see`,
  which looks at a picture by its address and answers a question about it. No native Claude Code subagent is
  opened by any run on it.
  - **Its system prompt is the platform's harness opening and then the instructions handed in**, as one
    file replacing the CLI's own, written under a name its text gives, so two sessions handed the same
    instructions read one prefix. What differs per run is the prompt. So nothing of Claude Code's coding
    conventions reaches a search.
  - **What a run establishes is written into the shared brain.** The search line holds
    `brain_learn` and `brain_similar` on the supervisor's brain: a fact or a piece of knowledge
    a run establishes is written there with the source it was read from, after the brain's similar rows
    are read, and the brain's write judgement confirms a duplicate, strengthens a version of a row and
    closes what a row contradicts.
- **Translate runner**: one translation asked through agency's `POST /translation-runs` (`docs/api.md`,
  「translations」), on the first road of the translator role's agents that can answer (`docs/provider-order.md`,
  「Whether a road can answer」). A translation holds no seat of the role: it goes out with the effort and the
  advisor of the agent that road was taken from, the first agent standing on it that carries no run, else the
  first that does (`docs/role-agents.md`, 「Which agent a run takes」). On the registry's provider, the registered translation model answers. Its system prompt is the translator role's prompt
  as the store holds it (`docs/agent-roles.md`) and then the instructions of the purpose asked —
  `prompts/agents/translator/instructions.md` for a document's part, `worker.md` for a text a job reads,
  `record.md` for a job's record the owner reads, that one after the plain-writing rulebook, and the
  caller's note on the part after them — and its input is one line naming the target language and then the
  text. Where the caller names terms (`docs/api.md`, 「translations」), the input is instead the translation
  model's own terminology form, which a translation model follows more closely than an instruction in the
  system prompt: a line saying that reference translations follow, one line for each term, its source and
  then its target, a blank line, the instruction to translate the text into the target language and answer
  with the translation alone, a blank line, and the text. On the registry's provider it is one request to
  the gateway's chat completions path under the registration the order's entry names, with thinking off and
  the profile's sampling (`docs/own-llm.md`, 「A request with no tools」), with no wait of agency's own and
  naming none to the gateway, so a translation on a registration naming a card service is waited for however
  long the translator takes, its parts queued at the engine included, and one on a registration of addresses
  waits that entry's own wait (`docs/own-llm.md`, 「The gateway」); where the registration names a card
  service, the request names the entry of the card the translations out on it at once share, waited on
  however long its grant takes and given back once the last of them has ended — answered, failed, or let go
  by its caller — whether its demand was granted yet or not (`docs/own-llm.md`, 「An engine the arbiter
  stands」). Such a registration is the registry's road whether or not its engine stands when the translation
  is asked: the demand starts it, the request waits until the gateway serves the granted card's entry, and a
  translation never moves to Claude Code because no instance stood. A demand the master refuses, or a
  request the engine fails, is a failure of the provider, and the translation fails or waits as its road
  says (`docs/provider-order.md`, 「A road that fails」; `docs/own-llm.md`, 「When a
  registered model does not answer」). On Claude Code it is a run in a pane of its own (`docs/agents.md`, 「A
  run in a pane of its own」): that system prompt as the whole of it, no tool shown or allowed, no MCP
  server, the session the translator role's setting gives it (`docs/agent-roles.md`, 「A role's sessions」),
  thinking off. No other provider carries the line. The answer is the translation whole. A translation
  through the gateway leaves a run of the translator role and its log lines (`docs/run-records.md`, 「The run
  ledger」), as a run apart on Claude Code leaves its own.
- **The app review**: one shared item's review (`docs/person-nodes.md`, 「Shared apps」) goes out as a translation
  does, on the same runner and its key, on the first road of the `app-reviewer` role's agents that can answer,
  with that role's prompt as its whole system prompt and the item's kind, permissions and source as its input;
  it answers the report alone.
- **Browse runner**: the line a browse goes out on, carried by agency and composed, as the search line is,
  by whoever starts an apart session on it: the model and effort the session names, else those the seeker
  role's order names for the provider it goes out on, don't-ask mode, no built-in tool shown, the
  chrome-devtools tools, the tool host's web tools and the brain's `bmo` namespace from the line's MCP
  configuration — chrome-devtools started headless, apart from a person's session, its D-Bus and its
  keyring, the web namespace as every line names it, the run's own values in its session record — and
  nothing else, no settings, no other MCP, no Chrome integration, run in its deployed directory. Its system
  prompt is the platform's harness opening, `prompts/harness.md`, then the instructions its caller handed
  in, as one file replacing the CLI's own; the task and the time the run has are the prompt. chrome-devtools
  is started without its performance and memory tools — tracing a page's load and taking a heap snapshot —
  since a run that reads what a page says has no use for them. Claude Code alone carries the line.
- **Plain runner**: the line a session goes out on where the instructions its caller hands in are the whole of
  what its run is told — the note unit's note assistant (`docs/agent-roles.md`, 「The note-assistant role」).
  It is carried by agency and composed, as the search line is, by whoever starts a session on it
  (`docs/api.md`, 「sessions」). On Claude Code: the model and effort the session's slot names, don't-ask mode,
  no built-in tool shown and none allowed, an empty MCP configuration under the strict flag so no server of
  any kind stands, no settings sources so no CLAUDE.md, memory, user or project setting and hook is read,
  slash commands and skills switched off, no Chrome, run in the line's own deployed directory. No tool host
  namespace is named for it, so its session record holds none.
  - **Its system prompt is the instructions handed in and nothing else:** no harness opening, no role prompt,
    no provider prompt and no persona, as one file replacing the CLI's own, written under a name its text
    gives, so two sessions handed the same instructions read one prefix.
  - **What the program adds of itself stays,** since no word of its command drops it: its one-line identity
    ahead of that file, the environment block it puts beside the first prompt (the folder, the platform, the
    model and the date), the login's mail address on a subscription login, and the typed line a run apart's
    words are pasted behind (`policy/resident-turn.toml`, `apart_lead`).
- **The web**: no agent of the system's is shown the CLI's WebSearch or WebFetch. Those hand back a
  model's rendering of a page, and a page can steer that model. The web reaches every agent
  through the tool host's `web` namespace (`mcp-host.md`):
  `web_search` asking the search engines directly (`web-access.md`), `web_fetch` returning
  the page's own text, a GitHub file page as the raw file. The runners get it from a configuration
  of their own — the search runner's standing `search.mcp.json` and the browse runner's `.mcp.json`
  beside its chrome-devtools; the worker line has the two built-ins disallowed and is handed
  `worker.mcp.json`, which carries that same server beside the worker's own and which no other
  line is given, since the scope this machine registers the server in is shut off for it.
- **`bmo use`**: a hosted tool a line holds and its namespaces do not list is reached with the `bmo use`
  command, under the run's own tool session token, as the tool host's paragraph in the session's prompt says
  (`toolbook.md`). No line's MCP configuration names the tool host's toolbook. The site tools are the
  voyager's and the seeker's alone; a run of the worker line and a supervisor's turn keep web search
  and page fetch.
- **Environment**: every one of these processes is started with the repository's git status
  left out of its prompt; every Claude line also carries the configuration directory of the
  account its run was put on, and that account's token where the account is a token account,
  or its endpoint's base URL, key and model ids where it is an endpoint account
  (`docs/cache-fallback.md`, 「The Claude Code road is a list of accounts」), so what a run goes out
  under is a login agency chose rather than whatever the machine happens to hold; no line sets the
  share of its model's window a session compacts at, so every program, the worker line's included,
  compacts at its own default (`docs/sessions.md`, 「Session compaction」), and only a run on a registered
  model's road carries `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`, at the share its registration leaves for
  the answer (`docs/own-llm.md`, 「A turn at the input limit」); nothing else is added beyond the
  switches below, and the store's address is never carried: it names agency's own store, not the
  archive a program reads from the data root's setting (`docs/store.md`, 「Connecting」).
  An account's variables are handed to a line by name rather than inherited, because the worker
  line rebuilds its environment from a few names and would otherwise drop them. They add
  nothing to a cached prefix — the flags, their order, the system prompt and the files it
  points at are untouched — but a run moved to another account stands on that account's own
  cache and warms it from where that account left it.
- **The interactive program's background calls are switched off on every line.** The program — a
  supervisor's resident one and every run's alike — would otherwise spend calls of its own that no
  agent of the system's reads, each one a fork of the session reading its whole context out of the cache and
  never written into the session's record, so the record and the status line's running cost part:
  - `CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION=0` — the next-prompt suggestion drawn in the input box. It
    runs after every turn whose last call wrote under ten thousand tokens, while the account is
    under its usage warning; on a chat carrying hundreds of thousands of tokens it is a
    whole-context read after most turns, and it is the whole of the gap between the chat's
    status-line cost and its session record (measured).
  - `CLAUDE_CODE_ENABLE_AWAY_SUMMARY=0` — the recap drawn when a person comes back to the pane after
    five minutes away: the same whole-context read, spent on a pane the owner only watches.
  - Both are part of the variables every line sets, beside the git-status one. Those variables are Claude Code's own
    declaration in agency's policy (`policy/providers/`), which every side that starts one of
    its lines reads. To turn one back on, set its value to `1` there and let the commit go live.
- **The CLI's own skills and plugins are switched off on every line.** A run carries the skills its role
  declares — the plugin its line hands it — and none the program brings of itself:
  - `CLAUDE_CODE_DISABLE_BUNDLED_SKILLS=1` is among the variables every line sets, so none of the skills the
    CLI bundles (a chart guide, a code review, an API reference, the Artifact guides, the CLI's own
    configuration helpers, a scheduler and the like) is listed to any run or supervisor: no role declares
    them, and each listing is prefix every turn pays for.
  - The plugins the CLI builds in that bring a run what no role declares — guides and tips, a chart server, a
    diagram and a diff renderer, the reading of a checkout's `AGENTS.md` beside its CLAUDE.md, the program's
    own platform helpers — which Claude Code's policy names (`policy/providers/claude-code.toml`,
    `builtin_plugins`), are each named off under `enabledPlugins` in every settings file a line hands its
    program; the built-in plugins holding the program's own security, privacy and telemetry defaults stay as
    they are. The files a line hands are the settings of a place a supervisor's or a run's program stands in,
    and the worker line's own settings, which names the same list. Their MCP servers, skills and instructions
    reach no run. A line that hands no settings file — the runner lines and the translate line — shows its
    program no built-in tool under a strict MCP configuration, so no skill is listed there and no built-in
    plugin's server stands. A plugin a later version of the program builds in is added to the policy's list
    when it appears.
- **The CLI's own memory is switched off on every line.** `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` is
  among those variables. Left on, the CLI's prompt tells every run that keeps it — the worker line's
  among them — about a memory directory of its own under the login's records and how to write
  it: some seven hundred tokens in a worker's prefix, and more where the file tools make it offer
  writing as well. No agent of the system's keeps anything there: what a job learns goes into its
  checkout's documents (the `bmo-docs-conventions` skill), and an agent's long-term memory is its brain.
  To turn it back on, remove the variable and let the commit go live;
  it moves the worker's prefix like any change to the line. A variable adds nothing to a cached prefix,
    but it is part of the command a pane's program was started with, so the change starts each
    supervisor's resident program again on `--resume` of its own session at its next turn, and a run
    of a role started after it is another command and opens its own session.

## What a line carries

- **Each line declares what its program carries, and its launch is built from that declaration:** the line's
  `carries` in `policy/run-lines.toml`, beside the namespaces, capabilities, shown entries and denied tools the
  line already declares. A role's run carries its line's declaration and its role's own prompt, provider
  prompts, bundles and skills (`docs/agent-roles.md`, 「What a role row holds」), and nothing else.
  - **`prompt`:** the pieces of its system prompt, in order — `harness`, the platform's opening;
    `opening`, the platform's opening or the base prompt of the account the run goes out on;
    `platform-rules`, `organisation`, `role`, `personal` and `persona`, the layers `docs/prompt-layers.md`
    names, a role's provider prompt standing in its role layer; `rules`, the line's rules file;
    `instructions`, what a session's caller hands in; `purpose`, a translation's instructions.
  - **`setting_sources`:** the setting sources its program reads; a line naming none reads no CLAUDE.md, no
    memory, no setting and no hook of the machine's or a checkout's.
  - **`skills`:** the sources of the skills its program carries, a list: `role`, the plugin bundle of its run's role;
    `attached`, the skills attached to the run's holder (`docs/person-nodes.md`, 「Attaching skills and MCP
    servers」). An empty list carries none, and a launch asked to place attached skills on a line naming no
    `attached` is refused.
  - **`mcp`:** the sources of the MCP servers its program reaches, a list: `line`, the line's own MCP configuration;
    `attached`, the person's MCP servers attached to the run's holder, reached through the tool host as personal
    tools, every other one withheld from the run's record. An MCP server of the person's reaches a run of a line
    naming no `attached` never.
  - **`slash_commands`:** whether its program keeps its own commands.
  - **`subagents`:** the program's own subagent types it keeps. Every other type Claude Code's policy names
    (`policy/providers/claude-code.toml`, `agent_types`) is denied by name, `Agent(<type>)`, on a line that
    shows the program's subagent tool, so its listing names those alone.
- **What reads it:** every launch reads the setting sources and the denied list off its line's carries; the
  runner lines and the translate line compose their system prompt piece by piece from `prompt` and take
  their commands flag from `slash_commands`, refusing a piece they cannot fill; the worker and chat lines
  compose theirs in the order `prompt` states. A test per seeded role holds the launch its line composes for
  the role to the role's declaration and its line's carries.
- **The lines' carries:**

  | Line | Prompt | Setting sources | Skills | MCP | Own commands | Subagent types |
  |---|---|---|---|---|---|---|
  | chat | harness, platform-rules, organisation, role, personal, persona | none | attached | line, attached | yes | `Explore`, `general-purpose` |
  | gate | instructions | none | none | none | yes | none |
  | worker | opening, organisation, role, personal, rules | project, local | role, attached | line, attached | yes | `Explore`, `general-purpose` |
  | search, browse | harness, instructions | none | none | line | yes | none |
  | plain | instructions | none | none | line | no | none |
  | translate | role, purpose | none | none | none | no | none |

- **What the program adds that no declaration removes:** its one-line identity ahead of the system prompt;
  the environment block beside the first prompt (the folder, the platform, the model and the date); the
  login's mail address on a subscription login; the typed line a run apart's words are pasted behind; the
  short title request it makes of a new session; and, on a line that shows built-in tools, the tool search
  that loads deferred tools and its deferred `EndConversation`, which a denial does not take off the list.

## What a line's program is allowed, by road

- **A run is allowed what its line and its role give it, on every road.** The road a run goes out on changes
  its model, its endpoint, its login and its records home; it changes nothing of the permission mode, the
  tools shown, the tools allowed or denied, or the MCP configuration. Every road a provider of this build
  names runs Claude Code — under an account on Claude Code's own road, pointed at agency's gateway on
  DeepSeek's, Codex's, OpenRouter's and the registry's — so the program on a gateway road takes every word
  the program on the own road is started with, and no road is a case of its own.
- **One table, every road:**

  | Line | Its runs | Permission mode | Built-in tools shown | Allowed | MCP configuration |
  |---|---|---|---|---|---|
  | chat | a supervisor's resident program — every supervisor's turns | bypass-permissions | every built-in tool but the chat line's denied list | everything shown and every server of its configuration — `bmo`, `gate`, `eye`, `ear`, `testbed`, `voice`, `research`, `job`, `agency`, `artifact` and `explorer` | the supervisor's chat configuration, strict |
  | worker | every role naming the line — a job, a worker ask, a co-work stage or branch, upkeep | bypass-permissions | every built-in tool but the line's denied list | everything shown and every server of its configuration | `worker.mcp.json`, the tool host's `web` and `worker`, strict |
  | search | a voyager's turn | don't-ask | none | `bmo`, `web`, `eye` and `research` by their prefixes | `search.mcp.json`, those four namespaces, strict |
  | browse | a seeker's turn | don't-ask | none | `bmo`, `chrome-devtools`, `web`, `eye` and `research` by their prefixes | the browse configuration, strict |
  | plain | a note assistant's turn | don't-ask | none | none | an empty configuration, strict |
  | translate, gate | a translation, a gate's judgement | don't-ask | none | none | none |

- **Every line's MCP configuration is strict.** A run carries the servers its line's configuration names and
  no other: an MCP server the account's login brings with it, a claude.ai connector among them, never rides
  on a run, since its tools are no part of the line and the firewall in front of the API refuses requests
  carrying some connectors' descriptions outright.
- **A supervisor is allowed the same on every road.** Its resident program is started on the chat line's one
  set of words on Claude Code's own road and on DeepSeek's, Codex's, OpenRouter's and the registry's gateway
  roads alike: bypass-permissions, the chat line's denied list as its only restriction, and its chat
  configuration whole. These are the words a role's run is started with on the
  worker line, the line's own denied list in place of the chat line's.
- **A supervisor's tool limit narrows its program, where its settings hold one** (`docs/supervisors.md`, 「Its
  settings」). The chat line's `held` declares a capability for each server of the chat configuration that
  stands outside the tool host and for each group of the program's own tools; a server whose capability the
  limit leaves out is not written into that supervisor's configuration, and a program
  tool whose capability it leaves out joins the denied list its program is started with. Its program is also
  started with the program's own list of the built-in tools it may have, the chat line's `limited_keeps` and
  the tools under each capability the limit names, and no other, so a built-in tool no capability declares,
  or one a later version of the program adds, is not there. A supervisor whose settings hold no limit is
  started exactly as above, with no such list. A program that is a harness of its own cannot narrow its tools
  by name, so a limited supervisor's turn is refused on it.
- **What stands beside the tools goes with them.** A run of the worker line takes its whole line on every road:
  the worker settings and their hooks — the guard on every shell command a job runs among them — the plugin
  and its skills, and the project and local setting sources. A gateway road writes the place's settings over the
  line's own, as every program standing in a place has them (`docs/own-llm.md`, 「Claude Code in the pane」).
- **Nothing a gateway road's program cannot take is on any line.** The one built-in a gateway's model could not
  serve is the CLI's own web search, which runs on Anthropic's side, and it is denied on every line that shows
  built-ins and shown on none that does not; the web reaches every line through the tool host's `web`
  namespace.
- **A program that is a harness of its own is told its tools another way.** dsh and Codex, carried for a
  rollback and named by no provider of this build, take a shown and an allowed list rather than a permission
  mode over every built-in: a run of the worker line on one of them is handed its line's MCP configuration and
  allowed its servers; a supervisor is shown the chat line's shown tools (`policy/chat-line.toml`, `shown`) and
  allowed every one it is shown, with no allow list of its own.

## What a room's words may make a supervisor do

- **The tools a supervisor acts with carry no room check.** `bmo/worker_ask`, `bmo/app_put`, `bmo/skill_put` and
  `run_job` on the supervisor's own `agency` server do what they are asked whoever sent the words the turn
  answers; agency asks no other unit who may ask them.
- **What a room's words may make it do is its rules'.** The absolute rules say what is the owner's alone, and no
  member of a room holds his authority (`docs/agent-documents.md`, 「How the documents are composed」). An
  item the supervisor files on a turn not the owner's stands a draft that an approver's approval opens
  (`docs/api.md`, 「assignments and jobs」).
- **What stands outside agency.** A deploy, `testbed/deploy`, is the testbed's hosted tool and a capability `bmo
  use` reaches is the tool host's; each is judged in the unit that serves it.

## Why a line is held steady

- The prompt cache matches a byte-identical prefix, and the tools shown, the system prompt and the
  machine configuration are that prefix.
- A change to any of them rewrites every affected session's context and, in a chat, costs that chat
  its warm cache.
- So a line changes when the work it carries needs it to, all of the change in one commit, and a
  line that keeps a session is changed once rather than over several commits, each of which would
  cost every session on it a cold start of its own.
