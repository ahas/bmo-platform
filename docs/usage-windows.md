# Usage windows

How agency keeps the usage windows of every account that has them: the share of each limit an
account has used and when that limit resets. Directory and token accounts have them; an endpoint
account is paid by the request and has none, so it holds no window slot and no window reader asks it
(`docs/cache-fallback.md`, 「The Claude Code road is a list of accounts」). What a paid account has left
is its balance, kept beside the windows (「Balances」 below). The account judgement, the road judgement's
reading of what a provider has left, the fallback's view and the quota source all read them from here
(`docs/cache-fallback.md`, 「The account judgement」, `docs/provider-order.md`, 「Quota: what a provider has
left」, `docs/api.md`, 「Accounts」). The keeping is the `cache-siren` crate, whose cache-lifetime judgement is
`docs/sessions.md`'s.

## A window and its slot

- **A window** is one limit of one account: its name, the fraction used (0 to 1), the moment it resets
  in epoch seconds or none, the moment it was seen, and, where the provider says them, how many minutes
  it runs and the name of the limit it belongs to.
- **A slot** is whose windows they are: the provider they were read of, and the login under it where the
  provider holds logins of agency's; a provider that holds none keeps its windows under the empty
  account. Two logins report windows of the same names, so a slot never overwrites another's.
- **A reading replaces its slot's whole set** in one transaction: every window the slot held goes, and
  the windows read are written with the moment they were seen. A window an answer did not state is
  gone from the slot rather than kept from an older reading.
- **The windows stand in the store** (`docs/store.md`), so a restart reads the last reading of every
  slot rather than nothing.

## Where readings come from

- **A turn's answer.** The windows a turn's answer carries — Claude Code's from its status line,
  Codex's from its rollout's `rate_limits` (`docs/agent-control.md`, 「The session record and usage are
  not squeezed out of the screen」) — are written under the slot of the login that produced them: the
  login a run was launched on, the login of the program that wrote the status record. A reading whose
  login cannot be named on a provider that holds logins is not written at all. A turn that read no
  windows leaves the slot as it was.
- **A reader on a clock.** Each provider whose windows can be asked outside a turn has a reader, and
  each reader declares its own provider, how often it passes and how long its reading stands. A pass
  never runs beside itself.

## The Claude account reader

- **Every registered directory and token account is asked on each pass,** on or off, with its own
  login, and an endpoint account never is: a GET of the
  provider's account-usage endpoint (`https://api.anthropic.com/oauth/usage`) carrying that account's
  access token as a bearer token. A request waits for its answer: it ends when the endpoint answers,
  refuses the connection or breaks it, and the connection's keep-alive probes tell an endpoint that has
  gone from one still working. No clock cuts it, and the next pass begins once this one has ended.
- **The token is read out of the account's own credential file and dropped:** a directory account's
  file holds it as `claudeAiOauth.accessToken` beside its `expiresAt`, and a token found expired is not
  sent; a token account's file is the token. The token goes into the one request header and is never
  kept, returned or logged. The reader never renews it and uses nothing else of the file: renewing a
  login is the CLI's, and the reader asks the CLI for it (「A login found expired」 below).
- **The windows taken are `five_hour` and `seven_day`:** each `utilization` is a percentage and is
  kept as its fraction, and each `resets_at` is an RFC 3339 moment kept in epoch seconds. A window with
  no figure is left out rather than written as nought, and an answer with no usable window is a
  failure.
- **It passes every minute, and a reading stands for thirty passes,** thirty minutes: long enough to
  ride out a passing failure of the endpoint, short enough that a full window that names no reset, a
  quota that may have come back unread, stops closing its login. A full window that names its reset
  closes its login until that reset however old its reading is (「What reads the windows」).
- **It never asks faster than the endpoint accepts.** An account the endpoint refuses for asking too
  often, an HTTP 429, is not asked again until the moment the refusal names in its `Retry-After`, or for
  five minutes where it names none; the passes in between leave its last reading as it stands and say
  the pause once as the account's failure.
- **An account that cannot be asked keeps its last reading.** A failure is said once when it starts,
  on the server's error output, naming the account and its reason (the credential file unreadable, no
  token, the token expired, the endpoint's HTTP status, the request failing, no usable window), and its
  recovery once when it ends. A reading taken says nothing, so a log silent on the reader is every
  account read.
- **The unread mark.** Where an account's token is found expired, or the endpoint refuses it as
  unauthorised, the reader marks the moment it first found so under the account, and the next reading it
  takes lifts the mark. A reading seen at or before the mark is stale for the account judgement however
  young it is, since the account cannot be asked again until a run renews its login: the run the reader
  asks for (below), or any other run that goes out on it.
