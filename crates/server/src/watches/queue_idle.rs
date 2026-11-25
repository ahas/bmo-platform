//! The queue's minute pass (docs/agent-control.md, 「An idle agent is told to the orderers of the items it could
//! take」): an agent switched on, free and able to take an open item, told to the orderer of every item it could
//! take once for each stretch it stays so. The same pass owes each item's orderer what calls for its judgement
//! (「An item's change is told to its orderer」), each in agency's outbox and each once: an open item its paused
//! queue holds, one whose assignee no checkout is of, one whose `agent` or `role` label no launch takes, one whose
//! model no agent of its role runs, a draft awaiting approval, and a held item every item it waits on has ended;
//! and it reads the job service's events for the rest. An item whose notices are switched off is told of by none
//! of these (「An item's notices switched off」).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use policy::fill_marks;
use policy::job_notices::JOB_NOTICES;
use policy::queue_idle::QUEUE_IDLE;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use store::all;

use crate::Refusal;
use crate::job_notices;
use crate::outbox::{self, Keep, Notice, Receiver};
use crate::peers::gate::Gate;
use crate::queue::orderer::{JobService, Orderer};
use crate::queue::settings;
use crate::repositories::{AGENT_LABEL, MODEL_LABEL, ROLE_LABEL, named_by};
use crate::role_agents::RoleAgent;

pub use crate::queue::orderer::{RAISED_BY, parent_of, room_of};

/// The settings rows, one an agent by its name, of when an agent was first seen idle.
pub const IDLE_SEEN_PREFIX: &str = "queue_agent_idle:";
/// The prefix of the key an idle agent's notice is owed under, `<agent>@<room>` after it, `owner` standing for
/// the owner's private room.
pub const IDLE_OWED: &str = "agent-idle:";
/// The settings rows, one an item by its number, of the `role` label the orderer was told names no role.
pub const ROLE_TOLD_PREFIX: &str = "queue_role_told:";
/// The settings rows, one an item by its number, of when the orderer was told the item waits on its paused
/// queue.
pub const PAUSED_TOLD_PREFIX: &str = "queue_paused_told:";
/// The settings rows, one an item by its number, of when the orderer was told the item names no checkout.
pub const UNLAUNCHABLE_TOLD_PREFIX: &str = "queue_unlaunchable_told:";
/// The settings rows, one an item by its number, of the `agent` label whose refused launch the orderer was
/// told of.
pub const AGENT_TOLD_PREFIX: &str = "queue_agent_told:";
/// The settings rows, one an item by its number, of the role and the model the orderer was told no agent runs.
pub const NO_CANDIDATE_TOLD_PREFIX: &str = "queue_no_candidate_told:";
/// The settings rows, one an item by its number, of when the orderer was told the draft awaits approval.
pub const DRAFT_TOLD_PREFIX: &str = "queue_draft_told:";
/// The settings rows, one an item by its number, of when the orderer was told the held item's wait has ended.
pub const WAIT_TOLD_PREFIX: &str = "queue_wait_told:";

/// An open item as the alarm reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Open {
  pub id: i64,
  pub title: String,
  /// The checkout it is to go to, where it names one.
  pub assignee: Option<String>,
  /// The agent its `agent` label names, where it names one: its job waits for that agent alone.
  pub agent: Option<String>,
  /// The model its `model` label names, where it names one: with no agent named, its job waits for a free agent
  /// of its role on that model.
  pub model: Option<String>,
  /// The role its `role` label names, where it names one.
  pub role: Option<String>,
  /// Its priority in its queue, the higher first.
  pub priority: i64,
  /// Its place in its queue among items of one priority, the lower first.
  pub position: i64,
  /// The room it was asked in, where it names one: its `roomId`, else the room its `scope` names.
  pub room: Option<i64>,
  /// The item it was raised from, where it names one.
  pub parent: Option<i64>,
  /// The queue it waits in.
  pub queue: String,
  /// Whether its queue is paused or under a hold, and so gives no lease.
  pub paused: bool,
}

impl Open {
  /// The item as the orderer's resolver reads it.
  pub fn as_item(&self) -> Value {
    json!({ "id": self.id, "title": self.title, "roomId": self.room, "raisedBy": self.parent })
  }

  /// The role its job runs as: its `role` label's, else the role a job runs as.
  pub fn role(&self) -> &str {
    self.role.as_deref().unwrap_or_else(|| crate::agent_roles::job_role())
  }

  /// The model its job is to take an agent for: its `model` label's where no `agent` label names one
  /// (docs/repositories.md, 「An agent an item names」).
  pub fn asked_model(&self) -> Option<&str> {
    self.model.as_deref().filter(|_| self.agent.is_none())
  }

  // Its place in the queue's order: the highest priority, then the earliest place, then the lowest number.
  fn place(&self) -> (std::cmp::Reverse<i64>, i64, i64) {
    (std::cmp::Reverse(self.priority), self.position, self.id)
  }
}

