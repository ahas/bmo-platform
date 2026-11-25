# A job's review

A job does not end on its worker's word. Every turn of a job that answers is a report, and the report is
examined by the supervisor that ordered the work before the job goes on or ends: the supervisor questions the
worker, sends the work back, or passes it, one check at a time, as often as it takes. A job whose item's work
order holds steps runs them one stage at a time, each stage examined before the next is handed over, and the
work whole is examined once more at the end. Every move of the examination is written onto the item's review in
the job service, in order.

This document states how a job goes through its review: its stages, the checks, what the supervisor may do at
each, the ceiling past which the work may be put to the owner, the owner's word, and the record. How a job is
launched and how its end is told stand in `docs/api.md`, 「assignments and jobs」.

## The words

- **The orderer** is the supervisor of the room the item's work was asked in, found as every queue notice finds
  it (`docs/agent-control.md`, 「An item's change is told to its orderer」). It examines the job's work.
- **A stage** is one step of the item's work order, in the order the steps stand. An item whose work order holds
  no step, or that has none, is one stage: its brief whole.
- **A report** is the words a work turn of the job ended on: what the worker says it did.
- **A round** is one report and the examination of it. The first report of a stage opens its first round, and
  each rework the supervisor sends back opens the next.

## The states of a job under review

- **`reviewing`** stands between `running` and the job's end: a turn of the job ended answered, its report is
  written onto the item's review, and the job waits on its orderer's move. No turn runs, and the job keeps its
  agent's seat: the agent answers for the job until it ends, so no other job is let in on it meanwhile, however
  long that job has waited for the agent (`docs/role-agents.md`, 「Which agent a run takes」). A review round is
  work and keeps the agent busy; only a job whose handed-over commit is not yet live, under review or with its
  close still waiting in its turn, leaves its agent free for another job beside it, still keeping its seat (`docs/role-agents.md`, 「When an
  agent is busy」). The seat is the
  job's slot lease, bound to none of its turns, so the end of the turn that reported does not give it back,
  and neither does a restart of agency while the job stands under review (`docs/role-slots.md`, 「The line」).
  The job's assignment keeps its checkout, every checkout lent to it stays lent (`docs/worker-slots.md`,
  「Loans across a job's review」), and its item's lease is renewed on the same clock as a running job's
  (`docs/api.md`, 「assignments and jobs」).
- **A turn the review asks for runs the job again:** the job is `running` while it goes and `reviewing` once it
  has answered, as many times as the review asks. A turn that ends in anything but an answer — failed, blocked,
  stopped, cut — ends the job as any job's turn ends it, its review left as it stood.
- **The job keeps where its review stands,** read on every read of the job (`docs/api.md`, 「assignments and
  jobs」): its stages, the stage under way and how many there are, the round, the check due now — kept while a
  question of it is out, and none while a work turn runs —, how many times
  its work was sent back (`rejections`), the owner's question where the work was put to the owner, and whether
  its checkout was given up while that question waited (「The owner's word」 below).
- **A stop reaches a job under review as it reaches any job:** no turn carries it, so it is written stopped at
  once, its assignment released and its lease given back.

## Stages

- **The stages are the orderer's, written as the item is filed** (`docs/api.md`, 「assignments and jobs」), each
  the question the job's agent is asked at that stage; the job's worker changes none of them. The item names
  the agent it runs on, or the model and the role whose free agent its launch takes, which the orderer chooses
  (`docs/repositories.md`, 「An agent an item names」); before it is filed its steps are checked against that
  agent, or the first of the role's agents on that model: the first filing is answered with that
  agent's skills and tools and what to check the steps against — the orderer's own lessons, and work those
  skills, tools or lessons say is always done for work like it — and the item is filed by the filing that
  names the mark that answer gave. The shape the check names is three stages: one that finds the grounds the
  implementation stands on, ending with a document and its voice briefing for the owner where its findings
  are not simple, the work going on without waiting for him; one implementation stage however much it does;
  and one that confirms what the implementation reported as not confirmed.
- **The stages are read from the job service as the job is launched:** the steps of the item's work order
  (`GET /items/{itemId}/work-order`), in order, kept on the job so a later change of the work order moves no
  stage of a job already running. The work order's tasks are moved as the stages go: the stage under way is
  `current`, a stage its review passed is `done`.
- **A work order that cannot be read is asked again, never read as none.** An item the job service answers
  404 for holds no work order, and its job is one stage; any other failure — the job service not answering,
  or answering an error — is asked again after a pause that doubles from the policy's first to its longest
  (`policy/job-review.toml`), however long it takes. The job stays `starting` meanwhile, its `heldFor`
  naming the wait, told to its item's orderer once as the wait begins (`job-waiting`), and goes no further
  where it is stopped while it waits. A job launched again after agency's restart keeps the stages its review
  was opened with and writes no second `opened` entry (`docs/api.md`, 「assignments and jobs」).
- **The first turn is handed the first stage alone.** Its prompt is the item's, as any job's, with the plan of
  every stage shown and the one stage it is to do named, and it says that the turn ends with the report of that
  stage, after which the stage is examined.
- **A stage passed hands the next one over** as a turn in the same session: the stage's number, how many there
  are, its step, and that this turn too ends with its report. The stage that holds the last step is said to be
  the last, so the worker closes the change there as its closing rules say.
- **A rejection may send the work back to a stage already passed.** A stage's examination can find that an
  earlier stage's work falls short — a check that shows the implementation wrong sends it back to the stage
  that implemented it. The rejection names that stage, and the work goes back there: that stage is the one
  under way again, every stage after it waits again, and the rework turn is handed that stage's step with the
  rejection's words as what is to be done. Once it passes, the stages after it are handed over again in order,
  each examined as before. A rejection naming no stage sends back the stage under way.
- **Each stage answers how many times its work was sent back:** the `rejection` entries of the item's review
  written under its number (「The record」), which the console shows on the stage's step.
- **Once the last stage has passed, a job of more than one stage is asked for its final report** in one more
  turn: the account of the work whole, every stage in it. That report is examined as a stage's is, under no stage
  of its own; passed, the job ends. A job of one stage ends once that stage passes: its report is the final one.

## The checks

- **Every report is examined by two checks, in order, one at a time:**
  1. `promises`: every item the work committed to — the brief, the stage's step, a rework the review sent back,
     each thing the report itself says it would do — was carried out, and nothing of it was left for later.
  2. `decisions`: every matter the report leaves to the owner, or calls the owner's to decide, stands on a
     ground that makes it the owner's: a decision only the owner can give. A matter the worker could settle
     from the work, the rules and the documents is not the owner's.
- **The orderer is told one check at a time:** an instruction naming the job, the item, the stage and its step,
  the round, the rejections so far, the check due now and what it requires, the language the run writes its
  results for a person in, and the words to examine — the report, the answer to its last question, or the
  owner's word. It closes by naming the actions open on the check and saying that the verdict is the
  `review_job` call alone, so nothing is written in the room's reply for it (「What the orderer may do」). The
  supervisor writes its own questions from what it read; nothing composes them for it.
- **A report's words are told once a round,** with the first check of the round. A later check of the same
  round that examines the same report carries none of its words: it says that the report is the one the
  round's first check was told with, unchanged, and that `job_reviews` reads it again where the session no
  longer holds it. An answer to a question and the owner's word are told whole with the check they come to,
  since nothing told them before.
- **The result language is the run's road's:** read off the road the job's last turn answered on. A run on a
  registration that names the languages it writes (`docs/own-llm.md`, 「What a registration holds」) writes its
  results in the brief's language where that is one of them and in English otherwise, and the instruction says
  so, naming the registration and its languages, and that an English report from it is no reason to send the
  work back (`docs/worker-language.md`, 「A model that writes only some languages」). Every other run writes them
  in the language its brief names, and the instruction says that. The words are the policy's
  (`policy/job-review.toml`).
- **The check due stays owed until the orderer moves.** Its instruction is a standing notice
  (`docs/agent-control.md`, 「An item's change is told to its orderer」): told again at every start of agency
  and every `retell_seconds` after it was last told (`policy/job-notices.toml`), under the same kind, so the
  receiver replaces the copy still pending, for as long as the job stands under review with that check due,
  no turn going out and no owner's question out. A move of the orderer's lets it go once the move is made,
  and agency's sender, before each of its passes, lets go of one whose job no longer stands so, however that
  came; a check that moved to the next is owed afresh. The words a check is told with are written on the job as
  it comes due, so a check whose row a restart of agency cut off before it was written is owed by the sender
  once `owe_grace_seconds` have passed since it came due (`docs/agent-control.md`, 「An item's change is told to
  its orderer」).
- **A check passed moves to the next;** the second passed passes the stage.

## What the orderer may do

The orderer gives its verdict on each instruction by calling `review_job` (`POST /job-reviews`, `docs/api.md`,
「assignments and jobs」), naming the job, one action and its words, and by nothing else.

| Action | Words | What follows |
|---|---|---|
| `ask` | A question for the worker on the check due | The question goes into the job's session as a turn; the worker's answer is written as the round's answer, and the orderer is told the same check again with the answer |
| `reject` | Why the work goes back, and what is to be done; optionally the `stage` it goes back to | The rejection is counted and the words go into the job's session as a work turn — of the stage named where it names a stage already reached, of the stage under way otherwise; its report opens the next round, from the first check |
| `pass` | Optional: what the check found | The check passes; the next check is told, or the stage passes and the next stage, the final report or the job's end follows |
| `escalate` | What the owner is asked, and why | The work is put to the owner (「The owner's word」 below) |

- **The call is the verdict, and the room's reply carries nothing for it:** no action's name and no account of
  the verdict. The instruction may come into a round that is already writing a reply to the room, as words
  pushed into a working turn (`docs/agent-control.md`, 「Words that arrive while a turn is working」), and the
  receiver keeps the reply that round stands on where the answer after the push is its keep mark,
  `<<KEEP>>`. So the instruction tells the supervisor to answer `<<KEEP>>` where it has nothing to add, never an
  empty answer, which a turn's door reads as a turn that wrote no answer, and never a mark that sends
  nothing, which would drop that reply; and, where the instruction came in a round of its own with nothing for
  the room, a skip naming why.
- **The call answers where the job now stands, and nothing of its report:** the job's id, its item, its state
  and its review's position — the stage, how many stages there are, the round, the check due, the rejections
  and the owner's question — so the verdict's answer never carries the words the instruction already told.
- **Any reason to send the work back sends it back.** While the job's work has been sent back fewer times than
  the review policy's ceiling, ten (`policy/job-review.toml`, `escalate_after`), `escalate` is refused 409
  `escalation-early`: the supervisor's only moves are to ask, to send it back or to pass it, and the manual
  holds it to send back whatever falls short.
- **A check that needs the job's own commit live is the orderer's to make possible.** A job never deploys and
  moves no state of its item, so such a check is reported as not confirmed, naming the commit; the orderer
  puts that commit live while the job stays under review and sends the stage that holds the check back with
  the words that it is live. The job is never held to wait on its own deployment.
- **Past the ceiling the choice is the supervisor's own:** send it back again, or put it to the owner.
- **An action that does not fit the job is refused:** a job not under review is 409 `not-reviewing`, an action
  outside the four 400 `bad-request`, an `ask`, `reject` or `escalate` without words 400 `bad-request`, a
  `reject` naming a stage the job has not reached, or a job of one stage, 400 `bad-request`, a job
  whose work waits on the owner's question 409 `awaits-owner` for every action but `reject` and `pass` once the
  owner has answered.
- **Who may act:** the supervisor whose harness plugin calls, carrying a turn now (409 `no-turn` otherwise), as
  `POST /item-filings` reads it; the job need not be in the room of that turn, since a review instruction is
  taken in whatever room the orderer reads it.

## The owner's word

- **`escalate` asks the owner through the job service:** a question on the item (`POST /questions`), asked by
  agency, its text the supervisor's words with the job, the stage and the rejections named and what each answer
  leads to, written for an owner who knows nothing of the work, its choices to send the work back or to take it as
  it stands, each saying what follows, and its due time the policy's `owner_wait_seconds`, sixty seconds, the job
  service's question wait, which holds any longer wait to it. It is put to a person, the owner, since the supervisor
  itself escalates it: agency asks it with the supervisors' own token (`BMO_JOB_CHAT_TOKEN`), the one the job
  service lets ask a person, and names the person as its addressee; the question is told to the orderer as every
  question asked of the owner is (`question-awaits-answer`), and the orderer puts it to him.
- **The owner is asked once for one question.** Before asking, the review reads the item's questions, and one
  still waiting (`open` or `paused`) with the same asker, words and choices is the job's question: an
  escalation made again after its question landed but its answer did not is written onto that one, so no second
  row is opened and nothing is told twice. Where the item's questions cannot be read, the escalation is refused
  `upstream-failed` rather than risk a second row.
- **Agency moves no question's state.** A question leaves `open` only by what the job service does with it: a
  settling reply answers it, and its asker withdraws it or closes it unsent. Its instruction is withdrawn only
  once a settling reply comes; a word that settles nothing withdraws nothing.
- **A review never ends a job's wait on its own question.** A job's `bmo job ask` withdraws its question only
  when its run is stopped. A review comes between the job's turns, so it never reaches a call still waiting,
  and while the item's question waits the call clock neither asks about that call nor stops it for its stall.
  Only a real stop of the job ends that wait (`docs/agent-control.md`, 「A run's tool calls are held to a
  clock」).
- **While it waits the job stays under review** and every action of the orderer's is refused `awaits-owner`.
- **Past its due time with no answer, the job gives its checkout and its agent's seat up:** its assignment is
  released, so the repository's checkout takes the next launch, every checkout lent to it is parked onto its
  branch and given back (`docs/worker-slots.md`, 「Loans across a job's review」), its agent is free for other work, and the
  orderer is told `job-review-parked`, to launch the work that can go now; this is the one pause in which an
  agent leaves a job before its end. The item stays leased, so no launch takes it as new work, and the job keeps its session.
- **The owner's settling answer is written as the round's ruling** and told to the orderer as the check due, with
  the owner's words; the orderer then sends the work back, folding the owner's word into the rework, or passes
  it.
- **A job that gave its checkout up takes it again before its next turn:** the same checkout, held for the job
  as a launch holds one, waiting for it to come free however long that takes, since the job's session and its
  commits stand there; then a checkout of every repository it held a loan of, chosen and waited for as its
  launch took them, the turn's words opening with where each now stands (`docs/worker-slots.md`, 「Loans across
  a job's review」); and its agent's seat, asked in the line for that agent and waited for however long that
  takes, since the review's move asked for that turn. A job stopped while it waits for any of them goes no
  further.

## The turns the review asks for

- **Each goes on in the job's own session:** the harness session the job's last turn answered on, named by the
  job, resumed whatever the role's own setting for keeping sessions and however long the review took, since
  the conversation is the job's work. Where it cannot be resumed — the road moved to another provider, model
  or effort, the prefix it was opened on moved, or, on a program whose session record agency reads, that
  record is gone — the session is asked for its handoff first and noted on the item (`docs/sessions.md`, 「The
  handoff a job's session leaves」), a new session opens, and the turn's prompt opens with the item's prompt, the
  handoff and the job's review so far, read from the job service, before the turn's own words.
- **Each goes out on the seat the job holds,** asking for no slot of its own, so the job's next stage, question
  or rework runs on its agent before any other job waiting for that agent starts; it goes out on the job's road
  as `docs/cache-fallback.md` resolves it. Only a job its review paused, its seat given back
  (「The owner's word」 above), asks for a slot of its role again, as its first turn asked it.
- **What each turn is told is the policy's** (`policy/job-review.toml`): the words the review hands over, said
  to be the review's, and what the turn ends with — the answer to the question, or the report of the rework or
  of the stage.
- **Each carries the job's words that waited for it:** words said to the job and notes added to its item while
  it stood under review, or while its last turn could not take them, follow the turn's own words behind one
  paragraph (`docs/agent-control.md`, 「Words for a running job」), so what the owner or the orderer added
  between stages reaches the next stage without a new job.

## The record

- **Every move is an entry of the item's review in the job service** (the job service's `docs/api.md`,
  「Reviews」), written by agency as the move is made, its `ref` `job:<jobId>`, its stage the stage's number or
  none for the final report, and its round:
  - `opened` as the job is launched, naming the stages;
  - `report` for each report, the worker's words whole;
  - `question` and `answer` for each question and its answer;
  - `rejection` for each time the work is sent back, the supervisor's words its reason, under the stage the
    work went back to;
  - `pass` for each check passed, its label `check` naming it;
  - `escalation` and `ruling` for the work put to the owner and the owner's word;
  - `accepted` as the job's work is taken whole, before the job ends.
- **An entry that could not be written is said in the job's log** and stops nothing.
- **The rejections are counted twice:** the job keeps its own, which the ceiling reads, and the item answers
  every rejection of its review, which the console shows as a badge on the item's row.

## The job's end

- **Once the last report passes, the job ends `ended`** with that report as its report, and its end is told to
  the orderer as `job-ended` (`docs/api.md`, 「assignments and jobs」): its assignment released, its slot given
  back, its lease kept for the item's close. The notice names that report as the one the review passed and the
  item it stands on, and carries none of its words, which the review's checks already told; `job_reviews` or
  the item reads it again. The supervisor closes the item as its procedure says.
- **A job that ends any other way** — stopped, failed, blocked — ends with its review left as it stood on the
  item, and its notice carries its report whole, since no check examined it.
