//! The tool host's toolbook proxy calls as the tool each reached: a booked tool is reached through
//! one of two proxy tools of the toolbook's own server, handed the booked tool's full name and its
//! arguments, so a call through it is read and shown as the inner call, in the spelling of the road that
//! made it.

use std::sync::LazyLock;

use regex::Regex;

static CLAUDE_MCP_NAME: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^mcp__([^_]+(?:_[^_]+)*)__(.+)$").expect("an MCP name pattern"));
static CODEX_MCP_NAME: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"^([A-Za-z0-9][A-Za-z0-9_.-]*)/([A-Za-z0-9_.-]+)$").expect("an MCP name pattern")
});

/// The toolbook's server name, which every line's configuration names beside its namespaces.
pub const SERVER: &str = "toolbook";

// Its two proxy tools.
const PROXIES: [&str; 2] = ["call", "act"];

/// The MCP server and tool a called name is, on either road's spelling.
pub fn mcp_call_of(name: &str) -> Option<(String, String)> {
  CLAUDE_MCP_NAME
    .captures(name)
    .or_else(|| CODEX_MCP_NAME.captures(name))
    .map(|found| (found[1].to_owned(), found[2].to_owned()))
}

/// The tool a call named `name` reached where it is a toolbook proxy call handed the booked tool `inner`
/// with arguments: the inner tool's name in the road's own spelling; none for any other call.
pub fn reached(name: &str, inner: &str) -> Option<String> {
  let (server, tool) = mcp_call_of(name)?;

  if server != SERVER || !PROXIES.contains(&tool.as_str()) {
    return None;
  }

  let found = CODEX_MCP_NAME.captures(inner)?;

  Some(if CLAUDE_MCP_NAME.is_match(name) {
    format!("mcp__{}__{}", &found[1], &found[2])
  } else {
    format!("{}/{}", &found[1], &found[2])
  })
}
