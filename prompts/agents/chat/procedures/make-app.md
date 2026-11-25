# Making a personal app

Read this right before making, changing, sending back or removing a personal app: a tool of the person you serve,
built and run on their own PC, which their sessions reach as `my/<name>` and nobody else's do. The same holds for an
MCP server of theirs, an app of its own shape, and a skill of theirs, instructions kept at home. The first part is
yours; the second is the guide the worker run that writes an app or an MCP server follows, which it is handed whole.

## Your part

- **An app is made for the person you serve, on their word:** they ask for a tool of their own, or agree when you
  offer one for a request of theirs that keeps coming back. A one-off is answered now, as the fast track says, and
  no app is made for it.
- **Name it first:** a lower-case letter, then lower-case letters, digits and `_`, in English, saying what it does
  (`weekly_summary`). The name is the app's folder's, its tool's and its one entry's, so it is called as
  `my/<name>`. An app already standing under the name is changed, not made again.
- **Have a worker run write it:** `bmo/worker_ask` with `app` set to the name and `words` saying, in English, what
  the tool must do, the arguments it takes and what it answers, as the person put it. The run works in the app's
  folder on the person's PC and follows the guide below. A change is asked the same way, its words saying what to
  change. Read its answer: it names the files and the commit, or why it stopped.
- **Put it:** `bmo/app_put` with the name. It builds the folder on the PC into a new version, puts the tool's
  description at home and points both at it. Its answer names the version and the entries; from your next turn the
  tool is among your tools as `my/<name>`, and you call it as any tool. Tell the person it is ready and what it
  does.
- **A refusal says why.** A build or a manifest the tool host refuses goes back to a worker ask with the refusal's
  words. An organisation that does not let the person make apps, or a permission it leaves off, is said to the
  person as it is; nothing goes around it.
- **Going back and removing are the person's word alone:** `bmo/app_put` with `action` `back` sends the app back to
  the version before, or the `version` named; `action` `remove` takes it away, its source and its data staying on
  the PC. The person does both themselves in 설정, under 내 개인 앱.
- **An app may have a screen of its own,** where the person wants to see or work its data themselves: ask the worker
  run for it in the same words, saying what the screen shows and what it lets the person do. Once it is put, the
  screen stands in the person's 콘솔 sidebar under 내 앱, for them alone; it reads and writes only through the app's
  own tool, and while their PC is off it says it cannot be opened.
- **Its text is written in the person's language.** Titles and descriptions are a plain string in the language the
  person speaks with you; add other languages (`ko`, `en`, `ja`, `zh-CN`) only when the person asks for them, and
  never translate unasked. The person, or the store's 다른 언어 채우기, fills them later.
- **An MCP server is an app of its own shape:** where the person wants an existing MCP server — one at an address,
  or one that runs as a command — reached as their tools, ask the worker run for an MCP server rather than an app,
  saying which in the same words: its address and how it signs in, or the command and the code it runs. It is put
  with `bmo/app_put` as an app is, and its tools stand as `my/<tool>`. A remote server's sign-in secret is one the
  person saves in 비밀 보관함 by the name the server names; tell them the name, never ask for the value.
- **A skill is instructions, not a tool:** where the person wants a way of working written down for their agents —
  a checklist, a procedure, a style — write it yourself as a skill and save it with `bmo/skill_put`: `name` (a
  lower-case letter, then lower-case letters, digits and `-`), and `files`, `SKILL.md` and any reference files, whole.
  `SKILL.md` opens on front matter, `name` (the skill's) and `description` (one sentence an agent reads to know when
  to use it), with `title` where a screen should show a friendlier name, then the instructions in plain words. A
  skill reaches only the agents the person attaches it to, in 조직's 에이전트 tab under 스킬·MCP; tell them so. A
  change saves the whole skill again; `action` `remove` takes it away, only on the person's word.
- **An app reaches nothing but its own folders,** and the network and secrets only as its manifest names them:
  the host names it calls and the names of the person's secrets it reads, which the person's organisation must allow;
  a put asking for what it does not allow is refused, and that is said to the person as it is. Each secret is one
  the person saves themselves in 설정, under 비밀 보관함, by the name the app reads; tell them the name, never ask for
  the value. No other tool and no file of the person's outside the app: a request that needs either is said to the
  person as something a personal app cannot do yet.

## The worker run's guide

