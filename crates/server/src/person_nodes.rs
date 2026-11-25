//! An account's node (docs/person-nodes.md, 「Which node is a person's」): the record agency keeps of the node an
//! account's runs stand on, set by the service that binds the account to its machine, else the account's isolation
//! node on this machine (docs/account-isolation.md); the supervisors and role runs placed there by it; the wait of a
//! run of it while the node's link is not heard, placed again by the present record on every ask, told, stopped and
//! bounded; several accounts
//! sharing one node, each under a root of its own (「Several accounts on one node」); and who reaches a node (「Who
//! reaches a node」).

use std::path::Path;
use std::time::Duration;

use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde::Serialize;
use store::{all, one, run};

use crate::Refusal;
use crate::peers::PeerFault;
use crate::peers::node::{NodeKind, NodeService};

/// One account's node as its record holds it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonNode {
  pub owner_account: String,
  pub node: String,
  /// Whether the administrator declared the node shared for the account, which lets other accounts' shared records
  /// name it too and gives the account a root of its own there.
  pub shared: bool,
  /// When it was set, in epoch milliseconds.
  pub set_at: i64,
}

fn of_row(row: &sea_orm::QueryResult) -> Result<PersonNode, sea_orm::DbErr> {
  Ok(PersonNode {
    owner_account: row.try_get("", "owner_account")?,
    node: row.try_get("", "node")?,
    shared: row.try_get("", "shared")?,
    set_at: row.try_get("", "set_at")?,
  })
}

/// The folder under a node's workplace root the roots of the accounts sharing the node stand in.
const ACCOUNTS_DIR: &str = "accounts";

/// The workplace root of an account sharing a node, as the node reads it, under the node's own root.
pub fn account_root(node_root: &str, account: &str) -> String {
  format!("{node_root}/{ACCOUNTS_DIR}/{account}")
}

/// Every account's node, by account.
pub async fn every(db: &DatabaseConnection) -> Result<Vec<PersonNode>, Refusal> {
  let rows = all(db, r#"SELECT * FROM "person_nodes" ORDER BY "owner_account""#, vec![]).await?;

  Ok(rows.iter().map(of_row).collect::<Result<_, _>>()?)
}

/// The node an account's runs stand on: the one its record names, else its isolation node; none for the
/// administrator's account, whose runs are the owner's own (docs/account-isolation.md, 「The scope」).
pub async fn node_of<C: ConnectionTrait>(db: &C, account: &str) -> Result<Option<String>, Refusal> {
  let row = one(
    db,
    r#"SELECT "node" FROM "person_nodes" WHERE "owner_account" = $1"#,
    vec![account.into()],
  )
  .await?;
  let recorded = row.map(|row| row.try_get::<String>("", "node")).transpose()?;

  placed_for(db, Some(account), recorded).await
}

/// The node a supervisor's program stands on: the one its account's runs stand on, none for a supervisor of no
/// account, for one of the administrator's account its record places nowhere, and for a name that is no
/// supervisor's.
pub async fn supervisor_node<C: ConnectionTrait>(
  db: &C,
  supervisor: &str,
) -> Result<Option<String>, Refusal> {
  match crate::supervisors::named(db, supervisor).await?.and_then(|row| row.owner_account) {
    Some(account) => node_of(db, &account.to_string()).await,
    None => Ok(None),
  }
}

/// The node the runs of an account stand on, given the node its record names: that node, else the account's
/// isolation node; none for no account and for the administrator's.
pub async fn placed_for<C: ConnectionTrait>(
  db: &C,
  account: Option<&str>,
  recorded: Option<String>,
) -> Result<Option<String>, Refusal> {
  let Some(account) = account else { return Ok(None) };

  if recorded.is_some() {
    return Ok(recorded);
  }

  if crate::account_isolation::owners_own(db, account).await? {
    return Ok(None);
  }

  Ok(Some(crate::account_isolation::node_name(account)))
}

/// Why a record cannot name a node: another account's record names it, and the two are not both shared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Taken {
  pub node: String,
  /// The account whose record names the node.
  pub by: String,
}

