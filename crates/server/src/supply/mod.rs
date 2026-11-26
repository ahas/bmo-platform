//! A person's supply inside a road (docs/supply.md): the supply hook, the source a run takes as it starts, its
//! reservations and their settlement, the meter that extends them while the run goes and finds a source run out,
//! and the facts told. A run of no account or of the administrator's account takes no source of a person's and is
//! only recorded; every rule here holds for the runs of the other accounts. Which sources a person's run may take
//! is the hook's order; where organisation supply goes out is read off the delegation chain (`sources`).

pub mod hook;
pub mod meter;
pub mod reservations;
pub mod sources;
pub mod told;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use policy::fill_marks;
use policy::organisation::ORGANISATION;
use policy::supply::SUPPLY;
use store::{one, run};

use crate::Refusal;
use crate::control::claude_accounts::{Carrying, Judging, moment_words, walk_for_user};
use crate::places::Ground;
pub use hook::Kind;

/// The source of a person's own credential.
pub const PERSONAL: &str = "personal";
/// The source the deployment's own runs go out on, which reserve nothing: the platform's.
pub const PLATFORM: &str = "platform";
/// The Claude Code provider, whose credentials are its accounts.
const CLAUDE_CODE: &str = "claude-code";
/// The provider whose models are the deployment's own registrations, which no person brings.
const REGISTERED: &str = "registered";

/// The source a run goes out on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
  /// A run of no account or of the administrator's: the platform's credentials, reserved nowhere.
  Deployment,
  /// The person's own credential: a Claude login or key of theirs, or their own key of a gateway road.
  Personal,
  /// A gateway road's member or a registration the person owns, which no key of theirs carries
  /// (docs/supply.md, 「Owned supply」).
  Owned,
  /// A platform source, by the name the hook gave it, with the store's row of the reservation it goes out
  /// on.
  Platform { name: String, reservation: i64 },
  /// An organisation unit's own credential, on the person's path, with the store's row of the reservation it
  /// goes out on (docs/supply.md, 「Organisation supply」).
  Organisation { unit: i64, reservation: i64 },
}

impl Source {
  /// The word the ledger keeps of it.
  pub fn word(&self) -> &str {
    match self {
      Self::Deployment => PLATFORM,
      Self::Personal | Self::Owned => PERSONAL,
      Self::Platform { name, .. } => name,
      Self::Organisation { .. } => &ORGANISATION.sources.organisation_word,
    }
  }

  /// The reservation it goes out on, where it holds one.
  pub fn reservation(&self) -> Option<i64> {
    match self {
      Self::Platform { reservation, .. } | Self::Organisation { reservation, .. } => Some(*reservation),
      _ => None,
    }
  }
}

/// The source word of an organisation's own credential in an order.
pub fn organisation_word() -> &'static str {
  &ORGANISATION.sources.organisation_word
}

/// A run's supply as it was chosen: the key its asks are made under, the person, the road and the source, and the
/// order the hook gave, which a source that runs out hands on along.
#[derive(Clone, Debug, PartialEq)]
pub struct Chosen {
  pub run: String,
  pub account: Option<i64>,
  pub provider: String,
  pub model: String,
  pub source: Source,
  pub order: Vec<String>,
  /// The units on the person's path whose own supply the run may take, the nearest first; none where the chain
  /// leaves organisation supply closed.
  pub units: Vec<i64>,
  /// The next step a money ask of the run takes.
  pub step: i64,
}

impl Chosen {
  /// The deployment's own run: on the platform's credentials, reserved nowhere.
  pub fn deployment(run: String, provider: &str, model: &str) -> Self {
    Self {
      run,
      account: None,
      provider: provider.to_owned(),
      model: model.to_owned(),
      source: Source::Deployment,
      order: Vec::new(),
      units: Vec::new(),
      step: 0,
    }
  }
}

/// What the choice comes to: a source to go out on, or none left with the sentence that says so.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
  Go(Chosen),
  Closed(Closed),
}

