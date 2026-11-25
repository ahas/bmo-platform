# Working in this repository

This repository is agency: the service that supplies, delivers and controls agents. It starts agent
sessions, carries turns into them, reads their state and output, interrupts, folds and ends them, keeps
the supervisors, the roles and their slots, the repositories, the accounts and logins, the model registry
and the links to other machines. It decides nothing about a turn's content: no turn assembly, no room,
permission or rule judgement, no reference selection and no output check. Which role a piece of work runs
as is the caller's decision; the provider and model it runs on are the role's order's, and agency supplies
them.

## Principles

- **Each service runs on its own,** with no context from another repository.
- **Services talk only through documented APIs.**
- **References run one way:** the base never knows what is below it, the lowest service owns a shared
  policy value, and each repository holds only what it uses.

## Rules

- **`docs/` is canonical.** `docs/api.md` is the boundary contract, every route and event agency
  exposes or calls. Read the document covering a change first. A change that would make it wrong
  repairs the document first, stating what is true now, and the code is then written to match. A change
  to the boundary API changes `docs/api.md` in the same commit and follows the version rule below.
- **herdr is one backend.** `crates/agent-session` declares the session contract, eleven operations:
  start, switch a session's provider and model, send a turn, push words into a working turn, read state,
  answer the question a turn stands on, read what the latest turn has written so far, interrupt, fold,
  end, list. Every caller reaches agents through it. `crates/herdr` is only herdr's socket client; the
  herdr implementation of the contract is the server's `herdr_backend` module, since it is built over
  the server's door, places and lines.
- **The role slots are a module of the server, not a crate.** Their dispatch shares the request, the roles,
  the provider orders and the routes with the control routes, so a crate boundary between them would be a
  cycle.
- **No other service's code.** Among other repositories only the base libraries `bmo-chat`,
  `bmo-chat-client`, `bmo-secret-shapes` and `bmo-ui` may be depended on or imported, and the Rust side may
  also build on the published clients and libraries of the upstreams it reaches (`docs/api.md`, 「Calls
  agency makes」);
  the testbed's `dependencies` check and `tests/packages/dependencies.test.ts` fail on any other. Another service is
  reached through its API; a call upward is an event agency publishes or a hook the consumer registers,
  with a default that does nothing; data a higher service holds is handed in by the caller.
- **A variation belongs to its member.** A value that differs per provider, model or consumer is
  answered by that member, never by a central table in the layer below.
- **No hand-written parser for a general format.** TOML, JSON, YAML, markdown, shell lines and command
  lines are read by a maintained library; where none fits, the reason is written beside the code.
