# Agency's policy

The fixed numbers and words agency runs by, kept as declarations rather than in the code: how long a
clock runs, which roles a store is seeded with, what a provider's program is launched with, what a line
says. Each is a TOML file under `policy/`, one file to a part, and the `policy` crate reads them.

## A part

- **Its file** stands under `policy/`, named after the part (`roles.toml`, `run-stop.toml`); the
  providers' files stand under `policy/providers/`, one to a provider. A comment at the head of each file
  says what the part is and which document states the behaviour it serves.
- **Its reader** is a module of the `policy` crate with the part's type. The file is compiled into the
  crate, so a built program carries the policy it was built with and reads no file at run time; editing
  a part takes a build. The TypeScript packages import the same file, through the `@agency/policy/`
  path, and read it in the shape its reader declares.
- **It is read once,** on first use, into its type. A file that does not read as its type is a fault of
  the build: every part has a test that reads it, so no build that passes its tests carries one.
- **Its words carry marks.** A line that names something it is said of holds a mark, a name of letters,
  digits and underscores between braces (`{room}`, `{code}`). The crate fills every mark of a line at
  once from the values its caller names, so a value that itself holds a mark is left as it is, and a
  mark nothing names stays as written.
- **A variation stays with its member.** What differs per provider is that provider's file and that
  provider's reader; no part keeps a table another part's members are looked up in.

## The parts agency shares with its callers

Some parts hold settings another service also works by: a worker's job prompt, the supervisor's health
notices and the browser's launch. Of each, agency's file holds only the keys agency itself reads (the
scratch folder under a worker's workbench, the account of a dead run and the notice of one that came back,
how a Chrome agency starts stands apart). Agency owns those keys:
a caller that needs one has it from agency rather than keeping a copy of its own, and every other key of
those settings is the caller's. The gates are not among them: a gate's run is the gate service's own, not
agency's, and agency's file holds only the supervisor's own part of a turn.

## The deployment's names

What the deployment's agent and its owner are called is the deployment's configuration, not the code's.
The part declares four names, each with a neutral default and the environment variable a deployment
names its own in:

| Name | What it is | Variable | Default |
| --- | --- | --- | --- |
| The agent's key | the name the agent stands under in the records | `AGENCY_AGENT_KEY` | `agent` |
| The agent's name | what the agent is called by in a room | `AGENCY_AGENT_NAME` | `Agent` |
| The owner's name | what the deployment calls its owner where nothing showed the owner by a name | `AGENCY_OWNER_NAME` | `Owner` |
| The sender's name | the name the agent's own messages are stored under | `AGENCY_SENDER_NAME` | `Agent` |

A variable that is set and not blank wins over its default. A process reads its environment for them
once, on first use.

## The renames

A supervisor or a repository that stood under another name is declared in `policy/agent-renames.toml`:
the name it had and the one it has now, its label and aliases as the row was seeded and as it reads now,
and what its places moved by. The store's settling renames the rows by it, and the side that keeps the
places closes the old name's workspace and moves a renamed repository's checkout by it
(`docs/agent-control.md`, `docs/worker-slots.md`).

## A line's shown entries

A line's `shown` in `policy/run-lines.toml` names tool host entries by their full names, and each must be
an entry the tool host's registry holds: the record a run's token is issued for shows what it names, and a
name the registry does not hold shows nothing and is written into the chat's tools paragraph as a tool
that is not there. The registry is the tool host's own, placed while the host runs, so no test of this
repository can read it and the build does not check the names; an entry the host stops serving is taken
out of every line's list in the same change that notices it, against what `bmo tool list` lists.