impl std::fmt::Display for Taken {
  fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      out,
      "{} is account {}'s node; accounts share a node only where each one's record is shared",
      self.node, self.by
    )
  }
}

/// An account's node set, replacing the one it had, or why it cannot be: another account's record names the node
/// and the two records are not both shared.
pub async fn set(
  db: &DatabaseConnection,
  account: &str,
  node: &str,
  shared: bool,
) -> Result<Result<PersonNode, Taken>, Refusal> {
  let taken = one(
    db,
    r#"SELECT "owner_account" FROM "person_nodes"
       WHERE "node" = $1 AND "owner_account" <> $2 AND NOT ("shared" AND $3)
       ORDER BY "owner_account" LIMIT 1"#,
    vec![node.into(), account.into(), shared.into()],
  )
  .await?;

  if let Some(row) = taken {
    return Ok(Err(Taken {
      node: node.to_owned(),
      by: row.try_get("", "owner_account")?,
    }));
  }

  let at = crate::now_ms();

  run(
    db,
    r#"INSERT INTO "person_nodes" ("owner_account", "node", "shared", "set_at") VALUES ($1, $2, $3, $4)
       ON CONFLICT ("owner_account") DO UPDATE SET "node" = $2, "shared" = $3, "set_at" = $4"#,
    vec![account.into(), node.into(), shared.into(), at.into()],
  )
  .await?;

  Ok(Ok(PersonNode {
    owner_account: account.to_owned(),
    node: node.to_owned(),
    shared,
    set_at: at,
  }))
}

/// Whether an account's record names `node` shared, which gives the account a root of its own there.
pub async fn shares<C: ConnectionTrait>(db: &C, account: &str, node: &str) -> Result<bool, Refusal> {
  let row = one(
    db,
    r#"SELECT 1 AS "one" FROM "person_nodes" WHERE "owner_account" = $1 AND "node" = $2 AND "shared""#,
    vec![account.into(), node.into()],
  )
  .await?;

  Ok(row.is_some())
}

/// The node an account's record names where another account's record names it too: a node several accounts share,
/// where every program runs as the node's one user and reads what every other program there holds, so no login
/// lent to the account is ever placed there (docs/supply.md, 「The lendable accounts」); none where the account's
/// node is its own.
pub async fn shared_node<C: ConnectionTrait>(db: &C, account: &str) -> Result<Option<String>, Refusal> {
  let row = one(
    db,
    r#"SELECT p."node" FROM "person_nodes" p
       WHERE p."owner_account" = $1
         AND EXISTS (SELECT 1 FROM "person_nodes" o WHERE o."node" = p."node" AND o."owner_account" <> p."owner_account")"#,
    vec![account.into()],
  )
  .await?;

  Ok(row.map(|row| row.try_get("", "node")).transpose()?)
}

/// Each supervisor of an account whose record names `node` shared, with that account, by name: the supervisors whose
/// folders stand under their account's root on the node.
pub async fn sharing_supervisors<C: ConnectionTrait>(
  db: &C,
  node: &str,
) -> Result<Vec<(String, String)>, Refusal> {
  let rows = all(
    db,
    r#"SELECT s."name", p."owner_account" FROM "supervisors" s
       JOIN "person_nodes" p ON p."owner_account" = s."owner_account"::text
       WHERE p."node" = $1 AND p."shared"
       ORDER BY s."name""#,
    vec![node.into()],
  )
  .await?;

  Ok(
    rows
      .iter()
      .map(|row| Ok((row.try_get("", "name")?, row.try_get("", "owner_account")?)))
      .collect::<Result<_, sea_orm::DbErr>>()?,
  )
}

