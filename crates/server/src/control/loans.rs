//! Checkouts lent to a job (docs/worker-slots.md, 「A checkout lent to a job」, 「Loans across a job's review」):
//! which checkout of a repository a job is lent, held in its name, made where it is missing and put on the job's
//! branch; the loans a launch takes together or waits for; and the words the job's prompt names them in. Agency
//! lends and gives back; what the job does in a lent checkout, and how it is closed, are the job's and the close's.

use axum::http::StatusCode;
use policy::fill_marks;
use policy::job_prompt::JOB_PROMPT;
use policy::workers::WORKERS;
use store::{all, one, run};

use super::Refused;
use super::work::{called_off, hold, hold_job, noted_line};
use crate::places::Ground;
use crate::places::loan_branch;
use crate::queue::holds::{JOB_HOLDER, is_free, release_ended};
use crate::queue::workers::{Worker, workers};

/// The events a wait for a checkout to lend looks again on: a job or an assignment changed.
const LOAN_MOVES: [&str; 5] =
  ["job.changed", "job.ended", "job.created", "assignment.changed", "assignment.created"];

fn refused(status: StatusCode, code: &str, error: String) -> Refused {
  Refused::new(status, error).with_code(code)
}

/// The repository's checkouts on the machine the job runs on, in the order a loan takes them: support worktrees
/// by number, then the base.
pub fn candidates<'a>(known: &'a [Worker], repository: &str, node: Option<&str>) -> Vec<&'a Worker> {
  let of = |checkout: &&Worker| {
    (checkout.name == repository || checkout.support_of.as_deref() == Some(repository))
      && checkout.node.as_deref() == node
      && !checkout.builtin
  };
  let support = known.iter().filter(of).filter(|checkout| checkout.is_support());
  let base = known.iter().filter(of).filter(|checkout| !checkout.is_support());

  support.chain(base).collect()
}

/// A checkout lent to a job: the repository, the checkout, the assignment held in the job's name, and whether this
/// lending took it rather than finding it held already.
#[derive(Clone, Debug)]
pub struct Lent {
  pub repository: String,
  pub checkout: Worker,
  pub assignment_id: i64,
  pub fresh: bool,
}

/// What asking for one repository's checkout came to: lent, or every checkout of it held, with the sentence
/// saying so.
pub enum Lending {
  Lent(Lent),
  Busy(String),
}

/// The job a checkout is lent to: its number, its item, its own checkout and the node that stands on.
pub struct Borrower {
  pub id: i64,
  pub item: i64,
  pub own: String,
  pub node: Option<String>,
}

impl Borrower {
  /// The job of this number, as its row names it; none where no job is.
  pub async fn of_job(ground: &Ground, id: i64) -> Result<Option<Self>, Refused> {
    let Some(row) = one(
      &ground.db,
      r#"SELECT "item_id", "worker_name" FROM "jobs" WHERE "id" = $1"#,
      vec![id.into()],
    )
    .await?
    else {
      return Ok(None);
    };
    let own: String = row.try_get("", "worker_name")?;
    let known = workers(&ground.db, &ground.source).await?;
    let node = known
      .iter()
      .find(|checkout| checkout.name == own)
      .and_then(|checkout| checkout.node.clone());

    Ok(Some(Self {
      id,
      item: row.try_get("", "item_id")?,
      own,
      node,
    }))
  }

  /// The holder name its loans stand under.
  pub fn holder(&self) -> String {
    format!("{JOB_HOLDER}{}", self.id)
  }

  /// The branch its lent checkouts stand on.
  pub fn branch(&self) -> String {
    loan_branch::job_branch(self.item)
  }

  // The repository its own checkout is of.
  fn own_repository<'a>(&'a self, known: &'a [Worker]) -> &'a str {
    known
      .iter()
      .find(|checkout| checkout.name == self.own)
      .map_or(self.own.as_str(), Worker::repository)
  }

  // The checkouts its loans hold now, by name.
  async fn held(&self, ground: &Ground) -> Result<Vec<(String, i64)>, Refused> {
    all(
      &ground.db,
      r#"SELECT "id", "worker_name" FROM "assignments" WHERE "holder_name" = $1 AND "state" = 'held'"#,
      vec![self.holder().into()],
    )
    .await?
    .iter()
    .map(|row| Ok((row.try_get("", "worker_name")?, row.try_get("", "id")?)))
    .collect()
  }

