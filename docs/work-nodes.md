# Work nodes

A work node is another machine agency places agents on and sends heavy runs to. Agency stands a
workplace on it, holds a link to it, carries each account's login to it, and reaches an agent that
stands there through its link. This document covers the workplace, the link, the logins, the bot's
side of a remote agent, the rotation, and what a dropped node does.

## The workplace on a node

- `bmo-agency node work` lists every workplace with its state, its link and what is missing.
  `setup <node> [--root <path>] [--install]` stands one, `check <node>` reads the same without writing
  anything on the node, `sync <node>` copies and builds the code again, `runs <node> on|off` says
  whether heavy runs go to it, and `remove <node>` makes it no workplace. `teardown <node>` takes an account's
  isolation node away whole, its user and home with its workplace, and refuses any other node
  (`docs/account-isolation.md`, 「Taking it down」). The root is a default under the node's home unless named.
- **The node verbs ask the node service the deployment names:** the one their environment names, else the one
  the data root's `env/agency.env` names, the file the links' units run on, so `node work` and `node person`
  reach the same node service from a shell as from those units.
- **Every command of a setup goes over one connection,** which the first makes and the rest share,
  held under the runtime directory: a node may refuse a run of logins made in quick succession, as a
  firewall limiting SSH does. Each command runs there until it ends, however long a cold build takes:
  the shared connection holds a command's output for as long as it runs, and the command's end there, or
  the connection breaking, is what ends the wait here. No clock cuts a step. A person's PC, which has no
  SSH route, is reached through its hub instead (`docs/person-nodes.md`, 「The workplace on a PC」), and an
  account's isolation node on this machine through the sandbox its user runs in, each command run there as that
  user (`docs/account-isolation.md`, 「Standing it up」).
- **What setup makes, each step checked before the next, and everything still missing said:**
  1. the login, and the node's home read through it;
  2. the tools: git, rsync and curl are the machine's to have; the JavaScript runtime and the Rust
     toolchain are installed into the node's account with `--install`, each installer
     fetched into a file and run with nothing on its standing input, and waited on until it exits.
     Claude Code is none of the account's tools: the workplace keeps its own (step 4), and a `claude` the
     account installed for itself is never run, installed or updated by agency. The runtime is the version this
     machine runs, since a newer one may refuse the repository's own configuration: where the account
     has none or another version, that version is installed into the workplace's own `<root>/bun`, which
     every command and program of the workplace finds ahead of the account's, so the account's own
     runtime and its shell files stay as they were. The Rust
     toolchain is this machine's version, installed beside the account's own and named for the node's
     build alone, so the account's default stays as it was;
  3. the processor: the runtime, and Claude Code built on it, need one with the vector extensions
     they are compiled for, so a node whose processor offers none — a virtual machine on a generic
     processor type — is said not ready, as is one where the runtime is installed and still does not
     run;
  4. the code: the deploy tree's commit copied to `<root>/deploy` — its installs, build output, logs,
     env files and the data root left out — its Bun workspace under `packages/` and its screen kit under `screen-kit/` (`docs/person-nodes.md`,
     「Personal screens」) installed there, and the
     node's own host, pane and link binaries built there, since this machine's binaries are linked
     against a newer C library than a node may have. They are built in a workspace of their own under `<root>/build`, the crates they are
     made of (the agent host, the store beneath it, the pane head and the node link) each at the path it
     holds in the repository, with the node link's node build manifest and the repository's lockfile,
     so a node builds those crates alone and nothing of the server's; the manifest's members, package
     fields and dependencies are the repository workspace's own, which a test holds it to. The testbed binary a
     node runs is the testbed unit's, not one agency builds; each binary built is put in place by renaming
     a copy over it, since one running there — the far end, the agent host — refuses to be written over;
     a build ends the agent host standing on the old binary. With every copy the workplace's own Claude Code
     is laid too, under a name of its own, `<root>/bin/bmo-claude`, on no search path of the account's, so it
     never stands in for or over a `claude` the account keeps: the release this machine's `claude` reports,
     fetched by the node from Claude Code's release downloads for the node's processor and C library, checked
     against the checksum the release's manifest names, and renamed into place, where the one standing reports
     another release or none. It never updates itself, since every program of the workplace runs it with its
     updater off; the next setup or sync brings it to the release this machine runs then. A check, which copies
     nothing, finds it missing where it does not stand or does not run;
  5. the node's data root, `<root>/data`, which holds the node's agent places and nothing of the
     store;
  6. herdr: a node with none is given this machine's, which is one static binary, so a matching one is
     found there instead of asking a person to approve installing it, and the node is saved as a
     machine of this herdr once, so herdr's own sidebar shows it; the node's own session, apart from
     any session a person keeps there, is started where it does not stand, and its name and socket
     written into the node's setup record here, which the link reads; the name is the setup verb's,
     the packages under it taking it as given;
  7. every login a run on the node goes out on marked as past its first-run screens, its configuration
     directory made where it is not, and the workplace's data root trusted in it, since a supervisor's own
     folder and a run's folder on the node stand under it; then each node checkout of a repository placed
     there, cloned from its base checkout's remote at its path, the
     repository's own setup run in it, trusted in every account's directory on the node so no first
     turn stops at the folder check, and its push to the remote tried. The trust is written by the
     node's own host binary, built in step 4, since the node's deploy tree holds no `bmo-agency` and
     the node runs nothing out of its scripts (`docs/agent-control.md`, 「A first interactive run stops
     at the trust-this-folder check」). A login a road makes after the setup — a gateway road's
     configuration directory among them — is marked by the node's agent host as it starts the program
     (「The logins on a node」 below).