/// An account's node taken away; whether it had one.
pub async fn clear(db: &DatabaseConnection, account: &str) -> Result<bool, Refusal> {
  Ok(run(db, r#"DELETE FROM "person_nodes" WHERE "owner_account" = $1"#, vec![account.into()]).await? > 0)
}

/// Why a node cannot be an account's, or none where the node service keeps it as a hub node reserved for no
/// service: a person's machine.
pub async fn refusal(nodes: &dyn NodeService, node: &str) -> Result<Option<String>, PeerFault> {
  let kept = nodes.node_reads().await?.into_iter().find(|read| read.name == node);

  Ok(match kept {
    None => Some(format!("{node} is no node the node service keeps")),
    Some(read) if read.kind != NodeKind::Jake => {
      Some(format!("{node} is no hub node, so it is no person's machine"))
    }
    Some(read) => read
      .role
      .map(|role| format!("{node} is reserved for {role}, so it is no person's machine")),
  })
}

/// The node each supervisor stands on, by name: its owner account's, none for a supervisor of no account or of
/// the administrator's account.
pub async fn supervisors_placed(db: &DatabaseConnection) -> Result<Vec<(String, Option<String>)>, Refusal> {
  let rows = all(
    db,
    r#"SELECT s."name", s."owner_account"::text AS "account", p."node" FROM "supervisors" s
       LEFT JOIN "person_nodes" p ON p."owner_account" = s."owner_account"::text
       ORDER BY s."name""#,
    vec![],
  )
  .await?;
  let mut placed = Vec::new();

  for row in &rows {
    let account: Option<String> = row.try_get("", "account")?;
    let node = placed_for(db, account.as_deref(), row.try_get("", "node")?).await?;

    placed.push((row.try_get("", "name")?, node));
  }

  Ok(placed)
}

/// Every account a supervisor or a role agent belongs to, each once: the accounts whose runs may be placed.
pub async fn accounts_placed(db: &DatabaseConnection) -> Result<Vec<String>, Refusal> {
  let rows = all(
    db,
    r#"SELECT "owner_account"::text AS "account" FROM "supervisors" WHERE "owner_account" IS NOT NULL
       UNION SELECT "owner_account" AS "account" FROM "role_agents" WHERE "owner_account" IS NOT NULL
       ORDER BY "account""#,
    vec![],
  )
  .await?;

  Ok(rows.iter().map(|row| row.try_get("", "account")).collect::<Result<_, _>>()?)
}

/// Where a role run stands and for whom: its seat's account, and that account's node.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunPlaced {
  pub account: Option<String>,
  pub node: Option<String>,
}

/// The account a role run is of, read off the seat its slot holds at the folder it works in, and the node that
/// account's record places it on; neither for a run of no account.
pub async fn run_placed<C: ConnectionTrait>(
  db: &C,
  role: Option<&str>,
  cwd: &str,
) -> Result<RunPlaced, Refusal> {
  let seat = crate::role_agents::seated_at(db, role, Some(cwd)).await?;
  let Some(account) = seat.and_then(|seat| seat.owner_account) else {
    return Ok(RunPlaced::default());
  };
  let node = node_of(db, &account).await?;

  Ok(RunPlaced {
    account: Some(account),
    node,
  })
}

/// The accounts whose record names `node`, and the account an isolation node is named for, by account; none for a
/// node that is no person's.
pub async fn owners_of<C: ConnectionTrait>(db: &C, node: &str) -> Result<Vec<String>, Refusal> {
  let rows = all(
    db,
    r#"SELECT "owner_account" FROM "person_nodes" WHERE "node" = $1 ORDER BY "owner_account""#,
    vec![node.into()],
  )
  .await?;
  let mut owners: Vec<String> =
    rows.iter().map(|row| row.try_get("", "owner_account")).collect::<Result<_, _>>()?;

  if let Some(isolation) = crate::account_isolation::Isolation::of_node(node)
    && !owners.contains(&isolation.account)
  {
    owners.push(isolation.account);
    owners.sort();
  }

  Ok(owners)
}

/// A caller as its tool session's record names it: the record's holder, and the role and the folder its labels
/// hold, where they hold them.
#[derive(Clone, Copy, Debug)]
pub struct Caller<'a> {
  pub agent: &'a str,
  pub role: Option<&'a str>,
  pub checkout: Option<&'a str>,
}

