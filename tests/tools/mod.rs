//! Agency's tools (docs/tools.md): their side of the tool host's call protocol, the procedures, the
//! worker's ask, the relay and the DeepSeek top-up, each held to its manifest, and each
//! executable answering a request as the tool host hands it one.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, StatusCode, header};

mod app_put;
mod call;
mod deepseek_top_up;
mod deepseek_top_up_payment;
mod jobs;
mod manifests;
mod procedures;
mod relay;
mod skill_put;
mod worker_ask;

/// The token the data root's `env/agency.env` names in `agency_named_by_its_file`, written quoted.
const FILE_TOKEN: &str = "from-file";

/// A stand-in of agency's server that refuses every request and writes down the `Authorization` each was asked
/// under, and a data root whose `env/agency.env` names it as a deployment writes the file: its port, and its
/// token quoted. A tool that reads the data root through `tool_call::Control` asks it under `Bearer from-file`.
struct NamedByFile {
  root: tempfile::TempDir,
  authorizations: Arc<Mutex<Vec<String>>>,
}

impl NamedByFile {
  /// At least one request was asked, and every one under the token the file names, unquoted.
  fn assert_asked_under_the_files_token(&self) {
    let authorizations = self.authorizations.lock().unwrap();

    assert!(!authorizations.is_empty(), "the server the file names was never asked");
    assert!(
      authorizations.iter().all(|asked| *asked == format!("Bearer {FILE_TOKEN}")),
      "{authorizations:?}"
    );
  }
}

async fn agency_named_by_its_file() -> NamedByFile {
  let authorizations = Arc::new(Mutex::new(Vec::new()));
  let app = Router::new().fallback({
    let authorizations = authorizations.clone();

    move |headers: HeaderMap| async move {
      let asked = headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok());

      authorizations.lock().unwrap().push(asked.unwrap_or_default().to_owned());

      (
        StatusCode::INTERNAL_SERVER_ERROR,
        axum::Json(serde_json::json!({ "error": "a stand-in" })),
      )
    }
  });
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let root = tempfile::tempdir().unwrap();

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  std::fs::create_dir_all(root.path().join("env")).unwrap();
  std::fs::write(
    root.path().join("env").join("agency.env"),
    format!("AGENCY_CONTROL_PORT={port}\nAGENCY_CONTROL_TOKEN=\"{FILE_TOKEN}\"\n"),
  )
  .unwrap();

  NamedByFile {
    root,
    authorizations,
  }
}
