//! An item a supervisor files (docs/api.md, 「assignments and jobs」): raised in the job service under the room of
//! the turn the agent carries now, and marked as the owner's ask where that turn is privileged, so neither is the
//! model's to write; the repository it names goes where a launch reads it, the item's `assigneeName`.

use std::collections::BTreeMap;

use axum::http::{Method, StatusCode};
use axum::response::Response;
use bmo_agency_client::{ItemFiled, ItemFilingAsk};
use policy::fill_marks;
use policy::job_review::JOB_REVIEW;
use policy::roles::ROLES;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{Call, Refused, Route, answer};
use crate::job_end::{address, token};
use crate::lines::Trees;
use crate::places::Ground;
use crate::turn_marks;

/// The label an item the owner asked for himself carries, which the job service's unattended labels open.
pub const ASKED_BY: &str = "asked-by";
/// Its value.
pub const OWNER: &str = "owner";

/// A name the ask gives, trimmed; none where it is left out or blank.
fn given(name: Option<&String>) -> Option<&str> {
  name.map(|name| name.trim()).filter(|name| !name.is_empty())
}

/// Where the item goes: the repository the ask names, which a launch reads as the item's `assigneeName`, else
/// the assignee it names; none for host work. An ask naming an assignee other than its repository is refused.
pub fn assignee_of(ask: &ItemFilingAsk) -> Result<Option<&str>, String> {
  match (given(ask.repository.as_ref()), given(ask.assignee_name.as_ref())) {
    (Some(repository), Some(assignee)) if assignee != repository => {
      Err(format!("assigneeName {assignee} is not the repository {repository} the item names"))
    }
    (Some(repository), _) => Ok(Some(repository)),
    (None, assignee) => Ok(assignee),
  }
}

/// The item the job service is asked to raise: the ask as it came, its room the turn's, its `asked-by` label
/// agency's alone, and its `assigneeName` the repository it names.
pub fn item_of(ask: &ItemFilingAsk, mark: turn_marks::Mark) -> Result<Value, String> {
  let assignee = assignee_of(ask)?;
  let mut labels: BTreeMap<String, String> = ask
    .labels
    .iter()
    .filter(|(key, _)| key.as_str() != ASKED_BY)
    .map(|(k, v)| (k.clone(), v.clone()))
    .collect();

  if mark.privileged {
    labels.insert(ASKED_BY.to_owned(), OWNER.to_owned());
  }

  let mut item = json!({
    "title": ask.title,
    "brief": ask.brief,
    "queueName": ask.queue_name,
    "scope": mark.chat_id.to_string(),
    "labels": labels,
  });

  for (field, value) in [
    ("priority", ask.priority.map(Value::from)),
    ("assigneeName", assignee.map(Value::from)),
    ("requesterName", ask.requester_name.clone().map(Value::from)),
  ] {
    if let Some(value) = value {
      item[field] = value;
    }
  }

  Ok(item)
}

// The front matter of a skill's own file, of which its description is read.
#[derive(serde::Deserialize)]
struct SkillFront {
  description: Option<String>,
}

// A skill named with the first sentence its own file describes it by, where it says one.
fn skill_line(skills: &std::path::Path, name: &str) -> String {
  let described = std::fs::read_to_string(skills.join(name).join("SKILL.md")).ok().and_then(|text| {
    let text = text.replace("\r\n", "\n");
    let (front, _) = text.strip_prefix("---\n")?.split_once("\n---")?;

    serde_norway::from_str::<SkillFront>(front).ok()?.description
  });

  match described.map(|said| said.split_whitespace().collect::<Vec<_>>().join(" ")) {
    Some(said) if !said.is_empty() => format!("- {name}: {said}"),
    _ => format!("- {name}"),
  }
}

