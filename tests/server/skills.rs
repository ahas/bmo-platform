//! A person's skills at home (docs/person-nodes.md, 「Skills at home」): saved whole under a new version, refused for
//! a name out of the alphabet, a front matter that does not read or names another skill, a path climbing out, a file
//! that is not text and an installed copy's name; listed with their text, origin and holders; read; removed with their
//! attachments, an installed copy refused; and a copy received only where its hash is the one shared.

use std::collections::BTreeMap;

use agency_server::skills::{self, Fault, Text};

use crate::support::grounds::seeded;
use crate::support::scratch::Scratch;

const ADMINISTRATOR: &str = "1";

fn files(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
  pairs.iter().map(|(path, text)| ((*path).to_owned(), (*text).to_owned())).collect()
}

const TIDY: &str = "---\nname: tidy\ndescription: Tidies the notes.\ntitle:\n  ko: Tidy-ko\n  en: Tidy\nlanguage: en\nallowed-tools: Read\n---\nTidy them.\n";

#[test]
fn a_skill_s_front_matter_reads_its_name_description_and_texts() {
  let read = skills::front_matter(TIDY).unwrap();

  assert_eq!(read.name, "tidy");
  assert_eq!(read.description, "Tidies the notes.");
  assert!(matches!(read.title, Some(Text::Locales(ref locales)) if locales.len() == 2));
  assert!(skills::front_matter("No front matter.\n").is_err());
  assert!(skills::front_matter("---\nname: tidy\ndescription: ''\n---\n").is_err());
  assert!(
    skills::skill_name("tidy-notes-2") && !skills::skill_name("Tidy") && !skills::skill_name("tidy_notes")
  );
  assert!(!skills::skill_name(&"a".repeat(65)));
}

#[tokio::test]
async fn a_skill_is_saved_listed_read_and_removed_at_home() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded(scratch.db(), home.path()).await;

  scratch
    .run(
      &format!(
        r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '{ADMINISTRATOR}')"#
      ),
      vec![],
    )
    .await;

  let refused = |fault: Fault| match fault {
    Fault::Bad(why) => why,
    other => panic!("{other:?}"),
  };
  let save = |name: &'static str, files: BTreeMap<String, String>| {
    let ground = &ground;

    async move { skills::save(ground, ADMINISTRATOR, name, &files).await }
  };

  assert!(
    refused(save("Tidy", files(&[("SKILL.md", TIDY)])).await.unwrap_err()).contains("no skill's name")
  );
  assert!(
    refused(save("other", files(&[("SKILL.md", TIDY)])).await.unwrap_err()).contains("names \"tidy\"")
  );
  assert!(
    refused(save("tidy", files(&[("SKILL.md", TIDY), ("../out.md", "x")])).await.unwrap_err())
      .contains("outside")
  );
  assert!(
    refused(save("tidy", files(&[("SKILL.md", TIDY), (".version", "x")])).await.unwrap_err())
      .contains("mark")
  );
  assert!(
    refused(save("tidy", files(&[("SKILL.md", TIDY), ("a.bin", "a\0b")])).await.unwrap_err())
      .contains("text")
  );
  assert!(refused(save("tidy", files(&[("notes.md", "x")])).await.unwrap_err()).contains("SKILL.md"));

  let saved = save("tidy", files(&[("SKILL.md", TIDY), ("references/style.md", "Short.")]))
    .await
    .unwrap();

  assert_eq!(saved.origin, "own");
  assert_eq!(saved.title.as_deref(), Some("Tidy"));
  assert_eq!(saved.titles.get("ko").map(String::as_str), Some("Tidy-ko"));
  assert!(!saved.version.is_empty());

  // A save again names a new version.
  tokio::time::sleep(std::time::Duration::from_millis(2)).await;

  let again = save("tidy", files(&[("SKILL.md", TIDY)])).await.unwrap();

  assert_ne!(again.version, saved.version);

  let (held, read) = skills::files(&ground.roots.data, ADMINISTRATOR, "tidy").unwrap();

  assert_eq!(held.version, again.version);
  assert_eq!(read.keys().collect::<Vec<_>>(), ["SKILL.md"], "saved whole: the reference file is gone");

  // An installed copy cannot be saved over or removed here; a copy whose hash is not the one shared is refused.
  let copy = files(&[("SKILL.md", &TIDY.replace("name: tidy", "name: shared"))]);
  let hash = node_link::keeping::source_hash(&copy);

  assert!(
    skills::receive(&ground.roots.data, ADMINISTRATOR, "shared", &copy, "0000", ("5-shared", "v1"))
      .is_err()
  );
  skills::receive(&ground.roots.data, ADMINISTRATOR, "shared", &copy, &hash, ("5-shared", "v1")).unwrap();
  assert!(matches!(save("shared", copy.clone()).await, Err(Fault::NameTaken(_))));
  assert!(matches!(
    skills::remove(&ground, ADMINISTRATOR, "shared").await,
    Err(Fault::InstalledCopy(_))
  ));
  assert!(matches!(
    skills::receive(&ground.roots.data, ADMINISTRATOR, "tidy", &copy, &hash, ("5-tidy", "v1")),
    Err(Fault::NameTaken(_))
  ));

  sea_orm::ConnectionTrait::execute_unprepared(
    &ground.db,
    r#"INSERT INTO "agent_attachments" VALUES ('1', 'supervisor', 'bmo', 'skill', 'tidy', 1)"#,
  )
  .await
  .unwrap();

  let listed = skills::list(&ground, ADMINISTRATOR).await.unwrap();

  assert_eq!(
    listed
      .iter()
      .map(|skill| (skill.name.as_str(), skill.origin.as_str()))
      .collect::<Vec<_>>(),
    [("shared", "installed"), ("tidy", "own")]
  );
  assert_eq!(listed[0].app.as_deref(), Some("5-shared"));
  assert_eq!(listed[1].attached_to, ["bmo"]);

  skills::remove(&ground, ADMINISTRATOR, "tidy").await.unwrap();
  assert!(skills::one(&ground.roots.data, ADMINISTRATOR, "tidy").is_none());
  assert!(
    agency_server::attachments::holders_of(&ground.db, ADMINISTRATOR, "skill", "tidy")
      .await
      .unwrap()
      .is_empty()
  );
  assert!(matches!(skills::remove(&ground, ADMINISTRATOR, "tidy").await, Err(Fault::NotFound(_))));
}