- **A login found expired is renewed by one run of the CLI.** Where a directory account's token is found
  expired, or the endpoint refuses it as unauthorised, the reader asks for one run of Claude Code on that
  login: a run in a pane of its own (`docs/agents.md`, 「A run in a pane of its own」) under the account's own
  configuration directory, the way every run on the account goes out, on the model and effort the provider's
  usage declaration names for it — the cheapest the login offers — with a one-word turn, no tools, and its
  answer thrown away. The run holds the role the roles policy names for agency's own runs, `upkeep`, and
  opens the session that role's setting gives it. The CLI renews the token for itself as the run goes out, and
  the run holds the account's renewal lock while it runs, waiting for it where a run near its login's end
  holds it for the same renewal (`docs/cache-fallback.md`, 「A login that is about to end」); the next pass
  asks with what it wrote. The
  reader never refreshes a token itself: a refresh token is replaced at every renewal and every pane on the
  login shares it, so a second refresher beside the CLI would leave those panes holding a credential it had
  replaced.
  - **Only a directory account is renewed so.** A token account's long-lived token holds nothing to
    renew with and an endpoint account holds no login, so neither is ever run on for it; a token account
    refused stays marked until its file holds a token the endpoint takes. A login waiting for a person
    or past its end (`docs/cache-fallback.md`, 「A login that is about to end」) is not run on either: only
    signing in again brings it back. A renewal run its login refuses marks the account as waiting for a
    person and tells the owner once, as a turn's refusal does.
  - **A directory account carrying on by its token for the work nodes is read from its turns.** The usage
    endpoint refuses that token for its scope, so the reader asks with the login alone: inside the span
    before its end, while the login still stands, the reader goes on reading and renewing it, and once the
    login has ended or is marked the reader cannot ask for it. The windows each turn's answer carries are
    then its readings (`docs/cache-fallback.md`, 「A directory account carries on by its token」).
  - **One renewal of an account at a time, at a pace.** None is asked while one runs, none before a pass
    after it has ended, and, while the account still cannot be read, none sooner than five minutes after
    the last was asked. The pass does not wait for the run, so every other account is read on time. The
    pace is between asks, not a bound: the run itself is never cut.
  - **Said once each way:** its start on the error output, naming the account, and its outcome once as
    the run ends — on the standard output where the run ended without failing, on the error output with
    its reason where it failed. Whether the login came back is the next pass's reading, which lifts the
    unread mark.
- **A window that came back early is said:** a window the last reading held full, not yet reset, that a
  new reading finds short of full is logged as having come back before the reset its reading named.

## The Codex reader

- **Codex holds no logins of agency's,** so its windows stand under the empty account.
- **The newest rollout first.** Where the newest rollout file among the newest three days' folders ends
  in a token count written within the last minute, the windows it carries are the reading: each of its
  `primary` and `secondary` windows named `codex:<limit id>:<primary or secondary>`, its used percentage
  kept as a fraction, its reset, its length in minutes and its limit's name.
- **Otherwise the app-server is asked.** Codex's app-server is started over standard input and output,
  initialised, and asked `account/rateLimits/read`; the answer's windows by limit are read, or its
  single `rateLimits` where it gives none by limit, in the same names and fields. The read waits for that
  answer or for the process to close its output, however long either takes; a process that ends before it
  has read the requests has closed before answering, as one that ends after. The process is ended with
  the reading, so none outlives it, and everything it had said on its error stream by its end is carried
  beside a failure: what stands in the pipe as it ends is read then, and the stream's close, which a process it
  started may hold off, is never waited for.
- **It passes every minute where Codex is installed, and a reading stands for six passes.** Only the
  public meter fields are kept: no account id, credit balance or plan detail.

## Balances

- **A balance is what a paid account has left:** an amount, its currency, and the moment it was seen.
  It stands in a slot as a window does, the provider it was read of and the account under it, the empty
  account for the provider's own road; a reading replaces the slot's balance, and an account that
  cannot be asked keeps its last.
- **Each provider that offers a balance declares how it is read,** in its own member and its own policy
  file: the address asked, the key it is asked with and what its answer states; a provider that declares
  none is never asked. The reader knows no provider.
  - **DeepSeek:** a GET of `https://api.deepseek.com/user/balance` with the road's key file as a bearer
    token; the first of the answer's `balance_infos` is the reading, its `total_balance` the amount and
    its `currency` the currency.
  - **OpenRouter:** a GET of `https://openrouter.ai/api/v1/credits` with the key in its key variable as a
    bearer token; the answer's total credits less its total usage is the amount, in US dollars.
- **A balance carries a request allowance where its provider states one:** how many requests of the day
  are used, the day's ceiling and how many are left, and what the allowance covers where the provider
  declares it, a kebab-case word a screen words it by. The provider declares a second address for it, asked
  with the same key right after the balance, and what its answer states; the allowance is kept in the
  slot beside the amount and replaced with it. A second address that cannot be read, or an answer that
  states none, keeps the amount with no allowance, and a provider that declares no second address is
  asked once.
  - **OpenRouter:** a GET of `https://openrouter.ai/api/v1/key`; the answer's `free_model_daily_requests`
    is the allowance of its free models for the UTC day, its `used`, its `limit` and its `remaining`, and
    what it covers is declared `free-models`.
