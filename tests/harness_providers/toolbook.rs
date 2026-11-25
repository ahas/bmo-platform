//! The toolbook's proxy calls read as the tool each reached.

use harness_providers::toolbook::*;

#[test]
fn a_proxy_call_reads_as_the_tool_it_reached_in_its_road_s_spelling() {
  assert_eq!(reached("mcp__toolbook__call", "web/web_fetch").as_deref(), Some("mcp__web__web_fetch"));
  assert_eq!(reached("toolbook/act", "web/web_fetch").as_deref(), Some("web/web_fetch"));
  assert_eq!(reached("mcp__web__web_fetch", "web/web_fetch"), None);
}
