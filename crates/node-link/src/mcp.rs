//! An MCP server as a personal app (docs/person-nodes.md, 「MCP servers」): the `mcp.toml` its folder of 'my apps'
//! holds in place of a manifest, what a version of it asks for, the manifest the runner makes from the tools it lists,
//! and `node-link mcp`, which runs inside a call's confinement and speaks MCP to the server as a client — a local
//! server started in the version's folder on its standard input and output, a remote one reached over streamable
//! HTTP through the call's proxy — and answers as a personal tool's executable does.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{ExitCode, Stdio};

use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, Tool};
use rmcp::service::{RoleClient, RunningService, ServiceError};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;

/// The file an MCP server's folder holds in place of a manifest.
pub const MCP_FILE: &str = "mcp.toml";

/// The file beside a version's made manifest naming each entry's server tool, by the entry's name.
pub const MCP_TOOLS: &str = "mcp-tools.json";

/// The folder a version's made manifest writes its entries' input schemas in.
const SCHEMAS: &str = "schemas";

/// The namespace and the capability every entry of a personal tool stands in.
const NAMESPACE: &str = "my";
const CAPABILITY: &str = "my.own";

/// The variable naming the folder a call's secrets stand in, and the folder where it names none.
const SECRETS_VARIABLE: &str = "BMO_SECRETS";
const SECRETS_AT: &str = "/secrets";

/// An MCP server's `mcp.toml`.
#[derive(Clone, Debug, Deserialize)]
pub struct McpFile {
  pub server: Server,
  #[serde(default)]
  pub permissions: Permissions,
}

/// Its `[server]`: its name, the folder's; its title and description, each a string or a table of locales; what it
/// is written in; and how it is reached.
#[derive(Clone, Debug, Deserialize)]
pub struct Server {
  pub name: String,
  #[serde(default)]
  pub title: Option<toml::Value>,
  #[serde(default)]
  pub description: Option<toml::Value>,
  #[serde(default)]
  pub language: Option<String>,
  pub transport: Transport,
  /// A local server's command, words run in the placed folder.
  #[serde(default)]
  pub command: Vec<String>,
  /// A local server's build, a shell line run as an app's build is.
  #[serde(default)]
  pub build: Option<String>,
  /// A remote server's address.
  #[serde(default)]
  pub url: Option<String>,
  #[serde(default)]
  pub auth: Auth,
  /// The header's name, for `auth = "header"`.
  #[serde(default)]
  pub header: Option<String>,
  /// The name of the secret the header carries, for `bearer` and `header`.
  #[serde(default)]
  pub secret: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
  Local,
  Remote,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
  #[default]
  None,
  Bearer,
  Header,
}

/// The network and the secrets a version asks for, as a manifest's `[permissions]` names them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Permissions {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub network: Vec<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub secrets: Vec<String>,
}

impl Permissions {
  fn is_empty(&self) -> bool {
    self.network.is_empty() && self.secrets.is_empty()
  }
}

impl McpFile {
  /// The folder's `mcp.toml` read and held to its rules: a local server names a command; a remote one an `http` or
  /// `https` address, and for `bearer` and `header` a secret's name, with a header's name for `header`.
  pub fn read(folder: &Path) -> Result<Self, String> {
    let text = std::fs::read_to_string(folder.join(MCP_FILE))
      .map_err(|why| format!("{} holds no {MCP_FILE}: {why}", folder.display()))?;
    let file: Self = toml::from_str(&text).map_err(|why| format!("its {MCP_FILE} does not read: {why}"))?;

    file.held()?;
    Ok(file)
  }

