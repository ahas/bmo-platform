//! What each harness provider answers about this machine (docs/provider-presence.md): whether it can
//! answer here at all, and which models it offers. Each provider is a module of its own and reads its
//! own file of the shared policy; what the archive holds of a provider is read by each side from its
//! own archive, never here.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use policy::providers::Declared;
use serde_json::Value;

pub mod balance;
pub mod claude_code;
pub mod codex;
pub mod deepseek;
pub mod member;
pub mod openrouter;
pub mod registered;
pub mod toolbook;

/// What a provider reads of the machine it is asked about: the asking process's environment and the
/// data root.
#[derive(Clone, Debug)]
pub struct Machine {
  vars: BTreeMap<String, String>,
  data_root: PathBuf,
}

impl Machine {
  /// The machine as this process sees it.
  pub fn of_process(data_root: impl Into<PathBuf>) -> Self {
    Self::with_env(std::env::vars(), data_root)
  }

  /// The machine with an environment of its own, as a test stands one in.
  pub fn with_env(vars: impl IntoIterator<Item = (String, String)>, data_root: impl Into<PathBuf>) -> Self {
    Self {
      vars: vars.into_iter().collect(),
      data_root: data_root.into(),
    }
  }

  /// A variable of the environment, where it is set to something.
  pub fn var(&self, name: &str) -> Option<&str> {
    self.vars.get(name).map(String::as_str).filter(|value| !value.is_empty())
  }

  /// Every variable of the environment set to something, by name.
  pub fn vars(&self) -> impl Iterator<Item = (&str, &str)> {
    self
      .vars
      .iter()
      .filter(|(_, value)| !value.is_empty())
      .map(|(name, value)| (name.as_str(), value.as_str()))
  }

  /// Every variable of the environment as it stands, an empty one among them: what a launch hands on
  /// is the environment as the process holds it.
  pub fn every_var(&self) -> impl Iterator<Item = (&str, &str)> {
    self.vars.iter().map(|(name, value)| (name.as_str(), value.as_str()))
  }

  pub fn data_root(&self) -> &Path {
    &self.data_root
  }

  /// Where an executable of this name stands on the environment's PATH, as a shell would find it.
  pub fn program(&self, name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    std::env::split_paths(self.var("PATH")?).map(|dir| dir.join(name)).find(|path| {
      path
        .metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    })
  }
}

/// A catalogue being read.
pub type Reading<'a> = Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'a>>;

/// One model as a tier mapping may name it: the efforts a mapping of it may hold, none among them
/// where it may name none, and what only some providers' catalogues tell.
#[derive(Clone, Debug, PartialEq)]
pub struct OfferedModel {
  pub id: String,
  pub efforts: Vec<Option<String>>,
  /// Whether it costs nothing, where the catalogue prices its models.
  pub free: Option<bool>,
  /// Whether its generation is served or superseded, where the catalogue tells the two apart.
  pub in_service: Option<bool>,
}

/// What a provider offers a tier mapping, and when the read was taken where it is past the provider's
/// own freshness mark.
#[derive(Clone, Debug, PartialEq)]
pub struct Offered {
  pub models: Vec<OfferedModel>,
  pub stale_read_at: Option<f64>,
}

/// Why a provider cannot answer here: the kebab-case code a screen words in its own language, and the
/// English sentence the log keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reason {
  pub code: String,
  pub text: String,
}

impl Reason {
  pub fn new(code: &str, text: &str) -> Self {
    Self {
      code: code.to_owned(),
      text: text.to_owned(),
    }
  }
}

/// What a provider offers being read.
pub type Offering<'a> = Pin<Box<dyn Future<Output = Result<Offered, String>> + Send + 'a>>;

/// The efforts one model takes being read.
/// A judgement being made: the sentence a refusal says, or nothing where it passes.
pub type Judged<'a> = Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>>;

pub type ModelEfforts<'a> = Pin<Box<dyn Future<Output = Option<Vec<String>>> + Send + 'a>>;

/// One usage window as a provider's headline reads it: its name, and the minutes it runs over where
/// its reading said.
#[derive(Clone, Copy, Debug)]
pub struct Window<'a> {
  pub name: &'a str,
  pub duration_minutes: Option<i64>,
}

/// Which of a slot's windows stand for its session and its week, by their places among them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Headline {
  pub session: Option<usize>,
  pub week: Option<usize>,
}

