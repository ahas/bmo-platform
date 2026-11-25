# Fast-track tools

Read this before writing or running a throwaway tool. When to use the fast track is in the manual;
this is how.

## The folder

- Make a fresh temporary folder with `mktemp -d` and work there. It is outside every checkout and
  the service's data root, so nothing a tool writes is committed, deployed or read by a run.
- One tool is one file directly in the folder, named in lowercase letters, digits, dots, dashes and
  underscores: `convert-hwp.py`, `rate-lookup.ts`.
- Look at the folder's list first: a tool already there that does the job is reused, not written
  again.
- Remove the folder by its exact path when the work is done; never remove through a glob or an
  unguarded variable.

## Writing and running a tool

- Write the file, then run it by its absolute path with the interpreter it
  names: `bun <path>`, `python3 <path>`, `sh <path>`.
- A tool may read, convert, calculate, parse a file someone handed over, or call an outside API. It
  never:
  1. opens, writes or queries the service's database, data root or workbenches;
  2. reads or changes the service's configuration or env files, or prints a secret;
  3. starts, stops or changes a service, a unit or a running process;
  4. touches the brain, a checkout, or another room's files;
  5. carries a credential in its own text: it reads one at run time from where it already lives.
- A file a tool makes is read back by you, and what it holds is what you answer. Where the room
  should have the file itself, send it into the room with a sending tool; never say you sent a file
  no tool answered as sent.
- A tool that takes longer than moments is launched on its own, and the reply does not wait for it:
  `bmo use <tool> --async`, whose end wakes you; a command no tool carries writes its output to a log,
  and `bmo/wait` with a `check` on that log, used with `--async`, wakes you once it is done. A result you promised
  comes back to you that way.
- A failed tool is fixed or given up on in the same run; say plainly what could not be done.

## Keeping a tool

- **A need that comes back is made a hosted tool.** Where the same kind of request is likely again
  and no tool the host serves does it (`bmo use --tools --query <words>` finds none), file it in the
  same run as a queue item with `file_item`, `requesterName` you and a title naming the tool:
  - **its repository:** the service's own where the tool is that service's door (it calls the
    service's routes or reads its store), the tool then a folder of its `tools/`; else the tool's own
    repository, `bmo-<name>`, whose checkout stands in the set's `tools/` folder. A repository not yet
    listed you make before filing, as 「A tool's own repository」 below says;
  - **its brief:** what the tool does, each entry's arguments and answer, and why it is worth keeping;
    that it is written to bmo-pocket's `docs/authoring.md`, started from the example in its language,
    with a test its manifest's `check` runs and a call through a scratch host as the guide says; its
    entries under the `bmo` namespace asking the capability `room.tools`, which your sessions hold,
    unless the owner names others; any grant it would need, named for the owner; and that the run
    puts nothing live.
  - It is a draft, and you never approve it: approval is the owner's.
- **A tool's own repository** you make yourself, once, before the item that fills it:
  - `gh repo create <owner>/bmo-<name> --private --description "<what the tool does, one line>"`, `<owner>` the account the
    set's other repositories stand under (`git remote get-url origin` of any of them), private like
    every new repository; making it public is the owner's word;
  - its checkout cloned into the set's `tools/` folder, `git clone <its address> <the set>/tools/<name>`, with
    a first commit holding its `README.md` (one line of what it is), the MIT `LICENSE`, its `testbed.toml`
    copied from the closest tool repository's and renamed, and nothing else; pushed to its `main`;
  - registered so a worker can work in it: `bmo repository add bmo-<name> --path <that checkout>`;
  - a name already taken on GitHub or in the registration is no reason to pick another quietly: tell the owner
    and wait. Then file the item on that repository.
- **A worker never puts a tool live.** Its job writes, tests and commits the tool and ends at the
  merge, as every job does.
- **Once the close has merged, the tool goes live:**
  - a service's own tool with that service's deployment, which registers it;
  - a tool of its own repository by your put: `bmo pocket put <the repository's git address> --allow
    room.tools`, the `--allow`s naming exactly what the owner approved and nothing more. A put refused
    for asking something not allowed goes to the owner; you never widen it yourself.
- **Then look at it and tell him:** `bmo use --tools --query <its name>` shows it, and one call of it
  answers as its brief says. Tell the owner in his own chat what was added: its name, what it does
  and what it was allowed.
- **Taking it back is his word:** `bmo pocket take <name>` takes it out from the next call, and an
  earlier version is put again from its earlier source.
- Say nothing about the filing unless asked; mention it to the owner in his own chat when you next
  report to him.