/// Why no source is left for a run: what kept the last source tried from taking it. It names the sentence a person
/// is told and the word the room's line carries (docs/supply.md, 「Running out mid-turn」 and 「The facts told」).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortfall {
  /// The order named no source.
  NoOrder,
  /// The person's chain closed every source the order named.
  Withheld,
  /// The person holds no open login or key of their own for the road's provider.
  NoPersonal,
  /// The provider refused the person's own key for the road.
  OwnRefused,
  /// No unit of the person's path holds an open credential for the road's provider.
  NoOrganisation,
  /// The hook refused a reservation of the organisation's own supply.
  OrganisationRefused,
  /// No Claude account marked lendable answers for other people's runs.
  NoLendable,
  /// The person's node is one other accounts share, where no login is lent (docs/supply.md, 「The lendable
  /// accounts」).
  SharedNode,
  /// The run's model is another person's own.
  Owned,
  /// The hook refused a reservation of a platform source.
  Refused,
  /// The source the run went out on ran out mid-run and no other took it.
  RanOut,
}

impl Shortfall {
  /// The word the room's line and the `supply.changed` fact carry.
  pub fn word(self) -> &'static str {
    match self {
      Self::NoOrder => "no-order",
      Self::Withheld => "withheld",
      Self::NoPersonal => "no-personal",
      Self::OwnRefused => "own-refused",
      Self::NoOrganisation => "no-organisation",
      Self::OrganisationRefused => "organisation-refused",
      Self::NoLendable => "no-lendable",
      Self::SharedNode => "shared-node",
      Self::Owned => "owned",
      Self::Refused => "refused",
      Self::RanOut => "ran-out",
    }
  }

  // The policy's sentence for it, with its marks still to fill.
  fn sentence(self) -> &'static str {
    let words = &SUPPLY.words.closed;

    match self {
      Self::NoOrder => &words.no_order,
      Self::Withheld => &words.withheld,
      Self::NoPersonal => &words.no_personal,
      Self::OwnRefused => &words.own_refused,
      Self::NoOrganisation => &words.no_organisation,
      Self::OrganisationRefused => &words.organisation_refused,
      Self::NoLendable => &words.no_lendable,
      Self::SharedNode => &words.shared_node,
      Self::Owned => &words.owned,
      Self::Refused => &words.refused,
      Self::RanOut => &words.ran_out,
    }
  }
}

/// No source left for a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Closed {
  pub why: Shortfall,
  pub source: String,
  pub reason: String,
  pub reopens_at: Option<i64>,
  pub sentence: String,
}

impl Closed {
  /// No source left, the last one tried named with why, and the one sentence a person is told for that reason.
  pub fn of(why: Shortfall, source: &str, reason: String, reopens_at: Option<i64>) -> Self {
    let reopens = reopens_at.map_or_else(String::new, |at| {
      fill_marks(&SUPPLY.words.reopens, &[("at", &moment_words(at / 1000))])
    });
    let sentence = fill_marks(
      why.sentence(),
      &[("source", source), ("reopens", &reopens), ("settings", &SUPPLY.words.settings_section)],
    );

    Self {
      why,
      source: source.to_owned(),
      reason,
      reopens_at,
      sentence,
    }
  }
}

/// What a run asks as it starts.
#[derive(Clone, Debug)]
pub struct Asked<'a> {
  pub kind: Kind,
  /// The run's own number: the job's id, or the ledger run a supervisor's turn opened.
  pub id: i64,
  pub agent: &'a str,
  pub account: Option<i64>,
  pub provider: &'a str,
  pub model: &'a str,
  pub job: Option<i64>,
  pub supervisor: Option<&'a str>,
  pub room: Option<i64>,
}

// Each agent's supply as its last run chose it, by the data root it was chosen under and the agent, so two
// stores a process serves at once never read each other's.
static CHOSEN: Mutex<Option<HashMap<(PathBuf, String), Chosen>>> = Mutex::new(None);

fn chosen_map<T>(work: impl FnOnce(&mut HashMap<(PathBuf, String), Chosen>) -> T) -> T {
  let mut held = CHOSEN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

  work(held.get_or_insert_with(HashMap::new))
}

/// The supply an agent's run goes out on now, where its run chose one.
pub fn chosen_for(data_root: &Path, agent: &str) -> Option<Chosen> {
  chosen_map(|map| map.get(&(data_root.to_path_buf(), agent.to_owned())).cloned())
}

/// The name a job's supply is kept under beside its checkout's.
pub fn job_key(job: i64) -> String {
  format!("job:{job}")
}