  fn held(&self) -> Result<(), String> {
    let server = &self.server;

    match server.transport {
      Transport::Local => {
        if server.command.first().is_none_or(|word| word.trim().is_empty()) {
          return Err(format!("its {MCP_FILE} names a local server with no command"));
        }
      }
      Transport::Remote => {
        let url = server.url.as_deref().unwrap_or_default();
        let read = reqwest::Url::parse(url)
          .map_err(|why| format!("its {MCP_FILE}'s url {url:?} does not read: {why}"))?;

        if !matches!(read.scheme(), "http" | "https") || read.host_str().is_none() {
          return Err(format!("its {MCP_FILE}'s url {url:?} is no http or https address"));
        }

        if server.auth != Auth::None && server.secret.as_deref().is_none_or(|secret| !plain(secret)) {
          return Err(format!("its {MCP_FILE} names a header's secret by no plain name"));
        }

        if server.auth == Auth::Header
          && server
            .header
            .as_deref()
            .is_none_or(|header| reqwest::header::HeaderName::from_bytes(header.as_bytes()).is_err())
        {
          return Err(format!("its {MCP_FILE} names no header the secret is sent in"));
        }
      }
    }

    Ok(())
  }

  /// What a version of it asks for: its `[permissions]`, and for a remote server its address's host, with its port
  /// where it names one, among the network and its secret among the secrets.
  pub fn asks(&self) -> Permissions {
    let mut asks = self.permissions.clone();
    let server = &self.server;

    if server.transport == Transport::Remote {
      if let Some(address) = server.url.as_deref().and_then(remote_address) {
        asks.network.push(address);
      }

      if server.auth != Auth::None
        && let Some(secret) = &server.secret
      {
        asks.secrets.push(secret.clone());
      }
    }

    for listed in [&mut asks.network, &mut asks.secrets] {
      let mut seen = std::collections::BTreeSet::new();

      listed.retain(|one| seen.insert(one.clone()));
    }

    asks
  }
}

// A remote server's address as a manifest's network writes it: its host, with the port its url names.
fn remote_address(url: &str) -> Option<String> {
  let read = reqwest::Url::parse(url).ok()?;
  let host = read.host_str()?;

  Some(match read.port() {
    Some(port) => format!("{host}:{port}"),
    None => host.to_owned(),
  })
}

// A secret's name a file of the secrets' folder stands under.
fn plain(name: &str) -> bool {
  crate::runner::plain_name(name)
}

/// A listed tool's name made into an entry's alphabet, as the tool host makes it (bmo-pocket's docs/api.md, 「How a
/// listed tool reads as an entry」): an upper-case letter lower-cased, with `_` before it where it follows a lower-case
/// letter or a digit, and every other character outside the alphabet made `_`.
pub fn entry_name_of(listed: &str) -> String {
  let mut name = String::new();
  let mut in_word = false;

  for one in listed.chars() {
    if one.is_ascii_uppercase() {
      if in_word {
        name.push('_');
      }

      name.push(one.to_ascii_lowercase());
      in_word = false;
    } else if one.is_ascii_lowercase() || one.is_ascii_digit() {
      name.push(one);
      in_word = true;
    } else {
      name.push('_');
      in_word = false;
    }
  }

  name
}

/// What a version's listing makes: its manifest's text, each entry's input schema by its path in the version, and
/// each entry's server tool by the entry's name.
#[derive(Debug)]
pub struct Made {
  pub manifest: String,
  pub schemas: BTreeMap<String, String>,
  pub names: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct MadeManifest<'a> {
  tool: MadeTool<'a>,
  entry: Vec<MadeEntry>,
  #[serde(skip_serializing_if = "Permissions::is_empty")]
  permissions: Permissions,
}

#[derive(Serialize)]
struct MadeTool<'a> {
  name: &'a str,
  protocol: u32,
  #[serde(skip_serializing_if = "Option::is_none")]
  language: Option<&'a str>,
  run: [&'static str; 2],
}

