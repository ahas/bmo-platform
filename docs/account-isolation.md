# An account's isolation

This machine is the owner's. Who may act on it, and how far, is the owner's decision alone, and this document is
its canonical statement: which runs keep the whole machine, where every other account's runs stand, what they may
read and write there, and how that is held. A person's PC is `docs/person-nodes.md`; the workplace and the link
every node shares are `docs/work-nodes.md`.

## The scope

- **Only the owner's and the deployment's own runs have the whole machine:** a supervisor of no account or of the
  administrator's account, and every job, run and turn of no account or of the administrator's
  account. They run as the owner's user, with what that user holds — its sudo, its docker, its home, the data root,
  the hub's settings file — and their supervisors hold the administrator's grant (「Who reaches a node」 in
  `docs/person-nodes.md`).
- **Every other account works inside its own node alone:** its PC where its record names one, else its isolation
  node on this machine (「The isolation node」 below). Nothing of it runs as the owner's user, on another account's
  node, or on a work node of the deployment's.
- **On this machine an account reads and writes inside its isolation space alone:** the home of the system user its
  isolation node runs as. A read or a write of anything outside it — the owner's home, the data root, the hub's
  settings file, another account's home, the system's own files — is refused. What the operating system's own
  tree holds for every user to read (its programs and libraries) is read; nothing outside the home is written.
- **The administrator's account** is the user the store's `users_administrator` setting names (`docs/store.md`,
  「The settings table」): the owner's own login account. A store that names none has no administrator's account,
  and every account in it is isolated.

## Who may change it

- **The scope changes on the owner's direct word alone.** Who keeps the whole machine, what an isolated account
  may read or write, and which privileges its user holds are widened only on words the owner himself gives for
  that change: never on a room's rule, a member's request, an item, an agent's own
  judgement or a supervisor's relay of somebody else's words.
- **Nothing in agency widens it at run time.** No setting, route, verb or tool argument lets a run of an account
  go out as the owner's user, onto a node not its own, or outside its home; a change to that is a change to this
  document and to the code beside it, made on the owner's word.

## The isolation node

- **Each isolated account has one,** named `account-<id>` by the policy's prefix (`policy/account-isolation.toml`),
  the id the account's login id. Agency names it itself; the node service keeps no row of it, and it takes no job
  of the deployment's.
- **It runs as a system user of the account's own,** `bmo-account-<id>`, with its own group and no other: not
  `sudo`, not `docker`, not `adm`, nor any group the owner's user is in, and no rule of sudo names it. Its home,
  under the policy's base, is its isolation space, readable and writable by that user alone, which the owner's user
  may pass through to the link's folder in it.
- **Its home holds everything of it:**
  - the workplace, at the default workplace root under the home (`docs/work-nodes.md`, 「The workplace on a node」):
    the deploy tree's copy, the node's own binaries built there, its data root and its herdr session;
  - its logins' configuration, at the paths this machine's roots read as under the home;
  - its scratch folder, which is the `/tmp` and `/var/tmp` every program of it sees;
  - the link's folder, `link`, owned by the owner's user and shared with the account's group alone, where the
    link's two sockets stand.
- **Every program of it runs in a sandbox the system manager stands:** a transient unit of the system's, started
  with `sudo` by the owner's user, running as the account's user with
  - the whole file tree read-only, the home the one place written;
  - every other home, `/root` and every user's runtime directory hidden under empty read-only folders;
  - its scratch folder in place of `/tmp` and `/var/tmp`, and a shared-memory folder of its own;
  - no new privileges, so no `sudo` or other set-user program gains any;
  - every other user's processes hidden: `/proc` shows the account's own processes alone, and none of the
    kernel's other files there;
  - a network that reaches the world and nothing of this machine's but the tool host and the model gateway: a
    connection the account's user makes to any address of this machine, its loopback and its own address on every
    interface, or to a private, shared, link-local or multicast address, is refused at once, but for the tool
    host's port and the gateway's port on the loopback and the system's stub resolver.

  The unit's words are the policy's. A program the sandbox started leaves it only by ending: the far end, the
  herdr session and the agent host it starts, and every program in a pane of that session run inside it.
