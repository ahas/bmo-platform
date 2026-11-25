//! A run's tool calls held to a clock (docs/agent-control.md, 「A run's tool calls are held to a clock」). A
//! call is read off the turn's record: its `tool_use` block until the `tool_result` that answers it. The
//! process tree of the turn's program is read as the kernel names it, and what the call owns is the part of
//! that tree that began with the call.
//!
//! A call that has burned no processor time, begun no process and written no output for its stall is
//! stopped: its process tree is signalled, the program's tool comes back to the model as a failed result,
//! and the turn goes on. A call that keeps burning processor time — a build, a test, a search — or whose
//! output keeps growing is never touched, however long it runs. On a stop,
//! a call with no processor time behind it is stopped the same way, so the stop's words can be read at the
//! next boundary instead of waiting behind a blocked read.
//!
//! A call that writes no output while its processes stand nearly idle — a wait that polls, which begins a
//! process on every pass and so never stalls — is not cut by the clock: it is handed to the caller as a
//! silent call, whose overtime policy asks the supervisor and answers keep, cut or wait for a verdict. The
//! call's output is what its processes write to the regular files their standard output and standard error
//! stand on. A call that waits on the job's own work — its command naming a file another living process of
//! the program's tree writes — has its silent window counted from the last read that saw that work alive.
//! Before its stall stops a call, the caller is asked whether the job's own question still waits: a wait on
//! an answer burns nothing, and the stop's signal would withdraw the question, so such a call is found
//! waiting on it instead.
//!
//! A call that holds no process of its own waits inside the program — on a socket, on a pipe — and leaves
//! nothing in the tree to read. Past a bound of its own it is cut too, but not by a signal: words are put
//! into the working turn saying the call did not complete, and the program ends the waiting call and hands
//! the model a failed result. A program that does not take the words ends the turn at the grace.
//!
//! The same reads keep the turn's progress as one moment (`super::stall`): the record's new lines, each call's
//! output grown with a new last line, and the processor time each call's tree, and the job's own work a call
//! waits on, burned. A turn that stands still past the stall span is handed to the caller once a stall.
//!
//! The notes this writes go to the run's log and to the unit's own log, so a hang is readable while it is
//! happening: the call's start with its pid and a one-line summary, a still-running line past a threshold,
//! and its end with its duration and outcome. Each is a `ClockNote`, its facts kept beside its sentence.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::time::Duration;

use agent_host::identity::{ProcessStat, boot_ms, clock_ticks, process_stat};
use policy::tool_clock::CallClock;
use serde_json::Value;

pub mod notes;

use super::stall::{Progress, StallChange, StallClock, Waits};

pub use notes::{ClockEvent, ClockNote, Cut, Outcome};

/// How far before a call's record line a process may have begun and still be the call's own: the kernel's
/// start time is kept to the second, and the record is written a moment before the process starts.
const START_SLACK_MS: i64 = 3_000;

/// How long a stopped call's processes are given to end on their own before they are killed.
const TERM_GRACE: Duration = Duration::from_millis(250);

/// How much of the end of a silent call's output is read for its last line.
const TAIL_BYTES: u64 = 4_096;

/// How many characters of a call's command its lines carry.
const LINE_CHARS: usize = 160;

/// The numbers a call is held to, from the policy or a test's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolClock {
  pub min_ms: i64,
  pub stall_ms: i64,
  pub look_ms: i64,
  pub in_process_ms: i64,
  pub in_process_grace_ms: i64,
  pub in_process_cut: String,
  pub in_process_grace_cut: String,
  pub log_after_ms: i64,
  pub still_every_ms: i64,
  pub stop_stall_ms: i64,
  pub stop_bound_ms: i64,
  pub silent: SilentClock,
}

/// The numbers a call gone silent and idle is asked about by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SilentClock {
  /// How long the call writes no output, its processes idle, before it is asked about.
  pub window_ms: i64,
  /// The share of one processor, in percent across the window, under which its processes are idle.
  pub idle_cpu_percent: u64,
  /// How many characters of its command and of its output's last line an ask carries.
  pub ask_chars: usize,
  /// The words put into the turn when the supervisor cuts it: `{tool}`, `{seconds}` and `{why}` filled.
  pub cut: String,
}

impl From<&CallClock> for ToolClock {
  fn from(call: &CallClock) -> Self {
    let ms = |seconds: u64| i64::try_from(seconds).unwrap_or(i64::MAX / 1_000).saturating_mul(1_000);

    Self {
      min_ms: ms(call.min_seconds),
      stall_ms: ms(call.stall_seconds),
      look_ms: ms(call.look_seconds),
      in_process_ms: ms(call.in_process_seconds),
      in_process_grace_ms: ms(call.in_process_grace_seconds),
      in_process_cut: call.in_process_cut.clone(),
      in_process_grace_cut: call.in_process_grace_cut.clone(),
      log_after_ms: ms(call.log_after_seconds),
      still_every_ms: ms(call.still_every_seconds),
      stop_stall_ms: ms(call.stop_stall_seconds),
      stop_bound_ms: ms(call.stop_bound_seconds),
      silent: SilentClock {
        window_ms: ms(call.silent.window_seconds),
        idle_cpu_percent: call.silent.idle_cpu_percent,
        ask_chars: call.silent.ask_chars,
        cut: call.silent.cut.clone(),
      },
    }
  }
}

/// One tool call as the record names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolUse {
  pub id: String,
  pub name: String,
  pub input: Value,
  pub started_at: Option<i64>,
}

impl ToolUse {
  /// A one-line summary of what the call was given: the command a shell was handed, else the description
  /// or the path, flattened and cut.
  pub fn summary(&self) -> String {
    self.summary_of(LINE_CHARS)
  }

