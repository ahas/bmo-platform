# A person's PC

A person's PC is the node their BMO runs on. They turn the PC on, jake attaches it to the hub, and agency stands
their account's sessions there, through jake alone, with no SSH. The sessions run on the PC; what they run on — the
providers and their accounts, the model gateway, the brain, the tool host — is the deployment's. It is the shape of a
desktop coding agent whose models and tools are served from elsewhere.

## Which node is a person's

- **A person's own node is the machine agency's `own` record names for their account:** the hub node of the
  machine their account was bound to when its invitation was proven, `u<user>` (the account service's
  `docs/accounts.md`, 「Invitations and a person's machine」). An account has one own node, and may be
  **designated** onto further nodes besides it: an account holds one record a node it stands on, many accounts
  stand on one node and one account on many, each set and taken away on its own by the service that binds the
  account or by the administrator (`docs/api.md`, 「person-nodes」), and each names a node the node service keeps
  as a hub node. The node service lists them as any hub nodes (`docs/work-nodes.md`, 「In herdr」). A node one
  account holds as its own belongs to it alone: no other account, the administrator's call included, is
  designated onto it or takes it as its own. A node that is no one's own is the platform's, and an account
  comes onto it only by designation, shared records of those that declare it working under roots of their own
  (「Several accounts on one node」).
- **A node an account stands on lends the account its place and nothing else:** it does not own the account's
  agents nor judge what they run. Which node a run of an agent stands on is the agent's home
  (`docs/role-agents.md`, 「What an agent is」), chosen from the nodes of its account — its own machine and its
  designations — and, for a home the platform's call writes, a platform node; where an agent names no home and
  the run is not a job's at its worker's, the account's own node places it, its newest designation where it has
  no own node.
- **The administrator's account needs no records: its own node is this machine.** Its runs are the owner's own
  and stand here whatever records name it, and the administrator is the one account that places agents on
  platform nodes. Any other account with no record waits on the node its invitation binds it to, its isolation
  node standing in place until then (`docs/account-isolation.md`). So every account but the administrator's
  comes to the platform with a node of its own.
- **An account's runs go there and nowhere else:**
  - its supervisors' sessions: a supervisor of the account stands on the node as a worker of a node does, its
    head, its room panes and every turn of it in the node's herdr session;
  - its role agents' runs: a run whose slot belongs to the account goes the place-based road whatever its
    provider, its place opened on the node, its folder read as the node reads this machine's roots. On Claude
    Code the program in that place is Claude Code on the run's line, on the login its supply source takes
    (`docs/supply.md`, 「Choosing a source as a run starts」): the person's own login signed in on the node, their
    own `api-key` account, or, on a node the account shares with no other, the token for the nodes of the first
    lendable login the walk offers among those that run on a node (`docs/work-nodes.md`, 「The logins on a node」). On another provider it is that road's own
    program, on the account's call key;
  - the link mirrors each of these places as it mirrors a worker's: its program and settings sent, its running
    record and its transcripts brought back;
  - the folder each of these programs works in, its supervisor's own or its run's, is named in the spec, and the
    far end makes it where it is not, since nothing of it need stand on the PC before;
  - the status line and the hooks of each of these programs answer at home: the place's settings name
    `node-link home bin/bmo agent-status` in place of this machine's verbs binary, which hands the call to the home
    bridge to run with the CLI's `bmo` here, so the readings the door waits on before it puts a prompt — the status
    line drawn, the hooks called — are written into the place here.
- **While the node is not attached, its runs wait:** a turn of the account asks again every few seconds whether the
  node's link has been heard from, and starts once it has; a head or a room pane of a supervisor of the account is
  not stood until then. Nothing of it runs as this machine's user or on another account's node. A run of no
  account, or of the administrator's account, is placed as before.
- **A waiting turn says so, can be stopped and does not wait for ever:**
  - its `waiting` names the wait, `node-link` with the node and the reason (`docs/api.md`, 「sessions」), published
    as `turn.changed` as it begins and as it clears, and its run log says so;
  - a stop standing over its run — the interrupt (`docs/api.md`, 「sessions」) or `bmo-agency run stop`
    (`docs/agent-control.md`, 「Stopping one from a shell」) — is read at each ask, and the turn ends `stopped`;
  - one that has waited past the bound the policy names (`policy/person-nodes.toml`) ends failed, its reason
    naming the node and how long its link went unheard, so a turn its caller has left behind ends too;
  - while it waits the node is kept beside its kept record, so a restart neither waits on it nor loses it silently: the
    deployment's quiet does not count it, and the next server ends it failed with why (`docs/sessions.md`, 「A
    restart hands the session over」).
- **An account whose record moves leaves nothing standing where it was.** When a supervisor's place is marked for
  another node, or for this machine, its program is let go first while the old mark still sends the end through
  the old node's link, and the next turn or standing pass writes it again where the place now stands; a turn going
  in the place keeps its program. The old node's far end, as its link next names its places, lets go of every place
  no link names any more (`docs/work-nodes.md`, 「The link」), so a node whose link was down at the move, or whose
  agent went away for good on a PC another node's link holds, stands nothing of the account once a link speaks to
  its far end again.
- **A waiting turn is placed by the record standing now:** before each ask it reads its placement again, so a
  record taken away or set to another node while it waits places it on the node the account's present record
  gives, the isolation node where none stands, and it waits on that node instead, or starts at once where that
  node is heard. A role run's place is marked again with that node. A turn whose supervisor was removed while it
  waits waits no more and ends with it (`docs/supervisors.md`, 「Adding and removing a supervisor」).

## Several accounts on one node

- **The administrator may bind several accounts to one node, declaring it shared for each.** A record carries
  `shared`, false unless the binding says so (`docs/api.md`, 「person-nodes」, and `bmo-agency node person
  --shared`). A record naming a node another account's record names is refused, `node-taken`, unless both
  records are shared, so no account lands on another's PC by a slip, and an account bound alone keeps its PC to
  itself.
- **An account whose record is shared has a workplace root of its own on that node:** `accounts/<account>` under
  the node's workplace root, `<node root>/accounts/<account>`. What agency names for the account's programs
  stands under it, each at the path it holds under this machine's data root:
  - each supervisor's own folder, `<account root>/supervisor-homes/<supervisor>`, in place of the one under the
    node's data root; the link spec reads the supervisor's folder here as that folder, so its program, the
    records the link brings back and the folder the far end makes all follow (`docs/work-nodes.md`, 「The link」);
  - a role run's folder is its caller's — a job's checkout, a worker ask's folder — and reads on the node
    through the roots as every other path, since agency names none of it.
- **An account whose record is not shared keeps its folders where they stand,** under the node's data root, so
  no path moves for a PC with one owner.
- **The rest of the node is one for all its accounts:** its data root with every place, its herdr session, the
  agent host, the CLI's records under the node's home, and the user every program runs as.
- **The node tool runs an owner's command inside that owner's root.** The access read answers `root`, the node
  path of the caller's account's root where its record names the node shared, null otherwise (`docs/api.md`,
  「person-nodes」). Confining the command to it is the node service's tool's.
- **No login lent to an account is placed on a node it shares.** Since every account's programs there read every
  other's, a platform login lent to a sharing account would be the others' too; a run of such an account on Claude
  Code goes out on its own login, its organisation's or none, and a program already standing there on a lent login
  is let go between its turns (`docs/supply.md`, 「The lendable accounts」).
- **One account's programs still read every other account's root on the node.** No launch wrap hides them, since
  one that hid `<node root>/accounts` alone would not confine a program running as the node's one user:
  - every program of that user that stands outside the wrap — the herdr session, the agent host, the far end —
    shows the whole filesystem through its `/proc/<pid>/root` unless the wrapped program has a process namespace
    and a `/proc` of its own;
  - the herdr session's socket and the agent host start programs outside any wrap for whoever asks them;
  - the places under the node's shared data root and the CLI's records under its home hold every account's
    programs, transcripts and settings side by side.
  So hiding needs all of these closed at once — a process namespace, the herdr and agent host sockets out of
  reach, places and records of an account's own — and a setup that installs bubblewrap on the PC and checks its
  user namespaces; it is left (「What stands and what is left」).

## A supervisor's tools on a node

