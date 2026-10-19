# The language packs

## What they are

- **Every word a person reads stands in a language pack, never in the code.** The code is written in English,
  and what a person is told — a line in a room, an answer on the console, a refusal, a note in a run's log a
  person reads — is a key the code names into the pack of the language it speaks.
- **This repository holds the deployment's language packs, and no other repository holds one.** Every other
  unit answers in English messages and codes with their fields; what a person reads of a code is rendered here
  (`docs/boundary.md`, 「The codes the packs render」).
- **Two packs stand under one set of rules, one to each reader.**
  - **The server's pack** is what the server says and renders for other units: one folder to a language under
    the pack's `lang/` folder in `server/`, one YAML file in it to a part. The server reads it into types; the
    console's tests read the same files where a test builds an expected line from a part.
  - **The console's pack** is what its screens show: `client/apps/web/i18n/locales/ko.yaml`
    (`docs/web.md`, 「Wording and the language pack」). It also answers every key the bmo-ui layer and the
    bmo-artifact bundle read, and those keys count as asked for, so none of them is pruned as unread.
  - **A key another unit names in an answer stands under that unit's name for it.** bmo-artifact's kind catalogue
    names each kind's label `archive.kind.<id>`, a key no code here spells out; the console's tests hold the
    catalogue's keys as a fixture, count them as asked for, and take their name as bmo-artifact gives it rather
    than in the pack's own `<part>.<group>.<key>` roles.
  - **Korean the console's code matches on is data, not wording:** a string compared against what a unit
    answers stands in `client/apps/web/i18n/data/ko.yaml`, beside the pack, and is read by name.
  - **They stay apart because their readers read a value differently.** The console's pack is compiled by
    vue-i18n, whose message syntax gives `@`, `|`, `{` and `}` meanings of their own; the server's is read as
    plain text with `{name}` marks filled in. A value written for one reader would be misread by the other, so
    neither pack holds the other's keys.
- **What only an agent reads is no wording.** A prompt, a tool's answer to an agent and a refusal an agent acts
  on are English, and stand in no pack.
- **No agent's name and no person's name stands in a pack.** A sentence that names an agent or a room carries a
  mark the code fills from the agent's registration, the room's record, or for the chat agent the server's
  `names.chat_agent` setting.

## A language is a folder

- **Each language has a folder of its own, named by its code:** `ko` today. A second language is a second folder
  holding every file of the first under the same keys.
- **The language a deployment speaks is chosen in one place on each side:** the folder the server's part reader
  compiles in, and the locale the console's i18n module is given. Nothing else names a language.

## A part is a file

- **One file to a part, named for what it is** — `running-report.yaml` for the report of what is running,
  `room-removal.yaml` for what a room's removal says. A part is the words of one feature.
- **A key is `<part>.<group>.<key>`:** the file, then a mapping inside it, then the line, in lower snake case. A
  group may hold groups where a feature's words fall into more than one kind.
- **Every key is commented** with what it is and the marks it carries.
- **One sentence is one key.** The code never assembles a sentence out of pieces a parameter could carry; where a
  phrasing turns on a state, the code chooses between whole keys.
  - A value is filled by marks written `{name}`, every mark at once, so a value holding a mark of its own is
    left as it is (the server's `fill_marks`).
  - A name the pack cannot know is handed in with its particle already joined by the code's particle helper,
    and its mark says so: `{subject}` is a name with 이 or 가, `{topic}` a word with 은 or 는. The server's
    helper takes the particles from the pack's `particles` part.
  - A short piece joined between parts of a line — a separator, a lead space — is a key of its own where it is
    wording, and stays in the code where it is markup.

## How each side reads it

- **The server reads a part into a type,** one module to a part, compiled in and read once on first use, every
  mapping denying keys it does not declare. A file that does not read as its type fails the pack's own tests.
- **The console's server** reads its own pack through `shared/wording.ts`, as `docs/web.md` says.
- **A console test** imports a server part with the runtime's own YAML import and reads its keys by name.

## What other units read

- **A fixed line** is rendered by `POST /words/render`: a code and its fields, in a language, answered with the
  sentence (`docs/boundary.md`, 「Fixed lines」).
- **The document wording file** is rendered from the server's pack into the file bmo-artifact's settings name,
  holding every key of bmo-artifact's 「Wording」 table and no other, and travels with the server's release.
  - Its groups are the pack's `archive-docs` part, as `archive_docs`, and its `search` part, as `search`; each
    holds bmo-artifact's keys under the same names, so a key added to either is a key added to the file.
  - `bmo-web-server wording <file>` writes it; the server's tests hold its keys and each line's marks to
    bmo-artifact's table, kept beside them as a fixture.
  - Each deployment writes it into the release as `documents.ko.yaml`, the path bmo-artifact's `[wording] file`
    names through the deploy tree (`docs/boundary.md`, 「The document wording file」).

## How it is held

- **The quick checks** count none of a pack as code: the Korean-in-the-code sweep reads only code files, and a
  pack is data.
- **A test checks by key, not by sentence.** A test asserting what a person is told builds the expected line from
  the pack — the part's key, filled with the values the test gave — rather than writing the Korean out, so a
  wording fix changes the pack alone and no test.
  - A test of the pack's own shape — that a key is there, that its marks are the ones the code fills — reads the
    file itself.
  - Korean the test feeds in as input stands in a fixture file, never in a test's code; any other input is
    neutral English.
- **A change to a pack reaches the deployment with this repository's release,** as a change to the code does.