/// How long a place's prefix stays cached, and how long it may stand idle before a ping warms it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaceCache {
  pub ttl_minutes: i64,
  pub idle_minutes: i64,
}

/// One provider as the machine's questions are put to it.
pub trait Provider: Sync {
  /// The name the owner chooses it by, as the archive stores it.
  fn name(&self) -> &'static str;

  /// What its policy file declares.
  fn declared(&self) -> &'static Declared;

  /// Why this machine cannot use it, or nothing where it can.
  fn missing(&self, machine: &Machine) -> Option<Reason>;

  /// Its catalogue as this machine holds it, read afresh. A login's configuration directory is named
  /// where the provider's catalogue is kept under one.
  fn catalogue<'a>(&'a self, machine: &'a Machine, config_dir: Option<&'a Path>) -> Reading<'a> {
    let _ = (machine, config_dir);
    let name = self.name();

    Box::pin(async move { Err(format!("{name} keeps no catalogue on this machine")) })
  }

  /// Whether its catalogue is kept under a login's configuration directory, which the caller then
  /// names.
  fn catalogue_under_login(&self) -> bool {
    false
  }

  /// Where its models are the registry's registrations, which each side reads from its own archive,
  /// the reason it gives where none is offered: it answers only where one is, with an address or
  /// naming a card service, and offers those. Nothing for a provider whose models are not.
  fn unregistered(&self) -> Option<Reason> {
    None
  }

  /// The file under the data root's `env/` its own key is read from, where it reads one: a console writing the
  /// deployment's secrets sets the key there. Nothing for a provider that reads no key of its own.
  fn key_file(&self) -> Option<&'static str> {
    None
  }

  /// Whether its models are served by agency's gateway: the registry's registrations, or the members of a
  /// road of its own. A gate on such a provider is one request through the gateway, never a program.
  fn models_on_gateway(&self) -> bool {
    self.unregistered().is_some()
  }

  /// Where a run, or a worker's pin, may name one of its models — the registry's registrations, or a
  /// model its own policy declares — the sentence it says of a name it holds no address for, the name
  /// following it. Nothing for a provider that takes no model of its own.
  fn not_registered(&self) -> Option<&'static str> {
    None
  }

  /// Whether an agent's pin may name one of its models, which the pin's runs then go out on: a provider
  /// that names its models one by one, and one whose catalogue lists them.
  fn pins_models(&self) -> bool {
    self.not_registered().is_some()
  }

  /// Whether one of its models is held by that name, where its models are its own rather than the
  /// registry's: a pin or a job naming one of them is taken.
  fn holds_model(&self, machine: &Machine, model: &str) -> bool {
    let _ = (machine, model);

    false
  }

  /// Where its models differ in the efforts they take, the efforts this one takes; nothing where every
  /// model takes the provider's words whole.
  fn model_efforts<'a>(
    &'a self,
    machine: &'a Machine,
    config_dir: Option<&'a Path>,
    model: &'a str,
  ) -> ModelEfforts<'a> {
    let _ = (machine, config_dir, model);

    Box::pin(async { None })
  }

  /// Whether a road of it may name an advisor its program consults (docs/provider-order.md, 「The advisor」):
  /// none takes one but a provider whose program does.
  fn takes_advisor(&self) -> bool {
    false
  }

  /// The family words a road of it may name an advisor by beside a model of its own; none where it takes no
  /// advisor.
  fn advisor_words(&self) -> &'static [String] {
    &[]
  }

  /// Why a road of it on `model` may not name `advisor`, or nothing where it may; a provider whose program
  /// takes no advisor refuses every one. A login's configuration directory is named where the provider's
  /// catalogue is kept under one.
  fn advisor_refusal<'a>(
    &'a self,
    machine: &'a Machine,
    config_dir: Option<&'a Path>,
    model: &'a str,
    advisor: &'a str,
  ) -> Judged<'a> {
    let _ = (machine, config_dir, model, advisor);
    let name = self.name();

    Box::pin(async move { Some(format!("{name} takes no advisor.")) })
  }

  /// What it offers a tier mapping, read afresh at the moment given in epoch milliseconds.
  fn offered<'a>(
    &'a self,
    machine: &'a Machine,
    config_dir: Option<&'a Path>,
    now_ms: f64,
  ) -> Offering<'a> {
    let _ = (machine, config_dir, now_ms);
    let name = self.name();

    Box::pin(async move { Err(format!("{name} offers no catalogue on this machine")) })
  }

  /// Which of the windows its slot reported stand for its session and its week; none for a provider
  /// that publishes no window.
  fn headline(&self, windows: &[Window]) -> Headline {
    let _ = windows;

    Headline::default()
  }

  /// How its account's balance is read, where it offers one (docs/usage-windows.md, 「Balances」); none for
  /// a provider that offers none.
  fn balance(&self) -> Option<&'static dyn balance::OffersBalance> {
    None
  }

  /// Whether it holds logins of its own, so that a run, a slot and its windows are each one login's.
  fn holds_logins(&self) -> bool {
    false
  }

  /// The entry its login home holds, where it holds one, by which a folder under the data root's `env/` is
  /// told as a login home and passed over when the saved values are read (docs/agent-control.md, 「A record
  /// holds no secret」).
  fn login_home_mark(&self) -> Option<&'static str> {
    None
  }

  /// The session one event its program prints in a run of its own names, where it names one.
  fn session_of_event(&self, event: &serde_json::Value) -> Option<String> {
    let _ = event;

    None
  }

  /// The tools of its program whose calls wait long, which a run on the gateway names in every request;
  /// none for a program that names none (docs/agent-control.md, 「What an agent provider is made of」).
  fn long_tools(&self) -> &'static [String] {
    &[]
  }

  /// What a person watching the agent's view is shown of one line its program prints in a run of its
  /// own, where the line says anything to one.
  fn progress_of(&self, line: &str) -> Option<String> {
    let _ = line;

    None
  }

  /// Whether what one turn of it spent is its last request alone, the program counting its requests one
  /// by one, rather than the whole of what it reports spending.
  fn spends_by_last_request(&self) -> bool {
    false
  }

  /// How many minutes a prefix a model of it holds stays cached, as the provider publishes it; none for
  /// a model it publishes no lifetime for, whose session is never closed for age alone.
  fn cache_minutes(&self, model: &str) -> Option<i64> {
    let _ = model;

    None
  }

  /// How long a co-worker place's prefix a model of it wrote stays cached and how long it may stand idle
  /// before a ping warms it — on a provider that routes to upstreams, as the upstream that took it publishes
  /// — or why there is no clock to keep it warm by (docs/coworker-pool.md, 「Reserving places and keepalive」).
  fn place_cache(&self, model: &str, upstream: Option<&str>) -> Result<PlaceCache, String> {
    let _ = (model, upstream);

    Err(format!("{} publishes no cache lifetime", self.name()))
  }

  /// Whether a rest of the Claude account holds the pings of its places.
  fn rests_with_account(&self) -> bool {
    false
  }

  /// Whether an agent's own thinking switch reaches a run of it (docs/model-tiers.md, 「An agent's own
  /// thinking and effort」).
  fn takes_thinking(&self) -> bool {
    true
  }

  /// The effort word a run of it goes out with, given the word its tier mapping named, the agent's own
  /// choice of thinking and level, and the levels the run's model takes: the agent's level where the
  /// model takes it, the mapping's word otherwise.
  fn run_effort(
    &self,
    mapped: Option<&str>,
    thinking: Option<bool>,
    level: Option<&str>,
    levels: &[String],
  ) -> Option<String> {
    let _ = thinking;

    match level {
      Some(level) if levels.iter().any(|one| one == level) => Some(level.to_owned()),
      _ => mapped.map(str::to_owned),
    }
  }

  /// The Claude Code effort word a run of the backend word goes out with, where the provider's words and
  /// Claude Code's differ; none where its words are Claude Code's own.
  fn claude_effort(&self, word: &str) -> Option<&'static str> {
    let _ = word;

    None
  }

  /// The files and folders its program keeps of one session it ran in `cwd` on `model`, wherever they
  /// would stand, whether or not they do; none for a provider whose program keeps nothing of a
  /// session on this machine (docs/supervisors.md, 「A session in each room」). `logins` are the
  /// configuration homes of the logins the caller holds, which a provider whose program runs under a
  /// login looks in as well as the machine's own (docs/supervisors.md, 「Starting it afresh」).
  fn session_records(
    &self,
    machine: &Machine,
    logins: &[PathBuf],
    cwd: &Path,
    session: &str,
    model: &str,
  ) -> Vec<PathBuf> {
    let _ = (machine, logins, cwd, session, model);

    vec![]
  }

  /// The files and folders its program keeps of every session it ran in `cwd`, on any model and under any
  /// of `logins` as `session_records` reads them, where they stand, whether or not anything recorded the
  /// session; none for a provider whose program keeps nothing of a session on this machine
  /// (docs/supervisors.md, 「A session in each room」).
  fn home_records(&self, machine: &Machine, logins: &[PathBuf], cwd: &Path) -> Vec<PathBuf> {
    let _ = (machine, logins, cwd);

    vec![]
  }

  /// The prompts its program kept in a history of what was put in while it worked in `cwd`, of one session
  /// where `session` names one, taken out of that history and of the history under each of `logins` as
  /// `session_records` reads them, as the reset of a supervisor takes the words they hold
  /// (docs/supervisors.md, 「A session in each room」); how many. None for a provider whose program keeps
  /// no such history on this machine.
  fn forget_prompts(
    &self,
    machine: &Machine,
    logins: &[PathBuf],
    cwd: &Path,
    session: Option<&str>,
  ) -> Result<usize, String> {
    let _ = (machine, logins, cwd, session);

    Ok(0)
  }

  /// Whether a program standing in an agent's pane is its own, read off the command the program was
  /// started on; a provider that stands no program owns none (docs/agent-control.md, 「The interactive
  /// surface a provider declares」).
  fn owns_program(&self, machine: &Machine, args: &[String], env: &BTreeMap<String, String>) -> bool {
    let _ = (machine, args, env);

    false
  }
}

