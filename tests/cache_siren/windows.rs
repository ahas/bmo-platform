//! The keeping: a reading replaces its slot's whole set and no other slot's, a slot is read back with the
//! freshness its provider's reader declares, and a window stands while its reading is young and later than
//! the account's unread mark, which keeps the moment it was first set until a reading lifts it.

use std::sync::Arc;
use std::time::Duration;

use cache_siren::reader::Passing;
use cache_siren::{Keeping, Reader, Slot, Window};

use crate::support::scratch::Scratch;

const NOW: i64 = 1_800_000_000;

struct Declares;

impl Reader for Declares {
  fn mode(&self) -> &str {
    "claude-code"
  }

  fn cadence(&self) -> Duration {
    Duration::from_secs(300)
  }

  fn fresh(&self) -> Duration {
    Duration::from_secs(1800)
  }

  fn pass<'a>(&'a self, _keeping: &'a Keeping, _now_ms: i64) -> Passing<'a> {
    Box::pin(async {})
  }
}

fn keeping(scratch: &Scratch) -> Keeping {
  Keeping::new(scratch.db().clone(), vec![Arc::new(Declares)])
}

fn window(name: &str, utilization: f64) -> Window {
  Window {
    name: name.into(),
    utilization,
    resets_at: Some(NOW + 3600),
    duration_minutes: None,
    limit_name: None,
  }
}

const MAIN: Slot<'static> = Slot {
  mode: "claude-code",
  account: "main",
};

#[tokio::test]
async fn a_reading_replaces_its_slots_whole_set_and_leaves_every_other_slot_as_it_was() {
  let scratch = Scratch::new().await;
  let keeping = keeping(&scratch);
  let spare = Slot {
    mode: "claude-code",
    account: "spare",
  };

  keeping
    .replace(MAIN, &[window("five_hour", 0.5), window("seven_day", 0.2)], NOW - 60)
    .await
    .unwrap();
  keeping.replace(spare, &[window("five_hour", 0.9)], NOW - 60).await.unwrap();
  keeping.replace(MAIN, &[window("seven_day", 0.3)], NOW).await.unwrap();

  let main = keeping.read(MAIN).await.unwrap();

  assert_eq!(main.windows.len(), 1, "a window the new reading did not state is gone");
  assert_eq!(main.windows[0].window, window("seven_day", 0.3));
  assert_eq!(main.windows[0].seen_at, NOW);
  assert_eq!(keeping.read(spare).await.unwrap().windows[0].window, window("five_hour", 0.9));
}

#[tokio::test]
async fn a_slot_is_read_with_its_readers_freshness_and_none_where_no_reader_reads_its_provider() {
  let scratch = Scratch::new().await;
  let keeping = keeping(&scratch);
  let codex = Slot {
    mode: "codex",
    account: "",
  };

  keeping.replace(codex, &[window("codex:default:primary", 0.4)], NOW).await.unwrap();

  assert_eq!(keeping.read(MAIN).await.unwrap().fresh, Duration::from_secs(1800));

  let read = keeping.read(codex).await.unwrap();

  assert_eq!(read.fresh, Duration::ZERO);
  assert_eq!(
    read.standing(NOW + 1).count(),
    0,
    "a reading no reader declares a span for stands for none"
  );
}

#[tokio::test]
async fn a_window_stands_while_its_reading_is_young_and_later_than_the_accounts_unread_mark() {
  let scratch = Scratch::new().await;
  let keeping = keeping(&scratch);

  keeping.replace(MAIN, &[window("five_hour", 1.0)], NOW - 1800).await.unwrap();

  let read = keeping.read(MAIN).await.unwrap();

  assert_eq!(read.standing(NOW).count(), 1, "a reading as old as its span stands");
  assert_eq!(read.standing(NOW + 1).count(), 0, "one older than its span does not");

  keeping.mark_unread("main", NOW - 1800).await.unwrap();

  let marked = keeping.read(MAIN).await.unwrap();

  assert_eq!(marked.unread_at, Some(NOW - 1800));
  assert_eq!(marked.standing(NOW).count(), 0, "a reading seen at the mark is stale");

  keeping.replace(MAIN, &[window("five_hour", 1.0)], NOW - 10).await.unwrap();

  assert_eq!(keeping.read(MAIN).await.unwrap().standing(NOW).count(), 1, "one seen after it stands");
}

#[tokio::test]
async fn the_unread_mark_keeps_the_moment_it_was_first_set_until_it_is_lifted() {
  let scratch = Scratch::new().await;
  let keeping = keeping(&scratch);

  keeping.mark_unread("main", NOW - 600).await.unwrap();
  keeping.mark_unread("main", NOW).await.unwrap();

  assert_eq!(keeping.unread_at("main").await.unwrap(), Some(NOW - 600));
  assert_eq!(keeping.unread_at("spare").await.unwrap(), None, "a mark is its own account's");
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "settings" WHERE "key" = $1"#,
        vec!["usage_windows_unread:main".into()]
      )
      .await,
    1,
    "the mark stands in the settings family docs/store.md names"
  );

  keeping.lift_unread("main").await.unwrap();

  assert_eq!(keeping.unread_at("main").await.unwrap(), None);
}
