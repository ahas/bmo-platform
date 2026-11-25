**English** | [한국어](README.ko.md) | [日本語](README.ja.md) | [简体中文](README.zh-CN.md)

# bmo-platform

The BMO platform as one service: the account model and the organisations, the delegation
chain every setting follows, the agents and the model gateway that carry the work, and the
console. It gathers what the `bmo-agency` and `bmo-web` repositories stand on today into one
place; until a part lands here, its own repository is where that part stands.

The platform answers one question in one place: what stands for the whole, for one
organisation unit, and for one person. A unit's head reaches their own unit and the units
under it; a person reaches their own; the platform keeps the whole. That chain stands here,
and every setting the platform carries is settable along it.

## What stands here now

The agency part moved in whole: the service that supplies, delivers and controls agents, on
its two axes, the supervisors who own rooms and talk with people and the roles that execute
work, with the model gateway, the stores, the policy and the tools. Two axes run in it: a
caller asks for a run of a role at a path; the platform lets it in under the role's cap,
gives it the provider its role's order holds quota on, starts it, carries its turns, reads
what it says and ends it. herdr is one backend behind the session contract.

The account service and the console stand here too: `server/` is the account service, the
people and the senders linked to them, the companies and organisation units, the delegation
chain every setting follows, the secret store and its kinds, the system mail and the language
packs, with its contract crate beside it under `server/client/`; `client/` is the console, the
screen a person signs in to, which shows the platform's one tree and speaks to both servers.

## Documents

The documents in `docs/` are the agency's, canonical for the part that stands here:

- `docs/api.md`: the boundary contract of the agency part, every route and event it exposes or calls.
- `docs/boundary.md`: the boundary contract of the account service, every route it serves and every call it makes.
- `docs/agents.md`: the two axes, supervisors and roles, a run, and the one interface every agent is reached
  through.
- `docs/supervisors.md`: what a supervisor row holds and how one runner carries every supervisor's turns.
- `docs/agent-roles.md`: what a role row holds, the roles the policy seeds, their bundles and sessions.
- `docs/repositories.md`: what a repository row holds, what a run there is handed, a job and a pinned model.
- `docs/job-review.md`: how a job's reports are examined by its orderer, stage by stage, before the job ends.
- `docs/provider-order.md`: how a run gets its provider and model: the order, quota, falling over and waiting.
- `docs/role-slots.md`: how a role's runs are let in under its cap: the line, reservations, a role that
  follows its supervisor and the standing branch checkouts.
- `docs/sessions.md`: how supervisor sessions and role sessions are kept, reused, folded and replaced.
- `docs/keepalive-schedule.md`: when a supervisor's prefix is pinged warm.
- `docs/PERSONAL-DATA.md`: the personal data the repository keeps out, and the check that holds it.
- The rest of `docs/`: putting words into a running program (`agent-control.md`), the launch lines, the
  accounts and their fallback, usage windows, the registered models and model files, a repository's
  checkouts and workbench, the work nodes, the tools and the run records.

## Folders

| Folder | Holds |
|---|---|
| `crates/` | the Rust crates |
| `server/` | the account service: its crate, its contract client under `server/client/`, its language packs and its settings |
| `client/` | the console: a Nuxt app, a Bun workspace of its own |
| `deploy/` | the service units |
| `docs/` | the canonical documents |
| `packages/` | the TypeScript packages, one Bun workspace |
| `screen-kit/` | what a personal app's screen is built with on a person's PC: bmo-ui's bundle build on the release the console runs |
| `policy/` | the fixed values the platform runs by |
| `prompts/` | the prompts an agent's launch is composed from |
| `skills/` | the worker skills the repository carries |
| `tests/` | every test |
| `tools/` | the tools provided to the tool host |

## Commands

```sh
cargo check --workspace --all-targets
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
(cd packages && bun install && bunx tsc --noEmit)
(cd client && bun install && bun run check)
(cd tests/packages && bun test)
```

## Licence

The repository's own code is MIT; its text stands in `LICENSE` at the root. The `cache-siren` crate is
derived from an upstream project whose licence could not be confirmed, so it carries no licence
declaration and is not covered by the repository's MIT licence.

## Version

The version is the set's, in semver, read off the git tag when a build is made; the workspace manifest's
version is a placeholder that never moves.