/// What a caller acts as: the account it acts for, none for the deployment's own, and whether it holds the
/// administrator's grant, which an administrator's supervisor alone does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActingFor {
  pub account: Option<String>,
  pub administrator: bool,
}

impl ActingFor {
  /// The administrator's grants a tool session's record for this caller holds: the policy's for an
  /// administrator's supervisor, none for any other.
  pub fn grants(&self) -> Vec<String> {
    if self.administrator {
      policy::person_nodes::PERSON_NODES.administrator_grants.clone()
    } else {
      Vec::new()
    }
  }

  /// The capabilities that operate the platform a tool session's record for this caller holds: the policy's for an
  /// administrator's supervisor, none for any other.
  pub fn capabilities(&self) -> Vec<String> {
    if self.administrator {
      policy::person_nodes::PERSON_NODES.administrator_capabilities.clone()
    } else {
      Vec::new()
    }
  }
}

/// What a caller acts as. Its account is the supervisor's it is, else the role agent's
/// of its name, else the seat's its run holds at its role and folder, each by its `ownerAccount`. It holds the
/// administrator's grant where it is a supervisor of no account or of the administrator's account. A thrown-away session's place is read as its
/// session's agent.
pub async fn acting_for<C: ConnectionTrait>(
  db: &C,
  data_root: &Path,
  caller: Caller<'_>,
) -> Result<ActingFor, Refusal> {
  let agent =
    crate::apart_places::session_agent(data_root, caller.agent).unwrap_or_else(|| caller.agent.to_owned());

  if let Some(supervisor) = crate::supervisors::named(db, &agent).await? {
    let account = supervisor.owner_account.map(|account| account.to_string());
    let administrator = match &account {
      None => true,
      Some(account) => crate::account_isolation::owners_own(db, account).await?,
    };

    return Ok(ActingFor {
      administrator,
      account,
    });
  }

  let account = match crate::role_agents::named(db, &agent).await? {
    Some(seat) => seat.owner_account,
    None if caller.role.is_none() && caller.checkout.is_none() => None,
    None => crate::role_agents::seated_at(db, caller.role, caller.checkout)
      .await?
      .and_then(|seat| seat.owner_account),
  };

  Ok(ActingFor {
    account,
    administrator: false,
  })
}

/// Whether a caller reaches a node (docs/api.md, 「person-nodes」).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Access {
  pub node: String,
  pub owners: Vec<String>,
  pub agent: String,
  /// The account the caller acts for; none for the deployment's own.
  pub account: Option<String>,
  /// Whether it holds the administrator's grant, which reaches every node.
  pub administrator: bool,
  pub reaches: bool,
  /// The node path of the workplace root of the caller's account, where its record names the node shared and the
  /// node's workplace is set up; the node tool runs the caller's command inside it.
  pub root: Option<String>,
}

/// Whether `caller` reaches `node`: where it holds the administrator's grant, or its account owns the node.
pub async fn access<C: ConnectionTrait>(
  db: &C,
  data_root: &Path,
  node: &str,
  caller: Caller<'_>,
) -> Result<Access, Refusal> {
  let owners = owners_of(db, node).await?;
  let ActingFor {
    account,
    administrator,
  } = acting_for(db, data_root, caller).await?;
  let reaches = administrator || account.as_ref().is_some_and(|account| owners.contains(account));
  let root = match &account {
    Some(account) if shares(db, account, node).await? => crate::watches::links::node_root(db, node)
      .await?
      .map(|node_root| account_root(&node_root, account)),
    _ => None,
  };

  Ok(Access {
    node: node.to_owned(),
    owners,
    agent: caller.agent.to_owned(),
    account,
    administrator,
    reaches,
    root,
  })
}