- **A credential file is never copied.** A subscription login renews itself by a token that changes
  as it is used, so two machines sharing one sign each other out. So:
  - Claude Code on a node runs on each account's token for the nodes, which this machine hands each
    run (「The logins on a node」 below); an account with no such token here needs a login made on the
    node in the node's directory for that account, and the setup names the command that makes the
    token;
  - pushing to the repository's remote is the node's own credential, and the setup tries it;
  - an endpoint account (`docs/cache-fallback.md`, 「The Claude Code road is a list of accounts」) is
    not offered on a node at all: its key file is a credential file, it has no token for the nodes,
    and its runs stand on this machine alone.
- **The row says what is missing,** one thing per line, and a node with anything missing is not ready:
  nothing is launched or run on it.
- **A look the node did not answer is not a thing missing:** where the command asking whether a tool stands
  could not run at all — the node's shell, the hub or a step's unit failing before it gave a code — the row
  says the tool could not be looked for, with why, rather than that it is not on the node. The node is still
  not ready until a look answers.
- **A workplace not ready is checked again by itself:** the process keeping the links (「The link」 below) starts
  a check of it as a unit of its own, `bmo-node-check-<node>`, on the server's own environment file, once the
  while the policy names (`policy/work-nodes.toml`) has passed since its last check, setup, sync or check. A check
  that finds it ready makes the next pass stand its link again, and a sync of its copy where the deployment has
  moved past it. So a check that failed on something passing — a hub that did not answer one step — does not
  keep the link down. An account's isolation node is stood up again instead (`docs/account-isolation.md`,
  「Standing it up」), and so is a person's PC (`docs/person-nodes.md`, 「The workplace on a PC」).
- **The node's copy follows the deployment.** The setup writes down the deployed commit it copied, as
  the deployment's record beside the deploy tree's link names it, and only where its copy was laid over the
  node's: a copy that failed leaves the commit the node held written, so the next sync copies again. Where the deployed commit has moved
  past it, the process keeping the links (「The link」 below) starts a sync of its own as a unit, on the
  server's own environment file, `env/agency.env`, as every unit of a link is; a sync
  stops the node's link, which that process stands again on the new code once the workplace reads ready,
  at once where the sync found it so and after the check above where it did not.

## Which nodes take jobs

- **A node takes jobs where a job can reach it and no other service holds it:** the node service names an
  SSH login to it, since every git question of a job's lifecycle runs there over SSH, and the services it
  reserves the node for, where it reserves it, include the jobs. A node reached through a hub has no SSH
  route and takes none; one reserved for another service alone takes none; an account's isolation node is no
  node the service keeps, and takes none.