A supervisor's core tools at home are programs its own program starts beside it with this machine's addresses and
secrets: the job service's plugin and agency's own server, holding the job service's and agency's tokens, the
artifact service's plugin, holding its token, and the explorer service's plugin, reading this machine's data root
(`docs/launch-lines.md`, 「What a line's program is allowed, by road」). None of those is ever sent to a node
(「The link over jake」), so on a node they would start with nothing to reach and nothing to present. A supervisor
whose program stands on a node is handed the same work as the tool host's entries instead, served at home and
reached the way the node already reaches the tool host: over the link, behind the session's own token.

- **Which supervisor:** one whose account's runs stand on a node — its person's PC, a node the administrator bound
  it to, or its isolation node (「Which node is a person's」). A supervisor of no account, and one of the
  administrator's account that no record places on a node, keep the servers as they stand.
- **What stands in, per capability the chat line holds outside the tool host** (`policy/run-lines.toml`, each
  `held` entry's `hosted`; `docs/supervisors.md`, 「What each capability stands for」):

  | Capability | Left out of the configuration | Handed in its place | The record holds |
  |---|---|---|---|
  | `jobs.file` | `job`, `agency` | the tool host's `agency` namespace, agency's own job tools — `file_item`, `stop_job`, `review_job` (`docs/tools.md`, 「The job tools on a node」) — with the job service's queue readings `bmo/todo_list` and `bmo/todo_show` shown by name | `jobs.file` |
  | `web.pages` | `explorer` | the tool host's `web` namespace, the explorer service's `web_search` and `web_fetch` | `web.read` |
  | `documents.file` | `artifact` | the tool host's `artifact` namespace, the artifact service's own entries of its harness tools (the artifact service's `docs/api.md`) | `documents.file` |

- **The names stay the program's own:** the `agency` and `artifact` namespaces stand under the names the
  servers they replace stand under at home, and their entries keep the servers' tool names, so a supervisor calls
  `file_item` or `artifact_publish` by the same name wherever its program stands.
- **Each call acts for the session's owner:** the tool host runs it at home under the session's record, checked as
  any hosted call is, against the record's namespaces and capabilities; agency's job tools act for the supervisor
  the record is held by, and the artifact service's for the account the record names, `ownerAccount` (`docs/tools.md`,
  「The owner and the runner a record names」). The node holds no address, token or file of any of them.
- **A tool limit closes each the same way:** a limit leaving a capability out leaves out both its servers at home
  and what stands in for them on a node (`docs/supervisors.md`, 「Its settings」).
- **What has no stand-in is not handed:** the job plugin's leasing of an item from a queue and agency's
  `assign_worker`, `release_worker` and `run_job`, which run an item a supervisor holds itself, stand on no node.
  A supervisor on a node files an item, and the job service and the owner's approval take it from there.

## Personal tools on a PC

A person's personal tools — tools of their own, which their BMO writes for them — are built, kept and run on their
PC; the tool host at home keeps a copy of each one's description alone and relays each call (the tool host's
`docs/mcp-host.md`, 「Personal tools」).

- **They stand under the workplace's data root,** `<node root>/data/personal-tools/owners/<account>/`, in the layout
  the tool host keeps an owner's registry in (the tool host's `docs/registry.md`, 「Personal tools」): a pointer per
  tool, `registry/<name>.json`, naming the version it points at; each placed version in `tools/<name>/<version>/`;
  and beside them the data folder each tool writes, `data/<name>/`, made by the runner the first time it runs the
  tool, readable by the PC's user alone.
- **The runner is `node-link runner`,** the far end's own binary, built on the PC with it: started for one call, it
  speaks the tool host's runner passage on its standard input and output (the tool host's `docs/api.md`, 「The
  runner」), runs the version of the tool the call names, which stands there, under the run words it is given, and
  answers each line the tool writes on its errors and how it ended. It runs the tools of the one account it is
  started for, and refuses a call of another, a name that is no folder's, a version not placed and a run command
  naming nothing.
- **It runs a tool confined by bubblewrap, or not at all:**
  - every namespace the tool's own, the network among them, so it reaches no address but through the call's proxy
    where the call names addresses (「The network and secrets of a call」); ended with the runner; in a session of
    its own;
  - a clean environment: the search path of the machine's own programs, its data folder as its home, a scratch
    folder of its own, and nothing of the runner's, so no token or key reaches it;
  - the machine's `/usr`, its links or folders `/bin`, `/sbin` and `/lib*`, and the files the dynamic loader and the
    clock read, read alone; a process table and devices of its own;
  - its version folder read alone at `/tool`, where it starts, and its data folder written at `/data`; nothing else
    of the PC — the home, the CLI's records, the workplace's data root, the link's tool token and the gateway's key
    files, another tool's folders — stands inside;
  - a call-back socket into the host and a helper are given no call yet.
  Where bubblewrap is not on the machine, the runner refuses the call saying so.
- **A call reaches the PC over a passage of its own:**
  - the near end listens on the link's runner socket, `runner.sock` in the node's folder here, readable by this
    machine's user alone, where the link's spec names one, as it does for every PC;
  - each connection there is one call: its first line names the account, and the near end refuses it where the spec
    does not name that account as the one the PC runs personal tools for;
  - otherwise it opens a new byte stream to the far end's mirror port, as it dials a connection home (「The link over
    jake」), names the passage on it with a `run` frame for that account, and joins the two;
  - the far end starts a runner of its own binary for that account in a process group of its own and joins the
    stream to it;
  - the caller's connection closing ends the stream, the stream ending ends the runner's whole group, and the runner
    whose input closes before the tool has answered ends the tool, so a call whose caller is gone ends on the PC.
- **A PC runs the personal tools of one account alone:** the spec names the account whose record names the PC, where
  it is the only one and its record is not shared. A PC several accounts share runs none, since no account's tool is
  yet kept from another's there (「Several accounts on one node」), and the records of its accounts name no runner.
- **The administrator's account has no PC:** its personal tools stand under this machine's data root,
  `personal-tools/`, and run under the same runner and confinement as this machine's user, its records naming the
  runner of this deployment's node link as a command (`docs/tools.md`, 「The owner and the runner a record names」).
  An account on its isolation node has no runner yet.
- **The setup looks for bubblewrap on a PC and tries it:** `bwrap` looked for, installed with `--install` through the
  Linux shell's package manager where `sudo` asks no password, and tried by making every namespace over the machine's
  filesystem read alone. What it found is said beside the setup's answer, as `personal tools: …`, and kept in the
  node's setup record as `confinement`, `ready` or why not. It holds nothing else of the workplace back: the PC's
  BMO runs as before, and its personal tools are refused until bubblewrap confines there.

## A local AI on a PC

A person's own local AI (`docs/supply.md`, 「A person's local AI」) is a server their PC reaches — on its loopback,
or on the network it stands on — and the gateway here asks it through the PC's link, so an address that reads
`localhost` is the PC's own and never this machine's.

- **The link carries a local AI's address from here to the PC:**
  - the spec names, for each local AI that is on of the account the PC runs personal tools for (「Personal tools on a
    PC」), a port of this machine's loopback and the host and port the address names, as the PC reaches them
    (`docs/own-llm.md`, 「A person's local AI」);
  - the near end listens on each such port at this machine's loopback, reading the spec again every few seconds so
    one added or taken away is listened on or let go with no new connection;
  - each connection taken there opens a new byte stream to the far end's mirror port, as a personal tool's call does,
    and names it with a `reach` frame naming the host and port;
  - the far end connects to that host and port from the PC, says whether it reached it, and joins the stream to the
    connection; one it could not reach closes the stream, so the gateway's request fails at once.
- **A test asks the same way, once:** a test of a person's local AI (`docs/supply.md`, 「Testing a source」) asks on
  the link's runner socket with a first line naming the account and the host and port to reach; the near end refuses
  it for any account but the one the spec names, as it refuses a call, opens the stream as above and answers the
  caller one line, whether the PC reached the address, before the two are joined. The test then speaks its one
  request over it.
- **A PC several accounts share, or none owns, carries none:** its spec names no account, so no local AI of anyone's
  is listened for, and a test on its runner socket is refused.

## The network and secrets of a call

A personal tool asks for the network and secrets in its manifest's `[permissions]` (the tool host's `docs/api.md`,
「`[permissions]`」): the host names it may reach, and the names of the secrets of its owner's it is handed. It is given
them for one call at a time, where the delegation chain allows them for the person, and nothing else past its folders.

- **The chain decides per person, at every turn:** agency resolves `personal-apps.network` and `personal-apps.secrets`
  for the person a record acts for and writes them into the record's withheld-apps file as `personal` (`docs/tools.md`,
  「The apps a person reaches」); the tool host refuses a call of a tool asking for what the file does not allow,
  saying the organisation does not allow it. A flag that does not resolve allows nothing. The administrator's records
  name no such file and are held to neither.
- **An address is a host name, never an IP address:** `api.example.com` on ports 80 and 443, `api.example.com:8443`
  on that port alone, `*.example.com` every name under `example.com` — not `example.com` itself — and nothing else:
  no other wildcard, no address written whole and no `localhost`.