/// The supply an agent's run goes out on from now.
pub fn remember(data_root: &Path, agent: &str, chosen: &Chosen) {
  chosen_map(|map| map.insert((data_root.to_path_buf(), agent.to_owned()), chosen.clone()));
}

/// A run's supply kept for its agent, and a person's source that is not the one their agent's last run took
/// told as a move (docs/supply.md, 「The facts told」).
pub async fn settle_choice(
  ground: &Ground,
  agent: &str,
  chosen: &Chosen,
  job: Option<i64>,
  told: &told::Told,
) {
  let before = chosen_for(&ground.roots.data, agent);

  remember(&ground.roots.data, agent, chosen);
  told::reopened(ground, told).await;

  let Some(before) = before.filter(|before| chosen.account.is_some() && before.account == chosen.account)
  else {
    return;
  };

  if before.source.word() != chosen.source.word() {
    let why = fill_marks(&SUPPLY.words.taken_first, &[("source", chosen.source.word())]);

    told::moved_on(ground, chosen, job, told, before.source.word(), chosen.source.word(), &why).await;
  }
}

/// The person a run is for, where it is for a person other than the administrator; none for the deployment's
/// own runs.
pub async fn person(ground: &Ground, account: Option<i64>) -> Result<Option<i64>, Refusal> {
  let Some(account) = account else { return Ok(None) };

  if crate::account_isolation::owners_own(&ground.db, &account.to_string()).await? {
    return Ok(None);
  }

  Ok(Some(account))
}

/// The account a named agent's runs are for: the supervisor's, else the role agent's.
pub async fn account_of(ground: &Ground, agent: &str) -> Result<Option<i64>, Refusal> {
  let acting = crate::person_nodes::acting_for(
    &ground.db,
    &ground.roots.data,
    crate::person_nodes::Caller {
      agent,
      role: None,
      checkout: None,
    },
  )
  .await?;

  Ok(acting.account.and_then(|account| account.parse().ok()))
}

/// The Claude logins a run walks, for the supply it goes out on: a person's own on personal supply, the
/// lendable ones on a platform source, and the platform's and the administrator's for the deployment's own.
pub fn carrying(chosen: Option<&Chosen>) -> Carrying {
  match chosen.map(|chosen| (&chosen.source, chosen.account)) {
    Some((Source::Personal | Source::Owned, Some(account))) => Carrying::Own(account),
    Some((Source::Platform { .. }, Some(account))) => Carrying::Lent(account),
    Some((Source::Organisation { unit, .. }, Some(_))) => Carrying::Organisation(*unit),
    _ => Carrying::Platform,
  }
}

/// The name a gateway road's run is sent under on the person's own key or their organisation's: its member's
/// name marked with the person or the unit (docs/own-llm.md, 「Personal members」); none where the run goes out on
/// the member's own name, the platform's, one the person owns, or one the unit itself owns — a member filed
/// under the unit carries the unit's own key and needs no variant (`docs/supply.md`, 「Organisation supply」).
pub fn personal_member(
  chosen: Option<&Chosen>,
  machine: &harness_providers::Machine,
  name: &str,
) -> Option<String> {
  let chosen = chosen?;
  match (&chosen.source, chosen.account) {
    (Source::Personal, Some(account)) => Some(format!("{name}@u{account}")),
    (Source::Organisation { unit, .. }, Some(_)) => {
      let own = harness_providers::member::member_named(machine, &chosen.provider, name)
        .is_ok_and(|held| held.is_some_and(|member| member.unit == Some(*unit)));

      (!own).then(|| format!("{name}@o{unit}"))
    }
    _ => None,
  }
}

/// The organisation unit a model is filed under, none where no unit owns it: a registration by its own row, a
/// gateway road's member by the members file; a Claude account's unit is read by the walk of its logins
/// (docs/supply.md, 「Owned supply」).
pub async fn model_unit(ground: &Ground, provider: &str, model: &str) -> Result<Option<i64>, Refusal> {
  match provider {
    CLAUDE_CODE => Ok(None),
    REGISTERED => Ok(
      one(
        &ground.db,
        r#"SELECT "owner_unit" FROM "registered_models" WHERE "name" = $1"#,
        vec![model.into()],
      )
      .await?
      .map(|row| row.try_get::<Option<i64>>("", "owner_unit"))
      .transpose()?
      .flatten(),
    ),
    road => Ok(
      harness_providers::member::member_named(&ground.machine(), road, model)
        .map_err(Refusal::failed)?
        .and_then(|member| member.unit),
    ),
  }
}