- **The judgement is agency's, read off the node service's list as it stands:** `GET /nodes` answers each
  node with `takesJobs` and the sentence saying why not (`docs/api.md`, 「nodes」), so a screen offers only the
  nodes that take jobs, and a repository's placement or node checkout naming another is refused.
- **Taking jobs is not being ready:** a node that takes jobs launches nothing while its workplace is not ready
  or its link is down (「The workplace on a node」, 「When a node drops」).

## The link

- **One link per ready workplace:** a connect program of the crate workspace, under a unit of its own,
  which agency's server starts with the workplace's spec — as it starts, once a minute and whenever
  the repositories move — and stops for a workplace no longer ready. Agency's server keeps the links; a
  process standing on a copy of the data root keeps none. The node's SSH login is asked of the node
  service, which keeps the nodes (`docs/api.md`, 「The node service」), a person's PC, which has none, is
  reached through the hub the service names for it (`docs/person-nodes.md`, 「The link over jake」), and an
  account's isolation node, on this machine, by the far end started in its sandbox (`docs/account-isolation.md`,
  「The link」).
- **Its spec is the server's:** a link record of the node's under the data root, written whole
  whenever what it says moves — the login, the forwards, what is mirrored each way, how this machine's
  roots read on the node, which variables of a program go there. The link reads it again as it moves,
  and makes a new connection only where the connection itself changed.
- **It holds one SSH connection,** made the check's way, and carries over it:
  - the node's herdr session socket, forwarded to a socket of the node's here, which is where every
    herdr call of the bot's for that node goes;
  - the tool host's port, forwarded to the node's loopback, so the program's MCP tools reach the one
    host here;
  - the model gateway's port, forwarded to the node's loopback at the same port, so a program on a gateway
    road reaches the one gateway here at the address its launch names (「The model gateway on a node」 below);
  - the home bridge's socket, forwarded to a socket named for that connection under the node's data
    root, which the node's bridge points at. What a node asks of the bridge — the CLI's `bin/bmo` — is the CLI's, not
    agency's, so the bridge runs it out of the CLI's own deploy tree, `$HOME/.local/share/bmo-cli/deploy`,
    never out of agency's (`docs/agent-control.md`, 「The job's release」), and that `bmo` hands each command
    to the unit carrying it. A person's PC asks it for the verbs agency's policy names alone
    (`docs/person-nodes.md`, 「Who reaches a node」);
  - the mirror, spoken with a serve program started at the far end.
- **A program on the node calls `bmo` as a program here does.** The node holds no CLI of its own, so the far end
  writes, as it starts, a `bmo` into the workplace's own binaries — on the search path every program there runs
  with — that hands its whole call to the home bridge (`node-link home bin/bmo`). A supervisor's `bmo use`, a
  hook's status reading and every other verb so run here under the bridge's rules, and a verb the node may not
  ask for is refused by the bridge with why. A `bmo` installed into the account's own tools stands ahead of it.
- **The far end makes sure of what stands on the node** whether a link holds it or not: the agent host
  of the node's data root, which the heads there ask, and the herdr session, each started apart from
  the connection where it does not stand.
- **The far end keeps only the places a link names.** Each watch list names the places of its node, by the
  running record it pulls of each. The far end keeps the latest list of every node a session spoke for, for as
  long as it stands, and a place under the node's data root that none of them names has its program file
  removed and the program its running record names ended: its head is answered the view, and nothing starts it
  again. So a place whose account moved to another node, was unbound, or whose node's agent went away on a
  machine another node's link now holds stands no program there, and nothing of it starts in a loop. The near
  end forgets what it sent of a place its spec no longer names, so a place named again is sent whole again.