#[derive(Serialize)]
struct MadeEntry {
  namespace: &'static str,
  name: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  title: Option<String>,
  description: String,
  capabilities: [&'static str; 1],
  input: String,
}

/// The manifest a version of the server is described by, made from the tools it listed: one entry a tool, in the
/// namespace `my`, named as the tool host names a listed tool's entry, the first of two taking one name keeping it;
/// its title the tool's, else its annotations'; its description the tool's, else its title, else its name; and its
/// input schema the tool's, written beside it. Its `[permissions]` are what the version asks for. Refused where no
/// tool is left to make an entry of.
pub fn made(file: &McpFile, listed: &[Tool]) -> Result<Made, String> {
  let mut entries = Vec::new();
  let mut schemas = BTreeMap::new();
  let mut names = BTreeMap::new();

  for tool in listed {
    let own = tool.name.to_string();
    let name = entry_name_of(&own);

    if name.is_empty() || names.contains_key(&name) {
      continue;
    }

    let title = tool
      .title
      .clone()
      .or_else(|| tool.annotations.as_ref().and_then(|notes| notes.title.clone()));
    let description = tool
      .description
      .as_ref()
      .map(|description| description.to_string())
      .filter(|description| !description.trim().is_empty())
      .or_else(|| title.clone())
      .unwrap_or_else(|| own.clone());
    let input = format!("{SCHEMAS}/{name}.json");
    let schema = serde_json::to_string_pretty(&Value::Object((*tool.input_schema).clone()))
      .map_err(|why| format!("the input schema of its tool {own} could not be written: {why}"))?;

    schemas.insert(input.clone(), schema);
    entries.push(MadeEntry {
      namespace: NAMESPACE,
      name: name.clone(),
      title,
      description,
      capabilities: [CAPABILITY],
      input,
    });
    names.insert(name, own);
  }

  if entries.is_empty() {
    return Err("its server lists no tool an entry can be made of".to_owned());
  }

  let manifest = MadeManifest {
    tool: MadeTool {
      name: &file.server.name,
      protocol: 1,
      language: file.server.language.as_deref(),
      run: [crate::runner::OWN_BINARY_AT, "mcp"],
    },
    entry: entries,
    permissions: file.asks(),
  };
  let manifest =
    toml::to_string(&manifest).map_err(|why| format!("its manifest could not be written: {why}"))?;

  Ok(Made {
    manifest,
    schemas,
    names,
  })
}

/// What a version's listing answers on its standard output: the tools its server listed.
#[derive(Debug, Deserialize, Serialize)]
pub struct Listing {
  pub tools: Vec<Tool>,
}

/// The call a personal tool's executable is handed on its standard input, of the kind `call` (bmo-pocket's
/// docs/api.md, 「What a tool is handed」).
#[derive(Deserialize)]
struct Call {
  kind: String,
  #[serde(default)]
  entry: String,
  #[serde(default)]
  arguments: Option<serde_json::Map<String, Value>>,
}

/// `node-link mcp`, inside the confinement, in the version's folder: with `list`, its server's tools listed, every
/// page, and printed as one JSON object, `{"tools": [...]}`, ending 1 with why on its errors where they cannot be;
/// otherwise the call on its standard input made a `tools/call` of the server's tool the entry names and its result
/// printed as one MCP tool result, an error the server or the road to it answers a result with `isError` true.
pub async fn adapt(list: bool) -> ExitCode {
  let folder = match std::env::current_dir() {
    Ok(folder) => folder,
    Err(why) => {
      eprintln!("node-link mcp: the version's folder is not found: {why}");
      return ExitCode::FAILURE;
    }
  };

  if list {
    return match listing(&folder).await {
      Ok(listing) => {
        println!("{listing}");
        ExitCode::SUCCESS
      }
      Err(why) => {
        eprintln!("node-link mcp: {why}");
        ExitCode::FAILURE
      }
    };
  }

  let mut asked = String::new();
  let result = match tokio::io::stdin().read_to_string(&mut asked).await {
    Ok(_) => called(&folder, &asked).await,
    Err(why) => Err(format!("the call could not be read: {why}")),
  };

  println!(
    "{}",
    result.unwrap_or_else(|why| json!({ "content": [{ "type": "text", "text": why }], "isError": true }))
  );
  ExitCode::SUCCESS
}

async fn listing(folder: &Path) -> Result<Value, String> {
  let file = McpFile::read(folder)?;
  let client = connect(&file, folder).await?;
  let tools = client.list_all_tools().await;
  let _ = client.cancel().await;
  let tools = tools.map_err(|why| format!("its server's tools could not be listed: {}", said(why)))?;

  serde_json::to_value(Listing { tools }).map_err(|why| format!("the listing could not be written: {why}"))
}

async fn called(folder: &Path, asked: &str) -> Result<Value, String> {
  let call: Call = serde_json::from_str(asked).map_err(|why| format!("the call does not read: {why}"))?;

  if call.kind != "call" {
    return Err(format!("an MCP server answers no {:?}", call.kind));
  }

  let entry = call.entry.rsplit('/').next().unwrap_or_default();
  let names: BTreeMap<String, String> = std::fs::read_to_string(folder.join(MCP_TOOLS))
    .ok()
    .and_then(|text| serde_json::from_str(&text).ok())
    .ok_or_else(|| format!("the version's {MCP_TOOLS} does not read"))?;
  let tool = names
    .get(entry)
    .ok_or_else(|| format!("the server has no tool for the entry {:?}", call.entry))?;
  let file = McpFile::read(folder)?;
  let client = connect(&file, folder).await?;
  let mut asking = CallToolRequestParams::new(tool.clone());

  asking.arguments = call.arguments;

  let answered = client.call_tool(asking).await;
  let _ = client.cancel().await;
  let answered = answered.map_err(said)?;
  let mut result = json!({
    "content": answered.content,
    "isError": answered.is_error.unwrap_or(false),
  });

  if let Some(structured) = answered.structured_content {
    result["structuredContent"] = structured;
  }

  Ok(result)
}

// What an error the server or the road to it answered says: the server's own message where it answered one.
fn said(error: ServiceError) -> String {
  match error {
    ServiceError::McpError(answered) => answered.message.into_owned(),
    other => other.to_string(),
  }
}

// The server reached and initialised: a local one's command started in the version's folder, the first word read
// there where a file of that name stands, on its standard input and output, its errors the adapter's own; a remote
// one over streamable HTTP at its url, through the proxy the call's variables name, with its secret in its header.
async fn connect(file: &McpFile, folder: &Path) -> Result<RunningService<RoleClient, ()>, String> {
  let server = &file.server;

  match server.transport {
    Transport::Local => {
      let (first, rest) = server.command.split_first().ok_or("the server names no command")?;
      let own = folder.join(first);
      let program = if plain(first) && own.is_file() { own.into_os_string() } else { first.into() };
      let mut command = tokio::process::Command::new(program);

      command.args(rest).current_dir(folder).kill_on_drop(true);

      let (transport, _) = TokioChildProcess::builder(command)
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|why| format!("its server {first} could not be started: {why}"))?;

      ().serve(transport)
        .await
        .map_err(|why| format!("its server could not be initialised: {why}"))
    }
    Transport::Remote => {
      let url = server.url.clone().unwrap_or_default();
      let mut headers = reqwest::header::HeaderMap::new();

      if let Some(secret) = server.secret.as_deref().filter(|_| server.auth != Auth::None) {
        let value = secret_value(secret)?;
        let (name, value) = match server.auth {
          Auth::Bearer => (reqwest::header::AUTHORIZATION, format!("Bearer {value}")),
          _ => (
            reqwest::header::HeaderName::from_bytes(
              server.header.as_deref().unwrap_or_default().as_bytes(),
            )
            .map_err(|_| "its header's name does not read".to_owned())?,
            value,
          ),
        };
        let mut value = reqwest::header::HeaderValue::from_str(&value)
          .map_err(|_| format!("the secret {secret} cannot stand in a header"))?;

        value.set_sensitive(true);
        headers.insert(name, value);
      }

      let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|why| format!("no HTTP client could be made: {why}"))?;
      let transport = StreamableHttpClientTransport::with_client(
        client,
        StreamableHttpClientTransportConfig::with_uri(url),
      );

      ().serve(transport)
        .await
        .map_err(|why| format!("its server could not be initialised: {why}"))
    }
  }
}

// A secret the call carries, read from its folder and its surrounding whitespace left out; never said.
fn secret_value(name: &str) -> Result<String, String> {
  let folder = std::env::var_os(SECRETS_VARIABLE).unwrap_or_else(|| SECRETS_AT.into());
  let bytes = std::fs::read(Path::new(&folder).join(name))
    .map_err(|_| format!("the call carries no secret {name}"))?;

  String::from_utf8(bytes)
    .map(|text| text.trim().to_owned())
    .map_err(|_| format!("the secret {name} is no text"))
}