  /// A checkout of the repository lent to the job: the one the job already holds; else, on the job's machine, the
  /// one standing on the job's branch where one does, a free support worktree, the base, or one grown where none is
  /// free; made where it is missing. Busy where none can be had now; refused for the job's own repository, a
  /// repository with no checkout on the job's machine, and a loan past the job's bound of checkouts.
  pub async fn lend(
    &self,
    ground: &Ground,
    known: &[Worker],
    repository: &str,
  ) -> Result<Lending, Refused> {
    if self.own_repository(known) == repository {
      return Err(refused(
        StatusCode::UNPROCESSABLE_ENTITY,
        "own-repository",
        format!("{repository} is this job's own repository; work in this job's own checkout"),
      ));
    }

    let members = candidates(known, repository, self.node.as_deref());

    if members.is_empty() {
      return Err(refused(
        StatusCode::UNPROCESSABLE_ENTITY,
        "no-checkout-here",
        format!("{repository} is no repository with a checkout on this job's machine"),
      ));
    }

    let held = self.held(ground).await?;

    if let Some(checkout) = members.iter().find(|member| held.iter().any(|(name, _)| *name == member.name))
    {
      let assignment_id = held.iter().find(|(name, _)| *name == checkout.name).map_or(0, |(_, id)| *id);

      return Ok(Lending::Lent(Lent {
        repository: repository.to_owned(),
        checkout: (*checkout).clone(),
        assignment_id,
        fresh: false,
      }));
    }

    if held.len() + 2 > WORKERS.job_checkouts_most {
      return Err(too_many(WORKERS.job_checkouts_most));
    }

    let on_branch = self.standing_on_branch(&members).await;
    let order: Vec<&Worker> = match on_branch.as_ref() {
      Some(name) => members.iter().copied().filter(|member| member.name == *name).collect(),
      None => members.clone(),
    };
    let taken_up = somebody_s_work(ground, repository, &order).await?;
    let order: Vec<&Worker> = order.into_iter().filter(|member| !taken_up.contains(&member.name)).collect();
    let holder = self.holder();

    for member in &order {
      if is_free(&ground.db, &member.name).await? {
        let assignment =
          hold(ground, self.item, &holder, &member.name, crate::agent_roles::job_role()).await?;
        let assignment_id = assignment["id"].as_i64().unwrap_or_default();

        made(ground, member, assignment_id).await?;

        return Ok(Lending::Lent(Lent {
          repository: repository.to_owned(),
          checkout: (*member).clone(),
          assignment_id,
          fresh: true,
        }));
      }
    }

    // A repository that grows grows one on this machine, bounded by the machine's room alone (docs/worker-slots.md,
    // 「Growing a checkout at a launch」); none grows for a branch already standing in one of its checkouts.
    if on_branch.is_none()
      && self.node.is_none()
      && let Some(of) = crate::repositories::named(&ground.db, repository).await?
    {
      let holding = super::support_growth::Holding {
        item_id: self.item,
        holder: &holder,
        role: crate::agent_roles::job_role(),
      };

      match super::support_growth::grow(ground, &of, None, holding).await {
        Ok(Some(grown)) => {
          let assignment_id = grown.assignment["id"].as_i64().unwrap_or_default();

          made(ground, &grown.checkout, assignment_id).await?;

          return Ok(Lending::Lent(Lent {
            repository: repository.to_owned(),
            checkout: grown.checkout,
            assignment_id,
            fresh: true,
          }));
        }
        Ok(None) => {}
        Err(why) if why.code.as_deref() == Some("no-worker-free") => return Ok(Lending::Busy(why.error)),
        Err(why) => return Err(why),
      }
    }

    let names: Vec<&str> = order.iter().map(|member| member.name.as_str()).collect();

    Ok(Lending::Busy(format!(
      "every checkout of {repository} this job could take is at work ({})",
      names.join(", ")
    )))
  }

  // The checkout of these that has the job's branch checked out, where one on this machine has: the only one git
  // lets take the branch.
  async fn standing_on_branch(&self, members: &[&Worker]) -> Option<String> {
    let branch = self.branch();
    let here: Vec<(String, String)> = members
      .iter()
      .filter(|member| member.node.is_none() && !member.cwd.is_empty())
      .map(|member| (member.name.clone(), member.cwd.clone()))
      .collect();

    tokio::task::spawn_blocking(move || {
      here
        .into_iter()
        .find(|(_, cwd)| loan_branch::standing_branch(cwd).as_deref() == Some(branch.as_str()))
        .map(|(name, _)| name)
    })
    .await
    .ok()
    .flatten()
  }