- **The mirror keeps one direction per file:**
  - from here to the node: an agent place's program, its settings, its run and the handed mark, with
    this machine's data root, deploy tree, prompt tree and home rewritten to the node's, and the folder of
    each supervisor of an account whose record names the node shared to that folder under the account's root
    there (`docs/person-nodes.md`, 「Several accounts on one node」), every file
    the program names under the data root or the prompt tree sent along — never one under the data
    root's credential directory but the call key of the gateway its programs go out on (「The model gateway on a node」 below) — and of
    the program's environment only the run's own variables, the CLI's (the account's token for the nodes
    among them), its place, its tool token, the console's public address, a supervisor's owner account and
    a gateway road's own variables: no other token of the bot's leaves this machine. The node's own search
    path and the bridge's socket are laid over it. A command whose program is `claude` runs the workplace's own
    Claude Code there, `<root>/bin/bmo-claude` in its place, with Claude Code's updater off, once the node's setup
    record names that copy standing (step 4 above); a node whose record names none yet runs the word as it is. A
    path a string names counts wherever it stands whole —
    alone, after a separator a command line or a list of paths puts before one, or between single quotes as
    a key helper's command quotes it;
  - from the node to here: the place's running record and start failure; the program's transcripts,
    under the same configuration directory here as the account's own, sent as what they grew by; and the node's load, memory, processor count and pressure, into the
    node's directory here, every few seconds;
  - a file is sent once and then as it changes, for as long as the far end watches it: a watch list the
    near end asks anew, as it does whenever a place comes or goes, sends from nothing only what it newly
    names, and a new connection starts each growing file from the length this machine already holds of it,
    where the node's file is at least that long. A node holds hundreds of megabytes of transcripts, which a
    send from nothing would put ahead of everything else the link says for many minutes;
  - the near end numbers each watch list it asks and keeps the latest of them, and every frame of what is
    watched names the list it was read under, so a frame still on its way when a list moved lands where its
    own list put it. A near end that numbers no list has its list sent from nothing whenever it changes,
    since a frame on its way could not say which numbering it was read under;
  - the far end reads the files it sends no faster than the connection carries them, so what the link says
    of its own goes ahead of the files still on their way rather than behind them: each place's running
    record and which of its processes are alive, the node's load, and on a person's PC the road home;
  - which processes of a remote place's running record are alive, written beside it, and a signal
    asked for a process on the node through the link's control socket.
- **Its state** — connected since, last heard from, the node's home, the reason it last dropped, and on a
  person's PC each port its far end could not listen on with why — is written beside the node's directory
  here. A link that drops is made again, waiting a doubling bound
  up to a minute between tries; one heard from in the last twenty seconds is up.
- **The link's log says when it comes up and when it goes down:** `link up` once the far end greets a
  connection; `link down` with how long it stood and why, when a connection that came up ends; and `link not
  made` with why, when a try ends before the far end greeted it. A link that keeps failing reads apart from one
  that keeps coming up and dropping.
- **A connection the node has not been heard on for two minutes is dropped and made again.** The far end
  speaks every few seconds while it stands, so a connection with nothing coming through it is a dead one,
  as a hub's connect program left standing after the PC's agent was replaced is; the reason it dropped says
  how long it went unheard.
- **A connection that cannot start says the program it tried:** the SSH program, the hub's connect program
  or the sandbox's first word, by the path or name it was started as, beside why it did not start.

## The model gateway on a node

- **A program on a node reaches the deployment's model gateway as a program here does,** at the address its
  launch names and with the key its settings' key helper reads (`docs/own-llm.md`, 「The gateway」, 「The
  keys」), so a supervisor or a run on a gateway road — a registration, DeepSeek, OpenRouter, Codex — answers
  from a node as it answers here.
- **The address is carried by the link,** where the gateway's address here is on this machine's loopback:
  its port on the node's loopback joined to the gateway's port here, the same number at both ends, so the
  address the launch names reads the same on the node. Each transport carries it its own way: an SSH
  forward, a port a PC's far end listens on and dials home as it dials the tool host's
  (`docs/person-nodes.md`, 「The link over jake」), and on an isolation node the network rule leaving the
  gateway's port on this machine's loopback open beside the tool host's (`docs/account-isolation.md`). A
  gateway whose address is not on this machine's loopback is reached from the node as it stands, and the
  link carries nothing for it.
