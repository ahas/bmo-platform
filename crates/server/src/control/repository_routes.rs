//! The repositories' routes (docs/api.md, 「repositories, prefix-checks and room-holdings」;
//! docs/repositories.md): every repository read with its checkouts, one added, changed — its fields, its
//! checkouts, its node, its pin and its sampling — its bench readied, and removed while no job holds a
//! checkout of it.

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use bmo_agency_client::SupportCount;
use policy::roles::ROLES;
use policy::workers::WORKERS;
use serde_json::{Value, json};
use store::{all, run};

use super::claude_accounts::Judging;
use super::{Call, Refused, Route, agent_sampling, answer};
use crate::access::Acting;
use crate::now_ms;
use crate::places::Ground;
use crate::queue::settings;
use crate::queue::workers::{self, Worker, WorkerRemoved};
use crate::repositories::{self, Repository};

fn bad(field: &str, error: impl Into<String>) -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, error).with_field(field)
}

fn not_found(name: &str) -> Refused {
  Refused::new(StatusCode::NOT_FOUND, format!("There is no repository {name}."))
}

/// The job holding a checkout now, where one does: its id, its item and the role it runs as.
async fn holder(ground: &Ground, checkout: &str) -> Result<Option<Value>, Refused> {
  let rows = all(
    &ground.db,
    r#"SELECT "id", "item_id", "role" FROM "jobs" WHERE "worker_name" = $1 AND "state" IN ('starting', 'running', 'reviewing')
       ORDER BY "id" DESC LIMIT 1"#,
    vec![checkout.into()],
  )
  .await?;

  Ok(rows.first().map(|row| {
    json!({
      "jobId": row.try_get::<i64>("", "id").ok(),
      "itemId": row.try_get::<i64>("", "item_id").ok(),
      "role": row.try_get::<Option<String>>("", "role").ok().flatten(),
    })
  }))
}

/// One repository as the routes answer it: every field of its row, its checkouts with whether a job holds
/// each, and its sampling.
pub async fn view(ground: &Ground, repository: &Repository) -> Result<Value, Refused> {
  let db = &ground.db;
  let mut checkouts = vec![];

  for worker in workers::workers(db, &ground.source).await? {
    if worker.name != repository.name && worker.support_of.as_deref() != Some(repository.name.as_str()) {
      continue;
    }

    let kind = match (&worker.support_of, worker.support_slot) {
      (None, _) => "base",
      (Some(_), Some(_)) => "support",
      (Some(_), None) => "node",
    };

    checkouts.push(json!({
      "name": worker.name,
      "path": worker.cwd,
      "nodeName": worker.node,
      "kind": kind,
      "job": holder(ground, &worker.name).await?,
    }));
  }

  let sampling_view = agent_sampling::view(db, &repository.name).await.ok();
  let sampling = sampling_view.as_ref().map(|view| view["values"].clone());
  let sampling_settings =
    sampling_view.as_ref().map(|view| view["settings"].clone()).unwrap_or_else(|| json!([]));

  Ok(json!({
    "name": repository.name,
    "label": repository.label,
    "icon": ROLES.icon("repository"),
    "ownerAccount": repository.owner_account,
    "ownerUnit": repository.owner_unit,
    "path": repository.path,
    "nodeName": repository.node,
    "supportCount": repository.support_count,
    "prefixDocuments": repository.prefix_documents,
    "preamble": repository.preamble,
    "rules": repository.rules,
    "devCommand": repository.dev_command,
    "pin": repository.pin.as_ref().map(|pin| json!({ "provider": pin.provider, "model": pin.model })),
    "umbrella": repository.umbrella,
    "downgradeHeal": repository.downgrade_heal,
    "sampling": sampling,
    "samplingSettings": sampling_settings,
    "checkouts": checkouts,
  }))
}

/// One checkout on a work node a change names: the node, and the path where it names one.
#[derive(Clone, Debug, PartialEq, Eq)]
struct NodeCheckout {
  node: String,
  path: Option<String>,
}