- **The confinement keeps a network of its own,** whose one road out is the call's proxy:
  - the runner serves a proxy for the call, a unix socket in a folder of the call's own under the PC's temporary
    folder, readable by its user alone, taken away with the folder as the tool ends, before the call is answered; a
    folder a runner ended early left behind is taken away by a later call once it is a day old;
  - the socket is bound into the confinement, and the runner's own binary with it, read alone, which runs as the
    confinement's first program: it listens on the confinement's loopback at `127.0.0.1:3128`, joins each connection
    there to the socket, starts the tool and ends as the tool ends, with its code;
  - the tool finds the proxy in `HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY`, in both cases, and reads the machine's
    certificates, `/etc/ssl`, `/etc/ca-certificates` and `/etc/pki` where they stand, read alone;
  - the confinement holds no interface but its own loopback and no resolver, so a raw socket, an address written
    whole and a name resolved by the tool itself reach nothing; a tool asking for no address has no proxy and no way
    out at all.
- **The proxy reaches what the manifest lists and nothing else:** a `CONNECT` to a host and port tunnels there, and a
  plain HTTP request naming its address whole is sent on and its answer carried back. The host must be a name the
  manifest admits on that port; the proxy resolves it itself and reaches only the addresses it found outside the PC —
  a loopback, unspecified, link-local, multicast or broadcast address is never reached, whatever a name resolves to.
  Anything else is answered 403 saying why: an address written whole, a name or a port not listed, a name resolving
  to the PC alone.
- **A secret is read at home and lives in the confinement's memory for the call alone:**
  - the tool host's relay reads each name from the record's secret space — the person's space of the console's secret
    store, `env/users/<account>/` under the data root; `env/` itself for the administrator; and the unit's space
    `env/units/<unit>/` for a caller whose agent row is filed under an organisation unit, whatever account it
    otherwise acts for (`docs/tools.md`, 「The owner and the runner a record names」) — and refuses the call naming
    a secret the space holds no file of;
  - the values travel on the call's first line alone, in base64, and are never written on the PC: the runner hands
    each to bubblewrap through a pipe, which copies it into a file of the confinement's own memory, `/secrets/<name>`,
    read alone, the folder named in `BMO_SECRETS`, gone with the confinement as the call ends;
  - no value stands on a command line, in an environment, in the runner's refusals or in a log, and every value of at
    least four characters, each of its lines and each `KEY=VALUE` line's value is masked as `[secret <name>]` in
    what the tool writes on its output and its errors, so the session never reads one back. A value written in
    another encoding is not recognised.

## Personal apps

A personal app is a personal tool a person's BMO makes for them on their PC: its source written there by a run of
the worker role, built there by the runner, kept there in versions, and its description put at home, so the person's
sessions reach it as `my/<entry>` from their next turn.

- **Its source is one folder of the person's 'my apps',** `<node root>/data/my-apps/<name>/`, beside the
  personal tools' folder, one local git repository an app: the manifest `tool.toml`, its schemas, its code, its
  manuals and its tests, written and committed by the worker run its BMO asks with the app's name (`docs/tools.md`,
  「The worker ask」). The administrator's apps stand in the data root's own `my-apps/` here. The procedure the run
  follows is `make-app` among the chat's procedures.
- **The runner keeps an app's versions** as well as running them: beside a call, the passage's first line may ask it
  one of six things, `{"owner": <account>, "<what>": {…}}`, each answered by one line:

| What is asked | Its fields | Answered with | What the runner does |
|---|---|---|---|
| `place` | `tool`; for an MCP server's folder `network` and `secrets` (「MCP servers」) | `{"t": "placed", "version", "files"}`, or `{"t": "wants", "network", "secrets"}` | builds the app's folder into a new version, pointed at by nothing yet, and answers its description; for an MCP server whose listing needs addresses or secrets the ask does not carry, places nothing and answers what it needs |
| `describe` | `tool`, `version` | `{"t": "described", "version", "files"}` | answers the description of a version standing |
| `point` | `tool`, `version` | `{"t": "pointed", "version", "kept"}` | moves the tool's pointer onto the version and keeps the newest versions older than it |
| `versions` | none | `{"t": "versions", "tools"}` | answers each tool's version pointed at, `current`, and those kept, `kept`, newest first |
| `take` | `tool` | `{"t": "taken"}` | takes the tool's pointer and every version away |
| `file` | `tool`, `version`, `path` | `{"t": "file", "bytes"}` | answers one file of a version's `screen/` folder by its path below it, its bytes in base64 |

  A `said` line carries each line a build, or an MCP server's listing, writes on its errors before the answer, its
  secrets masked, and a `refused` line, why the runner did nothing, as on a call's passage. `files` is the version's
  description by path: its manifest and the schemas and manuals the manifest names, as text.
- **A place builds confined, as a call runs:** the app's folder read alone at `/src`, where the build starts, and the
  new version's folder written at `/out`, named by `BMO_TOOL_OUT`, with the machine's own `/usr`, no network and a
  clean environment, by `sh` running the manifest's `build`; the manifest and the files it names are copied beside
  what the build wrote. With no `build`, the folder is copied whole, its `.git` left out. A manifest whose tool's
  name is not its folder's, a build that fails and a version that holds no manifest are refused, and the version
  staged is taken away. A version is named by the moment it was placed and a random suffix, as the tool host names
  one.
- **Each tool keeps three versions older than the one it points at,** for going back; a point takes the others away.
  A version a call started in stays while it is among those, so a call goes on in the version it started in
  through an update. A take leaves the app's source and its data folder standing, so the app can be put again as it
  was.
