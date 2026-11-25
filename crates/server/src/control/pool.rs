//! The standing branch checkouts' list as the routes write it (docs/role-slots.md, 「The list」).

/// What a place's line was written with now noted under its key: the provider, model, effort and upstream,
/// the place keeping the latest moment it was written, which a later write never moves back.
pub async fn place_written(
  db: &sea_orm::DatabaseConnection,
  place: &str,
  provider: &str,
  model: &str,
  effort: Option<&str>,
  upstream: Option<&str>,
) -> Result<(), crate::Refusal> {
  store::run(
    db,
    r#"INSERT INTO "place_writes" ("place", "provider", "model", "effort", "upstream", "written_at")
       VALUES ($1, $2, $3, $4, $5, $6)
       ON CONFLICT ("place") DO UPDATE SET "provider" = excluded."provider", "model" = excluded."model",
         "effort" = excluded."effort", "upstream" = excluded."upstream",
         "written_at" = GREATEST("place_writes"."written_at", excluded."written_at")"#,
    vec![
      place.into(),
      provider.into(),
      model.into(),
      effort.unwrap_or_default().into(),
      upstream.into(),
      crate::now_ms().into(),
    ],
  )
  .await?;

  Ok(())
}
