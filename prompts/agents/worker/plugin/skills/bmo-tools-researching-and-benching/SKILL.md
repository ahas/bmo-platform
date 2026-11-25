---
name: "bmo-tools-researching-and-benching"
bundle: worker
version: "1.3.0"
description: "How a job researches, benches or tunes something the lab runs — an engine, a model, a library, a card, a setting: the maker's own material first, then what the lab can do with it, then measurement; every option tried until none the job can try is left, with none handed back to the owner and no interim report; one final document in the deployment's 아카이브, reported once its narration is spoken; and the bench lock released before the report. Use before the first step of any item that measures, compares, tunes or researches something to run on the lab's machines."
---

# Researching and benching

## The order of the work

```mermaid
flowchart TD
  A[The maker's material] --> B[What the lab can do]
  B --> C[Measure]
  C --> D{An option left that a job can try now?}
  D -- yes --> E[Try the next option] --> C
  D -- no --> F[Final document and its narration]
  F --> G[Bench lock released]
  G --> H[Narration spoken, then the report]
```

- **1. The maker's material first.** How the thing is built and how it is tuned are read from its
  maker before anyone writing about it: the official documentation, the repository's README, docs
  and code, the release notes and changelog, the model card, the configuration files, the paper in
  full, and the issues and pull requests for what the documentation leaves out. What others write
  comes after and is told apart as theirs. The deep dive's rules bind how it is read and told,
  `~/.local/share/bmo-research/deploy/prompts/rules/deep-dives.md`, and the research routine beside it,
  `~/.local/share/bmo-research/deploy/prompts/rules/research.md`, is the shape of the document a job
  writes: a manual where nothing was measured yet, and the verification report — the maker's figures
  against ours, the pass line set before the first run, how it was run, what was kept and how the
  restore was proved — for the measurement this skill is for. Read both before the first step.
- **2. What our lab can do with it.** The lab's machines and cards are read from the brain's chunk on
  the compute nodes, `knowledge/compute-nodes-gpu.md`, never from memory. GPU work goes on the GPU node
  pool through each node's card arbiter: a card is taken by its ticket, never by stopping
  another's service by hand.
- **3. Measure.** The lab is taken for a measurement through the arbiter alone (「Taking the lab」
  below). Every figure stands beside the maker's own for the same measure, with the
  conditions of each; a run is repeated and its spread written; raw logs stay where the repository
  keeps its measurements. A value that cannot be measured now is a calculation shown whole.
- **4. Every option, until none is left.** The job raises its own options out of what 1 to 3 found,
  each a lever with the effect it is expected to have, and tries them one at a time: measured,
  adopted or rejected with its reason. A candidate a result turns up joins the list. The work stops
  only when no option a job can try now is left, never after the first measurement.
- **An option no job can try now** — hardware not yet in the lab, infrastructure only the owner changes, a
  purchase — is not tried. It goes into the document under 나중에 할 수 있는 선택지 with the effect
  expected and what it needs, and the work carries on with the rest.
- **The document is built as the work goes.** Its working copy is kept in the item's own folder
  under the workbench, which outlives the job, each option's order, measurement, verdict and reason
  added as it ends. A repository that keeps measurements in its own documents has them
  written there too.

## Taking the lab

- **The job is the occupant `bench`, on as many cards as the arbiter grants, at most four.** Before a
  measurement it asks the arbiter's master for the cards the measurement can use, as `bench`, at most
  four, and never names a machine: the cards it is granted are where it measures, one measurement on each.
  On each granted card it asks that machine's card arbiter for a ticket as `bench` and waits until its turn
  is given; every process it starts for the measurement carries `BMO_CARD_SERVICE=bench` in its
  environment, so the arbiter knows whose it is.
- **The arbiter alone makes room.** The job never stops, starts or restarts another service by hand to
  free a card; what has to give way, the arbiter moves.
- **The ticket goes back when the measurement on its card ends,** whatever it came to, and the cards
  with it once the last measurement ends, before the next step of the work.
- **A ticket told to give way ends the measurement at its next boundary:** the job stops at the end of the
  step it is in, gives the ticket back, and asks for a new one. A measurement cut this way is run again
  whole once the turn comes back; no figure of the cut run is kept.

## A capability nobody has confirmed

- **Check first:** the maker's own material, the code, the issues and pull requests, and a real run.
- **Where it exists, use it and measure it.**
- **Where it does not, port it or patch it.** Find whether it can be ported into an engine of our own
  or written as a patch of our own, then do it, check it the way that engine's repository checks a
  patch, and measure it. The repository holding each engine says how a patch is made, checked and
  installed.
- **Each step goes into the document:** what was looked at, what it answered, what was built and
  what it measured. "Not confirmed" is never a finding.

## Nothing handed back

- **No option is put up to be picked.** Never `bmo job ask` between options the job can try, and never a report
  that lists them for the owner to pick from: trying them is the work.
- **No interim report.** The job reports once, at the end, with the final document; nothing is told
  to a room about how far it has got.

## The final document

- **A deep dive in the deployment's 아카이브,** filed with `bmo archive doc` and given its narration with
  `bmo brief-attach` as the `bmo-asking` skill says, in the verification shape `research.md` sets: the conclusion,
  what the owner decides, the maker's figures against ours, how it was run, what was kept, the
  application plan, the open questions and the sources.
- **What is still open is only what no job can reach now,** each with what puts it there. A value a
  job could have measured is measured, never marked `추정` or `확인 필요`.
- **The narration** is written by the rules of a deep dive's, `deep-dives.md` 「The narration」: the
  written words, names and figures as written, from which the voice unit derives and checks the spoken
  words.
- **It is reported only once its voice is ready.** `bmo brief-attach <document number> <file>` returns once the
  document keeps its sound. Where it ends with a failure, run it again. Only then does the document's address
  go into the report.

## The bench lock

- **A bench lock is whatever the job took so a measurement owns the lab:** the arbiter's ticket it holds
  as `bench`, and the requests it asked a service of its own to refuse.
- **The job releases it before it reports:** the best configuration in place and its health read
  through its own checks, requests let through again, every ticket given back. The report says
  what was released and at what time.
- **Work set aside for the bench is the supervisor's to resume.** A job never takes a hold off: the report names
  each item set aside for the bench as ready to start now, and a hold carrying the bench's item as
  its condition is told to the supervisor by the bot when the item closes.
