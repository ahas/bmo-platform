//! The supervisors as their rows declare them (docs/supervisors.md, 「One runner」): the scope of each one's
//! session and the shape of its turns. Every reader that needs to know how a supervisor talks asks here, so
//! nothing chooses a path by a supervisor's name or by a kind of its own.

use sea_orm::{ConnectionTrait, QueryResult};

use crate::Refusal;
use store::{all, one};

/// Where a conversational agent's session stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
  /// One session across every room it answers, each turn carrying its room's block.
  Agent,
  /// One session in each room it answers.
  Room,
}

/// What a conversational agent's turn is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
  /// The chat documents as its system prompt, the window's round prompt and the re-asks after an answer.
  Window,
  /// The pieces role and its persona as its system prompt, its new words as its turn, and no re-ask.
  Pieces,
}

impl Scope {
  fn named(word: &str) -> Option<Self> {
    match word {
      "agent" => Some(Self::Agent),
      "room" => Some(Self::Room),
      _ => None,
    }
  }

  /// The word its row keeps it under.
  pub fn word(self) -> &'static str {
    match self {
      Self::Agent => "agent",
      Self::Room => "room",
    }
  }
}

impl Shape {
  fn named(word: &str) -> Option<Self> {
    match word {
      "window" => Some(Self::Window),
      "pieces" => Some(Self::Pieces),
      _ => None,
    }
  }

  /// The word its row keeps it under.
  pub fn word(self) -> &'static str {
    match self {
      Self::Window => "window",
      Self::Pieces => "pieces",
    }
  }
}

/// A supervisor's settings, as its row holds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
  pub scope: Scope,
  pub shape: Shape,
}

impl Settings {
  /// The settings a row holds, or none where its scope is none the runner carries.
  pub fn of(row: &QueryResult) -> Result<Option<Self>, Refusal> {
    let scope: Option<String> = row.try_get("", "session_scope")?;
    let shape: Option<String> = row.try_get("", "turn_shape")?;

    Ok(Self::read(scope.as_deref(), shape.as_deref()))
  }

  /// The settings a row's columns hold, as `of` reads them, for a reader holding the columns themselves.
  pub fn read(scope: Option<&str>, shape: Option<&str>) -> Option<Self> {
    let scope = scope.and_then(Scope::named)?;

    Some(Self {
      scope,
      shape: shape.and_then(Shape::named).unwrap_or(match scope {
        Scope::Agent => Shape::Window,
        Scope::Room => Shape::Pieces,
      }),
    })
  }

  /// The settings a supervisor registered through `POST /supervisors` is given.
  pub fn console_added() -> Self {
    Self {
      scope: Scope::Room,
      shape: Shape::Pieces,
    }
  }
}

/// The role a run under this name holds, where the name is a role's: a supervisor holds none, being no role.
pub async fn role_of(db: &impl ConnectionTrait, agent: &str) -> Result<Option<String>, Refusal> {
  Ok(crate::agent_roles::holds(db, agent).await?.then(|| agent.to_owned()))
}

/// A conversational agent's settings by its name, or none where it has no row or its row is no
/// conversational agent's.
pub async fn settings(db: &impl ConnectionTrait, agent: &str) -> Result<Option<Settings>, Refusal> {
  match one(
    db,
    r#"SELECT "session_scope", "turn_shape" FROM "supervisors" WHERE "name" = $1"#,
    vec![agent.into()],
  )
  .await?
  {
    Some(row) => Settings::of(&row),
    None => Ok(None),
  }
}

/// The prompt folder an agent's row names under `prompts/agents/`, which a supervisor's system prompt and
/// persona are read from, or none where its row names none or it has no row (`docs/store.md`). Every
/// path composing a supervisor's prompt hands this as its `prompts`, never the supervisor's name.
pub async fn prompt_folder(db: &impl ConnectionTrait, agent: &str) -> Result<Option<String>, Refusal> {
  match one(db, r#"SELECT "prompts" FROM "supervisors" WHERE "name" = $1"#, vec![agent.into()]).await? {
    Some(row) => Ok(row.try_get("", "prompts")?),
    None => Ok(None),
  }
}

/// The awake conversational agents whose settings pass a test, by name in order.
pub async fn agents_where(
  db: &impl ConnectionTrait,
  test: impl Fn(&Settings) -> bool,
) -> Result<Vec<String>, Refusal> {
  let rows = all(
    db,
    r#"SELECT "name", "session_scope", "turn_shape" FROM "supervisors" ORDER BY "name""#,
    vec![],
  )
  .await?;
  let mut named = Vec::new();

  for row in rows {
    if Settings::of(&row)?.is_some_and(|settings| test(&settings)) {
      named.push(row.try_get("", "name")?);
    }
  }

  Ok(named)
}
