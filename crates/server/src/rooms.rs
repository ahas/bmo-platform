//! A room forgotten (docs/api.md, 「Sessions」, forgetting a room): every row of agency's own tables naming the
//! room goes, as the consumer that owns the room removes its registration. The columns naming a room are read
//! off the database's own schema, within the tables the store makes and no other service's; the rows of a
//! table that stay are declared by the code that owns the table, beside that code, through `room_stays!`.
//! Nothing here names a table.

use std::collections::BTreeMap;

use sea_orm::{ConnectionTrait, TransactionTrait};
use serde::Serialize;
use store::migration::OWN_TABLES;
use store::{all, one, run};

use crate::Refusal;

/// Rows of a swept table that stay as its room is forgotten: those matching `stays`, a condition on the
/// table's own columns, for the reason given.
pub struct Stays {
  pub table: &'static str,
  pub stays: &'static str,
  pub why: &'static str,
}

inventory::collect!(Stays);

/// Declares which rows of a table stay as a room is forgotten, beside the code that owns the table.
#[macro_export]
macro_rules! room_stays {
  ($stays:expr) => {
    ::inventory::submit! { $stays }
  };
}

/// Every declaration linked into this binary.
pub fn stays() -> impl Iterator<Item = &'static Stays> {
  inventory::iter::<Stays>.into_iter()
}

/// Whether a column's name says it holds a room's number.
pub fn names_a_room(column: &str) -> bool {
  column == "chat_id" || column.starts_with("chat_") || column.contains("_chat")
}

/// What forgetting a room came to, or would: the rows of each table that go, and the rows that stay with
/// why, table by table.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Forgotten {
  pub gone: BTreeMap<String, u64>,
  pub stayed: Vec<Stayed>,
}

/// Rows of a table that stay, with why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Stayed {
  pub table: String,
  pub rows: u64,
  pub why: String,
}

// Each of the store's own tables and the columns of it naming a room.
async fn room_columns(db: &impl ConnectionTrait) -> Result<Vec<(String, Vec<String>)>, Refusal> {
  let rows = all(
    db,
    "SELECT c.table_name::text AS t, c.column_name::text AS c FROM information_schema.columns c
     JOIN information_schema.tables b ON b.table_schema = c.table_schema AND b.table_name = c.table_name
     WHERE c.table_schema = current_schema() AND b.table_type = 'BASE TABLE'
     ORDER BY c.table_name, c.ordinal_position",
    vec![],
  )
  .await?;
  let mut columns: BTreeMap<String, Vec<String>> = BTreeMap::new();

  for row in rows {
    let (table, column): (String, String) = (row.try_get("", "t")?, row.try_get("", "c")?);

    if OWN_TABLES.contains(&table.as_str()) && names_a_room(&column) {
      columns.entry(table).or_default().push(column);
    }
  }

  Ok(columns.into_iter().collect())
}

// A count of the rows a condition picks.
async fn count(db: &impl ConnectionTrait, table: &str, condition: &str, room: i64) -> Result<u64, Refusal> {
  let row = one(
    db,
    &format!(r#"SELECT COUNT(*) AS "rows" FROM "{table}" WHERE {condition}"#),
    vec![room.into()],
  )
  .await?;

  Ok(row.map_or(Ok(0), |row| row.try_get::<i64>("", "rows"))?.max(0) as u64)
}

/// The room forgotten, in one write: every row of the store's own tables naming it goes but those a
/// declaration keeps. A dry run writes nothing and answers what would go.
pub async fn forget<D>(db: &D, room: i64, dry_run: bool) -> Result<Forgotten, Refusal>
where
  D: ConnectionTrait + TransactionTrait,
{
  let transaction = db.begin().await?;
  let mut forgotten = Forgotten::default();

  for (table, columns) in room_columns(&transaction).await? {
    let names = columns
      .iter()
      .map(|column| format!(r#""{column}" = $1"#))
      .collect::<Vec<_>>()
      .join(" OR ");
    let staying: Vec<&Stays> = stays().filter(|stays| stays.table == table).collect();
    let mut goes = format!("({names})");

    for stays in &staying {
      let rows = count(&transaction, &table, &format!("({names}) AND ({})", stays.stays), room).await?;

      if rows > 0 {
        forgotten.stayed.push(Stayed {
          table: table.clone(),
          rows,
          why: stays.why.to_owned(),
        });
      }

      goes = format!("{goes} AND NOT ({})", stays.stays);
    }

    let rows = if dry_run {
      count(&transaction, &table, &goes, room).await?
    } else {
      run(&transaction, &format!(r#"DELETE FROM "{table}" WHERE {goes}"#), vec![room.into()]).await?
    };

    if rows > 0 {
      forgotten.gone.insert(table, rows);
    }
  }

  if dry_run {
    transaction.rollback().await?;
  } else {
    transaction.commit().await?;
  }

  Ok(forgotten)
}
