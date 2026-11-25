//! A job's lease kept on the harness's behalf (docs/api.md, 「assignments and jobs」): every job still
//! starting or running has its item's lease renewed in the job service, and a lease the service no longer holds
//! taken again on the same item for the same holder, and its assignee where it names one, the job keeping
//! the new lease's id. An item that moved under a running job — held, dropped, merged or leased by another
//! holder — is left as it stands and told once to its orderer. An ended job whose lease stands for the item's
//! close has it renewed the same way until the service holds it no more — the close moved the item to done, or
//! the lease was given back — and is never leased again; an item still leased at the pass after the one that
//! first found its job ended is told once for that lease to its orderer, unless its commit waits for its
//! repository's batch deployment (docs/agent-control.md, 「An item's change is told to its orderer」). A running
//! job's lease holds fixed what its launch read of the item, and an ended job's holds nothing fixed
//! (docs/repositories.md, 「An agent an item names」). A deployment that names no job service keeps nothing.

use std::collections::{BTreeMap, BTreeSet};

use policy::job_leases::JOB_LEASES;
use reqwest::StatusCode;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use store::{all, run};

use crate::Refusal;
use crate::job_end::{address, token};
use crate::job_notices;
use crate::outbox::{self, Keep, Notice, Receiver};
use crate::queue::orderer::{JobService, Orderer};
use crate::queue::settings;
use crate::repositories::{AGENT_LABEL, MODEL_LABEL, REPOSITORIES_LABEL, ROLE_LABEL};
use crate::watches::queue_idle::merged_awaiting_deploy;

/// The item fields a job's launch reads beside its labels: the repository its job runs in and the room it
/// answers to.
const LAUNCH_FIELDS: [&str; 2] = ["assigneeName", "scope"];

/// What of its item a running job's lease holds fixed (docs/repositories.md, 「An agent an item names」): the labels
/// and fields its launch read, with the job and the agent it runs on as the job service's reason for a refusal.
pub fn launch_frozen(job: i64, agent: Option<&str>) -> Value {
  let runs_on =
    agent.map_or_else(|| "the agent its launch took".to_owned(), |agent| format!("the agent {agent}"));

  json!({
    "fields": LAUNCH_FIELDS,
    "labels": [AGENT_LABEL, ROLE_LABEL, MODEL_LABEL, REPOSITORIES_LABEL],
    "reason": format!(
      "job {job} runs on {runs_on} with what its launch read; stop the job, and the change reaches the item's next launch"
    ),
  })
}

/// A launched job's lease set to hold fixed what its launch read, at once rather than at the next pass. A lease the
/// job service could not be asked to hold is said in the log, and the next pass asks again.
pub async fn hold_launch(vars: &BTreeMap<String, String>, job: i64, lease: i64, agent: Option<&str>) {
  let (Some(url), Some(token)) =
    (address(vars, "BMO_JOB_URL"), token(vars, "BMO_JOB_TOKEN", "BMO_JOB_TOKEN_FILE"))
  else {
    return;
  };
  let client = reqwest::Client::new();
  let service = Service {
    url: &url,
    token: &token,
    client: &client,
  };

  match service.renew(lease, &launch_frozen(job, agent)).await {
    Ok(true) => {}
    Ok(false) => eprintln!("job {job}: its lease {lease} was gone as its launch was held fixed"),
    Err(why) => eprintln!("job {job}: what its launch read was not held fixed on lease {lease}: {why}"),
  }
}

/// The settings rows, one a running job by its id, of the state its item was told to have moved to.
pub const MOVED_TOLD_PREFIX: &str = "job_moved_told:";
/// The settings rows, one an ended job by its id, of its item's close: `seen` once a pass found the lease
/// still standing.
pub const CLOSE_PREFIX: &str = "job_close_mark:";
/// The prefix of the key an ended job's unclosed item is owed under, the item's number after it; the row is
/// marked with the lease it was told for.
pub const CLOSE_OWED: &str = "job-awaits-close:";

/// What a pass did with one running job's lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kept {
  /// The lease stood and was renewed.
  Renewed,
  /// The lease had run out and the item was leased again, under this id.
  Taken(i64),
  /// An ended job's lease the service holds no more: the item's close ended it, or it was given back.
  Closed,
  /// Neither: the item is another holder's or no longer open, or the job service could not be asked; why.
  Left(String),
}

