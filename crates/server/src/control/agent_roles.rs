//! The roles as the owner lists, adds, changes and removes them (docs/agent-roles.md, 「Changing the roles」;
//! docs/api.md, 「roles」): each a row of the store with its own provider order, cap, sessions setting,
//! compaction threshold and bundles. A field is judged before anything is written.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use policy::control_words::CONTROL_WORDS;
use policy::fill_marks;
use policy::roles::{ROLES, Sessions};
use policy::run_lines::RUN_LINES;
use regex::Regex;
use sea_orm::ConnectionTrait;
use serde_json::{Value, json};

use super::{Call, Refused, Route, answer};
use crate::agent_roles::{self, AgentRole};
use crate::lines::{Trees, agent_plugin};
use crate::now_ms;
use crate::order;
use crate::order_run::dispatch;
use crate::places::Ground;
use crate::road::{self, Ask, Row};
use crate::text::length;
use store::{all, one, run};

static NAME: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(&ROLES.name_pattern).expect("the policy's role name pattern reads"));

// The lines a role may name: every line agency carries but the supervisors' and the gate's.
const NOT_A_ROLES_LINE: [&str; 2] = ["chat", "gate"];

fn unprocessable(error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::UNPROCESSABLE_ENTITY, error)
}

fn unknown(name: &str) -> Refused {
  Refused::new(StatusCode::NOT_FOUND, fill_marks(&CONTROL_WORDS.agent_roles.unknown, &[("name", name)]))
}

fn label_refusal() -> String {
  fill_marks(&CONTROL_WORDS.agent_roles.label, &[("most", &ROLES.label_most.to_string())])
}

fn bundles_refusal() -> String {
  let declared = ROLES.bundles.iter().map(|bundle| bundle.name.as_str()).collect::<Vec<_>>();

  fill_marks(&CONTROL_WORDS.agent_roles.bundles, &[("bundles", &declared.join(", "))])
}

/// Every line a role may name.
pub fn role_lines() -> Vec<String> {
  RUN_LINES
    .0
    .keys()
    .filter(|line| !NOT_A_ROLES_LINE.contains(&line.as_str()))
    .cloned()
    .collect()
}

fn line_refusal() -> String {
  fill_marks(&CONTROL_WORDS.agent_roles.line, &[("lines", &role_lines().join(", "))])
}

/// A bundle as a screen is handed it: its name, its kebab-case code and its label.
pub fn bundle_view(name: &str) -> Value {
  json!({ "name": name, "code": name.replace('.', "-"), "label": ROLES.bundle_label(name) })
}

/// What stands of a role now: how many of its runs go and wait, its active provider and why its runs wait.
async fn standing(ground: &Ground, role: &AgentRole) -> Result<Value, Refused> {
  let count = |state: &'static str| {
    let db = &ground.db;
    let name = role.name.clone();

    async move {
      one(
        db,
        r#"SELECT COUNT(*) AS "count" FROM "role_slot_requests" WHERE "role" = $1 AND "state" = $2"#,
        vec![name.into(), state.into()],
      )
      .await
      .map(|row| row.and_then(|row| row.try_get::<i64>("", "count").ok()).unwrap_or(0))
    }
  };
  let running = count("active").await?;
  let waiting = count("waiting").await?;

  // A role whose order is empty and on which no provider is forced has no road of its own: its runs go out
  // on the asking supervisor's.
  if role.order.follows_supervisor() && role.order.forced.is_none() {
    return Ok(json!({
      "running": running,
      "waiting": waiting,
      "provider": Value::Null,
      "waitingFor": Value::Null,
    }));
  }

  let resolution = road::resolve(ground, &Ask::of(Row::Role(&role.name), &role.line, &role.order))
    .await
    .map_err(Refused::failed)?;
  let provider = resolution.road().map(|road| road.provider.clone());
  let waiting_for = provider.is_none().then(|| resolution.sentence());

  Ok(json!({
    "running": running,
    "waiting": waiting,
    "provider": provider,
    "waitingFor": waiting_for,
  }))
}