- **Tests live in `tests/`.** The root package holds them in one test target, the harness
  `tests/main.rs` (bmo-chat's `docs/conventions.md`, 「Rust builds」): a crate's tests are a module of it, a
  directory under `tests/` named after the crate with underscores, the checks that span the repository are
  modules of a file each at the top of `tests/`, and what they share is the `support` module. One file's
  tests run as `bmo-testbed cargo test -p agency-checks --test main <module>::`. The TypeScript tests
  stand under `tests/packages/`. No test module sits in a source file.
- **No test runs Cargo.** A workspace binary a test starts is the root package's copy of it, a `[[bin]]`
  compiled from the crate's own entry point under the `test-binaries` feature, which Cargo builds before
  the tests run; `tests/support/binary.rs` hands it over under its own name. A binary a test newly starts is
  added there and in the root manifest. A TypeScript test that starts one is named `*.cargo-test.ts`, which
  Bun's own run passes over, and `tests/typescript_binaries.rs` runs it under Bun with the directory the
  built binaries stand in.
- **A test takes down every process it starts, with its descendants, and waits on none without a bound:** a
  process that should end by itself and does not fails the test rather than holding the test binary.
- **No test touches the owner's own browser profile.** The DeepSeek top-up pays in the owner's Firefox profile
  (`docs/tools.md`, 「How it pays」); every test of it that can reach the browser points the payer at a throwaway
  profiles folder, and one that drives the machine's Firefox does so on a throwaway profile alone. A run leaves
  nothing behind: Firefox mirrors a profile under its profiles root into a folder of the same name in its cache
  root, which no preference moves, so the throwaway folder stands outside that root, beside it, its disk cache
  off; its drop removes any mirror of its name all the same, and each run first sweeps the throwaway folders
  and mirrors of its prefix past an hour old that killed runs left.
- **A test drops every database it makes, and fails where it cannot.** A store of a test's own is a copy of
  a template its harness migrates once per process on the server `BMO_TEST_POSTGRES` names. The Rust harness
  names them `agency_rs_tmpl_` and `agency_rs_copy_`, the TypeScript one `agency_ts_tmpl_` and
  `agency_ts_copy_`, each followed by the process id. The drop first takes no new connection to the
  database, so nothing the test left running opens a session again, ends the suite role's own sessions on the
  database and drops it without forcing, since forcing needs the right to end an autovacuum worker, which
  the suite's role need not hold, while the plain drop stops one itself. Before a harness process makes its
  first database it drops those of its own two names whose process no longer runs on this machine and that
  no session is on, and names on the error stream any it cannot; it reads no other name.
- **English only** in code, comments, strings, tests and documents, and in commit messages. Korean stands
  in `prompts/`, where it is what the agents are told and answer in, and in the README's language line and
  its translations at the root (`README.ko.md`, `README.ja.md`, `README.zh-CN.md`); `tests/literals.rs`
  fails on Hangul anywhere else. Messages a service shows are English sentences; machine-read outcomes are status
  codes and kebab-case codes.
- **No names of agents, owners or hosts, and no personal paths** in code, prompts, documents or commits.
  Write the role instead: the owner, a worker, a node. A path is written from `$HOME` or from a variable.
  `tests/literals.rs` fails on the account name, the host name and the committer's address of the
  machine it runs on.
- **No personal data**, as `docs/PERSONAL-DATA.md` states it: not a name, handle, account id, mail
  address, host name, home path, device or other network detail of the owner or of a person they know.
  A value that differs by installation is read from settings; an example is neutral.
  The testbed's `personal-data` check fails on a mail address, a home path, a telephone number, a token and a
  private network address in any tracked file, naming the file, the line and the kind but never the value.
- **No directory named after its parent.** `tests/layout.rs` fails on one.
- **Only Cargo manifests at the root,** beside `testbed.toml`, the testbed's declaration. The root
  `Cargo.toml` is the workspace and the root package, which is never published and exists to hold
  `tests/`. The TypeScript packages are a Bun workspace of their own under `packages/`. `tests/layout.rs` fails
  on another build tool's manifest at the root.
- **Documents state what is true now**: no change history, no quoted source and no file-and-line pins.

## Layout

- `crates/`: the Rust crates. `agent-session`, `herdr`, `agent-host`, `pane-head`, `harness-machine`,
  `harness-providers`, `node-link`, `llm-gateway`, `policy`, `store`, `tool-call`, `cache-siren` (the
  accounts' usage windows and the cache-lifetime judgement, `docs/usage-windows.md`) and `server`.
- `tools/`: the tools agency provides to the tool host, `procedures`, `worker_ask`, `app_put`, `skill_put`, `relay`,
  `deepseek_top_up` and `jobs`, each a folder with its `tool.toml` and the crate of its executable
  (`docs/tools.md`).
- `packages/`: the TypeScript packages, `agent`, `archive` and `support`, one Bun workspace.
- `screen-kit/`: what a personal app's screen is built with on a person's PC, a Bun project of its own vendoring
  bmo-ui's release (`docs/person-nodes.md`, 「Personal screens」).
- `policy/`: the TOML files agency's fixed numbers and words stand in, read by `crates/policy`.
- `prompts/`: the prompts agency composes an agent's launch from.
- `docs/`: the canonical documents.
- `deploy/`: the service units. agency's server and the agent host carry the restart mark, the model gateway
  the binary mark, so a deployment restarts it only where its launcher changed; herdr's carries none, since
  herdr is a runtime agency stands on.
- `testbed.toml`: how the testbed tests, checks, builds and deploys the repository, the tools' placement
  among it.
- `tests/`: every test.

## Checks

- Static: `cargo check --workspace --all-targets` and, in `packages/`, `bunx tsc --noEmit`.
- Review: `cargo fmt --all --check` (108 columns, 2-space indentation, as `rustfmt.toml` sets) and
  `cargo clippy --workspace --all-targets -- -D warnings`.
- Tests: `cargo test --workspace`, and `bun test` for `tests/packages/`.
- The version is the set's, read off the git tag when a build is made: the testbed hands it to the build as
  `BMO_VERSION`, and the MCP server names it (bmo-chat's `docs/conventions.md`, 「Versions」). The workspace
  manifest's version is a placeholder that never moves; a commit, a deploy, a fix or a contract change moves
  no version, and a deployment is told apart by its commit. A sibling is a path dependency and is never pinned to a version.
