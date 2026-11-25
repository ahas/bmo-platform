//! The owner's own Firefox, started on his own profile for one call and taken down with everything it started
//! when the call ends (docs/tools.md, 「How it pays」).
//!
//! DeepSeek knows the owner's Firefox as his device and keeps his sign-in in its profile, so the call pays in that
//! profile itself, never in a copy. Firefox is started where the owner never sees it: on a virtual screen of its
//! own that `xvfb-run` raises and takes down, told away from his Wayland session and handed a session bus address
//! that leads nowhere, so no window, notice or portal of it reaches his desktop, and with `--no-remote`, so it
//! neither hands itself to a Firefox he opens nor takes one over. A snap's launcher needs a user scope of the
//! snap's own to start in, which it would otherwise ask the session bus for; the scope is made first, through the
//! user's service manager, and the bus is cut only inside it. The browser is driven over WebDriver BiDi on a
//! loopback port it chooses itself, its pages' input dispatched inside the browser and never through the screen,
//! and the mark an automated browser shows its pages is hidden from them, since DeepSeek's bot check meets a
//! browser showing it with a human check. What is typed goes in as key presses and is never written anywhere by
//! this module, and the browser is told for its one start to keep no form entry, login or card.
//!
//! Firefox's remote agent would otherwise lay its recommended preferences for automation over the default branch
//! while it runs. The owner's own setting that equals one of them is then left out of what Firefox writes as it
//! ends, and they turn telemetry upload off and point Sync and telemetry at servers that do not exist, which resets
//! his telemetry identity and queues a request to delete his data. So the call starts Firefox with them switched
//! off, as an ordinary start of his own browser, and the profile's settings are given back as they stood.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::AsyncBufReadExt;
use tokio::task::JoinHandle;

use crate::bidi::{Connection, Unanswered};
use crate::profile::{Held, Unheld, default_profile};

/// The owner's Firefox and how it is started.
#[derive(Debug, Clone)]
pub struct Place {
  /// Firefox's executable, by its absolute path.
  pub firefox: String,
  /// The folder Firefox keeps its profiles and `profiles.ini` in; the call's private folder is made in it, where
  /// a confined Firefox reads what it is handed.
  pub profiles: PathBuf,
  /// The user scope a snap's launcher expects to start in, without its unique suffix; empty for none.
  pub scope: String,
  /// The virtual screen Firefox is started on.
  pub screen: String,
  /// How long Firefox has to announce its endpoint, and to answer each command.
  pub start: Duration,
}

/// Why no browser is there to drive.
#[derive(Debug)]
pub enum Unopened {
  /// Another call holds the profile.
  Busy,
  /// The owner's Firefox has the profile open.
  Open,
  /// The browser could not be started or reached, with why; the words carry no value typed into a page.
  Start(String),
}

/// A browser stopped answering a step; the step is named by the caller, never by the browser's own words.
#[derive(Debug)]
pub struct Lost;

impl From<Unanswered> for Lost {
  fn from(_: Unanswered) -> Self {
    Self
  }
}

/// An element of a page, in the frame it stands in.
#[derive(Debug, Clone)]
pub struct Element {
  frame: String,
  shared: String,
}

/// A frame of the page the call drives, by its browsing context.
pub type Frame = String;

/// What a find looks for in a frame: a field to type into, or a control to press, by the words it shows.
pub enum Wanted<'a> {
  /// An editable field, by its autocomplete purposes or the words of its placeholder, label, name or id;
  /// `password` takes the frame's password field first, and `alone` the frame's only editable field where no
  /// other matches.
  Field {
    words: &'a [String],
    purposes: &'a [&'a str],
    password: bool,
    alone: bool,
  },
  /// A visible control that is not disabled, whose own text, else the name it is given for a reader, as a mark
  /// standing for a word is, is one of the words, the first word first.
  Control { words: &'a [String] },
}

impl Wanted<'_> {
  fn spec(&self) -> Value {
    match self {
      Self::Field {
        words,
        purposes,
        password,
        alone,
      } => {
        json!({ "kind": "field", "words": words, "purposes": purposes, "password": password, "alone": alone })
      }
      Self::Control { words } => json!({ "kind": "control", "words": words, "purposes": [] }),
    }
  }
}

