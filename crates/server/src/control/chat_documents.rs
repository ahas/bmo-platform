//! The supervisor's own documents as the owner keeps them (docs/agent-documents.md, 「A supervisor of the window shape」): the
//! opening, the absolute rules and the manual, each read from the store's `chat_documents` where a text is
//! kept and from its file otherwise, with a save that overwrites it and a reset that lets the file stand
//! again. The chat line composes every launch with what this route answers (docs/api.md, 「The chat's
//! documents」).

use std::path::Path;

use axum::http::{Method, StatusCode};
use axum::response::Response;
use policy::base_prompts::BASE_PROMPTS;
use serde_json::{Value, json};
use store::{one, run};

use super::sessions::asked_by;
use super::{Call, Refused, Route, answer};
use crate::lines::Trees;
use crate::now_ms;

/// Each document the owner may keep a text of: the name the store and this route use, and where its file
/// stands under the code tree.
const DOCUMENTS: [(&str, &str); 3] = [
  ("opening", "prompts/agents/chat/context.md"),
  ("absolute-rules", "prompts/rules/absolute-rules.md"),
  ("manual", "prompts/agents/chat/manual.md"),
];

// The document a name picks, where it names one of the three.
fn document(name: &str) -> Option<&'static (&'static str, &'static str)> {
  DOCUMENTS.iter().find(|(key, _)| *key == name)
}

/// One document as the screen reads it: the stored text where one is kept, the file's otherwise, and where
/// that text comes from.
async fn view(db: &sea_orm::DatabaseConnection, code: &Path, name: &str) -> Result<Value, Refused> {
  let Some((key, path)) = document(name) else {
    return Err(Refused::new(
      StatusCode::NOT_FOUND,
      format!("agency keeps no chat document named {name}"),
    ));
  };
  let row = one(
    db,
    r#"SELECT "text", "updated_at" FROM "chat_documents" WHERE "name" = $1"#,
    vec![(*key).into()],
  )
  .await?;
  let stored = row.as_ref().map(|row| row.try_get::<String>("", "text")).transpose()?;

  if let Some(text) = stored {
    let updated_at: Option<i64> = row
      .as_ref()
      .map(|row| row.try_get::<Option<i64>>("", "updated_at"))
      .transpose()?
      .flatten();

    return Ok(
      json!({ "name": name, "kind": "chat", "source": "stored", "version": updated_at, "text": text }),
    );
  }

  let file = std::fs::read_to_string(code.join(path)).unwrap_or_default();

  Ok(json!({ "name": name, "kind": "chat", "source": "file", "version": Value::Null, "text": file.trim() }))
}

/// Every chat document the owner may keep.
pub struct ChatDocuments;

impl Route for ChatDocuments {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/chat-documents";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let db = call.store().clone();
    let code = Trees::of(call.ground()).code;
    let mut items = Vec::with_capacity(DOCUMENTS.len());

    for (name, _) in DOCUMENTS {
      items.push(view(&db, &code, name).await?);
    }

    Ok(answer(StatusCode::OK, json!({ "items": items, "more": false })))
  }
}

crate::moved!(ChatDocuments);

/// One document saved or reset, answered as it then stands.
pub struct ChangeChatDocument;

impl Route for ChangeChatDocument {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/chat-documents/{documentName}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let name = call.param("documentName").unwrap_or_default();
    let db = call.store().clone();
    let code = Trees::of(call.ground()).code;
    let Some((key, _)) = document(name) else {
      return Err(Refused::new(
        StatusCode::NOT_FOUND,
        format!("agency keeps no chat document named {name}"),
      ));
    };

    match body.get("action").and_then(Value::as_str) {
      Some("save") => {
        let text = body.get("text").and_then(Value::as_str).map(str::trim).unwrap_or_default();

        if text.is_empty() || text.chars().count() > BASE_PROMPTS.text_most {
          return Err(Refused::new(
            StatusCode::BAD_REQUEST,
            "a chat document needs a text within its bounds",
          ));
        }

        run(
          &db,
          r#"INSERT INTO "chat_documents" ("name", "text", "updated_at", "updated_by")
             VALUES ($1, $2, $3, $4)
             ON CONFLICT ("name") DO UPDATE
               SET "text" = excluded."text", "updated_at" = excluded."updated_at", "updated_by" = excluded."updated_by""#,
          vec![(*key).into(), text.into(), (now_ms() / 1000).into(), asked_by(&body).into()],
        )
        .await?;
      }
      Some("reset") => {
        run(&db, r#"DELETE FROM "chat_documents" WHERE "name" = $1"#, vec![(*key).into()]).await?;
      }
      _ => {
        return Err(Refused::new(StatusCode::BAD_REQUEST, "a chat document change is a save or a reset"));
      }
    }

    Ok(answer(StatusCode::OK, view(&db, &code, name).await?))
  }
}

crate::moved!(ChangeChatDocument);