/// What the queue stands at as one pass reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Standing {
  /// The open items in the queues that take leases.
  pub open: Vec<Open>,
  /// The open items in the queues that give no lease.
  pub paused: Vec<Open>,
  /// The checkouts nothing holds now, as `POST /assignments` reads them (docs/api.md, 「assignments and jobs」).
  pub free: Vec<String>,
  /// The repositories a launch grows a support checkout for while no launch is held for the machine's room
  /// (docs/worker-slots.md, 「The queue with support checkouts」), by name.
  pub growing: Vec<String>,
  /// The repository each checkout is of, by the checkout's name; a checkout it does not name is a base, its
  /// repository its own name.
  pub repositories: BTreeMap<String, String>,
  /// Every checkout agency holds, free or not, the built-in worker among them.
  pub known: Vec<String>,
  /// The agents busy now: a job or a slot carries a run at work on them (docs/role-agents.md, 「When an agent is
  /// busy」).
  pub carrying: Vec<String>,
  /// Every agent the store holds, by role and then by name.
  pub agents: Vec<RoleAgent>,
  /// Every role the store holds, by name.
  pub roles: Vec<String>,
}

impl Standing {
  /// Whether a checkout the assignee names, or a checkout of the repository it names, is free, or the repository
  /// it names grows one; for an item naming none, the checkout named `worker`.
  pub fn free_for(&self, assignee: Option<&str>) -> bool {
    let assignee = assignee.unwrap_or(crate::queue::workers::BUILTIN_WORKER);

    if self.growing.iter().any(|repository| repository == assignee) {
      return true;
    }

    self.free.iter().any(|checkout| {
      let repository = self.repositories.get(checkout).map_or(checkout.as_str(), String::as_str);

      crate::queue::workers::answers_to(checkout, repository, assignee)
    })
  }

  /// The agents of a role, switched on, whose road runs a model, by name: the candidates of an item naming that
  /// model (docs/role-agents.md, 「Which agent a run takes」).
  pub fn candidates<'a>(&'a self, role: &'a str, model: &'a str) -> impl Iterator<Item = &'a RoleAgent> {
    self
      .agents
      .iter()
      .filter(move |agent| agent.enabled && agent.role == role && agent.runs(model))
  }

  /// Whether `agent` could be given `item` now, as an idle agent is read (docs/agent-control.md, 「An idle agent
  /// is told to the orderers of the items it could take」): a checkout the item may go to is free or grows, and its
  /// `agent` label names this agent, whatever its role, since the job runs as its agent's; or, naming none, its role
  /// is the agent's and its `model` label names a model the agent's road runs or none.
  pub fn could_take(&self, agent: &RoleAgent, item: &Open) -> bool {
    let named = match &item.agent {
      Some(name) => name == &agent.name,
      None => item.role() == agent.role && item.model.as_deref().is_none_or(|model| agent.runs(model)),
    };

    named && self.free_for(item.assignee.as_deref())
  }

  /// Whether an agent is idle here but for the items it could take: switched on, of the deployment's own account,
  /// on a road naming a provider and a model, and carrying no run.
  pub fn free_agent(&self, agent: &RoleAgent) -> bool {
    agent.enabled
      && agent.owner_account.is_none()
      && agent.provider.is_some()
      && agent.model.is_some()
      && !self.carrying.contains(&agent.name)
  }
}

/// An idle agent and the open items it could take, in the queue's order.
#[derive(Clone, Debug, PartialEq)]
pub struct Idle<'a> {
  pub agent: &'a RoleAgent,
  pub items: Vec<&'a Open>,
}

/// Every idle agent: free, and able to take one open item at least (docs/agent-control.md, 「An idle agent is told
/// to the orderers of the items it could take」), by role and then by name.
pub fn idle_agents(standing: &Standing) -> Vec<Idle<'_>> {
  standing
    .agents
    .iter()
    .filter(|agent| standing.free_agent(agent))
    .filter_map(|agent| {
      let mut items: Vec<&Open> =
        standing.open.iter().filter(|item| standing.could_take(agent, item)).collect();

      items.sort_by_key(|item| item.place());
      (!items.is_empty()).then_some(Idle { agent, items })
    })
    .collect()
}

/// The open items naming a model, and no agent, that no agent of their role switched on runs, so no launch
/// takes them (docs/repositories.md, 「An agent an item names」).
pub fn without_candidate(standing: &Standing) -> Vec<&Open> {
  standing
    .open
    .iter()
    .chain(&standing.paused)
    .filter(|item| {
      item
        .asked_model()
        .is_some_and(|model| standing.candidates(item.role(), model).next().is_none())
    })
    .collect()
}

/// The open items whose assignee is no checkout agency holds, so no launch will ever take them.
pub fn unlaunchable(standing: &Standing) -> Vec<&Open> {
  standing
    .open
    .iter()
    .filter(|item| item.assignee.as_ref().is_some_and(|worker| !standing.known.contains(worker)))
    .collect()
}

/// Why an open item's `agent` label takes no launch now, where it does not: an agent the agency does not register,
/// one switched off, or one whose road follows its supervisor (docs/repositories.md, 「An agent an item names」).
pub fn agent_refusal(standing: &Standing, item: &Open) -> Option<String> {
  let label = item.agent.as_deref()?;

  match standing.agents.iter().find(|agent| agent.name == label) {
    None => Some(format!(
      "the agency registers no agent {label}. The agents it may name:\n{}",
      crate::role_agents::roads_held(item.role(), &standing.agents)
    )),
    Some(agent) if !agent.enabled => Some(format!("the agent {label} is switched off")),
    Some(agent) if agent.provider.is_none() || agent.model.is_none() => {
      Some(format!("the agent {label} follows its supervisor's road, which no job asks"))
    }
    Some(_) => None,
  }
}

