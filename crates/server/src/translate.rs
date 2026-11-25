//! The translate runner (docs/launch-lines.md, 「Translate runner」): one text translated on the road the road
//! resolution answers for the `translate` key — one request through the model gateway on the registry's
//! provider, a run apart on Claude Code — under the translator role's prompt and the instructions of the
//! purpose asked.

use policy::roles::ROLES;
use std::sync::{Arc, LazyLock};

use bmo_agency_client::TranslationTerm;
use harness_providers::registered::Registered;
use serde_json::{Value, json};

use crate::cards;
use crate::control::claude_accounts::Account;
use crate::door::apart::{self, Apart, answered, new_session};
use crate::door::turn::Watch;
use crate::job_run::claude::failure::Failure;
use crate::job_run::claude::renewal::Renewing;
use crate::job_run::claude::walk::{Attempt, Walking, on_accounts};
use crate::lines::claude::{ApartRun, Command, Runs, apart_args, command_env};
use crate::lines::worker::{effort_of, rulebook, system_prompt_file};
use crate::lines::{Trees, read_required, role_prompt};
use crate::order::Entry;
use crate::places::Ground;
use crate::road::{self, Ask};
use crate::role_agents::RoleAgent;
use crate::run_ledger;
use crate::run_log::lines::Line;
use crate::run_log::{RunLog, Target};

/// The work key a translation goes out on.
pub const KEY: &str = "translate";

/// The line a translation runs on.
pub const LINE: &str = "translate";

// The agent folder the purposes' instructions stand in, and the folder a run apart works in.
const AGENT: &str = "translator";

// The card the translations out at once on a registration share.
static TRANSLATING: LazyLock<Arc<cards::Sharing>> = LazyLock::new(Arc::default);

// What tells two translations begun in the same millisecond apart in their runs' names.
static RECORDED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// What a translation is for, each with its own instructions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
  /// A part of a document a person reads in the console's viewer.
  Document,
  /// A text a job worker reads.
  Worker,
  /// A worker's record the owner reads.
  Record,
}

impl Purpose {
  /// The purpose a caller names, where it is one.
  pub fn read(word: &str) -> Option<Self> {
    match word {
      "document" => Some(Self::Document),
      "worker" => Some(Self::Worker),
      "record" => Some(Self::Record),
      _ => None,
    }
  }

  fn instructions(self, trees: &Trees) -> Result<String, String> {
    let read = |name: &str| read_required(&trees.agent_dir(AGENT).join(name)).map_err(|why| why.detail);

    match self {
      Self::Document => read("instructions.md"),
      Self::Worker => read("worker.md"),
      Self::Record => Ok(format!(
        "{}\n\n{}",
        rulebook(trees, "plain-writing").map_err(|why| why.detail)?,
        read("record.md")?
      )),
    }
  }

  fn word(self) -> &'static str {
    match self {
      Self::Document => "document",
      Self::Worker => "worker",
      Self::Record => "record",
    }
  }
}

/// What a run on the translate runner is for: a translation for its purpose, or a shared app's review
/// (docs/launch-lines.md, 「The app review」), each under its role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Work {
  Translation(Purpose),
  Review,
}

impl Work {
  fn word(self) -> &'static str {
    match self {
      Self::Translation(purpose) => purpose.word(),
      Self::Review => "app-review",
    }
  }

  fn role(self) -> &'static String {
    match self {
      Self::Translation(_) => &ROLES.translate,
      Self::Review => &ROLES.app_review,
    }
  }
}