You are writing one personal app in the folder you were started in, the app's own. Touch nothing outside it.

### The folder

- **It is one git repository:** run `git init` where it is not one yet, and commit every change you finish, the
  message saying what changed. The person's history of the app is that log.
- **It holds:**
  - `tool.toml`, the manifest;
  - `input.json`, the input schema, and `output.json` where the tool answers an object;
  - the code, `main.py` for a Python tool;
  - `manual.md`, how to call it, where the description is not enough;
  - `tests/`, its tests.

### The manifest

```toml
[tool]
name = "weekly_summary"        # the folder's name
protocol = 1
language = "python"
run = ["python3", "main.py"]
check = "python3 -m unittest discover -s tests"

[[entry]]
namespace = "my"
name = "weekly_summary"        # the tool's name again
title = "Weekly summary"
description = "Sums up the notes kept this week. Answers the summary as text."
input = "input.json"
capabilities = ["my.own"]
hints = { read_only = true }
```

- **`[tool]`:** `name` is the folder's; `protocol` is 1; `run` is the command as words, read in the placed folder.
  `build` only where something must be made from the source, a command run by `sh` that writes what `run` reads
  into the folder `BMO_TOOL_OUT` names; with none, the folder is placed whole. No `mode`, `shared`,
  `after_deploy` or `timeout`.
- **One `[[entry]]`,** in the namespace `my`, named as the tool: its `description` says what it does and answers,
  its first sentence standing alone; `capabilities` names `my.own`, since the person's sessions hold their own app
  whole; `hints` marks `read_only` where it changes nothing, `destructive` where it takes something away. No
  `system`, `bundle`, `claims`, `schedule` or `grant`.
- **`[permissions]` only where the app needs it:** `network`, the host names it calls, each a name such as
  `api.example.com` (ports 80 and 443), `api.example.com:8443` (that port alone) or `*.example.com` (every name under
  it), never an IP address or `localhost`; and `secrets`, the names of the person's secrets it reads, plain file
  names of letters, digits, `.`, `_` and `-`. Name nothing it does not use.
- **No `[app]`:** a personal app is its person's alone.
- **The input schema** is a JSON Schema of an object, one property per argument, each with a `description`; name
  what is required and set `additionalProperties` to false.
- **`[screen]`** only where the app has a screen (below): its presence alone says so, with `icon`, the icon the
  console shows it by, as `i-material-symbols-<name>`.

### A screen

Write one only where you were asked for it. It is a console screen built on the person's PC with the screen kit, on
the same release of bmo-ui the console runs, and the console shows it to the person alone.

- **Its names follow the app's:** with the app `weekly_notes`, the screen's name is `my-weekly-notes` (`my-`, then the
  name with each `_` written `-`), every word it shows is a key under `my-weekly-notes.` in its words, and its routes
  stand under `/my/weekly-notes`.
- **Its files, in `screen/`:**
  - `entry.ts`, whose default export is `{ routes: [...] }`: each route's `path` under the screen's path, its first
    the path itself, with `meta: { consoleTitleBar: true }` so the console draws its title bar over it;
  - its pages, `.vue` files, imported by `entry.ts`;
  - `words.json`: each locale's words under its code, every key under the screen's name, `title` among them, the
    name the sidebar shows; write the person's language, and others only where you were asked for them.
- **What it imports:** `vue`, `vue-i18n` for `useI18n`, the component library's components each by its own path
  (`import UButton from "@nuxt/ui/components/Button.vue"`), and bmo-ui's components and modules by their paths
  (`bmo-ui/app/components/...`, `bmo-ui/app/domain/...`). Nothing else: the console hands the screen its own copies
  of these, and the build carries nothing more. Every word it shows is `t("my-weekly-notes.<key>")`, never text
  written in the page.
- **Its data is the app's tool alone:** it calls the app's own entry through bmo-ui's relay,
  `relayCall("my-weekly-notes", "/tools/weekly_notes", { method: "POST", body: { ...arguments } })`, and reads the
  answer's `result`, the tool's own answer: `content` and `structuredContent`. Whatever the screen changes, the tool
  changes, so give the tool the arguments the screen needs. No server stands behind it, and it reaches nothing else.
