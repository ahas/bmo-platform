//! The mail service, bmo-mail, as the top-up reads DeepSeek's sign-in mail from it (docs/api.md, 「Calls agency
//! makes」; docs/tools.md, 「Where a person is asked」): its address, its token and the owner key it is read for,
//! each from the tool's environment, else from the mail unit's settings file under the data root. The token is
//! read to be sent and nowhere else; a body read here is handed back whole and never written anywhere.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;
use url::Url;

/// The variables naming the mail service's address, its token and the owner key it is read for, and the mail
/// unit's settings file under the data root, which holds each where no variable does.
pub const ADDRESS_VARIABLE: &str = "BMO_MAIL_ADDRESS";
pub const TOKEN_VARIABLE: &str = "BMO_MAIL_TOKEN";
pub const OWNER_VARIABLE: &str = "BMO_MAIL_OWNER";
pub const SETTINGS_FILE: &str = "env/mail.toml";
/// The role the mail service is read with: its administrator's, whose key the settings name.
const ROLE: &str = "administrator";

#[derive(Default, Deserialize)]
struct Settings {
  #[serde(default)]
  server: Server,
  #[serde(default)]
  accounts: Accounts,
}

#[derive(Default, Deserialize)]
struct Server {
  listen: Option<String>,
  token: Option<String>,
}

#[derive(Default, Deserialize)]
struct Accounts {
  administrator: Option<String>,
}

/// The mail service as the tool reaches it.
#[derive(Clone)]
pub struct MailService {
  pub address: String,
  pub token: String,
  pub owner: String,
}

impl std::fmt::Debug for MailService {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("MailService")
      .field("address", &self.address)
      .finish_non_exhaustive()
  }
}

/// A mail's bodies, as the service answers them.
pub struct Bodies {
  pub html: Option<String>,
  pub text: Option<String>,
}

impl MailService {
  /// The mail service a data root names, each variable `variable` answers standing first; none where its address,
  /// its token or its owner key stands nowhere.
  pub fn read(data_root: &Path, variable: impl Fn(&str) -> Option<String>) -> Option<Self> {
    let settings: Settings = std::fs::read_to_string(data_root.join(SETTINGS_FILE))
      .ok()
      .and_then(|text| toml::from_str(&text).ok())
      .unwrap_or_default();
    let given =
      |name: &str| variable(name).map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
    let address = given(ADDRESS_VARIABLE)
      .or_else(|| settings.server.listen.map(|listen| format!("http://{listen}")))?;
    let token = given(TOKEN_VARIABLE).or(settings.server.token).filter(|token| !token.is_empty())?;
    let owner = given(OWNER_VARIABLE)
      .or(settings.accounts.administrator)
      .filter(|owner| !owner.is_empty())?;

    Some(Self {
      address: address.trim_end_matches('/').to_owned(),
      token,
      owner,
    })
  }

  /// A route's address, each segment of the path given set in as one segment.
  fn route(&self, segments: &[&str]) -> Option<Url> {
    let mut route = Url::parse(&self.address).ok()?;

    route.path_segments_mut().ok()?.pop_if_empty().extend(segments);
    Some(route)
  }

  /// A route's answer, or none where the service cannot be asked or refuses.
  async fn read_route(&self, route: Url, query: &[(&str, String)]) -> Option<Value> {
    let answer = reqwest::Client::new()
      .get(route)
      .query(query)
      .bearer_auth(&self.token)
      .header("Owner-Role", ROLE)
      .header("Owner-Key", &self.owner)
      .send()
      .await
      .ok()?;

    if !answer.status().is_success() {
      return None;
    }

    answer.json().await.ok()
  }

  /// The name of the owner's mail account carrying the address, matched without regard to case.
  pub async fn account_for(&self, address: &str) -> Option<String> {
    let accounts = self.read_route(self.route(&["accounts"])?, &[]).await?;

    accounts["items"].as_array()?.iter().find_map(|account| {
      let carried = account["address"].as_str()?;

      carried
        .eq_ignore_ascii_case(address.trim())
        .then(|| account["name"].as_str().map(str::to_owned))
        .flatten()
    })
  }

  /// The bodies of the account's mail from any of the senders that reached it after `after`, in epoch
  /// milliseconds, newest first, leaving out the messages `seen` holds and adding each one read to it.
  pub async fn fresh(
    &self,
    account: &str,
    senders: &[String],
    after: i64,
    seen: &mut BTreeSet<String>,
  ) -> Vec<Bodies> {
    let mut read = Vec::new();

    for sender in senders {
      let Some(listed) = self.route(&["accounts", account, "messages"]) else {
        continue;
      };
      let query = [
        ("q", format!("from:{sender}")),
        ("after", after.to_string()),
        ("limit", "20".to_owned()),
      ];
      let Some(page) = self.read_route(listed, &query).await else {
        continue;
      };

      for message in page["items"].as_array().into_iter().flatten() {
        let Some(id) = message["id"].as_str() else {
          continue;
        };

        if message["at"].as_i64().is_some_and(|at| at < after) || !seen.insert(id.to_owned()) {
          continue;
        }

        let Some(whole) = self.route(&["accounts", account, "messages", id]) else {
          continue;
        };

        if let Some(message) = self.read_route(whole, &[]).await {
          read.push(Bodies {
            html: message["html"].as_str().map(str::to_owned),
            text: message["text"].as_str().map(str::to_owned),
          });
        }
      }
    }

    read
  }
}