/// One role as the routes answer it: every field of its row, and what stands of it now.
pub async fn view(ground: &Ground, role: &AgentRole) -> Result<Value, Refused> {
  let carrying = crate::role_agents::carrying(&ground.db, &role.name).await?;
  let agents: Vec<Value> = role
    .agents
    .iter()
    .map(|agent| super::role_agents::view(agent, role.follows_asker, &carrying))
    .collect();
  let mut shown = json!({
    "name": role.name,
    "code": role.name,
    "label": role.label,
    "prompt": role.prompt,
    "description": role.description,
    "bundles": role.bundles.iter().map(|bundle| bundle_view(bundle)).collect::<Vec<_>>(),
    "skills": role.skills,
    "line": role.line,
    "agents": agents,
    "maxConcurrent": role.max_concurrent,
    "followsSupervisor": role.order.follows_supervisor() || role.runs_follow(),
    "followsAsker": role.follows_asker,
    "providerPrompts": agent_roles::provider_prompts(&ground.db, &role.name).await?,
  });

  if let (Some(shown), Value::Object(now)) = (shown.as_object_mut(), standing(ground, role).await?) {
    shown.extend(now);
  }

  Ok(shown)
}

/// What a body asks of a role's fields: each one it names, judged. A field left out is `None`; a prompt or a
/// description given as null is `Some(None)`.
#[derive(Default)]
struct Fields {
  label: Option<String>,
  bundles: Option<Vec<String>>,
  /// The skills named, `Some(None)` where the body clears them.
  skills: Option<Option<Vec<String>>>,
  line: Option<String>,
  prompt: Option<Option<String>>,
  description: Option<Option<String>>,
  sessions: Option<Sessions>,
  fold_ratio: Option<f64>,
  /// The provider prompts named, each by its provider, `None` where the body clears that provider's.
  provider_prompts: BTreeMap<String, Option<String>>,
  /// Whether its agents follow the supervisor that asks a run of them.
  follows_asker: Option<bool>,
}

// What a role's row does not hold: a body naming one is told where it stands.
const AGENTS_FIELDS: [&str; 6] =
  ["order", "quotaShare", "maxConcurrent", "active", "sessions", "foldRatio"];

const PROVIDER_PROMPTS_REFUSAL: &str =
  "providerPrompts: An object naming providers, each with its prompt's text, or null to clear it.";

// The provider prompts a body names written: each text set under its provider, each null taken away.
async fn write_provider_prompts<C: ConnectionTrait>(
  db: &C,
  role: &str,
  named: &BTreeMap<String, Option<String>>,
) -> Result<(), Refused> {
  for (provider, text) in named {
    match text {
      Some(text) => {
        run(
          db,
          r#"INSERT INTO "role_provider_prompts" ("role", "provider", "prompt", "updated_at")
             VALUES ($1, $2, $3, $4)
             ON CONFLICT ("role", "provider") DO UPDATE SET "prompt" = excluded."prompt",
               "updated_at" = excluded."updated_at""#,
          vec![role.into(), provider.as_str().into(), text.as_str().into(), now_ms().into()],
        )
        .await?;
      }
      None => {
        run(
          db,
          r#"DELETE FROM "role_provider_prompts" WHERE "role" = $1 AND "provider" = $2"#,
          vec![role.into(), provider.as_str().into()],
        )
        .await?;
      }
    }
  }

  Ok(())
}

fn optional_text(value: &Value, refusal: &str) -> Result<Option<String>, Refused> {
  match value {
    Value::Null => Ok(None),
    Value::String(text) if text.trim().is_empty() => Ok(None),
    Value::String(text) => Ok(Some(text.clone())),
    _ => Err(unprocessable(refusal)),
  }
}