/// The account that owns the model a run on the road goes out on, none for the platform's: a gateway road's
/// member by the members file, a registration by its own row; a Claude account's owner is read by the walk
/// of its logins (docs/supply.md, 「Owned supply」).
pub async fn owner_of(ground: &Ground, provider: &str, model: &str) -> Result<Option<i64>, Refusal> {
  match provider {
    CLAUDE_CODE => Ok(None),
    REGISTERED => Ok(
      one(
        &ground.db,
        r#"SELECT "owner_account" FROM "registered_models" WHERE "name" = $1"#,
        vec![model.into()],
      )
      .await?
      .map(|row| row.try_get::<Option<i64>>("", "owner_account"))
      .transpose()?
      .flatten(),
    ),
    road => Ok(
      harness_providers::member::member_named(&ground.machine(), road, model)
        .map_err(Refusal::failed)?
        .and_then(|member| member.account),
    ),
  }
}

/// The person's own supply open for the road's provider, where one is: a Claude login or key that can answer
/// now; on a gateway road the road's member they own, else an open key of the road; or the registration they
/// own.
pub async fn personal_source(
  ground: &Ground,
  account: i64,
  provider: &str,
  model: &str,
) -> Result<Option<Source>, Refusal> {
  if provider == CLAUDE_CODE {
    let open = !walk_for_user(Judging::of(ground), Carrying::Own(account), Some(model))
      .await
      .map_err(|refused| Refusal::failed(refused.error))?
      .is_empty();

    return Ok(open.then_some(Source::Personal));
  }

  if owner_of(ground, provider, model).await? == Some(account) {
    // A local AI of theirs carries their runs only while it is on and their chain allows it (docs/supply.md, 「A
    // person's local AI」).
    if provider == REGISTERED && crate::local_ai::is_local(&ground.db, model).await? {
      let on = crate::local_ai::of_person(&ground.db, account)
        .await?
        .iter()
        .any(|one| one.registration == model && one.enabled);

      if !on || local_withheld(ground, account).await {
        return Ok(None);
      }
    }

    return Ok(Some(Source::Owned));
  }

  if provider == REGISTERED {
    return Ok(None);
  }

  let keyed = one(
    &ground.db,
    r#"SELECT 1 AS "open" FROM "personal_supply"
       WHERE "owner_account" = $1 AND "road" = $2 AND "enabled" AND "refused_at" IS NULL LIMIT 1"#,
    vec![account.into(), provider.into()],
  )
  .await?
  .is_some();

  Ok(keyed.then_some(Source::Personal))
}

// Whether the person's chain forbids them a local AI of their own; an account service that does not answer forbids
// it too, so no run goes out on what the chain may close.
async fn local_withheld(ground: &Ground, account: i64) -> bool {
  !crate::local_ai::allowed(ground, account).await.unwrap_or(false)
}

/// Whether the run's model is a local AI of the person's that their chain forbids.
async fn own_local_withheld(
  ground: &Ground,
  account: i64,
  provider: &str,
  model: &str,
) -> Result<bool, Refusal> {
  if provider != REGISTERED || owner_of(ground, provider, model).await? != Some(account) {
    return Ok(false);
  }

  Ok(crate::local_ai::is_local(&ground.db, model).await? && local_withheld(ground, account).await)
}

/// Whether the person holds a key of their own for the road that its provider refused, where none of theirs is open.
async fn own_key_refused(ground: &Ground, account: i64, provider: &str) -> Result<bool, Refusal> {
  if provider == CLAUDE_CODE || provider == REGISTERED {
    return Ok(false);
  }

  Ok(
    one(
      &ground.db,
      r#"SELECT 1 AS "refused" FROM "personal_supply"
         WHERE "owner_account" = $1 AND "road" = $2 AND "enabled" AND "refused_at" IS NOT NULL LIMIT 1"#,
      vec![account.into(), provider.into()],
    )
    .await?
    .is_some(),
  )
}

