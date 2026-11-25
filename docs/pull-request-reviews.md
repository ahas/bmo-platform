# Pull request reviews

How a pull request opened on one of the deployment's repositories reaches a job in that repository, with no
person reading GitHub and no model watching it: GitHub tells agency, agency files a review item for the
repository, and a merge lets the item that waited on the pull request go on.

## What is watched

- **The repositories agency holds.** A repository row whose base checkout's `origin` remote is a GitHub
  repository holds that repository's pull requests. Agency reads each repository's remote from its checkout as
  a delivery or the fallback needs it; a GitHub repository no row holds is let be.
- **No model watches.** The watch is GitHub's delivery and one search an hour; a model runs only inside the
  review job of a pull request that exists.

## The delivery

- **GitHub pushes.** Each repository agency holds sends its `pull_request` events
  (opened, reopened, synchronize, closed) to the public address, where the console's gateway hands each
  delivery, its body as it came and its headers, to agency's `POST /github/deliveries` (`docs/api.md`,
  「pull request reviews」).
- **It is GitHub's or nothing.** Agency checks the delivery's `X-Hub-Signature-256` against the webhook secret
  the deployment names (`BMO_GITHUB_WEBHOOK_SECRET`, or the file `BMO_GITHUB_WEBHOOK_SECRET_FILE` names); a
  delivery that does not carry it, or carries another, is refused 401 `bad-signature` and nothing is read of
  it. A deployment that names no secret takes no delivery: every one is refused 503 `no-webhook-secret`.
- **It is answered at once.** A delivery that reads is answered 202 and acted on after the answer; an event
  agency does not act on is answered 202 as well, so GitHub never retries it.

## The fallback

- **Once an hour, one search.** Agency asks GitHub's search for every open pull request of the repositories the
  owner's login the deployment names (`BMO_GITHUB_ORG`) owns, in one request (`gh api graphql`, a search of
  `is:pr is:open user:<login>`, a hundred to a page), and files what a delivery missed. The search reads of
  each pull request what a delivery carries of who opened it: its author's association with the repository and
  the repository its head branch stands in, so the fallback files by the same rule a delivery does (「Who opened
  it」). A deployment that names no login runs no fallback.
- **The budget:** one GitHub call an hour for the fallback, a page more only past a hundred open pull requests,
  and none for a delivery; no model tokens at all for the watch.

## Who opened it

- **A review runs unattended only for the owner's own change.** The review worker checks the pull request's
  branch out and runs the repository's checks on it under its own rights, so the code it runs must be code the
  owner already stands behind. A pull request is the owner's own where both hold: its author's association
  with the repository, as GitHub names it, is `OWNER` or `MEMBER`; and its head branch stands in the
  repository the pull request is opened on, not in a fork.
- **`COLLABORATOR` is not enough.** A collaborator is somebody outside the owner's account or organization
  given write access to one repository; that access is not leave to run code on the deployment's machine, so
  a collaborator's pull request waits for the owner as anybody else's does. The trusted associations stand in
  `policy/pull-requests.toml`.
- **A fork is never the owner's own,** whoever opened it, and neither is a pull request whose head repository
  GitHub no longer names (a fork since deleted) or whose association it does not say.
- **Anybody else's pull request waits for the owner.** It is filed as a review item all the same, but in the
  approval lane (below), which the job service does not open by itself: the item stands a draft until the
  owner approves it, and its brief tells the worker to review and check it and send what it found as a review
  comment, never to merge it — the merge of a pull request from outside is the owner's own act.

## The review item

- **One item per pull request and head.** An open pull request on a repository agency holds is filed in the job
  service as an item for that repository (`POST /items`), the repository the pull request belongs to named as its
  `assigneeName`, so only a job in that repository takes it and the idle agent's notice tells the supervisor once
  an agent could take it while one of the repository's checkouts is free (`docs/agent-control.md`, 「An idle agent is
  told to the orderers of the items it could take」): its title names the repository and the pull request's number and title, and its
  brief is the review the job runs — hold the change to the base-library rule where the repository is a
  base library (the general case only, the base's own tests, nothing of one asker's), run the repository's
  checks on the pull request's branch, use bmo-review where it helps, then merge it or send it back with the
  reason as a review comment on the pull request, and tell the owner's room what became of it in one line.
  A pull request that is not the owner's own (「Who opened it」) carries the approval brief instead: the same
  review and checks, what was found left as a review comment, no merge, and the owner's room told that the merge
  waits for the owner. The words stand in `policy/pull-requests.toml`.
- **Its labels:** `pr`, the pull request's address, by which a second delivery or the fallback finds it filed
  and files nothing; `head`, the head commit it was filed for where the delivery names one; and
  `lane`. The owner's own pull request takes the lane `pr-review`, the job service's unattended lane, which
  opens an item of that lane without an approval and says so in the item's note where the deployment's job
  service holds it; until then the item is a draft, approved as any other. Anybody else's takes the lane
  `pr-review-approval`, which no deployment names unattended, so the item is a draft until the owner approves
  it. Both lanes read as review items wherever agency looks for one.
- **Filed once for a head.** An opening, a reopening or the fallback files nothing while a review item of the
  pull request stands, nor where one was filed for the head it names — the fallback, which names no head,
  where any was filed; an item filed before items named their head counts as filed for every head.
- **A pull request pushed again** (`synchronize`): a review item that still stands — a draft, open or
  running — is told as a note naming the new head, and the worker reads that head in its review; nothing
  more is filed. Where none stands and the latest review item of the pull request is `done` — the worker
  sent it back — a new review item is filed for the new head, as an opening files one and by the same rule of
  who opened it; a latest item dropped files nothing.

## After the pull request

- **Merged:** every other item labelled with the same `pr` address — the item whose work stopped on the pull
  request — that stands `held` is opened again (`state` `open`) with a note saying the pull request merged, so
  its work goes on; and a review item still standing, its pull request merged by somebody else, is told so in a
  note.
- **Closed unmerged:** the review item still standing is told so in a note, and the items waiting on it are
  told as well; none is opened.