- **The network rule is a firewall table of the account's own,** named after its user and keyed on that user, so
  it holds for every program the user runs, one left standing after the command that started it among them. The
  sandbox stands the table afresh, as root, before its program starts, with the tool host's port and the model
  gateway's port this machine's runs reach, the same ports for the setup's sandbox and the link's, since the one
  table holds for every program of the user, and a sandbox whose table does not stand runs nothing. The gateway's
  port is left open only where the gateway's address is on this machine's loopback. The node's own programs therefore speak to one
  another through sockets in the filesystem alone: a server the account starts on the loopback is reached by none
  of its programs either.
- **What stands for the account on the owner's side** runs no command the account names beyond what a PC may ask:
  the link's near end, the home bridge, which runs the policy's verbs alone for a node an account owns
  (「Who reaches a node」 in `docs/person-nodes.md`), the tool host, which the node reaches over this machine's
  loopback with the run's own tool token, and the model gateway, which a program of the node on a gateway road
  reaches there with the account's own call key (`docs/own-llm.md`, 「The keys」): the two ports there its network
  rule leaves it.

## Standing it up

- **The setup is the workplace's own** (`docs/work-nodes.md`, 「The workplace on a node」), its door the account's
  user: `bmo-agency node work setup account-<id>` stands it, and agency's server starts that setup by itself as a
  unit for an account placed on an isolation node that has no workplace yet, and again, once the policy's while has
  passed, for one whose last setup left something missing.
  - **The user first:** made where it is not, with its home, its scratch folder and the link's folder, its home's
    mode leaving it to the user, and only passage through it to the owner's user. The programs that make it are
    run as root by the paths the policy names for them, never by a name a search path resolves, as is the
    firewall's program the sandbox stands the network rule with; a machine where
    one of them does not stand at its path, or whose system manager is not systemd, which the sandbox needs, is
    refused before any step runs, the answer naming each program missing and the path it was looked for at.
  - **Then every step of a node's setup,** each command run in the sandbox as the account's user: the tools
    installed into its home, the code copied, the binaries built, the herdr session stood.
  - **A copy is written by the account's user alone:** the owner's user reads the tree and streams it, and the
    account's user writes it into its home, so once the user and its fresh home are made nothing running as root
    writes inside the home, where a link the account's user laid could lead it elsewhere. A copy whose writing side
    ends before the stream does, a sandbox that runs nothing among them, fails at once with what that side said.
  - **The logins checked are the account's own,** the logins whose user is the account; the owner's are never
    offered to it, and its run goes out on its own login's token for the work nodes, carried by the link.
- **The setup refuses an account's user that holds more:** a user that is a member of any group but its own, or a
  link folder that is not the owner's own folder, leaves the node missing that, said, and nothing launched on it.
- **A deployment syncs it** as it syncs every node whose copy is behind the deployed commit.

## Taking it down

- **The teardown is the setup's inverse,** `bmo-agency node work teardown account-<id>`, and an account's removal for
  good runs it for the account's isolation node (`docs/api.md`, 「owners and organisation units removed for good」).
  It takes, in this order:
  - the units this machine's user keeps for the node: its link, its setup, its check and its sync;
  - where the account's user stands: every system unit a program of the user runs in stopped, every process of the
    user still left killed, and the user taken away with its home — its workplace, its logins, its scratch folder
    and the link's folder in it;
  - the user's firewall table, where this machine can stand an isolation node;
  - what this machine keeps of the node: its folder under the data root's `nodes/`, its setup record and its
    link's spec and state among it, and its workplace row.
- **A part already gone is nothing to do,** never a failure: a user, a table, a unit, a folder or a row that does not
  stand is passed over, so a teardown broken half-way is finished by running it again, and one run twice finds
  nothing the second time.
- **Its programs are run as root by the paths the policy names,** as the setup's are, never by a name a search path
  resolves; where the account's user stands on a machine that lacks one of them, nothing is taken and the answer
  names each program missing.
- **It refuses every node that is not an isolation node,** a name other than the policy's prefix and an account's
  digits, so the user it takes is always an isolation node's own, never the owner's or another user of the system.