/// One pass over the running jobs, each one's lease renewed or taken again, and the ended jobs whose lease
/// stands for the item's close, each one's renewed or let go once the service holds it no more. Answers each
/// job with what was done; nothing where the deployment names no job service.
pub async fn pass(
  db: &DatabaseConnection,
  vars: &BTreeMap<String, String>,
) -> Result<Vec<(i64, Kept)>, Refusal> {
  let (Some(url), Some(token)) =
    (address(vars, "BMO_JOB_URL"), token(vars, "BMO_JOB_TOKEN", "BMO_JOB_TOKEN_FILE"))
  else {
    return Ok(vec![]);
  };
  let rows = all(
    db,
    r#"SELECT "id", "item_id", "lease_id", "holder_name", "worker_name", "state", "item_agent" FROM "jobs"
       WHERE "state" IN ('starting', 'running', 'reviewing') OR ("state" = 'ended' AND "lease_kept") ORDER BY "id""#,
    vec![],
  )
  .await?;
  let client = reqwest::Client::new();
  let service = Service {
    url: &url,
    token: &token,
    client: &client,
  };
  let tells = Tells {
    db,
    vars,
    service: JobService {
      url: url.clone(),
      token: token.clone(),
    },
    receiver: Receiver::of(vars).is_some(),
  };
  let mut kept = vec![];
  let mut running = BTreeSet::new();
  let mut ended = BTreeSet::new();
  let mut unclosed = BTreeSet::new();

  for row in rows {
    let (Ok(id), Ok(item), Ok(lease), Ok(holder), Ok(worker), Ok(state)) = (
      row.try_get::<i64>("", "id"),
      row.try_get::<i64>("", "item_id"),
      row.try_get::<i64>("", "lease_id"),
      row.try_get::<String>("", "holder_name"),
      row.try_get::<String>("", "worker_name"),
      row.try_get::<String>("", "state"),
    ) else {
      continue;
    };
    let agent = row.try_get::<Option<String>>("", "item_agent").ok().flatten();

    if item <= 0 {
      continue;
    }

    if state == "ended" {
      // An ended job runs on nothing its item's next launch could change.
      let done = match service.renew(lease, &Value::Null).await {
        Ok(true) => {
          tells.unclosed(id, item, lease).await;
          Kept::Renewed
        }
        Ok(false) => {
          run(db, r#"UPDATE "jobs" SET "lease_kept" = false WHERE "id" = $1"#, vec![id.into()]).await?;
          println!("job {id}: the lease {lease} on item #{item} has ended with the item's close");
          Kept::Closed
        }
        Err(why) => {
          eprintln!(
            "job {id}: the lease {lease} on item #{item}, kept for its close, was not renewed: {why}"
          );
          Kept::Left(why)
        }
      };

      // A lease that ended lets its close mark go with this pass; one not read keeps its item owed as it stood.
      if done != Kept::Closed {
        ended.insert(id);
        unclosed.insert(item);
      }

      kept.push((id, done));
      continue;
    }

    running.insert(id);

    let frozen = launch_frozen(id, agent.as_deref());
    let done = match service.renew(lease, &frozen).await {
      Ok(true) => Kept::Renewed,
      Ok(false) => match service.take(item, &holder, &frozen).await {
        Ok(Took::Leased(taken)) => {
          run(
            db,
            r#"UPDATE "jobs" SET "lease_id" = $1 WHERE "id" = $2"#,
            vec![taken.into(), id.into()],
          )
          .await?;
          println!("job {id}: the lease on item #{item} had run out, taken again as lease {taken}");
          Kept::Taken(taken)
        }
        Ok(Took::Moved(held)) => {
          let why = format!("the item is {}, so it is left as it stands", job_notices::moved_state(&held));

          eprintln!("job {id}: the lease on item #{item} had run out and was not taken again: {why}");
          tells.moved(id, &worker, &held).await;
          Kept::Left(why)
        }
        Err(why) => {
          eprintln!("job {id}: the lease on item #{item} had run out and was not taken again: {why}");
          Kept::Left(why)
        }
      },
      Err(why) => {
        eprintln!("job {id}: the lease {lease} on item #{item} was not renewed: {why}");
        Kept::Left(why)
      }
    };

    kept.push((id, done));
  }

  tells.let_go(&running, &ended, &unclosed).await;

  Ok(kept)
}

// What became of a lease asked again: taken, under its id, or not, the item having moved, as it stands.
enum Took {
  Leased(i64),
  Moved(Value),
}

// The job service as the pass reaches it.
struct Service<'a> {
  url: &'a str,
  token: &'a str,
  client: &'a reqwest::Client,
}

