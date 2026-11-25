//! An owner — a login account — or an organisation unit removed for good (docs/api.md, 「owners and organisation
//! units removed for good」): what agency keeps of it counted, changing nothing, and removed through agency's own
//! removals, the administrator's call alone. Every row is found by the number handed, so nothing of no account's and
//! nothing of another account's is touched; a removal asked again takes nothing more.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::{Method, StatusCode};
use axum::response::Response;
use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde_json::json;
use store::{all, one, run};

use super::supervisor_removal::{Left, remove};
use super::supervisor_reset::{reset_whole, to_its_end};
use super::{Bearer, Call, Refused, Route, answer};
use crate::account_isolation::Isolation;
use crate::order_run::dispatch;
use crate::places::Ground;
use crate::supply::reservations::{self, Owner};
use crate::verbs::work_node_teardown::{self as teardown, Reach};

/// Who a supervisor reset by a removal is recorded as reset by.
const REMOVED_BY: &str = "owner-removal";

/// Why the slot requests of an account removed for good are given back.
const GIVEN_BACK: &str = "its account was removed for good";

/// The kinds an account's count and removal answer, every one always present.
pub const ACCOUNT_KINDS: [&str; 17] = [
  "supervisors",
  "slot-requests",
  "role-agents",
  "attachments",
  "mcp-servers",
  "skills",
  "repositories",
  "repository-checkouts",
  "registered-models",
  "claude-accounts",
  "personal-supply",
  "supply-reservations",
  "call-keys",
  "gateway-requests",
  "person-nodes",
  "workplaces",
  "isolation-users",
];

/// The kinds an organisation unit's count and removal answer, every one always present.
pub const UNIT_KINDS: [&str; 8] = [
  "supervisors",
  "repositories",
  "repository-checkouts",
  "role-agents",
  "registered-models",
  "organisation-supply",
  "claude-accounts",
  "supply-reservations",
];

/// Counts by kind, every kind of a list present.
#[derive(Clone, Debug, Default)]
struct Tally(BTreeMap<String, i64>);

impl Tally {
  fn of(kinds: &[&str]) -> Self {
    Self(kinds.iter().map(|kind| ((*kind).to_owned(), 0)).collect())
  }

  fn add(&mut self, kind: &str, by: i64) {
    *self.0.entry(kind.to_owned()).or_default() += by;
  }

  fn json(&self) -> serde_json::Value {
    json!(self.0)
  }
}

fn forbidden() -> Refused {
  Refused::new(
    StatusCode::FORBIDDEN,
    "Only the platform's administrator removes an owner or an organisation unit for good.",
  )
  .with_code("forbidden")
}

/// The administrator's call alone: the control token with `Owner-Role: administrator`.
fn administrator(call: &Call) -> Result<(), Refused> {
  let named = call
    .header("owner-role")
    .is_some_and(|role| role.trim().eq_ignore_ascii_case("administrator"));

  if *call.bearer() == Bearer::Control && named { Ok(()) } else { Err(forbidden()) }
}

/// A positive decimal whole number, or the refusal naming the field it was given as.
fn positive(given: &str, field: &str) -> Result<i64, Refused> {
  given
    .bytes()
    .all(|byte| byte.is_ascii_digit())
    .then(|| given.parse::<i64>().ok())
    .flatten()
    .filter(|number| *number > 0)
    .ok_or_else(|| {
      Refused::new(StatusCode::BAD_REQUEST, format!("{given:?} is no positive whole number"))
        .with_field(field)
    })
}

fn account_of(call: &Call) -> Result<i64, Refused> {
  administrator(call)?;
  positive(call.param("account").unwrap_or_default(), "account")
}

/// The unit a call names, its company read where given and no further, since agency keys nothing by a company.
fn unit_of(call: &Call) -> Result<i64, Refused> {
  administrator(call)?;

  let unit = positive(call.param("unit").unwrap_or_default(), "unit")?;

  if let Some(company) = call.query("company") {
    positive(&company, "company")?;
  }

  Ok(unit)
}

