//! `bmo-agency machine memory`: how much memory each program on this machine holds
//! (docs/agent-control.md, 「What each program holds」), read out of the kernel's own files for every
//! process the caller may read.
//!
//! What a process holds is its proportional share of each page it maps, plus its proportional share of
//! what has been swapped out: the kernel's `Pss` and `SwapPss` in `/proc/<pid>/smaps_rollup`. A runtime
//! mapped by a hundred processes counts once across them, and a process counts the same however much of
//! it the machine had swapped out when it was read. The resident share and the swapped-out share are
//! shown beside it, and the resident size (`Rss`), which counts shared pages again in every process, only
//! for reference.
//!
//! A program is what a process runs: for an interpreter — bun, node, python — the script it was started
//! on; for anything else the executable's own name.

use std::path::{Path, PathBuf};

use clap::Parser;
use policy::fill_marks;
use policy::verb_words::VERB_WORDS;
use procfs::FromRead as _;
use procfs::process::all_processes_with_root;
use serde::Serialize;
use serde::ser::SerializeMap;

use super::Answer;

/// What `bmo-agency machine memory` is asked.
#[derive(Parser, Debug, Default)]
pub struct MemoryArgs {
  /// A runtime whose programs are shown, by the executable its processes were started as; bun where none
  /// is named, and repeatable.
  #[arg(long = "runtime")]
  pub runtimes: Vec<String>,
  /// A unit whose own processes are shown by their programs, `.service` taken as said or added;
  /// repeatable.
  #[arg(long = "unit")]
  pub units: Vec<String>,
  /// The rows handed back as they stand, as JSON.
  #[arg(long)]
  pub json: bool,
}

/// One process as its memory is counted: what it runs and what it holds, in kilobytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessMemory {
  pub pid: u32,
  /// The executable it was started as.
  pub runtime: String,
  pub program: String,
  /// The user or system unit whose control group it runs in, by its own name.
  pub unit: Option<String>,
  pub pss_kb: i64,
  pub rss_kb: i64,
  pub swap_pss_kb: i64,
}

impl ProcessMemory {
  /// What the process holds: its resident share and its swapped-out share.
  pub fn held_kb(&self) -> i64 {
    self.pss_kb + self.swap_pss_kb
  }
}

/// The processes counted, and how many could not be read — gone, or another user's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessMemoryReading {
  pub processes: Vec<ProcessMemory>,
  pub unreadable: usize,
}

/// Several processes taken together. `held_kb` is the resident and swapped-out shares together.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGroup {
  pub name: String,
  pub count: usize,
  pub held_kb: i64,
  pub pss_kb: i64,
  pub rss_kb: i64,
  pub swap_pss_kb: i64,
  pub median_held_kb: i64,
}

// The runtimes whose processes are told apart by the script they run rather than by their own name: bun,
// node, and python with or without its version.
fn interpreted(runtime: &str) -> bool {
  let version = match runtime.strip_prefix("python") {
    Some(version) => version,
    None => return runtime == "bun" || runtime == "node",
  };
  let major = version.trim_start_matches(|one: char| one.is_ascii_digit());

  match major.strip_prefix('.') {
    Some(minor) => !minor.is_empty() && minor.chars().all(|one| one.is_ascii_digit()),
    None => major.is_empty(),
  }
}

fn last_two_parts(path: &str) -> String {
  let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();

  parts[parts.len().saturating_sub(2)..].join("/")
}

/// What a process runs: for an interpreter, the script it was started on by the last two parts of its
/// path, `run` and the target for a package script, and the module for `python -m`; for anything else, the
/// command's own name.
pub fn program_of(runtime: &str, args: &[String]) -> String {
  if !interpreted(runtime) {
    return runtime.to_owned();
  }

  let rest = args.get(1..).unwrap_or_default();

  for (at, arg) in rest.iter().enumerate() {
    match arg.as_str() {
      "-m" => return rest.get(at + 1).cloned().unwrap_or_else(|| runtime.to_owned()),
      "-c" => return format!("{runtime} -c"),
      flag if flag.starts_with('-') => continue,
      "run" => {
        return match rest[at + 1..].iter().find(|next| !next.starts_with('-')) {
          Some(target) => format!("run {}", last_two_parts(target)),
          None => "run".to_owned(),
        };
      }
      script => return last_two_parts(script),
    }
  }

  runtime.to_owned()
}