- **The road's variables go with the program:** its base address, its model names, its headers, its waits
  and its tool search, which the link's environment keeps beside the run's own, and the name of the run's program, its agent, in
  the `x-bmo-run` header its requests carry. The variables a login of Claude's road would stand in — an auth token,
  an API key — never go, but for a person's own `api-key` account on that person's own node
  (`docs/supply.md`, 「What a person brings」).
- **The key goes as a file beside the program that names it,** the one file under the data root's
  credential directory a link sends for it: the call key of the account the program runs for, or the
  gateway's own key for a program of the deployment's, sent where a pushed program's settings name it for
  their key helper, written at its path under the node's data root readable by the node's user alone, as the
  run's tool token is written into the program's file there, and sent again when it changes. A node that is a
  person's is sent the call keys of the accounts its record names and never the gateway's own key
  (`docs/own-llm.md`, 「The keys」). A key is never put in a command line or an environment, and a node none of
  whose programs names it holds none.

## The logins on a node

- **Each directory and token account has one token for the work nodes:** the long-lived token the
  CLI's setup command issues for it. An endpoint account has none and is neither counted nor warned
  about here. This machine keeps it outside the repository, in the data root's own token directory,
  one file named by the account's id and readable by the bot's user alone; a token account's own token
  file is its token for the nodes as well. Where the files stand is Claude Code's declaration in
  agency's policy (`policy/`), read by every process that needs it. No token is written into the
  store, a log line, a document, a commit or a report.
- **What the CLI does with it:**
  - the setup-token command opens the subscription sign-in in a browser and prints a token to set as
    `CLAUDE_CODE_OAUTH_TOKEN`; it asks for a lifetime of a year and holds no refresh, so nothing
    renews it and no other machine's use of it signs anyone out;
  - the token is the account's that approves the sign-in in the browser, whatever configuration
    directory the command was run under;
  - it is inference-only: a run on it answers prompts, while a remote-control surface, the
    browser-driving surface and the provider's usage read refuse it, and no run on a node uses any of
    them;
  - a program handed `CLAUDE_CODE_OAUTH_TOKEN` signs in with it ahead of any login its configuration
    directory holds and writes no login there, so a node needs no login file.
- **It carries the account here too, once its login here cannot be counted on:** a directory account
  whose login here has ended, waits for a person or is near its end is handed the same token beside its
  configuration directory for its runs on this machine, and goes back to its login once a person signs
  in again (`docs/cache-fallback.md`, 「A directory account carries on by its token」).
- **The token goes with each run, not with the node.** A job's program on a node is composed here for
  the account its walk goes out on: a directory account's program is handed that account's
  configuration directory and its token for the nodes, and the link carries the program's
  `CLAUDE_CODE_*` variables to the node with it (「The link」 above). So:
  - a node registered, a workplace set up, a hub node newly installed and a herdr machine newly added
    need nothing more for Claude Code: the first job placed there carries the token;
  - an account switch — a full weekly window, the owner's choice — moves a node's next run onto the
    next account's token as it moves a run here, and the program on the node starts again on it;
  - a token made again reaches every node at its next run, since a program handed another token is
    another program;
  - an account with no token for the nodes goes out on a node with its directory alone, signed in by
    whatever login a person made in that directory there.
- **Making one is one command per account, run by the owner on this machine from any folder:**
  `bmo-agency node claude-token '<account name>'`. It runs the CLI's setup-token command under that
  account's directory, whose sign-in the owner approves in a browser signed in to that account, then
  takes the printed token pasted in, unechoed, refuses one not shaped as a Claude Code token, and
  writes it to the account's file. `--paste` takes a token already made, without the sign-in, from the
  terminal or from standard input.
- **A token that stops answering is told, once.** A run on a node that fails on its login, or a run here
  that was handed the token and fails on its login, is read as that account's token for the nodes gone —
  expired, revoked or never made — and never as the account's login here:
  - the mark is a setting holding the token file's moment as it stood, and while it stands the account
    is passed over for every run on a node, its work there going to the next account in the walk, and
    is not carried by the token here either;
  - a new token is a new file, so the mark lifts by itself, on nobody's word;
  - the owner hears of it once per mark, in a private chat: which account, and the one command that
    makes its token again;
  - a token's year is warned about well before it is out, since only a browser approval makes another:
    once as each of the leads the Claude Code provider's policy names is crossed — a month, a week and a
    day before — by the keepalive pass's login watch (`docs/cache-fallback.md`, 「A login that is about to
    end」).