  /// The summary cut at `chars` characters.
  pub fn summary_of(&self, chars: usize) -> String {
    let field = |name: &str| self.input.get(name).and_then(Value::as_str).filter(|one| !one.is_empty());
    let said = field("command")
      .or_else(|| field("description"))
      .or_else(|| field("file_path"))
      .or_else(|| field("path"))
      .or_else(|| field("pattern"))
      .or_else(|| field("url"))
      .or_else(|| field("query"))
      .unwrap_or_default();
    let flat = said.split_whitespace().collect::<Vec<_>>().join(" ");

    flat.chars().take(chars).collect()
  }

  /// What the call is called in a line: its tool's name, and the summary where it has one.
  pub fn words(&self) -> String {
    let after = self.summary();

    if after.is_empty() { self.name.clone() } else { format!("{} {}", self.name, after) }
  }
}

// A process as the call's own tree: its pid, parent, processor time, that of the children it reaped, and
// when it began.
#[derive(Clone, Debug)]
struct Proc {
  pid: i32,
  ppid: i32,
  cpu: u64,
  reaped_cpu: u64,
  started_at: i64,
  cmdline: String,
}

// Where a call stands with the supervisor's overtime ask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Asking {
  // No ask stands: the call is handed over once it is silent and idle.
  #[default]
  None,
  // Handed to the caller, its answer awaited; `open` where an ask already stood.
  Due {
    open: bool,
  },
  // An ask stands with no verdict, looked for on each read.
  Open,
  // Waits on the job's own work the caller last saw moving: handed over on each read, to learn whether it
  // still moves, until the silent window from that moment has passed.
  Awaiting,
  // Kept, or an ask that could not be put, until then; handed over again then if still silent and idle.
  Held(i64),
  // The caller asks nothing about its calls.
  Never,
}

// One call the watcher is holding.
#[derive(Clone, Debug, Default)]
struct Call {
  name: String,
  summary: String,
  // What an ask carries of the call: its tool and its command, cut to the ask's length.
  asked_as: String,
  started_at: i64,
  pids: Vec<i32>,
  cpu: HashMap<i32, u64>,
  last_progress_at: i64,
  started_said: bool,
  still_said_at: i64,
  cut: bool,
  // A call cut while it held no process of its own: its words are put into the turn, and a program that
  // does not take them ends the turn at the grace.
  in_process: bool,
  // The files its processes write their output to, by the path the kernel names, with the size each
  // stood at and the descriptor it was last read through; and when any of them last changed.
  outputs: BTreeMap<String, (u64, String)>,
  output_changed_at: i64,
  last_written: Option<String>,
  // The processor time its processes and the children they reaped stood at on the last read, the time
  // burned since the call began, and that time as it stood at each read across the silent window.
  tree_cpu: u64,
  burned: u64,
  burned_at: VecDeque<(i64, u64)>,
  asking: Asking,
  // Whether an ask was ever opened on it, so its end closes it.
  asked: bool,
  // The last moment the job's own work it waits on was seen moving; its silent window counts from here
  // where that is later than its output's last growth.
  awaited_at: i64,
  // The file the job's own work in the background wrote while the call waited on it, and the last moment it
  // was seen moving.
  work: Option<WaitedWork>,
  // Until when the supervisor kept it.
  kept_until: i64,
  // The moment of the read before and the processor time its tree had burned by then; the same of the tree
  // of the job's own work it waits on.
  burned_read: Option<(i64, u64)>,
  work_read: Option<(i64, u64)>,
}

impl Call {
  // A note on the call, `elapsed` its time so far where the note tells it.
  fn note(&self, run: &str, id: &str, elapsed: Option<i64>, event: ClockEvent) -> ClockNote {
    ClockNote {
      run: run.to_owned(),
      call: Some(id.to_owned()),
      tool: self.name.clone(),
      command: self.summary.clone(),
      pid: self.pids.first().copied(),
      elapsed_ms: elapsed.map(|elapsed| elapsed.max(0)),
      event,
    }
  }

  fn words(&self) -> String {
    if self.summary.is_empty() {
      self.name.clone()
    } else {
      format!("{} {}", self.name, self.summary)
    }
  }
}

/// A call cut while it had no process of its own: the words to put into the working turn, and what the
/// call was. A program that takes the words ends the waiting call and hands the model a failed result;
/// one that does not is ended at the grace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InProcessCut {
  pub tool: String,
  pub at: i64,
  pub seconds: u64,
  pub words: String,
}

/// The job's own work in the background a call waited on: the file its process wrote, which the call's
/// command names, and the last moment that process was seen alive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaitedWork {
  pub path: String,
  pub moved_at: i64,
}

/// A call that owns processes and has gone silent and idle, handed to the caller's overtime policy: the
/// facts an ask on it carries.
#[derive(Clone, Debug, PartialEq)]
pub struct SilentCall {
  /// The call's id in the record.
  pub id: String,
  /// Its tool and its command, cut to the ask's length.
  pub tool: String,
  pub started_at: i64,
  /// When its output last changed, its start where it never did.
  pub output_changed_at: i64,
  /// When its silence began as the clock counts it: its output's last change, or the last moment the job's
  /// own work it waited on was seen moving, whichever is later.
  pub quiet_since: i64,
  /// The job's own work in the background it waited on, where it waited on any.
  pub work: Option<WaitedWork>,
  /// The last line its output holds, none where it has written nothing.
  pub last_line: Option<String>,
  /// The share of one processor its processes burned across the window, in percent.
  pub cpu_percent: f64,
  pub window_ms: i64,
}

/// The caller's answer on a silent call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallVerdict {
  /// The caller asks nothing: the call runs on and is not handed over again.
  NotAsked,
  /// The call waits on the job's own work, last seen moving at this moment: no ask is opened, its silent
  /// window and its stall count from then, and it is handed over on each read until that window has passed.
  Awaiting(i64),
  /// An ask stands with no verdict: the call runs on, and is handed over again at the next read.
  Waiting,
  /// Kept until this moment; handed over again then only if it is still silent and idle.
  Kept(i64),
  /// Cut on the supervisor's call, for this reason.
  Cut(String),
  /// The ask could not be put: the call runs on, handed over again once the window has passed.
  Failed(String),
}