- **An app is put, sent back and removed through agency's server** (`docs/api.md`, 「personal-apps」), which asks
  the runner the account's records name (「Personal tools on a PC」) and the tool host:
  - **a put** asks the runner to place the app, puts the description it answers at home under the same version with
    `bmo-pocket put --owner` (the tool host's `docs/registry.md`, 「Put a personal tool」), then asks the runner to
    point at it. A refusal at home leaves the PC's pointer as it stood; the version placed and never pointed at is
    taken away by the next point. A place answered `wants` is asked again carrying what it names, judged and read as
    「MCP servers」 says; an MCP server of the person's own is put at home under its own key (「MCP servers」);
  - **going back** asks the runner which versions it keeps, the newest older than the current where none is named,
    its description, puts that at home under its version, and points the runner at it;
  - **a removal** takes the tool out of the account's registry at home first, so no session reaches it from its
    next call, and then asks the runner to take it; a PC that cannot be reached keeps its versions, which no call
    reaches any more;
  - **a listing** reads the account's registry at home and the versions the runner keeps, the latter left out where
    the PC cannot be reached.
- **An app may carry a screen of its own** (「Personal screens」), built in the same place.
- **Who may make one is the delegation chain's:** `personal-apps.make`, a flag narrowing down the chain, on unless a
  link turns it off, decides whether a person may put an app; `personal-apps.network` and `personal-apps.secrets`,
  flags narrowing down the chain and off until the platform turns them on for a company by a rule at its link, are
  the ceiling of what an app may ask for in its manifest's `[permissions]`. A put asking for what its ceiling leaves
  off is refused saying the organisation does not allow it; a call is held to the ceiling as it stands at the
  session's turn (「The network and secrets of a call」). Agency declares the three with its other kinds
  (`docs/prompt-layers.md`, 「The settings agency declares」); the administrator's account is held to none of them.

## Personal screens

A personal app may carry a screen: a console bundle of its own, built on the person's PC on the same release of
bmo-ui the console runs, which the console attaches for that person alone (the tool host's `docs/mcp-host.md`,
「Personal screens」; bmo-ui's `docs/console-bundles.md`, 「A bundle that differs per reader」).

- **The screen kit stands in the workplace:** `screen-kit/` of agency's tree, a Bun project of its own whose
  manifest vendors the release of bmo-ui bmo-chat's shared versions pin, as the console does, beside Vite and its
  Vue plugin, installed wherever the tree is (「The workplace on a PC」; `docs/work-nodes.md`, 「The workplace on a
  node」). Its build, `build.mjs`, builds the app's `screen/` with bmo-ui's bundle build into the new version's
  `screen/`: the bundle's name `my-` and the app's name with each `_` written `-`, its words `screen/words.json`
  of the app, its routes the default export of `screen/entry.ts`, which stand under `/my/` and the same.
- **A build sees the kit:** where the runner is told a kit, a place's confined build sees it read alone at `/kit`,
  and the JavaScript runtime the runner itself finds on its path, read alone at `/runtime/bun`, with `/runtime`
  first on its path, so an app's build runs `bun /kit/build.mjs` with no network and nothing of the PC's beside
  them. The far end tells its runners the kit of the workplace's tree; the administrator's runner command names the
  deploy tree's.
- **The runner writes the screen's index as it places a version** whose manifest names a `[screen]` (the tool host's
  `docs/api.md`, 「`[screen]`」): `screen/files.json`, every other file of the version's `screen/` by its path
  below it with the SHA-256 of its bytes, written over whatever the build wrote there. A version whose `screen/`
  holds no `module.js`, `layer.json` or `words.json` is refused and taken away. The description it answers carries
  the screen's words, layer and index beside the manifest's files, and the put places them at home.
- **A file is read for the tool host alone,** with the `file` ask: a path inside the version's `screen/` folder,
  never one climbing out of it, answered in base64; the tool host checks it against the index at home before it
  answers it on.
- **The screen reads and writes through its app's tool alone:** no server stands behind it, so what it does is a
  call of the app's own entry, under a session acting for the person, through the same records, checks and
  confinement as their BMO's call.
- **A console reads a person's screens under a session agency issues for them,** `POST
  /personal-apps/{ownerAccount}/screen-session` (`docs/api.md`, 「personal-apps」): short-lived, naming the namespace
  `my` alone, no capability and no grant, the account and its runner, and the file of the apps the person does not
  reach, so the session reaches that person's personal tools and nothing else.
- **A PC switched off** leaves the screen's listing standing from what the tool host holds, saying the runner was
  not reached, and its files unread, so the console says the screen cannot be opened.

## Shared apps

A person shares an item they made with people of their own company: its source is copied into the company's
catalogue at home (the tool host's `docs/catalogue.md`), and each receiver holds a copy of their own. Nothing of it
reaches another company.

- **An item is one of three kinds,** which the catalogue reads off its source as it is shared (the tool host's
  `docs/catalogue.md`, 「Kinds」), and every rule of this section holds for each unless it says otherwise:
  - **an app**, a personal app (「Personal apps」): each receiver's PC builds and runs a copy of its own, whose data
    stays on that PC;
  - **a skill** (「Skills at home」): instructions an agent's program reads, kept at home, so no PC is involved in
    sharing, installing, moving, dropping or adopting one;
  - **an MCP server** (「MCP servers」): a personal app of its own shape, shared, received, placed and put as an
    app is.
- **An item's text is in the maker's language, and in others where somebody wrote them** (the tool host's
  `docs/catalogue.md`, 「Text in several languages」): a share names the maker's language as the share request
  names it, else `en`, and nothing translates at share and nothing fills an item shared before. The maker, the
  owner or a head of the maker's home unit asks for the other languages to be filled: each of `ko`, `en`, `ja` and
  `zh-CN` holding no title is translated from the maker's title and description by the translator role
  (`docs/launch-lines.md`, 「Translate runner」), as a document's part is, and written with `catalogue text
  --machine`. The maker or the owner sets one locale's text, which takes its machine mark away, or clears it.

- **The runner keeps each version's source, and receives a shared one,** beside its other keepings:

| What is asked | Its fields | Answered with | What the runner does |
|---|---|---|---|
| `source` | `tool`, `version` | `{"t": "source", "version", "files"}` | answers the source the version was placed from, every file of the app's folder as it then stood, its `.git` left out, as text |
| `receive` | `tool`, `files`, `hash`, `app` | `{"t": "received"}` | writes a shared app's source into the app's folder of 'my apps', where its hash is the one given |
| `adopt` | `tool` | `{"t": "adopted"}` | makes a received app the person's own: it is no shared app's copy any more |

  - **A place keeps the source it built from** inside the new version, so a version shared later is shared as it was
    built, whatever the folder holds since. The source is what the app's own git repository keeps or would keep —
    its tracked files and the untracked ones its ignore rules let in — so a test run's caches stay out; a folder
    that is no repository of its own gives every file. A source holding a file that is not UTF-8 text, or more than
    the catalogue takes, places all the same and keeps no source, and such a version is not shared.
  - **A receive checks the hash before it writes anything:** the files' hash, reckoned as the catalogue reckons it
    (the tool host's `docs/catalogue.md`, 「The source hash」), must be the one given, else nothing is written. A
    folder of the name that stands already and is no received copy of the same app is refused, so a receiver's own
    app is never written over; a received copy is replaced whole, the app's data folder left standing, and an empty
    folder, which holds no app, is written over. The folder is marked with the app it is a copy of.
  - **An adoption** takes the mark away, so the folder is the person's own app from then.
- **Who may share, with whom, and who may install, is the delegation chain's,** three kinds agency declares with the
  others (`docs/prompt-layers.md`, 「The settings agency declares」):
  - `personal-apps.share-scope`, the targets a person may share with, keys out of `team` (their home unit and every
    unit under it), `heads` (the heads of the units on their line) and `company` (anyone of the company), narrowing
    down the chain, `team` and `heads` unless a link says otherwise;
  - `personal-apps.share-review`, whether a head of the maker's home unit must approve a shared version before
    anybody installs it, a flag the nearest link decides, off until a company turns it on;
  - `personal-apps.install`, whether a person may install a received app, a flag narrowing down the chain, on.
  A person who may not make personal apps (「Personal apps」) shares none.
- **A share** names the app and its targets: a person, a group of the chain, a unit, or the words `team`, `heads`
  and `company` (the tool host's `docs/catalogue.md`, 「Targets」). Each target is judged against the maker's ceiling:
  `team` and a unit at or under their home unit need `team`; `heads`, and a person heading their home unit, need
  `heads`; a person of their home unit needs `team`; any other person or unit of the company, a group by its
  widest member, and `company` need `company`. A person, a unit or a group of another company is refused, whatever
  the ceiling. The version shared is the one the person's PC points at, its source read from the runner and put in
  the catalogue with `bmo-pocket catalogue share`, naming the maker's language, which records its hash and kind and
  declares the app on the chain, neither offered nor on; a skill's version is its own at home, read from there
  (「Skills at home」); agency then offers it to the maker's company alone, the platform's rule naming the company written
  as the platform's administrator, the platform link's one writer, so nobody of another company is offered it. The
  review is asked once it is kept.
- **The review report** is written by an agent of the `app-reviewer` role (`docs/agent-roles.md`), a run apart of
  one turn on the role's road, the way a translation goes out (`docs/launch-lines.md`, 「Translate runner」): its
  input's first line names the item's kind, then the permissions the version asks for, then every file of the source
  under a line naming its path — for a skill its `SKILL.md` and its reference files, for an MCP server its
  `mcp.toml` and its code. It answers what the item does and what it reaches for: for an app, anything beyond its
  manifest; for a skill, any hidden or dangerous instruction, any ask to send data anywhere and any prompt
  injection; for an MCP server, the address, the auth and what its secret grants, or the command, its code and
  anything beyond its permissions. The report is written onto the version with
  `catalogue review`, or the review written failed with why. It decides nothing by itself.
- **Who sees an app:** its maker and its owner, the heads of the maker's home unit, whoever one of its targets takes
  in, and whoever holds a copy. Everyone else of the company, and everyone of another company, never meets it.
- **The administrator is a member of their own company's catalogue:** reading the company their home unit stands in,
  they install, update, drop and adopt a copy there as any member does, on the runner their records name, besides
  recalling at the platform; in any other company they see every app and install nothing.
- **Installing one,** by the receiver: the app seen, not withdrawn, its newest version approved where a head must
  approve it, offered to them on the chain and installing allowed them. The catalogue's source is read checked
  against its hash, handed to the runner to receive under the app's name, placed, put at home with `put --owner
  --app <key>` so the copy stands for the shared app, and pointed at; then the receiver's own link of the app's `on`
  is turned on in their name and the copy recorded as theirs. A refusal anywhere leaves the PC's pointer where it
  stood.
  A skill's copy is written into the receiver's skills at home instead, checked against the same hash, with no PC
  (「Skills at home」); an MCP server's place carries what its version asks for (「MCP servers」).
- **A receiver's copy is theirs to drop or adopt:** dropping it removes it as a personal app is removed — a skill's
  folder at home, and every attachment of it (「Attaching skills and MCP servers」) — and turns their link of `on`
  off, so a head's turning on does not bring it back; adopting it adopts
  the folder on the runner, puts it again as the person's own personal app naming no shared app — an MCP server
  under its own key — turns their link off and lets the catalogue's record of the copy go, so no update reaches it
  again; a skill's copy is adopted by taking its mark away at home. Neither is open where a lock
  above keeps the app on for them.
- **A head turns an app on for a unit they head:** the unit's link of `on` turned on, locked where the head asks,
  and its `offered` left to follow above. Each member under the unit whose PC is reached then has a copy installed
  for them; one whose PC is off has it installed as their PC is next heard (「Keeping the copies」). Turning it off
  there leaves the copies, which the chain withholds.
- **A recall** — a head's at a unit they head, the administrator's at the platform — turns the app's `offered` off
  at that link and locks it; the administrator's takes the platform's rule offering it to the company away too. Every session under it is withheld the app from its next turn (`docs/tools.md`, 「The
  apps a person reaches」), and each copy under it is taken off its PC as the PC is reached.
- **A withdrawal** is the owner's or the maker's: no new install from then, every copy going on.
- **When the maker leaves,** their shared apps stay, and each passes to a head of the maker's home unit, the nearest
  unit up the line holding one who has not left, as the catalogue is next read; the maker's own apps are sealed with
  the account, since no session of it starts again.
- **A promotion** — a request to make the app a platform app — is asked by its owner, its maker or a head of the
  maker's unit, and listed for the administrator.

### Keeping the copies

- **A person's copies are kept to what the chain and the catalogue say** each time their PC is heard after it was not,
  each time a head turns an app on or recalls it for a unit above them, and each time a new version of an app they
  hold is shared:
  - a copy of an app no longer offered to them is taken off their PC and let go;
  - a copy standing on an older version moves onto the newest where the newest asks for nothing its own version did
    not, and is left for the receiver to confirm otherwise, the console asking them again for what grew;
  - an app turned on for them by a unit's link, offered, not withdrawn and approved where it must be, is installed
    where they hold no copy and may install received apps.
- **A PC that cannot be reached is kept next time,** and nothing at home waits on it. A person's skills are kept
  whether or not their PC is heard, since they stand at home.
- **What a reader is told:** the apps shared with them since they last looked, and the copies waiting for their
  confirmation, which the console lists as notices; and for a head, the versions waiting for their approval.

## Skills at home

A skill is instructions an agent's program reads: `SKILL.md`, opening on its front matter, and the reference files
beside it. A person's skills stand at home, never on their PC, and reach only the agents they are attached to
(「Attaching skills and MCP servers」).

- **They stand under the data root,** `agency/skills/<account>/<name>/`: the skill's `SKILL.md` and its reference
  files, text alone, all of them together at most the catalogue's bundle size, eight mebibytes. A folder holding
  `.received.json`, `{"app": <catalogue id>, "version"}`, is an installed copy of a shared skill; one without it is
  the person's own. `.version` holds the version the folder stands on: each save of an own skill names a new one by
  the moment and a random suffix, as the runner names an app's, and a copy takes the shared version's name.
- **A skill reads where its `SKILL.md` opens on YAML front matter** (read by a maintained YAML library, the block
  found by the markdown reader) holding `name`, the folder's name — a lower-case letter, then lower-case letters,
  digits and `-`, at most 64 — and `description`, a non-empty string, what the agent's program reads to know when to
  use it. It may hold `title` and `summary`, each a string or a table of locales out of `ko`, `en`, `ja` and `zh-CN`,
  the text a screen shows it by, and `language`, the language a plain string is written in; any other key is the
  program's and is kept as written.
- **Who may make one** is the delegation chain's `personal-apps.make`, as for an app (「Personal apps」); the
  administrator's account is held to none of it. A skill asks for nothing: no network and no secret.
- **Saving a skill writes the whole folder at once:** the files staged beside it and moved into place, refused where
  `SKILL.md` does not read, its name is not the folder's, a path climbs out or names `.version` or `.received.json`,
  a file is not text or the whole is larger than the bundle size, or a folder of the name is an installed copy.
  The person's BMO writes one with `bmo/skill_put` (`docs/tools.md`, 「The skill put」), and the console through
  `docs/api.md`, 「personal-skills」.
- **Removing an own skill** takes its folder and every attachment of it away; an installed copy is dropped through
  the catalogue.
- **Shared, installed, moved, dropped and adopted at home:** a share reads the own skill's folder and its `.version`;
  an install or an update writes the catalogue's source, checked against its hash as the runner checks an app's, into
  the folder with its mark, refusing a folder of the name that is the person's own; a drop removes the folder and its
  attachments; an adoption takes the mark away. None of it waits on the PC.

## MCP servers

An MCP server is a personal app of its own shape: the source in its folder of 'my apps' holds `mcp.toml` (the tool
host's `docs/catalogue.md`, 「Kinds」) in place of a manifest, and the runner makes the manifest itself as it places
it, so everything after the place — the put at home, the pointer, a call's relay, sharing and keeping copies — is an
app's. The person's BMO has it written by a worker run as it has an app written (the `make-app` procedure) and puts
it with `bmo/app_put`.

- **`mcp.toml`'s `[server]`** names it, `name`, the folder's; `title` and `description`, each a string or a table of
  locales; `language`; and `transport`:
  - `local`: `command`, a list of words run in the placed folder inside the confinement, and `build`, a shell line
    run as an app's build is, where its source is built;
  - `remote`: `url`, an `https` or `http` address; `auth`, `none`, `bearer` or `header`; `header`, the header's name
    for `header`; and `secret`, for `bearer` and `header`, the name of a secret in each receiver's own secret space,
    never a value.

  A `[permissions]` table beside it names the network and the secrets as an app's does. What a version asks for is
  that table's, and for a remote server its address's host, with its port where it names one, among the network and
  its `secret` among the secrets.
- **A place of a folder holding `mcp.toml`:**
  - it is built where `build` is named, confined as an app's build, and copied as an app is, its source kept;
  - its tools are listed by running the runner's own binary confined as a call runs, at `/.bmo/node-link`, as
    `node-link mcp --list`, with the version's folder at `/tool`, the network through a proxy of the listing's own
    reaching the addresses the place ask carries, `network`, and the secrets it carries, `secrets`, each a name and
    its bytes in base64 as a call carries them, in `/secrets`;
  - where the version asks for an address or a secret the ask does not carry, nothing is placed: the runner answers
    `{"t": "wants", "network", "secrets"}`, every address and every secret's name the version asks for, and agency asks
    again carrying them. Before it does, it judges them as a put's permissions are judged (「Personal apps」) and
    reads each secret from the account's secret space as the tool host's relay does (「The network and secrets of a
    call」), refusing one the space holds no file of, naming it and never its value. A received copy's place carries
    the version's permissions from the first ask;
  - the listing writes the version's manifest, `tool.toml`: `[tool]` with `name`, `protocol = 1` and `run =
    ["/.bmo/node-link", "mcp"]`; one `[[entry]]` a listed tool, in the namespace `my`, named as the tool host makes a
    listed tool's name into an entry's (the tool host's `docs/api.md`, 「How a listed tool reads as an entry」), the
    first of two taking one name keeping it, with `capabilities = ["my.own"]`, its title and its description as that
    rule reads them, and `input = "schemas/<entry>.json"`, the tool's input schema written there; and
    `[permissions]`, what the version asks for. Beside it, `mcp-tools.json`, each entry's name to the server's own
    name of the tool. A listing that fails, or lists nothing, refuses the place, and the version staged is taken away;
  - the description answered is that manifest and its schemas, so the put at home is an app's.
- **A call runs the runner's own binary:** a run whose first word is `/.bmo/node-link` has the runner's binary bound
  there, read alone, as the proxy's forwarder is. `node-link mcp` reads `/tool/mcp.toml` and `/tool/mcp-tools.json`
  and the call on its standard input (the tool call protocol, kind `call`), and speaks MCP as a client: a local
  server's `command` started in `/tool` on its standard input and output; a remote server reached over streamable
  HTTP at its `url` through the call's proxy (`HTTPS_PROXY`), with `Authorization: Bearer <secret>` or `<header>:
  <secret>`, the secret read from `/secrets/<secret>`. It initialises, calls the server's tool the entry names with
  the call's arguments, prints the tool's result as one JSON object — `content`, `structuredContent` where the
  server answers one, and `isError` — and ends, ending a local server with it. An error the server answers is a
  result with `isError` true and its message as text. The confinement, the proxy and the masking of secrets are a
  personal tool call's.
- **Its home copy names a key:** a received copy its shared app's, as every copy's; the person's own server
  `own-<account>-<name>`, the name's `_` written `-` and, where that is longer than an app key holds (fifty), the
  name written as the first twelve hex digits of its SHA-256. Agency tells an MCP server's put by the run words of
  the manifest the runner answered. So the tool host withholds each from a record as the record's withheld-apps file
  names it (「Attaching skills and MCP servers」).
- **Which personal tools of an account are MCP servers** is read from the tool host's listing of the account's
  personal tools: one whose key is an own key, and one whose shared key is the key of a catalogue record of the kind
  `mcp`, its company read off the key.
- **Agency keeps its own record of them,** `mcp_servers` in its store (`docs/store.md`): one row a server of an
  account's, its name and the key its home copy stands under. Agency puts every one at home itself, so the row is
  written before the put — an own server's under its own key, a received copy's under its shared app's key, an
  adopted copy's moved onto its own key — and a put the row cannot be written for is refused; a put of an app that is
  no MCP server takes away any row of its name, and a removal at home takes the row away with the server, a dropped
  copy's among them. Each time a record's withheld-apps file is resolved and the tool host's listing reads for the
  account, the record is made to agree with it: every server it lists is written, and a row whose key the listing holds no tool of is taken away; a row whose key
  the listing holds but the catalogue cannot class is kept.

## Attaching skills and MCP servers

A person's skill, and a person's MCP server, reach the agents it is attached to and no other: never every run.

- **The holders an item is attached to are the account's agents:** its supervisors, and its role agents (`docs/role-agents.md`). For the administrator's account, the supervisors of no account and the role agents of no account are
  its holders too.
- **Kept in agency's store,** `agent_attachments` (`docs/store.md`): one row an attachment — the account, the holder's
  kind, `supervisor` or `agent`, its name, the item's kind, `skill` or `mcp`, and the item's name. Changed through
  `docs/api.md`, 「attachments」, which sets one holder's two lists whole. A list naming a kind the holder's line takes
  none of is refused: a line takes attached items of a kind where its carries name the source `attached` for it
  (`docs/launch-lines.md`, 「What a line carries」).
- **A role run's attached skills:** the worker line's plugin holds its role's skills as it does without them, and
  beside them each skill attached to its seat, the agent it is let in on (`docs/role-agents.md`, 「What a run reads
  off its agent」), copied from the account's skills under its own name into the seat's own plugin; a name the
  role's skills hold already is left out and said in the log.
- **A supervisor's attached skills:** a plugin of its own under the agents root, `supervisor-skills/<supervisor>/plugin`, holding a minimal `.claude-plugin/plugin.json` and its attached skills, passed to its program with
  `--plugin-dir` where it holds any. A change to its attachments changes its command, so its resident program starts
  again on its session at its next turn.
- **A line whose carries' `skills` lacks `attached` never carries one,** and a launch asked to place attached skills
  on such a line is refused.
- **An MCP server reaches a record's holder alone:** every record's withheld-apps file (`docs/tools.md`, 「The apps a
  person reaches」) names, beside the apps the chain withholds, the key of every MCP server of the record's account
  not attached to the record's holder — a supervisor's record, that supervisor; a role run's, its seat — and of every
  one where the record's line's carries' `mcp` lack `attached` or the record names no holder. The servers are those of
  agency's record and of the tool host's listing together. A record of the administrator's account has a file written
  as well, naming those keys alone with `personal` both true, so its sessions keep every other app and the network
  and secrets of their personal tools.
- **Only what is known to be attached reaches a run; what cannot be known withholds:**
  - where the tool host's listing of the account's personal tools cannot be read, or the holder's attachments cannot
    be read, the file names the key of every MCP server agency's record holds, the attached ones among them, so the
    run goes on without any;
  - where agency's own record cannot be read, the file is written with `apps` null, which the tool host does not read
    and so takes as every app withheld (the tool host's `docs/api.md`, 「`withheldAppsFile` withholds whole apps」);
    the file keeps its account, holder and line, so a later turn resolves it again;
  - either way the file's `attached` says so, `{"withheld": true, "why"}`, and agency's log says it once, as the
    file comes to say it.
- **Attached skills are placed whole or not at all:** where a holder's attachments cannot be read from the store, or
  the folder of a skill attached to it does not read whole — its `SKILL.md` and every file and folder under it —
  none of its attached skills is placed, and the log says why once a launch: a role run carries the plugin it carries
  with nothing attached, its role's or its agent's own list, and a supervisor's program is handed no plugin of
  attached skills.

## Who reaches a node

- **An agent runs a command on a node through the node service's tool alone,** the tool host's `bmo/node_exec`
  (the node service's `docs/tools.md`), asked with `bmo use bmo/node_exec`. jake's console, the hub's exec route
  asked by hand and a script holding the hub's token are no way an agent takes. Agency's own setup, check and sync
  of a PC's workplace and the link it holds reach the hub for the deployment (「The workplace on a PC」, 「The link
  over jake」), and run no command an agent names.
- **A caller reaches the nodes its own account owns, and the administrator's supervisors reach every node.** The
  tool judges each call; agency supplies what it judges by, from its records:
  - **the node's owners,** the accounts whose record names the node, and for an isolation node the account it is
    named for; a node neither names is no person's and is reached by the administrator's supervisors alone;
  - **the account the caller acts for,** read off its agent: the supervisor it is and its `ownerAccount`; else the role agent of that name and
    its `ownerAccount`; else the seat its run holds at its role and folder (`docs/role-agents.md`, 「What a run
    reads off its agent」) and that seat's `ownerAccount`. A caller none of these names an account of is the
    deployment's own: the owner's supervisors, his jobs, his runs;
  - **the administrator's grant,** which a supervisor of no account or of the administrator's account holds, and
    no other caller does: not an account's caller, and not a role agent, a seat's run or a job of the
    deployment's own, which owns no node and so reaches none through the tool. Agency writes it into the record of
    every session it issues for such a supervisor: the grants `policy/person-nodes.toml` names, `node.admin`, the
    resource the node tool reads as reaching every node, and `platform.payment`, the platform's own card, which
    agency's DeepSeek top-up declares (`docs/tools.md`, 「The DeepSeek top-up」). A record holds what was true when it was issued, and a session issued
    again reads its caller afresh.
  - **the capabilities that operate the platform** go with the grant, on a record of a line that hands its runs
    tools: `deploy`, which the testbed's deploy asks, and `research.owner`, which the research unit's taking,
    dismissing, stopping and resuming of the research queue ask (`policy/person-nodes.toml`,
    `administrator_capabilities`). No line gives them, so a person's supervisor, a role agent, a seat's run and a job
    hold neither, and its tool limit narrows them as it narrows every other capability.
- **The tool reads the first two over agency's routes and the third off the caller's record** (`docs/api.md`,
  「person-nodes」): the node's owners from the list of these records, the caller's account from the supervisor or
  the role agent its record is held under, and the grant from the record itself, which agency wrote when it
  issued it. A run of an account's seat whose record is held under neither name holds no grant and names the
  tool no account, so it reaches no node at all, its own account's included.
- **One read answers all three for a caller and a node,** `GET /nodes/{nodeName}/access` (`docs/api.md`,
  「person-nodes」), judged as the record's grant is: the node's owners, the caller's account, whether it holds the
  administrator's grant, and whether it reaches the node. The node service reads it as its owners' read, asked on
  agency's control port for each run with the run's caller named as `agent`, and takes the node's owners alone
  from the answer (the node service's `docs/nodes.md`, 「A person's node」).
- **The hub's settings file is read by the units that reach a hub, and by no agent:** the node tool, run by the
  tool host for a call a caller makes; agency's server and its node verbs, for a PC's workplace and its link; and
  the node service, for its checks, its readings and the runs units make through it, reading the hub's control
  token for each run's one request and handing it to no caller. No session is handed the file's path or anything
  read from it: no record, run variable, place or link spec of a session names it, and the link sends none of it
  to a PC. An account's sessions stand on its PC (「Which node is a person's」), where the file never is, so their
  shells reach a node through the tool alone.
- **Agency reaches a hub for one node with a credential scoped to that node's agent alone,** never with the hub's
  control token: `<agent>.<mac>`, the mac the lowercase hex of HMAC-SHA256 over the agent's name keyed by the
  hub's control key, `JAKE_CONTROL_KEY` in the hub's settings file (jake's `docs/architecture.md`, 「Configuration
  and secrets」). The workplace's setup, check and sync present it on every request to the hub, and the link to
  the hub's connect program for every byte stream, so the hub refuses a request naming any
  other agent with 403 `agent-out-of-reach` and its status shows that one agent alone. A slip that names the
  wrong node then reaches no other person's machine.
  - **Agency makes the credential itself,** each time it opens its way to the hub, and keeps it nowhere: the key
    stands in the hub's settings file beside the control token, which every unit here can read already
    (「What stands and what is left」), so a credential handed in by the service that binds an account would close
    nothing more, and a hub node no account's record names is
    reached the same way.
  - **A hub whose settings hold no control key is not reached:** the setup and the link say the key is missing
    rather than falling back to the control token.
  - **The control token, which reaches every agent, is the node service's alone:** its tool, for a caller the
    administrator's grant lets reach every node; its runs, `POST /nodes/{nodeName}/runs` (the node service's
    `docs/api.md`, 「Runs」), each one command a unit that is not a session runs as itself with a token of its own,
    on a node whose callers setting names the unit and for an account among the node's owners, which the node
    service reads through agency's access read (「Who reaches a node」, above); and its checks and readings.
- **An account's node asks the home bridge for the verbs `policy/person-nodes.toml` names alone** (`bridge_verbs`):
  the status line's and the hooks' `agent-status`, a hosted tool's `use`, and the verbs of other units a
  supervisor's manual names. The bridge runs a call of a node an account owns, its PC or its isolation node, only
  where it asks for the CLI's `bmo` with one of those verbs as its first word, and refuses every other verb, every
  script and a call naming its verb after an option, saying so with exit code 2. A node no account owns is the
  deployment's own and asks for any verb, as an SSH node does. So no session of an account's node runs agency's
  node verbs, the testbed's checks or a script here, any of which would reach the hub's settings file as this
  machine's user.
- **The hub's settings file is the owner's user's,** under the owner's home, readable by that user alone. The
  sessions that run as that user are the owner's and the deployment's own, which keep the whole machine
  (`docs/account-isolation.md`, 「The scope」): its supervisors hold the administrator's grant and reach every node
  through the tool already, and its role agents' and jobs' runs reach none through the tool while their shells
  read the file. No session of any other account runs as that user: its runs stand on its PC, or on its isolation
  node under a user of its own, which neither reads the file nor runs `sudo` or docker.

## What the PC needs

- **A Linux shell:** WSL on a Windows PC, a Linux PC as it stands. herdr and the programs run there, as on every work
  node (「The workplace on a node」 in `docs/work-nodes.md`).
- **jake, installed by the invitation,** and nothing else installed by hand: agency lays the workplace down through
  jake — the tools, herdr, the programs and the data root — as setup lays one down on an SSH node.
- **A user manager of its own that runs while nobody is logged in:** systemd in the Linux shell, with lingering on
  for the person's account, since every long step and the far end run as units of it.
- **bubblewrap in the Linux shell,** for the person's personal tools alone (「Personal tools on a PC」): the
  `bubblewrap` package, which the setup installs where `sudo` asks no password, with unprivileged user namespaces on,
  as WSL and most distributions have them.
- **jake's agent offering the link's two ports to stream to:** the far end's mirror and herdr ports among its
  stream ports, which are agency's policy (`policy/person-nodes.toml`). The agent listens on nothing for the link:
  the ports a program on the PC reaches home through are the far end's own (「The link over jake」).

## The workplace on a PC

`bmo-agency node work setup <node> --install`, `check` and `sync` stand, read and bring up a PC's workplace with
the steps an SSH node's take (`docs/work-nodes.md`, 「The workplace on a node」), through the hub the node service
names for the node: its control address, its settings file, and the agent's name, every request carrying a
credential scoped to that agent (「Who reaches a node」).

- **A person's PC is stood up by itself, with nobody's hand:** the process keeping the links (`docs/work-nodes.md`,
  「The link」) starts the setup, with `--install`, as a unit of its own, `bmo-node-setup-<node>`, on the server's own
  environment file, for every node an account's record names that its hub holds now (the node service answers it
  up) and that has no workplace yet. So a person who joins by invitation has their PC's workplace laid down and its
  link stood as soon as their machine is proven and attached, and a PC that was off waits for its agent to attach.
- **One not ready is stood up again, not only checked:** once the policy's while (`policy/person-nodes.toml`) has
  passed since the setup, sync or check that left it missing something, and while its hub holds it, the setup runs
  again with `--install`, since a check installs nothing and a PC whose first setup was cut short — the agent going
  away mid-step, a tool not yet installed — would otherwise stay missing for good. Its link stands once a setup finds
  it ready, as for any workplace.

- **A command goes through the hub whole, as bytes no shell on the way reads:** the hub is handed one line that
  decodes the command and gives it to the Linux shell, since an agent on a Windows PC hands its Linux shell a line
  another shell has already read once, and a variable the command sets and reads would come out empty.
- **Each command of the setup is a transient unit of the person's own:** a command through the hub writes the step
  into a script under the PC's cache directory and starts it with the user manager, and further commands watch it
  until it ends, each well inside the hub's ceiling of time on one command, then read back what it wrote and the
  code it ended with. So no step is cut by that ceiling, however long a cold build takes, and a step outlives the
  jake agent restarting: its watch is asked again, for a couple of minutes, until the agent attaches again. A
  program a step starts apart outlives the step's unit, as it outlives an SSH session.
- **The commands run in the shell the agent offers for Linux:** its own where it runs inside the Linux shell, WSL's
  where it runs on Windows.
- **A step that fails says why in what the PC wrote:** the code it ended with, what it wrote on its errors, and
  what it wrote on its output where its errors hold nothing, since WSL's own launcher, failing before any command
  runs, writes why on its output. Every command asks WSL to write its own messages as UTF-8, so they read here as
  written.
- **The copy goes through the hub's uploads:** the same files an SSH copy sends are gathered here into a mirror
  under the node's directory, packed into one archive, uploaded with its length and checksum declared, and laid over
  the PC's copy there with the same files left out, so what is built or installed on the PC stays. A PC without
  herdr is given this machine's the same way.
- **An upload is written where the agent's own filesystem reaches:** at the Linux path where the agent runs inside
  the Linux shell, and at WSL's Windows view of that path where the agent runs on Windows, asked of WSL for the
  folder the file goes into.
- **A PC is not saved as a machine of this herdr,** which reaches a machine over SSH alone; its herdr session is
  reached through the link.
- **The far end stands as a user service, `bmo-node-link`,** written by the setup into the person's user units and
  enabled, and started again after every copy so it runs the binary just built: the link's serve program on the
  workplace's root, listening at the PC's loopback on the mirror port and the herdr port, making the node's bridge
  socket, and keeping the herdr session and the agent host standing. Stopping it stops the far end
  alone; the herdr session and the agent host it started stay, so a restart ends no agent.
- **One PC's Linux shell holds one far end, and so serves one node's workplace.** The far end's ports are the
  policy's one pair and its user service has one name for the user, so a connection a program on the PC makes to
  the bridge socket or a port carried home is dialed home by whichever near end stood up last. Two agents of one PC,
  jake on Windows and jake inside its WSL, would each be given a link to that one far end, and a program of one
  node could be answered by the other node's near end, whose bridge takes other verbs. So:
  - the setup, check and sync read the Linux shell's machine id through the hub before anything else, and keep it
    in the node's setup record beside its herdr session;
  - a workplace whose machine id is the one another ready workplace's record holds is not set up and not made
    ready: the check says which node's workplace holds the PC's far end, and nothing is laid on the PC, since a
    setup would stand the far end again on its own root; it stays missing until one of the two workplaces is
    removed;
  - a machine id that cannot be read is said with the rest of what is missing, since nothing then tells whether
    another workplace holds the PC.
- **What the hub's agent does not offer is said** with the rest of what is missing: a mirror or herdr port not among
  its stream ports, or no Linux shell.
- **A port the far end could not listen on is said** with the rest of what is missing, by its number, what it is
  for and why, as the link's state last holds it (「The link over jake」), so the check fails on it.
- **WSL's networking is read and said on a Windows PC:** its networking mode, and in mirrored mode the range of
  ports its Linux side takes for its own connections, which Windows keeps from every program of its own. Any of the
  far end's ports inside that range is said with it, since a Linux connection's own port may hold it when the far
  end starts again.

## The link over jake

The link of a person's node carries what the SSH link carries (`docs/work-nodes.md`, 「The link」), over jake:

| What | Direction | How jake carries it |
|---|---|---|
| the mirror | both ways | a byte stream to the far end's mirror port on the PC |
| the node's herdr session socket | here → PC | a byte stream to the far end's herdr port, which the far end joins to the socket |
| the tool host's port | PC → here | the far end listens on the port at the PC's loopback; each connection is dialed home over a byte stream of its own to the mirror port, joined here to the tool host |
| the model gateway's port | PC → here | the same way, each connection joined here to the gateway (`docs/work-nodes.md`, 「The model gateway on a node」) |
| the home bridge's socket | PC → here | the same way from the node's bridge socket, each connection joined here to the bridge's socket |
| a personal tool's call | here → PC | a byte stream of its own to the far end's mirror port for each call asked on the link's runner socket, named `run`, joined there to a runner (「Personal tools on a PC」) |
| a local AI's address | here → PC | the near end listens on a port of this machine's loopback for each, and opens a byte stream of its own to the far end's mirror port for each connection, named `reach`, joined there to the host and port the PC reaches (「A local AI on a PC」) |

- **The far end stands on the PC as a service of the person's own,** started by the workplace's setup and kept
  running there: the link's serve program, listening at the PC's loopback alone, inside the Linux shell beside the
  programs. A machine reached through the hub keeps nothing open to the world, since jake's agent dials out.
- **The near end makes no SSH connection:** it reaches the far end's ports through the hub's byte streams alone.
  What the mirror says is the SSH link's, frame for frame, with the frames of the road home below added.
- **Everything a program on the PC reaches at home goes by the same road:** a byte stream the near end opens to the
  far end's mirror port, as the mirror and the herdr socket go. Nothing is bound on the Windows side. An agent on
  Windows listening at its own loopback stands where WSL's mirrored networking keeps ports for its Linux side that
  no Windows tool shows as held, the whole range Linux takes for its own connections among them, so such a listen
  can fail with no holder anyone can find and no turn of the PC's ever taken; the far end listens where the
  programs connect, on the Linux side, which its own check reads. It needs no listen port of the agent's and no
  target of the hub's.
- **The road home, connection by connection:**
  - once the mirror is up, the near end names the ports here a program reaches — the tool host's and the model
    gateway's — and the far end listens on each port any standing session names at the same number on its
    loopback, keeping one it already listens on, trying a refused one again every half minute, and answering
    which it could not listen on and why: to the session that named them, and to every standing session whenever
    that changes;
  - each connection the far end takes there, or on the node's bridge socket, is held while the far end asks the
    newest standing session's near end over the mirror to dial it home, naming it by a number of its own and what
    it is for, the ask going ahead of any file the mirror is still sending (`docs/work-nodes.md`, 「The link」), so
    a transcript on its way never holds a hook's call past the wait below;
  - the near end opens a new byte stream to the mirror port, reads the far end's greeting, names the connection on
    it, and joins the stream to the port here, or to the bridge's socket here; the far end joins the stream to the
    connection it held, each side's close passed on to the other as it comes. A connection not dialed within
    twenty seconds is closed;
  - the near end joins a connection to a port here only where it named that port itself, so the far end reaches
    nothing else here.
- **The hub listens for the link on nothing, and the near end lets go of whatever it holds:** as each connection
  is made, the near end reads the listens the hub keeps for the node's agent and asks it to drop each one, with the
  same credential scoped to that agent. A listen the agent holds at Windows' loopback takes the same port from the
  far end, since mirrored networking shares one set of loopback ports between the two sides, and nothing of the link
  asks for one.
- **A port the far end could not listen on is the link's state,** with why, written with the rest of the state, and
  said once in the link's log, which is agency's, each time what it could not listen on changes; a turn of a program
  on the node says it waits on that port, and ends on it (`docs/work-nodes.md`, 「When a node drops」).
- **The spec writer writes a jake spec for a node with no SSH login and a hub reach:** the agent the node service
  names, the hub's connect program run as the hub's executable the deployment names in `AGENCY_JAKE_HUB` with its
  `connect` mode, the hub's settings file, and the far end's two ports from agency's policy. A node whose hub program
  is not named has no spec, and the pass says why.
- **A connection the PC has not been heard on is dropped and dialed again** (`docs/work-nodes.md`, 「The link」),
  since the hub's connect program can stand open with nothing coming through it after the PC's agent was replaced.
- **The far end speaks the mirror with each node's newest near end alone:** a near end names its node first on every
  mirror session it opens, and a session that opens for a node ends the one an earlier connection of that node
  holds. A near end that went away — its unit stopped, the connection it held dropped — may leave its stream
  standing on the PC with nothing to close it, and a session waiting behind it would greet each new near end and
  then say nothing, so the link would never be heard again. A session that names no node counts as one node of its
  own.
- **The sessions of different nodes stand side by side:** two agents on one PC, one on Windows and one inside its
  Linux shell, reach the same far end at the same loopback, so two nodes' links can hold one far end. Were one to
  end the other's session, each link would dial again and end the other's in turn for as long as both stand, and
  the one whose wait had grown longest, as an agent's outage grows it, would stay down nearly all the time.
- **Nothing of the deployment's credentials is sent to the PC** beyond what the SSH link sends a work node: the run's
  own variables, the CLI's account token for the nodes, its place, its tool token, the console's public address,
  its supervisor's owner account, and for a program on a gateway road the account's call key file its key helper reads,
  written there readable by the PC's user alone as the tool token is (`docs/work-nodes.md`, 「The model gateway on a
  node」).

## What stands and what is left

| Part | Stands | Left |
|---|---|---|
| the link over jake | a link spec may name a person's PC (`jake`): the near end speaks the mirror over the hub's connect program, joins the herdr socket, and dials home each connection the far end takes for the tool host, the model gateway or the bridge, over a stream of its own to the mirror port, a connection unheard for its bound dropped and dialed again; the far end listens at the PC's loopback for the mirror, the herdr socket and the ports the near end names, and says which it could not listen on | — |
| the workplace on a PC | setup, check and sync reach a PC through its hub, each command a transient unit of the person's own watched until it ends: the deploy tree's commit uploaded and laid out there, the workplace's own runtime, the node binaries built there, the data root and the herdr session, and the far end standing as the user service `bmo-node-link` on the mirror and herdr ports; a second workplace on a PC whose far end a ready workplace holds, told by the Linux shell's machine id, refused and said; what the agent does not offer, a port the far end could not listen on and WSL's networking on a Windows PC are said; a look the PC did not answer said as such rather than as a tool missing; the PC an account's record names set up by the links pass once its hub holds it and it has no workplace, and set up again past the policy's while while it is not ready, so its link stands once a setup finds it ready | — |
| an account's runs on its PC | agency's record of an account's node, set by the binding service or the verb; the spec writer writes a jake spec for a PC; an account's supervisors' sessions and its role agents' runs placed on its node alone, waiting while the node's link is not heard, the wait typed `node-link`, ended by a stop and bounded by the policy, and neither counted by the quiet nor lost silently across a restart | — |
| a supervisor's tools on a node | a supervisor whose account's runs stand on a node handed the tool host's `agency`, `web` and `artifact` namespaces in place of the job, agency, explorer and artifact servers, its record holding the capabilities their entries ask, each limited as the servers are; agency's `jobs` tool filing, stopping and reviewing for the record's holder | — |
| who reaches a node | the administrator's grant and the capabilities that operate the platform written into every record agency issues for a supervisor of no account or of the administrator's account and into no other; the records the node tool reads a node's owners and a caller's account from, and one read of them with the grant and the reach; no session handed the hub's settings file's path or anything read from it, an account's sessions standing on its PC or its isolation node, and the home bridge of an account's node running the policy's verbs alone; agency's setup, check, sync and link reaching a hub with a credential scoped to the node's agent, made from the hub's control key, and refused 403 `agent-out-of-reach` on any other agent | — |
| several accounts on one node | a record's `shared`, a second account's record on a node refused `node-taken` unless both are shared; each account of a shared record given `<node root>/accounts/<account>`, its supervisors' folders standing there through the link spec, and its root answered by the access read; a record not shared keeping its folders | the node service's tool running an owner's command inside the `root` the access read answers; a program of one account kept from every other account's root, places and records on the node, which needs a process namespace, the herdr and agent host sockets out of its reach, places and CLI records of each account's own and bubblewrap set up and checked on the PC, all at once (「Several accounts on one node」) |
| personal tools on a PC | the runner, `node-link runner`, running an owner's placed version confined by bubblewrap to its version and data folders with a clean environment and no network but the call's own proxy to the addresses its manifest lists, the secrets it names handed in the confinement's memory for the call alone, refusing without bubblewrap; each call asked on the link's runner socket carried over a stream of its own to a runner on the PC, refused for any account but the one the spec names, a shared PC naming none; the caller gone ending the runner's group and the tool; records naming their account and its runner, the administrator's a command on this machine; the setup looking for bubblewrap, installing it where `sudo` asks no password, trying it, and saying and keeping what it found | a call-back into the host, an isolation node's runner, and personal tools on a shared PC |
| a local AI on a PC | the spec naming each local AI that is on of the PC's own account with its port here and its host and port there; the near end listening on each port and carrying each connection over a stream of its own, named `reach`, to the far end, which connects from the PC and says whether it reached it; a test asking once on the runner socket | — |
| personal apps | an app's source in the PC's 'my apps', one folder each, written by a worker ask naming the app; the runner placing it confined into a new version, describing, pointing, listing and taking versions, three older ones kept; a put, a going back, a removal and a listing through agency's server, the description put at home under the PC's version; the chain's `personal-apps.make` deciding who may put one and `personal-apps.network` and `personal-apps.secrets` the ceiling of what it may ask for; a screen of an app's own built with the workplace's screen kit, its index written as it is placed, its files read for the tool host, and a short-lived session issued for a console to read and call it under | — |
| a check on a real PC | a PC attached as a jake agent with its stream ports set; its workplace ready, set up through the hub by the links pass as soon as the hub holds the node, set up again past the policy's while where it is left missing, and its copy kept on the deployed commit by the links pass's sync; the server writing its jake spec and holding its link, the mirror, the herdr session, the tool host and the bridge each answering through it; an account bound to it whose supervisor's folder is made on the PC, whose program starts there past its first-run screens, and whose turn is put in once its status line, answered through the bridge, is drawn, so its BMO answers from the PC | — |