/// The open items whose `role` label names no role the store holds.
pub fn role_unknown(standing: &Standing) -> Vec<&Open> {
  standing
    .open
    .iter()
    .chain(&standing.paused)
    .filter(|item| item.role.as_ref().is_some_and(|role| !standing.roles.contains(role)))
    .collect()
}

/// The instruction an idle agent is told with to the orderer of `items`, the room `orderer` names, under a kind
/// naming the agent and that room, `owner` where it names none: the receiver hands the owner's private room and a
/// room naming it to one chat, where one kind would replace the other's copy.
pub fn idle_instruction(agent: &RoleAgent, items: &[&Open], orderer: &Orderer) -> Value {
  let room = orderer.room.map_or_else(|| "owner".to_owned(), |room| room.to_string());
  let listed: Vec<String> = items
    .iter()
    .map(|item| {
      fill_marks(&QUEUE_IDLE.agent_idle_item, &[("item", &item.id.to_string()), ("title", &item.title)])
    })
    .collect();
  let order = fill_marks(
    &QUEUE_IDLE.agent_idle_order,
    &[
      ("agent", &agent.name),
      ("role", &agent.role),
      ("provider", agent.provider.as_deref().unwrap_or_default()),
      ("model", agent.model.as_deref().unwrap_or_default()),
      ("items", &listed.join("\n")),
    ],
  );

  json!({
    "room": orderer.room,
    "said": { "order": order },
    "agentName": Value::Null,
    "sort": "job",
    "kind": format!("{IDLE_OWED}{}@{room}", agent.name),
    "summary": format!("The agent {} is idle", agent.name),
    "run": Value::Null,
  })
}

/// Whether an item's notices are switched off (docs/agent-control.md, 「An item's notices switched off」): the job
/// service answers its switch-off, with its reason, as `noticesOff`.
pub fn notices_off(item: &Value) -> bool {
  item["noticesOff"].is_object()
}

/// A draft as the pass reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
  pub id: i64,
  pub title: String,
  /// Who raised it: the item it was raised from, else its requester.
  pub raised_by: Option<String>,
  /// The item it was raised from, where it names one: its `raisedBy`, else its `raised-by` label.
  pub parent: Option<i64>,
  /// The repository its work is in, its `assigneeName`.
  pub repository: Option<String>,
  pub brief: String,
  /// The room it was asked in, where it names one itself.
  pub room: Option<i64>,
}

impl Draft {
  /// The draft as the orderer's resolver reads it.
  pub fn as_item(&self) -> Value {
    json!({ "id": self.id, "roomId": self.room, "raisedBy": self.parent })
  }
}

/// A job service item read as a draft; none for a member that carries no number.
pub fn draft_of(item: &Value) -> Option<Draft> {
  let parent = parent_of(item);
  let requester = item["requesterName"].as_str().map(str::trim).filter(|name| !name.is_empty());

  Some(Draft {
    id: item["id"].as_i64()?,
    title: item["title"].as_str().unwrap_or_default().to_owned(),
    raised_by: parent.map(|parent| format!("item #{parent}")).or(requester.map(str::to_owned)),
    parent,
    repository: item["assigneeName"].as_str().map(str::to_owned),
    brief: item["brief"].as_str().unwrap_or_default().to_owned(),
    room: room_of(item),
  })
}

/// The drafts the orderer is yet to be told of.
pub fn drafts_due<'a>(drafts: &'a [Draft], told: &BTreeSet<i64>) -> Vec<&'a Draft> {
  drafts.iter().filter(|draft| !told.contains(&draft.id)).collect()
}

/// The first lines of a brief the orderer is told, blank ones left out, cut at the policy's characters.
pub fn brief_head(brief: &str) -> String {
  let head = brief
    .lines()
    .map(str::trim_end)
    .filter(|line| !line.trim().is_empty())
    .take(QUEUE_IDLE.draft_brief_lines)
    .collect::<Vec<_>>()
    .join("\n");

  if head.chars().count() <= QUEUE_IDLE.draft_brief_chars {
    return head;
  }

  let mut cut: String = head.chars().take(QUEUE_IDLE.draft_brief_chars).collect();

  cut.push('…');
  cut
}

/// The instruction a draft awaiting approval is queued with for its orderer.
pub fn approval_instruction(draft: &Draft, orderer: &Orderer) -> Value {
  let number = draft.id.to_string();
  let brief = brief_head(&draft.brief);
  let order = fill_marks(
    &QUEUE_IDLE.draft_order,
    &[
      ("item", &number),
      ("title", &draft.title),
      ("raised_by", draft.raised_by.as_deref().unwrap_or("no one named")),
      ("repository", draft.repository.as_deref().unwrap_or("none, host work")),
      ("brief", if brief.is_empty() { "(no brief)" } else { &brief }),
    ],
  );

  job_notices::instruction(
    "item-awaits-approval",
    draft.id,
    orderer,
    format!("Item #{} awaits approval", draft.id),
    order,
  )
}