/// What an answer on a silent call came to: the notes to keep, and the words to put into the working turn
/// where it was cut.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answered {
  pub lines: Vec<ClockNote>,
  pub words: Option<String>,
}

/// Whether the job's own question still waits for its answer, asked of the caller before a call that owns
/// processes is stopped for its stall: a wait on an answer burns nothing, and the stop's signal would end it.
pub trait QuestionWait {
  /// Whether the call waits on the job's own question, still waiting for its answer.
  fn waits_on_question(&mut self, call: &str) -> impl Future<Output = bool> + Send;
}

/// A caller whose run asks no question: every call due for its stall is stopped.
pub struct NoQuestion;

impl QuestionWait for NoQuestion {
  async fn waits_on_question(&mut self, _call: &str) -> bool {
    false
  }
}

/// A turn's tool calls watched over a turn.
#[derive(Debug)]
pub struct Watch {
  clock: ToolClock,
  calls: BTreeMap<String, Call>,
  last_look: i64,
  in_process: VecDeque<InProcessCut>,
  silent: VecDeque<SilentCall>,
  closed_asks: Vec<String>,
  progress: Progress,
  subagents_read_at: i64,
}

impl Watch {
  /// A watch over a turn just put in, its stall judged by the policy's numbers.
  pub fn new(clock: ToolClock) -> Self {
    Self::stalling(clock, Progress::new(StallClock::policy()))
  }

  /// A watch over a turn already going, taken up as after a restart: a stall told before it began may stand.
  pub fn taken_up(clock: ToolClock) -> Self {
    Self::stalling(clock, Progress::taken_up(StallClock::policy()))
  }

  /// A watch whose turn's progress is kept by `progress`.
  pub fn stalling(clock: ToolClock, progress: Progress) -> Self {
    Self {
      clock,
      calls: BTreeMap::new(),
      last_look: i64::MIN / 2,
      in_process: VecDeque::new(),
      silent: VecDeque::new(),
      closed_asks: vec![],
      progress,
      subagents_read_at: i64::MIN / 2,
    }
  }

  /// What became of the turn's progress since this was last asked: a stall, or a move after one.
  pub fn take_stall(&mut self) -> Option<StallChange> {
    self.progress.take()
  }

  /// The subagents' own records beside the turn's `record` read, at most once a look: one grown is progress.
  pub fn subagent_records(&mut self, record: &Path, now: i64) {
    if now - self.subagents_read_at < self.clock.look_ms {
      return;
    }

    self.subagents_read_at = now;

    let Some(session) = record.to_str().and_then(|path| path.strip_suffix(".jsonl")) else {
      return;
    };
    let Ok(entries) = std::fs::read_dir(Path::new(session).join("subagents")) else {
      return;
    };

    for entry in entries.flatten() {
      let Ok(meta) = entry.metadata() else { continue };

      if meta.is_file() {
        self.progress.output(&entry.path().to_string_lossy(), meta.len(), || None, now);
      }
    }
  }

  /// A call gone silent and idle, or one whose ask stands, to hand to the caller's overtime policy; its
  /// answer goes back through `answer`.
  pub fn take_silent(&mut self) -> Option<SilentCall> {
    self.silent.pop_front()
  }

  /// The calls asked about that have ended since this was last asked, whose asks are to close as ended.
  pub fn take_closed_asks(&mut self) -> Vec<String> {
    std::mem::take(&mut self.closed_asks)
  }

  /// The caller's answer on a silent call carried out: a keep holds it, a cut stops its processes and
  /// gives the words for the turn, and an ask with no verdict is looked at again on the next read.
  pub async fn answer(&mut self, name: &str, id: &str, verdict: CallVerdict, now: i64) -> Answered {
    let window_ms = self.clock.silent.window_ms;
    let cut_words = self.clock.silent.cut.clone();
    let Some(call) = self.calls.get_mut(id) else { return Answered::default() };
    let opened = !matches!(call.asking, Asking::Due { open: true });
    let elapsed = (now - call.started_at).max(0);
    let mut answered = Answered::default();

    match verdict {
      CallVerdict::NotAsked => call.asking = Asking::Never,
      CallVerdict::Awaiting(moved_at) => {
        call.asking = Asking::Awaiting;
        call.awaited_at = call.awaited_at.max(moved_at);
      }
      CallVerdict::Waiting => {
        call.asking = Asking::Open;
        call.asked = true;

        if opened {
          let silent_ms = (now - call.output_changed_at.max(call.awaited_at)).max(0);

          answered.lines.push(call.note(name, id, Some(elapsed), ClockEvent::Asked { silent_ms }));
        }
      }
      CallVerdict::Kept(until) => {
        call.asking = Asking::Held(until);
        call.asked = true;
        call.kept_until = until;

        let for_ms = (until - now).max(0);

        answered.lines.push(call.note(name, id, Some(elapsed), ClockEvent::Kept { for_ms }));
      }
      CallVerdict::Failed(why) => {
        call.asking = Asking::Held(now + window_ms);
        answered.lines.push(call.note(name, id, Some(elapsed), ClockEvent::AskFailed { why }));
      }
      CallVerdict::Cut(why) => {
        call.asking = Asking::Never;
        call.asked = true;
        call.cut = true;

        let seconds = u64::try_from(elapsed / 1_000).unwrap_or_default();

        answered.words = Some(policy::fill_marks(
          &cut_words,
          &[("tool", &call.words()), ("seconds", &seconds.to_string()), ("why", why.trim())],
        ));
        answered
          .lines
          .push(call.note(name, id, Some(elapsed), ClockEvent::Cut(Cut::Supervisor(why))));

        let pids = call.pids.clone();

        stop(&pids).await;
      }
    }

    answered
  }

  /// A call just cut because it held no process of its own, whose words are yet to go into the turn.
  pub fn take_in_process_cut(&mut self) -> Option<InProcessCut> {
    self.in_process.pop_front()
  }