/// The runtime a process is: the executable it was started as, by its own file name. The kernel's name for
/// the process is only the fallback, since a runtime may rename its main thread — node's stands as
/// `MainThread`.
pub fn runtime_of(args: &[String], kernel_name: &str) -> String {
  let executable = args
    .first()
    .and_then(|first| first.split_whitespace().next())
    .and_then(|word| word.rsplit('/').next())
    .map(|name| name.strip_suffix(':').unwrap_or(name))
    .unwrap_or_default();

  match (executable, kernel_name) {
    ("", "") => "?".to_owned(),
    ("", kernel) => kernel.to_owned(),
    (executable, _) => executable.to_owned(),
  }
}

/// The unit a process runs in, out of its `/proc/<pid>/cgroup`: the deepest part of its control group's
/// path named as a service or a scope, which is the unit systemd started it under. The kernel's cgroup
/// file is one line per hierarchy, `id::path`, and no library here reads it.
pub fn unit_of(cgroup: Option<&str>) -> Option<String> {
  let path = cgroup?.lines().find_map(|line| line.strip_prefix("0::"))?;

  path
    .split('/')
    .rfind(|part| part.ends_with(".service") || part.ends_with(".scope"))
    .map(str::to_owned)
}

// The first value a `Name: value kB` line of a rollup holds.
fn rollup_kilobytes(rollup: &str, name: &str) -> Option<i64> {
  let line = rollup
    .lines()
    .find(|row| row.strip_prefix(name).is_some_and(|rest| rest.starts_with(':')))?;

  line[name.len() + 1..].split_whitespace().next()?.parse().ok()
}

// What a rollup's maps hold together, in kilobytes: the proportional share, the resident size and the
// swapped-out share, and whether a Pss stood among them at all.
fn from_maps(maps: &procfs::process::MemoryMaps) -> Option<(i64, i64, i64)> {
  let (mut pss, mut rss, mut swap) = (0_i64, 0_i64, 0_i64);
  let mut stood = false;

  for map in maps.iter() {
    let field = |name: &str| map.extension.map.get(name).map(|bytes| (*bytes / 1024) as i64);

    if let Some(value) = field("Pss") {
      pss += value;
      stood = true;
    }

    rss += field("Rss").unwrap_or(0);
    swap += field("SwapPss").unwrap_or(0);
  }

  stood.then_some((pss, rss, swap))
}

// A process's three figures, in kilobytes. A kernel that prints the rollup's fields with no mapping
// header is one procfs refuses, and its `Name: value kB` lines are then read directly: the rollup is a
// kernel pseudo-file of one repeated shape, and no other library reads that shape.
fn rollup(root: &Path, pid: u32, process: &procfs::process::Process) -> Option<(i64, i64, i64)> {
  if let Ok(rollup) = process.smaps_rollup()
    && let Some(figures) = from_maps(&rollup.memory_map_rollup)
  {
    return Some(figures);
  }

  let text = std::fs::read_to_string(root.join(pid.to_string()).join("smaps_rollup")).ok()?;
  let pss = rollup_kilobytes(&text, "Pss")?;

  Some((
    pss,
    rollup_kilobytes(&text, "Rss").unwrap_or(0),
    rollup_kilobytes(&text, "SwapPss").unwrap_or(0),
  ))
}

/// Every process whose memory the caller may read, under the proc root `root`. A kernel thread has no
/// mappings and no arguments and is left out without being counted as unreadable; a process gone between
/// the listing and its reading, or one the caller may not read, is counted as unreadable.
pub fn read_process_memory(root: &Path) -> ProcessMemoryReading {
  let Ok(processes) = all_processes_with_root(root) else {
    return ProcessMemoryReading::default();
  };
  let mut reading = ProcessMemoryReading::default();

  for process in processes {
    let Ok(process) = process else {
      reading.unreadable += 1;
      continue;
    };
    let Ok(pid) = u32::try_from(process.pid) else { continue };

    let args = match process.cmdline() {
      Ok(args) => args,
      Err(_) => {
        reading.unreadable += 1;
        continue;
      }
    };

    if args.is_empty() {
      continue;
    }

    let Some((pss_kb, rss_kb, swap_pss_kb)) = rollup(root, pid, &process) else {
      reading.unreadable += 1;
      continue;
    };
    let kernel_name = process.stat().map(|stat| stat.comm).unwrap_or_default();
    let runtime = runtime_of(&args, kernel_name.trim());
    let cgroup = std::fs::read_to_string(root.join(pid.to_string()).join("cgroup")).ok();

    reading.processes.push(ProcessMemory {
      pid,
      program: program_of(&runtime, &args),
      runtime,
      unit: unit_of(cgroup.as_deref()),
      pss_kb,
      rss_kb,
      swap_pss_kb,
    });
  }

  reading.processes.sort_by_key(|process| process.pid);
  reading
}

