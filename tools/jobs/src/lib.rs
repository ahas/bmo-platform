//! `jobs`: the supervisor's own filing, stopping and reviewing of work, served by the tool host for a supervisor
//! whose program stands on a node, which holds neither agency's address nor its token (docs/tools.md, 「The job
//! tools on a node」). Each call is one route of agency's server, asked at home for the supervisor the session's
//! record is held by, as the supervisor's own server asks it for the agent it was started for.

use bmo_agency_client::{
  AgencyClient, ClientFailure, FILE_ITEM, FileItem, ItemFilingAsk, JobReviewAsk, JobStopAsk, REVIEW_JOB,
  ReviewJob, STOP_JOB, StopJob,
};
use serde_json::{Value, json};
use tool_call::{Answer, Asked, Control, Handed, Session};

/// The namespace the entries are shown under, the name the supervisor's own server stands under, so a program
/// calls each by the same name wherever it stands.
pub const NAMESPACE: &str = "agency";

/// The capability a session must hold to be shown them, the one the chat line's filing servers stand under.
pub const CAPABILITY: &str = "jobs.file";

/// One entry: its name, title and description, and its input's schema file beside the manifest.
pub struct Entry {
  pub name: &'static str,
  pub title: &'static str,
  pub description: &'static str,
  pub input: &'static str,
}

/// The entries the manifest declares, in its order.
pub const ENTRIES: [Entry; 3] = [
  Entry {
    name: "file_item",
    title: "File an item",
    description: FILE_ITEM,
    input: "file_item.json",
  },
  Entry {
    name: "stop_job",
    title: "Stop a job",
    description: STOP_JOB,
    input: "stop_job.json",
  },
  Entry {
    name: "review_job",
    title: "Review a job",
    description: REVIEW_JOB,
    input: "review_job.json",
  },
];

/// An entry's full name.
pub fn full_name(name: &str) -> String {
  format!("{NAMESPACE}/{name}")
}

/// One call against agency's server as the record's data root describes it.
pub async fn call(session: &Session, entry: &str, arguments: Value) -> Answer {
  let control = Control::read(&session.data_root(), |name| std::env::var(name).ok());

  call_with(&control, session, entry, arguments).await
}

/// The same call against the server the caller names, for a test of the refusals and the round trip.
pub async fn call_with(control: &Control, session: &Session, entry: &str, arguments: Value) -> Answer {
  // The supervisor the record is held by is the agent the work is filed, stopped and reviewed for: agency writes
  // the holder of every record it issues, and no argument names another.
  let agent = session.holder_name.trim();

  if agent.is_empty() {
    return Answer::failure(
      "this session is held by no supervisor, so nothing is filed, stopped or reviewed for it",
    );
  }

  let client = match AgencyClient::new(&control.base(), control.token.clone().unwrap_or_default()) {
    Ok(client) => client,
    Err(why) => return Answer::failure(format!("agency's server cannot be named: {why}")),
  };
  let agent = agent.to_owned();
  let answered = match entry.strip_prefix(&format!("{NAMESPACE}/")) {
    Some("file_item") => match read::<FileItem>(arguments) {
      Ok(item) => {
        let ask = ItemFilingAsk {
          agent_name: agent,
          title: item.title,
          brief: item.brief,
          queue_name: item.queue_name,
          repository: item.repository,
          priority: item.priority,
          assignee_name: item.assignee_name,
          requester_name: item.requester_name,
          labels: item.labels,
          steps: item.steps,
          steps_check: item.steps_check,
        };

        rendered(client.create_item_filings(&ask).await)
      }
      Err(refused) => return refused,
    },
    Some("stop_job") => match read::<StopJob>(arguments) {
      Ok(stop) => {
        let ask = JobStopAsk {
          agent_name: agent,
          job_id: stop.job_id,
          item_id: stop.item_id,
          reason: stop.reason,
        };

        rendered(client.create_job_stops(&ask).await)
      }
      Err(refused) => return refused,
    },
    Some("review_job") => match read::<ReviewJob>(arguments) {
      Ok(review) => {
        let ask = JobReviewAsk {
          agent_name: agent,
          job_id: review.job_id,
          action: review.action,
          text: review.text,
          stage: review.stage,
        };

        rendered(client.create_job_reviews(&ask).await)
      }
      Err(refused) => return refused,
    },
    _ => return Answer::failure(format!("this executable holds no entry {entry}")),
  };

  match answered {
    Ok(text) => Answer::text(text),
    Err(failure) => Answer::failure(refusal(&failure)),
  }
}

// The arguments as the entry takes them, or a failed call saying why they do not fit.
fn read<T: serde::de::DeserializeOwned>(arguments: Value) -> Result<T, Answer> {
  serde_json::from_value(arguments)
    .map_err(|why| Answer::failure(format!("the arguments are not the shape this tool takes: {why}")))
}

// The route's answer as the text a call answers, the JSON it gave.
fn rendered<T: serde::Serialize>(answered: Result<T, ClientFailure>) -> Result<String, ClientFailure> {
  answered.map(|answer| serde_json::to_string(&answer).unwrap_or_default())
}

// A refusal as the supervisor's own server words it: the route's status, code and sentence.
fn refusal(failure: &ClientFailure) -> String {
  match failure {
    ClientFailure::Answered {
      status,
      answer: Some(answer),
    } => json!({ "status": status, "code": answer.code, "error": answer.error }).to_string(),
    ClientFailure::Answered { status, .. } => {
      json!({ "status": status, "error": failure.to_string() }).to_string()
    }
    other => json!({ "error": other.to_string() }).to_string(),
  }
}

pub async fn answer(handed: Handed) -> Result<Value, String> {
  let answered = match handed.asked {
    Asked::Call { entry, arguments } => call(&handed.session, &entry, arguments).await,
    Asked::ResourcesList => return Ok(json!({ "resources": [] })),
    Asked::ResourcesRead { .. } => return Ok(json!({ "error": "this executable serves no resources" })),
    Asked::Other => return Err("this executable answers calls alone".to_owned()),
  };

  serde_json::to_value(answered).map_err(|why| format!("the answer could not be written: {why}"))
}