impl Service<'_> {
  // Whether the lease stood and was renewed, holding `frozen` fixed; false where the service holds it no more.
  async fn renew(&self, lease: i64, frozen: &Value) -> Result<bool, String> {
    if lease <= 0 {
      return Ok(false);
    }

    let answer = self
      .client
      .patch(format!("{}/leases/{lease}", self.url))
      .bearer_auth(self.token)
      .json(&json!({ "ttlMs": JOB_LEASES.ttl_ms, "frozen": frozen }))
      .send()
      .await
      .map_err(|why| why.to_string())?;

    match answer.status() {
      status if status.is_success() => Ok(true),
      StatusCode::GONE | StatusCode::NOT_FOUND => Ok(false),
      status => Err(format!("the job service answered {status}")),
    }
  }

  // The item leased again for the job's holder and worker, holding `frozen` fixed, where it is open: the new lease's
  // id; the item as it stands where it is no longer open, or another holder took it first.
  async fn take(&self, item: i64, holder: &str, frozen: &Value) -> Result<Took, String> {
    let read = self
      .client
      .get(format!("{}/items/{item}", self.url))
      .bearer_auth(self.token)
      .send()
      .await
      .map_err(|why| why.to_string())?;

    if !read.status().is_success() {
      return Err(format!("the item could not be read: {}", read.status()));
    }

    let held: Value = read.json().await.map_err(|why| why.to_string())?;

    if held["state"].as_str() != Some("open") {
      return Ok(Took::Moved(held));
    }

    let mut asked = json!({
      "queueName": held["queueName"],
      "itemId": item,
      "holderName": holder,
      "ttlMs": JOB_LEASES.ttl_ms,
      "frozen": frozen,
    });

    // An item with an assignee is taken only by a lease naming that assignee.
    if held["assigneeName"].as_str().is_some_and(|assignee| !assignee.is_empty()) {
      asked["assigneeName"] = held["assigneeName"].clone();
    }

    let answer = self
      .client
      .post(format!("{}/leases", self.url))
      .bearer_auth(self.token)
      .json(&asked)
      .send()
      .await
      .map_err(|why| why.to_string())?;
    let status = answer.status();
    let taken: Value = answer.json().await.unwrap_or(Value::Null);

    match taken["id"].as_i64() {
      Some(lease) if status.is_success() => Ok(Took::Leased(lease)),
      // Another holder took the item first: it is leased, but not to this job.
      _ if status == StatusCode::CONFLICT => {
        let mut moved = held;

        moved["state"] = json!("leased");
        Ok(Took::Moved(moved))
      }
      _ => Err(format!("the job service answered {status} to the lease: {}", taken["error"])),
    }
  }
}

// What the pass tells an item's orderer, and the marks it keeps so each is told once.
struct Tells<'a> {
  db: &'a DatabaseConnection,
  vars: &'a BTreeMap<String, String>,
  service: JobService,
  /// Whether the deployment names a receiver: where it names none, nothing is told.
  receiver: bool,
}