async fn count<C: ConnectionTrait>(db: &C, sql: &str, values: Vec<sea_orm::Value>) -> Result<i64, Refused> {
  Ok(match one(db, sql, values).await? {
    Some(row) => row.try_get::<i64>("", "n")?,
    None => 0,
  })
}

async fn names<C: ConnectionTrait>(
  db: &C,
  sql: &str,
  values: Vec<sea_orm::Value>,
) -> Result<Vec<String>, Refused> {
  all(db, sql, values)
    .await?
    .iter()
    .map(|row| row.try_get::<String>("", "name").map_err(Refused::from))
    .collect()
}

// The skills standing in an account's own folder under the data root.
fn skills_in(folder: &Path) -> i64 {
  std::fs::read_dir(folder).map_or(0, |entries| {
    entries
      .filter_map(Result::ok)
      .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
      .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
      .count() as i64
  })
}

// A folder taken away, one already gone being nothing to do.
fn folder_gone(folder: &Path) -> Result<(), Refused> {
  match std::fs::remove_dir_all(folder) {
    Ok(()) => Ok(()),
    Err(why) if why.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(why) => Err(failed(format!("{}: {why}", folder.display()))),
  }
}

fn failed(why: impl Into<String>) -> Refused {
  Refused::new(StatusCode::INTERNAL_SERVER_ERROR, why).with_code("failed")
}

/// The PC an account's node record names where no other account's record names it: the node is the account's
/// alone, so its workplace goes with the account. None for an isolation node or a node another account shares.
async fn own_pc<C: ConnectionTrait>(db: &C, account: i64) -> Result<Option<String>, Refused> {
  let row = one(
    db,
    r#"SELECT p."node" AS "name" FROM "person_nodes" p
       WHERE p."owner_account" = $1
         AND NOT EXISTS (SELECT 1 FROM "person_nodes" o WHERE o."node" = p."node" AND o."owner_account" <> p."owner_account")"#,
    vec![account.to_string().into()],
  )
  .await?;
  let node: Option<String> = row.map(|row| row.try_get("", "name")).transpose()?;

  Ok(node.filter(|node| Isolation::of_node(node).is_none()))
}

