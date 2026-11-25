//! The decisions agency hands up (docs/api.md, 「Hooks」). Each point a decision is asked at is a type of
//! its own, which names what it asks, what it is answered, and its default; a consumer registers its
//! answer at the point, and agency asks it there. A point nobody registered at answers its default,
//! never a failure. A consumer across the boundary registers at a point by its name and an address: agency
//! posts each ask to it as JSON and reads its answer back, and a call that fails answers what the point says an
//! unanswered ask stands for, its default unless it says otherwise. A point may declare that its registration
//! outlives a restart: its address is kept in the store's settings and its token in a file under the data root's
//! `env/`, and the server registers it again from them as it starts.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;

/// One point a decision is asked at.
pub trait HookPoint: 'static {
  /// What agency hands the consumer.
  type Ask: Send + 'static;
  /// What the consumer answers.
  type Answer: Send + 'static;
  /// The kebab-case name the point is known by.
  const NAME: &'static str;

  /// Whether a consumer's registration at the point outlives the server's restart.
  const KEPT: bool = false;

  /// What the point answers where nobody registered at it: a default that does nothing.
  fn default_answer(ask: Self::Ask) -> Self::Answer;

  /// What the point answers where a consumer registered at it and did not answer, or answered what does not
  /// read: its default, unless the point needs another.
  fn unanswered(ask: Self::Ask) -> Self::Answer {
    Self::default_answer(ask)
  }
}

/// A registered answer's future.
pub type Answering<A> = Pin<Box<dyn Future<Output = A> + Send>>;

type Registered<H> =
  Arc<dyn Fn(<H as HookPoint>::Ask) -> Answering<<H as HookPoint>::Answer> + Send + Sync>;

/// Every answer registered, by its point. Cloned freely: every clone is the one set of hooks.
#[derive(Clone, Default)]
pub struct Hooks {
  registered: Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>,
}

impl Hooks {
  /// The consumer's answer at a point, standing in for any registered there before.
  pub fn register<H, Answer, Answered>(&self, answer: Answer)
  where
    H: HookPoint,
    Answer: Fn(H::Ask) -> Answered + Send + Sync + 'static,
    Answered: Future<Output = H::Answer> + Send + 'static,
  {
    let boxed: Registered<H> = Arc::new(move |ask| Box::pin(answer(ask)));

    self
      .registered
      .write()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .insert(TypeId::of::<H>(), Box::new(boxed));
  }

  /// The point's registered answer taken away, so it answers its default again.
  pub fn unregister<H: HookPoint>(&self) {
    self
      .registered
      .write()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .remove(&TypeId::of::<H>());
  }

  /// Whether a consumer registered at the point.
  pub fn registered<H: HookPoint>(&self) -> bool {
    self
      .registered
      .read()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .contains_key(&TypeId::of::<H>())
  }

  /// The decision asked at a point: the registered answer's, or the point's default.
  pub async fn ask<H: HookPoint>(&self, ask: H::Ask) -> H::Answer {
    let registered = self
      .registered
      .read()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .get(&TypeId::of::<H>())
      .and_then(|answer| answer.downcast_ref::<Registered<H>>())
      .cloned();

    match registered {
      Some(answer) => answer(ask).await,
      None => H::default_answer(ask),
    }
  }
}

/// A consumer's answer at a point, reached at its address: each ask posted as `{ point, ask }`, under its
/// token where it names one, and the answer read from the `answer` of what it answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
  pub url: String,
  pub token: Option<String>,
}

/// One point a consumer across the boundary may register at, by its name.
pub struct Wired {
  pub name: &'static str,
  pub kept: bool,
  pub register: fn(&Hooks, Remote),
  pub unregister: fn(&Hooks),
  pub registered: fn(&Hooks) -> bool,
}

inventory::collect!(Wired);

/// Makes a point one a consumer across the boundary registers at: `crate::wired!(RoomTitle);`
#[macro_export]
macro_rules! wired {
  ($point:ty) => {
    $crate::control::inventory::submit! {
      $crate::hooks::Wired {
        name: <$point as $crate::hooks::HookPoint>::NAME,
        kept: <$point as $crate::hooks::HookPoint>::KEPT,
        register: $crate::hooks::register_remote::<$point>,
        unregister: $crate::hooks::Hooks::unregister::<$point>,
        registered: $crate::hooks::Hooks::registered::<$point>,
      }
    }
  };
}

/// Every point a consumer across the boundary may register at.
pub fn wired() -> impl Iterator<Item = &'static Wired> {
  inventory::iter::<Wired>.into_iter()
}

/// The point a consumer across the boundary names, where there is one.
pub fn wired_named(name: &str) -> Option<&'static Wired> {
  wired().find(|point| point.name == name)
}