impl Tells<'_> {
  async fn mark(&self, key: &str) -> Option<String> {
    settings::get(self.db, key).await.ok().flatten()
  }

  async fn set(&self, key: &str, value: &str) {
    if let Err(why) = settings::set(self.db, key, value).await {
      eprintln!("jobs' leases: the mark {key} was not kept: {}", why.detail);
    }
  }

  // An item that moved under a running job, owed to its orderer once for that job and that state, and for a
  // merge to the carrying item's orderer too where it answers another room, under a key of that room's.
  async fn moved(&self, job: i64, worker: &str, item: &Value) {
    if !self.receiver {
      return;
    }

    let key = format!("{MOVED_TOLD_PREFIX}{job}");
    let state = job_notices::moved_state(item);

    if self.mark(&key).await.as_deref() == Some(state.as_str()) {
      return;
    }

    let owed = async {
      let orderer = self.service.orderer_of(item).await?;
      let notice = Notice::instruction(job_notices::moved(item, job, worker, &orderer), Keep::Once);

      outbox::owe(self.db, self.vars, notice).await?;

      if let Some(into) = item["mergedInto"].as_i64() {
        let (_, carrying) = self.service.orderer_of_id(into).await?;

        if carrying.room != orderer.room {
          let notice = Notice::instruction(job_notices::moved(item, job, worker, &carrying), Keep::Once);
          let key =
            format!("{}@{}", notice.key, carrying.room.map_or("owner".to_owned(), |room| room.to_string()));

          outbox::owe(self.db, self.vars, notice.keyed(key)).await?;
        }
      }

      Ok::<_, String>(())
    };

    match owed.await {
      Ok(()) => {
        self.set(&key, &state).await;
        println!("job {job}: its item moved to {state} under it, owed to its orderer");
      }
      Err(why) => eprintln!("job {job}: its item's move was not owed: {why}"),
    }
  }

  // An ended job's item still leased: marked at the first pass that finds it so, and read at each pass after it.
  // An item whose commit waits for its repository's batch deployment is told by the notice of that commit, so a
  // notice of it told before is let go; any other is owed to its orderer once for the lease and kept.
  async fn unclosed(&self, job: i64, item: i64, lease: i64) {
    if !self.receiver {
      return;
    }

    let key = format!("{CLOSE_PREFIX}{job}");

    if self.mark(&key).await.is_none() {
      self.set(&key, "seen").await;
      return;
    }

    let owed_key = format!("{CLOSE_OWED}{item}");
    let lease_mark = lease.to_string();
    let told = async {
      let owed = outbox::owed(self.db, &owed_key).await?.remove("");
      let read = self.service.item(item).await?;

      // An item whose notices are switched off is told no more of its close (docs/agent-control.md, 「An item's
      // notices switched off」), as one whose commit waits for its deployment is not.
      if read
        .as_ref()
        .is_some_and(|read| merged_awaiting_deploy(read) || crate::watches::queue_idle::notices_off(read))
      {
        if let Some(owed) = owed {
          outbox::let_go_as_read(self.db, self.vars, &owed_key, &owed).await?;
        }

        return Ok(false);
      }

      if owed.is_some_and(|owed| owed.mark == lease_mark) {
        return Ok(false);
      }

      let orderer = match &read {
        Some(read) => self.service.orderer_of(read).await?,
        None => Orderer::default(),
      };
      let notice = Notice::instruction(job_notices::awaits_close(item, lease, &orderer), Keep::Kept);

      outbox::owe(self.db, self.vars, notice.marked(lease_mark)).await?;

      Ok::<_, String>(true)
    };

    match told.await {
      Ok(true) => println!("job {job}: item #{item} still leased after its end, owed to its orderer"),
      Ok(false) => {}
      Err(why) => eprintln!("job {job}: item #{item} awaiting its close was not owed: {why}"),
    }
  }

  // The marks of jobs that moved on let go: a moved item's once its job no longer runs, a close's once its job's
  // lease is no longer kept, and with it the notice of its item, where no other ended job keeps that item's
  // lease.
  async fn let_go(&self, running: &BTreeSet<i64>, ended: &BTreeSet<i64>, unclosed: &BTreeSet<i64>) {
    match outbox::owed(self.db, CLOSE_OWED).await {
      Ok(owed) => {
        for (item, row) in owed {
          if item.parse::<i64>().is_ok_and(|item| unclosed.contains(&item)) {
            continue;
          }

          let key = format!("{CLOSE_OWED}{item}");

          if let Err(why) = outbox::let_go_as_read(self.db, self.vars, &key, &row).await {
            eprintln!("jobs' leases: the notice {key} was not let go: {why}");
          }
        }
      }
      Err(why) => eprintln!("jobs' leases: the notices of unclosed items were not read: {why}"),
    }

    for (prefix, standing) in [(MOVED_TOLD_PREFIX, running), (CLOSE_PREFIX, ended)] {
      let Ok(marked) = settings::numbered(self.db, prefix).await else { continue };

      for job in marked.into_iter().filter(|job| !standing.contains(job)) {
        if let Err(why) = settings::forget(self.db, &format!("{prefix}{job}")).await {
          eprintln!("jobs' leases: the mark {prefix}{job} was not let go: {}", why.detail);
        }
      }
    }
  }
}
