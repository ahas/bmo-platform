//! A runner line's configuration: the servers it names on the tool host.

use agency_server::lines::runner::*;

#[test]
fn a_line_names_its_namespaces_alone_on_the_host_behind_the_run_s_token() {
  let server = |namespace: &str| {
    format!(
      "  \"{namespace}\": {{\n    \"type\": \"http\",\n    \"url\": \"http://127.0.0.1:3290/mcp/{namespace}\",\n    \"headers\": {{\n      \"Authorization\": \"Bearer ${{BMO_MCP_TOKEN}}\"\n    }}\n  }}"
    )
  };

  // In the order the deployment's writer writes them, which the line's prefix is read off. The toolbook is
  // the host's own face `bmo use` asks, never a server named in a line's configuration.
  assert_eq!(
    servers_on(3290, &["web", "worker"]).pretty(),
    format!("{{\n{},\n{}\n}}", server("web"), server("worker"))
  );
  assert!(!servers_on(3290, &["web", "worker"]).pretty().contains("toolbook"));
}