/// Where a run stands as the store says now: on a node, or on this machine where the node is none; or nowhere,
/// since the agent it was carried for no longer stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
  On(Option<String>),
  Gone,
}

/// What a run's wait for its node came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Waited {
  /// It goes on, on the node named, or on this machine where none is.
  On(Option<String>),
  /// The agent it was carried for no longer stands.
  Gone,
  /// A stop standing over its run ended the wait.
  Stopped,
  /// It waited past its bound: why, naming the node and how long its link went unheard.
  Overdue(String),
}

/// What a run's wait for its node asks and tells as it goes (docs/person-nodes.md, 「Which node is a person's」).
pub trait NodeWait {
  /// Whether the node's link was heard from lately.
  fn heard(&self, node: &str) -> bool;
  /// The run's placement as the present records give it.
  fn present(&mut self) -> impl Future<Output = Result<Placement, Refusal>> + Send;
  /// Whether a stop stands over the run.
  fn stopped(&mut self) -> impl Future<Output = bool> + Send;
  /// The wait told: the node it waits on now and the moment it began waiting, or none once it waits no more.
  fn told(&mut self, waiting: Option<(&str, i64)>) -> impl Future<Output = ()> + Send;
}

// How long a link went unheard, as its reason says it.
fn spoken(waited: Duration) -> String {
  match waited.as_secs() {
    seconds if seconds >= 60 && seconds % 60 == 0 => format!("{} minutes", seconds / 60),
    seconds => format!("{seconds} seconds"),
  }
}

/// Waits while the node a run stands on is not heard from, asking again every `every`, and reads the run's
/// placement afresh before each ask (docs/person-nodes.md, 「Which node is a person's」): a node heard from goes on
/// at once, a placement naming another node, or none, is followed, an agent gone ends the wait, a stop standing
/// over the run ends it, and so does a wait past `bound`. A placement that cannot be read leaves the node as it
/// was. The wait is told as it begins and as it moves to another node, and told cleared once it ends.
pub async fn until_placed(
  run: &str,
  mut node: Option<String>,
  wait: &mut impl NodeWait,
  every: Duration,
  bound: Duration,
) -> Waited {
  let began = tokio::time::Instant::now();
  let since = crate::now_ms();
  let mut waited_on: Option<String> = None;

  let came = loop {
    let Some(unheard) = node.as_deref().filter(|node| !wait.heard(node)).map(str::to_owned) else {
      if waited_on.is_some() {
        println!("{run}: {} is heard, so it goes on", node.as_deref().unwrap_or("this machine"));
      }

      break Waited::On(node);
    };

    if waited_on.as_deref() != Some(unheard.as_str()) {
      println!("{run}: waits for {unheard}, whose link is not heard");
      wait.told(Some((&unheard, since))).await;
      waited_on = Some(unheard.clone());
    }

    let waited = began.elapsed();

    if waited >= bound {
      let why = format!(
        "{unheard}'s link was not heard for {}, so the turn ends without starting",
        spoken(bound)
      );

      println!("{run}: {why}");
      break Waited::Overdue(why);
    }

    tokio::time::sleep(every.min(bound - waited)).await;

    if wait.stopped().await {
      println!("{run}: a stop stands over it, so it waits for {unheard} no more");
      break Waited::Stopped;
    }

    match wait.present().await {
      Ok(Placement::Gone) => {
        println!("{run}: the agent it was carried for no longer stands, so it waits no more");
        break Waited::Gone;
      }
      Ok(Placement::On(now)) => {
        if now != node {
          println!(
            "{run}: placed again on {} by the present record",
            now.as_deref().unwrap_or("this machine")
          );
        }

        node = now;
      }
      Err(why) => eprintln!("{run}: where it stands could not be read again: {}", why.detail),
    }
  };

  if waited_on.is_some() {
    wait.told(None).await;
  }

  came
}
