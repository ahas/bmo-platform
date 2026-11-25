//! A place's file as it goes to the node (docs/work-nodes.md, 「The link」): this machine's roots in
//! every string of it read as the node's, the program's environment the node's own, and the files
//! under this machine's roots it names gathered to go along.

use serde_json::Value;

/// Where a root may begin inside a string: its start, after a separator a command line or a list of paths
/// puts before one, or after the single quote a shell command puts around a path, as a key helper's does.
fn begins_here(text: &str, at: usize) -> bool {
  at == 0 || matches!(text.as_bytes()[at - 1], b'=' | b':' | b' ' | b'\'')
}

/// Where a root may end inside a string: its end, a path going on under it, a separator, or a closing quote.
fn ends_here(text: &str, at: usize) -> bool {
  at == text.len() || matches!(text.as_bytes()[at], b'/' | b':' | b' ' | b'"' | b'\'')
}

/// Every root of this machine standing in a string replaced by the node's, the first root that holds a
/// place in it taking it.
pub fn rewrite_text(text: &str, roots: &[(String, String)]) -> String {
  let mut out = String::with_capacity(text.len());
  let mut at = 0;

  'scan: while at < text.len() {
    if begins_here(text, at) {
      for (from, to) in roots {
        if text[at..].starts_with(from.as_str()) && ends_here(text, at + from.len()) {
          out.push_str(to);
          at += from.len();
          continue 'scan;
        }
      }
    }

    let width = text[at..].chars().next().map_or(1, char::len_utf8);

    out.push_str(&text[at..at + width]);
    at += width;
  }

  out
}

/// The paths a string names under one root, by the same boundaries the rewrite reads.
fn named_under(text: &str, root: &str, found: &mut Vec<String>) {
  let mut at = 0;

  while let Some(offset) = text[at..].find(root) {
    let start = at + offset;
    let after = start + root.len();

    if begins_here(text, start) && after < text.len() && text.as_bytes()[after] == b'/' {
      let end = text[after..].find([':', ' ', '"', '=', '\'']).map_or(text.len(), |stop| after + stop);

      found.push(text[start..end].to_owned());
    }

    at = after;
  }
}

/// How a program reads on the node: only the variables of its environment kept, its search path the
/// node's own, the node's own variables laid over it, and a program word the node keeps a program of its own for
/// run as that one. A variable this machine's environment holds that is not kept never leaves it, which is how a
/// token of the bot's stays here.
#[derive(Clone, Copy, Debug, Default)]
pub struct NodeEnv<'a> {
  /// The names kept; one ending in `*` keeps every name it begins.
  pub keep: &'a [String],
  pub path: Option<&'a str>,
  pub add: &'a [(String, String)],
  /// Each program word with the path of the node's own program run in its place, as a command's first word.
  pub programs: &'a [(String, String)],
}

fn kept(name: &str, keep: &[String]) -> bool {
  keep
    .iter()
    .any(|one| one.strip_suffix('*').map_or(one == name, |prefix| name.starts_with(prefix)))
}

/// A JSON document with every string rewritten, each `env` object read as the node's, and the paths it
/// names under any of `gather` collected.
pub fn rewrite_json(
  value: &mut Value,
  roots: &[(String, String)],
  env: &NodeEnv,
  gather: &[String],
  named: &mut Vec<String>,
) {
  match value {
    Value::String(text) => {
      for root in gather {
        named_under(text, root, named);
      }

      *text = rewrite_text(text, roots);
    }
    Value::Array(items) => {
      for item in items {
        rewrite_json(item, roots, env, gather, named);
      }
    }
    Value::Object(fields) => {
      for (key, field) in fields.iter_mut() {
        if key == "env"
          && let Value::Object(variables) = field
        {
          variables.retain(|name, _| kept(name, env.keep));

          for variable in variables.values_mut() {
            rewrite_json(variable, roots, env, gather, named);
          }

          if let Some(path) = env.path {
            variables.insert("PATH".into(), Value::String(path.to_owned()));
          }

          for (name, value) in env.add {
            variables.insert(name.clone(), Value::String(value.clone()));
          }

          continue;
        }

        rewrite_json(field, roots, env, gather, named);

        // A command's program the node keeps one of its own for runs as that one.
        if key == "args"
          && let Some(Value::String(program)) = field.as_array_mut().and_then(|args| args.first_mut())
          && let Some((_, own)) = env.programs.iter().find(|(word, _)| word == program)
        {
          *program = own.clone();
        }
      }
    }
    _ => {}
  }
}