/// A held item that names the items it waits on, as the pass reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Follower {
  pub id: i64,
  pub title: String,
  /// The labels naming what it waits on, each as `name=value`.
  pub label: String,
  /// The items it waits on, by their number.
  pub waits_on: Vec<i64>,
  /// The room it was asked in, where it names one itself.
  pub room: Option<i64>,
  /// The item it was raised from, where it names one.
  pub parent: Option<i64>,
}

impl Follower {
  /// The item as the orderer's resolver reads it.
  pub fn as_item(&self) -> Value {
    json!({ "id": self.id, "roomId": self.room, "raisedBy": self.parent })
  }
}

/// A job service item read as one that waits on others: none for a member that carries no number or no wait
/// label, or whose wait label has a part naming no item number. A label is the job service's own
/// comma-separated list of numbers, each with or without its `#`.
pub fn follower_of(item: &Value) -> Option<Follower> {
  let mut label = Vec::new();
  let mut waits_on = Vec::new();

  for name in &JOB_NOTICES.wait_labels {
    let value = match &item["labels"][name.as_str()] {
      Value::String(value) => value.clone(),
      Value::Number(value) => value.to_string(),
      _ => continue,
    };

    for part in value.split(',') {
      waits_on.push(part.trim().trim_start_matches('#').parse::<i64>().ok().filter(|id| *id > 0)?);
    }

    label.push(format!("{name}={value}"));
  }

  if waits_on.is_empty() {
    return None;
  }

  Some(Follower {
    id: item["id"].as_i64()?,
    title: item["title"].as_str().unwrap_or_default().to_owned(),
    label: label.join(" and "),
    waits_on,
    room: room_of(item),
    parent: parent_of(item),
  })
}

/// Whether an item waited on has ended: it stands `done`. An item whose commit waits to go live stays leased
/// until that commit is live, so `done` is already past its deployment, whatever its close label still reads.
pub fn wait_over(item: &Value) -> bool {
  item["state"] == "done"
}

// The item numbers a family of rows is kept for, each with its moment.
async fn marks(db: &DatabaseConnection, prefix: &str) -> Result<BTreeMap<i64, i64>, Refusal> {
  let mut held = BTreeMap::new();

  for name in settings::named(db, prefix).await? {
    if let (Ok(id), Some(at)) = (name.parse::<i64>(), setting_number(db, &format!("{prefix}{name}")).await?)
    {
      held.insert(id, at);
    }
  }

  Ok(held)
}

// The told rows of a family let go for every item no longer among `standing`; the items still told answered.
async fn kept_marks(
  db: &DatabaseConnection,
  prefix: &str,
  standing: &BTreeSet<i64>,
) -> Result<BTreeSet<i64>, String> {
  let fault = |why: Refusal| why.detail;
  let told = marks(db, prefix).await.map_err(fault)?;

  for id in told.keys().filter(|id| !standing.contains(id)) {
    settings::forget(db, &format!("{prefix}{id}")).await.map_err(fault)?;
  }

  Ok(told.into_keys().filter(|id| standing.contains(id)).collect())
}

// The orderer of an item as the pass finds it: through the job service where the deployment names one, else
// the room the item names itself.
async fn orderer(service: Option<&JobService>, item: &Value) -> Result<Orderer, String> {
  match service {
    Some(service) => service.orderer_of(item).await,
    None => Ok(Orderer {
      room: room_of(item),
      requester: None,
    }),
  }
}

/// Each idle agent owed to the orderers of the items it could take, once for each stretch it stays idle and
/// again where a room's items change, and the rows kept for it brought up to date: an agent first seen idle now
/// marked, and both the mark and the kept notices of an agent no longer idle, or of a room left with no item it
/// could take, let go, so it is told again only once a new stretch begins.
pub async fn tell_idle(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  standing: &Standing,
  now: i64,
) -> Result<(), String> {
  // A deployment that carries no receiver has no supervisor to tell.
  if Receiver::of(vars).is_none() {
    return Ok(());
  }

  let service = JobService::of(vars);
  let fault = |why: Refusal| why.detail;
  let idle = idle_agents(standing);
  let names: BTreeSet<&str> = idle.iter().map(|one| one.agent.name.as_str()).collect();
  let mut seen: BTreeMap<String, i64> = BTreeMap::new();

  for name in settings::named(db, IDLE_SEEN_PREFIX).await.map_err(fault)? {
    let key = format!("{IDLE_SEEN_PREFIX}{name}");

    if names.contains(name.as_str()) {
      seen.insert(name, setting_number(db, &key).await.map_err(fault)?.unwrap_or(now));
    } else {
      settings::forget(db, &key).await.map_err(fault)?;
    }
  }

  for name in &names {
    if !seen.contains_key(*name) {
      settings::set(db, &format!("{IDLE_SEEN_PREFIX}{name}"), &now.to_string())
        .await
        .map_err(fault)?;
      seen.insert((*name).to_owned(), now);
    }
  }

  // Each due agent's items grouped by the room their orderer stands in, `owner` for the owner's private room.
  let mut due: BTreeMap<String, (&RoleAgent, Orderer, Vec<&Open>)> = BTreeMap::new();

  for one in &idle {
    if seen
      .get(&one.agent.name)
      .is_none_or(|since| now - since < QUEUE_IDLE.launch_seconds * 1000)
    {
      continue;
    }

    for item in &one.items {
      let orderer = orderer(service.as_ref(), &item.as_item()).await?;
      let room = orderer.room.map_or_else(|| "owner".to_owned(), |room| room.to_string());

      due
        .entry(format!("{}@{room}", one.agent.name))
        .or_insert_with(|| (one.agent, orderer, vec![]))
        .2
        .push(item);
    }
  }

  let owed = outbox::owed(db, IDLE_OWED).await?;

  for (key, row) in &owed {
    if !due.contains_key(key) {
      outbox::let_go_as_read(db, vars, &format!("{IDLE_OWED}{key}"), row).await?;
    }
  }

  for (key, (agent, orderer, items)) in &due {
    let mark: Vec<String> = items.iter().map(|item| item.id.to_string()).collect();
    let mark = mark.join(",");

    if owed.get(key).is_some_and(|row| row.mark == mark) {
      continue;
    }

    // Keyed by its kind, which names the agent and the room as `key` does.
    let notice =
      Notice::instruction(idle_instruction(agent, items, orderer), Keep::Kept).marked(mark.clone());

    outbox::owe(db, vars, notice).await?;
    println!("queue: the agent {} is idle and could take {mark}, owed to their orderer", agent.name);
  }

  Ok(())
}