  /// Whether a call cut with no process of its own still stands unanswered, so the words did not end it.
  pub fn in_process_cut_still(&self) -> bool {
    self.calls.values().any(|call| call.in_process)
  }

  /// Watch the record once: what began, what ended, what is still going, and what has stopped making
  /// progress. `program` is the pid of the turn's program, whose tree the call's processes are part of;
  /// `question` is asked, of a call due for its stall alone, whether it waits on the job's own question.
  pub async fn look(
    &mut self,
    name: &str,
    records: &[Value],
    from: usize,
    program: i32,
    now: i64,
    question: &mut impl QuestionWait,
  ) -> Vec<ClockNote> {
    let uses = tool_uses(records, from);
    let results = tool_results(records, from);
    let mut lines = vec![];

    self.progress.records(records, from, now);

    for used in &uses {
      if !self.calls.contains_key(&used.id) {
        self.calls.insert(
          used.id.clone(),
          Call {
            name: used.name.clone(),
            summary: used.summary(),
            asked_as: format!("{} {}", used.name, used.summary_of(self.clock.silent.ask_chars))
              .trim_end()
              .to_owned(),
            started_at: used.started_at.unwrap_or(now),
            output_changed_at: used.started_at.unwrap_or(now),
            last_progress_at: now,
            ..Call::default()
          },
        );
      }
    }

    // A call the record now answers is over. It is read before the process scan so its own last look sees
    // it; the line is the duration it stood and what the answer said.
    let inflight: HashSet<&str> = uses.iter().map(|used| used.id.as_str()).collect();

    self.calls.retain(|id, call| {
      if inflight.contains(id.as_str()) {
        return true;
      }

      if call.asked {
        self.closed_asks.push(id.clone());
      }

      for path in call.outputs.keys() {
        self.progress.forget(path);
      }

      let outcome = match results.get(id) {
        Some(true) => Outcome::Failed,
        Some(false) => Outcome::Completed,
        None => Outcome::EndedWithTurn,
      };

      lines.push(call.note(name, id, Some(now - call.started_at), ClockEvent::Ended(outcome)));

      false
    });

    let scanning = now - self.last_look >= self.clock.look_ms;
    let procs = scanning.then(scan);

    if scanning {
      self.last_look = now;

      for used in &uses {
        let Some(call) = self.calls.get_mut(&used.id) else { continue };
        let Some(procs) = &procs else { continue };
        let owned = owned_by(procs, program, call.started_at, &used.name, &used.input);

        let mut progress = false;

        for owned in &owned {
          match call.cpu.get(&owned.pid) {
            Some(cpu) if owned.cpu > *cpu => progress = true,
            None => progress = true,
            _ => {}
          }

          call.cpu.insert(owned.pid, owned.cpu);
        }

        if progress {
          call.last_progress_at = now;
        }

        // The pids are kept once a call had them: a call cut or ended leaves its last pid for its end line.
        if !owned.is_empty() {
          call.pids = owned.iter().map(|owned| owned.pid).collect();
          read_activity(call, &owned, now, self.clock.silent.window_ms);
          made_progress(&mut self.progress, call, now);

          if now - call.started_at >= self.clock.min_ms
            && let Some((path, writer)) = awaited_work(procs, program, &owned, used)
          {
            call.awaited_at = now;
            work_progress(&mut self.progress, call, procs, &path, writer, now);
            call.work = Some(WaitedWork {
              path,
              moved_at: now,
            });
          }
        }
      }
    }

    let mut cutting: Vec<Call> = vec![];

    for used in &uses {
      let Some(call) = self.calls.get_mut(&used.id) else { continue };
      let elapsed = now - call.started_at;

      // A call with no process yet may have one on its way, so its start waits a moment — no longer than
      // its own bound, since a call that never gets one is cut there.
      let waiting_for_process = call.pids.is_empty() && elapsed < 2_000.min(self.clock.in_process_ms);

      if !call.started_said && waiting_for_process {
        continue;
      }

      if !call.started_said {
        call.started_said = true;
        call.still_said_at = now;
        lines.push(call.note(name, &used.id, Some(elapsed), ClockEvent::Started));
      }

      // A call with no process of its own has nothing in the tree to read, so it is held to a bound of its
      // own; one that owns a tree is cut when that tree burns no processor time, begins no process and
      // writes no output, as a check waiting on a step run elsewhere still says it waits.
      let in_process = call.pids.is_empty();
      let due = if in_process {
        elapsed >= self.clock.in_process_ms.max(self.clock.min_ms)
      } else {
        let moved_at = call.last_progress_at.max(call.output_changed_at).max(call.awaited_at);

        elapsed >= self.clock.min_ms && now - moved_at >= self.clock.stall_ms
      };

      // A call waiting on the job's own question is found waiting on it, as a silent call handed over would
      // be, and its stall counts again from now: the cut would withdraw the question unanswered.
      if !call.cut && due && !in_process && question.waits_on_question(&used.id).await {
        call.awaited_at = now;

        if call.asking == Asking::None {
          call.asking = Asking::Awaiting;
        }

        lines.push(call.note(name, &used.id, Some(elapsed), ClockEvent::Awaiting));
        continue;
      }

      if !call.cut && due {
        call.cut = true;
        call.in_process = in_process;

        if in_process {
          let seconds = u64::try_from(elapsed.max(0) / 1_000).unwrap_or_default();
          let words = policy::fill_marks(
            &self.clock.in_process_cut,
            &[("tool", &call.words()), ("seconds", &seconds.to_string())],
          );

          self.in_process.push_back(InProcessCut {
            tool: call.words(),
            at: now,
            seconds,
            words,
          });
          lines.push(call.note(name, &used.id, Some(elapsed), ClockEvent::Cut(Cut::NoProcess)));
        } else {
          cutting.push(call.clone());
          lines.push(call.note(name, &used.id, Some(elapsed), ClockEvent::Cut(Cut::NoProgress)));
        }
      } else if call.started_said
        && elapsed >= self.clock.log_after_ms
        && now - call.still_said_at >= self.clock.still_every_ms
      {
        call.still_said_at = now;
        lines.push(call.note(name, &used.id, Some(elapsed), ClockEvent::Running));
      }
    }

    for call in &cutting {
      stop(&call.pids).await;
    }

    if scanning {
      self.hand_over_silent(&uses, now);
      self.judge_stall(&uses, now);
    }

    lines
  }