  /// The lent checkout put on the job's branch, on this machine; one on a work node, and a directory that is no
  /// git checkout, is lent as it stands. A checkout that cannot be put there is given back, with what git said.
  pub async fn place(&self, ground: &Ground, lent: &Lent) -> Result<(), Refused> {
    if lent.checkout.node.is_some() || !std::path::Path::new(&lent.checkout.cwd).join(".git").exists() {
      return Ok(());
    }

    let base = crate::repositories::named(&ground.db, &lent.repository)
      .await?
      .map(|repository| repository.path)
      .unwrap_or_else(|| lent.checkout.cwd.clone());
    let bun = ground
      .machine()
      .program("bun")
      .map_or_else(|| "bun".to_owned(), |path| path.to_string_lossy().into_owned());
    let (cwd, branch) = (lent.checkout.cwd.clone(), self.branch());
    let placed =
      tokio::task::spawn_blocking(move || loan_branch::put_on_branch(&cwd, &base, &branch, &bun))
        .await
        .map_err(Refused::failed)?;

    match placed {
      Ok(_) => Ok(()),
      Err(why) => {
        give_up(ground, lent.assignment_id).await?;

        Err(refused(StatusCode::INTERNAL_SERVER_ERROR, "checkout-not-placed", why))
      }
    }
  }
}

// The checkouts of these, on this machine, standing on main with something open: somebody's work, which no loan
// takes (docs/worker-slots.md, 「A checkout lent to a job」).
async fn somebody_s_work(
  ground: &Ground,
  repository: &str,
  members: &[&Worker],
) -> Result<Vec<String>, Refused> {
  let Some(base) = crate::repositories::named(&ground.db, repository)
    .await?
    .map(|repository| repository.path)
  else {
    return Ok(vec![]);
  };
  let here: Vec<(String, String)> = members
    .iter()
    .filter(|member| member.node.is_none() && std::path::Path::new(&member.cwd).join(".git").exists())
    .map(|member| (member.name.clone(), member.cwd.clone()))
    .collect();

  tokio::task::spawn_blocking(move || {
    here
      .into_iter()
      .filter(|(_, cwd)| loan_branch::open_on_main(cwd, &base))
      .map(|(name, _)| name)
      .collect()
  })
  .await
  .map_err(Refused::failed)
}

/// What refuses a job more checkouts than its bound.
pub fn too_many(most: usize) -> Refused {
  refused(
    StatusCode::UNPROCESSABLE_ENTITY,
    "too-many-checkouts",
    format!(
      "one job holds at most {most} checkouts, its own among them (policy/workers.toml, job_checkouts_most)"
    ),
  )
}

// A loan let go at once, as one this lending took and cannot keep.
async fn give_up(ground: &Ground, assignment_id: i64) -> Result<(), Refused> {
  run(
    &ground.db,
    r#"UPDATE "assignments" SET "state" = 'released', "changed_at" = $1 WHERE "id" = $2 AND "state" = 'held'"#,
    vec![crate::now_ms().into(), assignment_id.into()],
  )
  .await?;

  Ok(())
}

// The lent checkout's worktree made where it is missing, as a launch makes it (docs/worker-slots.md, 「Names and
// places」); a worktree that cannot be made lets the loan go, refused with why.
async fn made(ground: &Ground, checkout: &Worker, assignment_id: i64) -> Result<(), Refused> {
  let Err(why) = crate::places::checkouts::ensure_support_checkout(ground, &checkout.name).await else {
    return Ok(());
  };

  give_up(ground, assignment_id).await?;

  Err(refused(StatusCode::INTERNAL_SERVER_ERROR, "checkout-not-made", why))
}

/// Waits for a job or an assignment to change, or for the policy's look to come round, whichever is first.
pub async fn next_look(moved: &mut crate::events::WireSubscription) {
  let look = std::time::Duration::from_secs(WORKERS.loan_look_seconds);

  if let Ok(None) = tokio::time::timeout(look, moved.next()).await {
    tokio::time::sleep(look).await;
  }
}

/// A watch on the changes a wait for a checkout looks again on.
pub fn loan_watch(ground: &Ground) -> crate::events::WireSubscription {
  ground
    .events
    .subscribe_wire(Some(LOAN_MOVES.iter().map(|kind| (*kind).to_owned()).collect()))
}