/// The open items that cannot go told to their orderer once each: those its paused queue holds, and those
/// whose assignee no checkout is of; each row let go once its item no longer stands so.
pub async fn tell_stuck(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  standing: &Standing,
  now: i64,
) -> Result<(), String> {
  if Receiver::of(vars).is_none() {
    return Ok(());
  }

  let service = JobService::of(vars);
  let fault = |why: Refusal| why.detail;
  let paused: BTreeSet<i64> = standing.paused.iter().map(|item| item.id).collect();
  let told = kept_marks(db, PAUSED_TOLD_PREFIX, &paused).await?;

  for item in standing.paused.iter().filter(|item| !told.contains(&item.id)) {
    let orderer = orderer(service.as_ref(), &item.as_item()).await?;

    let notice = job_notices::paused(item.id, &item.title, &item.queue, &orderer);

    outbox::owe(db, vars, Notice::instruction(notice, Keep::Once)).await?;
    settings::set(db, &format!("{PAUSED_TOLD_PREFIX}{}", item.id), &now.to_string())
      .await
      .map_err(fault)?;
    println!("queue: item #{} waits on its paused queue {}, owed to its orderer", item.id, item.queue);
  }

  let stuck = unlaunchable(standing);
  let standing_ids: BTreeSet<i64> = stuck.iter().map(|item| item.id).collect();
  let told = kept_marks(db, UNLAUNCHABLE_TOLD_PREFIX, &standing_ids).await?;

  for item in stuck.into_iter().filter(|item| !told.contains(&item.id)) {
    let assignee = item.assignee.as_deref().unwrap_or_default();
    let orderer = orderer(service.as_ref(), &item.as_item()).await?;

    let notice = job_notices::unlaunchable(item.id, &item.title, assignee, &orderer);

    outbox::owe(db, vars, Notice::instruction(notice, Keep::Once)).await?;
    settings::set(db, &format!("{UNLAUNCHABLE_TOLD_PREFIX}{}", item.id), &now.to_string())
      .await
      .map_err(fault)?;
    println!(
      "queue: item #{} names {assignee}, which no checkout is of, owed to its orderer",
      item.id
    );
  }

  let refused: Vec<(&Open, String)> = standing
    .open
    .iter()
    .chain(&standing.paused)
    .filter_map(|item| Some((item, agent_refusal(standing, item)?)))
    .collect();
  let refused_ids: BTreeSet<i64> = refused.iter().map(|(item, _)| item.id).collect();

  let_go_marks(db, AGENT_TOLD_PREFIX, &refused_ids).await?;

  for (item, why) in refused {
    let label = item.agent.as_deref().unwrap_or_default();

    tell_agent_refused(db, vars, item.id, &item.as_item(), label, &why).await?;
  }

  let unknown = role_unknown(standing);
  let unknown_ids: BTreeSet<i64> = unknown.iter().map(|item| item.id).collect();

  let_go_marks(db, ROLE_TOLD_PREFIX, &unknown_ids).await?;

  for item in unknown {
    let label = item.role.as_deref().unwrap_or_default();

    tell_role_unknown(db, vars, item.id, &item.as_item(), label, &standing.roles).await?;
  }

  let lacking = without_candidate(standing);
  let lacking_ids: BTreeSet<i64> = lacking.iter().map(|item| item.id).collect();

  let_go_marks(db, NO_CANDIDATE_TOLD_PREFIX, &lacking_ids).await?;

  for item in lacking {
    let (role, model) = (item.role(), item.asked_model().unwrap_or_default());
    let agents = crate::role_agents::roads_held(role, &standing.agents);

    tell_no_candidate(db, vars, item.id, &item.as_item(), role, model, &agents).await?;
  }

  Ok(())
}