/// Whether a standing command is the pane program a provider declares standing a dsh session of its own:
/// the program its resident surface names, on its `dsh` subcommand, with a model choice whose route is the
/// provider's own name. Every provider whose turns go through dsh answers its program by this.
pub fn stands_dsh_session(provider: &dyn Provider, args: &[String]) -> bool {
  let Some(program) =
    provider.declared().resident.as_ref().and_then(|resident| resident.program.as_deref())
  else {
    return false;
  };
  let named = args.iter().position(|arg| arg == "--model").map(|at| at + 1);
  let choice = named
    .and_then(|at| args.get(at))
    .and_then(|choice| serde_json::from_str::<Vec<String>>(choice).ok());

  args
    .first()
    .and_then(|path| Path::new(path).file_name())
    .is_some_and(|name| Some(name) == Path::new(program).file_name())
    && args.get(1).is_some_and(|arg| arg == "dsh")
    && choice.and_then(|choice| choice.first().cloned()).as_deref() == Some(provider.name())
}

/// The provider whose program a standing command is, where one owns it.
pub fn standing_provider(
  machine: &Machine,
  args: &[String],
  env: &BTreeMap<String, String>,
) -> Option<&'static dyn Provider> {
  providers().into_iter().find(|one| one.owns_program(machine, args, env))
}

/// The providers the archive stores rows under, in the order its lists name them: the tier store's
/// providers, the default provider order.
pub const ARCHIVE_PROVIDERS: [&str; 5] = ["claude-code", "codex", "openrouter", "registered", "deepseek"];

/// The providers this build carries, in the order a screen offers them, Claude Code first as the
/// program a line's own code spawns on.
pub fn providers() -> [&'static dyn Provider; 5] {
  [
    &claude_code::ClaudeCode,
    &registered::Registered,
    &codex::Codex,
    &openrouter::OpenRouter,
    &deepseek::DeepSeek,
  ]
}

/// The provider of a name, where this build carries one.
pub fn provider(name: &str) -> Option<&'static dyn Provider> {
  providers().into_iter().find(|one| one.name() == name)
}

/// Every provider with what it says about this machine, in the providers' order.
pub fn presence(machine: &Machine) -> Vec<(&'static str, Option<Reason>)> {
  providers().into_iter().map(|one| (one.name(), one.missing(machine))).collect()
}

/// A number of a catalogue as JSON carries it: whole where it is whole.
pub fn number(value: f64) -> Value {
  if value.fract() == 0.0 && value.abs() < 9.0e15 {
    Value::from(value as i64)
  } else {
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
  }
}