// The middle value, or the two middle ones' mean rounded half up.
fn median(values: &[i64]) -> i64 {
  let mut sorted = values.to_vec();
  let middle = sorted.len() / 2;

  sorted.sort_unstable();

  match sorted.len() {
    0 => 0,
    count if count % 2 == 1 => sorted[middle],
    _ => (sorted[middle - 1] + sorted[middle] + 1).div_euclid(2),
  }
}

/// The processes taken together by the name `by` gives each, the largest holding first and equal holdings
/// by name.
pub fn memory_groups<'a>(
  processes: impl IntoIterator<Item = &'a ProcessMemory>,
  by: impl Fn(&ProcessMemory) -> String,
) -> Vec<MemoryGroup> {
  let mut grouped: Vec<(String, Vec<&ProcessMemory>)> = Vec::new();

  for process in processes {
    let name = by(process);

    match grouped.iter_mut().find(|(named, _)| *named == name) {
      Some((_, members)) => members.push(process),
      None => grouped.push((name, vec![process])),
    }
  }

  let mut groups: Vec<MemoryGroup> = grouped
    .into_iter()
    .map(|(name, members)| MemoryGroup {
      name,
      count: members.len(),
      held_kb: members.iter().map(|one| one.held_kb()).sum(),
      pss_kb: members.iter().map(|one| one.pss_kb).sum(),
      rss_kb: members.iter().map(|one| one.rss_kb).sum(),
      swap_pss_kb: members.iter().map(|one| one.swap_pss_kb).sum(),
      median_held_kb: median(&members.iter().map(|one| one.held_kb()).collect::<Vec<_>>()),
    })
    .collect();

  groups.sort_by(|a, b| b.held_kb.cmp(&a.held_kb).then_with(|| a.name.cmp(&b.name)));
  groups
}

/// The machine as the reading found it, its figures in kilobytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineMemory {
  pub at: String,
  pub mem_total_kb: Option<i64>,
  pub mem_available_kb: Option<i64>,
  pub swap_total_kb: Option<i64>,
  pub swap_free_kb: Option<i64>,
  pub counted: usize,
  pub unreadable: usize,
}

// Groups by the name each was asked by, in the order asked, written out as one object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named(pub Vec<(String, Vec<MemoryGroup>)>);

impl Serialize for Named {
  fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(self.0.len()))?;

    for (name, groups) in &self.0 {
      map.serialize_entry(name, groups)?;
    }

    map.end()
  }
}

/// What the verb answers: the machine, the total for each runtime, the programs of each runtime asked and
/// of each unit asked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MemoryAnswer {
  pub machine: MachineMemory,
  pub runtimes: Vec<MemoryGroup>,
  pub programs: Named,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub units: Option<Named>,
}

// Each name once, in the order first asked.
fn once(names: impl IntoIterator<Item = String>) -> Vec<String> {
  let mut kept: Vec<String> = Vec::new();

  for name in names {
    if !kept.contains(&name) {
      kept.push(name);
    }
  }

  kept
}