// The checkouts on the nodes a body names, the list whole, or none where it names none.
fn node_checkouts_of(body: &Value) -> Result<Option<Vec<NodeCheckout>>, Refused> {
  let field = "nodeCheckouts";
  let Some(listed) = body.get(field) else { return Ok(None) };
  let shape = || bad(field, "Node checkouts are a list, each a nodeName and a path or null.");
  let mut read: Vec<NodeCheckout> = vec![];

  for one in listed.as_array().ok_or_else(shape)? {
    let node = one
      .get("nodeName")
      .and_then(Value::as_str)
      .map(str::trim)
      .filter(|node| !node.is_empty());
    let path = match one.get("path") {
      None | Some(Value::Null) => None,
      Some(Value::String(path)) if path.trim().is_empty() => None,
      Some(Value::String(path)) if path.starts_with('/') || path.starts_with("~/") => {
        Some(path.trim().to_owned())
      }
      Some(_) => {
        return Err(bad(field, "A node checkout's path is written from the node's home or absolute."));
      }
    };
    let node = node.ok_or_else(shape)?.to_owned();

    if read.iter().any(|held| held.node == node) {
      return Err(bad(field, format!("A repository keeps one checkout on {node}.")));
    }

    read.push(NodeCheckout { node, path });
  }

  Ok(Some(read))
}

// The node's home as its setup read it, where it has read it.
async fn node_home(ground: &Ground, node: &str) -> Option<String> {
  crate::verbs::work_node::workplace(&ground.db, node)
    .await
    .ok()
    .flatten()
    .and_then(|place| place.home)
}

/// The checkouts on the nodes a change writes, judged: the nodes it takes out and the node and path of each it
/// keeps or adds.
struct NodeCheckoutsJudged {
  removed: Vec<String>,
  written: Vec<(String, String)>,
}

/// A repository's checkouts on the nodes judged against the list a change names (docs/worker-slots.md,
/// 「Checkouts on the nodes」): every node it adds judged to take jobs and the base standing on this machine,
/// every one it leaves out held by no job. Nothing is written here.
async fn judge_node_checkouts(
  ground: &Ground,
  repository: &Repository,
  base_node: Option<&str>,
  asked: &[NodeCheckout],
) -> Result<NodeCheckoutsJudged, Refused> {
  let field = "nodeCheckouts";
  let held = all(
    &ground.db,
    r#"SELECT "node", "path" FROM "repository_node_checkouts" WHERE "repository" = $1"#,
    vec![repository.name.clone().into()],
  )
  .await?;
  let held: Vec<(String, String)> = held
    .iter()
    .map(|row| Ok((row.try_get("", "node")?, row.try_get("", "path")?)))
    .collect::<Result<_, sea_orm::DbErr>>()?;

  if !asked.is_empty()
    && let Some(node) = base_node
  {
    return Err(bad(
      field,
      format!("{} stands on {node}, so it keeps no checkout on a node.", repository.name),
    ));
  }

  let mut written = vec![];

  for one in asked {
    let kept = held.iter().find(|(node, _)| *node == one.node);

    if kept.is_none()
      && let Some(why) = super::node_reads::placing_refusal(ground, &one.node).await?
    {
      return Err(bad(field, why));
    }

    let path = match (&one.path, kept) {
      (Some(path), _) => path.clone(),
      (None, Some((_, path))) => path.clone(),
      (None, None) => {
        crate::verbs::agents::node_checkout_path(&repository.path, &one.node, &crate::verbs::home())
      }
    };
    let home = node_home(ground, &one.node).await;

    written.push((one.node.clone(), crate::watches::links::node_path(&path, home.as_deref())));
  }

  let removed: Vec<String> = held
    .into_iter()
    .map(|(node, _)| node)
    .filter(|node| !asked.iter().any(|one| one.node == *node))
    .collect();

  for node in &removed {
    let name = workers::node_checkout_name(&repository.name, node);

    if holder(ground, &name).await?.is_some() {
      return Err(
        Refused::new(
          StatusCode::CONFLICT,
          format!("A job holds {name}, a checkout of {}.", repository.name),
        )
        .with_code("conflict"),
      );
    }
  }

  Ok(NodeCheckoutsJudged { removed, written })
}