/// An item whose model no agent of its role runs told to its orderer, once for that role and model
/// (docs/agent-control.md, 「An item's change is told to its orderer」): the role and the model told are kept by
/// the item, so the same pair is not told again and another one is. `item` carries its title and what its
/// orderer is found by. Answers whether it was told now.
pub async fn tell_no_candidate(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  id: i64,
  item: &Value,
  role: &str,
  model: &str,
  agents: &str,
) -> Result<bool, String> {
  if Receiver::of(vars).is_none() {
    return Ok(false);
  }

  let fault = |why: Refusal| why.detail;
  let key = format!("{NO_CANDIDATE_TOLD_PREFIX}{id}");
  let pair = format!("{role} {model}");

  if settings::get(db, &key).await.map_err(fault)?.as_deref() == Some(pair.as_str()) {
    return Ok(false);
  }

  let mut item = item.clone();

  item["id"] = json!(id);

  let orderer = orderer(JobService::of(vars).as_ref(), &item).await?;
  let title = item["title"].as_str().unwrap_or_default();

  let notice = job_notices::no_candidate(id, title, role, model, agents, &orderer);

  outbox::owe(db, vars, Notice::instruction(notice, Keep::Once)).await?;
  settings::set(db, &key, &pair).await.map_err(fault)?;
  println!(
    "queue: item #{id} names model={model} for the {role} role, which no agent runs, owed to its orderer"
  );

  Ok(true)
}

/// An item whose launch was refused for its `agent` label told to its orderer, once for that label
/// (docs/agent-control.md, 「An item's change is told to its orderer」): the label told is kept by the item, so
/// the same label refused again is not told again and another one is. Answers whether it was told now.
pub async fn tell_agent_refused(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  id: i64,
  item: &Value,
  label: &str,
  why: &str,
) -> Result<bool, String> {
  if Receiver::of(vars).is_none() {
    return Ok(false);
  }

  let fault = |why: Refusal| why.detail;
  let key = format!("{AGENT_TOLD_PREFIX}{id}");

  if settings::get(db, &key).await.map_err(fault)?.as_deref() == Some(label) {
    return Ok(false);
  }

  let mut item = item.clone();

  item["id"] = json!(id);

  let orderer = orderer(JobService::of(vars).as_ref(), &item).await?;
  let title = item["title"].as_str().unwrap_or_default();

  let notice = job_notices::agent_refused(id, title, label, why, &orderer);

  outbox::owe(db, vars, Notice::instruction(notice, Keep::Once)).await?;
  settings::set(db, &key, label).await.map_err(fault)?;
  println!("queue: item #{id} names agent={label}, which no job can run on, owed to its orderer");

  Ok(true)
}

/// An item whose `role` label names no role agency holds told to its orderer, once for that label, with the roles it
/// holds. Answers whether it was told now.
pub async fn tell_role_unknown(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  id: i64,
  item: &Value,
  label: &str,
  roles: &[String],
) -> Result<bool, String> {
  if Receiver::of(vars).is_none() {
    return Ok(false);
  }

  let fault = |why: Refusal| why.detail;
  let key = format!("{ROLE_TOLD_PREFIX}{id}");

  if settings::get(db, &key).await.map_err(fault)?.as_deref() == Some(label) {
    return Ok(false);
  }

  let mut item = item.clone();

  item["id"] = json!(id);

  let orderer = orderer(JobService::of(vars).as_ref(), &item).await?;
  let title = item["title"].as_str().unwrap_or_default();
  let notice = job_notices::role_unknown(id, title, label, roles, &orderer);

  outbox::owe(db, vars, Notice::instruction(notice, Keep::Once)).await?;
  settings::set(db, &key, label).await.map_err(fault)?;
  println!("queue: item #{id} names role={label}, which agency does not hold, owed to its orderer");

  Ok(true)
}

// The marks of a family kept by an item's number let go for every item no longer among `standing`, so an item that
// comes to stand so again is told again.
async fn let_go_marks(
  db: &DatabaseConnection,
  prefix: &str,
  standing: &BTreeSet<i64>,
) -> Result<(), String> {
  let fault = |why: Refusal| why.detail;

  for id in settings::named(db, prefix).await.map_err(fault)? {
    if id.parse::<i64>().is_ok_and(|id| !standing.contains(&id)) {
      settings::forget(db, &format!("{prefix}{id}")).await.map_err(fault)?;
    }
  }

  Ok(())
}

/// The mark of an item's refused `agent` label let go, as a job of the item is launched.
pub async fn agent_told_let_go(db: &DatabaseConnection, id: i64) {
  if let Err(why) = settings::forget(db, &format!("{AGENT_TOLD_PREFIX}{id}")).await {
    eprintln!("queue: the told mark of item #{id}'s agent was not let go: {}", why.detail);
  }
}

/// The mark of an item's model told as having no candidate let go, as a job of the item is launched.
pub async fn no_candidate_told_let_go(db: &DatabaseConnection, id: i64) {
  if let Err(why) = settings::forget(db, &format!("{NO_CANDIDATE_TOLD_PREFIX}{id}")).await {
    eprintln!("queue: the told mark of item #{id}'s model was not let go: {}", why.detail);
  }
}

async fn setting_number(db: &DatabaseConnection, key: &str) -> Result<Option<i64>, Refusal> {
  Ok(
    settings::get(db, key)
      .await?
      .and_then(|value| value.trim().parse::<i64>().ok())
      .filter(|value| *value > 0),
  )
}

