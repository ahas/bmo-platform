# The model catalogue

The model files the system's own engines serve, kept as data: what each file is, where it comes from, where
it lies and how it is asked by default. An engine serves the entry its placement names, and choosing
another entry for an engine fetches it, checks it and installs it, with no commit and no deployment.
A registration (`docs/own-llm.md`, 「Registered models」) is how a model is reached; a catalogue
entry is the file behind it, where the file is the system's own to serve.

## An entry

- **Its name:** lowercase letters, digits and hyphens, beginning with a letter, unique in the
  catalogue, and fixed once made — `rvn`. An engine's placement names an entry by it.
- **Its label:** how a screen shows it.
- **Its file:** a plain file name ending in `.gguf`, never a path, under the engine's models folder.
- **Its size and its SHA-256:** the bytes and the lowercase hexadecimal hash the file is checked
  against before an engine serves it.
- **Its source:** where the file comes from — an `https` address it is fetched from, or the path
  under the data root of the training run that made it, which carried it to the head itself — or
  none.
- **Its engine:** the engine that serves it, by the name of that engine's installer (`own-llm`), or
  none for a file no engine of the system installs.
- **Its registration:** the registration the engine is reached by (`own-0-1`), or none. A
  supervisor whose order names that registration is offered the entry (`docs/supervisors.md`, 「Its
  settings」).
- **Its nodes:** the nodes the file lies on now, by their names in the node list, as the engine's
  installer records them; nothing in the code names a node.
- **Its defaults:** the settings the file is meant to be asked with — thinking, `temperature`
  from 0 to 2, `top_p` above 0 up to 1, `presence_penalty` from −2 to 2 and `max_tokens` from 1 —
  each one empty or a value. They are the file's own record, shown with it, and no run reads them: a
  run on its engine is sampled with what its own caller names, a supervisor's generation settings
  (`docs/supervisors.md`, 「Its generation settings」), and the engine's defaults otherwise.
- **Its note:** free text for the owner.

## Where it is kept

- **In agency's store:** the `model_catalogue` table, one row per entry, and the `model_uses` table,
  one row per time an entry was chosen for an engine.
- **Read afresh by every request and every screen,** so a saved change reaches the next one with no
  deployment and no restart.
- **The seed:** the file the deployment's own engine serves is written once, when the table is made, and is the owner's
  from then on — a seed removed stays removed:
  - `rvn`, RVN: `RVN-Q4_K_M-multilingual-mtp.gguf`, 16,998,720,992 bytes, fetched from its GGUF
    repository on Hugging Face; the engine and the registration it names are the deployment's,
    given in agency's configuration.
  - It names no node, since a node is a fact of the machines and not of the code: the first
    install that stands the engine on one records where it lies.

## Which entry answers

- **An engine serves one file,** the one its placement names (the engine's own placement
  document), and every request to its registration reaches that file.
- **The file served now is read from the engine,** never assumed: the `model` its health gives with
  the key names the file, and the entry of that file is the one the console marks as served. A
  health that cannot be read, or a file no entry names, marks none.

## Choosing a model for an engine

- **Where:** a supervisor's settings, which offer the entries of its registration, the model files tab
  on the console, or `bmo-agency model use <entry>`.
- **What it does, in order,** as one run of its own that outlives the screen that started it:
  1. checks the entry has an engine, and that no other choice for the same engine is running;
  2. asks the engine's health, at the addresses of the entry's registration, and refuses while the
     engine is set aside — a training holds its cards — or while it has requests in hand, naming why;
  3. runs the engine's installer in the system's source checkout with the entry's name: the installer reads
     the entry out of the catalogue, writes it into the engine's placement as the file to serve, and
     stands the head again, whose node setup fetches the file from its source where it is missing,
     checks its SHA-256, and starts the engine on it; the cards hold no file and are not touched;
  4. records the head in the entry's nodes once its health answers.
- **What it refuses before anything runs:** an entry with no engine; an entry whose file is on no
  node and has no `https` source to fetch it from, since nothing could bring it to the head; a choice
  while another for the same engine runs.
- **One choice per engine at a time:** the run is a user unit named after the engine, so a second
  start is refused while the first stands.
- **What it leaves:** a row of the model uses table — the entry, the engine, who chose it, when it
  started and ended, how it ended and why — and its log under the data root, which the console shows
  the end of. A run that fails leaves the engine as its installer left it and says at which step.
- **Every file once served stays on the head,** so choosing one served before is the placement
  pointed back at it and the head's install, with nothing fetched.
- **Every run on the registration then answers on the new file:** a job in a repository that pins it,
  and every supervisor's turn and role's run whose order's entry names it.
- **The machine's part is the harness-machine crate's** (`docs/harness-machine.md`, 「A model's use on
  its engine」): the engine's health, the unit, the installer with its log and the end of the log a
  screen is shown, for agency's server and the TypeScript that asks alike. The rows are each side's own.

## Where it is set

- **The console:** the model files tab of the agent management screen lists every entry with the file
  each engine serves now, and adds, changes and removes one; the latest choice for each engine stands
  above the list with how it is going. Registered models are a tab of their own beside it
  (`docs/own-llm.md`, 「Where it is set」).
- **At the boundary:** the catalogue is the `/model-catalogue` resource (`docs/api.md`, 「providers, models
  and model files」), never `/models`: a catalogue entry is the file behind a model, never the model.
- **The command:**
  - `bmo-agency model` lists every entry and the latest choice for each engine.
  - `bmo-agency model add <name> --file <file> --bytes <n> --sha256 <hash> [--label <label>]
    [--source <https or path>] [--engine <engine>] [--registration <name>] [--note <text>]
    [--thinking on|off] [--temperature <n>] [--top-p <n>] [--presence-penalty <n>]
    [--max-tokens <n>]` makes an entry.
  - `bmo-agency model set <name> [the same options]` changes the named fields of one; a field
    given as `default` or `none` is emptied.
  - `bmo-agency model remove <name>` removes one.
  - `bmo-agency model use <name>` chooses it for its engine (「Choosing a model for an engine」
    above).
  - `--db <address>` asks another store's database.
- **What the engines' installers ask of it:** `bmo-agency model --entry <name> --json` prints one
  entry for an installer to read, and `bmo-agency model --placed <name> <node>` records a node
  the file now lies on.
- **What a removal is refused for:** the entry a choice is running for, and the entry whose file its
  engine serves now, as its health names it.
- **The rules a save is held to** — what a name, a file, a hash, a source, an engine and a node may
  be, how long a label and a note may be, and the bounds of each default — are agency's policy's
  (`policy/model-catalogue.toml`).