/// The machine under the proc root `root` read as `asked` names, at the moment `at`.
pub fn memory_answer(root: &Path, asked: &MemoryArgs, at: String) -> MemoryAnswer {
  let reading = read_process_memory(root);
  let meminfo = procfs::Meminfo::from_file(root.join("meminfo")).ok();
  let runtimes =
    once(if asked.runtimes.is_empty() { vec!["bun".to_owned()] } else { asked.runtimes.clone() });
  // A unit named as said or with its `.service` added, its processes each by its own program.
  let units = once(asked.units.iter().map(|unit| {
    if unit.ends_with(".service") || unit.ends_with(".scope") {
      unit.clone()
    } else {
      format!("{unit}.service")
    }
  }));
  let programs_where = |keep: &dyn Fn(&ProcessMemory) -> bool| {
    memory_groups(reading.processes.iter().filter(|one| keep(one)), |one| one.program.clone())
  };

  // The meminfo reader counts bytes; the answer speaks kilobytes.
  let kilobytes = |bytes: u64| i64::try_from(bytes / 1024).unwrap_or(i64::MAX);

  MemoryAnswer {
    machine: MachineMemory {
      at,
      mem_total_kb: meminfo.as_ref().map(|memory| kilobytes(memory.mem_total)),
      mem_available_kb: meminfo.as_ref().and_then(|memory| memory.mem_available).map(kilobytes),
      swap_total_kb: meminfo.as_ref().map(|memory| kilobytes(memory.swap_total)),
      swap_free_kb: meminfo.as_ref().map(|memory| kilobytes(memory.swap_free)),
      counted: reading.processes.len(),
      unreadable: reading.unreadable,
    },
    runtimes: memory_groups(&reading.processes, |one| one.runtime.clone()),
    programs: Named(
      runtimes
        .iter()
        .map(|runtime| (runtime.clone(), programs_where(&|one| one.runtime == *runtime)))
        .collect(),
    ),
    units: (!units.is_empty()).then(|| {
      Named(
        units
          .iter()
          .map(|unit| (unit.clone(), programs_where(&|one| one.unit.as_deref() == Some(unit.as_str()))))
          .collect(),
      )
    }),
  }
}

// Kilobytes in megabytes to one decimal, a tie rounded up; a figure the machine did not give as a dash.
fn mb(kilobytes: Option<i64>) -> String {
  kilobytes
    .map_or_else(|| "-".to_owned(), |kb| format!("{:.1}", (kb as f64 * 10.0 / 1024.0).round() / 10.0))
}

fn table(groups: &[MemoryGroup]) -> Vec<String> {
  let words = &VERB_WORDS.machine_memory;
  let mut lines = vec![words.head.clone()];

  lines.extend(groups.iter().map(|group| {
    format!(
      "  {:<30}{:>5}{:>12}{:>12}{:>11}{:>11}{:>12}",
      group.name,
      group.count,
      mb(Some(group.held_kb)),
      mb(Some(group.pss_kb)),
      mb(Some(group.swap_pss_kb)),
      mb(Some(group.rss_kb)),
      mb(Some(group.median_held_kb)),
    )
  }));

  lines
}

/// The answer as a person reads it: the machine, then a table under a title for the runtimes, each runtime
/// asked and each unit asked.
pub fn memory_text(answer: &MemoryAnswer) -> Vec<String> {
  let words = &VERB_WORDS.machine_memory;
  let machine = &answer.machine;
  let mut lines = vec![
    fill_marks(
      &words.machine,
      &[
        ("at", &machine.at),
        ("total", &mb(machine.mem_total_kb)),
        ("available", &mb(machine.mem_available_kb)),
        ("swap_total", &mb(machine.swap_total_kb)),
        ("swap_free", &mb(machine.swap_free_kb)),
      ],
    ),
    fill_marks(
      &words.counted,
      &[
        ("counted", &machine.counted.to_string()),
        ("unreadable", &machine.unreadable.to_string()),
      ],
    ),
    String::new(),
    words.runtimes.clone(),
  ];

  lines.extend(table(&answer.runtimes));

  for (name, groups) in answer.programs.0.iter().chain(answer.units.iter().flat_map(|units| &units.0)) {
    lines.push(String::new());
    lines.push(fill_marks(&words.programs, &[("name", name)]));

    if groups.is_empty() {
      lines.push(words.none.clone());
    } else {
      lines.extend(table(groups));
    }
  }

  lines
}

/// The proc root the machine is read from; `BMO_PROC_ROOT` names another, as it does for the machine's
/// other readings.
pub fn proc_root() -> PathBuf {
  std::env::var_os("BMO_PROC_ROOT").map_or_else(|| PathBuf::from("/proc"), PathBuf::from)
}

/// `bmo-agency machine memory` over this machine's proc root, as text or with `--json` as JSON.
pub fn machine_memory(asked: &MemoryArgs) -> Answer {
  let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
  let answer = memory_answer(&proc_root(), asked, at);
  let lines = if asked.json {
    vec![serde_json::to_string_pretty(&answer).unwrap_or_default()]
  } else {
    memory_text(&answer)
  };

  Answer { lines, code: 0 }
}