- **Who is asked:** the provider's own road where the provider can answer on this machine, and every
  endpoint account whose base address stands on the same origin as the provider's balance address, with
  the account's own key file, its balance kept under the account. A key goes into the one request header
  and is never kept, returned or said.
- **It passes every five minutes, and a reading stands for six passes,** thirty minutes, its ask waiting
  for its answer as the account reader's does. A failure is said once when it starts, naming the provider, the account and the
  reason, and its recovery once when it ends.
- **What reads it:** the accounts list answers an endpoint account's balance, and the fallback's view its
  active slot's (`docs/api.md`, 「accounts, rest and usage」), each with the moment it was seen. The
  road judgement reads a provider's own road's balance as its quota (「What reads a provider's quota」
  below), and the account judgement an endpoint account's: one at or below zero is a road that cannot
  answer now, and one never read closes nothing (`docs/cache-fallback.md`, 「Availability: whether a
  provider can answer now」). A provider's own spent balance leaves its road open to a model the provider
  says costs nothing; an endpoint account's closes the account whatever the model.

## What reads the windows

- **A window is full only where all of it is used,** its used fraction at one. No share short of that
  closes a login, rests it or reads as no quota left, so a login takes runs until its provider refuses it
  or a reading finds one of its windows full.
- **The account judgement** closes a login whose own window reads full, not reset: one that
  names its reset, until that reset whatever the reading's age, since only a newer reading can say it came
  back; one that names none, while its reading stands — younger than its reader's span and later than the
  account's unread mark (`docs/cache-fallback.md`, 「The account judgement」). An endpoint account has no
  window for it to read, and the judgement asks it nothing here.
- **The rest a run's limit begins** reads its login's windows where the answer names no reset
  (`docs/cache-fallback.md`, 「Automatic walks only on providers, in order」): a login's own limit rests
  the login only where a window of its own reads full, until that window resets; one model's
  limit rests that model on the login until the soonest reset any of its windows names. Where no window
  names one, the next reading decides: a model's rest written then holds until a reading of its login is
  seen after it, no longer than a reading stands.
- **The road judgement** reads every window of a provider's slots for what the provider has left
  (「What reads a provider's quota」 below).
- **The fallback's view and the quota source** read each slot's two headline windows (`docs/api.md`,
  「Accounts」): the active slot's, and the fallback's view every directory and token account's own
  whichever road the chat stands on, so an account's windows are shown while another provider answers.
- **The accounts list** reads each directory and token account's two headline windows too
  (`docs/api.md`, 「accounts, rest and usage」), each with the moment it was seen, so its reader judges
  how old it is.
- **An account's reported windows are its answering login's alone.** A window is the login's that was
  read, not the account id's, so what the fallback's view, the quota source and the accounts list report
  of an account leaves out every window seen at or before the account's unread mark (its token found
  expired or refused, 「The Claude account reader」) or its mark of waiting for a person
  (`docs/cache-fallback.md`, 「A login that is about to end」): neither login answers the reader, so what
  was read before is no current window of the account. A window seen after both stands, and the next
  reading after the login answers again brings the account's own windows back. The account judgement
  still reads every kept window as 「What reads the windows」 says, so an account can rest until a reset
  the list no longer shows; its `restUntil` says until when.
- **A login changed under an account takes its windows with it.** Pointing an account at another home,
  token file or endpoint, and a login waiting for a person signed in again, each drop the account's kept
  windows and its unread mark, since the login that answers next may not be the one read before; the
  account has no windows until its new login is read.
- **A keeping that cannot be read is a fault, said with its cause,** never read as an account with no
  windows.

## What reads a provider's quota

- **The road judgement reads one quota for each provider,** afresh as each run starts, out of the windows
  and the balances kept here (`docs/provider-order.md`, 「Quota: what a provider has left」). Each
  provider's member says which of its readings count; the reader knows no provider, and nothing of a
  quota is kept on an order.
- **A provider with usage windows has left what its window with the least left has left:** one less the
  used fraction of that window, the five-hour and the weekly window weighed alike. Codex's windows are
  read under the empty account (「The Codex reader」).
- **Claude Code's quota is read account by account:** each account open to the run's model, by its own
  window with the least left, so the provider has quota while one of those accounts has any left, and
  the account walk offers that account first (`docs/cache-fallback.md`, 「The account
  judgement」). An endpoint account has no window, and has quota while the account judgement holds it open.
- **A window read full and not reset is no quota left;** anything short of full is quota, and no share
  of it is held back.
- **A provider whose member reads its quota from a balance — DeepSeek and OpenRouter —** has quota while
  the balance its reader last kept is above zero, and none once it is at or below zero; a balance never
  read reads as quota (「Balances」 above). A run on a model the provider says costs nothing has quota
  whatever the balance (`docs/provider-order.md`, 「Quota: what a provider has left」).
- **The registry's provider always has quota:** a registered model runs on the deployment's own engine
  and reports neither a window nor a balance. **A provider that reports neither** has quota as well.