/// The nearest unit of `units` whose own supply is open for the road's provider, where one is: a Claude account
/// of the unit's that can answer now for a Claude Code road, an open key of the unit's or a member of the road
/// the unit itself owns for a gateway road, or the run's model as a registration filed under the unit for the
/// registry's provider (docs/supply.md, 「Organisation supply」).
pub async fn organisation_source(
  ground: &Ground,
  units: &[i64],
  provider: &str,
  model: &str,
) -> Result<Option<i64>, Refusal> {
  for unit in units {
    let open = match provider {
      CLAUDE_CODE => !walk_for_user(Judging::of(ground), Carrying::Organisation(*unit), Some(model))
        .await
        .map_err(|refused| Refusal::failed(refused.error))?
        .is_empty(),
      // A registration answers the walk by its filing alone, as a person's own one answers personal supply;
      // whether the engine behind it stands is the gateway's to answer at the request.
      REGISTERED => model_unit(ground, REGISTERED, model).await? == Some(*unit),
      road => {
        one(
          &ground.db,
          r#"SELECT 1 AS "open" FROM "personal_supply"
             WHERE "owner_unit" = $1 AND "road" = $2 AND "enabled" AND "refused_at" IS NULL LIMIT 1"#,
          vec![(*unit).into(), road.into()],
        )
        .await?
        .is_some()
          || harness_providers::member::member_named(&ground.machine(), road, model)
            .map_err(Refusal::failed)?
            .is_some_and(|member| member.unit == Some(*unit))
      }
    };

    if open {
      return Ok(Some(*unit));
    }
  }

  Ok(None)
}

/// Whether the platform's credentials can carry `account`'s run on the road: for Claude Code, an open
/// lendable account of someone else's; on every other road, a model no person owns.
pub async fn platform_open(
  ground: &Ground,
  account: i64,
  provider: &str,
  model: &str,
) -> Result<bool, Refusal> {
  if provider != CLAUDE_CODE {
    // What a person or a unit owns stands with their things: no platform source (docs/supply.md, 「Owned
    // supply」).
    return Ok(
      owner_of(ground, provider, model).await?.is_none()
        && model_unit(ground, provider, model).await?.is_none(),
    );
  }

  Ok(
    !walk_for_user(Judging::of(ground), Carrying::Lent(account), Some(model))
      .await
      .map_err(|refused| Refusal::failed(refused.error))?
      .is_empty(),
  )
}

/// Whether the person's supply carries a run on each of `models` of a provider now, as a run's start walks it but
/// reserving nothing (docs/role-agents.md, 「A person's own agents」): a source of the hook's order holding a
/// credential for the road. The administrator's and the deployment's own runs go out on
/// every road.
pub async fn carries(
  ground: &Ground,
  account: i64,
  provider: &str,
  models: &[String],
) -> Result<Vec<bool>, Refusal> {
  let Some(account) = person(ground, Some(account)).await? else {
    return Ok(vec![true; models.len()]);
  };
  let ask = hook::OrderAsk {
    run: format!("road-choice-{account}-{}", crate::now_ms()),
    account,
    provider: provider.to_owned(),
    model: models.first().cloned().unwrap_or_default(),
    kind: Kind::Turn,
    job: None,
    supervisor: None,
    room: None,
  };
  // A provider the owner switched off or the person's chain closes carries nothing, as a run of it would find
  // its walk ended before any source (docs/supply.md, 「A closed provider is read as closed, not as absent」).
  if provider_state(ground, provider, account).await?.is_some() {
    return Ok(vec![false; models.len()]);
  }

  let order = hook::order(&ground.hooks, ask).await.unwrap_or_else(|| vec![PERSONAL.to_owned()]);
  let units = sources::units_for(ground, account, &order).await;
  let mut carried = Vec::with_capacity(models.len());

  for model in models {
    let mut any = false;

    for source in &order {
      any = if source == PERSONAL {
        personal_source(ground, account, provider, model).await?.is_some()
      } else if source == organisation_word() {
        organisation_source(ground, &units, provider, model).await?.is_some()
      } else {
        platform_open(ground, account, provider, model).await?
      };

      if any {
        break;
      }
    }

    carried.push(any);
  }

  Ok(carried)
}

/// Who stands a provider off for a person: the owner's switch, or their chain holding
/// `provider.<code>.offered` off below a link that said it, with that link; none where its sources stand open
/// for them (docs/supply.md, 「A closed provider is read as closed, not as absent」).
pub enum ProviderOff {
  Owner,
  Chain(Option<String>),
}