/// The repositories a job is to be lent: those its item named at its launch, then every other it held a loan of,
/// each once.
pub async fn wanted(ground: &Ground, id: i64) -> Result<Vec<String>, Refused> {
  let named = one(
    &ground.db,
    r#"SELECT "named_repositories" FROM "jobs" WHERE "id" = $1"#,
    vec![id.into()],
  )
  .await?
  .and_then(|row| row.try_get::<Option<String>>("", "named_repositories").ok().flatten())
  .unwrap_or_default();
  let mut wanted: Vec<String> = named
    .split(',')
    .map(str::trim)
    .filter(|name| !name.is_empty())
    .map(str::to_owned)
    .collect();
  let known = workers(&ground.db, &ground.source).await?;

  for row in all(
    &ground.db,
    r#"SELECT DISTINCT "worker_name" FROM "assignments" WHERE "holder_name" = $1"#,
    vec![format!("{JOB_HOLDER}{id}").into()],
  )
  .await?
  {
    let worker: String = row.try_get("", "worker_name")?;

    if let Some(checkout) = known.iter().find(|checkout| checkout.name == worker) {
      let repository = checkout.repository().to_owned();

      if !wanted.contains(&repository) {
        wanted.push(repository);
      }
    }
  }

  Ok(wanted)
}

/// Every repository the job is to be lent, lent together before its turn (docs/worker-slots.md, 「A checkout lent
/// to a job」): where any has no checkout to give, what the pass took is given back and the job waits, its
/// `heldFor` naming them and its orderer told once, looking again as jobs and assignments change, with no clock;
/// then each it took is put on the job's branch. None where the job was stopped while it waited; an error where a
/// loan was refused, which fails the job.
pub async fn lend_job(ground: &Ground, id: i64) -> Result<Option<Vec<Lent>>, String> {
  let wanted = wanted(ground, id).await.map_err(|why| why.error)?;

  if wanted.is_empty() {
    return Ok(Some(vec![]));
  }

  let borrower = Borrower::of_job(ground, id)
    .await
    .map_err(|why| why.error)?
    .ok_or_else(|| format!("job {id} is gone"))?;
  let mut moved = loan_watch(ground);
  let mut told = false;
  let mut wanted = wanted;
  let taken = loop {
    release_ended(&ground.db).await.map_err(|why| why.detail)?;

    let known = workers(&ground.db, &ground.source).await.map_err(|why| why.detail)?;
    let mut taken: Vec<Lent> = vec![];
    let mut busy = None;

    // The pass stops at the first repository with nothing to give, which the next pass asks first, so a wait
    // takes nothing while that one stays busy.
    for (at, repository) in wanted.iter().enumerate() {
      match borrower.lend(ground, &known, repository).await {
        Ok(Lending::Lent(lent)) => taken.push(lent),
        Ok(Lending::Busy(_)) => {
          busy = Some(at);
          break;
        }
        // The job's own repository named in its own list is simply its own checkout.
        Err(why) if why.code.as_deref() == Some("own-repository") => {}
        Err(why) => {
          given_back(ground, &taken).await;

          return Err(why.error);
        }
      }
    }

    let Some(at) = busy else {
      break taken;
    };

    given_back(ground, &taken).await;

    let waits_on = wanted.remove(at);

    wanted.insert(0, waits_on.clone());

    if !told {
      told = true;

      let why = format!("it waits for a free checkout of {waits_on} to be lent");

      hold_job(ground, id, Some(&why), true).await;
      noted_line(ground, id, &format!("Waiting for a free checkout of {waits_on}")).await;
    }

    if called_off(ground, id).await {
      return Ok(None);
    }

    next_look(&mut moved).await;
  };

  if told {
    hold_job(ground, id, None, false).await;
  }

  // A loan the job held already stands where the job left it; only one this pass took is put on its branch.
  for lent in taken.iter().filter(|lent| lent.fresh) {
    if let Err(why) = borrower.place(ground, lent).await {
      given_back(ground, &taken).await;

      return Err(why.error);
    }
  }

  if !taken.is_empty() {
    let names: Vec<String> = taken
      .iter()
      .map(|lent| format!("{} at {}", lent.repository, lent.checkout.cwd))
      .collect();

    noted_line(ground, id, &format!("Lent {}", names.join(", "))).await;
  }

  Ok(Some(taken))
}

// The loans this pass took itself, let go again.
async fn given_back(ground: &Ground, taken: &[Lent]) {
  for lent in taken.iter().filter(|lent| lent.fresh) {
    if let Err(why) = give_up(ground, lent.assignment_id).await {
      eprintln!("loan {}: not let go: {}", lent.assignment_id, why.error);
    }
  }
}

/// The section a job's prompt names its lent checkouts in, under its title; nothing where none is lent.
pub fn lent_words(taken: &[Lent], branch: &str) -> String {
  if taken.is_empty() {
    return String::new();
  }

  let words = &JOB_PROMPT.lent;
  let mut text = format!("{}\n\n{}\n\n", words.heading, words.lead);

  for lent in taken {
    text.push_str(&fill_marks(
      &words.line,
      &[("repository", &lent.repository), ("path", &lent.checkout.cwd), ("branch", branch)],
    ));
    text.push('\n');
  }

  text
}
