//! Where an item's closes stand (docs/role-agents.md, 「When an agent is busy」): a pair of labels for each
//! repository, the summing pair read alone only on an item that carries none.

use agency_server::watches::closes::{Close, of, waiting_in};
use serde_json::json;

fn close(repository: Option<&str>, phase: &str, commit: Option<&str>) -> Close {
  Close {
    repository: repository.map(str::to_owned),
    phase: phase.to_owned(),
    commit: commit.map(str::to_owned),
  }
}

#[test]
fn each_repository_s_pair_is_read_and_the_summing_pair_only_on_an_item_that_carries_none() {
  let spanning = of(&json!({
    "close-phase": "waiting-batch", "close-commit": "aaa",
    "close-phase:web": "waiting-batch", "close-commit:web": "aaa",
    "close-phase:drive": "testing",
    "close-phase:": "waiting-batch",
    "series": "s",
  }));

  assert_eq!(
    spanning,
    [close(Some("drive"), "testing", None), close(Some("web"), "waiting-batch", Some("aaa"))],
    "by repository, a label naming no repository left out"
  );
  assert_eq!(
    spanning.iter().filter_map(Close::waiting).collect::<Vec<_>>(),
    ["aaa"],
    "a testing close waits on nothing"
  );
  assert_eq!(waiting_in(&spanning, "web"), Some("aaa"));
  assert_eq!(waiting_in(&spanning, "drive"), None);
  assert_eq!(
    waiting_in(&spanning, "mail"),
    None,
    "the summing pair stands for no repository beside the pairs"
  );

  let older = of(&json!({ "close-phase": "waiting-live", "close-commit": "bbb" }));

  assert_eq!(older, [close(None, "waiting-live", Some("bbb"))]);
  assert_eq!(waiting_in(&older, "web"), Some("bbb"), "an item's single pair is its own repository's");
  assert!(of(&json!({ "series": "s" })).is_empty());
}
