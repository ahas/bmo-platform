//! What a reload sorts the documents named by: which ship with the code, and which a chat session's system
//! prompt or turn carries. Agency holds no persona, so no persona file is one of its documents.

use agency_server::verbs::reload::{is_chat_prompt_document, is_chat_turn_document, tree_documents};

#[test]
fn only_the_rulebooks_and_the_agent_documents_ship_with_the_code() {
  let named = [
    "prompts/rules/answer-shape.md",
    "prompts/agents/chat/manual.md",
    "prompts/harness.md",
    "prompts/persona/base.md",
    "prompts/rules/notes.txt",
    "prompts/base/dsh.md",
  ]
  .map(str::to_owned);

  assert_eq!(
    tree_documents(&named),
    ["prompts/rules/answer-shape.md", "prompts/agents/chat/manual.md", "prompts/harness.md"]
  );
}

#[test]
fn a_chat_session_is_given_its_system_documents_and_carries_its_turn_documents() {
  assert!(is_chat_prompt_document("prompts/rules/answer-shape.md"));
  assert!(is_chat_prompt_document("prompts/agents/chat/manual.md"));
  // The platform's harness opening opens the chat session's system prompt.
  assert!(is_chat_prompt_document("prompts/harness.md"));
  assert!(is_chat_turn_document("prompts/agents/chat/owner.md"));
  assert!(!is_chat_prompt_document("prompts/agents/chat/owner.md"));
  assert!(!is_chat_prompt_document("prompts/persona/base.md"));
}