/// A repository's checkouts on the nodes written as they were judged.
async fn write_node_checkouts(
  ground: &Ground,
  repository: &Repository,
  judged: NodeCheckoutsJudged,
) -> Result<(), Refused> {
  let NodeCheckoutsJudged { removed, written } = judged;

  for node in removed {
    run(
      &ground.db,
      r#"DELETE FROM "repository_node_checkouts" WHERE "repository" = $1 AND "node" = $2"#,
      vec![repository.name.clone().into(), node.clone().into()],
    )
    .await?;
  }

  for (node, path) in written {
    run(
      &ground.db,
      r#"INSERT INTO "repository_node_checkouts" ("repository", "node", "path", "created_at")
         VALUES ($1, $2, $3, $4)
         ON CONFLICT ("repository", "node") DO UPDATE SET "path" = EXCLUDED."path""#,
      vec![repository.name.clone().into(), node.into(), path.into(), now_ms().into()],
    )
    .await?;
  }

  Ok(())
}

/// `GET /repositories`: every repository.
pub struct RepositoryList;

impl Route for RepositoryList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/repositories";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let mut items = vec![];

    for repository in repositories::every(call.store()).await? {
      items.push(view(call.ground(), &repository).await?);
    }

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(RepositoryList);

/// `GET /repositories/{repositoryName}`: one repository.
pub struct RepositoryRead;

impl Route for RepositoryRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/repositories/{repositoryName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("repositoryName").unwrap_or_default().to_owned();
    let repository = repositories::named(call.store(), &name).await?.ok_or_else(|| not_found(&name))?;

    Ok(answer(StatusCode::OK, view(call.ground(), &repository).await?))
  }
}

crate::moved!(RepositoryRead);

/// The fields a body names, each judged.
#[derive(Default)]
struct Fields {
  label: Option<Option<String>>,
  path: Option<String>,
  node: Option<Option<String>>,
  support_count: Option<SupportCount>,
  prefix_documents: Option<Vec<String>>,
  preamble: Option<Option<String>>,
  rules: Option<Option<String>>,
  dev_command: Option<Option<String>>,
  pin: Option<Option<(String, String)>>,
  umbrella: Option<bool>,
  downgrade_heal: Option<bool>,
}

// Why an umbrella would hold checkouts beside its one, or nothing: it takes no support checkouts and no node
// checkouts, so its one checkout's lock runs one job at a time (docs/worker-slots.md, 「An umbrella repository」).
fn umbrella_refusal(umbrella: bool, support: SupportCount, node_checkouts: bool) -> Result<(), Refused> {
  if umbrella && (support.stored() != Some(0) || node_checkouts) {
    return Err(bad(
      "umbrella",
      "An umbrella takes no support checkouts and no node checkouts: its support count is 0.",
    ));
  }

  Ok(())
}

fn optional_text(body: &Value, field: &str) -> Result<Option<Option<String>>, Refused> {
  match body.get(field) {
    None => Ok(None),
    Some(Value::Null) => Ok(Some(None)),
    Some(Value::String(text)) if text.trim().is_empty() => Ok(Some(None)),
    Some(Value::String(text)) => Ok(Some(Some(text.clone()))),
    Some(_) => Err(bad(field, format!("{field} is text, or null for none."))),
  }
}

impl Fields {
  async fn of(ground: &Ground, body: &Value) -> Result<Self, Refused> {
    let mut fields = Self {
      label: optional_text(body, "label")?,
      node: optional_text(body, "nodeName")?,
      preamble: optional_text(body, "preamble")?,
      rules: optional_text(body, "rules")?,
      dev_command: optional_text(body, "devCommand")?,
      ..Self::default()
    };

    if let Some(path) = body.get("path") {
      match path.as_str().map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => fields.path = Some(path.to_owned()),
        None => return Err(bad("path", "A repository's path is its base checkout's directory.")),
      }
    }