// The repositories filed under an owner column, and how many node and support checkouts' rows go with them.
async fn repositories_of<C: ConnectionTrait>(
  db: &C,
  column: &str,
  id: i64,
) -> Result<(Vec<String>, i64), Refused> {
  let held = names(
    db,
    &format!(r#"SELECT "name" FROM "repositories" WHERE "{column}" = $1 ORDER BY "name""#),
    vec![id.into()],
  )
  .await?;
  let checkouts = count(
    db,
    &format!(
      r#"SELECT (SELECT COUNT(*) FROM "repository_node_checkouts" c JOIN "repositories" r ON r."name" = c."repository" WHERE r."{column}" = $1)
              + (SELECT COUNT(*) FROM "support_checkouts" c JOIN "repositories" r ON r."name" = c."repository" WHERE r."{column}" = $1) AS "n""#
    ),
    vec![id.into()],
  )
  .await?;

  Ok((held, checkouts))
}

// The Claude accounts given to a person or a unit, by their ids.
async fn claude_accounts_of<C: ConnectionTrait>(
  db: &C,
  column: &str,
  id: i64,
) -> Result<Vec<String>, Refused> {
  all(
    db,
    &format!(r#"SELECT "id" FROM "claude_accounts" WHERE "{column}" = $1 ORDER BY "id""#),
    vec![id.into()],
  )
  .await?
  .iter()
  .map(|row| row.try_get::<String>("", "id").map_err(Refused::from))
  .collect()
}

/// What agency keeps of an account, counted as its removal would take it now.
async fn account_counts(ground: &Ground, reach: Reach, account: i64) -> Result<Tally, Refused> {
  let db = &ground.db;
  let text = account.to_string();
  let mut tally = Tally::of(&ACCOUNT_KINDS);
  let by_number =
    |table: &str, column: &str| format!(r#"SELECT COUNT(*) AS "n" FROM "{table}" WHERE "{column}" = $1"#);

  for (kind, table, column, textual) in [
    ("supervisors", "supervisors", "owner_account", false),
    ("slot-requests", "role_slot_requests", "account", true),
    ("role-agents", "role_agents", "owner_account", false),
    ("attachments", "agent_attachments", "account", true),
    ("mcp-servers", "mcp_servers", "account", true),
    ("registered-models", "registered_models", "owner_account", false),
    ("claude-accounts", "claude_accounts", "owner_account", false),
    ("personal-supply", "personal_supply", "owner_account", false),
    ("supply-reservations", "supply_reservations", "account", false),
    ("call-keys", "gateway_call_keys", "account", false),
    ("gateway-requests", "gateway_requests", "account", false),
    ("person-nodes", "person_nodes", "owner_account", true),
  ] {
    let value: sea_orm::Value = if textual { text.clone().into() } else { account.into() };

    tally.add(kind, count(db, &by_number(table, column), vec![value]).await?);
  }

  tally.add("skills", skills_in(&crate::skills::folder_of(&ground.roots.data, &text)));

  let (repositories, checkouts) = repositories_of(db, "owner_account", account).await?;

  tally.add("repositories", repositories.len() as i64);
  tally.add("repository-checkouts", checkouts);

  let isolation =
    Isolation::of_account(&text).ok_or_else(|| failed("no isolation node for the account"))?;
  let mut workplaces = i64::from(teardown::workplace_stands(db, &isolation.node).await?);

  if let Some(pc) = own_pc(db, account).await? {
    workplaces += i64::from(teardown::workplace_stands(db, &pc).await?);
  }

  tally.add("workplaces", workplaces);
  tally.add("isolation-users", i64::from(teardown::user_stands(reach, &isolation.user).await));

  Ok(tally)
}

/// What the account's removal reads of the call before it runs apart from the request.
struct Removal {
  ground: Arc<Ground>,
  reach: Reach,
  sessions: Option<crate::herdr_backend::Shared>,
  account: i64,
  /// Each supervisor of the account and what is left of it to take once its reset has run.
  supervisors: Vec<(String, Left)>,
}

impl Removal {
  fn of(call: &Call, account: i64, supervisors: Vec<String>) -> Self {
    Self {
      ground: call.shared_ground(),
      reach: Reach::of(call.machine()),
      sessions: call.sessions(),
      account,
      supervisors: supervisors.into_iter().map(|name| (name.clone(), Left::of(call, &name))).collect(),
    }
  }

  fn db(&self) -> &DatabaseConnection {
    &self.ground.db
  }

  fn agents_root(&self) -> PathBuf {
    self.ground.roots.agents.clone()
  }
}

/// Refused while a job holds a checkout of a repository filed under an owner column, before anything is taken.
async fn none_held(ground: &Ground, column: &str, id: i64) -> Result<(), Refused> {
  let (repositories, _) = repositories_of(&ground.db, column, id).await?;

  for name in repositories {
    if let Some(checkout) = super::repository_routes::held_checkout(ground, &name).await? {
      return Err(
        Refused::new(StatusCode::CONFLICT, format!("A job holds {checkout}, a checkout of {name}."))
          .with_code("conflict"),
      );
    }
  }

  Ok(())
}

// The repositories filed under an owner column taken away, as their removal takes each; how many repositories and
// checkouts' rows went.
async fn repositories_gone(ground: &Ground, column: &str, id: i64) -> Result<(i64, i64), Refused> {
  let (repositories, checkouts) = repositories_of(&ground.db, column, id).await?;

  for name in &repositories {
    super::repository_routes::take_away(ground, name).await?;
  }

  Ok((repositories.len() as i64, checkouts))
}

// The Claude accounts named taken away, with their usage windows and balances and the settings kept under them:
// their rests, their models' rests and their marks.
async fn claude_accounts_gone(db: &DatabaseConnection, ids: &[String]) -> Result<i64, Refused> {
  for id in ids {
    let written = sea_orm::TransactionTrait::begin(db).await?;

    for table in ["usage_windows", "usage_balances"] {
      run(
        &written,
        &format!(r#"DELETE FROM "{table}" WHERE "account" = $1"#),
        vec![id.clone().into()],
      )
      .await?;
    }

    for prefix in [
      "rest_until:",
      "model_rest:",
      "usage_windows_unread:",
      crate::control::claude_accounts::NODE_TOKEN_GONE_KEY,
    ] {
      let key = format!("{prefix}{id}");

      run(
        &written,
        r#"DELETE FROM "settings" WHERE "key" = $1 OR left("key", length($1) + 1) = $1 || ':'"#,
        vec![key.into()],
      )
      .await?;
    }

    run(&written, r#"DELETE FROM "claude_accounts" WHERE "id" = $1"#, vec![id.clone().into()]).await?;
    written.commit().await?;
  }

  super::claude_account_changes::let_choice_go_if_off(db).await?;

  Ok(ids.len() as i64)
}

// The reservations of an owner taken: each still open captured through the outbox, and then the rows themselves for
// an account, or their unit taken off them for a unit, since each is a person's run's own. How many rows it reached.
async fn reservations_gone(ground: &Ground, owner: Owner) -> Result<i64, Refused> {
  reservations::settle_open_of(ground, owner).await?;
  reservations::send_due(ground).await;

  let reached = match owner {
    Owner::Account(id) => {
      run(&ground.db, r#"DELETE FROM "supply_reservations" WHERE "account" = $1"#, vec![id.into()]).await?
    }
    Owner::Unit(id) => {
      run(
        &ground.db,
        r#"UPDATE "supply_reservations" SET "unit_id" = NULL WHERE "unit_id" = $1"#,
        vec![id.into()],
      )
      .await?
    }
  };

  Ok(reached as i64)
}

impl Removal {
  /// Everything of the account taken, each part through agency's own removal of it.
  async fn carry_out(self) -> Result<Tally, Refused> {
    let ground = self.ground.clone();
    let db = self.db().clone();
    let account = self.account;
    let text = account.to_string();
    let mut removed = Tally::of(&ACCOUNT_KINDS);

    // Its supervisors, each as its own removal takes one once its reset has run, its brain kept for the brain's
    // own removal.
    for (name, left) in &self.supervisors {
      let written = sea_orm::TransactionTrait::begin(&db).await?;

      match remove(&written, name).await {
        Ok(()) => {}
        Err(super::fault::Fault::Field(field, error)) => return Err(failed(format!("{field}: {error}"))),
        Err(super::fault::Fault::Failed(refused)) => return Err(refused),
      }

      written.commit().await?;
      left.taken(name, false).await;
      folder_gone(&self.agents_root().join("supervisor-skills").join(name))?;
      removed.add("supervisors", 1);
    }

    // Its slot requests: each still open given back as its owner's release gives one back, the role session bound
    // to it ended, and then every row of them.
    let requests = all(
      &db,
      r#"SELECT "token", "state", "session", "child_id" FROM "role_slot_requests" WHERE "account" = $1"#,
      vec![text.clone().into()],
    )
    .await?;
    let mut sessions = vec![];

    for row in &requests {
      let token: String = row.try_get("", "token")?;
      let state: String = row.try_get("", "state")?;
      let session: Option<String> = row.try_get("", "session")?;
      let child: Option<String> = row.try_get("", "child_id")?;
      let active = state == "active";

      if !["waiting", "reserved", "active"].contains(&state.as_str()) {
        continue;
      }

      if active && let Some(session) = session.filter(|session| child.as_ref() == Some(session)) {
        sessions.push(session);
      }

      dispatch::finish_by(&db, &ground.events, &token, GIVEN_BACK, !active, dispatch::BY_OWNER)
        .await
        .map_err(failed)?;
    }

    if let Some(backend) = self.sessions.clone()
      && !sessions.is_empty()
    {
      tokio::spawn(async move {
        for session in sessions {
          if let Err(fault) = backend
            .end_under_turns(&agent_session::SessionId(session.clone()), super::pool_routes::RELEASED)
            .await
          {
            eprintln!(
              "agency: the role session {session} of a removed account was not ended: {}",
              fault.message
            );
          }
        }
      });
    }

    removed.add(
      "slot-requests",
      run(
        &db,
        r#"DELETE FROM "role_slot_requests" WHERE "account" = $1"#,
        vec![text.clone().into()],
      )
      .await? as i64,
    );

    // Its role agents, with their own plugins.
    for agent in names(
      &db,
      r#"SELECT "name" FROM "role_agents" WHERE "owner_account" = $1 ORDER BY "name""#,
      vec![account.into()],
    )
    .await?
    {
      crate::role_agents::remove(&db, &agent).await?;
      folder_gone(&self.agents_root().join("role-agents").join(&agent))?;
      removed.add("role-agents", 1);
    }

    for (kind, table) in [("attachments", "agent_attachments"), ("mcp-servers", "mcp_servers")] {
      let gone = run(
        &db,
        &format!(r#"DELETE FROM "{table}" WHERE "account" = $1"#),
        vec![text.clone().into()],
      )
      .await?;

      removed.add(kind, gone as i64);
    }

    let skills = crate::skills::folder_of(&ground.roots.data, &text);

    removed.add("skills", skills_in(&skills));
    folder_gone(&skills)?;

    let (repositories, checkouts) = repositories_gone(&ground, "owner_account", account).await?;

    removed.add("repositories", repositories);
    removed.add("repository-checkouts", checkouts);

    // Its registrations, each as the registry removes one.
    for name in names(
      &db,
      r#"SELECT "name" FROM "registered_models" WHERE "owner_account" = $1 ORDER BY "name""#,
      vec![account.into()],
    )
    .await?
    {
      let removal = json!({ "action": "remove", "name": name });

      if let Err(sentence) = super::registry::change(&db, &ground.roots.data, &removal).await? {
        return Err(Refused::new(StatusCode::CONFLICT, sentence).with_code("conflict"));
      }

      removed.add("registered-models", 1);
    }

    let ids = claude_accounts_of(&db, "owner_account", account).await?;

    removed.add("claude-accounts", claude_accounts_gone(&db, &ids).await?);
    removed.add(
      "personal-supply",
      run(&db, r#"DELETE FROM "personal_supply" WHERE "owner_account" = $1"#, vec![account.into()]).await?
        as i64,
    );
    removed.add("supply-reservations", reservations_gone(&ground, Owner::Account(account)).await?);

    crate::call_keys::revoke(&db, &ground.roots.data, account).await?;
    removed.add(
      "call-keys",
      run(&db, r#"DELETE FROM "gateway_call_keys" WHERE "account" = $1"#, vec![account.into()]).await?
        as i64,
    );
    removed.add(
      "gateway-requests",
      run(&db, r#"DELETE FROM "gateway_requests" WHERE "account" = $1"#, vec![account.into()]).await?
        as i64,
    );

    // Its nodes: the PC its record names where it is the account's alone, its record, and its isolation node.
    if let Some(pc) = own_pc(&db, account).await? {
      if self.reach == Reach::Machine {
        teardown::stop_node_units(&pc).await;
      }

      let gone = teardown::forget_node(&db, &ground.roots.data, &pc)
        .await
        .map_err(|why| failed(why.to_string()))?;

      // Off the node service's list too, before the record that names it goes, so a removal broken here
      // finds the PC again; the node's workspace and its pane end on the next pass over the list.
      ground.peers.node.remove_node(&pc).await.map_err(|fault| failed(fault.to_string()))?;
      removed.add("workplaces", i64::from(gone));
    }

    removed.add("person-nodes", i64::from(crate::person_nodes::clear(&db, &text).await?));

    let isolation =
      Isolation::of_account(&text).ok_or_else(|| failed("no isolation node for the account"))?;
    let torn = teardown::tear_down(&db, &ground.roots.data, &isolation.node, self.reach)
      .await
      .map_err(|why| failed(why.to_string()))?;

    removed.add("workplaces", i64::from(torn.workplace));
    removed.add("isolation-users", i64::from(torn.user));

    Ok(removed)
  }
}

/// What agency keeps of an organisation unit, counted as its removal would take it now.
async fn unit_counts(ground: &Ground, unit: i64) -> Result<Tally, Refused> {
  let db = &ground.db;
  let mut tally = Tally::of(&UNIT_KINDS);

  for (kind, table) in [
    ("supervisors", "supervisors"),
    ("role-agents", "role_agents"),
    ("registered-models", "registered_models"),
  ] {
    tally.add(
      kind,
      count(
        db,
        &format!(r#"SELECT COUNT(*) AS "n" FROM "{table}" WHERE "owner_unit" = $1"#),
        vec![unit.into()],
      )
      .await?,
    );
  }

  let (repositories, checkouts) = repositories_of(db, "owner_unit", unit).await?;

  tally.add("repositories", repositories.len() as i64);
  tally.add("repository-checkouts", checkouts);
  tally.add(
    "organisation-supply",
    count(
      db,
      r#"SELECT COUNT(*) AS "n" FROM "personal_supply" WHERE "owner_unit" = $1"#,
      vec![unit.into()],
    )
    .await?,
  );
  tally.add("claude-accounts", claude_accounts_of(db, "owner_unit", unit).await?.len() as i64);
  tally.add(
    "supply-reservations",
    count(
      db,
      r#"SELECT COUNT(*) AS "n" FROM "supply_reservations" WHERE "unit_id" = $1"#,
      vec![unit.into()],
    )
    .await?,
  );

  Ok(tally)
}

/// Everything agency keeps of an organisation unit taken: its supervisors each as its own removal takes one,
/// its seats with their plugins, its registrations as the registry removes them, and the resources removed
/// under the account's rule alike.
async fn unit_removed(
  ground: Arc<Ground>,
  unit: i64,
  supervisors: Vec<(String, Left)>,
) -> Result<Tally, Refused> {
  let db = ground.db.clone();
  let mut removed = Tally::of(&UNIT_KINDS);

  for (name, left) in supervisors {
    let written = sea_orm::TransactionTrait::begin(&db).await?;

    match remove(&written, &name).await {
      Ok(()) => {}
      Err(super::fault::Fault::Field(field, error)) => return Err(failed(format!("{field}: {error}"))),
      Err(super::fault::Fault::Failed(refused)) => return Err(refused),
    }

    written.commit().await?;
    left.taken(&name, false).await;
    folder_gone(&ground.roots.agents.join("supervisor-skills").join(&name))?;
    removed.add("supervisors", 1);
  }

  for agent in names(
    &db,
    r#"SELECT "name" FROM "role_agents" WHERE "owner_unit" = $1 ORDER BY "name""#,
    vec![unit.into()],
  )
  .await?
  {
    crate::role_agents::remove(&db, &agent).await?;
    folder_gone(&ground.roots.agents.join("role-agents").join(&agent))?;
    removed.add("role-agents", 1);
  }

  for name in names(
    &db,
    r#"SELECT "name" FROM "registered_models" WHERE "owner_unit" = $1 ORDER BY "name""#,
    vec![unit.into()],
  )
  .await?
  {
    let removal = json!({ "action": "remove", "name": name });

    if let Err(sentence) = super::registry::change(&db, &ground.roots.data, &removal).await? {
      return Err(Refused::new(StatusCode::CONFLICT, sentence).with_code("conflict"));
    }

    removed.add("registered-models", 1);
  }

  let (repositories, checkouts) = repositories_gone(&ground, "owner_unit", unit).await?;

  removed.add("repositories", repositories);
  removed.add("repository-checkouts", checkouts);
  removed.add(
    "organisation-supply",
    run(&db, r#"DELETE FROM "personal_supply" WHERE "owner_unit" = $1"#, vec![unit.into()]).await? as i64,
  );

  let ids = claude_accounts_of(&db, "owner_unit", unit).await?;

  removed.add("claude-accounts", claude_accounts_gone(&db, &ids).await?);
  removed.add("supply-reservations", reservations_gone(&ground, Owner::Unit(unit)).await?);

  Ok(removed)
}

/// `GET /owners/{account}/removal`: what a removal of the account for good would take now, changing nothing.
pub struct OwnerRemovalCount;

impl Route for OwnerRemovalCount {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/owners/{account}/removal";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;
    let counts = account_counts(call.ground(), Reach::of(call.machine()), account).await?;

    Ok(answer(StatusCode::OK, json!({ "counts": counts.json() })))
  }
}

crate::moved!(OwnerRemovalCount);

/// `DELETE /owners/{account}`: everything agency keeps of the account removed for good.
pub struct OwnerRemoval;

impl Route for OwnerRemoval {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/owners/{account}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let account = account_of(&call)?;

    none_held(call.ground(), "owner_account", account).await?;

    let supervisors = names(
      call.store(),
      r#"SELECT "name" FROM "supervisors" WHERE "owner_account" = $1 ORDER BY "name""#,
      vec![account.into()],
    )
    .await?;
    let resets: Vec<_> = supervisors
      .iter()
      .map(|name| reset_whole(&call, name, vec![], REMOVED_BY, false))
      .collect();
    let removal = Removal::of(&call, account, supervisors);
    let registrations = count(
      call.store(),
      r#"SELECT COUNT(*) AS "n" FROM "registered_models" WHERE "owner_account" = $1"#,
      vec![account.into()],
    )
    .await?;
    let removed = to_its_end(async move {
      for reset in resets {
        reset.await?;
      }

      removal.carry_out().await
    })
    .await?;

    if registrations > 0 {
      super::models::restand(&call);
    }

    if removed.0.get("role-agents").is_some_and(|gone| *gone > 0) {
      super::role_agents::queue_told(&call);
    }

    eprintln!("agency: everything of the account {account} was removed for good");

    Ok(answer(StatusCode::OK, json!({ "removed": removed.json() })))
  }
}

crate::moved!(OwnerRemoval);

/// `GET /org-units/{unit}/removal`: what a removal of the unit for good would take now, changing nothing.
pub struct UnitRemovalCount;

impl Route for UnitRemovalCount {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/org-units/{unit}/removal";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let unit = unit_of(&call)?;
    let counts = unit_counts(call.ground(), unit).await?;

    Ok(answer(StatusCode::OK, json!({ "counts": counts.json() })))
  }
}

crate::moved!(UnitRemovalCount);

/// `DELETE /org-units/{unit}`: everything agency keeps of the unit removed for good.
pub struct UnitRemoval;

impl Route for UnitRemoval {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/org-units/{unit}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let unit = unit_of(&call)?;

    none_held(call.ground(), "owner_unit", unit).await?;

    let supervisors = names(
      call.store(),
      r#"SELECT "name" FROM "supervisors" WHERE "owner_unit" = $1 ORDER BY "name""#,
      vec![unit.into()],
    )
    .await?;
    let resets: Vec<_> = supervisors
      .iter()
      .map(|name| reset_whole(&call, name, vec![], REMOVED_BY, false))
      .collect();
    let left: Vec<_> = supervisors.into_iter().map(|name| (name.clone(), Left::of(&call, &name))).collect();
    let registrations = count(
      call.store(),
      r#"SELECT COUNT(*) AS "n" FROM "registered_models" WHERE "owner_unit" = $1"#,
      vec![unit.into()],
    )
    .await?;
    let seats = count(
      call.store(),
      r#"SELECT COUNT(*) AS "n" FROM "role_agents" WHERE "owner_unit" = $1"#,
      vec![unit.into()],
    )
    .await?;
    let ground = call.shared_ground();
    let removed = to_its_end(async move {
      for reset in resets {
        reset.await?;
      }

      unit_removed(ground, unit, left).await
    })
    .await?;

    if registrations > 0 {
      super::models::restand(&call);
    }

    if seats > 0 {
      super::role_agents::queue_told(&call);
    }

    eprintln!("agency: everything of the organisation unit {unit} was removed for good");

    Ok(answer(StatusCode::OK, json!({ "removed": removed.json() })))
  }
}

crate::moved!(UnitRemoval);