impl Fields {
  // The fields a body names, judged; `carried` names every skill a plugin of the release carries.
  fn of(body: &Value, carried: &BTreeSet<String>) -> Result<Self, Refused> {
    let words = &CONTROL_WORDS.agent_roles;
    let mut fields = Self::default();

    if let Some(label) = body.get("label") {
      match label.as_str().map(str::trim) {
        Some(label) if !label.is_empty() && length(label) <= ROLES.label_most => {
          fields.label = Some(label.to_owned());
        }
        _ => return Err(unprocessable(label_refusal())),
      }
    }

    // Whether its agents follow the supervisor that asks a run of them, each agent's own setting standing over
    // it (docs/role-agents.md, 「An agent that follows its asker」).
    if let Some(follows) = body.get("followsAsker") {
      fields.follows_asker =
        Some(follows.as_bool().ok_or_else(|| unprocessable("followsAsker: true or false."))?);
    }

    if let Some(bundles) = body.get("bundles") {
      let named: Option<Vec<&str>> =
        bundles.as_array().and_then(|list| list.iter().map(Value::as_str).collect());
      let Some(named) = named.filter(|named| named.iter().all(|bundle| ROLES.declares_bundle(bundle)))
      else {
        return Err(unprocessable(bundles_refusal()));
      };
      let mut given: Vec<String> = Vec::with_capacity(named.len());

      for bundle in named {
        if !given.iter().any(|one| one == bundle) {
          given.push(bundle.to_owned());
        }
      }

      fields.bundles = Some(given);
    }

    if let Some(skills) = body.get("skills") {
      fields.skills = Some(match skills {
        Value::Null => None,
        Value::Array(list) => {
          let mut named: Vec<String> = Vec::with_capacity(list.len());

          for one in list {
            let Some(name) = one.as_str().map(str::trim).filter(|name| !name.is_empty()) else {
              return Err(unprocessable(
                "skills: A role's skills are a list of skill names, or null for none set.",
              ));
            };

            if !carried.contains(name) {
              return Err(unprocessable(format!(
                "skills: {name} is no skill a plugin of this release carries."
              )));
            }

            if !named.iter().any(|held| held == name) {
              named.push(name.to_owned());
            }
          }

          Some(named)
        }
        _ => {
          return Err(unprocessable(
            "skills: A role's skills are a list of skill names, or null for none set.",
          ));
        }
      });
    }

    if let Some(line) = body.get("line") {
      match line.as_str().filter(|line| role_lines().iter().any(|one| one == line)) {
        Some(line) => fields.line = Some(line.to_owned()),
        None => return Err(unprocessable(line_refusal())),
      }
    }

    // A role holds no road, no cap, no sessions setting and no threshold: its agents do (docs/role-agents.md).
    if let Some(named) = AGENTS_FIELDS.iter().find(|field| body.get(**field).is_some()) {
      return Err(unprocessable(format!(
        "{named}: A role holds no order, cap, forced provider, sessions setting or compaction threshold. Each \
         is its agent's: add or change the agents at /role-agents."
      )));
    }

    if let Some(prompt) = body.get("prompt") {
      fields.prompt = Some(optional_text(prompt, &words.prompt)?);
    }

    if let Some(description) = body.get("description") {
      fields.description = Some(optional_text(description, &words.description)?);
    }

    // A provider prompt is named by a provider of this build, and is text or null (docs/agent-roles.md,
    // 「A role's provider prompts」).
    if let Some(named) = body.get("providerPrompts") {
      let Some(named) = named.as_object() else {
        return Err(unprocessable(PROVIDER_PROMPTS_REFUSAL));
      };

      for (provider, text) in named {
        if super::harness::named(provider).is_err() {
          return Err(unprocessable(format!("providerPrompts: {provider} is no provider of this build.")));
        }

        let text = match text {
          Value::Null => None,
          Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
          Value::String(_) => None,
          _ => return Err(unprocessable(PROVIDER_PROMPTS_REFUSAL)),
        };

        fields.provider_prompts.insert(provider.clone(), text);
      }
    }

    Ok(fields)
  }
}

// A list of names as its column keeps it: a JSON list, or none.
fn listed(names: Option<&[String]>) -> Result<Option<String>, Refused> {
  names.map(|names| serde_json::to_string(names).map_err(Refused::failed)).transpose()
}

/// How many runs of a role go or wait now.
async fn holding<C: ConnectionTrait>(db: &C, role: &str) -> Result<i64, Refused> {
  Ok(
    one(
      db,
      r#"SELECT COUNT(*) AS "count" FROM "role_slot_requests"
         WHERE "role" = $1 AND "state" IN ('waiting', 'reserved', 'active')"#,
      vec![role.into()],
    )
    .await?
    .map(|row| row.try_get::<i64>("", "count"))
    .transpose()?
    .unwrap_or(0),
  )
}

/// `GET /roles`: every role the store holds, every bundle a role may give and every line a role may name.
pub struct RoleList;

