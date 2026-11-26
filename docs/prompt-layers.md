# A supervisor's prompt layers

A supervisor's system prompt is made of layers, each whole and in a fixed order, so an agent of any
organisation is composed out of the same documents and told only what is its own. This document is the
canonical statement of the layers, who each one reaches and where the settings that shape them are read.
What each turn shape composes inside its role is `docs/agent-documents.md` (the window shape) and
`docs/supervisors.md`, 「What a turn is given」 (the pieces shape).

## The order

- **Six layers, in this order, each whole:**
  1. **the harness** — the base prompt the supervisor chooses, else the platform's harness opening, else none
     on a program whose own prompt is its harness (`docs/base-prompts.md`);
  2. **the platform's rules** — the platform's own operations, for the platform administrator's supervisors
     alone (「The platform's rules」 below);
  3. **the organisation's instructions** — what the organisations of the person the supervisor serves require
     of it, read off the delegation chain (「The organisation's instructions」 below);
  4. **the role** — the turn shape's role: on the window shape the chat's documents, rulebooks and disciplines,
     after the role prompt of an agent that holds a role; on the pieces shape the pieces role. Every role
     opens its rulebooks or its pieces with, or carries among them, the rule against destroying the system,
     `prompts/rules/self-preservation.md`, which binds every agent;
  5. **the person's instructions** — what the person the supervisor serves asks of their own agents, where their
     organisation lets them give any (「The person's instructions」 below);
  6. **the persona** — the supervisor's own, unless the delegation chain chose none for the person it serves
     (「The persona's choice」 below).
- **What comes from whom.** The harness and the platform's rules are the platform's, the organisation's
  instructions are supplied down the delegation chain from the company to each unit, the role is the preset every
  agent of it shares, and the person's instructions are the person's own layer over what their organisation
  supplies: they stand after everything the platform and the organisation set and take none of it away.
- **A layer with nothing to say is left out,** and no layer knows the others. A supervisor of an account still
  has its account's mark lead the whole prompt (`docs/agent-ownership.md`, 「What the supervisor's row
  carries」).
- **On a program that is a harness of its own,** dsh among them, the layers after the harness go as its
  persona suffix, in the same order.

## The platform's rules

- **The platform's rulebooks stand under `prompts/platform/`,** apart from the rulebooks every agent reads
  under `prompts/rules/`:
  - `environment.md` — the platform around the agent: the system the owner runs on his own machines, the
    owner at the keyboard of a bare line, its units with their stores, ports and token files and the shell's
    reach into them, the `bmo` command's verbs, the deploy trees and the checkouts, and the owner's routing of
    models;
  - `preservation.md` — the platform's own parts the rule against destroying the system protects in
    particular — its stores and data root, its deploy trees, its checkouts, its services, its configuration
    and key files, the brain repository, the model engines and the gateway — and what is not
    self-destruction on the platform;
  - `operations.md` — the platform's own work: the procedure documents and where they are read, whose work
    is whose between a repository's worker and the supervisor's own hands, the owner's machines and
    infrastructure, a road found by hand, putting a repository live and adding one to the live machine, a
    hosted tool made for a need that comes back, and a supervisor's turn its guard cut short.
- **They reach a supervisor of no account or of the platform administrator's account,** the runs that keep
  the whole machine (`docs/account-isolation.md`, 「The scope」), and no other. A supervisor of any other
  account is never given them, whatever its organisation's settings say.
- **Each turn shape names the rulebooks it carries,** in its own policy and in its own order: the window
  shape in `policy/chat-line.toml` (`[system] platform`), all three; the pieces shape in
  `policy/supervisors.toml` (`pieces_platform`), `environment.md` and `preservation.md`, since its role holds no
  part of the platform's operations for that rulebook to govern.
- **The worker line carries `environment.md` and `preservation.md` on every run,** after the rule against
  destroying the system, since its standing rules are the platform's own work (`docs/launch-lines.md`).

## What every agent is told

- **Every text an agent of any organisation is given names nothing of the platform's own:** the harness
  opening, the rule against destroying the system and the pieces role name no unit, path, token, store, port,
  repository or machine of the platform's. The harness opening says what any agent's harness is, how words
  reach it, where its memory is and how a hosted tool is found; the platform around it is `environment.md`.
- **A supervisor's own folder** stands under the platform's data root, so its path is told to a supervisor the
  platform's rules reach alone; any other is told it as the folder its shell starts in, in the words of
  `policy/prompt-layers.toml`.
- **The owner still stands in the role** as the authority the rooms mark (the absolute rules and the pieces
  role read his lines by his recorded sender), which every supervisor answering the platform's rooms needs.
- **The base prompt seeded for dsh** follows the harness opening: the platform's environment is not part of it.

## The organisation's instructions

- **A setting of the delegation chain** (the account service's 「The delegation chain」), the kind
  `agency.org-instructions`: a text, `layered`, held at the platform's link and at a unit's. The platform
  administrator writes the platform's, and a head of a unit writes that unit's and every unit's below it.
- **Resolved for the person the supervisor serves,** its owner account, as a person: every link's text up
  that person's chain, the outermost first — the platform's, their company's, each unit's down to their home
  unit. A supervisor of no account serves no person and is given none.
- **Composed as one layer:** the header and end line the policy holds (`policy/prompt-layers.toml`), and
  between them every link's text that says any, in the chain's order. The header says they bind every turn,
  whoever asks, and that the role and the persona work within them.
- **A link below never takes away what one above says:** every link above it is read whatever it holds. A
  lock at a link cuts the chain there, so no link below it adds any instruction of its own while the lock
  stands; what they held stays kept and unread, as the chain keeps everything below a lock.
- **One text for every role.** The layer reaches every agent serving the person, whatever its role, and each is
  told its role by the role's own layer after it; so a unit that asks something of one role writes it in its
  instructions naming the role, and no kind of its own holds a role's instructions. A role's prompt stays the
  platform's preset, the same for every organisation.

## The person's instructions

- **A person may give their agents instructions of their own:** one text for every agent of theirs, and one for a
  single agent by its name — their BMO, another supervisor of theirs or one of their role agents. An agent is given
  the text for every agent and then its own, under the header and end line the policy holds
  (`policy/prompt-layers.toml`), which say that they stand within everything before them and that one asking what
  those forbid is not followed.
- **Kept on the person's own link of the delegation chain,** the kind `agency.personal-instructions.text`: a JSON
  object, `{ all, agents }`, held at the person's link alone and written by the person. Agency judges it on its own
  route — each text at most the policy's characters (`policy/organisation.toml`), each name an agent of the
  person's — and writes it in the person's name (`docs/api.md`, 「own-agents」).
- **Whether the person may give them** is the kind `agency.personal-instructions`: a flag, `nearest`, allowed by
  default, held at the platform's link, a unit's and the person's, with rules, a head writing a unit's and a
  person's. A unit that sets it off with a lock decides it for everyone under it. Withheld, what the person wrote is
  kept and given to no agent, and stands again once it is allowed.
- **Never in place of the organisation's:** the organisation's instructions stand before the role whatever the person
  writes, and nothing of the person's removes or edits them.

## A role's agents of an account

- **A run seated on a role's agent of an account** (`docs/role-agents.md`, 「What a run reads off its agent」) on the
  worker line is given the same layers around its role's prompt: the organisation's instructions of the person the
  agent belongs to before it, and that person's instructions for every agent and for that agent after it. A run on
  an agent of no account keeps its role's prompt alone. Where the account service is named and does not answer, the
  layers this process last read for the account stand; with none read yet, the run is refused with the sentence of
  why.

## The persona's choice

- **A setting of the delegation chain,** the kind `agency.persona`: keys out of `own` and `none`, `nearest`,
  `own` by default, held at the platform's link, a unit's and a person's, with rules for a person, a group or
  a unit. A head writes a unit's and a person's; the person writes their own as well.
- **`none` leaves the persona out:** the supervisor keeps its name, which its frame or its own documents tell
  it, and its picture, its mark on a screen, and is given no tone or character instruction at all. `own`
  keeps the supervisor's own persona as its turn shape reads it.
- **A company holds `none` for everyone under it** by setting it at its link with a lock; nobody below
  changes it then.
- **Resolved for the person the supervisor serves,** as the organisation's instructions are; a supervisor of
  no account keeps its own.

## The settings agency declares

- **Agency declares the kinds of its layers on the account service** (`PUT /chain/kinds/{key}`, under the unit
  name `agency`) as its server stands, from the declarations `policy/prompt-layers.toml` holds — the organisation's
  instructions, the persona's choice, and the person's instructions with whether they may give them — and again wherever a
  resolution finds a kind undeclared. A process on a copy of the data root declares nothing as it stands. Its
  other kinds are declared with them, from `policy/organisation.toml`: the sign-up composition (`docs/role-agents.md`, 「The agents an
  account is given」), repository management (`docs/repositories.md`, 「Who may add and change a
  repository」), a person's own changes over the agents supplied to them (`docs/role-agents.md`, 「A person's own
  agents」), personal apps (`docs/person-nodes.md`, 「Personal apps」), the sharing of them (`docs/person-nodes.md`, 「Shared apps」), a
  person's own local AI (`docs/supply.md`, 「A person's local AI」) and the head check for the resources filed
  under a unit, `agency.unit-resources.manage` (`docs/api.md`, 「Who a caller acts for」). With them agency
  declares its own settings — the research supervisor `agency.research-supervisor`, the deploy queue's flush
  minutes `agency.deploy-flush-minutes` and the DeepSeek top-up's `agency.deepseek.card-secret` and
  `agency.deepseek.login-secret`, each read and written as `docs/api.md`, 「settings」 tells — and one
  `provider.<code>.offered` flag for every provider the build carries, which turns that provider's sources off
  below a link that says it (`docs/supply.md`, 「The sources a run walks」) — and a person's supply reads say so
  of it, naming the link, rather than leaving the provider out of the reads altogether
  (`docs/supply.md`, 「A closed provider is read as closed, not as absent」). A `json` kind's value agency judges on its own route and writes on the chain naming itself the
  kind's declarer.
- **Every value is read afresh** as a supervisor's prompt is composed — each turn, and each read of its prompt
  on the console — and as a run of an account's role agent starts, with `GET /chain/kinds/{key}/resolve?person=`,
  every kind side by side. A changed answer
  changes the prompt, which reaches the session as any moved prompt does (`docs/sessions.md`, 「Session
  replacement」).
- **An answer that says nothing** — a person the account service does not hold, or a kind that stays
  undeclared after declaring it again — leaves no instructions and the persona kept.
- **Where no account service is named,** there is no chain: no instructions, and the persona kept.
- **Where the account service is named and does not answer,** the layers this process last read for that
  account stand. With none read yet, the turn is refused with the sentence of why, so no turn of a person's
  supervisor goes out without its organisation's instructions; the console's read of the prompt shows none
  then.

## Tests

- **The proof is composed prompts:** a supervisor of no account and one of the administrator's account carry
  the platform's rulebooks and one of another account does not, on either turn shape; the organisation's
  instructions stand between the platform's rules and the role in the chain's order, and a lock above leaves
  a unit's below out; the person's instructions stand after the role and before the persona, the text for every
  agent before the agent's own, and none stands where the chain withholds them; a persona chosen `none` leaves no
  persona text in the prompt.