  // The turn's stall judged: told unless a call of it stands under an ask, a keep or a wait on the job's own
  // question, which the supervisor already holds; what it waits on is each call still running.
  fn judge_stall(&mut self, uses: &[ToolUse], now: i64) {
    let running: Vec<&Call> = uses
      .iter()
      .filter_map(|used| self.calls.get(&used.id))
      .filter(|call| !call.cut)
      .collect();
    let known = running.iter().any(|call| {
      matches!(call.asking, Asking::Open | Asking::Due { open: true } | Asking::Awaiting)
        || call.kept_until > now
    });
    let (lines, chars) = (self.progress.clock().lines, self.progress.clock().chars);

    self.progress.judge(now, known, || {
      running
        .iter()
        .map(|call| Waits {
          tool: call.asked_as.chars().take(chars).collect(),
          lines: waited_lines(call, lines, chars),
        })
        .collect()
    });
  }

  // Each call that owns processes and has gone silent and idle, each whose ask stands, and each the caller
  // last found waiting on the job's own work, queued for the caller's policy. A call already cut is not.
  fn hand_over_silent(&mut self, uses: &[ToolUse], now: i64) {
    let clock = &self.clock;

    for used in uses {
      let Some(call) = self.calls.get_mut(&used.id) else { continue };

      if call.cut || call.pids.is_empty() {
        continue;
      }

      let idle = idle_percent(call, now, clock);
      let open = match call.asking {
        Asking::None if idle.is_some() => false,
        Asking::Open => true,
        Asking::Awaiting if now - call.awaited_at < clock.silent.window_ms => false,
        Asking::Awaiting => {
          call.asking = Asking::None;

          if idle.is_none() {
            continue;
          }

          false
        }
        Asking::Held(until) if now >= until => {
          if idle.is_none() {
            call.asking = Asking::None;
            continue;
          }

          false
        }
        _ => continue,
      };

      call.asking = Asking::Due { open };
      self.silent.push_back(SilentCall {
        id: used.id.clone(),
        tool: call.asked_as.clone(),
        started_at: call.started_at,
        output_changed_at: call.output_changed_at,
        quiet_since: call.output_changed_at.max(call.awaited_at),
        work: call.work.clone(),
        last_line: if open { None } else { last_line(call, clock.silent.ask_chars) },
        cpu_percent: idle.unwrap_or_default(),
        window_ms: clock.silent.window_ms,
      });
    }
  }

  /// Stop the calls that are making no progress, so a stop's words can be read at the next boundary. A call
  /// that is still burning processor time is left for the stop's own bound.
  pub async fn cut_for_stop(
    &mut self,
    name: &str,
    records: &[Value],
    from: usize,
    program: i32,
    now: i64,
  ) -> Vec<ClockNote> {
    let uses = tool_uses(records, from);
    let procs = scan();
    let mut lines = vec![];

    for used in &uses {
      let Some(call) = self.calls.get_mut(&used.id) else { continue };

      if call.cut {
        continue;
      }

      let owned = owned_by(&procs, program, call.started_at, &used.name, &used.input);

      if !owned.is_empty() {
        call.pids = owned.iter().map(|owned| owned.pid).collect();
      }

      if call.pids.is_empty() {
        continue;
      }

      let elapsed = now - call.started_at;

      if elapsed < self.clock.stop_stall_ms || now - call.last_progress_at < self.clock.stop_stall_ms {
        continue;
      }

      call.cut = true;
      lines.push(call.note(name, &used.id, Some(elapsed), ClockEvent::Cut(Cut::Stop)));
      stop(&call.pids).await;
    }

    lines
  }
}

// The processes of the program's tree that began with the call, and that the call's command names where it
// names one. A process older than the call is a part of the program's own standing tree, not the call's.
fn owned_by(procs: &[Proc], program: i32, started_at: i64, tool: &str, input: &Value) -> Vec<Proc> {
  let since = started_at.saturating_sub(START_SLACK_MS);
  let mut owned = descendants(procs, program)
    .into_iter()
    .filter(|proc| proc.started_at >= since)
    .collect::<Vec<_>>();

  if tool == "Bash"
    && let Some(command) = input.get("command").and_then(Value::as_str)
  {
    let named = owned
      .iter()
      .filter(|proc| names_command(&proc.cmdline, command))
      .map(|proc| proc.pid)
      .collect::<HashSet<_>>();

    if !named.is_empty() {
      let under = descendants_of(procs, &named);

      owned.retain(|proc| under.contains(&proc.pid));
    }
  }

  owned
}

fn names_command(cmdline: &str, command: &str) -> bool {
  command
    .split_whitespace()
    .filter(|token| token.len() > 1 && !token.starts_with('-') && !token.starts_with('|'))
    .any(|token| cmdline.contains(token))
}

fn descendants(procs: &[Proc], root: i32) -> Vec<Proc> {
  let children = |pid: i32| procs.iter().filter(move |proc| proc.ppid == pid);
  let mut found = vec![];
  let mut queue = vec![root];
  let mut seen = HashSet::new();

  while let Some(pid) = queue.pop() {
    for child in children(pid) {
      if seen.insert(child.pid) {
        found.push(child.clone());
        queue.push(child.pid);
      }
    }
  }

  found
}