- **Where it runs:** the composing, the reading of a failure and the warning are agency's server's,
  whose job runner runs every job.
- **A program's own login file is marked where the program starts:** a resident program whose command names
  the configuration file its login keeps has that file's first-run screens marked as passed and the program's
  folder trusted in it by the agent host that starts it, on whichever machine that is, its directory made where it
  is not, just before the program comes up. A road whose configuration directory is new to a node — a gateway
  road's home made after the setup — so stops at neither the first-run screens nor the folder check there, for
  every folder a program of it is started in (`docs/agent-control.md`, 「A first interactive run stops at the
  trust-this-folder check」).

## The bot's side of a remote agent

- **Every herdr call for an agent on a node goes to that node's socket:** an agent's home names the
  herdr it stands in, and standing its pane, a turn, a stop, a confirmation and the leftovers sweep
  take it from there. Pane and agent ids are the node's, so they are always read and used together
  with the node. A silent herdr of a node is the node's link, and is told as no want of room here.
- **Only a resident program stands on a node.** A structured execution — a gate, a runner,
  a co-work coordinator — always runs here, so its cancel files and process groups are this machine's
  as ever.
- **A remote place's processes are the node's.** Every operation on a place's processes — whether its
  program or its head is alive, ending the program, interrupting it — is asked of the place, which
  answers locally for a place here and through the link for a place on a node: liveness off what the
  link mirrored, an end as a signal the link carries. A link that has told nothing lately says nothing
  either way, and the program is taken as standing rather than stood up again. Nothing here signals a
  process number of another machine.
- **The program is composed here, with this machine's paths,** and every file its command names is
  checked here as for any launch; the link rewrites them as it pushes. What it names in the deploy
  tree is the node's copy of the same commit.
- **The head's loop is composed with the node's roots,** so the pane on the node starts that node's
  pane head, which asks the node's agent host, on the node's own environment.
- **The job runner reaches the checkout through the node:** every git question of the job's lifecycle
  — the branch prepared, what is open, what the job committed, parking — runs there over one shared
  connection, main is the remote's, brought up before the job's branch is made from it. The job unit reads the nodes and how SSH reaches one through agency, which hands on the node service's reads
  (`docs/api.md`, 「repositories, prefix-checks and room-holdings」).
- **The records read here as they always did,** off the mirrored files: the turn's answer and
  activity, the session collection, the run log, the token usage, a job's quiet and stalled watch.
  The link pulls them from every account's records directory and, for a place whose program keeps its
  records under a home of its own — a gateway road's — from that home as well, so a turn on a registered or
  member model is read off a node as one on a Claude account is.

## The rotation

- **A launch goes round the machines.** Of a base's free checkouts, a support checkout already on the
  item's branch keeps the item on its machine, waiting where that machine has no room; a launch naming
  one checkout goes to that checkout's machine; any other takes the machine after the one the base's
  last launch went to, in the node list's order, passing over every machine with no room or no free
  checkout of the base, and takes the free checkouts there, the base's own first. Where no machine has
  room the launch waits, every machine's want of room said.
- **Heavy runs go round the same way,** in a rotation of their own over this machine and every ready
  work node that takes runs.
- **Where each rotation stands is the store's,** a rotation row per rotation — one for each base's
  launches, one for the heavy runs — naming the machine it last went to, so a restart carries on from
  there; and each machine's count of jobs and runs it was given is kept beside it.
- **Both are shown:** the console's live board in the machine popover, each machine with how many jobs
  and runs it was given and its room, and where the next heavy run goes, wherever any node takes work.

## When a node drops

- **The program keeps working:** it stands in the node's herdr session, which no connection holds.
- **A turn waits for the link** however long the node's herdr does not answer, as a turn here waits
  for a silent herdr; the mirror catches the transcript up when the link is back, and the turn reads on
  from where it stood. It ends on the link's answer — its own end, its pane gone — or on a stop.