/// A shared app's review (docs/launch-lines.md, 「The app review」): one turn on the translate runner, on the first
/// road of the `app-reviewer` role's agents that can answer, its system prompt that role's prompt and its input the
/// app's manifest, permissions and source; answers the report.
pub async fn review_app(ground: &Ground, input: &str) -> Result<String, String> {
  let held = crate::agent_roles::named(&ground.db, &ROLES.app_review)
    .await
    .map_err(|why| why.detail)?
    .ok_or_else(|| crate::agent_roles::unknown(&ROLES.app_review))?;
  let resolution =
    road::resolve(ground, &Ask::of(road::Row::Role(&held.name), &held.line, &held.order)).await?;
  let Some(taken) = resolution.road().cloned() else {
    road::went(ground, &resolution, None, &[]).await;

    return Err(resolution.sentence());
  };

  road::went(ground, &resolution, Some(&taken), &[]).await;

  let trees = Trees::of(ground);
  let role = role_prompt(&ground.db, &ROLES.app_review, None)
    .await
    .ok_or_else(|| format!("the role {} carries no prompt", ROLES.app_review))?;
  let system = composed(&trees, Some(&role), None)?;

  if taken.provider == crate::control::registry::PROVIDER {
    recorded(ground, (Work::Review, "English"), &taken, &system, input).await
  } else if taken.provider == crate::lines::runner::PROVIDER {
    let entry = held.agent_of_road(&taken.provider, &taken.model).and_then(RoleAgent::entry);

    apart_on_claude(ground, &trees, Work::Review, (&taken.model, entry.as_ref()), &system, input).await
  } else {
    Err(format!("{} carries no review", taken.provider))
  }
}

/// A translation's system prompt: the translator role's prompt, where the store's role carries one, then the
/// purpose's instructions, as the translate line's carries order them.
pub fn system_prompt(trees: &Trees, role: Option<&str>, purpose: Purpose) -> Result<String, String> {
  composed(trees, role, Some(purpose))
}

/// A system prompt of the translate line, its carries' pieces in order (docs/launch-lines.md, 「What a line
/// carries」): the role's prompt where it names `role` and the role carries one, and the purpose's instructions
/// where it names `purpose` and the work has one — a translation has, an app's review has none. A piece the line
/// cannot fill is refused rather than left out.
pub fn composed(trees: &Trees, role: Option<&str>, purpose: Option<Purpose>) -> Result<String, String> {
  let mut parts = Vec::new();

  for piece in &policy::run_lines::RUN_LINES.carries(LINE).prompt {
    match piece.as_str() {
      "role" => parts.extend(role.map(|role| role.trim().to_owned())),
      "purpose" => {
        if let Some(purpose) = purpose {
          parts.push(purpose.instructions(trees)?);
        }
      }
      other => return Err(format!("the translate line's system prompt holds no piece named {other}")),
    }
  }

  Ok(parts.join("\n\n"))
}

/// What a translation is asked with: the target language's line, then the text; where the caller names
/// terms, the translation model's own terminology form instead — the reference translations, one line for
/// each term, then the instruction to answer with the translation alone, then the text. A caller's note never
/// stands in the input: a translation model renders every line of its input, a note among them, so the
/// note is carried in the system prompt instead (`noted_system`).
pub fn input(language: &str, text: &str, terms: &[TranslationTerm]) -> String {
  let language = language.trim();
  let terms: Vec<String> = terms
    .iter()
    .filter(|term| !term.source.trim().is_empty())
    .map(|term| {
      let source = term.source.trim();
      let target = term.target.as_deref().map(str::trim).filter(|target| !target.is_empty());
      format!("{source} translates to {}", target.unwrap_or(source))
    })
    .collect();

  if terms.is_empty() {
    return format!("Translate into {language}.\n\n{text}");
  }

  format!(
    "Reference the following translations:\n{}\n\nTranslate the following text into {language}. Note that you \
     must ONLY output the translated result without any additional explanation:\n\n{text}",
    terms.join("\n")
  )
}

/// The system prompt a part is translated under: the translator's, and the caller's note on this one part
/// after it where the caller gives one, as an instruction the model reads and does not render.
pub fn noted_system(system: &str, note: Option<&str>) -> String {
  match note.map(str::trim).filter(|note| !note.is_empty()) {
    Some(note) => {
      format!("{}\n\nA note on the part that follows, for you alone: {note}", system.trim_end())
    }
    None => system.to_owned(),
  }
}

/// Whether a model's refusal says the part is longer than its window takes.
pub fn too_large(why: &str) -> bool {
  let why = why.to_lowercase();

  ["context length", "context window", "maximum context", "too long", "too large"]
    .iter()
    .any(|said| why.contains(said))
}

/// A translation as it came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translated {
  pub text: String,
  pub provider: String,
  pub model: String,
}