/// What agency itself holds of the queue's standing: its checkouts, its jobs and the alarm's own rows, the open
/// items split by whether their queue gives a lease.
pub async fn held(db: &DatabaseConnection, open: Vec<Open>) -> Result<Standing, Refusal> {
  // An item a job of agency's is still at work on waits on nothing, its lease run out or not.
  let working: Vec<i64> = all(
    db,
    r#"SELECT "item_id" FROM "jobs" WHERE "state" IN ('starting', 'running', 'reviewing')"#,
    vec![],
  )
  .await?
  .iter()
  .filter_map(|row| row.try_get::<i64>("", "item_id").ok())
  .collect();
  let (paused, open): (Vec<Open>, Vec<Open>) = open
    .into_iter()
    .filter(|item| !working.contains(&item.id))
    .partition(|item| item.paused);
  // A checkout is free as an assignment reads it: the holds that have ended let go first, then nothing holding
  // it, a hold no job has started on as much as a job at work.
  crate::queue::holds::release_ended(db).await?;

  let busy = crate::queue::holds::held_checkouts(db).await?;
  let mut carrying: Vec<String> = all(
    db,
    r#"SELECT DISTINCT "item_agent" FROM "jobs"
       WHERE ("state" IN ('starting', 'running') OR ("state" = 'reviewing' AND NOT "review_parked"))
         AND "item_agent" IS NOT NULL AND NOT "machine_turn""#,
    vec![],
  )
  .await?
  .iter()
  .filter_map(|row| row.try_get::<String>("", "item_agent").ok())
  .collect();

  carrying.extend(crate::role_agents::busy_any(db).await?);

  let agents = crate::role_agents::every(db).await?;
  let roles = crate::agent_roles::names(db).await?;

  let workers = crate::queue::workers::workers(db, std::path::Path::new("")).await?;
  let known = workers.iter().map(|worker| worker.name.clone()).collect();
  let repositories = workers
    .iter()
    .filter(|worker| worker.is_support())
    .map(|worker| (worker.name.clone(), worker.repository().to_owned()))
    .collect();
  let free = workers
    .into_iter()
    .filter(|worker| !worker.builtin && !busy.contains(&worker.name))
    .map(|worker| worker.name)
    .collect();
  // A launch held for the machine grows nothing, so no repository counts as growing while it stands.
  let growing = match crate::watches::machine::launch_held(db).await? {
    Some(_) => vec![],
    None => crate::repositories::every(db)
      .await?
      .into_iter()
      .filter(crate::repositories::Repository::grows)
      .map(|repository| repository.name)
      .collect(),
  };
  Ok(Standing {
    open,
    paused,
    free,
    growing,
    repositories,
    known,
    carrying,
    agents,
    roles,
  })
}

// The job service's open items, each marked by whether its queue gives a lease — open, holding nothing. An item
// stands open only once an approver approved it or it was raised with an unattended label, so its state is the
// whole of whether it may be launched.
/// Whether an open item's close already merged its commit, its label `close-phase` reading `waiting-batch` or an
/// older close's `waiting-live`: its work stands on main and only its repository's deployment remains, so it is no
/// work a checkout could take, and it is told only as waiting to go live.
pub fn merged_awaiting_deploy(item: &Value) -> bool {
  let phase = &item["labels"]["close-phase"];

  phase == "waiting-batch" || phase == "waiting-live"
}

async fn open_items(service: &JobService) -> Result<Vec<Open>, String> {
  let queues = service.read("/queues", &[]).await?;
  let taking: Vec<&str> = queues["items"]
    .as_array()
    .into_iter()
    .flatten()
    .filter(|queue| {
      queue["state"] == "open" && queue["holds"].as_array().is_none_or(|holds| holds.is_empty())
    })
    .filter_map(|queue| queue["name"].as_str())
    .collect();

  Ok(
    service
      .every_page("/items", &[("state", "open")])
      .await?
      .iter()
      .filter(|item| !merged_awaiting_deploy(item) && !notices_off(item))
      .filter_map(|item| {
        let queue = item["queueName"].as_str().unwrap_or_default().to_owned();

        Some(Open {
          id: item["id"].as_i64()?,
          title: item["title"].as_str().unwrap_or_default().to_owned(),
          assignee: item["assigneeName"].as_str().filter(|name| !name.is_empty()).map(str::to_owned),
          agent: named_by(&item["labels"], AGENT_LABEL).map(str::to_owned),
          model: named_by(&item["labels"], MODEL_LABEL).map(str::to_owned),
          role: named_by(&item["labels"], ROLE_LABEL).map(str::to_owned),
          priority: item["priority"].as_i64().unwrap_or_default(),
          position: item["position"].as_i64().unwrap_or_default(),
          room: room_of(item),
          parent: parent_of(item),
          paused: !taking.contains(&queue.as_str()),
          queue,
        })
      })
      .collect(),
  )
}