/// The find run inside a frame's sandbox: the element `spec` asks for, or null.
const FIND: &str = r#"(spec) => {
  const lower = (text) => (text || "").toLowerCase().trim();
  const words = spec.words.map(lower);
  const shown = (element) => {
    const box = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    return box.width > 0 && box.height > 0 && style.visibility !== "hidden" && style.display !== "none";
  };
  if (spec.kind === "field") {
    const skipped = ["hidden", "radio", "checkbox", "submit", "button", "file", "image", "reset"];
    const fields = [...document.querySelectorAll("input, textarea")].filter(
      (field) => shown(field) && !field.disabled && !field.readOnly && !skipped.includes(lower(field.type)),
    );
    if (spec.password) {
      const password = fields.find((field) => lower(field.type) === "password");
      if (password) return password;
    }
    const said = (field) =>
      [field.placeholder, field.getAttribute("aria-label"), field.name, field.id, ...[...(field.labels || [])].map((label) => label.innerText)]
        .map(lower)
        .join(" | ");
    const found = fields.find(
      (field) => spec.purposes.includes(lower(field.autocomplete)) || words.some((word) => said(field).includes(word)),
    );
    if (found) return found;
    return spec.alone && fields.length === 1 ? fields[0] : null;
  }
  const disabled = (element) => Boolean(element.closest('[disabled], [aria-disabled="true"], [class*="disabled"]'));
  const own = (element) => lower(element.innerText || element.value || element.getAttribute("aria-label"));
  const elements = [...document.querySelectorAll("body *")].filter(shown);
  for (const word of words) {
    const found = elements.find(
      (element) => own(element) === word && ![...element.children].some((child) => own(child) === word) && !disabled(element),
    );
    if (found) return found;
  }
  return null;
}"#;

/// Run in every page before its own scripts: the page's `navigator.webdriver` answers false, as a person's browser
/// answers it.
const UNMARKED: &str = "() => { Object.defineProperty(Navigator.prototype, 'webdriver', { get: () => false, configurable: true }); }";

/// The name of the tool's own sandbox in each frame: its scripts see the frame's document, never the page's own
/// scripts, and the page never sees them.
const SANDBOX: &str = "top-up";

/// The preferences the call's Firefox starts with, as lines of a Firefox user file laid over the owner's own for
/// that start alone: the remote agent's recommended preferences off; no form entry, login or card kept from what
/// the call types; and its window, unfocused on a screen nobody looks at, behaving as focused, as a payment
/// processor's fields expect of a person's window.
const CALL_PREFERENCES: &str = r#"user_pref("remote.prefs.recommended", false);
user_pref("browser.formfill.enable", false);
user_pref("signon.rememberSignons", false);
user_pref("extensions.formautofill.creditCards.enabled", false);
user_pref("extensions.formautofill.addresses.enabled", false);
user_pref("focusmanager.testmode", true);"#;

/// The words Firefox announces its WebDriver BiDi endpoint with on its error stream, the endpoint after them.
const ANNOUNCED: &str = "WebDriver BiDi listening on ";

/// The environment Firefox starts in, and nothing of the caller's beyond its `PATH` and `HOME`: English pages, its
/// runtime folder and a session bus address in the call's private `folder`, the bus address leading nowhere, the
/// X11 display `xvfb-run` names rather than the owner's Wayland session, and no crash reporter.
pub fn firefox_environment(folder: &Path) -> Vec<(String, String)> {
  let caller = |name: &str| std::env::var(name).unwrap_or_default();

  vec![
    ("PATH".to_owned(), caller("PATH")),
    ("HOME".to_owned(), caller("HOME")),
    ("LANG".to_owned(), "en_US.UTF-8".to_owned()),
    ("XDG_RUNTIME_DIR".to_owned(), folder.join("run").display().to_string()),
    (
      "DBUS_SESSION_BUS_ADDRESS".to_owned(),
      format!("unix:path={}", folder.join("no-bus").display()),
    ),
    ("DISABLE_WAYLAND".to_owned(), "1".to_owned()),
    ("GDK_BACKEND".to_owned(), "x11".to_owned()),
    ("MOZ_CRASHREPORTER_DISABLE".to_owned(), "1".to_owned()),
  ]
}