/// The agent an item is filed for, and what the steps check calls it by (docs/repositories.md, 「An agent an item
/// names」): the agent its `agent` label names, which stands over a `model` label; else, for the model its `model`
/// label names, the first by name of the agents of the role it would run as — its `role` label's, else the role a
/// job runs as — switched on, of the filing supervisor's account and running that model, called by the role, the
/// model and every such agent. A filing naming neither, or `auto`, is refused 409 `agent-unnamed` with the agents
/// of that role so the supervisor names one from what it has learned; an agent the agency does not register is
/// refused 422 `unknown-agent`, and a model no agent of the role runs 422 `no-candidate` with the role's agents.
pub async fn filed_agent(
  ground: &Ground,
  ask: &ItemFilingAsk,
) -> Result<(crate::role_agents::RoleAgent, String), Refused> {
  let label = given(ask.labels.get("agent")).filter(|name| *name != crate::repositories::AUTO);
  let model = given(ask.labels.get("model")).filter(|model| *model != crate::repositories::AUTO);
  let role = given(ask.labels.get("role"))
    .filter(|role| *role != crate::repositories::AUTO)
    .unwrap_or_else(|| crate::agent_roles::job_role())
    .to_owned();

  if let Some(name) = label {
    let agent = crate::role_agents::named(&ground.db, name).await?.ok_or_else(|| {
      Refused::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("The item names agent={name}, which is no agent the agency registers"),
      )
      .with_code("unknown-agent")
    })?;
    let called = agent.name.clone();

    return Ok((agent, called));
  }

  let Some(model) = model else {
    let agents = crate::role_agents::listing(&ground.db, &role).await?;
    let sentence =
      fill_marks(&JOB_REVIEW.agent_unnamed, &[("item", "The item"), ("role", &role), ("agents", &agents)]);

    return Err(Refused::new(StatusCode::CONFLICT, sentence).with_code("agent-unnamed"));
  };
  let account = crate::supervisors::named(&ground.db, &ask.agent_name)
    .await?
    .and_then(|supervisor| supervisor.owner_account);
  let candidates = crate::role_agents::candidates(ground, &role, model, account).await?;
  let names: Vec<&str> = candidates.iter().map(|agent| agent.name.as_str()).collect();
  let called = format!("a free agent of the {role} role on model={model} ({})", names.join(", "));

  match candidates.into_iter().next() {
    Some(first) => Ok((first, called)),
    None => {
      let agents = crate::role_agents::listing(&ground.db, &role).await?;
      let sentence = fill_marks(
        &JOB_REVIEW.no_candidate,
        &[("item", "The item"), ("model", model), ("role", &role), ("agents", &agents)],
      );

      Err(Refused::new(StatusCode::UNPROCESSABLE_ENTITY, sentence).with_code("no-candidate"))
    }
  }
}

/// What an item's steps are checked against before it is filed (docs/job-review.md, 「Stages」): the agent its
/// labels name, or the first candidate for the model they name, refused where they name neither (「filed_agent」);
/// the skills and the tools that agent holds; and the mark a filing that checked them names, read off the item
/// and what the agent is called by alone, so the steps may change between the two filings. Answers the mark and
/// the words the supervisor is told.
pub async fn steps_check(ground: &Ground, ask: &ItemFilingAsk) -> Result<(String, String), Refused> {
  let (agent, called) = filed_agent(ground, ask).await?;
  let role_name = agent.role.clone();
  let role = crate::agent_roles::named(&ground.db, &role_name).await?;
  let trees = Trees::of(ground);
  let plugin = Some(trees.agent_dir(&role_name).join("plugin"))
    .filter(|dir| dir.exists())
    .unwrap_or_else(|| trees.agent_dir("worker").join("plugin"));
  let skills_dir = plugin.join("skills");
  let named_skills = agent.skills.clone().or_else(|| role.as_ref().and_then(|role| role.skills.clone()));
  let skills: Vec<String> = match named_skills {
    Some(named) => named,
    None => {
      let mut held: Vec<String> = std::fs::read_dir(&skills_dir)
        .map(|read| {
          read
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect()
        })
        .unwrap_or_default();

      held.sort();
      held
    }
  };
  let bundles = agent
    .bundles
    .clone()
    .or_else(|| role.as_ref().map(|role| role.bundles.clone()))
    .unwrap_or_default();
  let listed = |lines: Vec<String>| {
    if lines.is_empty() { JOB_REVIEW.steps_check_none.clone() } else { lines.join("\n") }
  };
  let skill_lines = listed(skills.iter().map(|name| skill_line(&skills_dir, name)).collect());
  let tool_lines =
    listed(bundles.iter().map(|bundle| format!("- {}", ROLES.bundle_label(bundle))).collect());
  let digest = Sha256::digest(format!("{}\n{}\n{called}\n{role_name}", ask.title.trim(), ask.brief.trim()));
  let mark = format!("{digest:x}")[..12].to_owned();
  let words = fill_marks(
    &JOB_REVIEW.steps_check,
    &[
      ("role", &role_name),
      ("agent", &called),
      ("skills", &skill_lines),
      ("tools", &tool_lines),
      ("mark", &mark),
    ],
  );

  Ok((mark, words))
}

/// One item filed for the turn an agent carries now.
pub struct FileItem;