impl ProviderOff {
  /// The word the reads carry it by.
  pub fn word(&self) -> &'static str {
    match self {
      Self::Owner => "owner",
      Self::Chain(_) => "chain",
    }
  }

  /// The link that closed the provider, where the chain did.
  pub fn link(&self) -> Option<String> {
    match self {
      Self::Owner => None,
      Self::Chain(from) => from.clone(),
    }
  }
}

pub async fn provider_state(
  ground: &Ground,
  provider: &str,
  person: i64,
) -> Result<Option<ProviderOff>, Refusal> {
  if !crate::control::harness::provider_enabled(&ground.db, provider)
    .await
    .map_err(|refused| Refusal::failed(refused.error))?
  {
    return Ok(Some(ProviderOff::Owner));
  }

  if !harness_providers::ARCHIVE_PROVIDERS.contains(&provider) {
    return Ok(None);
  }

  let key = format!("provider.{provider}.offered");
  let answer =
    crate::chain::resolve(&crate::apps::service(ground), &key, crate::chain::Subject::Person(person)).await;
  let answered = match answer {
    // A chain that does not answer says nothing: as every narrowing flag does, its default stands open
    // (docs/cache-fallback.md, 「The account judgement」).
    Err(why) => {
      eprintln!("agency: the offer of provider {provider} for account {person} was not read: {why}");
      return Ok(None);
    }
    Ok(answer) => answer,
  };

  if crate::chain::flag(answered.as_ref(), true) {
    return Ok(None);
  }

  // The link that closed the provider is the outermost that said so: what it closes, no link below opens.
  let from = answered
    .as_ref()
    .and_then(|answer| answer["layers"].as_array())
    .into_iter()
    .flatten()
    .find(|layer| layer["value"] == serde_json::Value::Bool(false))
    .and_then(|layer| layer["link"].as_str())
    .map(str::to_owned);

  Ok(Some(ProviderOff::Chain(from)))
}

/// The order a person's run walks: the hook's, or personal supply alone where nobody is registered, with the units
/// of their path organisation supply goes out on where the order names it.
///
/// Answers too whether the hook answered and named no source at all.
async fn order_of(
  ground: &Ground,
  asked: &Asked<'_>,
  run: &str,
  account: i64,
) -> (Vec<String>, Vec<i64>, bool) {
  let (order, withheld) = match hooked_order(ground, asked, run, account).await {
    Some(order) => {
      let withheld = order.is_empty();

      (order, withheld)
    }
    None => (vec![PERSONAL.to_owned()], false),
  };
  let units = sources::units_for(ground, account, &order).await;

  (order, units, withheld)
}

async fn hooked_order(ground: &Ground, asked: &Asked<'_>, run: &str, account: i64) -> Option<Vec<String>> {
  let ask = hook::OrderAsk {
    run: run.to_owned(),
    account,
    provider: asked.provider.to_owned(),
    model: asked.model.to_owned(),
    kind: asked.kind,
    job: asked.job,
    supervisor: asked.supervisor.map(str::to_owned),
    room: asked.room,
  };

  hook::order(&ground.hooks, ask).await
}

/// The key a run's money asks are made under: its kind, its number and the moment it chose, so a run after a
/// restart never takes an earlier run's key.
fn run_key(asked: &Asked<'_>) -> String {
  let kind = match asked.kind {
    Kind::Job => "job",
    Kind::Turn => "turn",
  };

  format!("{kind}-{}-{}", asked.id, crate::now_ms())
}

/// The source a run takes as it starts (docs/supply.md, 「Choosing a source as a run starts」): the deployment's
/// own runs the platform's, unreserved; a person's the first of their order that can take it inside the road,
/// a platform source only once the hook reserved it, and none where nothing is left.
pub async fn choose(ground: &Ground, asked: &Asked<'_>) -> Result<Choice, Refusal> {
  let run = run_key(asked);
  let Some(account) = person(ground, asked.account).await? else {
    return Ok(Choice::Go(Chosen::deployment(run, asked.provider, asked.model)));
  };
  let (order, units, withheld) = order_of(ground, asked, &run, account).await;
  let mut chosen = Chosen {
    run,
    account: Some(account),
    provider: asked.provider.to_owned(),
    model: asked.model.to_owned(),
    source: Source::Personal,
    order: order.clone(),
    units,
    step: 1,
  };

  if withheld {
    return Ok(Choice::Closed(Closed::of(
      Shortfall::Withheld,
      PLATFORM,
      SUPPLY.words.withheld.clone(),
      None,
    )));
  }

  match next_source(ground, &mut chosen, &order).await? {
    Ok(()) => Ok(Choice::Go(chosen)),
    Err(closed) => Ok(Choice::Closed(closed)),
  }
}

