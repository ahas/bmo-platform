//! The machine's processes by their process groups, as the dev-server listing reads a checkout's dev
//! server and the memory it holds (docs/worker-bench.md, 「Dev server」).

/// One process as the listing reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessRow {
  pub pid: i32,
  pub group: i32,
  pub age_seconds: i64,
  /// Resident memory and swap together.
  pub kilobytes: i64,
}

// What a process holds in memory and swap: a server long idle has most of what it took swapped out,
// which its resident size alone would hide.
fn held_kilobytes(pid: i32) -> i64 {
  procfs::process::Process::new(pid)
    .and_then(|process| process.status())
    .map_or(0, |status| (status.vmrss.unwrap_or(0) + status.vmswap.unwrap_or(0)) as i64)
}

/// Every process of the machine, as `ps` numbers them.
pub async fn processes() -> Vec<ProcessRow> {
  let listed = tokio::process::Command::new("ps")
    .args(["-e", "-o", "pid=,pgid=,etimes="])
    .stdin(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .output()
    .await
    .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    .unwrap_or_default();

  listed
    .lines()
    .filter_map(|line| {
      let mut numbers = line.split_whitespace().map(str::parse::<i64>);
      let (Some(Ok(pid)), Some(Ok(group)), Some(Ok(age))) =
        (numbers.next(), numbers.next(), numbers.next())
      else {
        return None;
      };
      let (pid, group) = (i32::try_from(pid).ok()?, i32::try_from(group).ok()?);

      Some(ProcessRow {
        pid,
        group,
        age_seconds: age,
        kilobytes: held_kilobytes(pid),
      })
    })
    .collect()
}