    if let Some(count) = body.get("supportCount") {
      let most = WORKERS.support_most;
      let judged = match serde_json::from_value::<SupportCount>(count.clone()) {
        Ok(SupportCount::Count(count)) => (0..=most).contains(&count).then_some(SupportCount::Count(count)),
        Ok(grown) => Some(grown),
        Err(_) => None,
      };

      match judged {
        Some(count) => fields.support_count = Some(count),
        None => {
          return Err(bad(
            "supportCount",
            format!("A support count is \"auto\" or a whole number from 0 to {most}."),
          ));
        }
      }
    }

    if let Some(documents) = body.get("prefixDocuments") {
      let listed: Option<Vec<String>> = documents
        .as_array()
        .and_then(|list| list.iter().map(|one| one.as_str().map(str::to_owned)).collect());
      let Some(listed) = listed.filter(|listed| {
        listed
          .iter()
          .all(|path| !path.trim().is_empty() && !path.split('/').any(|part| part == ".."))
      }) else {
        return Err(bad(
          "prefixDocuments",
          "Prefix documents are paths, none empty and none climbing with ..",
        ));
      };

      fields.prefix_documents = Some(listed);
    }

    if let Some(pin) = body.get("pin") {
      fields.pin = Some(match pin {
        Value::Null => None,
        Value::Object(_) => {
          let judged = crate::order::judge_one(
            Judging::of(ground),
            &ground.machine(),
            crate::queue::routes::job_line(),
            pin,
          )
          .await
          .map_err(|why| bad("pin", why))?;

          Some((judged.provider, judged.model))
        }
        _ => return Err(bad("pin", "A pin is a provider and a model, or null for none.")),
      });
    }

    if let Some(umbrella) = body.get("umbrella") {
      fields.umbrella = Some(
        umbrella
          .as_bool()
          .ok_or_else(|| bad("umbrella", "A repository is an umbrella or is not."))?,
      );
    }

    if let Some(heal) = body.get("downgradeHeal") {
      fields.downgrade_heal =
        Some(heal.as_bool().ok_or_else(|| bad("downgradeHeal", "The downgrade heal is on or off."))?);
    }

    Ok(fields)
  }
}

/// `POST /repositories`: a repository added.
pub struct RepositoryCreate;

impl Route for RepositoryCreate {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/repositories";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let name = body.get("name").and_then(Value::as_str).map(str::trim).unwrap_or_default().to_owned();

    let shape = regex::Regex::new(&policy::workers::WORKERS.name_pattern).map_err(Refused::failed)?;

    if !shape.is_match(&name) {
      return Err(bad("name", "A repository's name is lowercase letters, digits and hyphens."));
    }

    if workers::taken_names(call.store()).await?.contains(&name) {
      return Err(
        Refused::new(StatusCode::CONFLICT, format!("The name {name} is taken.")).with_code("name-taken"),
      );
    }

    // A member's repository is filed under their own account (docs/api.md, 「Who a caller acts for」), or under a
    // unit they head where the body names one (docs/repositories.md, 「An organisation's repository」).
    let acting = Acting::of(&call)?;
    let unit = body.get("ownerUnit").and_then(Value::as_i64);
    let owner = match unit {
      Some(unit) => {
        crate::organisation::repositories::may_file_under_unit(call.ground(), acting, unit).await?;
        None
      }
      None => acting
        .files_under(body.get("ownerAccount").and_then(Value::as_i64), &format!("the repository {name}"))?,
    };
    let fields = Fields::of(call.ground(), &body).await?;
    let path = fields.path.clone().ok_or_else(|| bad("path", "A repository names its path."))?;
    let pin = fields.pin.clone().flatten();
    let umbrella = fields.umbrella.unwrap_or(false);
    // An umbrella naming no support count takes none (docs/api.md, 「repositories, prefix-checks and
    // room-holdings」).
    let support =
      fields
        .support_count
        .unwrap_or(if umbrella { SupportCount::Count(0) } else { SupportCount::AUTO });
    let now = now_ms();

    umbrella_refusal(umbrella, support, false)?;