// Whether the walk may go out on the provider's sources at all: the chain kind `provider.<code>.offered`
// (docs/supply.md, 「The sources a run walks」). A link of it saying false turns every source of that provider
// off below it — the person's own, the units' and the platform's alike; none turns it back on. A road the
// build carries no kind for, and a chain that says nothing, stand open.
async fn provider_offered(ground: &Ground, provider: &str, account: Option<i64>) -> Result<bool, Refusal> {
  if !harness_providers::ARCHIVE_PROVIDERS.contains(&provider) {
    return Ok(true);
  }

  let key = format!("provider.{provider}.offered");

  if let Some(account) = account {
    let answer =
      crate::chain::resolve(&crate::apps::service(ground), &key, crate::chain::Subject::Person(account))
        .await
        .map_err(Refusal::failed)?;

    return Ok(crate::chain::flag(answer.as_ref(), true));
  }

  // A run of no account stands on the platform alone, so the platform link's own word closes it.
  match crate::chain_settings::platform(ground, &key).await {
    Ok(Some(serde_json::Value::Bool(on))) => Ok(on),
    _ => Ok(true),
  }
}

/// The first source of `sources` that can take the run, written on it; where none can, why the last one tried
/// could not.
pub async fn next_source(
  ground: &Ground,
  chosen: &mut Chosen,
  sources: &[String],
) -> Result<Result<(), Closed>, Refusal> {
  if !provider_offered(ground, &chosen.provider, chosen.account).await? {
    let reason = fill_marks(&SUPPLY.words.provider_off, &[("provider", &chosen.provider)]);

    return Ok(Err(Closed::of(Shortfall::Withheld, PLATFORM, reason, None)));
  }

  next_walked_source(ground, chosen, sources).await
}

// The walk itself, once the provider stands open below the run's links.
async fn next_walked_source(
  ground: &Ground,
  chosen: &mut Chosen,
  sources: &[String],
) -> Result<Result<(), Closed>, Refusal> {
  let Some(account) = chosen.account else {
    chosen.source = Source::Deployment;
    return Ok(Ok(()));
  };
  let mut last = Closed::of(Shortfall::NoOrder, PLATFORM, SUPPLY.words.no_order.clone(), None);

  for source in sources {
    if source == PERSONAL {
      if let Some(personal) = personal_source(ground, account, &chosen.provider, &chosen.model).await? {
        chosen.source = personal;
        return Ok(Ok(()));
      }

      if own_local_withheld(ground, account, &chosen.provider, &chosen.model).await? {
        last = Closed::of(Shortfall::Withheld, PERSONAL, SUPPLY.words.withheld.clone(), None);
        continue;
      }

      let reason = fill_marks(&SUPPLY.words.no_personal, &[("provider", &chosen.provider)]);
      let why = if own_key_refused(ground, account, &chosen.provider).await? {
        Shortfall::OwnRefused
      } else {
        Shortfall::NoPersonal
      };

      last = Closed::of(why, PERSONAL, reason, None);
      continue;
    }

    if source == organisation_word() {
      let Some(unit) = organisation_source(ground, &chosen.units, &chosen.provider, &chosen.model).await?
      else {
        let reason = fill_marks(&ORGANISATION.sources.no_organisation, &[("provider", &chosen.provider)]);

        last = Closed::of(Shortfall::NoOrganisation, source, reason, None);
        continue;
      };

      match reservations::reserve(ground, chosen, source, Some(unit)).await? {
        Ok(row) => {
          chosen.source = Source::Organisation {
            unit,
            reservation: row,
          };
          return Ok(Ok(()));
        }
        Err(reopens_at) => {
          let reason = fill_marks(&SUPPLY.words.refused, &[("source", source)]);

          last = Closed::of(Shortfall::OrganisationRefused, source, reason, reopens_at);
          continue;
        }
      }
    }

    if !platform_open(ground, account, &chosen.provider, &chosen.model).await? {
      let shared = chosen.provider == CLAUDE_CODE
        && crate::person_nodes::shared_node(&ground.db, &account.to_string()).await?.is_some();
      let (why, reason) = if shared {
        (
          Shortfall::SharedNode,
          fill_marks(&SUPPLY.words.shared_node, &[("provider", &chosen.provider)]),
        )
      } else if chosen.provider == CLAUDE_CODE {
        (
          Shortfall::NoLendable,
          fill_marks(&SUPPLY.words.no_lendable, &[("provider", &chosen.provider)]),
        )
      } else {
        (Shortfall::Owned, fill_marks(&SUPPLY.words.owned, &[("model", &chosen.model)]))
      };

      last = Closed::of(why, source, reason, None);
      continue;
    }

    match reservations::reserve(ground, chosen, source, None).await? {
      Ok(row) => {
        chosen.source = Source::Platform {
          name: source.clone(),
          reservation: row,
        };
        return Ok(Ok(()));
      }
      Err(reopens_at) => {
        let reason = fill_marks(&SUPPLY.words.refused, &[("source", source)]);

        last = Closed::of(Shortfall::Refused, source, reason, reopens_at);
      }
    }
  }

  Ok(Err(last))
}