/// A consumer's address registered at a point: every ask there posted to it.
pub fn register_remote<H>(hooks: &Hooks, remote: Remote)
where
  H: HookPoint,
  H::Ask: Serialize,
  H::Answer: DeserializeOwned,
{
  let client = reqwest::Client::new();
  let remote = Arc::new(remote);

  hooks.register::<H, _, _>(move |ask: H::Ask| {
    let (client, remote) = (client.clone(), remote.clone());

    // The ask is written out before anything is awaited, and kept for the default.
    let body = serde_json::to_value(&ask).map(|ask| json!({ "point": H::NAME, "ask": ask }));

    async move {
      let answered = match body {
        Ok(body) => asked::<H>(&client, &remote, &body).await,
        Err(why) => Err(why.to_string()),
      };

      match answered {
        Ok(answer) => answer,
        Err(why) => {
          eprintln!(
            "hook {}: {} did not answer ({why}); its unanswered default stands",
            H::NAME,
            remote.url
          );
          H::unanswered(ask)
        }
      }
    }
  });
}

async fn asked<H>(
  client: &reqwest::Client,
  remote: &Remote,
  body: &serde_json::Value,
) -> Result<H::Answer, String>
where
  H: HookPoint,
  H::Answer: DeserializeOwned,
{
  let mut call = client.post(&remote.url).json(body);

  if let Some(token) = &remote.token {
    call = call.bearer_auth(token);
  }

  let answer = call.send().await.map_err(|why| why.to_string())?;
  let status = answer.status();

  if !status.is_success() {
    return Err(format!("it answered {status}"));
  }

  let mut body: serde_json::Value = answer.json().await.map_err(|why| why.to_string())?;

  serde_json::from_value(body["answer"].take()).map_err(|why| why.to_string())
}

/// The settings family a kept registration is written under, by its point.
pub const KEPT_PREFIX: &str = "hook_kept:";

/// Where a kept registration's token stands under the data root.
pub fn kept_token_path(data_root: &std::path::Path, point: &str) -> std::path::PathBuf {
  data_root.join("env").join("agency-hooks").join(format!("{point}.token"))
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Kept {
  url: String,
  #[serde(default)]
  token_file: Option<String>,
}

/// A registration kept to outlive a restart: its address in the settings, its token in its own private file.
pub async fn keep<C: sea_orm::ConnectionTrait>(
  db: &C,
  data_root: &std::path::Path,
  point: &str,
  remote: &Remote,
) -> Result<(), crate::Refusal> {
  let token_file = match &remote.token {
    Some(token) => {
      let path = kept_token_path(data_root, point);
      let folder = path.parent().expect("a kept token stands in a folder");

      std::fs::create_dir_all(folder)
        .map_err(|why| crate::Refusal::failed(format!("{}: {why}", folder.display())))?;
      agent_host::disk::private_file(&path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, token.as_bytes()))
        .map_err(|why| crate::Refusal::failed(format!("{}: {why}", path.display())))?;
      Some(path.to_string_lossy().into_owned())
    }
    None => {
      match std::fs::remove_file(kept_token_path(data_root, point)) {
        Err(why) if why.kind() != std::io::ErrorKind::NotFound => {
          return Err(crate::Refusal::failed(why.to_string()));
        }
        _ => {}
      }

      None
    }
  };
  let kept = Kept {
    url: remote.url.clone(),
    token_file,
  };

  crate::queue::settings::set(db, &format!("{KEPT_PREFIX}{point}"), &json!(kept).to_string()).await
}

/// A kept registration let go: its setting and its token file.
pub async fn forget<C: sea_orm::ConnectionTrait>(
  db: &C,
  data_root: &std::path::Path,
  point: &str,
) -> Result<(), crate::Refusal> {
  crate::queue::settings::forget(db, &format!("{KEPT_PREFIX}{point}")).await?;

  match std::fs::remove_file(kept_token_path(data_root, point)) {
    Err(why) if why.kind() != std::io::ErrorKind::NotFound => Err(crate::Refusal::failed(why.to_string())),
    _ => Ok(()),
  }
}

/// Whether the store keeps a registration at a point.
pub async fn is_kept<C: sea_orm::ConnectionTrait>(db: &C, point: &str) -> Result<bool, crate::Refusal> {
  Ok(crate::queue::settings::get(db, &format!("{KEPT_PREFIX}{point}")).await?.is_some())
}

/// Every kept registration registered again, as the server starts: how many were.
pub async fn restore<C: sea_orm::ConnectionTrait>(hooks: &Hooks, db: &C) -> Result<usize, crate::Refusal> {
  let mut restored = 0;

  for point in wired().filter(|point| point.kept) {
    let Some(text) = crate::queue::settings::get(db, &format!("{KEPT_PREFIX}{}", point.name)).await? else {
      continue;
    };
    let Ok(kept) = serde_json::from_str::<Kept>(&text) else {
      eprintln!("hook {}: its kept registration does not read; it is not registered", point.name);
      continue;
    };
    let token = kept
      .token_file
      .as_deref()
      .and_then(|file| std::fs::read_to_string(file).ok())
      .map(|token| token.trim().to_owned())
      .filter(|token| !token.is_empty());

    (point.register)(
      hooks,
      Remote {
        url: kept.url,
        token,
      },
    );
    restored += 1;
  }

  Ok(restored)
}
