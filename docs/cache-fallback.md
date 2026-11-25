# Model fallback and demotion

Agency launches every model run and reads back which model, provider and account it actually answered
on. Two things can make a run come back on something other than what it was launched on, and agency
carries both.

- The account safeguard **demotes** a run to a lower same-family sibling when it reads the session's
  accumulated context as material it must refuse. The demotion happens at the provider's side, at run
  time, with no advance signal; agency catches it after the fact by comparing the model a run reported
  against the model it launched.
- When an account answers nothing at all, agency walks the **account order** inside the Claude Code road
  to find one that will. It never leaves the run's one road for another provider or model
  (`docs/provider-order.md`).

No sanitiser reads the words that cross a session boundary agency owns
(「No sanitiser stands on a session's boundary」 below), so a passage the safeguard catches is answered
after the fact, by the demotion detection and its heal (「Demotion detection and self-healing」).

None of this is a turn-content decision. Which words a turn carries, which room or rule governs it,
which references it loads, and whether an answer is acceptable are the caller's (`docs/api.md`).
Agency's part is the boundary those words cross on the way into and out of the session it owns, the
model it launched, the account the run went out on, and the walk it runs when a login will not answer.

## No sanitiser stands on a session's boundary

- **Words go in as handed.** A round's new messages, the past-record block a first turn reloads, the
  reference attached to a turn or to a job's first prompt, the instructions a search or browse run
  passes with someone else's page, and the prompt a job worker is handed enter the session context as
  their caller handed them or agency composed them, translated where `docs/worker-language.md` says.
- **Nobody on the way asks a sanitiser to read them.** Agency runs no sanitising of its own, the job
  service hands agency the item rather than a prompt, and the gate's order holds no sanitiser (the
  gate's `docs/gates.md`, 「No gate stands」).
- **Words come out as written.** A session's answer, an alert, a caption and what a voice message speaks
  go back to their room as the session wrote them.
- **No verdict travels with the words.** No memo of read texts is kept, and a text carries no mark of
  having been read.
- **What this leaves to the safeguard:** the safeguard rereads a session's whole accumulated context on
  every run, so a passage that caught once stays in the record and can catch again on later turns. The
  demotion detection reads each such fall, and its heal discards the fallen session so the next turn
  opens on a context with nothing carried over (「Demotion detection and self-healing」).

## Demotion detection and self-healing

### The mechanism

- **A session is always launched on the model its road names,** or its repository's
  pin (`docs/provider-order.md`, `docs/repositories.md`, 「A pinned model」). A sibling is never chosen at
  launch; when the safeguard reads a session's context and refuses, the provider answers at run time
  with a lower same-family sibling instead. There is no flag that turns this off, and the run stream
  carries no advance signal.
- **So a demotion is caught after the fact:** if a run reports having spent tokens on a same-family
  sibling of the model it was launched on, that turn was demoted. The comparison target is the launched
  model itself, read from the same entry the run was launched on, so a model the owner changes on the
  console moves the check with it.
- **Not a demotion:** the small model a CLI uses for its own chores — titles, summaries — is a
  different family, and a run launched by a short model name cannot be told here what the CLI resolved
  it to, so neither is read as a fall.

### First catch: notify and heal

When a demotion first catches — when a healthy session first drops — two things happen.

- **Notify:** the owner's private chat is told which room dropped to which model. The room itself is
  not told. (Agency raises the fact; the supervisor is what tells the owner.)
- **Heal:** that room's session is discarded so the next turn opens a new one, which opens with
  nothing carried over and so restarts on a context the safeguard does not catch.
- **Once only:** the notice and heal happen on the turn a room started dropping, not on every turn it
  stays dropped, or a room that keeps touching the safeguard would be reset every turn to no effect.
- **The latch:** once dropped and then cleared, the latch releases, so the next fresh drop notifies
  again.

### Limits

- **The safeguard is probabilistic** — the same context does not catch on every run, but catches on
  most. Detection catches every run that actually fell, after the fact, so it works whatever the catch's
  probability.
- **A place where such work is discussed keeps needing healing,** since the detect-heal loop reverts a
  drop but a conversation that keeps returning to attack material keeps re-contaminating itself.

### Job workers stand on the same safeguard

- **Same position as a room:** a job's run goes on in the session its role kept at that path on that
  provider and model (`docs/sessions.md`, 「A role's session」), so it sits where a room sits. A job's
  prompt goes in as composed (「No sanitiser stands on a session's boundary」) and words already in the
  session transcript reload as they are, so one job can quietly answer on a sibling model and end —
  nobody the wiser, and the next job at that path inherits the same session.
- **One function for both:** detection uses one function for rooms and jobs — compare the model a
  run reported against the model chosen at launch; if tokens went to a different same-family sibling,
  it was demoted.
- **The one difference** is the comparison target: for a job it is the model that job launched. A
  job launched on one family that answered on another is not a fall.

#### Healing differs for a job

- **A room** opens its new session with nothing carried over, as every replaced session does.
- **A job** has nothing to hand over either — it brings its own instructions and needs no memory of past
  jobs — so a job's healing is only the other half: delete the refused session's record, and publish which
  repository's job, under which role, dropped to what as `worker.changed` (`docs/api.md`, 「Events」). The
  consumer that owns the owner's room reads that fact and explains it to the owner. The next job opens a new
  session.

#### A repository the owner keeps on its fallen model

- **The switch:** each repository row carries whether its jobs' downgrade heal is on. It is on by
  default; the owner turns it off per repository from the console's repository tab.
- **What off means:** the fall is still read and the owner is still told which repository's job fell to
  which model, but the session is kept and recorded as any answering session is. The next run of the same
  role at that path on that provider and model resumes it and goes on answering on whatever model the
  safeguard allows.
- **The record says it fell:** the kept session's record stays under the model it was launched on — the
  key every later run asks for — and carries beside it the model that actually answered.
- **A turn on it goes out on the fallen model:** every later turn that resumes that session — the next
  job, the next stage of a co-work, a fold — is launched on the model that answered, not the one its
  road or its repository's pin names, since launching that model again only puts the same
  context back in front of the same safeguard. A turn that answers on the fallen model
  keeps the mark; one that falls further marks the lower model.
- **The session's held state follows it:** the model and prefix held for the session are those of the
  model that answered, so the next turn on the fallen model is read as the same road and resumes rather
  than being replaced.
- **A fresh session carries no mark:** where a session is opened anew — a job's handover, a lost or
  expired record, a prefix that moved — it is launched on its own model again and falls only if the
  safeguard refuses it afresh.
- **Heal on ignores the mark:** a job in a repository whose heal is on launches on its own model
  whatever a record says, and a fall there drops the session as before.
- **Only a repository's own:** the switch is the repository's alone, and a room's heal is not touched by
  it.

#### When to delete

- **At the point a refused run's answer is received,** that session is deleted instead of recorded.
- **Not while a job runs:** deleting then would be undone, since the job writes its own session id again
  at the end. For the same reason a deleted session is not compacted.

#### What is not touched

- **A job's ending is unchanged** (the job unit decides its ending branches). Work done is done, work
  not done is not, and a session splitting underneath is not an attempt on the item.
- **Other sessions:** the role's sessions at other paths or on other models, and other roles' sessions,
  are left alone.

#### The cost

- **Deleting makes the next job fill its context from scratch,** so the only condition to delete is a
  run reporting a fall in its own spend — no clock and no reading of the transcript deletes a session.
  Only a run that was actually refused tells you its context is the problem.

#### Limit

- **A job's run can open subagents,** and a run's spend catches the models they used too, so a
  subagent explicitly given a different version of the job's own family is read as a demotion though it
  is not a fall. In practice agents pick models by family name, so the case does not arise, and even a
  misread costs only one new session — the answer is not wrong — where a job quietly answering on a
  lower model and ending costs more.

## When no account answers at all — the account order

- **Above demotion, one more layer.** Demotion is an account still answering, at a lower sibling. This
  layer is an account answering nothing at all: the run then goes to the next account of the same road.
  Once no account is left, the road cannot answer, and the run fails or waits as its road says; no other
  provider or model is walked to.

### Each supervisor and each agent stands on one road

- **There is no order of providers anywhere.** Every supervisor and every agent of a role stands on one road
  — a provider, a model, the effort and the thinking — and its runs go out on it alone
  (`docs/provider-order.md`). This document adds what happens inside the Claude Code road — the accounts — and
  what each provider answers for itself as a run starts.
- **The owner's switch on each provider stays one for the deployment:** a provider that is off cannot
  answer now, so no run goes out on a road of it, and turning it on again lets those roads answer. The last
  provider left on is not turned off (`docs/provider-order.md`, 「The owner's switch on a provider」).
- **The Claude Code road is itself a list of accounts** (「The Claude Code road is a list of accounts」),
  walked the same way whichever row stands on it.
- **Outside the console:** `bmo-agency agent --provider-on <provider>` and `--provider-off <provider>`
  change a provider's switch, and with no argument the switches show at the top of the listing.

### The Claude Code road is a list of accounts

- **Agency holds several Claude logins, and each is an account:** a name the owner gives it, a place in
  the account order, an on/off, and where its credential is reached.
- **What an account is known by is not what it is called.** Every account carries an identifier of its
  own that never moves, and that is what its sessions, its usage windows, its rest and the chosen
  account all name; the name is the owner's to rewrite at will, and rewriting it changes nothing the
  account already holds. Two accounts share neither, so a list never holds two that cannot be told
  apart.
- **A credential is reached one of four ways, each a kind of account that answers for itself:**
  - **A directory account** names an absolute configuration directory, whose login the CLI keeps in
    that directory's own credentials file.
  - **A token account** names a file holding the long-lived token the CLI issues, which is handed to a
    run in the environment. That token lasts a year, carries no refresh of its own, and is
    inference-only.
  - **An endpoint account** is Claude Code run against another provider's Anthropic-compatible
    endpoint: it names a configuration home of its own, which holds no login and keeps its sessions and
    transcripts, the endpoint's base URL, and a file holding the endpoint's API key. It may also map the
    family words `opus`, `sonnet` and `haiku` to the endpoint's own model ids. An endpoint account is
    paid by the request rather than by a subscription window, so it has no usage windows, and its key
    neither expires nor renews.
  - **An `api-key` account** is Claude Code on Anthropic's own API with an Anthropic API key: it names a
    configuration home of its own and the secret holding the key under the data root's `env/`, a person's
    `users/<user>/<name>`, an organisation unit's `units/<unit>/<name>` or a plain name for the platform's. It is paid by the request, so it has no usage
    windows, and its key neither expires nor renews.
- **What is kept is where a credential is, never a credential.** No token, no key and no file body
  enters the store, a log line, a document or a report; the reader that opens a credential file keeps
  only the moment it expires, and an endpoint account's key file is read only as a run is launched.
- **The account is handed over in the environment, with one exception in a flag.** Every line's flags,
  its system prompt and the tools it is shown are what they were, so a run moved to another account
  stands on the line it always stood on and only the cache it is warm in changes.
  - A directory account is handed its configuration home, and any token the launching process carried
    goes with it, since a token left in place would answer for the run instead of the login the owner
    chose. The one token it is handed is its own token for the work nodes, beside that home, while it
    carries on by that token (「A login that is about to end」).
  - A token account is handed its token alone.
  - An endpoint account is handed its configuration home, its base URL, its key as the run's
    authorisation token, and the model ids its map names for each family; every other credential the
    launching process carried goes with it.
  - An `api-key` account is handed its configuration home and its key as `ANTHROPIC_API_KEY`, read from its
    secret as the run is launched; every other credential the launching process carried goes. A person's, and an
    organisation unit's carrying a person of its path, goes out here or on that person's own node, carried by the
    link with the run's variables; the platform's never goes out on a node, since its key stays on this machine.
  - A person's directory account signed in on their node is handed the configuration directory there, and no
    token (`docs/supply.md`, 「Signing a person's login in」).
  - **The exception:** an account may name a base prompt (`docs/base-prompts.md`), and a run on the worker
    line on that account opens its system prompt on that base in place of the platform's harness opening, so
    on the worker line the file the system-prompt flag names is the account's (`docs/launch-lines.md`). An
    endpoint account names the base its endpoint's own harness runs on, since that is the opening the
    endpoint's model runs best on.
- **An account carries its own everything.** Its sessions, transcripts, usage windows where it has
  them, spend and rest belong to it, because a login is what they belong to: a session opened under one
  account cannot be resumed under another, one account's windows say nothing about another's, and each
  account's cache is warmed on its own.
- **An account that refuses a run rests alone, and the run goes out on the next account.** A run that comes
  back with its account's own refusal — the CLI asking for `/login`, or the API answering 403, the firewall
  in front of it among them — passes that account over the way a full window does:
  - it is marked as waiting for a person and the owner is told once (「A login that is about to end」);
  - where its credential would lift that mark at once — a directory or token account whose file states a
    moment still ahead — the account also rests for the span the Claude Code provider's policy names
    (`policy/providers/claude-code.toml`, `[login]`), written under it alone by the one writer of rests, so
    no other account rests for it and the credential's own moment cannot open it again before the rest
    ends. A credential already past its moment, and an endpoint account's key, hold the mark until a person
    signs in or writes the key again, so they take no rest;
  - a directory account that the refusal hands to its token for the work nodes (「A login that is about to
    end」) takes no rest and is not passed over: the same run goes out again on the same account, on that
    token, once, and its log says so;
  - otherwise the same run goes out on the next account of its walk. A job's log says which account refused and in
    its own words, the job keeps the account it moved to among its roads with that refusal as the reason
    (`docs/api.md`, 「assignments and jobs」), and its prompt on the next account opens with the line saying
    it continues work already in the checkout, since the session it was on belongs to the account that
    refused.
  - Only where every account of the walk refused does the run end, on the login failure naming each account
    with its words and saying a person must sign in to it again.
- **An account whose login is near its end is renewed before a run goes out on it, never during the run,**
  unless the run goes out on its token for the work nodes, which renews nothing.
  As the walk reaches such an account, the run renews the login in a short step of its own: the one run that
  takes the account's lock renews it with one run of the CLI and lets the lock go before its turn goes out,
  and every other run on the account waits only for that step, saying so in its log as it starts waiting
  and as it goes out. A renewal refused on the login is that account's refusal for each of those runs, so the
  walk carries them to the next account as above (「A login that is about to end」).
- **A road names the provider; the Claude Code road holds the accounts.** The account order is one list for
  the deployment, and it is what every run on the Claude Code road walks.
- **An account belongs to a person or to the platform:** its `user_id` names the person, or none for the
  platform's. A person's run on personal supply walks that person's own accounts alone, in the list's order; a
  person's run on a platform source walks the accounts marked lendable alone, whoever owns them, save that
  person's own, and none is marked until the administrator marks one (`docs/supply.md`, 「The lendable
  accounts」). An account of the administrator's is the owner's own personal supply (`docs/supply.md`, 「Owned
  supply」): a run of no account or of the administrator's walks it with the platform's accounts, lent or not,
  and it goes out for another person's run only once it is marked lendable.
- **A person's account is made by that person alone:** an account added with a signed-in person's headers is
  pinned to that person, and a person reads and changes their own accounts alone (`docs/api.md`, 「Who a caller
  acts for」).
- **The account order is each account's own place and on/off, held together,** so the two cannot
  disagree and a turned-off account keeps its place. An account order is refused unless it holds every
  registered account once each with at least one on.
- **With nothing said, the order is the order the accounts were added in, all on** — what a fresh
  installation has and what the owner resets it to. There is no stored default to remove, since the
  accounts are the owner's own list from the start.
- **A fresh installation has one account,** standing for the login the machine already holds, so
  nothing answers differently until a second is added.
- **Outside the console,** an account named by its id or its given name:
  - `bmo-agency agent --account-add <name> --path <dir|file> [--kind directory|token|endpoint|api-key]` adds an
    account at the end of the order, on; an `api-key` account also takes `--key-file <secret>`, its path being
    its configuration home; an endpoint account also takes `--endpoint <url>`,
    `--key-file <path>`, `--models opus=<id>,sonnet=<id>,haiku=<id>` and `--base-prompt <name>`, its
    path being its configuration home
  - `bmo-agency agent --account-set <account>` changes the same fields, `--base-prompt none` letting
    the account's base go, and `--account-remove <account>` removes an account that is not the last
  - `bmo-agency agent --account-order <account,…>` sets the order
  - `bmo-agency agent --account-on <account>` and `--account-off <account>` change on/off
  - `bmo-agency agent --account-reset` puts the order back to the default
  - with no argument the account order shows beside the providers' switches, marked where it is still
    the default

### The login in use is used to the end

- **One login is in use, for the whole road.** Every run on Claude Code — a supervisor's turn, a role's
  run, a session opened after a restart — starts its account walk on that login for as long as it can
  answer, behind the chosen account while that one can answer, and
  ahead of the order.
- **A login coming back moves nothing, except the chosen one.** An earlier account answering again is no
  reason to leave the login in use; the road moves only when that login stops answering, when the owner
  chooses an account, or when the chosen account can answer again (「Running on one account now」). A
  session and its warm cache belong to the login that opened them, and the owner spends one account's
  window before the next, so moving off a login that still answers throws the sessions away for nothing.
- **What is kept:** one setting naming the login in use, so a restart or a deployment starts every run
  where the road stood. It is written by the first run that answers while no login is in use or while
  the one in use cannot answer, and by the owner's choice.
- **A login in use that cannot answer is walked past like any other** — off, removed, at its limit or
  waiting for a person — and the walk goes on from the chosen account and then the order from the top;
  the login that answers there becomes the one in use.

### Running on one account now

- **The owner can name the account the road runs on now,** from a row on the console or with the command
  below. That account is the chosen one, and choosing it is a switch: it becomes the login in use at once,
  so the next run of every supervisor and role on Claude Code goes out on it. The chosen account heads every
  account walk whenever it can answer; while it cannot, the login in use answers where it can, then every
  other account that is on, from the top.
- **The choice leaves the order alone.** It is one setting beside the accounts' rows, naming one account
  by its id; neither the order nor any switch is rewritten to express it. With nothing chosen the walk
  is the account order as it stands.
- **Only an account that is on can be chosen.** Choosing one that is off is refused; turning the chosen
  account off, removing it or resetting the order lets the choice go with it.
- **A chosen account that cannot answer is walked past like any other:** it is not the account serving
  now — the first open account is — and the account that answers becomes the login in use. The choice
  stays, and once the chosen account can answer again — its windows reopened, its login signed in again
  — the next run goes out on it; a run already going finishes where it went out.
- **What moves, and when.** The choice changes the head of the walk and nothing else, so everything that
  already happens when the head moves happens at the next run, not at the click: a run already going
  finishes on the login it went out on; the next chat round asks the receiver for the old session's
  handoff, then closes it and opens a fresh one on the chosen login (`docs/sessions.md`, 「A restart
  hands the session over」); the next role run goes out on the chosen account, resuming the role's
  session kept under that login where one is valid, or opening one (`docs/sessions.md`, 「A role's
  session」). A turn on a session already held stays on that session's own login, since no other login can
  reach it.
- **Outside the console:**
  - `bmo-agency agent --account-use <account>` chooses an account by its id or name
  - `bmo-agency agent --account-use auto` lets the choice go, back to the order
  - with no argument the listing names the chosen account and the one runs go out on now

### A login that is about to end

- **A login does not last.** A directory account's login is an OAuth pair the CLI renews inside its own
  run; the long-lived token an account may point at instead stands for a year and then simply dies,
  carrying no refresh. No command renews either from outside a run.
- **The moment a credential states is not the login's end.** A subscription login's file states when
  the access token in it stops being accepted, about an hour away at any moment, which the CLI renews
  for itself out of the refresh token beside it. The two questions are kept apart: the moment the access
  token stops is what a renewal is timed against, and the login's own end is what a person is told about.
- **What does end:** a long-lived token, which holds no refresh and dies a year after it was issued;
  and a subscription's sign-in, whose refresh stops being taken at a moment renewing does not move. Where
  the file states when its refresh stops — a moment under a name that speaks of refreshing and of
  expiring — that moment is the login's end; a file that holds something to renew with and states no
  such moment is a login with no end this side can read. The sign-in may still end before the moment
  its file states, which no file says and only a run that went out on it can tell.
- **Only moments are read, never the credential.** A credential file is opened to learn three things —
  when its access token stops, when its refresh stops where it says, and whether it holds anything to
  renew with — and nothing else. No token and no part of the file's body enters a row, a log line, a
  document or a report. All three are looked for as shapes rather than fields, since the file's shape is
  the CLI's and free to move. A token file states no ending, so what stands for its year is the day the
  file was written.
- **A login whose file stands but states no end this side can read is left standing and nothing is said
  about it;** a login with no file at all is not held (「The account judgement」).
- **The account list tells each login as the account judgement holds it:** `none` where this machine holds
  no login for the account (「The account judgement」), so a CLI that was never signed in is never shown as
  signed in; `gone` where it waits for a person or is past its end; `ending` inside the notice window; and
  `standing` otherwise. A login held nowhere here has no end to tell of, so no ending notice is said for it.
- **A renewal is timed against the CLI's own margin.** The CLI renews a token only as a run goes out
  inside the last minutes before the moment its file states, and passes it by before them, so a login
  counts as near its end only inside that margin, which the provider's usage declaration names.
- **One renewal, not several, and never across a turn.** Runs of one account share that account's
  configuration directory, and two of them renewing at once would leave each holding a credential the
  other has replaced. So a run going out on a login near its end renews it first, in a short step of its
  own and not in its turn. The run that takes the account's own lock renews the login with one run of the
  CLI on it whose only purpose is that renewal, the same run the usage reader asks for
  (`docs/usage-windows.md`, 「The Claude account reader」), and lets the lock go before its turn goes out.
  No run holds the lock across a turn, so a run that finds the lock held waits for a renewal of seconds,
  never for another run's work.
- **A waiting run waits for its event, not for a span:** it goes out once the credential file shows the
  renewal written or the holder lets the lock go, the holder's process ending among the ways. A run that
  waits says so in its log as the wait starts and again as it goes out (`docs/run-records.md`, 「Agent
  log」).
- **A renewal that fails on the login ends the runs waiting on it.** The holder records the refusal
  beside the account's lock, against the moment the credential it could not renew states, before it lets
  the lock go. So it, every run that waited on it, and every run after them while that credential stands
  end as that login's failure without another renewal, and the walk marks the login, tells the owner once
  and carries each run to the next login. A credential written since, by a person signing in again,
  states another moment, and the record no longer holds. A renewal that meets a full window rests the
  login as a turn would. A renewal that fails any other way is said, and the run goes out regardless, its
  own CLI renewing as it goes out.
- **A long-lived token removes the renewal:** an account pointed at one needs no browser for a year, so
  none of the above happens for it; it is warned about before that year is out like any other account. A
  run that goes out on a directory account's token for the work nodes (below) renews nothing either.
- **A renewal run that the login refuses marks it.** The run the usage reader asks for, refused on its
  login, marks the account as waiting for a person and tells the owner once, as a turn's refusal does, so a
  sign-in that ended before its stated moment is found by the reader's next ask and not only by a turn;
  a login already marked or past its end is not run on for a renewal at all.
- **An endpoint account has no login to end.** Its key states no ending and is renewed by nobody, so it
  takes no renewal lock, is never warned about, and is marked as waiting for a person only when the
  endpoint refuses its key or its balance (「Automatic walks only on providers, in order」).
- **An account's token for the work nodes is a login of its own** (`docs/work-nodes.md`, 「The logins on
  a node」), counted and warned about separately; a node login found gone is marked for the nodes alone,
  and the account's login here is neither marked nor passed over for it. It is also what a directory
  account carries on by here once its login here ends (「A directory account carries on by its token」).
- **A login that has ended is marked, and the mark is the walk's instruction:** the account's row says
  it waits for a person, the walk passes it over, and its work moves to the next account; with none left
  the road cannot answer. A directory account that carries on by its token is the exception: the mark
  stands, and the walk offers the account on that token.
- **The mark is lifted by the credential, not by anybody's word.** Signing in again writes a moment in
  the future, and the walk's own reading of that credential takes the mark off, so no console action is
  needed to put the account back in the order. An endpoint account's mark is lifted by its key file
  being written after the mark, or by the owner changing the account, since a balance topped up at the
  endpoint leaves nothing on this side to read. A subscription's file states a moment ahead at almost any
  time, so the mark a run's refusal writes there is lifted at once; the rest written beside it (「The Claude
  Code road is a list of accounts」) is what keeps the account out of the walk, and it ends at its own moment.
  An account that carries on by its token takes no such rest: its mark lifts only off a credential written
  after the mark, by a person signing in again, so a file that still states a moment ahead neither sends the
  next run back to the login that refused it nor tells the owner again.
- **The owner is told before it ends, once — and never about a login that renews itself with no end
  stated.** Agency publishes `account-login-notice` (`docs/api.md`, 「Events」), carrying the account, how
  long is left, and the one command the owner runs to be done with it, with that account's own
  configuration directory in front of it; the consumer that owns the owner's private chat says it there.
  The command is passed over verbatim rather than reworded, because a command reworded is a command that
  does not run. Where the account carries on by its token, or will once its login ends, the notice says
  so in place of saying its work moves to the next account, with the day that token's own year ends.
- **Once is per ending, not per login.** The warning is recorded under the moment it is about as it is
  published, so a later pass over the same ending publishes nothing; a login signed in again is a new
  moment and is warned about again. Delivering it is the consumer's.
- **Recognising that a login needs a person rests on more than one word** — the run's ending read
  through the program's own failure reading, and the credential's moment — asked together in one place,
  with a credential plainly past its end enough on its own.
- **The watch runs on the keepalive pass** (`docs/keepalive-schedule.md`), so the marking, lifting and
  warning happen without anything being launched for them. It watches directory and token accounts; an
  endpoint account has nothing for it to read. Its wording, how long before the end it speaks and the year a
  token stands for are read from agency's policy (`policy/`).

### A directory account carries on by its token

- **A directory account whose login here cannot be counted on runs here on its token for the work nodes.**
  Its login here cannot be counted on at three moments: it has ended, past the end its file states; it is
  marked as waiting for a person, by a run's or a renewal's refusal or by the watch; or it is within the span
  before that end the Claude Code provider's policy names (`policy/providers/claude-code.toml`, `[login]`), so
  no turn goes out on a login that would end under it. At each of them, where this machine holds that
  account's token for the work nodes (`docs/work-nodes.md`, 「The logins on a node」) and that token is not
  marked gone, every run here on the account — a supervisor's program, a job's turn, a role's run — is handed
  the token beside the account's configuration directory.
- **Beside the directory, never in place of it.** A program handed `CLAUDE_CODE_OAUTH_TOKEN` signs in with it
  ahead of any login its configuration directory holds and writes no login there, so the account's sessions,
  transcripts and settings stay where they are, and a session kept under the account resumes on the token. A
  program standing on the login is another program than one on the token, so the next turn into a standing
  program stands it again, between turns, on what the account is handed then, as any change of its login does.
- **The account stays open.** The account judgement counts an account that carries on by its token as a
  login that answers, mark and all; its windows and rests close it as they close any other account. A refusal
  on its login here writes no rest for it, and the run that met the refusal goes out again on the same
  account, on the token, rather than on the next account.
- **A token that fails here is the token's failure.** A run here handed the token that fails on its login — a
  job's turn, a supervisor's turn into its program — has found that token gone, as a run on a node does: the token is marked gone, for here and the nodes alike, the
  owner is told once with the command that makes it again, and the run is carried on as a node token's
  failure is.
- **It goes back to the login by itself.** A person signing in again writes a credential, after the mark,
  whose moments stand ahead: the mark lifts off it, its end is no longer within the span, and the next run is
  handed the directory alone. Nobody's word is needed.
- **What the token cannot do.** It is inference-only, so every surface here that needs the account's full
  login does without it meanwhile:
  - **the usage read** (`docs/usage-windows.md`, 「The Claude account reader」): the provider's usage endpoint
    refuses the token for its scope, and the login here is not renewed for the reader once it has ended or is
    marked, so the reader cannot ask for the account's windows. They come instead from each turn's answer:
    the status line of a program on the token carries the account's `rate_limits` as one on the login does,
    and they are kept under the account as any turn's are;
  - **the renewal step** before a run (above): a run on the token renews nothing, so it neither takes the
    account's renewal lock nor waits for it;
  - **remote control and the browser-driving surface** of the CLI refuse the token. No line here asks for
    either: the CLI's browser integration is off unless a run asks for it and none does, the browse role
    drives its browser through its own chrome-devtools server, and nothing here starts remote control. A
    person who starts either by hand on the account meets the refusal until the account is signed in again;
  - **the console's sign-in** (「Signing an account in from the console」) never runs on the token: it starts
    the CLI with no token of the launching process, so it signs the account's own login in as it always
    does, and that sign-in is what puts the account back on its login.
- **The token's own year is watched.** The token holds no refresh and ends a year after it was made, and only
  a browser approval makes another, so the owner is warned well ahead of that end (`docs/work-nodes.md`,
  「The logins on a node」).
- **An account with no such token, or whose token is marked gone, has nothing to carry on by:** its login's
  end, mark and refusal are what the rest of this document says, and its work moves to the next account.

### Signing an account in from the console

- **A directory account is signed in through the CLI's own sign-in, driven from here,** so the owner needs
  no terminal on this machine. Agency starts `claude auth login` with the account's configuration directory as
  the CLI's, no browser of its own, and no token or key of the launching process; the CLI prints the address of
  the subscription sign-in and waits for the code that page shows. Agency hands the address back, the owner
  approves the sign-in in their own browser, signed in to the account it is for, and pastes the code; agency
  writes the code to the CLI, which ends once it has written the login into that directory. Nothing of the
  OAuth exchange is agency's: the CLI performs it, and no part of it is read.
- **A person's sign-in runs on that person's node** (`docs/supply.md`, 「Signing a person's login in」): the
  same CLI and the same exchange, started in the node's herdr session, its directory under the node's data
  root, and the account added pinned to that person with that node named.
- **It signs in an account that is there, or one that is not yet:**
  - an account that is there is a directory account, signed in again in its own directory, where its login
    has ended or is about to; a token or endpoint account keeps no login of its own and is refused;
  - an account that is not yet is named, or named by agency where the start names none: `claude-NN`, one past
    the highest number among every account's names, the people's among them, which the owner's own list does not
    show, the next number taken where that name's directory already holds anything. A name another account holds
    is refused 409 `name-taken`. Its directory is made under the data root's env directory, named by the account's
    name, readable by this user alone; a name whose directory already holds anything is refused. The account is added, at the end of the order, only once the CLI has left a login there, so a
    sign-in abandoned leaves no account behind; the directory it made, which the CLI writes its own files into
    as it starts, is taken away whole as that sign-in ends, given up, refused or ended for standing too long, so
    the same name may be tried again.
- **A sign-in is held in this process alone and for a bounded while.** It is known by an identifier of its own,
  answered as it starts; the code is taken by that identifier. One left standing longer than the provider's
  policy says is ended, as is one the owner gives up, and the process ending ends every one: the CLI is ended
  with it, and an account not yet added is not added. Waiting for the address and for the CLI's ending after
  the code are each bounded by the same policy.
- **The code is the owner's word and is kept nowhere.** It goes to the CLI and nowhere else: no row, log line or
  answer carries it, and a refusal the CLI gives is answered in its last words with the code taken out.
- **The login is the CLI's to make, and its file the proof.** The CLI ending well with no credentials file in
  the directory is answered as a sign-in that did not take, and nothing is added.

### Availability: whether a provider can answer now

- **A road sets only which provider and model, and the owner's switch only on and off.** Whether a provider
  that is on can answer now, each provider answers for itself, out of its own member. It cannot
  answer for one of these reasons, and can otherwise:
  - **no credential on this machine** — the presence its member reads (`docs/provider-presence.md`):
    Codex's executable, OpenRouter's key, the gateway's key file and an offered registration — one with
    an address or one naming a card service, whose engine standing or stopped at that moment is no part of
    its presence (`docs/own-llm.md`, 「An engine the arbiter stands」) — DeepSeek's key file; Claude Code's
    credentials are its logins, below;
  - **its balance spent** — where it offers a balance, the one its reader last kept for its own road is
    at or below zero (`docs/usage-windows.md`, 「Balances」); a balance never read closes nothing, and a
    spent one closes no run on a model the provider says costs nothing (`docs/provider-order.md`, 「Quota:
    what a provider has left」);
  - **no login open** — where it holds logins, none of its logins is open by the account judgement
    below, each account's own reason beside it.
- **A provider whose program does not carry a row's line is never that row's road**
  (`docs/provider-order.md`, 「What a road is」), so availability is asked only of providers that carry
  it. Whether a provider has quota is asked beside availability, never in place of it
  (`docs/usage-windows.md`, 「What reads a provider's quota」).
- **A road whose provider cannot answer now takes no run,** and says its reason; an account the judgement
  closes is passed over inside the Claude Code road.

### The road judgement

- **One place judges a run's road:** the road judgement (`docs/provider-order.md`, 「Whether a road can
  answer」). It is asked for a supervisor's turn or a role's run as the run starts, and answers whether that
  run goes out now on its road — the provider, the model, the effort and the thinking the road names and,
  where the provider holds logins, the account — or why it cannot: no quota left, or unable to
  answer now (「Availability: whether a provider can answer now」).
- **The account.** Where the provider holds logins, the first login the account judgement offers
  (「The account judgement」), the chosen account and the login in use heading the walk.
- **Runs on another's road** — a repository's pinned model, a run of an agent that names no road on the road of
  the supervisor that asks it (the `coder` role as seeded), a session whose caller names its provider and
  model — are `docs/provider-order.md`'s, 「Runs on another's road」; the account walk inside the Claude Code road
  is the same for each of them.
- **Every line asks it, and none keeps a judgement of its own:** a supervisor's turn, a job's launch and its
  relaunch, every other role's run, and the warming, which warms no provider that cannot answer now. The
  judgement knows no provider by name: every provider's answer is its own member's.
- **A road that cannot answer is said, never stood in for.** A supervisor's turn fails at once with the
  sentence; a job waits, held with it, and goes out once the road can (`docs/provider-order.md`, 「Whether a
  road can answer」). The sentence names what a person must do where only a person can reopen the road: an
  account waiting for a person, the sign-in command with that account's own configuration directory in front of
  it, as the login notice words it; a spent balance, where its provider's member says it is topped up. A road
  that only waits for a window names the moment it reopens.

### The account judgement

- **One judgement says whether a login answers now, and every Claude run is offered a login by it** —
  every supervisor's turn, fold and warm ping, and every role's run, search and browse among them. No line
  keeps a judgement of its own, and none falls back to the machine's own login in place of one.
- **A login is open while all four hold:** it is on; it holds a login on this machine; no window of its own
  is full (neither a rest standing under it, a limit's or a refusal's, nor a reading of its own windows showing
  one full that has not reset — `docs/usage-windows.md`); and its login has not gone (「A login that is about
  to end」), or it carries on by its token for the work nodes (「A directory account carries on by its
  token」). A login that waits for a person and has no such token to carry on by is closed.
- **A login is held where its credential file stands, a file and not empty:** a directory account's
  credentials file in its configuration directory, a token account's token file. The file is only looked at,
  never opened. One that holds none — a directory the CLI was never signed in to, the machine's own login on a
  fresh installation among them — is closed as never signed in, and the road's sentence names the sign-in
  command with that account's own configuration directory in front of it, as for a login waiting for a person.
  A person's login signed in on their node stands there and nowhere here, so it is not looked for here.
- **A run on a model asks one thing more: that model does not rest on that login.** A model's rest is one
  login's and one model's, so a login whose model rests is closed to runs on that model alone and stays
  open to every other model, and its rest never counts toward the deployment's rest
  (`docs/api.md`, 「Accounts」). It closes the model until its moment; one written while nothing named a
  reset closes it until a reading of the login's windows is seen after it was written, and never longer
  than such a reading stands, so the next reading decides. A line that asks for a login without naming a
  model, the warming among them, reads no model's rest.
- **An endpoint account is open while it is on, its key file is present and not empty, no rest stands
  under it, and the balance last read for it is above zero.** It has no windows to read, so only a rest
  written from a refusal or a spent balance closes it, and only a refusal of its key or its balance marks
  it as waiting for a person; a balance never read closes nothing.
- **A window read full closes its login** whether or not any run has met the refusal yet. The windows are
  read on a clock from the account's own login, so a login spent elsewhere is closed by the reading, and
  a rest removed by hand does not reopen a login whose window is still full. The administrator's end of the
  deployment's rest (`docs/api.md`, 「accounts, rest and usage」) is the one exception: it sets the full windows
  kept for the platform's own logins aside with their rests, so they answer again at once, and the reader's
  next reading of each login closes it again where its window is still full.
- **A full window that names its reset closes its login until that reset, however old its reading,**
  since nothing but a newer reading can say it came back early: a newer reading that finds it short of
  full opens the login at once, and the reset opens it on time. A login spent until a day later
  therefore stays out of every walk until then, even where its token has lapsed and the reader can no
  longer ask for it.
- **A full window that names no reset closes its login only while its reading is fresh,** a bounded
  window, since a quota can come back and nothing tells this side but the next reading; past that it
  closes nothing, so the walk never leaves a login shut forever on a reading no one can refresh, and the
  next run settles it — one that answers renews its own token and takes a fresh reading, one still spent
  meets the provider's refusal, which rests it as any refusal does.
- **A rest written from a run's refusal or its own usage is not a reading:** it closes the login until
  its moment whatever the reader later finds, so a login a run found spent is never sent another run
  before the refusal's own moment. A done run's usage reported to agency rests the
  login only where a window of it reads full, all of it used (`docs/api.md`, 「accounts, rest and
  usage」); a window short of full rests nothing, and the login takes runs until its provider refuses it.
- **A closed login is never offered.** The walk offers the open logins in its order and none other.
- **A run none of whose logins answers goes out on none.** It ends without launching anything, and
  whatever receives that failure reads it as it reads any limit or lost login, beginning no rest, since
  the run went out on no login.

### Automatic walks only on providers, in order

- **The Claude turn:** if Claude Code is on and its turn is reached, the chosen account answers where it
  can, then the login in use (「The login in use is used to the end」); otherwise the account order is
  walked from the top and the first open account answers. The Claude Code turn is closed only when every
  account is.
- **A run that fails on the account rather than on the work** — its limit reached, its login gone or
  refusing it — moves to the next open account before the walk leaves the Claude Code turn. A job's log
  and its roads say which account it left and why; any other work that asked never learns the first account
  was tried. A failure of the work itself moves nothing, and neither does the service being down, since no
  other account would answer that either. A login passed over for its limit or its refusal is put to rest
  as it is passed, under that login, so nothing that reads the failure afterwards writes a rest under
  another. Where a limit, a window read full or the outage closes the last open login of the platform's own
  work, agency publishes the silence once as `rest.changed` (`docs/api.md`, 「accounts, rest and usage」), and
  the consumer holding the owner's chat says it; a refusal's rest is told as the login notice tells it
  (below), never as a limit.
- **What rests for a limit is what the answer and the windows say reached it,** read in this order:
  - **One model's own limit.** An answer that names the run's own model as the limit reached — Claude
    Code's "You've reached your Fable limit" on a run of a Fable model and its kin, the name being a word
    of the model's id — rests that model on that login and nothing else: the login stays open to every
    other model, and the run moves to the next login that carries the model. The rest runs until the reset
    the answer names, else until the soonest reset the login's windows name, else until the next reading.
  - **The login's own limit.** An answer that names its reset rests the login until then. One that names
    none rests the login only where the windows confirm it, a window of the login's own read full
    and not reset, until that window opens (「The account judgement」).
  - **Neither.** A limit the windows do not confirm, and that names no model of the run, never rests the
    login: the run's model rests on that login until the next reading decides, as one with no reset does.
  - Each rest is said once in the machine's log, naming the login, the model where it is one model's, and
    until when, or that the next reading decides; the run's failure carries the same, so the job it ends
    says which model on which login rests and until when, and so does every run its rest holds back.
- **An endpoint account is walked like any other, and its refusals are read as account failures.** The
  endpoint answers in HTTP status codes rather than in a subscription's words:
  - a 401, the key refused, is its login gone, so the account is marked as waiting for a person;
  - a 402, the balance spent, is marked the same way, since only the owner topping the balance up
    opens it again;
  - a 429, too many requests at once, is a limit that names no reset, so the run's model rests on the
    account for a short while, no longer than a reading of a login's windows stands, since no reader asks
    an endpoint account;
  - in each case the work moves to the next open account, as from any login.
- **A login gone, or its token for the work nodes, is told the way the login watch tells it:** marked as
  waiting for a person and published once as `account-login-notice`. Agency says nothing in a room
  itself.
- **A supervisor's program left on a closed login is taken down.** The loop in a supervisor's pane starts
  the program its place holds again on the login written into it, so the login watch takes down every
  supervisor's program whose login the judgement closes, unless a turn is in it; the supervisor's next turn
  stands its program again on the login the walk offers then. A program that names no login of its own is
  the machine's and is left alone. A role's run keeps no program between runs, so nothing of a role is
  left standing on a closed login.
- **Which of the two a failure is, is one judgement,** read off the run's ending through the program's
  own failure reading, in one place every run asks, so no two lines read the same failure differently.
- **A line that spawns once takes the first open account rather than walking.** Search and browse runs
  are each one process given the account their run is offered first, and a pieces-shaped turn on Claude
  Code takes the first open account the same way. Everything that can be launched again is carried
  account by account.
- **Never walking off the road:** a run whose road has no quota left, cannot answer now, is not
  run or ends with no answer goes out on no other provider or model (`docs/provider-order.md`, 「A road that
  fails」). With Claude Code off, a supervisor whose road is on it fails its turns and a job on it waits.
- **Who receives that failure, each in its own place:**
  - **A supervisor's turn** ends failed with the failure, naming its road (`docs/supervisors.md`, 「Where a
    supervisor's turn runs」); a failure that is the work's own — the provider refusing the request for its
    shape — ends it failed alike. A turn stopped from outside ends with the stop, a wait for a card included.
  - **A job waits for its road rather than failing.** A job whose turn fails for its provider's availability —
    the provider refusing its quota, its window, its balance or its credential, its last open login closing
    under it, or the provider it went out on found unable to answer as the turn ends — waits for its agent's
    road, held with the reason, and goes out again on it once it can: the same job, item and lease, in the same
    checkout, its prompt opened with a line saying it continues work already in the checkout. A provider that
    answered it cannot serve now — a 5xx such as a service bench's 503, or a refused connection — has its
    answer kept as the road's block, and the job waits for that block to open, its sentence in `heldFor`,
    however long that takes, then goes out again, keeping the session it had, and no other provider or model is
    tried. It is never failed for that. A failure of the work itself moves nothing and ends the job as it stands.
  - **A job every account of which refused it ends on that refusal.** Where its turn ended on the login
    failure of every account of the Claude Code walk refusing it, the job ends failed with that failure,
    naming each account a person must sign in to, rather than holding its seat and its checkout until the
    refusals' rests end.
  - **A blocked road stays passed over for a while.** A road a job's turn leaves for its provider's failure is
    blocked, and the harness blocks one through the boundary (`docs/api.md`, 「road-blocks and launch-room」);
    a run of a role that could take any free agent passes a blocked agent's road over while another agent's
    road can answer (`docs/provider-order.md`, 「A road that fails」) — a minute for an outage, five for a
    refused access, fifteen or until its reset for a limit; a road a run names alone is never passed over for
    a block.
    On a provider whose models are services of their own, that road is the one registration that did not
    answer, and one registration going quiet leaves the others open (`docs/own-llm.md`).
    - **A turn stopped from outside closes no road:** a turn that ends without an answer while its job has
      been stopped is the stop, on every road.
    - **A request the road refused closes no road:** a registered model's turn that ends on a 400, 413 or
      422 — a conversation grown past the registration's window among them — is the backend answering the
      request, not the road gone (`docs/own-llm.md`, 「A turn at the input limit」).
    - **A registration closed for an outage is asked again before it is passed over:** a run's walk asks
      the engine behind the registration for its health, and an engine that answers opens the road again
      at once, while a registration whose engine gives no health waits out its minute.
      A registration whose engine the arbiter stands (`docs/own-llm.md`, 「An engine the arbiter stands」)
      is asked through the arbiter's master instead: a stopped engine is no outage, and the road stays
      open while its card service is placed on a node no claim and no lower tier holds.
  - **A gate** is no run of agency's and is in no walk here (`docs/own-llm.md`, 「Gates are not agency's
    runs」).
- **Re-checked just before running:** on/off is checked again just before a run starts. A provider turned
  off while waiting for a lock is not launched anew, a request already running is left to finish, and
  Claude's run line is unchanged. While Claude is off, folding a session on Claude Code is deferred and
  session records preserved.
- **A co-work order's stages** each go out on the road of the agent they are let in on like any role's run,
  and the next stage's reservation holds slots of its own role (`docs/role-slots.md`, 「Reserving slots」).

### How a supervisor's turn and a role's run cross over

- **A supervisor's turn** carries the request its caller assembled with recent conversation, and its tool
  presence and write rights, as a common run contract, on its road.
- **A role's run** carries its whole prompt — the platform's harness opening, the role's prompt and what
  the work hands in — its path, the tools and MCP configuration its line shows, and, in a repository's
  checkout, the repository's instructions and rules (`docs/repositories.md`, 「What a run in a repository
  is handed」). The rules go where each program keeps them beside its own default prompt
  (`docs/launch-lines.md`).
  - **A job's turn goes out on its agent's road.** The launch asks the road judgement as the job's work parts
    are read (`docs/api.md`, 「assignments and jobs」), and the turn goes on in the role's session kept at that
    path on that provider and model where one is valid (`docs/sessions.md`, 「A role's session」).
- **A search or a browse run goes out on the road of its role's agent** like any role's run, each such road
  one whose program carries its line (`docs/agent-roles.md`, 「The voyager and the seeker roles」).

### A turned-off provider is not used

- **What is skipped:** no run goes out on a turned-off provider — a supervisor's turn, a role's run, a job
  in a repository that pins one of its models, a session whose caller names it — and its sessions are not
  warmed, since turning off is the owner saying not to use that provider and no row overrides that. A
  turned-off provider is one that cannot answer now, so a supervisor's turn on it fails and a job on it waits
  (`docs/repositories.md`, 「A pinned model」).
- **A turned-off account is passed over the same way:** the Claude Code road does not walk it and its
  sessions are not warmed.
- **No per-model off:** turning off a provider closes every road on it, and changing a row's model is done on
  that row's road.

### Warming only enabled providers

- **A supervisor is warmed,** on the providers its row switches warming on for, **and the kept session of an
  agent whose row keeps its cache warm** (`docs/keepalive-schedule.md`, 「Whose prefix is warmed」). No switch
  stops every warming at once, and nothing of a repository or a place is warmed; the kept session of any
  other agent is resumed while its cache is still warm and opened afresh once it has gone
  (`docs/sessions.md`, 「A role's session」).
- **Claude off:** while Claude Code is off, no Claude keepalive goes out. **One account off or resting:**
  its own sessions are not warmed and its prefixes are left to cool, while every other account's are
  warmed, since a prefix belongs to the account it was built under. **An endpoint account:** its
  sessions are never warmed, whether it is on or off. The endpoint ignores the cache markers a ping
  would renew and keeps its own cache by itself, so a ping would only be a paid request. **Codex off /
  OpenRouter off / DeepSeek off:** that provider's sessions are not warmed. **A provider on that cannot
  answer now** — its credential gone, its balance spent, every login it holds closed (「Availability:
  whether a provider can answer now」) — is not warmed either, since no run goes out on it until it can.
  An unwarmed session's record is left in place, and re-enabling a provider judges the next warming from
  the time that record names.

### The Codex road is a member of the gateway

- **How it runs:** Claude Code on agency's gateway under the member named for the Codex model, on an OpenAI
  API key kept in the member's key file under the data root's `env/`. The Codex CLI remains in the build for a
  rollback, and no provider of this build names it.
- **The models:** the members the deployment names for the Codex road, each with its window, answer ceiling,
  thinking default and request wait. A member road serves no subscription window, and no run of it is warmed.

### The OpenRouter road is a member of the gateway

- **How it runs:** Claude Code on agency's gateway under the member named for the OpenRouter model, with the
  key its key file holds. The public listing is still read for the chooser that declares a member.
- **The models:** the members the deployment names for the OpenRouter road, each with its window, answer
  ceiling, thinking default and request wait. The road publishes no cache lifetime, and no run of it is
  warmed.

### The registered models are a provider like any other

- **Only when:** the registry's provider stands only when the gateway's key file is present, and answers
  with the registration a road names, or a repository's pin.
- **How:** a tool-less run is one request to the gateway; a run with tools is Claude Code on the gateway
  (`docs/own-llm.md`). Whether thinking is on and the ceiling of answer tokens are named on the road
  (`docs/provider-order.md`, 「What a road is」).

### The DeepSeek road is a member of the gateway

- **A provider of its own.** DeepSeek's models are served by the `deepseek` provider rather than by the
  registry, and so are roads of their own provider, with its own quota read from its balance
  (`docs/usage-windows.md`, 「What reads a provider's quota」). The members the deployment names for the road
  carry its key file, `env/deepseek.key`, and its window, answer ceiling, thinking default and request wait.
- **How a run goes out.** Claude Code on the gateway under the member the run names: a supervisor or a
  role's run stands it in its pane, its turns put into the program and read back out of its transcript.
  The session is kept and resumed like any Claude Code session, and what a turn spent is reported off the
  transcript.
- **Thinking and effort.** The provider's words are `none`, `low`, `medium`, `high` and `max`. `none` is a run
  without thinking, and so is a road with thinking turned off. The word goes out in the effort
  header, which the gateway writes into the request its backend reads; the Claude Code effort word a run is
  launched with is the provider's own mapping of it.
- **Which model a row runs on there** is its road's, a member of the DeepSeek road at the effort it names;
  the seeded agents' roads are the roles policy's, and the owner changes them on the row
  (`docs/agent-roles.md`, 「The roles the policy seeds」).
- **What the provider is not.** Its models are not registrations, but its runs reach them through the model
  gateway; nothing on this road is placed on a work node.

### The models a road names

- **Not a fixed launch line:** the console reads each provider's models (`docs/provider-order.md`,
  「Models a provider holds」) and each model's supported efforts, and lets a road's model and effort be
  changed per supervisor and per agent. So changing one row's road moves no other row's.

### Keeping the mode across cache, session and usage

- **The roads answer differently and do not pretend to be the same.** Every common answer carries the
  mode that actually answered (`claude-code`, `codex`, `openrouter`, `registered`, `deepseek`) with its
  model and effort, and a measured usage row keeps mode and model as a pair, so models of the same name on
  different roads are not merged. Agency reads those figures off each road — Codex's JSON events,
  OpenRouter's stream usage read through a loopback tab on a headless run, Claude's own answer — and
  hands them to the usage collector, which splits and shows them (`usage.md`). A run whose stream
  carried no usage gets no invented figures, and a turn in the terminal client's dsh session reports none: its record
  carries the answer's words and model alone.
- **Each window and run belongs to a slot:** the provider that reported it, and the account too where
  that provider holds logins, built by one constructor so no writer decides for itself whether an account
  belongs on a row. Two accounts report windows of the same names, so neither can overwrite the other.
- **Which provider and account serve a row now is answered in one place** — the row's road, and, where its
  provider holds logins, the login the provider walks to now.
- **Sessions follow the same principle:** Claude Code sessions and Codex threads are shown with their own
  mode and model, a Claude session with the account it was opened under. A dsh session stands in
  the terminal client's pane across its turns, but agency records no id for it, so an OpenRouter answer offers no
  session back. A session is offered back only to a run on the account that opened it.
- **Cache lifetime and keepalive follow the provider's policy.** A ping goes only into a supervisor's
  program (`docs/keepalive-schedule.md`); a role's session is judged by the same lifetime when it is
  resumed (`docs/sessions.md`, 「The warm/cold criterion」).
  - Claude Code reads a one-hour cache regardless of model and pings at forty minutes idle.
  - Codex reads a thirty-minute cache for models that announce one and resumes its thread after twenty
    minutes idle; for earlier models that promise no minimum lifetime it invents none and sends no ping.
    The lifetimes stand in Codex's provider declaration, by the longest model name each begins.
  - OpenRouter warms only when the actual upstream it rode has a cache: the tab reads the upstream that
    answered from the stream, and judges a cache by whether an endpoint of the same provider in that
    model's public endpoint list prices a cache read. The ping goes only to the recorded upstream, with
    fallback off, since a ping warming another provider only spends money. The interval is the same
    two-thirds-of-lifetime ratio the other roads use.
  - The own LLM services require neither cache nor keepalive: no registration announces a lifetime, so a
    registration gets no ping, and a supervisor standing on the registry's provider warms no session of
    another provider.
- **A role's kept session follows the same record lifetime,** so a session idle longer than that is not
  resumed but opened anew. Every ping obeys the supervisor's switch for the provider it stands on, skips a
  supervisor standing on an own LLM service, and, if a real turn is running in the same scope, skips that
  round without waiting.

### A supervisor's turn on a road other than Claude Code

- **A rest is one Claude account's,** so it closes only a road on Claude Code. A supervisor whose road is on
  another provider keeps taking its rooms while every Claude account rests, and the room holder asks
  agency whether a supervisor's road answers now rather than judging it (`GET /supervisors/{supervisorName}/road`,
  `docs/api.md`, 「supervisors」); a login is closed for it exactly as the account judgement closes it
  (「The account judgement」).
- **How the turn goes out.** The turn takes the supervisor's road (`docs/provider-order.md`, 「Whether a road
  can answer」), on the model and effort it names; on Claude Code it goes out on Claude Code's own session.
  On another provider the program stands in the supervisor's own pane as that provider's program, on the
  chat line's system prompt, tools and MCP configuration:
  - **Codex** on the supervisor's kept thread for that model and effort, opened afresh where Codex no longer
    holds it, and the thread that answered kept for the next turn;
  - **OpenRouter** through the terminal client, which keeps one dsh session on the Agent Client Protocol, the turn put
    into its inbox and its answer read out of its record; the overlay composes the gateway route, the key
    rides in the program's environment, and the agency's MCP configuration stands in the overlay as the
    session's servers, rows the overlay inserts; a server that does not attach is told to the session as
    a fact it reads in its next round, and the shell is handed back the variables dsh's own start-up
    drops by their names (`docs/agent-control.md`, 「dsh's providers: OpenRouter and DeepSeek」); the
    system prompt's tools paragraph names dsh's own tools; no upstream is pinned, since agency keeps no
    record of a warmed one;
  - **DeepSeek** through the terminal client in the same way, the overlay's route DeepSeek's own API on the model the
    road names, with the road's key in the program's environment (「The DeepSeek road is a member of the
    gateway」);
  - **a registered model** as Claude Code on the gateway, the registration first held to the gateway's
    list, on a session of its own placed where the registration places it.

  A turn whose road does not answer ends failed with why; no other provider is tried. A turn whose words
  never became a turn — a pane left in a dialog somebody opened, the words unsubmitted — is not a road that
  did not answer: nothing is read as closed, and the session the turn would have gone on is left as it
  stands.

### The response options each road receives

Each response option the Claude Code line receives is set out below, saying whether a substitute road has
the same. "None" means that program has no such concept, with the reason.

| Option | Claude Code | Codex | OpenRouter (dsh) | Registered models |
|---|---|---|---|---|
| Process time limit | None: a supervisor's turn and a role's run each run until they end | Same | Same | None: a request with no tools waits for its answer; a run with tools is Claude Code on a registered model |
| Cache lifetime · ping interval | 1 hour · 40 minutes | 30 minutes · 20 minutes for models that announce it, else none | Follows the answering upstream, none if not listed | None. No announced lifetime |
| Ping time limit | None: a ping goes until it answers or fails on its own | Same | Same | No ping |
| Keepalive switch · rest | Warms a supervisor only where its switch for the provider is on, pauses during its own account's rest | Obeys the switch but warms during a rest, since a rest is one Claude account's affair | As Codex | Not applicable |
| Session continuation | Resumes a supervisor's session and a role's kept session | Resumes a thread per run scope, model and effort | The session stands in the terminal client's pane and carries the next turn; agency stores no id and resumes none | None for a tool-less request; a run with tools is Claude Code's |
| Context folding | The caller folds with the CLI's compaction | The Codex CLI compacts its own thread | dsh compacts its own session; agency folds nothing | Not applicable |
| effort · thinking | The effort set per line | Passes effort, thinking-off is low | The entry's level is the session's reasoning effort, and thinking off passes off | Thinking on or off; a run with tools sends medium or low |
| System prompt | Replaces the CLI instruction with one whole file opening on the platform's harness, worker its rules after it | Replaces via `model_instructions_file`; worker the Codex default and `AGENTS.md`, rules as `developer_instructions` | Goes in as one part of the dsh prompt, no replacement | A request with no tools carries its system prompt in the request; a run with tools as on Claude Code |
| Tool · permission isolation | A supervisor's and a role's program in bypass mode with its line's denied tools, a runner's shown tools and an allow list, settings sources blocked but a role's checkout's own | Moves to a shell and sandbox, loads no user settings | Moves to filesystem/shell rows and the dsh sandbox, no per-command allow list | A run with tools is Claude Code's line |
| Output schema | None. A structured answer is asked in the prompt and validated by the caller | None, for the same reason: a schema option moves the cached prefix | None | None |
