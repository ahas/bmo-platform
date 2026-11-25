//! Who may read or change the agents a console manages (docs/api.md, 「The control port」): a control route
//! of the supervisors, the roles, the repositories, the registered models or the model files, asked by a
//! signed-in person the console names, goes on only where the console's permission for that person lets it:
//! each kind stands under the console section of the agent management tab that holds it, and the agent list,
//! which the console's screen reads under any one of its parts, is read under any of the five.
//!
//! A repository an organisation unit owns is the exception: a member's change of one, or an add naming the unit,
//! asks no section, its unit's heads alone passing the chain's check that follows (「unit_repository」).
//!
//! A caller carrying no `Owner-Key` is a unit, a tool, a job or the shell acting on the deployment's own
//! token, and keeps the routes' present behaviour. The platform administrator, `Owner-Role: administrator`,
//! is the owner and keeps every access. A member's permission is the console's, kept on the account service:
//! agency asks it, with that person's headers, and refuses where the answer gives no read or no write.
//!
//! A section's write lets a member change only the rows filed under their own account: a supervisor, a
//! repository or a role agent of another account, or one of the platform's, filed under none, is refused, a row a
//! member adds is filed under their account, and the roles, which no account owns, are the platform's alone. A
//! supervisor's or another holder's work stands only on a repository of its own side (「works_on」).

use std::path::PathBuf;
use std::time::Duration;

use axum::http::{Method, StatusCode};
use serde_json::Value;
use tool_call::account_service::AccountService;

use crate::control::{Call, Refused};
use crate::places::Ground;

/// One part of agent management: a kind of route, and the console section of the tab that holds it (the web's
/// `docs/accounts.md`, 「Console sections」). A role's agents stand under the roles section with the roles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
  Supervisors,
  Roles,
  RoleAgents,
  Repositories,
  Models,
  ModelFiles,
}

impl Part {
  const ALL: [Self; 6] = [
    Self::Supervisors,
    Self::Roles,
    Self::RoleAgents,
    Self::Repositories,
    Self::Models,
    Self::ModelFiles,
  ];

  /// The path the part's routes stand under.
  fn root(self) -> &'static str {
    match self {
      Self::Supervisors => "/supervisors",
      Self::Roles => "/roles",
      Self::RoleAgents => "/role-agents",
      Self::Repositories => "/repositories",
      Self::Models => "/models",
      Self::ModelFiles => "/model-catalogue",
    }
  }

  /// The console section the part stands under.
  fn section(self) -> &'static str {
    match self {
      Self::Supervisors => "supervisors",
      Self::Roles | Self::RoleAgents => "roles",
      Self::Repositories => "repositories",
      Self::Models => "models",
      Self::ModelFiles => "model-files",
    }
  }

  /// The part alone, as the parts a change of it is allowed under.
  fn alone(self) -> &'static [Self] {
    match self {
      Self::Supervisors => &[Self::Supervisors],
      Self::Roles => &[Self::Roles],
      Self::RoleAgents => &[Self::RoleAgents],
      Self::Repositories => &[Self::Repositories],
      Self::Models => &[Self::Models],
      Self::ModelFiles => &[Self::ModelFiles],
    }
  }

  /// The parts whose read lets a person read this one: the agent list is read under any part, since the
  /// console's screen opens on any one of them and an agent's page stands under the three it is made of.
  fn read_under(self) -> &'static [Self] {
    match self {
      Self::Supervisors | Self::Roles | Self::RoleAgents | Self::Repositories => &Self::ALL,
      Self::Models | Self::ModelFiles => self.alone(),
    }
  }

  /// The path parameter that names the row a route of this part changes, none for a part whose rows are judged
  /// whatever the path names.
  fn row_param(self) -> Option<&'static str> {
    match self {
      Self::Supervisors => Some("supervisorName"),
      Self::Repositories => Some("repositoryName"),
      Self::RoleAgents => Some("agentName"),
      Self::Roles | Self::Models | Self::ModelFiles => None,
    }
  }

  /// Whose the row named is: `Unnamed` for a route naming none — an add, which its route files — or a row the
  /// store does not hold, which its route answers; the account it is filed under, none for the platform's; and
  /// `Unjudged` for a part whose rows carry their owner on routes of their own.
  async fn row(self, ground: &Ground, name: Option<String>) -> Result<Row, Refused> {
    let db = &ground.db;

    Ok(match (self, name) {
      (Self::Roles, _) => Row::Platform,
      (Self::Models | Self::ModelFiles, _) => Row::Unjudged,
      (_, None) => Row::Unnamed,
      (Self::Supervisors, Some(name)) => crate::supervisors::named(db, &name)
        .await?
        .map_or(Row::Unnamed, |held| Row::Owned("supervisor", name, held.owner_account)),
      (Self::Repositories, Some(name)) => crate::repositories::named(db, &name)
        .await?
        .map_or(Row::Unnamed, |held| Row::Owned("repository", name, held.owner_account)),
      // A role agent is a seat of its account, its owner written as the account's id in digits.
      (Self::RoleAgents, Some(name)) => {
        crate::role_agents::named(db, &name).await?.map_or(Row::Unnamed, |held| {
          Row::Owned("role agent", name, held.owner_account.as_deref().map(seat_owner))
        })
      }
    })
  }

  /// The part a path stands under, none for a route of another kind.
  fn of(path: &str) -> Option<Self> {
    Self::ALL.into_iter().find(|part| {
      let root = part.root();

      path == root || path.starts_with(root) && path.as_bytes().get(root.len()) == Some(&b'/')
    })
  }
}

