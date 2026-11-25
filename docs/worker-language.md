# The language between agency and its workers

A job worker is an agent, and what only an agent reads is English. A result it writes for a person is
the exception: its report, the account it closes its item with, a document it files for the archive, a
measurement report or a research write-up is written in the language of the person who asked
(`docs/agent-documents.md`). Its notes, a question it puts to the owner, which the supervisor puts to
him in his own language, and what it writes into git are English. So the worker reads English, writes
English for agents and the asker's language for the asker, and every other translation in between is
done on the way in or on the way out, never by the worker. Agency's part is what reaches a
worker through it and what a worker hands back through it; the rest — the item's record, the
screens, the rooms — is read in the owner's language by the units that show it.

## The rule

- **What reaches a worker is English.** Every text a worker is handed or reads through agency — a
  job's prompt, words said to the worker, the owner's words in the worker's preamble, a co-work
  step's texts, an item as the queue tools show it — reaches the worker in English.
- **Whoever writes for a worker writes in English at the source.** A caller that hands a job its
  prompt, or a change to it, writes it in English, so nothing waits on a translator and nothing is
  lost to one. Only what a person wrote for a worker directly — words said to the worker, the
  preamble — is translated by agency on its way.
- **The owner's own words are quoted only where the wording is itself the requirement.** A name, a
  label or a sentence to be used as said stands in a ```` ```text ```` fence exactly as said, and an
  English rendering in ordinary prose follows right after the fence, saying all it says and nothing
  more. The fence is code to every language reading and every translator here, so the original
  reaches the worker untouched.
- **A result written for a person is in the asker's language.** A job's report, the account it closes
  its item with, a document it files for the archive, a measurement report and a research write-up are
  written in the language of the person who asked: the item's requester, else its room, as the item's
  brief names it, the same rule a search's result keeps with its question's language. Numbers, tables,
  code, commands, paths, identifiers, sources and quoted words stand as they are. A brief that names no
  language leaves the result in English. A run on a model that writes only some languages is the one
  exception (「A model that writes only some languages」).
- **Every worker reasons in English,** whatever language its brief, its item or a person writes in: its
  thinking, its plans and what it says to itself between its tools. The worker's standing rules say so.
- **What else a worker writes back is English:** its notes, the reasons it holds or stops an item for,
  a question it puts to the owner and a subagent's prompt and report. The worker writes in no persona
  and never addresses the owner.
- **The worker never translates.** A worker that meets another language anyway — a quoted domain
  term, an error message, a file's own contents — reads it as it stands and never renders it for
  anyone.
- **What is never translated:** code identifiers, paths, commands, product and service names, error
  messages, names of address, and domain terms a reader needs to match against the code or the
  screens.
- **What a worker writes into git is English only.** The worker's standing rules say so
  (`prompts/agents/worker/standing-rules.md`, 「Language」), and the job unit's close refuses a commit that
  breaks it (`docs/worker-slots.md`, 「Landing on main」). Every repository a worker lands in is held
  to it, since every worker reads the same standing rules.

## A model that writes only some languages

- **A registration names the languages a run on it writes a result for a person in,** or none, which
  means every language (`docs/own-llm.md`, 「What a registration holds」). The owner sets them, for a model
  whose supported languages lack the language a person asks in or that writes it badly; they are
  settings, and no model or language is named in the code.
- **A run on the worker line on such a registration is told the list** in the result-language piece its
  rules carry (`docs/own-llm.md`, 「A run on the worker line is composed as on Claude」). Where its brief
  names a result language outside the list, it writes every result for a person in English: the report,
  the account it closes its item with, and every document it files for a person. It writes nothing in
  the asker's language and translates nothing; the person reads the English and a translation into
  their own language shown beside it, made by the units that show it. Where the brief names one of the
  listed languages, or names none, the rule above holds as written.
- **A run on any other road writes the asker's language,** as the rule above says: a registration
  naming no languages, Claude Code, Codex and every member road alike.
- **The review takes the English report as the run's own language.** The supervisor examining a job is
  told which language the run writes its results in, so an English report from such a run is never sent
  back for its language (`docs/job-review.md`, 「The checks」).

## What agency translates on the way in

- **The job run, as it composes the prompt a fresh session opens on,** translates the item's title in
  the heading the prompt opens on and the owner's part of the worker's preamble (`docs/worker-bench.md`,
  「Preamble」), both asked at once, before the prompt goes in: the job's first turn, and a turn carried
  into a fresh session. Agency's header under the title is English at its source and goes in as
  written. A title or a preamble that could not be rendered goes in under the mark (「When there is no
  translation」), and the job starts all the same.
- **The job run, as it hands words to a running job,** translates them: a word said to the worker and
  a note of its item alike, each as it is offered to the turn working now and as a later turn carries
  it behind its own prompt. The word is kept and logged as said (`docs/agent-control.md`, 「Words for a
  running job」); only what goes into the turn is the English. A change to the brief is English already
  and goes in as written.
- **An item's title is asked of the gate ahead of any job.** As the job service tells of an item raised,
  or of one changed with its state left where it stood, which is where a new title shows, agency asks the
  gate for the title's translation in a task nothing waits on, bounded like every hand-over. The gate
  keeps it, so the queue tools find it kept when a worker reads the queue, its own item and the rows
  beside it alike. A title in the Latin script asks nothing.
- **The queue tools show the titles as the gate keeps their translations** and the rest of an item as
  written (`docs/worker-mcp.md`): its brief is English at its source, its notes reach a running job
  translated as words handed to it, and its questions and reports are a worker's own.
- **A co-work step's prompt** is composed by the job unit, which translates what it carries before it
  enqueues the step; an earlier stage's result is a co-worker's own English and passes untouched.
- **No translation for a job's prompt.** Whoever hands a job its prompt, agency hands it to the
  worker as written. A part in another language that a person handed
  over anyway reaches the worker as written, and the worker reads it as it stands.
- **The hand that translates is the gate's,** asked through its `POST /translations` into the worker's
  language (the gate's `docs/api.md`, 「Translations」). Its translator (`translator`) runs outside
  agency (`docs/own-llm.md`, 「Gates are not agency's runs」), under instructions for a worker: quoted
  speech is translated too, since the person who said it is not the reader. A part already in English,
  a fenced quote in it included, passes untouched and asks nothing of the translator.
- **Nothing a worker runs calls the translator.** The queue tools read only the translations the gate
  keeps, never asking its translator, so a worker's reading never waits on a model.

## Where the translation is made

- **The gate decides the language, part by part.** It cuts a text into parts, leaves a part already in
  English exactly as written and translates the others, and answers each text's reading: whole, or with
  the parts it could not render standing as written (the gate's `docs/api.md`, 「Translations」). Agency
  cuts nothing and judges no part itself.
- **A text with no letter outside the Latin script is never sent.** The gate would leave every part of
  it as written, so it goes in as it stands and its hand-over waits on nothing; a text holding a letter
  of any other script is the gate's to read.
- **The texts of one hand-over are asked together,** in calls of no more originals than the gate takes
  in one, and each comes back in the order asked.

## What is kept

- **The gate keeps one translation per original and language,** so an unchanged original is answered
  from what is kept and never translated twice, whichever preamble, word or job it stands in, while a
  changed one is translated afresh.
- **The original is never overwritten.** The repository's preamble stays as the owner wrote it, a job's
  word as it was said and an item's title as it was filed; the translation is a reading of them, kept
  by the gate beside them.
- **Only a translation that passed the structure check is kept.** Every part the translator answers is
  held to the check that it is a translation — not a line introducing one, a word about the gate's role,
  a question back or a summary — and has no translation where it fails.

## When there is no translation

- **The job is never held for a translation.** The gate queues a translation behind those asked before
  it and waits however long its translator takes, so agency waits for each hand-over's answer no
  longer than its own bound, `policy/job-prompt.toml`'s `[translation]` `wait_ms`, and then goes on
  with the texts as written. A gate that is not deployed, not reached or answering out of shape is no
  translation the same way.
- **A text with no translation goes in its original, marked:** a line in English before it says it
  could not be translated and stands in its original language. A reading the gate answered as not
  whole is used under the same mark, since half a text in English is better than none.
- **A title stands inside a line,** so it wears its mark after it rather than a line over it:
  `[not translated]`, the words `policy/job-prompt.toml`'s `[translation]` `untranslated_title` gives,
  which are the words the queue tools mark a title with.
- **A failure is not kept.** The next reading that needs the text asks again.
- **An item's instruction keeps English heads even then.** The heads are the queue's own words, so
  the worker still reads each block's label and the fixed words of its source line in English; the
  words inside each block stay as written under the mark.

## What agency writes for a worker

- **Text no person wrote for the moment is English at its source.** The preamble's header, the job's
  trailer, the header over a job's turns, a co-worker place's instructions, and every answer and
  refusal of the worker's tools (`docs/worker-mcp.md`) are written in English where they are kept, so
  nothing waits on the translator and nothing is kept twice.
- **The heading a job's prompt opens on names the item as the queue tools do:** its title in the
  worker's language, or, where the gate gave no whole reading in time, the title as written with
  `[not translated]` after it.
- **What it quotes stays as written:** a title as a person filed it, a phrase a verb was handed, a row
  a query read.

## What a worker hands back

- **A job's run ends with the worker's report,** in its result language — the asker's, or English on a
  model that writes only some languages where the asker's is not among them — returned whole in the run's
  result and in its run record (`docs/run-records.md`). Agency renders nothing of it into the owner's language and
  sends it to nobody: the caller that started the job reads it, and telling the owner is the chat
  agent's.
- **What a worker writes onto an item** goes through the job unit's verbs, which keep the English and
  render it for people to read (the job unit's `todo.md`). Agency's tools hand those verbs the
  worker's text as written.
- **A worker's answer on its own console screen** when the owner speaks to it directly with no job
  behind it is the one place a worker answers in the owner's language: the owner reads it there, with
  nothing retelling it. The owner's words still reach the worker in English.