    run(
      call.store(),
      r#"INSERT INTO "repositories" ("name", "label", "owner_account", "path", "node", "support_count",
           "prefix_documents", "preamble", "prompts", "dev_command", "pinned_provider", "pinned_model", "downgrade_heal",
           "created_at", "updated_at", "umbrella", "owner_unit")
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $14, $15, $16)"#,
      vec![
        name.clone().into(),
        fields.label.clone().flatten().into(),
        owner.into(),
        path.into(),
        fields.node.clone().flatten().into(),
        support.stored().into(),
        serde_json::to_string(&fields.prefix_documents.clone().unwrap_or_default()).map_err(Refused::failed)?.into(),
        fields.preamble.clone().flatten().into(),
        fields.rules.clone().flatten().into(),
        fields.dev_command.clone().flatten().into(),
        pin.as_ref().map(|(provider, _)| provider.clone()).into(),
        pin.as_ref().map(|(_, model)| model.clone()).into(),
        i64::from(fields.downgrade_heal.unwrap_or(true)).into(),
        now.into(),
        i64::from(umbrella).into(),
        unit.into(),
      ],
    )
    .await?;

    let repository = repositories::named(call.store(), &name).await?.ok_or_else(|| not_found(&name))?;

    Ok(answer(StatusCode::CREATED, view(call.ground(), &repository).await?))
  }
}

crate::moved!(RepositoryCreate);

/// `PATCH /repositories/{repositoryName}`: the fields the body names changed, its sampling saved.
pub struct RepositoryChange;

impl Route for RepositoryChange {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/repositories/{repositoryName}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let name = call.param("repositoryName").unwrap_or_default().to_owned();
    let body = call.body().await?;
    let held = repositories::named(call.store(), &name).await?.ok_or_else(|| not_found(&name))?;
    let mut fields = Fields::of(call.ground(), &body).await?;
    let node_checkouts = node_checkouts_of(&body)?;
    let pin = fields
      .pin
      .clone()
      .unwrap_or_else(|| held.pin.clone().map(|pin| (pin.provider, pin.model)));

    // A node other than the one it stands on places it there: a node that takes jobs, the path written as the
    // node reads it (docs/api.md, 「repositories, prefix-checks and room-holdings」).
    if let Some(Some(node)) = fields.node.clone()
      && held.node.as_deref() != Some(node.as_str())
    {
      if let Some(why) = super::node_reads::placing_refusal(call.ground(), &node).await? {
        return Err(bad("nodeName", why));
      }

      let path = fields.path.clone().unwrap_or(held.path.clone());
      let home = node_home(call.ground(), &node).await;

      fields.path = Some(crate::watches::links::node_path(&path, home.as_deref()));
    }

    // The whole change is judged before anything of it is written, so a refusal leaves the repository as it
    // stood (docs/api.md, 「repositories, prefix-checks and room-holdings」).
    let sampling = match body.get("sampling") {
      Some(sampling) => {
        let key = agent_sampling::key(&name);
        let kept = agent_sampling::held_values(settings::get(call.store(), &key).await?);
        let next = agent_sampling::changed(sampling, kept).map_err(|fault| match fault {
          super::fault::Fault::Field(_, error) => Refused::failed(error),
          super::fault::Fault::Failed(failed) => failed,
        })?;

        Some((key, next))
      }
      None => None,
    };
    let umbrella = fields.umbrella.unwrap_or(held.umbrella);
    let keeps_node_checkouts = match &node_checkouts {
      Some(asked) => !asked.is_empty(),
      None => store::one(
        call.store(),
        r#"SELECT 1 AS "kept" FROM "repository_node_checkouts" WHERE "repository" = $1 LIMIT 1"#,
        vec![name.clone().into()],
      )
      .await?
      .is_some(),
    };

    umbrella_refusal(umbrella, fields.support_count.unwrap_or(held.support_count), keeps_node_checkouts)?;

    let node_checkouts = match &node_checkouts {
      Some(asked) => {
        let base_node = fields.node.clone().unwrap_or(held.node.clone());

        Some(judge_node_checkouts(call.ground(), &held, base_node.as_deref(), asked).await?)
      }
      None => None,
    };

    if let Some(judged) = node_checkouts {
      write_node_checkouts(call.ground(), &held, judged).await?;
    }

