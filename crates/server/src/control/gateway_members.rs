//! The gateway's members as the owner reads them and sets their owners (docs/api.md, 「providers, models and
//! model files」; docs/own-llm.md, 「The gateway's members」): each member of the members file with its road,
//! label, provider and the account it belongs to, and that account set or taken away in the file, every
//! other field and member left as it stands. Both are the administrator's and the deployment's own.

use std::path::Path;
use std::sync::Mutex;

use axum::http::{Method, StatusCode};
use axum::response::Response;
use harness_machine::members::{self, Member};
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use serde_json::{Value, json};

use super::registry::checked_account;
use super::{Call, Refused, Route, answer};
use crate::access::Acting;

// One owner's change of the members file at a time, so two never write over each other's.
static WRITING: Mutex<()> = Mutex::new(());

// The refusal of a member's call: who owns a member is the owner's to say.
fn owner_only(call: &Call) -> Result<(), Refused> {
  match Acting::of(call)? {
    Acting::Platform => Ok(()),
    Acting::Person(_) => Err(
      Refused::new(StatusCode::FORBIDDEN, CONTROL_WORDS.registry.owner_only.as_str())
        .with_code("forbidden"),
    ),
  }
}

fn unread(why: impl std::fmt::Display) -> Refused {
  Refused::new(
    StatusCode::SERVICE_UNAVAILABLE,
    fill_marks(&CONTROL_WORDS.registry.members_unread, &[("why", &why.to_string())]),
  )
  .with_code("members-unread")
}

fn unwritten(path: &Path, why: std::io::Error) -> Refused {
  Refused::new(StatusCode::INTERNAL_SERVER_ERROR, format!("{}: {why}", path.display()))
    .with_code("members-unwritten")
}

fn unknown(name: &str) -> Refused {
  Refused::new(
    StatusCode::NOT_FOUND,
    fill_marks(&CONTROL_WORDS.registry.unknown_member, &[("name", name)]),
  )
  .with_code("unknown-member")
}

// One member as the routes answer it; its key file is never named.
fn item(member: &Member) -> Value {
  json!({
    "name": member.name,
    "road": member.road,
    "label": member.label,
    "provider": member.provider,
    "account": member.account,
  })
}

fn read(path: &Path) -> Result<Vec<Member>, Refused> {
  members::read(path).map_err(unread)
}

// The file with one member's owner written into it, every other field and member as it stands, published
// whole by a rename at the mode the file held; whether the file held the member. A file that does not read as
// members is refused before anything is written.
fn rewritten(path: &Path, name: &str, account: Option<i64>) -> Result<bool, Refused> {
  let _writing = WRITING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

  if !read(path)?.iter().any(|member| member.name == name) {
    return Ok(false);
  }

  let text = std::fs::read_to_string(path).map_err(unread)?;
  let mode = std::fs::metadata(path).map_err(unread)?.permissions();
  let mut held: Value = serde_json::from_str(&text).map_err(unread)?;
  let Some(member) = held
    .as_array_mut()
    .and_then(|all| all.iter_mut().find(|member| member.get("name").and_then(Value::as_str) == Some(name)))
    .and_then(Value::as_object_mut)
  else {
    return Ok(false);
  };

  match account {
    Some(account) => member.insert("account".to_owned(), account.into()),
    None => member.remove("account"),
  };

  let written = serde_json::to_string_pretty(&held).expect("a file read is written") + "\n";
  let mut staged = path.as_os_str().to_owned();

  staged.push(format!(".{}.new", agent_host::disk::staged_suffix()));

  let staged = Path::new(&staged);

  std::fs::write(staged, written)
    .and_then(|()| std::fs::set_permissions(staged, mode))
    .and_then(|()| std::fs::rename(staged, path))
    .map_err(|why| unwritten(path, why))?;

  Ok(true)
}

/// `GET /gateway-members`: every member of the members file, each with its road, label, provider and owner.
pub struct ListGatewayMembers;

impl Route for ListGatewayMembers {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/gateway-members";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    owner_only(&call)?;

    let items: Vec<Value> = read(&members::path(&call.data_root()))?.iter().map(item).collect();

    Ok(answer(StatusCode::OK, json!({ "items": items })))
  }
}

crate::moved!(ListGatewayMembers);

/// `PATCH /gateway-members/{memberName}`: a member's owner set to a user's id, or taken away with null.
pub struct ChangeGatewayMember;

impl Route for ChangeGatewayMember {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/gateway-members/{memberName}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    owner_only(&call)?;

    let name = call.param("memberName").unwrap_or_default().to_owned();
    let body = call.body().await?;
    let Some(Ok(account)) = body.get("account").map(checked_account) else {
      return Err(Refused::failed(&CONTROL_WORDS.registry.account).with_field("account"));
    };
    let path = members::path(&call.data_root());

    if !rewritten(&path, &name, account)? {
      return Err(unknown(&name));
    }

    let member = read(&path)?
      .into_iter()
      .find(|member| member.name == name)
      .ok_or_else(|| unknown(&name))?;

    Ok(answer(StatusCode::OK, item(&member)))
  }
}

crate::moved!(ChangeGatewayMember);