fn descendants_of(procs: &[Proc], roots: &HashSet<i32>) -> HashSet<i32> {
  let mut found = roots.clone();
  let mut queue: Vec<i32> = roots.iter().copied().collect();

  while let Some(pid) = queue.pop() {
    for proc in procs.iter().filter(|proc| proc.ppid == pid) {
      if found.insert(proc.pid) {
        queue.push(proc.pid);
      }
    }
  }

  found
}

fn scan() -> Vec<Proc> {
  let Some(boot) = boot_ms() else { return vec![] };
  let ticks = clock_ticks().max(1);
  let Ok(entries) = std::fs::read_dir("/proc") else { return vec![] };

  entries
    .flatten()
    .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
    .filter_map(|pid| {
      let stat: ProcessStat = process_stat(pid)?;

      if stat.state.starts_with('Z') || pid == std::process::id() as i32 {
        return None;
      }

      let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).ok().map(|line| {
        line
          .split(|byte| *byte == 0)
          .filter(|arg| !arg.is_empty())
          .map(|arg| String::from_utf8_lossy(arg).into_owned())
          .collect::<Vec<_>>()
          .join(" ")
      })?;

      let birth = i64::try_from(stat.birth.parse::<u64>().ok()?).ok()?;

      Some(Proc {
        pid,
        ppid: stat.ppid,
        cpu: stat.cpu,
        reaped_cpu: stat.reaped_cpu,
        started_at: boot.saturating_add(birth.saturating_mul(1_000) / ticks),
        cmdline,
      })
    })
    .collect()
}

// What a call's processes did since the last read: the processor time they and the children they reaped
// burned, kept across the silent window, and whether the files they write their output to changed.
fn read_activity(call: &mut Call, owned: &[Proc], now: i64, window_ms: i64) {
  let tree: u64 = owned.iter().map(|proc| proc.cpu + proc.reaped_cpu).sum();

  call.burned += tree.saturating_sub(call.tree_cpu);
  call.tree_cpu = tree;
  call.burned_at.push_back((now, call.burned));

  while call.burned_at.len() > 1 && call.burned_at[1].0 <= now - window_ms {
    call.burned_at.pop_front();
  }

  for proc in owned {
    for fd in [1, 2] {
      let through = format!("/proc/{}/fd/{fd}", proc.pid);
      let Ok(meta) = std::fs::metadata(&through) else { continue };

      if !meta.is_file() {
        continue;
      }

      let Ok(path) = std::fs::read_link(&through) else { continue };
      let path = path.to_string_lossy().into_owned();
      let size = meta.len();
      let changed = match call.outputs.get(&path) {
        Some((was, _)) => *was != size,
        None => size > 0,
      };

      if changed {
        call.output_changed_at = now;
        call.last_written = Some(path.clone());
      }

      call.outputs.insert(path, (size, through));
    }
  }
}

// What a call's own reads came to for the turn's progress: each output file at its size, its last line read
// only where it grew, and the processor time its tree burned since the read before.
fn made_progress(progress: &mut Progress, call: &mut Call, now: i64) {
  for (path, (size, through)) in &call.outputs {
    progress.output(path, *size, || tail_lines(Path::new(through), 1, usize::MAX).pop(), now);
  }

  if let Some((at, burned)) = call.burned_read {
    progress.burned(ticks_ms(call.burned.saturating_sub(burned)), now - at, now);
  }

  call.burned_read = Some((now, call.burned));
}

// What the job's own work a call waits on came to for the turn's progress: the file it writes at its size, its
// last line read only where it grew, and the processor time the writer's tree burned since the read before.
fn work_progress(
  progress: &mut Progress,
  call: &mut Call,
  procs: &[Proc],
  path: &str,
  writer: i32,
  now: i64,
) {
  if let Ok(meta) = std::fs::metadata(path) {
    progress.output(path, meta.len(), || tail_lines(Path::new(path), 1, usize::MAX).pop(), now);
  }

  let tree = descendants_of(procs, &HashSet::from([writer]));
  let burned: u64 = procs
    .iter()
    .filter(|proc| tree.contains(&proc.pid))
    .map(|proc| proc.cpu + proc.reaped_cpu)
    .sum();

  if let Some((at, was)) = call.work_read {
    progress.burned(ticks_ms(burned.saturating_sub(was)), now - at, now);
  }

  call.work_read = Some((now, burned));
}

fn ticks_ms(ticks: u64) -> i64 {
  let per_second = u64::try_from(clock_ticks().max(1)).unwrap_or(1);

  i64::try_from(ticks.saturating_mul(1_000) / per_second).unwrap_or(i64::MAX)
}

// The last lines of what a running call waits on: the file the job's own work writes where it waits on one,
// else its own output.
fn waited_lines(call: &Call, lines: usize, chars: usize) -> Vec<String> {
  if let Some(work) = &call.work {
    return tail_lines(Path::new(&work.path), lines, chars);
  }

  call
    .last_written
    .as_ref()
    .and_then(|path| call.outputs.get(path))
    .or_else(|| call.outputs.values().find(|(size, _)| *size > 0))
    .map(|(_, through)| tail_lines(Path::new(through), lines, chars))
    .unwrap_or_default()
}

// The file of the job's own work that still moves a shell call waits on, and the process writing it, where it
// waits on one: its command names, by its whole path, a regular file that a living process of the program's
// tree, not one of the call's own, holds open for writing — a check or a close an earlier call put in the
// background, writing the log the call polls.
fn awaited_work(procs: &[Proc], program: i32, owned: &[Proc], used: &ToolUse) -> Option<(String, i32)> {
  let command = used.input.get("command").and_then(Value::as_str)?;
  let own: HashSet<i32> = owned.iter().map(|proc| proc.pid).collect();

  descendants(procs, program)
    .iter()
    .filter(|proc| !own.contains(&proc.pid))
    .flat_map(|proc| written_files(proc.pid).into_iter().map(|path| (path, proc.pid)))
    .find(|(path, _)| command.contains(path.as_str()))
}