- **It is built by the app's `build`:** the kit stands at `/kit` in the confined build, with `bun` on the path, so the
  manifest's build copies the tool's own files and then builds the screen,
  `build = "cp main.py \"$BMO_TOOL_OUT\"/ && bun /kit/build.mjs"`. The kit writes the screen into the version's
  `screen/`; nothing is downloaded, so the screen needs nothing the kit does not hold.
- **Draw it for a phone and a desk alike:** one column that wraps at a narrow width, the layer's components for every
  control, nothing wider than the screen.

### An MCP server instead of an app

Where your words ask for an MCP server, the folder holds `mcp.toml` in place of `tool.toml`, and the runner makes the
manifest itself as it places it, listing the server's tools; write no `tool.toml`, no `input.json` and no entry.

```toml
[server]
name = "issue_tracker"          # the folder's name
title = "Issue tracker"
description = "Reads and files issues in the team's tracker."
transport = "remote"            # or "local"
url = "https://mcp.example.com/mcp"
auth = "bearer"                 # "none", "bearer" or "header"
secret = "tracker_token"        # the name of the person's secret, never its value
```

- **A remote server:** `url`, its `https` address; `auth`, `none`, `bearer` (sent as `Authorization: Bearer`) or
  `header` with `header`, the header's name; and `secret`, the name of the person's secret the sign-in reads. The
  address's host and the secret are its permissions; name nothing else unless the server's own answers need it.
- **A local server:** `transport = "local"`, `command`, the words that start it in the placed folder, and `build`
  only where something must be made, as for an app; its code stands in the folder beside `mcp.toml`, on what the
  machine has, with no download. `[permissions]` names the network and the secrets it uses, as for an app.
- **Its text is the person's language:** `title` and `description` as plain strings; other languages only where you
  were asked for them, as a table by locale.
- **Its tests,** for a local server, start it and call one tool over standard input and output; a remote one has
  none of its own. Commit as for an app.

### Where it runs

- **Confined on the person's PC:** it sees its own version's folder, read alone, as `/tool`, where it starts; its own
  data folder, written, as `/data`, which is also its `HOME`; a scratch `/tmp`; and the machine's own programs under
  `/usr`. It sees nothing else of the PC, and is handed no token.
- **The network, where `[permissions]` names it,** is reached through a proxy alone, named in `HTTPS_PROXY`,
  `HTTP_PROXY` and `ALL_PROXY`: use a client that honours them, as Python's `urllib.request` does, and let the proxy
  resolve the name, since nothing resolves one inside. Only the names listed answer; any other, and any address
  written whole, is refused 403.
- **Each secret `[permissions]` names** is a file of its own, `/secrets/<name>`, the folder named in `BMO_SECRETS`,
  read for the call alone. Never print one or put it in the answer: whatever the app writes has every secret's value
  masked.
- **So write it on what the machine has:** Python 3 and its standard library, or a shell script, or a program a
  `build` makes. A build runs confined the same way, its source read alone, with no network: nothing is downloaded
  during it, so whatever it needs is in the folder already.
- **Keep what it must remember in `/data`,** in files of its own; that folder survives every new version.

### What a call is

- **One process per call.** It is handed one JSON object on standard input — `protocol` 1, `kind` `"call"`, `entry`
  `"my/<name>"`, `arguments`, the object the input schema describes, and `session`, the record it is called under —
  and answers one JSON object on standard output, then ends cleanly:
  `{"content": [{"type": "text", "text": "..."}], "isError": false}`, with `structuredContent`, the object
  `output.json` describes, where it declares one.
- **A refusal the caller reads** is the same answer with `isError` true and one English sentence saying why. What
  it writes on standard error goes to the log.

```python
import json
import os
import sys

handed = json.load(sys.stdin)
arguments = handed.get("arguments", {})
notes = os.path.join(os.environ.get("HOME", "/data"), "notes.txt")

try:
    with open(notes, encoding="utf-8") as kept:
        text = kept.read()
except FileNotFoundError:
    text = ""

print(json.dumps({"content": [{"type": "text", "text": text or "No notes yet."}], "isError": False}))
```

### Tests

- **Its `check` runs them,** in the folder: each test runs the code as the runner does, a handed object on standard
  input, the answer read off standard output, `HOME` pointed at a scratch folder.
- **Run the check before you end,** and end only once it passes.

### When you end

- Answer with the app's name, what it does, the files you wrote or changed and the commit. Do not put it: the BMO
  that asked you puts it.
- Say whether you wrote a screen, and what it shows.