impl Route for FileItem {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/item-filings";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let ask: ItemFilingAsk = serde_json::from_value(call.body().await?).map_err(|why| {
      Refused::new(StatusCode::BAD_REQUEST, format!("an item's filing: {why}")).with_code("bad-request")
    })?;
    let Some(mark) = turn_marks::of(&ask.agent_name) else {
      return Err(
        Refused::new(
          StatusCode::CONFLICT,
          format!("{} is carrying no turn to file an item for", ask.agent_name),
        )
        .with_code("no-turn"),
      );
    };
    let item = item_of(&ask, mark).map_err(|why| {
      Refused::new(StatusCode::BAD_REQUEST, format!("an item's filing: {why}")).with_code("bad-request")
    })?;

    if let Some(repository) = given(ask.repository.as_ref())
      && crate::repositories::named(&call.ground().db, repository).await?.is_none()
    {
      return Err(
        Refused::new(StatusCode::UNPROCESSABLE_ENTITY, format!("no repository is named {repository}"))
          .with_code("unknown-repository"),
      );
    }

    // The repositories a change reaching several is lent at its launch, each one the store holds and no more than
    // one job's bound of checkouts (docs/api.md, 「assignments and jobs」).
    let lent =
      crate::repositories::named_repositories(&serde_json::to_value(&ask.labels).unwrap_or_default());
    let own = given(ask.repository.as_ref());

    for name in &lent {
      if crate::repositories::named(&call.ground().db, name).await?.is_none() {
        return Err(
          Refused::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("no repository is named {name}, in repositories"),
          )
          .with_code("unknown-repository"),
        );
      }
    }

    let checkouts = lent.iter().filter(|name| Some(name.as_str()) != own).count() + 1;

    // A supervisor files work on the repositories of its own account alone, and a supervisor of the platform's side
    // on the platform's (docs/api.md, 「assignments and jobs」).
    let account = crate::supervisors::named(&call.ground().db, &ask.agent_name)
      .await?
      .and_then(|supervisor| supervisor.owner_account);
    let goes_to = assignee_of(&ask).ok().flatten();

    for name in goes_to.into_iter().chain(lent.iter().map(String::as_str)) {
      crate::access::works_on(call.ground(), account, name).await?;
    }

    if !lent.is_empty() && checkouts > policy::workers::WORKERS.job_checkouts_most {
      return Err(super::loans::too_many(policy::workers::WORKERS.job_checkouts_most));
    }

    // The steps are checked against the agent the item runs on before anything is filed: a filing that names no
    // mark, or not the one that check answers, is told what to check and filed by the next.
    let (mark, check) = steps_check(call.ground(), &ask).await?;

    if given(ask.steps_check.as_ref()) != Some(mark.as_str()) {
      return Err(Refused::new(StatusCode::CONFLICT, check).with_code("steps-unchecked"));
    }

    let vars = call.ground().vars();
    let (Some(url), Some(bearer)) = (
      address(vars, "BMO_JOB_URL"),
      token(vars, "BMO_JOB_CHAT_TOKEN", "BMO_JOB_CHAT_TOKEN_FILE")
        .or_else(|| token(vars, "BMO_JOB_TOKEN", "BMO_JOB_TOKEN_FILE")),
    ) else {
      return Err(Refused::failed("no job service is named").with_code("upstream-failed"));
    };
    let upstream = |why: String| Refused::new(StatusCode::BAD_GATEWAY, why).with_code("upstream-failed");
    let answered = reqwest::Client::new()
      .post(format!("{url}/items"))
      .bearer_auth(&bearer)
      .json(&item)
      .send()
      .await
      .map_err(|why| upstream(format!("the job service: {why}")))?;
    let status = answered.status();
    let body: Value = answered.json().await.unwrap_or(Value::Null);

    if !status.is_success() {
      let said = body["error"].as_str().map_or_else(|| status.to_string(), str::to_owned);

      // A refusal of the item itself, a label naming what agency does not hold among them (docs/agent-control.md,
      // 「A label agency does not hold is refused as it is set」), is the caller's to mend, and keeps its status
      // and its code.
      if status.is_client_error() {
        let refused = Refused::new(status, format!("the job service refused the item: {said}"));

        return Err(match body["code"].as_str() {
          Some(code) => refused.with_code(code),
          None => refused,
        });
      }

      return Err(upstream(format!("the job service refused the item: {said}")));
    }

    let id = body["id"].as_i64().unwrap_or_default();
    let steps: Vec<&str> =
      ask.steps.iter().map(|step| step.trim()).filter(|step| !step.is_empty()).collect();
    // The steps the item's job runs in, saved as its work order (docs/job-review.md, 「Stages」).
    let work_order_refused = if steps.is_empty() {
      None
    } else {
      let order = json!({
        "purpose": ask.title,
        "directive": ask.brief,
        "steps": steps,
        "state": "saved",
      });
      let saved = reqwest::Client::new()
        .put(format!("{url}/items/{id}/work-order"))
        .bearer_auth(&bearer)
        .json(&order)
        .send()
        .await;

      match saved {
        Ok(saved) if saved.status().is_success() => None,
        Ok(saved) => {
          let status = saved.status();
          let said: Value = saved.json().await.unwrap_or(Value::Null);

          Some(said["error"].as_str().map_or_else(|| status.to_string(), str::to_owned))
        }
        Err(why) => Some(why.to_string()),
      }
    };
    let filed = ItemFiled {
      id,
      state: body["state"].as_str().unwrap_or_default().to_owned(),
      scope: body["scope"].as_str().map(str::to_owned),
      labels: body["labels"].clone(),
      work_order_refused,
    };

    Ok(answer(StatusCode::CREATED, serde_json::to_value(filed).unwrap_or_default()))
  }
}

crate::moved!(FileItem);