/// The command line Firefox is started with on `profile`: in a user scope named `{scope}-{unique}` where the place
/// names a scope, with the environment `firefox_environment` gives and no other, on a virtual screen whose
/// authority file stands in the call's `folder`, with no remote of its own and a loopback WebDriver BiDi endpoint
/// on a port it chooses itself, free when it binds it, which it announces.
pub fn launch_line(place: &Place, folder: &Path, profile: &Path, unique: &str) -> Vec<String> {
  let mut line = Vec::new();

  if !place.scope.is_empty() {
    line.extend(["systemd-run", "--user", "--scope", "--quiet", "--collect"].map(str::to_owned));
    line.push(format!("--unit={}-{unique}", place.scope));
    line.push("--".to_owned());
  }

  line.extend(["env", "-i"].map(str::to_owned));
  line.extend(firefox_environment(folder).into_iter().map(|(name, value)| format!("{name}={value}")));
  line.extend(["xvfb-run", "--auto-servernum", "-f"].map(str::to_owned));
  line.push(folder.join("Xauthority").display().to_string());
  line.push(format!("--server-args=-screen 0 {} -nolisten tcp", place.screen));
  line.push(place.firefox.clone());
  line.extend(["--no-remote", "--profile"].map(str::to_owned));
  line.push(profile.display().to_string());
  line.extend(["--remote-debugging-port", "0", "about:blank"].map(str::to_owned));
  line
}

/// How long the members of a killed group are given to be gone: a killed process ends only once the kernel has
/// freed what it held, which for a browser on a loaded machine takes a moment.
const KILLED_GONE: Duration = Duration::from_secs(5);

/// Ends a process group: asked to end first, so `xvfb-run` clears its screen's files, then killed with whatever
/// is left, the group's leader waited on and every other member waited for, within a bound, until none still
/// runs. The leader can end first, the browser's own processes and the screen still ending behind it, and the
/// owner's session is given back only once nothing of the call's browser runs on his profile.
fn end_group(child: &mut Child) {
  let Ok(group) = libc::pid_t::try_from(child.id()) else {
    return;
  };

  // SAFETY: `kill` reads only its two integer arguments; a negative pid names the group the child leads.
  unsafe { libc::kill(-group, libc::SIGTERM) };

  for _ in 0..30 {
    if matches!(child.try_wait(), Ok(Some(_))) {
      break;
    }

    std::thread::sleep(Duration::from_millis(100));
  }

  // SAFETY: as above.
  unsafe { libc::kill(-group, libc::SIGKILL) };
  let _ = child.wait();

  let gone_by = std::time::Instant::now() + KILLED_GONE;

  while group_runs(group) && std::time::Instant::now() < gone_by {
    std::thread::sleep(Duration::from_millis(20));
  }
}

/// Whether any member of the process group `group` still runs. A zombie runs nothing, and reaping it is its new
/// parent's business, so it counts as gone.
fn group_runs(group: libc::pid_t) -> bool {
  procfs::process::all_processes().is_ok_and(|all| {
    all.flatten().any(|process| {
      process.stat().is_ok_and(|stat| stat.pgrp == group && !matches!(stat.state, 'Z' | 'X'))
    })
  })
}

/// One browser on the owner's profile, its virtual screen and the one tab a call drives. The profile is held, and
/// its session given back, by `_held`, which is let go only once the browser has ended.
pub struct Browser {
  screen: Child,
  /// Reads the browser's output away once its endpoint is announced.
  output: JoinHandle<()>,
  bidi: Connection,
  tab: String,
  _held: Held,
}