/// The account service is waited on for its answer: the local service answers at once, and a call that cannot
/// be made is no access, never a guessed one.
const WAIT: Duration = Duration::from_secs(10);

/// One managed route a member asks: whose permission it needs, which way, and the account service to ask.
pub struct Check {
  key: String,
  data_root: PathBuf,
  part: Part,
  write: bool,
}

/// The check a call needs, or none where it needs none: a route of another kind, a caller carrying no key, or
/// the owner.
pub fn asked(call: &Call, method: &Method, path: &str) -> Option<Check> {
  let part = Part::of(path)?;
  let key = call
    .header("owner-key")
    .map(|key| key.trim().to_owned())
    .filter(|key| !key.is_empty())?;

  if call
    .header("owner-role")
    .is_some_and(|role| role.trim().eq_ignore_ascii_case("administrator"))
  {
    return None;
  }

  Some(Check {
    key,
    data_root: call.data_root(),
    part,
    write: !matches!(method, &Method::GET | &Method::HEAD),
  })
}

/// Whether a member's write is on a repository an organisation unit owns, or adds one under a unit: such a call asks
/// no console section, since heading the unit is the whole of who may, and the chain's checks after it judge that
/// (docs/api.md, 「Who a caller acts for」; docs/repositories.md, 「An organisation's repository」). A change names
/// the row, read for its unit; an add names the unit in its body, read ahead of the route and put back.
pub async fn unit_repository(call: &mut Call, method: &Method, path: &str) -> bool {
  if Part::of(path) != Some(Part::Repositories) || matches!(method, &Method::GET | &Method::HEAD) {
    return false;
  }

  if let Some(name) = call.param("repositoryName").map(str::to_owned) {
    return matches!(
      crate::repositories::named(&call.ground().db, &name).await,
      Ok(Some(held)) if held.owner_unit.is_some()
    );
  }

  if method != Method::POST || path != Part::Repositories.root() {
    return false;
  }

  call
    .peek_body()
    .await
    .is_some_and(|body| body.get("ownerUnit").and_then(Value::as_i64).is_some())
}

