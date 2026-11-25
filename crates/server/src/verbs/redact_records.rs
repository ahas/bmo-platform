//! `bmo-agency redact-records`: the records every place under the data root wrote before read through the
//! readings a record is written through now, and rewritten where a line changed (docs/agent-control.md, 「A
//! record holds no secret」). It says how many records and lines it changed and how many runs each source took
//! out, and nothing of what it took out.

use std::path::Path;

use super::{Answer, VerbError};
use crate::door::place_run::{record_files, redact_record};
use crate::door::record_secrets::{Readings, Tally};
use crate::lines::place::places_dir;

/// Every place's records read through `readings`, each record rewritten where a line changed.
pub fn redact_records(data_root: &Path, readings: &Readings) -> Result<Answer, VerbError> {
  let dir = places_dir(data_root);
  let mut places: Vec<_> = std::fs::read_dir(&dir)
    .map_err(|why| VerbError::Refused(format!("{}: {why}", dir.display())))?
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|path| path.is_dir())
    .collect();
  let mut read = 0;
  let mut rewritten = 0;
  let mut changed = 0;
  let mut faults = Vec::new();
  let mut tally = Tally::default();

  places.sort();

  for record in places.iter().flat_map(|place| record_files(place)) {
    if !record.is_file() {
      continue;
    }

    read += 1;

    match redact_record(&record, readings, &mut tally) {
      Ok(0) => {}
      Ok(lines) => {
        rewritten += 1;
        changed += lines;
      }
      Err(why) => faults.push(format!("{}: {why}", record.display())),
    }
  }

  let mut lines = vec![format!(
    "{read} record(s) of {} place(s) read; {rewritten} rewritten, {changed} line(s) changed",
    places.len()
  )];

  lines.extend(tally.lines().into_iter().map(|line| format!("  {line}")));

  if faults.is_empty() {
    Ok(Answer::from(lines))
  } else {
    Err(VerbError::Partly {
      lines,
      refusal: faults.join("\n"),
    })
  }
}