/// Why a translation has none.
#[derive(Debug)]
pub enum Refused {
  Ask(String),
  Closed(String),
  /// The model refused the part as longer than its window takes.
  TooLarge(String),
  NoAnswer(String),
}

/// One text translated on the `translate` road.
pub async fn translate(
  ground: &Ground,
  purpose: &str,
  language: &str,
  (note, terms): (Option<&str>, &[TranslationTerm]),
  text: &str,
) -> Result<Translated, Refused> {
  let purpose = Purpose::read(purpose)
    .ok_or_else(|| Refused::Ask(format!("{purpose} is no purpose: document, worker or record")))?;

  if language.trim().is_empty() || text.trim().is_empty() {
    return Err(Refused::Ask("a translation names its language and its text".to_owned()));
  }

  let held = crate::agent_roles::named(&ground.db, &ROLES.translate)
    .await
    .map_err(|why| Refused::NoAnswer(why.detail))?
    .ok_or_else(|| Refused::NoAnswer(crate::agent_roles::unknown(&ROLES.translate)))?;
  let resolution = road::resolve(ground, &Ask::of(road::Row::Role(&held.name), &held.line, &held.order))
    .await
    .map_err(Refused::NoAnswer)?;
  let Some(taken) = resolution.road().cloned() else {
    road::went(ground, &resolution, None, &[]).await;

    return Err(Refused::Closed(resolution.sentence()));
  };

  road::went(ground, &resolution, Some(&taken), &[]).await;

  let trees = Trees::of(ground);
  let role = role_prompt(&ground.db, &ROLES.translate, None).await;
  let system =
    noted_system(&system_prompt(&trees, role.as_deref(), purpose).map_err(Refused::NoAnswer)?, note);
  let input = input(language, text, terms);
  let answer = if taken.provider == crate::control::registry::PROVIDER {
    recorded(ground, (Work::Translation(purpose), language), &taken, &system, &input).await
  } else if taken.provider == crate::lines::runner::PROVIDER {
    let entry = held.agent_of_road(&taken.provider, &taken.model).and_then(RoleAgent::entry);

    apart_on_claude(
      ground,
      &trees,
      Work::Translation(purpose),
      (&taken.model, entry.as_ref()),
      &system,
      &input,
    )
    .await
  } else {
    Err(format!("{} carries no translation", taken.provider))
  }
  .map_err(|why| if too_large(&why) { Refused::TooLarge(why) } else { Refused::NoAnswer(why) })?;

  Ok(Translated {
    text: answer,
    provider: taken.provider,
    model: taken.model,
  })
}

/// The request a translation is on the registry's provider: its system prompt, the input, thinking off and the
/// profile's sampling.
pub fn gateway_request(registration: &str, system: &str, input: &str) -> Value {
  let mut request = json!({
    "model": registration,
    "messages": [
      { "role": "system", "content": system },
      { "role": "user", "content": input },
    ],
    "chat_template_kwargs": { "enable_thinking": false },
  });
  let profile = policy::work_keys::WORK_KEYS.profile(KEY);

  if let Some(tokens) = profile.and_then(|profile| profile.gate_answer_tokens) {
    request["max_tokens"] = tokens.into();
  }

  for (name, value) in profile.and_then(|profile| profile.gate_sampling.as_ref()).into_iter().flatten() {
    request[name] = (*value).into();
  }

  request
}

