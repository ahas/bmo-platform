//! What an item's orderer and the owner are told of a change in the queue (docs/agent-control.md, 「An item's
//! change is told to its orderer」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobNotices {
  /// How many items up the chain an item was raised from are read to find its orderer.
  pub orderer_depth: usize,
  /// The word a launch notice names where the item goes to any free checkout.
  pub any_worker: String,
  /// An open item in a queue that gives no lease: `{item}`, `{title}` and `{queue}`.
  pub paused_order: String,
  /// An open item whose assignee no checkout is of: `{item}`, `{title}` and `{assignee}`.
  pub unlaunchable_order: String,
  /// An item whose `agent` label no launch takes: `{item}`, `{title}`, `{label}` and `{why}`.
  pub agent_order: String,
  /// An item whose `role` label names no role agency holds: `{item}`, `{title}`, `{label}` and `{roles}`.
  pub role_order: String,
  /// An open item whose model no agent of its role runs: `{item}`, `{title}`, `{model}`, `{role}` and
  /// `{agents}`.
  pub no_candidate_order: String,
  /// An item moved while its job runs: `{item}`, `{title}`, `{state}`, `{merged}`, `{job}` and `{worker}`.
  pub moved_order: String,
  /// An item closed by somebody else: `{item}`, `{title}` and `{by}`.
  pub closed_order: String,
  /// The labels a held item names the items it waits on with.
  pub wait_labels: Vec<String>,
  /// A held item whose every item it waits on has ended: `{item}`, `{title}`, `{label}` and `{waited}`.
  pub wait_ended_order: String,
  /// One item a held item waited on, as `{waited}` lists it: `{item}`, `{title}` and `{state}`.
  pub waited_item: String,
  /// An item whose closes merged commits that wait for their repositories' batch deployments: `{item}`,
  /// `{title}` and `{merged}`, each waiting commit as `merged_commit` gives it.
  pub deploy_order: String,
  /// A deployment that ended without going live: `{repository}`, `{commit}`, `{unit}`, `{outcome}`, `{stage}`,
  /// `{reason}`, `{items}` and `{journal}`.
  pub deploy_failed_order: String,
  /// One waiting item a failed deployment carries, as `{items}` lists it: `{item}` and `{title}`.
  pub deploy_failed_item: String,
  /// What `{items}` reads where the deployment carries no waiting item of the room.
  pub deploy_failed_no_items: String,
  /// The outcome a run cut off with none written is told as.
  pub deploy_cut_off: String,
  /// What `{stage}` reads where the run entered no stage.
  pub deploy_no_stage: String,
  /// What `{reason}` reads where the run gave none.
  pub deploy_no_reason: String,
  /// How often the deploy roots' progress records are read, in seconds.
  pub deploy_read_seconds: u64,
  /// How many of the last lines of a failed run's unit journal the order carries.
  pub deploy_journal_lines: usize,
  /// One waiting commit, as `{merged}` lists it: `{commit}` and `{repository}`.
  pub merged_commit: String,
  /// How many minutes pass between two flushes of the deploy queue, unless the owner's own settings row names
  /// another.
  pub deploy_flush_minutes: i64,
  /// How often the clock is read for the deploy queue's flush, in seconds.
  pub deploy_flush_read_seconds: u64,
  /// The reason a flushed deployment is made with: `{items}`. Its words before `{items}` mark a deployment as the
  /// flush's in the repository's deployment record.
  pub deploy_flush_why: String,
  /// One item whose commit waits, as `{items}` lists it: `{item}`.
  pub deploy_flush_item: String,
  /// A flushed deployment that went live: `{repository}`, `{commit}`, `{unit}` and `{items}`.
  pub deploy_live_order: String,
  /// Commits waiting for a repository no deployment of this machine puts live: `{repository}`, `{why}` and
  /// `{items}`, each as `deploy_stalled_item` gives it.
  pub deploy_undeployable_order: String,
  /// How many minutes a commit may wait for its repository's batch deployment before it is told as waiting too
  /// long.
  pub deploy_stale_minutes: i64,
  /// Commits waiting too long: `{repository}`, `{why}`, `{hours}` and `{items}`, each as `deploy_stalled_item`
  /// gives it.
  pub deploy_stale_order: String,
  /// One waiting item, as a stalled notice lists it: `{item}`, `{title}` and `{commit}`.
  pub deploy_stalled_item: String,
  /// Why a waiting commit is not going live: agency holds no checkout of its repository here.
  pub deploy_why_no_checkout: String,
  /// Its repository's declaration places no deployment: `{said}`, the testbed's refusal.
  pub deploy_why_no_root: String,
  /// Its repository's main could not be read: `{said}`.
  pub deploy_why_main_unread: String,
  /// Its repository's last run failed at main's commit: `{commit}`, `{unit}`, `{outcome}` and `{reason}`.
  pub deploy_why_failed_at: String,
  /// The testbed refused the deployment the flush asked: `{said}`.
  pub deploy_why_refused: String,
  /// The flush asks its deployment, and none has put the commit live yet.
  pub deploy_why_asked: String,
  /// A run's question: `{question}`, `{item}`, `{text}` and `{choices}`.
  pub question_order: String,
  /// A job's question the job service put to the owner once the supervisor's minute on it passed: the same marks.
  pub question_handed_order: String,
  /// A job waiting for a road or a slot: `{item}`, `{worker}` and `{why}`.
  pub waiting_order: String,
  /// An ended job's item still leased: `{item}` and `{lease}`.
  pub close_order: String,
  /// A job's turn that has made no progress for its stall span: `{job}`, `{item}`, `{worker}`, `{minutes}`,
  /// `{since}` and `{waiting}`, what it waits on.
  pub stalled_order: String,
  /// One running call a stalled turn waits on: `{tool}`, its tool and command, and `{lines}`, its last lines.
  pub stalled_call: String,
  /// What a stalled turn waits on where no call runs.
  pub stalled_no_call: String,
  /// The last lines a stalled call is told with where it has written none.
  pub stalled_no_lines: String,
  /// A person's question past its due time: `{question}`, `{item}`, `{asker}`, `{text}` and `{choices}`.
  pub question_due_order: String,
  /// A question settled on its recommendation once its due time passed: `{question}`, `{item}`, `{answer}`
  /// and `{why}`.
  pub question_settled_order: String,
  /// The item a settled question names, as `{item}` of `question_settled_order` gives it: `{item}`.
  pub question_settled_item: String,
  /// A job that failed in another room, told in the owner's: `{job}`, `{item}`, `{worker}`, `{outcome}` and
  /// `{room}`.
  pub failed_order: String,
  /// How often agency's sender sends the notices due, in seconds.
  pub send_seconds: u64,
  /// The wait after a notice's first failed try, in seconds, doubled after each try that fails after it.
  pub retry_first_seconds: u64,
  /// The longest wait between two tries of a notice, in seconds.
  pub retry_most_seconds: u64,
  /// How long a standing notice waits after it was last told before it is told again, in seconds.
  pub retell_seconds: u64,
  /// How long one try waits for the receiver's answer, in seconds; a notice is held to one sender for twice it.
  pub answer_seconds: u64,
  /// How long after a job's end, a check of its review coming due or its wait beginning, the sender waits before
  /// it owes a notice of it not yet owed itself, in seconds.
  pub owe_grace_seconds: u64,
}

crate::part!(pub JOB_NOTICES: JobNotices = "job-notices.toml");