// The regular files a process holds open for writing, by the path the kernel names: a descriptor's link
// carries the mode it was opened with.
fn written_files(pid: i32) -> Vec<String> {
  let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/fd")) else { return vec![] };

  entries
    .flatten()
    .filter(|entry| {
      std::fs::symlink_metadata(entry.path()).is_ok_and(|link| link.permissions().mode() & 0o200 != 0)
        && std::fs::metadata(entry.path()).is_ok_and(|meta| meta.is_file())
    })
    .filter_map(|entry| std::fs::read_link(entry.path()).ok())
    .map(|path| path.to_string_lossy().into_owned())
    .collect()
}

// The share of one processor a call's processes burned across the silent window, in percent, where the
// call is past its minimum, has written nothing and waited on no moving work of the job's own for the
// window, and burned under the idle share; none otherwise, and none until the reads cover the whole
// window.
fn idle_percent(call: &Call, now: i64, clock: &ToolClock) -> Option<f64> {
  let window = clock.silent.window_ms;

  if now - call.started_at < clock.min_ms || now - call.output_changed_at.max(call.awaited_at) < window {
    return None;
  }

  let (from_at, from) = *call.burned_at.front()?;
  let span = now - from_at;

  if span < window || span <= 0 {
    return None;
  }

  let burned = call.burned.saturating_sub(from);
  let ticks = u64::try_from(clock_ticks().max(1)).unwrap_or(1);
  let burned_ms = i64::try_from(burned.saturating_mul(1_000) / ticks).unwrap_or(i64::MAX);
  let limit = i64::try_from(clock.silent.idle_cpu_percent).unwrap_or(i64::MAX);

  (burned_ms.saturating_mul(100) < limit.saturating_mul(span))
    .then(|| burned_ms as f64 * 100.0 / span as f64)
}

// The last line a call's output holds, read off the end of the file it last wrote to, or the first it
// writes to; none where it has written nothing.
fn last_line(call: &Call, chars: usize) -> Option<String> {
  let (_, through) = call
    .last_written
    .as_ref()
    .and_then(|path| call.outputs.get(path))
    .or_else(|| call.outputs.values().find(|(size, _)| *size > 0))?;

  tail_lines(Path::new(through), 1, chars).pop()
}

// The last `count` lines that hold anything at the end of a file, oldest first, each with its control characters
// taken out and cut at `chars` characters; none where it cannot be read.
fn tail_lines(path: &Path, count: usize, chars: usize) -> Vec<String> {
  let Ok(mut file) = std::fs::File::open(path) else { return vec![] };
  let Ok(meta) = file.metadata() else { return vec![] };
  let mut tail = vec![];

  if file.seek(SeekFrom::Start(meta.len().saturating_sub(TAIL_BYTES))).is_err()
    || file.read_to_end(&mut tail).is_err()
  {
    return vec![];
  }

  let text = String::from_utf8_lossy(&tail);
  let mut lines: Vec<String> = text
    .lines()
    .rev()
    .map(|line| line.trim().chars().filter(|one| !one.is_control()).take(chars).collect::<String>())
    .filter(|line| !line.is_empty())
    .take(count)
    .collect();

  lines.reverse();
  lines
}

// A call's processes stopped: a polite end first, then the kill for what a blocked read will not take.
async fn stop(pids: &[i32]) {
  for pid in pids {
    signal(*pid, libc::SIGTERM);
  }

  tokio::time::sleep(TERM_GRACE).await;

  for pid in pids {
    signal(*pid, libc::SIGKILL);
  }
}

fn signal(pid: i32, signal: i32) {
  if pid > 1 {
    // SAFETY: a signal sent to a pid the call's own tree named a moment ago.
    unsafe {
      libc::kill(pid, signal);
    }
  }
}

/// The tool calls the record holds after `from` that no result answers yet, in order.
pub fn tool_uses(records: &[Value], from: usize) -> Vec<ToolUse> {
  uses_in(records, from, false)
}

/// The tool calls a subagent's own record holds that no result answers yet, in order: every line of that
/// record is the subagent's, written as a side chain of its program's conversation.
pub fn subagent_uses(records: &[Value]) -> Vec<ToolUse> {
  uses_in(records, 0, true)
}

fn uses_in(records: &[Value], from: usize, side_chain: bool) -> Vec<ToolUse> {
  let answered: HashSet<String> = tool_results(records, from).into_keys().collect();
  let mut uses = vec![];

  for record in records.get(from..).unwrap_or_default() {
    if record.get("type").and_then(Value::as_str) != Some("assistant")
      || (!side_chain && record.get("isSidechain").and_then(Value::as_bool) == Some(true))
    {
      continue;
    }

    let started_at = record
      .get("timestamp")
      .and_then(Value::as_str)
      .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
      .map(|stamp| stamp.timestamp_millis());

    for block in blocks(record) {
      if block.get("type").and_then(Value::as_str) != Some("tool_use") {
        continue;
      }
      let Some(id) = block.get("id").and_then(Value::as_str) else { continue };

      if answered.contains(id) {
        continue;
      }

      uses.push(ToolUse {
        id: id.to_owned(),
        name: block.get("name").and_then(Value::as_str).unwrap_or("a tool").to_owned(),
        input: block.get("input").cloned().unwrap_or(Value::Null),
        started_at,
      });
    }
  }

  uses
}

// Every answer the record holds after `from`, by the call it answers, and whether it was an error.
fn tool_results(records: &[Value], from: usize) -> HashMap<String, bool> {
  let mut results = HashMap::new();

  for record in records.get(from..).unwrap_or_default() {
    for block in blocks(record) {
      if block.get("type").and_then(Value::as_str) != Some("tool_result") {
        continue;
      }
      let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else { continue };

      results.insert(id.to_owned(), block.get("is_error").and_then(Value::as_bool).unwrap_or(false));
    }
  }

  results
}