impl Route for RoleList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/roles";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let mut roles = vec![];

    for role in agent_roles::every(call.store()).await? {
      roles.push(view(call.ground(), &role).await?);
    }

    let bundles: Vec<Value> = ROLES.bundles.iter().map(|bundle| bundle_view(&bundle.name)).collect();

    let skills = agent_plugin::carried(&Trees::of(call.ground()));

    Ok(answer(
      StatusCode::OK,
      json!({ "roles": roles, "bundles": bundles, "skills": skills, "lines": role_lines() }),
    ))
  }
}

crate::moved!(RoleList);

/// `GET /roles/{roleName}`: one role, with how many of its runs go and wait.
pub struct RoleRead;

impl Route for RoleRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/roles/{roleName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("roleName").unwrap_or_default().to_owned();
    let role = agent_roles::named(call.store(), &name).await?.ok_or_else(|| unknown(&name))?;

    Ok(answer(StatusCode::OK, view(call.ground(), &role).await?))
  }
}

crate::moved!(RoleRead);

/// `POST /roles`: a role added.
pub struct RoleCreate;

impl Route for RoleCreate {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/roles";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let words = &CONTROL_WORDS.agent_roles;
    let name = body.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();

    if !NAME.is_match(&name) || length(&name) > ROLES.name_most {
      return Err(unprocessable(fill_marks(&words.name, &[("most", &ROLES.name_most.to_string())])));
    }

    if agent_roles::holds(call.store(), &name).await? {
      return Err(unprocessable(fill_marks(&words.taken, &[("name", &name)])));
    }

    let fields = Fields::of(&body, &agent_plugin::carried(&Trees::of(call.ground())))?;
    let label = fields.label.clone().ok_or_else(|| unprocessable(label_refusal()))?;
    let bundles = fields.bundles.clone().ok_or_else(|| unprocessable(bundles_refusal()))?;
    let line = fields.line.clone().ok_or_else(|| unprocessable(line_refusal()))?;
    let now = now_ms();

    run(
      call.store(),
      r#"INSERT INTO "roles" ("name", "label", "bundles", "max_concurrent", "prompt", "position", "created_at",
           "updated_at", "sessions", "description", "line", "provider_order", "fold_ratio", "skills",
           "follows_asker")
         VALUES ($1, $2, $3, $4, $5, (SELECT COALESCE(MAX("position"), -1) + 1 FROM "roles"), $6, $6, $7, $8, $9, $10,
           $11, $12, $13)"#,
      vec![
        name.clone().into(),
        label.into(),
        serde_json::to_string(&bundles).map_err(Refused::failed)?.into(),
        0_i64.into(),
        fields.prompt.flatten().into(),
        now.into(),
        fields.sessions.unwrap_or_default().word().into(),
        fields.description.flatten().into(),
        line.into(),
        "[]".into(),
        fields.fold_ratio.unwrap_or(order::DEFAULT_FOLD_RATIO).into(),
        listed(fields.skills.flatten().as_deref())?.into(),
        i64::from(fields.follows_asker.unwrap_or(false)).into(),
      ],
    )
    .await?;
    write_provider_prompts(call.store(), &name, &fields.provider_prompts).await?;

    let role = agent_roles::named(call.store(), &name).await?.ok_or_else(|| unknown(&name))?;

    Ok(answer(StatusCode::CREATED, view(call.ground(), &role).await?))
  }
}

crate::moved!(RoleCreate);

/// `PATCH /roles/{roleName}`: any field of a role changed; a raised cap lets in at once the reserved slots it
/// frees.
pub struct RoleChange;

impl Route for RoleChange {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/roles/{roleName}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let name = call.param("roleName").unwrap_or_default().to_owned();
    let body = call.body().await?;
    let Some(held) = agent_roles::named(call.store(), &name).await? else {
      return Err(unknown(&name));
    };
    let fields = Fields::of(&body, &agent_plugin::carried(&Trees::of(call.ground())))?;

