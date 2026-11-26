//! `bmo-agency node person`: the node each account's runs stand on, listed, set or taken away
//! (docs/person-nodes.md, 「Which node is a person's」), as the binding service sets it through the boundary.

use std::path::Path;

use super::{Answer, VerbError, service};
use crate::person_nodes;

/// Every account's node with no account named; the node named set for the account, shared with other accounts where
/// `shared` says so and marked its own machine where `own` does, or its node taken away.
pub async fn person_node(
  data_root: &Path,
  account: Option<String>,
  node: Option<String>,
  shared: bool,
  own: bool,
  clear: bool,
) -> Result<Answer, VerbError> {
  let service = service(data_root).await?;
  let db = &service.db;
  let refused = |refusal: crate::Refusal| VerbError::Refused(refusal.detail);

  match (account, node, clear) {
    (None, None, false) => {
      let every = person_nodes::every(db).await.map_err(refused)?;

      if every.is_empty() {
        return Ok(Answer::from(vec!["no account has a node".to_owned()]));
      }

      Ok(Answer::from(
        every
          .iter()
          .map(|one| {
            let shared = if one.shared { " (shared)" } else { "" };
            let own = if one.own { " (own)" } else { "" };

            format!("{:<12} {}{own}{shared}", one.owner_account, one.node)
          })
          .collect::<Vec<_>>(),
      ))
    }
    (Some(account), None, true) if !shared => {
      if !person_nodes::clear(db, &account).await.map_err(refused)? {
        return Err(VerbError::Refused(format!("account {account} has no node")));
      }

      Ok(Answer::from(vec![format!("account {account}: its runs stand on this machine again")]))
    }
    (Some(account), Some(node), false) => {
      let unanswered = |fault: crate::peers::PeerFault| VerbError::Refused(fault.to_string());

      let nodes = super::node_service(data_root, &service);

      if let Some(why) = person_nodes::refusal(nodes.as_ref(), &node).await.map_err(unanswered)? {
        return Err(VerbError::Refused(why));
      }

      if let Err(taken) = person_nodes::set(db, &account, &node, shared, own).await.map_err(refused)? {
        return Err(VerbError::Refused(taken.to_string()));
      }

      let alongside = match (own, shared) {
        (true, _) => ", its own machine",
        (false, true) => ", shared with other accounts",
        (false, false) => "",
      };

      Ok(Answer::from(vec![format!("account {account}: its runs stand on {node}{alongside}")]))
    }
    _ => Err(VerbError::Refused(
      "name an account and a node to set, with --own for the account's own machine or --shared to share the node \
       with other accounts, an account with --clear to take its nodes away, or nothing to list"
        .into(),
    )),
  }
}