- **What the program says through the home bridge while the link is down is lost.** Its hooks and its status
  line are calls the bridge runs here, not files the mirror carries, and nothing sends a lost call again. So the
  turn's end is read from its transcript, which the mirror catches up, never from a hook alone, and the status
  line's reading after the turn is waited for within its bound only (`docs/agent-control.md`, 「What a resident
  turn reads」): a turn whose program ended it while its node's link was being made again ends answered, with
  no price.
- **A program on a node that does not take its turn is not waited on for ever.** A program can stand on a
  node and never come to take its words — one with no road to its model, one stuck on a screen it cannot
  pass — while the link answers for it as standing. So the time a turn waits for its program to come up,
  come to rest and take its words is counted while the node's link is heard, and counted afresh after any
  while it is not:
  - past a minute, the turn's `waiting` names it, `node-program` with the node and the reason (`docs/api.md`,
    「sessions」), and its run log and agency's log say so;
  - past five minutes, the program is ended, so the next turn stands a new one, and the turn ends
    `unsubmitted` with the reason, which its caller reads as the turn's failure; a program that never came up
    at all fails the turn's launch with the same reason.
  Once the words are taken the turn is the program's, and no clock of this kind reads it again. The two
  bounds are the resident turn's policy.
- **A program on a node whose far end could not listen on a port it reaches home through is not waited on
  either.** Such a program can never reach the tool host, the gateway or the bridge, so while the link's state
  names a port its far end could not listen on (`docs/person-nodes.md`, 「The link over jake」), the turn's
  `waiting` names it at once, `node-program` with the node and the port, and once that has held for half a minute
  the program is ended and the turn ends `unsubmitted` with the same reason, as it ends past the bound above. The
  half minute is the resident turn's policy.
- **A call to a node's herdr is answered within half a minute:** one unanswered that long is answered as herdr
  silent, so the waits above go on reading their bounds rather than standing on a socket nothing answers through,
  and a turn goes on waiting for the link as it waits for a silent herdr. The ceiling is the resident turn's
  policy.
- **Nothing new goes to a node whose link is down:** neither a launch nor a heavy run.

## In herdr

- **A work node is a machine of herdr's own as well:** its setup saves it as a herdr machine, so
  herdr's sidebar shows the node's own session, where the agents placed there stand.
- **Every node has a workspace of its own in the bot's herdr session,** standing after every other
  workspace, in the list's order, so the nodes read as one block at the bottom of herdr's left side.
- The workspace carries the token naming the node and is labelled with the node's label and its state
  word, its name where it has no label. Agency's server stands the workspaces after every reading of
  the node service's list (`docs/api.md`, 「The node service」): it makes a missing one, renames one
  whose state changed, moves the block back to the bottom and closes the workspace of a node that is
  gone, and publishes `nodes-moved` where the list's shape moved since the reading before.
- The workspace's pane runs the deploy tree's node-pane command, which shows the node as the node
  service lists it and keeps it current: its state, and its kind, `ssh` or `jake` for a node reached
  through a hub. The pane starts in the data root, as every pane agency stands does (`docs/agent-control.md`,
  「Where a pane stands」). The pane is handed the node service's address and token as the server holds them, beside
  the variables every pane is given, since it reads the list itself. A deployment retires the verb: once the
  deploy tree's link names another release of the same tree than the one the verb runs from, it ends between
  logins and its loop starts the link's, so no node's pane keeps an old release from the prune; a login under
  way holds it until the login ends. A pane that is named no node
  service, or cannot read the list, says so and is started again by its loop; only a node the list no
  longer holds ends it, and with it the workspace. Agency tells a hub node apart by that kind alone and keeps no record of the hubs
  itself. On an SSH node Enter logs in, with the login the node service names to it,
  interactively — a person is at the pane, so a password is asked for there if no key is taken — and
  the pane comes back to the state when the login ends. Ctrl-C ends a login — a password prompt
  included — and never the pane: the loop and the pane verb let it pass, so only the login it was meant
  for stops.
- A hub node's pane shows its state alone, since the hub offers no interactive shell.
- A browser on a hub node is driven through the remote browser tool (`remote-browser.md`).
