//! `bmo-agency machine memory` (docs/agent-control.md, 「What each program holds」): what a process runs,
//! what it holds read off a proc root of the test's own, and the table a person reads.

use std::fs;
use std::path::Path;

use agency_server::verbs::process_memory::{
  MemoryArgs, ProcessMemory, memory_answer, memory_groups, memory_text, program_of, read_process_memory,
  runtime_of, unit_of,
};

fn write(root: &Path, at: &str, text: &str) {
  let path = root.join(at);

  if let Some(folder) = path.parent() {
    fs::create_dir_all(folder).unwrap();
  }

  fs::write(path, text).unwrap();
}

// A proc root holding the two processes and the machine's own figures.
fn proc_root() -> tempfile::TempDir {
  let root = tempfile::tempdir().unwrap();

  write(
    root.path(),
    "meminfo",
    "MemTotal:       16000000 kB\nMemFree:         4000000 kB\nMemAvailable:    8000000 kB\nBuffers:          100000 kB\nCached:          2000000 kB\nSwapCached:            0 kB\nActive:          3000000 kB\nInactive:        2000000 kB\nSwapTotal:       4000000 kB\nSwapFree:        1000000 kB\nDirty:               100 kB\nWriteback:             0 kB\nMapped:           500000 kB\nSlab:             300000 kB\nCommitted_AS:    9000000 kB\nVmallocTotal:   34359738367 kB\nVmallocUsed:           0 kB\nVmallocChunk:          0 kB\n",
  );
  write(root.path(), "100/cmdline", "bun\0run\0dev\0");
  write(root.path(), "100/comm", "bun\n");
  write(
    root.path(),
    "100/smaps_rollup",
    "1000-2000 r--p 00000000 00:00 0 [rollup]\nRss:            2048 kB\nPss:            1024 kB\nSwapPss:         512 kB\n",
  );
  write(
    root.path(),
    "100/cgroup",
    "0::/user.slice/user-1000.slice/user@1000.service/app.slice/bmo-task-1.scope\n",
  );
  write(root.path(), "101/cmdline", "/usr/bin/node\0/a/b/server.js\0");
  write(root.path(), "101/comm", "node\n");
  write(
    root.path(),
    "101/smaps_rollup",
    "1000-2000 r--p 00000000 00:00 0 [rollup]\nRss:            8192 kB\nPss:            6144 kB\nSwapPss:           0 kB\n",
  );
  write(
    root.path(),
    "101/cgroup",
    "0::/user.slice/user-1000.slice/user@1000.service/app.slice/agency.service\n",
  );

  root
}

#[test]
fn what_a_process_runs_is_its_runtime_and_its_script() {
  assert_eq!(runtime_of(&["bun".into(), "run".into(), "dev".into()], "MainThread"), "bun");
  assert_eq!(runtime_of(&["/usr/bin/node".into(), "server.js".into()], "node"), "node");
  assert_eq!(runtime_of(&[], "MainThread"), "MainThread");
  assert_eq!(runtime_of(&[], ""), "?");

  assert_eq!(program_of("bun", &["bun".into(), "run".into(), "dev".into()]), "run dev");
  assert_eq!(program_of("node", &["node".into(), "/a/b/server.js".into()]), "b/server.js");
  assert_eq!(
    program_of("python3", &["python3".into(), "-m".into(), "http.server".into()]),
    "http.server"
  );
  assert_eq!(program_of("sh", &["sh".into(), "-c".into(), "ls".into()]), "sh");
}

#[test]
fn the_unit_is_the_deepest_service_or_scope_of_the_control_group() {
  assert_eq!(
    unit_of(Some("0::/user.slice/user@1000.service/app.slice/bmo-task-1.scope\n")).as_deref(),
    Some("bmo-task-1.scope")
  );
  assert_eq!(
    unit_of(Some("0::/user.slice/user@1000.service/app.slice/agency.service\n")).as_deref(),
    Some("agency.service")
  );
  assert_eq!(unit_of(Some("")), None);
  assert_eq!(unit_of(None), None);
}

#[test]
fn the_read_holds_each_process_with_its_shares_and_its_unit() {
  let root = proc_root();
  let reading = read_process_memory(root.path());

  assert_eq!(reading.unreadable, 0);
  assert_eq!(reading.processes.len(), 2);

  let bun = &reading.processes[0];
  let node = &reading.processes[1];

  assert_eq!((bun.pid, bun.runtime.as_str(), bun.program.as_str()), (100, "bun", "run dev"));
  assert_eq!((bun.pss_kb, bun.rss_kb, bun.swap_pss_kb), (1024, 2048, 512));
  assert_eq!(bun.held_kb(), 1536);
  assert_eq!(bun.unit.as_deref(), Some("bmo-task-1.scope"));
  assert_eq!((node.pid, node.program.as_str()), (101, "b/server.js"));
  assert_eq!(node.unit.as_deref(), Some("agency.service"));
}

#[test]
fn the_answer_gives_the_machine_the_runtimes_and_the_programs_asked() {
  let root = proc_root();
  let asked = MemoryArgs {
    runtimes: vec!["bun".to_owned(), "node".to_owned()],
    units: vec!["agency".to_owned()],
    json: false,
  };
  let answer = memory_answer(root.path(), &asked, "now".to_owned());

  assert_eq!(answer.machine.mem_total_kb, Some(16_000_000));
  assert_eq!(answer.machine.mem_available_kb, Some(8_000_000));
  assert_eq!(answer.machine.swap_total_kb, Some(4_000_000));
  assert_eq!(answer.machine.counted, 2);
  assert_eq!(answer.machine.unreadable, 0);
  assert_eq!(answer.runtimes.len(), 2);

  let bun = &answer.programs.0[0];
  let node = &answer.programs.0[1];

  assert_eq!(bun.0, "bun");
  assert_eq!((bun.1[0].name.as_str(), bun.1[0].count), ("run dev", 1));
  assert_eq!(node.0, "node");
  assert_eq!(node.1[0].name, "b/server.js");

  let units = answer.units.as_ref().unwrap();
  assert_eq!(units.0[0].0, "agency.service");
  assert_eq!(units.0[0].1[0].name, "b/server.js");
}

#[test]
fn the_table_holds_a_line_a_program_and_a_median() {
  let processes = [
    ProcessMemory {
      pid: 1,
      runtime: "bun".into(),
      program: "run dev".into(),
      unit: None,
      pss_kb: 1024,
      rss_kb: 2048,
      swap_pss_kb: 0,
    },
    ProcessMemory {
      pid: 2,
      runtime: "bun".into(),
      program: "run dev".into(),
      unit: None,
      pss_kb: 3072,
      rss_kb: 4096,
      swap_pss_kb: 1024,
    },
  ];
  let groups = memory_groups(&processes, |one| one.program.clone());

  assert_eq!(groups.len(), 1);
  assert_eq!(groups[0].count, 2);
  assert_eq!(groups[0].held_kb, 5120);
  assert_eq!(groups[0].median_held_kb, 2560);

  let answer = memory_answer(Path::new("/nowhere"), &MemoryArgs::default(), "now".to_owned());
  let lines = memory_text(&answer);

  assert!(lines.iter().any(|line| line == "runtimes"));
  assert!(lines.iter().any(|line| line == "programs · bun"));
}
