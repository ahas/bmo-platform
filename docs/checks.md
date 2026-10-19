# The repository's checks

The checks that span the repository stand in the root's `tests/`, in `repository.rs`. The root is a Cargo
workspace and holds no package, so the server's crate runs them as a test target of its own, `repository`, and they
run with the server's tests:

```
bmo-testbed cargo test --manifest-path server/Cargo.toml --test repository
```

They read every file git tracks or would track, the new ones it does not ignore among them.

- **Dependencies.** No package depends on a `bmo-*` package but the base libraries `bmo-chat`, `bmo-chat-client`
  and `bmo-ui`, and the console plugin bundle `bmo-artifact-client` the console extends (`docs/boundary.md`): not
  the Cargo workspace's packages, as Cargo's metadata answers them, and not a `package.json` of the console. No
  Rust file names another `bmo_` crate at the head of a path but the server's own and those four, and no console
  file imports another `bmo-*` or `@bmo/` package. Another unit is reached over HTTP alone.
- **Korean.** No code or test file holds a Hangul character: the Korean a person reads stands in the language
  packs (`server/lang/`, `client/apps/web/i18n/`), and the Korean a test reads in the tests' fixtures
  (`server/tests/fixtures/`, `client/tests/fixtures/`). The testbed's Korean ledger, `korean-ledger.json`, stays
  empty, allowing no file any.
- **Layout.** No directory is named after its parent, the root holds no manifest but Cargo's workspace and its
  lockfile, and no test stands inside a `src/` folder or anywhere but a `tests/` folder: no Rust test module in a
  crate's source, and no `.test.` or `.spec.` file beside the code it tests.
- **Names.** No tracked file holds an agent's, a person's or a machine's name. No repository keeps the list of those
  names, so the check reads it from the file the variable `BMO_NAMES_LIST` names, outside the checkout: a TOML
  file with `words` (matched as whole words, ignoring case), `exact_words` (whole words, case kept), `anywhere`
  (matched anywhere, as a Korean name takes its particles with no space) and `allowed` (terms of others that hold a
  name, such as a dependency's address, set aside first). Where the variable names no file, the names are not
  checked and the run says so.

What a name is replaced by: an agent by its registration, which the deployment's settings carry (the chat agent's
name and label among them); a person by their role; a test's people and agents by neutral names such as `agent-a`.
The console's own name is the pack's, filled with the chat agent's label.