// One translation through the gateway with its record kept (docs/run-records.md, 「The run ledger」): a run of
// the translator role opened as the request goes out and closed as it ends, with what the gateway counted, and
// its log lines — the start, a note of the purpose, the language and the characters in and back, and the end.
// None of the text is written down, and a record that cannot be kept is said and never fails the translation.
async fn recorded(
  ground: &Ground,
  (work, language): (Work, &str),
  taken: &road::Road,
  system: &str,
  input: &str,
) -> Result<String, String> {
  let db = &ground.db;
  let at = crate::now_ms();
  let unit = format!("{KEY}:{at}:{}", RECORDED.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
  let ledger = run_ledger::open(
    db,
    &run_ledger::Opening {
      agent: work.role().clone(),
      chat_id: None,
      work_unit: unit.clone(),
      depth: 0,
      cap_ms: None,
      started_at: at,
      started_by: None,
      search_id: None,
      round: None,
      message_id: None,
      title: None,
      carried: false,
    },
  )
  .await
  .map_err(|why| eprintln!("{unit}: the translation could not be written to the ledger: {}", why.detail))
  .ok();
  let mut log = RunLog::new(Target {
    agent: work.role().clone(),
    run: unit.clone(),
    chat_id: None,
    job_id: None,
  });

  log.apart_started(db, &taken.provider, &taken.model).await;

  let answered = on_registry(ground, &taken.model, system, input).await;
  let back = answered.as_ref().map_or(0, |(text, _)| text.chars().count());

  log
    .write(
      db,
      Line::note(format!(
        "{} · {} · {} characters in, {back} back",
        work.word(),
        language.trim(),
        input.chars().count()
      )),
    )
    .await;
  log
    .apart_ended(db, &taken.provider, answered.as_ref().map(|_| "").map_err(String::as_str))
    .await;

  if let Some(id) = ledger {
    let closing = run_ledger::Closing {
      finished_at: crate::now_ms(),
      outcome: if answered.is_ok() { "answered" } else { "failed" }.to_owned(),
      context_tokens: None,
      spent: answered.as_ref().ok().and_then(|(_, spent)| *spent),
    };

    if let Err(run_ledger::NotClosed::Store(why)) = run_ledger::close(db, id, &closing).await {
      eprintln!("{unit}: the translation could not be closed in the ledger: {}", why.detail);
    }
  }

  answered.map(|(text, _)| text)
}

// One translation on the registry's provider: on a registration naming a card service, the card every
// translation out on it at once shares, asked by the first and given back once the last is answered or
// has failed, and the request naming the card's entry (docs/own-llm.md, 「An engine the arbiter stands」).
async fn on_registry(
  ground: &Ground,
  registration: &str,
  system: &str,
  input: &str,
) -> Result<(String, Option<run_ledger::Spent>), String> {
  let card = match crate::control::registry::registration(&ground.db, registration).await {
    Ok(Some(one)) => cards::hold_shared(ground, &TRANSLATING, &one, AGENT)
      .await
      .map_err(|refused| refused.detail)?,
    Ok(None) => None,
    Err(why) => return Err(why.error),
  };
  let entry = match &card {
    Some(card) => format!("{registration}@{}", card.held.node),
    None => registration.to_owned(),
  };
  let answered = on_gateway(ground, &entry, system, input).await;

  if let Some(card) = card {
    card.leave().await;
  }

  answered
}

// One request through the model gateway, answering the model's words and the tokens its answer counts: it
// names no wait, so the gateway's entry decides, and on a card service's engine that is however long the engine
// takes (docs/own-llm.md, 「The gateway」).
async fn on_gateway(
  ground: &Ground,
  registration: &str,
  system: &str,
  input: &str,
) -> Result<(String, Option<run_ledger::Spent>), String> {
  let machine = ground.machine();
  let url = harness_machine::gateway::url(machine.var("BMO_LLM_GATEWAY_URL"));
  let key = std::fs::read_to_string(Registered::key_file(&machine)).unwrap_or_default();
  let answer = reqwest::Client::new()
    .post(format!("{url}/v1/chat/completions"))
    .bearer_auth(key.trim())
    .json(&gateway_request(registration, system, input))
    .send()
    .await
    .map_err(|why| format!("the gateway: {why}"))?;
  let status = answer.status();
  let body: Value = answer.json().await.map_err(|why| format!("the gateway's answer: {why}"))?;

  if !status.is_success() {
    return Err(format!("the gateway answered {}: {}", status.as_u16(), body["error"]));
  }

  let text = body
    .pointer("/choices/0/message/content")
    .and_then(Value::as_str)
    .map(str::trim)
    .filter(|text| !text.is_empty())
    .map(str::to_owned)
    .ok_or_else(|| "the gateway answered no words".to_owned())?;

  Ok((text, spent_of(&body)))
}

/// What a gateway's answer counts of its request: the prompt's tokens and the answer's, none where it counts
/// neither.
pub fn spent_of(answer: &Value) -> Option<run_ledger::Spent> {
  let count = |name: &str| answer.pointer(&format!("/usage/{name}")).and_then(Value::as_i64);
  let (input, output) = (count("prompt_tokens"), count("completion_tokens"));

  (input.is_some() || output.is_some()).then(|| run_ledger::Spent {
    input_tokens: input.unwrap_or(0),
    output_tokens: output.unwrap_or(0),
    ..run_ledger::Spent::default()
  })
}

// A run apart's pane watched for nothing but its end.
struct Quiet;

impl Watch for Quiet {}

// One translation on one login, a run apart of its own.
struct OnLogin<'a> {
  ground: &'a Ground,
  trees: &'a Trees,
  work: Work,
  model: &'a str,
  effort: String,
  /// The model the program may consult, as the translator's road on Claude Code names it.
  advisor: Option<String>,
  system: std::path::PathBuf,
  input: &'a str,
}