impl Browser {
  /// Firefox started on the owner's profile, held for the call, its endpoint reached and a tab of the call's own
  /// opened.
  pub async fn open(place: &Place) -> Result<Self, Unopened> {
    let profile = default_profile(&place.profiles)
      .map_err(|why| Unopened::Start(format!("the owner's Firefox profile was not found: {why}")))?;
    let held = Held::take(&profile, &place.profiles, CALL_PREFERENCES).map_err(|unheld| match unheld {
      Unheld::Busy => Unopened::Busy,
      Unheld::Open => Unopened::Open,
      Unheld::Fault(why) => Unopened::Start(why),
    })?;
    let run = held.folder().join("run");

    std::fs::create_dir(&run)
      .and_then(|()| std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o700)))
      .map_err(|why| Unopened::Start(format!("its runtime folder: {why}")))?;

    let line = launch_line(place, held.folder(), held.profile(), &uuid::Uuid::new_v4().to_string());
    let mut screen = Command::new(&line[0])
      .args(&line[1..])
      .stdin(Stdio::null())
      // `xvfb-run` hands the browser's error stream on as its own output, which carries the announced endpoint.
      .stdout(Stdio::piped())
      .stderr(Stdio::null())
      .process_group(0)
      .spawn()
      .map_err(|why| Unopened::Start(format!("{} could not run: {why}", line[0])))?;

    let output = screen.stdout.take();
    let (endpoint, output) = match Self::announced(output, &mut screen, place.start).await {
      Ok(announced) => announced,
      Err(why) => {
        end_group(&mut screen);
        return Err(Unopened::Start(why));
      }
    };

    let opened = match Connection::open(&endpoint, place.start).await {
      Ok(bidi) => Self::tab(&bidi, place.start).await.map(|tab| (bidi, tab)),
      Err(why) => Err(why),
    };

    match opened {
      Ok((bidi, tab)) => Ok(Self {
        screen,
        output,
        bidi,
        tab,
        _held: held,
      }),
      Err(why) => {
        output.abort();
        end_group(&mut screen);
        Err(Unopened::Start(why))
      }
    }
  }

  /// The call's own tab, its pages unmarked, once its window has a size: a window still being mapped has none,
  /// and a press there would land nowhere.
  async fn tab(bidi: &Connection, start: Duration) -> Result<String, String> {
    bidi
      .call("script.addPreloadScript", json!({ "functionDeclaration": UNMARKED }))
      .await
      .map_err(|_| "its pages could not be unmarked".to_owned())?;

    let made = bidi
      .call("browsingContext.create", json!({ "type": "tab" }))
      .await
      .map_err(|_| "no tab could be opened".to_owned())?;
    let tab = made["context"].as_str().ok_or("no tab could be opened")?.to_owned();
    let deadline = tokio::time::Instant::now() + start;

    loop {
      let sized = call_in(bidi, &tab, "() => innerWidth > 0 && innerHeight > 0", &[]).await;

      if sized.is_ok_and(|sized| sized["value"] == true) {
        return Ok(tab);
      }

      if tokio::time::Instant::now() >= deadline {
        return Err(format!("its window had no size within {} seconds", start.as_secs()));
      }

      tokio::time::sleep(Duration::from_millis(100)).await;
    }
  }

  /// The endpoint the browser announces on its output once it listens, within the start wait, and the task that
  /// reads the rest of its output away so the browser never stands on a full pipe. Only the browser's own word
  /// names its endpoint, on the port it chose.
  async fn announced(
    output: Option<ChildStdout>,
    screen: &mut Child,
    start: Duration,
  ) -> Result<(String, JoinHandle<()>), String> {
    let output = output.ok_or_else(|| "its output could not be read".to_owned())?;
    let output = tokio::process::ChildStdout::from_std(output)
      .map_err(|why| format!("its output could not be read: {why}"))?;
    let mut lines = tokio::io::BufReader::new(output).split(b'\n');

    let announced = tokio::time::timeout(start, async {
      while let Ok(Some(line)) = lines.next_segment().await {
        if let Some(endpoint) = String::from_utf8_lossy(&line).trim().strip_prefix(ANNOUNCED) {
          return Some(endpoint.to_owned());
        }
      }

      None
    })
    .await;

    match announced {
      Ok(Some(endpoint)) => {
        let rest = tokio::spawn(async move { while let Ok(Some(_)) = lines.next_segment().await {} });

        Ok((endpoint, rest))
      }
      Ok(None) => Err(match screen.try_wait() {
        Ok(Some(status)) => format!("it ended before it listened, {status}"),
        _ => "it ended before it listened".to_owned(),
      }),
      Err(_) => Err(format!("it did not listen within {} seconds", start.as_secs())),
    }
  }

  /// Sends the tab to an address; what it shows is waited for by the caller.
  pub async fn go(&mut self, address: &str) -> Result<(), Lost> {
    self
      .bidi
      .call(
        "browsingContext.navigate",
        json!({ "context": self.tab, "url": address, "wait": "none" }),
      )
      .await?;
    Ok(())
  }

  /// The tab's frames, the main frame first, each with its address.
  pub async fn frames(&mut self) -> Result<Vec<(Frame, String)>, Lost> {
    fn walk(context: &Value, into: &mut Vec<(Frame, String)>) {
      if let (Some(frame), Some(address)) = (context["context"].as_str(), context["url"].as_str()) {
        into.push((frame.to_owned(), address.to_owned()));
      }

      for child in context["children"].as_array().into_iter().flatten() {
        walk(child, into);
      }
    }

    let tree = self.bidi.call("browsingContext.getTree", json!({ "root": self.tab })).await?;
    let mut frames = Vec::new();

    for context in tree["contexts"].as_array().into_iter().flatten() {
      walk(context, &mut frames);
    }

    Ok(frames)
  }

  /// The text a frame shows, lowered.
  pub async fn text(&mut self, frame: &Frame) -> Result<String, Lost> {
    let shown =
      call_in(&self.bidi, frame, "() => document.body ? document.body.innerText : ''", &[]).await?;

    Ok(shown["value"].as_str().map(str::to_lowercase).unwrap_or_default())
  }

  /// The element a frame holds for what is wanted, or none.
  pub async fn find(&mut self, frame: &Frame, wanted: &Wanted<'_>) -> Result<Option<Element>, Lost> {
    let found = call_in(&self.bidi, frame, &format!("() => ({FIND})({})", wanted.spec()), &[]).await?;

    Ok(found["sharedId"].as_str().map(|shared| Element {
      frame: frame.clone(),
      shared: shared.to_owned(),
    }))
  }

  /// The first frame holding what is wanted, and the element there.
  pub async fn find_anywhere(&mut self, wanted: &Wanted<'_>) -> Result<Option<Element>, Lost> {
    for (frame, _) in self.frames().await? {
      if let Ok(Some(found)) = self.find(&frame, wanted).await {
        return Ok(Some(found));
      }
    }

    Ok(None)
  }

  /// A mail's HTML read by the browser's own HTML reader, in the main frame's sandbox: its visible words, styles
  /// and scripts left out, and each link's address and words. A document so read runs nothing.
  pub async fn read_mail(&mut self, html: &str) -> Result<(String, Vec<(String, String)>), Lost> {
    const READ: &str = r#"(html) => {
      const read = new DOMParser().parseFromString(html, "text/html");
      read.querySelectorAll("style, script, head").forEach((element) => element.remove());
      return JSON.stringify({
        text: read.body ? read.body.textContent : "",
        links: [...read.querySelectorAll("a[href]")].map((link) => [link.getAttribute("href"), link.textContent]),
      });
    }"#;
    let main = self.tab.clone();
    let read = call_in(&self.bidi, &main, &format!("() => ({READ})({})", Value::from(html)), &[]).await?;
    let read: Value = serde_json::from_str(read["value"].as_str().unwrap_or_default()).map_err(|_| Lost)?;
    let text = read["text"].as_str().unwrap_or_default().to_owned();
    let links = read["links"]
      .as_array()
      .into_iter()
      .flatten()
      .filter_map(|link| {
        Some((link[0].as_str()?.to_owned(), link[1].as_str().unwrap_or_default().to_owned()))
      })
      .collect();

    Ok((text, links))
  }

  /// An address opened in a background tab of the same browser and profile, as a person opens a mailed link: the
  /// tab is closed once its page has loaded or the wait is out.
  pub async fn visit(&self, address: &str, wait: Duration) -> Result<(), Lost> {
    let made = self
      .bidi
      .call("browsingContext.create", json!({ "type": "tab", "background": true }))
      .await?;
    let tab = made["context"].as_str().ok_or(Lost)?.to_owned();

    self
      .bidi
      .call("browsingContext.navigate", json!({ "context": tab, "url": address, "wait": "none" }))
      .await?;

    let deadline = tokio::time::Instant::now() + wait;

    while tokio::time::Instant::now() < deadline {
      let loaded = call_in(&self.bidi, &tab, "() => document.readyState === 'complete'", &[]).await;

      if loaded.is_ok_and(|loaded| loaded["value"] == true) {
        break;
      }

      tokio::time::sleep(Duration::from_millis(200)).await;
    }

    self.bidi.call("browsingContext.close", json!({ "context": tab })).await?;
    Ok(())
  }

  /// A press of an element where a person's pointer would press it: its middle, once it is scrolled into view.
  pub async fn press(&self, element: &Element) -> Result<(), Lost> {
    let reference = json!({ "sharedId": element.shared });

    call_in(
      &self.bidi,
      &element.frame,
      "(element) => element.scrollIntoView({ block: 'center', inline: 'center' })",
      std::slice::from_ref(&reference),
    )
    .await?;

    let pointer = json!([{
      "type": "pointer",
      "id": "pointer",
      "parameters": { "pointerType": "mouse" },
      "actions": [
        { "type": "pointerMove", "x": 0, "y": 0, "origin": { "type": "element", "element": reference } },
        { "type": "pointerDown", "button": 0 },
        { "type": "pointerUp", "button": 0 },
      ],
    }]);

    self.perform(&element.frame, pointer).await
  }

  /// Text typed into a field as a person's keyboard would, over whatever the field held: the field focused and
  /// its text selected, then each character pressed and let go in the field's own frame.
  pub async fn type_into(&self, field: &Element, text: &str) -> Result<(), Lost> {
    call_in(
      &self.bidi,
      &field.frame,
      "(field) => { field.focus(); if (field.select) field.select(); }",
      &[json!({ "sharedId": field.shared })],
    )
    .await?;

    let keys: Vec<Value> = text
      .chars()
      .flat_map(|character| {
        let character = character.to_string();

        [
          json!({ "type": "keyDown", "value": character }),
          json!({ "type": "keyUp", "value": character }),
        ]
      })
      .collect();

    self
      .perform(&field.frame, json!([{ "type": "key", "id": "keyboard", "actions": keys }]))
      .await
  }

  /// Input actions dispatched in a frame, and every key and button let go after them.
  async fn perform(&self, frame: &Frame, actions: Value) -> Result<(), Lost> {
    let performed = self
      .bidi
      .call("input.performActions", json!({ "context": frame, "actions": actions }))
      .await;

    self.bidi.call("input.releaseActions", json!({ "context": frame })).await?;
    performed?;
    Ok(())
  }

  /// The browser closed through its endpoint, as a person's window is closed, so what the profile holds is written
  /// out, and its screen left to end with it; whatever has not ended within ten seconds is ended by the drop,
  /// which then gives the owner's session back.
  pub async fn close(&mut self) {
    let _ = self.bidi.call("browsingContext.close", json!({ "context": self.tab })).await;
    let _ = self.bidi.call("browser.close", json!({})).await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

    while tokio::time::Instant::now() < deadline && matches!(self.screen.try_wait(), Ok(None)) {
      tokio::time::sleep(Duration::from_millis(100)).await;
    }
  }
}

/// A function called in a frame's sandbox with the arguments given, each a value as WebDriver BiDi writes one, and
/// its result as WebDriver BiDi writes a value: an element by its `sharedId`, anything else by its `value`. A
/// function that throws is a step the page did not answer.
async fn call_in(
  bidi: &Connection,
  frame: &str,
  function: &str,
  arguments: &[Value],
) -> Result<Value, Lost> {
  let mut called = bidi
    .call(
      "script.callFunction",
      json!({
        "functionDeclaration": function,
        "awaitPromise": true,
        "target": { "context": frame, "sandbox": SANDBOX },
        "arguments": arguments,
        "resultOwnership": "none",
        "serializationOptions": { "maxDomDepth": 0 },
      }),
    )
    .await?;

  if called["type"] == "success" { Ok(called["result"].take()) } else { Err(Lost) }
}

impl Drop for Browser {
  fn drop(&mut self) {
    end_group(&mut self.screen);
    self.output.abort();
    // `_held` drops after this, once the browser has ended, and gives the owner's session back.
  }
}