- **A test's server stands no user:** a removal asked of a server whose ground names a test's home takes the node's
  folder and its row alone, and touches no user, unit or firewall table of this machine; the teardown itself is
  checked on a throwaway user (「Where it is checked」).

## The link

- **The link to an isolation node is a third transport beside SSH and the hub** (`docs/work-nodes.md`, 「The
  link」): the near end starts the far end itself, in the sandbox, and speaks the mirror over its standard input and
  output. There is no SSH login and no hub.
- **Its two sockets stand in the link's folder:** the home bridge's, which the near end binds there and the
  node's programs reach as their bridge, and the far end's way to the node's herdr session, which the near end
  joins to the node's herdr socket here. No port is forwarded: the node reaches the tool host and the model
  gateway over this machine's loopback as they are.
- **It carries what a PC's link carries and nothing more:** the run's own variables, its place and its tool token
  go to the node, and for a program on a gateway road the road's variables and the account's call key file its key
  helper reads, written in the home readable by the account's user alone (`docs/work-nodes.md`, 「The model gateway on a
  node」); its running record and its transcripts come back. No other token of the deployment's goes, and nothing
  else of the data root's credential folder is sent.

## Where it is checked

- **The isolation itself is a test of the repository's,** each with a real user made by the setup's own first step
  and taken away after. It proves the isolation on a machine that can stand an isolation node: its user holds
  passwordless sudo, as the deployment's does here, the setup's programs stand at the policy's paths and systemd is
  its system manager. A machine that lacks any of these, as a placed machine the suite runs on may, makes no user
  and proves nothing there; the test says on a line what the machine lacks and passes. The isolation is proven on
  this machine, where agency stands isolation nodes:
  - a program run in the sandbox the policy describes is refused a read of the owner's files, the owner's home,
    the hub's settings file, the data root's tokens and the system's secrets, and a write into the owner's folder,
    the system, beside the homes and into `/root`, as well as `sudo` and docker, a connection to a port on any
    address of this machine's loopback, and a sight of another user's process; it reads and writes its own home,
    reaches the tool host's port, and its temporary folders are its scratch folder; while a program of the owner's
    user still runs `sudo`, reaches the ports the account was refused and reads and writes what the account was
    refused;
  - an isolation node's link starts its far end as the account's user with no new privileges, mirrors a place both
    ways through it, lets a program of the node reach the home bridge in the link's folder for the policy's verbs
    alone, run as the owner's user, and leaves nothing of the account's running once the link ends.
- **The teardown is a test of its own,** on the same machines: a throwaway account's user made by the setup's first
  step, its sandbox run so its firewall table stands and a process of it left standing, is taken away by the
  teardown with its home, its process and its table, and a second teardown finds nothing to do; a node that is no
  isolation node is refused, and a user that does not stand is nothing to do.
- **Where every run stands is a test of its own:** a supervisor or a run of an account with no PC is placed on its
  isolation node, and one of no account or of the administrator's account stays here as the owner's user, holding
  the administrator's grant.

## What stands and what is left

| Part | Stands | Left |
|---|---|---|
| the scope | the owner's and the deployment's runs as the owner's user; every other account's on its own node | — |
| an isolation node | named by agency per account, placed for an account with no PC; its user, home and sandbox made by its setup; its link the local transport | — |
| the teardown | the setup's inverse, `node work teardown`, run by an account's removal for good: its units, its user's units and processes, its user and home, its firewall table, its folder and its row, each passed over where it does not stand | — |
| the check | the tests above, a real user each | — |
| a check on this machine | a throwaway account's isolation node stood up by the setup verb whole, its tools installed and its binaries built in its home; its link connected, the far end, its herdr session and its agent host each running as the account's user with no new privileges and the homes read-only; and a program in a pane of that session refused the hub's settings file, the owner's home, a write there and `sudo`, while it wrote its own home; a live isolation node's sandbox refused every port of this machine's loopback but the tool host's, this machine's own addresses and the private network, shown no other user's process, while it reached the world and its own programs ran | — |