/// Whose the row a managed route changes is (「Part::row」).
enum Row {
  /// A row of the kind named, by its name, filed under an account or, with none, the platform's.
  Owned(&'static str, String, Option<i64>),
  /// A row no account owns: a role, the platform's preset.
  Platform,
  /// No row the store holds, or none the route names.
  Unnamed,
  /// A row whose owner its own route judges: a registered model's or a model file's.
  Unjudged,
}

/// The account a role agent's seat is filed under, its text read as a user id; a text that reads as none is no
/// member's account.
pub fn seat_owner(text: &str) -> i64 {
  text.trim().parse().unwrap_or(NO_MEMBER)
}

/// An account id no member holds, standing for an owner that does not read as one.
const NO_MEMBER: i64 = i64::MIN;

/// A member's change of a managed route, whose row is judged before the route answers.
pub struct Owning {
  acting: Acting,
  part: Part,
  name: Option<String>,
  ground: std::sync::Arc<Ground>,
}

/// The ownership a call's change needs judged (docs/api.md, 「Who a caller acts for」), none for a read, a route of
/// another kind or the platform's own call; or the refusal of a member's key that names no user id.
pub fn owning(call: &Call, method: &Method, path: &str) -> Result<Option<Owning>, Refused> {
  let Some(part) = Part::of(path) else {
    return Ok(None);
  };

  if matches!(method, &Method::GET | &Method::HEAD) {
    return Ok(None);
  }

  let acting = Acting::of(call)?;

  if acting == Acting::Platform {
    return Ok(None);
  }

  Ok(Some(Owning {
    acting,
    part,
    name: part.row_param().and_then(|param| call.param(param)).map(str::to_owned),
    ground: call.shared_ground(),
  }))
}

/// Whether a member's change reaches a row they may change: a row of another account or of the platform is
/// refused, and so is every change of a role; a route naming no row the store holds goes on to answer.
pub async fn judge(owning: Owning) -> Result<(), Refused> {
  if owning.part == Part::Repositories {
    crate::organisation::repositories::may_manage(&owning.ground, owning.acting).await?;

    if let Some(name) = &owning.name
      && let Some(held) = crate::repositories::named(&owning.ground.db, name).await?
      && let Some(unit) = held.owner_unit
    {
      return crate::organisation::repositories::heads(&owning.ground, owning.acting, unit, name).await;
    }
  }

  match owning.part.row(&owning.ground, owning.name).await? {
    Row::Owned(kind, name, owner) => owning.acting.may_change(owner, &format!("the {kind} {name}")),
    Row::Platform => Err(forbidden(
      "The signed-in person may not change a role: the roles are the platform's presets, the administrator's alone"
        .to_owned(),
    )),
    Row::Unnamed | Row::Unjudged => Ok(()),
  }
}

/// A member's refusal, saying what was asked and why.
fn forbidden(error: String) -> Refused {
  Refused::new(StatusCode::FORBIDDEN, error).with_code("forbidden")
}

/// Whether work held by an account may stand on a repository (docs/api.md, 「assignments and jobs」): a repository
/// filed under an account takes the work of that account alone, and one filed under none, the platform's, the work
/// of the platform's side alone — a holder of no account or of the administrator's own. Refused 403
/// `foreign-repository` naming the repository otherwise; a repository the store does not hold is its route's to
/// answer.
pub async fn works_on(ground: &Ground, account: Option<i64>, repository: &str) -> Result<(), Refused> {
  let Some(held) = crate::repositories::named(&ground.db, repository).await? else {
    return Ok(());
  };

  if let Some(unit) = held.owner_unit {
    return crate::organisation::repositories::works_on_unit(ground, account, unit, repository).await;
  }

  if same_side(&ground.db, account, held.owner_account).await? {
    return Ok(());
  }

  let whose = match held.owner_account {
    Some(_) => "another account's",
    None => "the platform's",
  };
  let holder = match account {
    Some(account) => format!("account {account}'s"),
    None => "the platform's".to_owned(),
  };

  Err(
    Refused::new(
      StatusCode::FORBIDDEN,
      format!("The repository {repository} is {whose}: {holder} work does not stand on it"),
    )
    .with_code("foreign-repository"),
  )
}

/// Whether a holder's account and a repository's owner stand on one side: the same account, or both the
/// platform's, where the administrator's own account stands with the platform.
async fn same_side(
  db: &sea_orm::DatabaseConnection,
  account: Option<i64>,
  owner: Option<i64>,
) -> Result<bool, Refused> {
  if account == owner {
    return Ok(true);
  }

  let platform = |held: Option<i64>| async move {
    match held {
      None => Ok::<_, Refused>(true),
      Some(id) => Ok(crate::account_isolation::owners_own(db, &id.to_string()).await?),
    }
  };

  Ok(platform(account).await? && platform(owner).await?)
}

/// Whom a call on a person's own supply acts for (docs/api.md, 「Who a caller acts for」): a person the console
/// names as a member, on their own accounts and keys alone, or the platform — the administrator, or the
/// deployment's own caller carrying no key — on the platform's accounts and of each person's supply only the
/// roads it serves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acting {
  Person(i64),
  Platform,
}

impl Acting {
  /// Whom the call acts for, or the refusal of a member's key that names no user id.
  pub fn of(call: &Call) -> Result<Self, Refused> {
    let Some(key) = call.header("owner-key").map(|key| key.trim().to_owned()).filter(|key| !key.is_empty())
    else {
      return Ok(Self::Platform);
    };

    if call
      .header("owner-role")
      .is_some_and(|role| role.trim().eq_ignore_ascii_case("administrator"))
    {
      return Ok(Self::Platform);
    }

    key.parse::<i64>().map(Self::Person).map_err(|_| {
      Refused::new(StatusCode::FORBIDDEN, "The signed-in person is named by no user id agency reads")
        .with_code("forbidden")
    })
  }

  /// Whether the caller reaches the rows filed under an account: the platform every account's, a person their
  /// own alone. The one place a person's reach over the rows of others would be granted.
  fn reaches(self, account: i64) -> bool {
    match self {
      Self::Platform => true,
      Self::Person(user) => user == account,
    }
  }

