# web

The web console, `bmo-web`: the server, which is the account service on sumida with the language packs,
and the client, the Nuxt console a person signs in to. The console's screens call each unit's HTTP API;
the account model, the grants and the words a room is told in a fixed line are this repository's own,
served to the other units over HTTP. It is the one repository that holds language packs: every other unit
answers in English messages and codes, which these packs render.

## Layout

| Folder | Holds |
|---|---|
| `server/` | the server: the account service on sumida (`src/`, with the deployment's rule file, the routes, the checks, the grants, the data move, the phone keys, the receiver port, the settings, the fixed lines' renderer, the document wording file, the migrations under `src/migration/` and the language pack's parts under `src/lang/`), its binary under `src/bin/`, the language pack's files under `lang/`, one folder to a language, its settings and sumida's, and its tests under `tests/` with their fixtures under `tests/fixtures/` |
| `client/` | the console, a Bun workspace of its own with its lockfile: the Nuxt app under `apps/web/`, the account service's client under `packages/accounts/`, the console's tests under `tests/` with their support under `tests/support/`, the packed bmo-ui layer and bmo-artifact bundle under `vendor/`, and the patch the install applies under `patches/` |
| `graph/` | the code dependency graph: one node per feature a person names, its files, its exported names, the owner's words for it and what it needs, an area to a file (`console.json`, `server.json`), written only by `bmo graph` |
| `tests/` | the checks that span the repository: its dependencies, Korean, layout and names (`docs/checks.md`), run by the server's crate as its test target `repository` |
| `deploy/` | the user units a deployment links: the server's, `bmo-web-server.service`, and the console's, `bmo-web.service` |
| `.cargo/` | Cargo's configuration for this checkout: sumida, a git dependency, fetched through the git program |
| `docs/` | the canonical statements of what the console and its server do, and the inventory of what this repository took from the single repository it was separated from |

At the root stand the Cargo workspace manifest, whose one member is `server/`, and its lockfile; the version in
`VERSION`; `testbed.toml` (the testbed's declaration of how this repository is tested, checked, built and
deployed); `rustfmt.toml`; `CLAUDE.md`; and `korean-ledger.json` (the Korean left in a code file, which the
testbed's Korean sweep holds every file to; no code file of this repository holds any).

## Documents

| Document | States |
|---|---|
| `docs/boundary.md` | The boundary: every route the server serves, every call the server and the console make on another unit, the settings that name those units, and the codes the packs render |
| `docs/accounts.md` | The account service: what it holds, where it answers, when the model is in force, its routes, its command and its tables |
| `docs/language-pack.md` | The language packs: the server's and the console's, how each is read, and what other units read of them |
| `docs/web.md` | The console: its layout, motion, access, wording, every screen, notifications, how it talks to the units, its settings and how it runs |
| `docs/documents-viewer.md` | The document viewer: its roots, names and versions, path boundaries, fragments, the contents, and asking about a passage |
| `docs/skeleton-loading.md` | How the console keeps the place of content it is still fetching |
| `docs/checks.md` | How the suite runs the tests, the browser tests among them; the checks that span the repository: what its packages and code may depend on, no Korean in code or tests, its layout, and the names no tracked file may hold |
| `docs/inventory.md` | Every member of the single repository this one was separated from: where it stands here, the task that carried it, and the unit that owns the ones that did not come |

## Version

- **One version for the whole repository,** in semver. `VERSION` holds it, and the server's and the client's
  manifests equal it.
- **A major change is a change to a contract a consumer reads;** an addition no existing consumer reads is
  a minor change, anything else a patch.

## Checks

`bmo-testbed check quick` runs the three sweeps that read the tree before anything runs: Korean in the code,
the README's folder table and the dependencies the manifests name. The only dependencies from another
repository are the base libraries `bmo-chat`, `bmo-chat-client` and `bmo-ui`, and the console plugin bundle
`bmo-artifact-client`, as `testbed.toml` declares; the console takes the last two from their packs under
`client/vendor/`. `bun run typecheck` in `client/` checks the console app, the account service's client and the
console's tests.