impl Renewing for OnLogin<'_> {}

impl Attempt<String> for OnLogin<'_> {
  async fn attempt(&mut self, account: Option<Account>) -> Result<String, Failure> {
    let session = new_session();
    let args = apart_args(
      &ApartRun {
        line: LINE,
        model: self.model,
        effort: &self.effort,
        system_prompt: Some(&self.system),
        mcp_config: None,
        tools_shown: "",
        tools_allowed: "",
        host_commands: policy::run_lines::RUN_LINES.carries(LINE).slash_commands,
        advisor: self.advisor.as_deref(),
      },
      &session,
    )
    .map_err(|why| Failure::error(why.detail))?;
    let env = command_env(self.ground.env(), account.as_ref(), &Runs::here(self.ground), false)
      .map_err(|why| Failure::error(why.detail))?;
    let cwd = self.trees.agents.join(AGENT);

    std::fs::create_dir_all(&cwd).map_err(|why| Failure::error(why.to_string()))?;

    let command = Command {
      args,
      cwd: cwd.to_string_lossy().into_owned(),
      env,
    };
    let ending = apart::run(
      self.ground,
      Apart {
        agent: AGENT,
        role: self.work.role(),
        line: LINE,
        provider: crate::lines::runner::PROVIDER,
        title: format!("{KEY} · {} · {}", self.work.word(), self.model),
        command,
        session,
        prompt: self.input.to_owned(),
        model: self.model.to_owned(),
        effort: self.effort.clone(),
        account: None,
        parent: None,
        root: None,
        run: None,
        resume: None,
        resume_else: None,
      },
      &mut Quiet,
    )
    .await
    .map_err(|why| Failure::Transport { message: why })?;

    Ok(answered(ending, KEY)?.text.trim().to_owned())
  }
}

// A run apart on Claude Code, on the logins the key's walk offers, at the effort and with the advisor of the
// translator agent whose road it took. A translation holds no seat of its role (docs/role-agents.md, 「Which agent a
// run takes」): it goes out as the agent its road was taken from.
async fn apart_on_claude(
  ground: &Ground,
  trees: &Trees,
  work: Work,
  (model, entry): (&str, Option<&Entry>),
  system: &str,
  input: &str,
) -> Result<String, String> {
  let provider = crate::lines::runner::PROVIDER;
  let effort = effort_of(
    ground,
    &ground.machine(),
    provider,
    model,
    entry.and_then(|entry| entry.effort.as_deref()),
    None,
  )
  .await
  .map_err(|why| why.detail)?
  .unwrap_or_default();
  let system =
    system_prompt_file(trees, &format!("{LINE}-{}", work.word()), system).map_err(|why| why.detail)?;
  let mut attempt = OnLogin {
    ground,
    trees,
    work,
    model,
    effort,
    advisor: entry.and_then(Entry::advisor_taken),
    system,
    input,
  };
  let text = on_accounts(&Walking { ground, model }, KEY, None, &mut attempt)
    .await
    .map_err(|failure| failure.message())?;

  Some(text)
    .filter(|text| !text.is_empty())
    .ok_or_else(|| "the run answered no words".to_owned())
}