    run(
      call.store(),
      r#"UPDATE "roles" SET "label" = $1, "bundles" = $2, "prompt" = $3, "updated_at" = $4, "sessions" = $6,
           "description" = $7, "line" = $8, "fold_ratio" = $9, "skills" = $10, "follows_asker" = $11
         WHERE "name" = $5"#,
      vec![
        fields.label.unwrap_or(held.label).into(),
        serde_json::to_string(&fields.bundles.unwrap_or(held.bundles))
          .map_err(Refused::failed)?
          .into(),
        fields.prompt.unwrap_or(held.prompt).into(),
        now_ms().into(),
        name.clone().into(),
        fields.sessions.unwrap_or(held.sessions).word().into(),
        fields.description.unwrap_or(held.description).into(),
        fields.line.unwrap_or(held.line).into(),
        fields.fold_ratio.unwrap_or(held.fold_ratio).into(),
        listed(fields.skills.unwrap_or(held.skills).as_deref())?.into(),
        i64::from(fields.follows_asker.unwrap_or(held.follows_asker)).into(),
      ],
    )
    .await?;
    write_provider_prompts(call.store(), &name, &fields.provider_prompts).await?;

    let promoted = dispatch::promote_role(call.store(), &name, now_ms()).await.map_err(Refused::failed)?;

    for request in &promoted {
      dispatch::announce(call.events(), request);
    }

    let role = agent_roles::named(call.store(), &name).await?.ok_or_else(|| unknown(&name))?;

    Ok(answer(StatusCode::OK, view(call.ground(), &role).await?))
  }
}

crate::moved!(RoleChange);

/// `DELETE /roles/{roleName}`: a role no run holds or waits for and no default names, removed.
pub struct RoleDelete;

impl Route for RoleDelete {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/roles/{roleName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let words = &CONTROL_WORDS.agent_roles;
    let name = call.param("roleName").unwrap_or_default().to_owned();

    if !agent_roles::holds(call.store(), &name).await? {
      return Err(unknown(&name));
    }

    if ROLES.default_role(&name) {
      return Err(Refused::new(StatusCode::CONFLICT, fill_marks(&words.default_role, &[("name", &name)])));
    }

    if holding(call.store(), &name).await? > 0 {
      return Err(Refused::new(StatusCode::CONFLICT, fill_marks(&words.held, &[("name", &name)])));
    }

    run(
      call.store(),
      r#"DELETE FROM "role_agents" WHERE "role" = $1"#,
      vec![name.clone().into()],
    )
    .await?;
    run(
      call.store(),
      r#"DELETE FROM "role_provider_prompts" WHERE "role" = $1"#,
      vec![name.clone().into()],
    )
    .await?;
    run(call.store(), r#"DELETE FROM "roles" WHERE "name" = $1"#, vec![name.into()]).await?;

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(RoleDelete);

// The most sessions a history page holds.
const HISTORY_PAGE: i64 = 50;

/// `GET /roles/{roleName}/sessions`: the sessions the role's runs opened, newest first (docs/sessions.md,
/// 「The session history of a role」).
pub struct RoleSessions;

impl Route for RoleSessions {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/roles/{roleName}/sessions";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("roleName").unwrap_or_default().to_owned();

    if !agent_roles::holds(call.store(), &name).await? {
      return Err(unknown(&name));
    }

    let before = call.query("before").and_then(|before| before.parse::<i64>().ok()).unwrap_or(i64::MAX);
    let limit = call
      .query("limit")
      .and_then(|limit| limit.parse::<i64>().ok())
      .filter(|limit| *limit > 0)
      .unwrap_or(HISTORY_PAGE);
    let rows = all(
      call.store(),
      r#"SELECT * FROM "role_sessions" WHERE "role" = $1 AND "id" < $2 ORDER BY "id" DESC LIMIT $3"#,
      vec![name.into(), before.into(), (limit + 1).into()],
    )
    .await?;
    let more = i64::try_from(rows.len()).unwrap_or(i64::MAX) > limit;
    let mut items = vec![];

    for row in rows.iter().take(usize::try_from(limit).unwrap_or(usize::MAX)) {
      let held = crate::apart_sessions::Kept::of(row).map_err(Refused::failed)?;

      items.push(json!({
        "id": held.id,
        "sessionId": held.session,
        "path": held.path,
        "provider": held.provider,
        "model": held.model,
        "effort": (!held.effort.is_empty()).then_some(held.effort.clone()),
        "account": held.account,
        "openedAt": held.opened_at,
        "lastAnsweredAt": held.answered_at,
        "runs": held.runs,
        "context": { "tokens": held.context_tokens, "share": held.context_share },
        "valid": crate::apart_sessions::valid(&held, now_ms()),
      }));
    }

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": more })))
  }
}

crate::moved!(RoleSessions);
