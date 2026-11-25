//! Where an item's closes stand (docs/role-agents.md, 「When an agent is busy」): a pair of labels for each
//! repository the item's work was closed in, its phase and the commit that close handed over, summed up by the
//! pair with no repository, which is read alone on an item that carries no repository's pair.

use policy::machine_turn::MACHINE_TURN;
use serde_json::Value;

use crate::queue::orderer::JobService;

/// The phase of a close while it checks.
pub const TESTING: &str = "testing";
/// The phase of a close whose commit is merged and waits for its repository's batch deployment.
pub const WAITING_BATCH: &str = "waiting-batch";
/// The phase an older close left, its commit waiting to go live.
pub const WAITING_LIVE: &str = "waiting-live";

/// One close on an item, as its pair of labels names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Close {
  /// The repository the close ran in, by the name its testbed declaration gives; none for the summing pair.
  pub repository: Option<String>,
  pub phase: String,
  pub commit: Option<String>,
}

impl Close {
  pub fn testing(&self) -> bool {
    self.phase == TESTING
  }

  /// The commit this close handed over, where it waits for its deployment.
  pub fn waiting(&self) -> Option<&str> {
    matches!(self.phase.as_str(), WAITING_BATCH | WAITING_LIVE)
      .then_some(self.commit.as_deref())
      .flatten()
  }
}

/// Every close an item's labels name, by repository: each repository's pair, else the summing pair alone.
pub fn of(labels: &Value) -> Vec<Close> {
  let phase_label = MACHINE_TURN.close_label.as_str();
  let commit_label = MACHINE_TURN.commit_label.as_str();
  let text =
    |key: &str| labels[key].as_str().map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned);
  let mut pairs: Vec<Close> = labels
    .as_object()
    .into_iter()
    .flatten()
    .filter_map(|(key, _)| {
      let repository = key
        .strip_prefix(phase_label)?
        .strip_prefix(':')
        .filter(|repository| !repository.is_empty())?;

      Some(Close {
        repository: Some(repository.to_owned()),
        phase: text(key)?,
        commit: text(&format!("{commit_label}:{repository}")),
      })
    })
    .collect();

  if pairs.is_empty() {
    return text(phase_label)
      .map(|phase| Close {
        repository: None,
        phase,
        commit: text(commit_label),
      })
      .into_iter()
      .collect();
  }

  pairs.sort_by(|one, other| one.repository.cmp(&other.repository));
  pairs
}

/// The commit an item's closes leave waiting for `repository`'s deployment: that repository's pair's, or the summing
/// pair's on an item that carries no repository's pair.
pub fn waiting_in<'a>(closes: &'a [Close], repository: &str) -> Option<&'a str> {
  closes
    .iter()
    .find(|close| close.repository.as_deref().is_none_or(|named| named == repository))?
    .waiting()
}

/// The states of an item not ended, the only ones whose closes still stand.
const UNENDED: &str = "draft,open,leased,held";

/// The job service's items under a close that have not ended, the label naming a close's phase carried, every page
/// of them: the items that ended outnumber them by far and would crowd them off a single page.
pub async fn items(service: &JobService) -> Result<Vec<Value>, String> {
  service
    .every_page("/items", &[("label", MACHINE_TURN.close_label.as_str()), ("state", UNENDED)])
    .await
}