fn blocks(record: &Value) -> &[Value] {
  record.pointer("/message/content").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// How a call of a program's background work is found in the program's process tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owns {
  /// A background shell, by the file the program writes the task's output to, named after the task: every
  /// process of the tree standing on that file, and every process each of them began.
  Output(String),
  /// A call still in flight, by its command and when it began, as a turn's call is found.
  Command { command: String, started_at: i64 },
}

/// One call of a program's background work handed to the background clock: an id of the caller's own, the
/// command it runs, how its processes are found, and when it began.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundCall {
  pub id: String,
  pub command: String,
  pub owns: Owns,
  pub started_at: i64,
}

/// A call of a program's background work the clock cut: its id, its command, how long it had run, and
/// whether it was cut for its stall rather than for writing nothing while it stood idle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundCut {
  pub id: String,
  pub command: String,
  pub minutes: i64,
  pub stalled: bool,
}

/// The call clock over a program's background work (docs/agent-control.md, 「Background work on a line that
/// carries it apart」): each call read as a turn's call is, and cut, its processes signalled, where it is past
/// the clock's minimum and for the stall its processes burned no processor time, began no process and wrote
/// no output, or where it wrote no output and stood idle across the silent window. Nobody is asked first. A
/// call with no process of its own leaves nothing to read or cut.
#[derive(Debug)]
pub struct BackgroundClock {
  clock: ToolClock,
  calls: BTreeMap<String, Call>,
  last_look: i64,
}

impl BackgroundClock {
  pub fn new(clock: ToolClock) -> Self {
    Self {
      clock,
      calls: BTreeMap::new(),
      last_look: i64::MIN,
    }
  }

  /// The calls handed over read against the program's tree, at most once a look; answers those it cut. A call
  /// no longer handed over is forgotten.
  pub async fn look(&mut self, program: i32, calls: &[BackgroundCall], now: i64) -> Vec<BackgroundCut> {
    self.calls.retain(|id, _| calls.iter().any(|call| call.id == *id));

    for call in calls {
      self.calls.entry(call.id.clone()).or_insert_with(|| Call {
        name: "Bash".to_owned(),
        summary: call.command.clone(),
        started_at: call.started_at,
        output_changed_at: call.started_at,
        last_progress_at: now,
        ..Call::default()
      });
    }

    if now.saturating_sub(self.last_look) < self.clock.look_ms {
      return vec![];
    }

    self.last_look = now;

    let procs = scan();
    let mut cuts = vec![];

    for handed in calls {
      let Some(call) = self.calls.get_mut(&handed.id) else { continue };

      if call.cut {
        continue;
      }

      let owned = match &handed.owns {
        Owns::Output(task) => on_output(&procs, program, task),
        Owns::Command {
          command,
          started_at,
        } => owned_by(&procs, program, *started_at, "Bash", &serde_json::json!({ "command": command })),
      };

      if owned.is_empty() {
        continue;
      }

      for proc in &owned {
        if call.cpu.get(&proc.pid).is_none_or(|cpu| proc.cpu > *cpu) {
          call.last_progress_at = now;
        }

        call.cpu.insert(proc.pid, proc.cpu);
      }

      call.pids = owned.iter().map(|proc| proc.pid).collect();
      read_activity(call, &owned, now, self.clock.silent.window_ms);

      let elapsed = now - call.started_at;
      let moved_at = call.last_progress_at.max(call.output_changed_at);
      let stalled = elapsed >= self.clock.min_ms && now - moved_at >= self.clock.stall_ms;
      let idle = idle_percent(call, now, &self.clock).is_some();

      if stalled || idle {
        call.cut = true;
        stop(&call.pids).await;
        cuts.push(BackgroundCut {
          id: handed.id.clone(),
          command: handed.command.clone(),
          minutes: elapsed.max(0) / 60_000,
          stalled,
        });
      }
    }

    cuts
  }
}

// The processes of the program's tree standing on a background task's output file, and every process they
// began.
fn on_output(procs: &[Proc], program: i32, task: &str) -> Vec<Proc> {
  let file = format!("/tasks/{task}.output");
  let roots: HashSet<i32> = descendants(procs, program)
    .iter()
    .filter(|proc| {
      [1, 2].iter().any(|fd| {
        std::fs::read_link(format!("/proc/{}/fd/{fd}", proc.pid))
          .is_ok_and(|path| path.to_string_lossy().ends_with(&file))
      })
    })
    .map(|proc| proc.pid)
    .collect();

  if roots.is_empty() {
    return vec![];
  }

  let under = descendants_of(procs, &roots);

  procs.iter().filter(|proc| under.contains(&proc.pid)).cloned().collect()
}

/// The processes of a program's tree that the named background shells stand on, and every process they began.
pub fn task_processes(program: i32, tasks: &[String]) -> Vec<i32> {
  let procs = scan();
  let mut pids: Vec<i32> = tasks
    .iter()
    .flat_map(|task| on_output(&procs, program, task))
    .map(|proc| proc.pid)
    .collect();

  pids.sort_unstable();
  pids.dedup();
  pids
}

/// Every process of a program's tree.
pub fn tree_processes(program: i32) -> Vec<i32> {
  descendants(&scan(), program).iter().map(|proc| proc.pid).collect()
}

/// Processes ended: a polite end first, then, past the grace, a kill of what still stands. Answers those still
/// standing after the kill.
pub async fn end_processes(pids: &[i32], grace: Duration) -> Vec<i32> {
  let standing = |pids: &[i32]| -> Vec<i32> {
    pids
      .iter()
      .copied()
      .filter(|pid| process_stat(*pid).is_some_and(|stat| !stat.state.starts_with('Z')))
      .collect()
  };

  for pid in pids {
    signal(*pid, libc::SIGTERM);
  }

  let by = tokio::time::Instant::now() + grace;

  while tokio::time::Instant::now() < by && !standing(pids).is_empty() {
    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  for pid in standing(pids) {
    signal(pid, libc::SIGKILL);
  }

  tokio::time::sleep(Duration::from_millis(50)).await;
  standing(pids)
}