/// Each draft awaiting approval told to its orderer, once: the told rows of items no longer a draft let go, and
/// a row kept for each told now.
pub async fn tell_drafts(db: &DatabaseConnection, vars: &BTreeMap<String, String>) -> Result<(), String> {
  let (Some(service), Some(_)) = (JobService::of(vars), Receiver::of(vars)) else {
    return Ok(());
  };
  let fault = |why: Refusal| why.detail;
  let drafts: Vec<Draft> = service
    .every_page("/items", &[("state", "draft")])
    .await?
    .iter()
    .filter(|item| !notices_off(item))
    .filter_map(draft_of)
    .collect();
  let standing: BTreeSet<i64> = drafts.iter().map(|draft| draft.id).collect();
  let told = kept_marks(db, DRAFT_TOLD_PREFIX, &standing).await?;

  for draft in drafts_due(&drafts, &told) {
    let orderer = service.orderer_of(&draft.as_item()).await?;

    outbox::owe(db, vars, Notice::instruction(approval_instruction(draft, &orderer), Keep::Once)).await?;
    settings::set(db, &format!("{DRAFT_TOLD_PREFIX}{}", draft.id), &crate::now_ms().to_string())
      .await
      .map_err(fault)?;
    println!("queue: draft #{} awaits approval, owed to its orderer", draft.id);
  }

  Ok(())
}

/// Each held item every item it waits on has ended told to its orderer, once while it stays held: the told rows
/// of items no longer held, or no longer naming what they wait on, let go, and a row kept for each told now. An
/// item waited on that the job service does not answer for ends no wait.
pub async fn tell_waits(db: &DatabaseConnection, vars: &BTreeMap<String, String>) -> Result<(), String> {
  let (Some(service), Some(_)) = (JobService::of(vars), Receiver::of(vars)) else {
    return Ok(());
  };
  let fault = |why: Refusal| why.detail;
  let followers: Vec<Follower> = service
    .every_page("/items", &[("state", "held")])
    .await?
    .iter()
    .filter(|item| !notices_off(item))
    .filter_map(follower_of)
    .collect();
  let standing: BTreeSet<i64> = followers.iter().map(|follower| follower.id).collect();
  let told = kept_marks(db, WAIT_TOLD_PREFIX, &standing).await?;
  let mut read: BTreeMap<i64, Option<Value>> = BTreeMap::new();

  for follower in followers.iter().filter(|follower| !told.contains(&follower.id)) {
    let mut waited = Vec::new();

    for id in &follower.waits_on {
      if !read.contains_key(id) {
        read.insert(*id, service.read(&format!("/items/{id}"), &[]).await.ok());
      }

      waited.push(read[id].clone());
    }

    let Some(waited) = waited.into_iter().collect::<Option<Vec<Value>>>() else { continue };

    if !waited.iter().all(wait_over) {
      continue;
    }

    let orderer = service.orderer_of(&follower.as_item()).await?;

    let notice = job_notices::wait_ended(follower.id, &follower.title, &follower.label, &waited, &orderer);

    outbox::owe(db, vars, Notice::instruction(notice, Keep::Once)).await?;
    settings::set(db, &format!("{WAIT_TOLD_PREFIX}{}", follower.id), &crate::now_ms().to_string())
      .await
      .map_err(fault)?;
    println!("queue: held item #{} waits on nothing more, owed to its orderer", follower.id);
  }

  Ok(())
}

/// One pass: the label values published where they changed; the queue read; each idle agent, each open item that
/// cannot go, each draft awaiting approval and each held item whose wait has ended told to its orderer; and the job
/// service's events told.
pub async fn pass(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
  gate: &Arc<dyn Gate>,
) -> Result<(), String> {
  let Some(service) = JobService::of(vars) else { return Ok(()) };

  if let Err(why) = crate::label_values::publish(db, &service).await {
    eprintln!("queue: the label values were not published: {why}");
  }

  let open = open_items(&service).await?;
  let standing = held(db, open).await.map_err(|why| why.detail)?;
  let now = crate::now_ms();

  if let Err(why) = tell_idle(db, vars, &standing, now).await {
    eprintln!("queue: an idle agent was not told: {why}");
  }

  if let Err(why) = tell_stuck(db, vars, &standing, now).await {
    eprintln!("queue: an item that cannot go was not told: {why}");
  }

  if let Err(why) = tell_drafts(db, vars).await {
    eprintln!("queue: a draft awaiting approval was not told: {why}");
  }

  if let Err(why) = tell_waits(db, vars).await {
    eprintln!("queue: a held item whose wait has ended was not told: {why}");
  }

  if let Err(why) = crate::watches::job_events::pass(db, vars, gate).await {
    eprintln!("queue: the job service's events were not told: {why}");
  }

  // Words kept for a running job while its turn could not take them are offered to that turn again.
  crate::job_words::offer_kept(db, gate).await;

  Ok(())
}

/// What cannot go told at once, as the owner adds, changes, switches or removes an agent or a role
/// (docs/agent-control.md, 「A label agency does not hold is refused as it is set」): the label values published
/// and the open items read and told as the minute pass tells them. A deployment naming no job service tells
/// nothing.
pub async fn agents_changed(db: &DatabaseConnection, vars: &BTreeMap<String, String>) {
  let Some(service) = JobService::of(vars) else { return };

  if let Err(why) = crate::label_values::publish(db, &service).await {
    eprintln!("queue: the label values were not published: {why}");
  }

  let standing = match open_items(&service).await {
    Ok(open) => held(db, open).await.map_err(|why| why.detail),
    Err(why) => Err(why),
  };

  match standing {
    Ok(standing) => {
      if let Err(why) = tell_stuck(db, vars, &standing, crate::now_ms()).await {
        eprintln!("queue: an item no launch takes after an agent's change was not told: {why}");
      }
    }
    Err(why) => eprintln!("queue: the queue was not read after an agent's change: {why}"),
  }
}