  /// Whether the caller may change a row filed under `owner`, none being the platform's, or the refusal naming
  /// `what` it asked: the platform changes every row, a person the rows of the accounts they reach alone.
  pub fn may_change(self, owner: Option<i64>, what: &str) -> Result<(), Refused> {
    match (self, owner) {
      (Self::Platform, _) => Ok(()),
      (_, Some(owner)) if self.reaches(owner) => Ok(()),
      (_, Some(_)) => Err(forbidden(format!(
        "The signed-in person may not change {what}: it is filed under another account"
      ))),
      (_, None) => Err(forbidden(format!(
        "The signed-in person may not change {what}: it is the platform's, the administrator's alone"
      ))),
    }
  }

  /// The account a row the caller adds is filed under, given the one its body names, none where it names none:
  /// the platform files it where the body says; a person files it under their own account, and a body naming an
  /// account they do not reach is refused.
  pub fn files_under(self, named: Option<i64>, what: &str) -> Result<Option<i64>, Refused> {
    match (self, named) {
      (Self::Platform, named) => Ok(named),
      (Self::Person(_), Some(named)) if self.reaches(named) => Ok(Some(named)),
      (Self::Person(_), Some(_)) => Err(forbidden(format!(
        "The signed-in person may not file {what} under another account: what they add is filed under their own"
      ))),
      (Self::Person(user), None) => Ok(Some(user)),
    }
  }

  /// The person the call acts for, none for the platform.
  pub fn person(self) -> Option<i64> {
    match self {
      Self::Person(user) => Some(user),
      Self::Platform => None,
    }
  }
}

/// Whether the caller may make this call, or the refusal saying why it may not.
pub async fn check(check: Check) -> Result<(), Refused> {
  if access_of(&check).await? {
    return Ok(());
  }

  let what = if check.write { "change" } else { "read" };
  let access = if check.write { "write" } else { "read" };
  let sections = match check.sections() {
    [one] => format!("the {} section", one.section()),
    _ => "any section of agent management".to_owned(),
  };

  Err(Refused::new(
    StatusCode::FORBIDDEN,
    format!(
      "The signed-in person may not {what} {}: the console holds no {access} access to {sections} for them",
      check.part.root()
    ),
  ))
}

impl Check {
  /// The parts any one of which lets the call go on: a change needs its own part's write, a read the read of a
  /// part it stands under.
  fn sections(&self) -> &'static [Part] {
    if self.write { self.part.alone() } else { self.part.read_under() }
  }
}

/// Whether the account service says the person may use a section the call stands under for what it asks: the
/// write for a change, the read for a read.
async fn access_of(check: &Check) -> Result<bool, Refused> {
  let accounts = AccountService::read(&check.data_root, |name| std::env::var(name).ok());

  if accounts.address.is_empty() {
    return Err(unchecked("the account service is not named"));
  }

  let asked = reqwest::Client::new()
    .get(format!("{}/users/{}/sections", accounts.address, check.key))
    .bearer_auth(&accounts.token)
    .header("Owner-Key", &check.key)
    .header("Owner-Role", "member")
    .timeout(WAIT)
    .send()
    .await
    .map_err(|why| unchecked(&format!("the account service was not reached: {why}")))?;
  let status = asked.status();
  let read = asked.json::<Value>().await.unwrap_or(Value::Null);

  if status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED {
    return Ok(false);
  }

  if !status.is_success() {
    let said = read.get("error").and_then(Value::as_str).unwrap_or("no reason given");

    return Err(unchecked(&format!("the account service answered {}: {said}", status.as_u16())));
  }

  Ok(sections_allow(&read, check.sections(), check.write))
}

/// Whether a section of the answer among the parts named reads and, where a change asks, writes.
fn sections_allow(body: &Value, parts: &[Part], write: bool) -> bool {
  let access = if write { "write" } else { "read" };

  body.get("sections").and_then(Value::as_array).is_some_and(|rows| {
    rows.iter().any(|row| {
      let named = row.get("section").and_then(Value::as_str);

      parts.iter().any(|part| named == Some(part.section()))
        && row.get(access).and_then(Value::as_bool).unwrap_or(false)
    })
  })
}

/// The refusal of a check that could not be made: no access is guessed.
fn unchecked(why: &str) -> Refused {
  Refused::new(
    StatusCode::SERVICE_UNAVAILABLE,
    format!("Agent management's access could not be checked: {why}"),
  )
  .with_code("access-unchecked")
}