/// The sources of a run's order after the one it goes out on now, the platform's alone: where an extension
/// refused hands the run on along (docs/supply.md, 「Running out mid-turn」). A run on its organisation's own
/// credential is handed to none, since a platform source goes out on another credential.
pub fn platform_after(chosen: &Chosen) -> Vec<String> {
  if matches!(chosen.source, Source::Organisation { .. }) {
    return Vec::new();
  }

  let now = chosen.source.word().to_owned();
  let at = chosen.order.iter().position(|source| *source == now).map_or(0, |at| at + 1);

  chosen.order[at.min(chosen.order.len())..]
    .iter()
    .filter(|source| *source != PERSONAL && *source != organisation_word())
    .cloned()
    .collect()
}

/// The sources of a run's order after personal supply, for a run whose own credential was refused.
pub fn after_personal(chosen: &Chosen) -> Vec<String> {
  chosen.order.iter().filter(|source| *source != PERSONAL).cloned().collect()
}

/// A run on a model the person owns that refused it, passed over to the rest of its order: the next source
/// after personal supply that can take it, or why none can (docs/supply.md, 「Owned supply」).
pub async fn pass_owned(ground: &Ground, chosen: &Chosen) -> Result<Choice, Refusal> {
  let mut next = chosen.clone();

  Ok(match next_source(ground, &mut next, &after_personal(chosen)).await? {
    Ok(()) => Choice::Go(next),
    Err(closed) => Choice::Closed(closed),
  })
}

/// A person's own key for a gateway road, or their organisation unit's, found refused by its provider, where the
/// run went out on one: it waits for its owner, the person told once, and its next key or the next source is taken
/// (docs/supply.md, 「Running out mid-turn」 and 「Organisation supply」). A run on a model the person owns, or on a
/// Claude login, marks no key. Answers the key marked.
pub async fn refuse_personal_key(ground: &Ground, chosen: &Chosen) -> Result<Option<i64>, Refusal> {
  let (owner, id) = match (&chosen.source, chosen.account) {
    (Source::Personal, Some(account)) => ("owner_account", account),
    (Source::Organisation { unit, .. }, Some(_)) => ("owner_unit", *unit),
    _ => return Ok(None),
  };
  if chosen.provider == CLAUDE_CODE {
    return Ok(None);
  }
  let road = chosen.provider.as_str();
  let Some(row) = one(
    &ground.db,
    &format!(
      r#"SELECT "id" FROM "personal_supply"
         WHERE "{owner}" = $1 AND "road" = $2 AND "enabled" AND "refused_at" IS NULL ORDER BY "place" LIMIT 1"#
    ),
    vec![id.into(), road.into()],
  )
  .await?
  else {
    return Ok(None);
  };
  let id: i64 = row.try_get("", "id")?;

  run(
    &ground.db,
    r#"UPDATE "personal_supply" SET "refused_at" = $2 WHERE "id" = $1"#,
    vec![id.into(), crate::now_ms().into()],
  )
  .await?;

  Ok(Some(id))
}