    if let Some((key, next)) = sampling {
      settings::set(call.store(), &key, &Value::Object(next).to_string()).await?;
    }

    run(
      call.store(),
      r#"UPDATE "repositories" SET "label" = $2, "path" = $3, "node" = $4, "support_count" = $5,
           "prefix_documents" = $6, "preamble" = $7, "prompts" = $8, "dev_command" = $9, "pinned_provider" = $10,
           "pinned_model" = $11, "downgrade_heal" = $12, "updated_at" = $13, "umbrella" = $14
         WHERE "name" = $1"#,
      vec![
        name.clone().into(),
        fields.label.clone().unwrap_or(held.label.clone()).into(),
        fields.path.clone().unwrap_or(held.path.clone()).into(),
        fields.node.clone().unwrap_or(held.node.clone()).into(),
        fields.support_count.unwrap_or(held.support_count).stored().into(),
        serde_json::to_string(&fields.prefix_documents.clone().unwrap_or(held.prefix_documents.clone()))
          .map_err(Refused::failed)?
          .into(),
        fields.preamble.clone().unwrap_or(held.preamble.clone()).into(),
        fields.rules.clone().unwrap_or(held.rules.clone()).into(),
        fields.dev_command.clone().unwrap_or(held.dev_command.clone()).into(),
        pin.as_ref().map(|(provider, _)| provider.clone()).into(),
        pin.as_ref().map(|(_, model)| model.clone()).into(),
        i64::from(fields.downgrade_heal.unwrap_or(held.downgrade_heal)).into(),
        now_ms().into(),
        i64::from(umbrella).into(),
      ],
    )
    .await?;

    // The bench a job's runner readies as the job starts (docs/worker-bench.md, 「How it is kept」): the
    // workbench made, what a job before asked of the dev server taken back, and the dev pane brought up.
    if body.get("bench").and_then(Value::as_bool) == Some(true) {
      crate::places::dev::job_workbench(call.ground(), &name);
      crate::places::dev::dev_pane(call.ground(), &name).await;
    }

    let repository = repositories::named(call.store(), &name).await?.ok_or_else(|| not_found(&name))?;

    Ok(answer(StatusCode::OK, view(call.ground(), &repository).await?))
  }
}

crate::moved!(RepositoryChange);

/// The checkout of a repository a job holds now, its base or one of its support checkouts, where one does.
pub(super) async fn held_checkout(ground: &Ground, name: &str) -> Result<Option<String>, Refused> {
  let checkouts: Vec<Worker> = workers::workers(&ground.db, &ground.source)
    .await?
    .into_iter()
    .filter(|worker| worker.name == name || worker.support_of.as_deref() == Some(name))
    .collect();

  for checkout in checkouts {
    if holder(ground, &checkout.name).await?.is_some() {
      return Ok(Some(checkout.name));
    }
  }

  Ok(None)
}

/// A repository taken away with its node checkouts' rows, its support checkouts' going with its row, and its
/// removal published.
pub(super) async fn take_away(ground: &Ground, name: &str) -> Result<(), Refused> {
  run(
    &ground.db,
    r#"DELETE FROM "repository_node_checkouts" WHERE "repository" = $1"#,
    vec![name.into()],
  )
  .await?;
  run(&ground.db, r#"DELETE FROM "repositories" WHERE "name" = $1"#, vec![name.into()]).await?;
  ground.events.publish(WorkerRemoved {
    worker: name.to_owned(),
  });

  Ok(())
}

/// `DELETE /repositories/{repositoryName}`: a repository no job holds a checkout of, removed.
pub struct RepositoryDelete;

impl Route for RepositoryDelete {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/repositories/{repositoryName}";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let name = call.param("repositoryName").unwrap_or_default().to_owned();

    repositories::named(call.store(), &name).await?.ok_or_else(|| not_found(&name))?;

    if let Some(checkout) = held_checkout(call.ground(), &name).await? {
      return Err(
        Refused::new(StatusCode::CONFLICT, format!("A job holds {checkout}, a checkout of {name}."))
          .with_code("conflict"),
      );
    }

    take_away(call.ground(), &name).await?;

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::moved!(RepositoryDelete);
