//! The migrations that make the store's tables, from the 1.0.0 baseline (docs/store.md, 「Its tables」). The
//! record of what ran is a table of the store's own, so the store may share a database with another service's
//! tables without either's record standing in the other's way.

mod m20270124_000001_baseline;

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
use sea_orm_migration::prelude::*;

/// The table the store's migrations keep the record of what ran in.
pub const MIGRATIONS_TABLE: &str = "agency_migrations";

/// The migration that makes every table; a seed given once is given as it runs (`crate::seed`).
pub const BASELINE: &str = m20270124_000001_baseline::NAME;

/// Every table the store's migrations make, beside the record of what ran: what reads every table of the
/// store reads these and no other service's beside them (docs/store.md, 「Its tables」).
pub const OWN_TABLES: [&str; 65] = [
  "supervisors",
  "roles",
  "role_seeded",
  "repositories",
  "repository_node_checkouts",
  "support_checkouts",
  "role_sessions",
  "role_slot_requests",
  "role_agents",
  "role_slot_reservations",
  "role_provider_prompts",
  "role_provider_prompt_seeded",
  "place_writes",
  "gate_kinds",
  "chat_documents",
  "agent_log",
  "agent_executions",
  "execution_attempts",
  "agent_base_prompts",
  "base_prompts",
  "base_prompt_seeded",
  "branch_agents",
  "session_resets",
  "claude_accounts",
  "usage_windows",
  "usage_balances",
  "model_catalogue",
  "fallback_sessions",
  "model_fallbacks",
  "model_uses",
  "registered_models",
  "registered_model_addresses",
  "card_demands",
  "node_workplaces",
  "person_nodes",
  "agent_attachments",
  "mcp_servers",
  "node_rotation",
  "node_assignments",
  "live_claude_openings",
  "live_claude_runs",
  "prefix_moves",
  "runs",
  "run_clocks",
  "clock_asks",
  "room_sessions",
  "room_session_turns",
  "supervisor_settings",
  "supervisor_sleep",
  "settings",
  "assignments",
  "jobs",
  "job_sessions",
  "job_words",
  "session_work",
  "session_loads",
  "turn_liveness",
  "stream_events",
  "relays",
  "notices",
  "gateway_call_keys",
  "gateway_requests",
  "personal_supply",
  "supply_reservations",
  "supply_settlements",
];

/// The record an install from before 1.0.0 holds once every migration of its last release ran. Its schema is
/// the baseline's.
pub const BEFORE: [&str; 108] = [
  "m20261016_000001_store",
  "m20261017_000001_clocks",
  "m20261019_000001_runs",
  "m20261020_000001_worker_context",
  "m20261021_000001_worker_words",
  "m20261022_000001_usage_windows",
  "m20261023_000001_work",
  "m20261024_000001_job_model",
  "m20261025_000001_base_prompt_seeds",
  "m20261025_000002_endpoint_accounts",
  "m20261026_000001_pool_session",
  "m20261027_000001_deepseek_provider",
  "m20261028_000001_mapping_thinking",
  "m20261029_000001_session_summaries_retired",
  "m20261029_000002_fold_ratios",
  "m20261030_000001_agent_owner",
  "m20261031_000001_recollectors_retired",
  "m20261101_000001_usage_balances",
  "m20261101_000002_run_rounds",
  "m20261102_000001_route_registration",
  "m20261103_000001_run_usage",
  "m20261104_000001_job_usage",
  "m20261105_000001_prefix_documents",
  "m20261106_000001_job_instruction",
  "m20261107_000001_run_tools",
  "m20261108_000001_gate_kinds",
  "m20261109_000001_job_roads",
  "m20261109_000002_worker_skill_names",
  "m20261109_000003_usage_balance_requests",
  "m20261110_000001_translator_runner",
  "m20261110_000002_pickers_retired",
  "m20261111_000001_moments_in_milliseconds",
  "m20261111_000002_picker_keys_retired",
  "m20261112_000001_role_files",
  "m20261113_000001_agent_log_prune",
  "m20261114_000001_chat_documents",
  "m20261114_000002_registration_last_answered",
  "m20261115_000001_agent_roles",
  "m20261116_000001_agent_log_run_id",
  "m20261117_000001_card_services",
  "m20261118_000001_pool_asks",
  "m20261119_000001_registration_wait_retired",
  "m20261120_000001_clock_asks_unbounded",
  "m20261121_000001_assignments_unbounded",
  "m20261122_000001_role_sessions",
  "m20261122_000002_delegation_titles",
  "m20261122_000003_job_close_lease",
  "m20261123_000001_turn_liveness",
  "m20261124_000001_renewal",
  "m20261124_000001_stream_events",
  "m20261125_000001_delegator_follows_supervisor",
  "m20261126_000001_role_names",
  "m20261127_000001_supervisor_settings",
  "m20261128_000001_supervisors_alike",
  "m20261129_000001_chosen_roads",
  "m20261130_000001_supervisor_card_rank",
  "m20261201_000001_relays",
  "m20261202_000001_supervisor_system_hash",
  "m20261203_000001_role_agents",
  "m20261204_000001_role_agent_settings",
  "m20261205_000001_role_skills",
  "m20261206_000001_agent_wanted",
  "m20261207_000001_arbiter_model",
  "m20261208_000001_arbiter_model_gone",
  "m20261209_000001_account_seats",
  "m20261210_000001_job_review",
  "m20261211_000001_person_nodes",
  "m20261212_000001_coworkers_gone",
  "m20261213_000001_agent_order_gone",
  "m20261214_000001_job_lines_by_agent",
  "m20261215_000001_job_seats_unbound",
  "m20261216_000001_job_machine_turn",
  "m20261217_000001_notices",
  "m20261218_000001_job_turn_words",
  "m20261219_000001_registration_languages",
  "m20261220_000001_job_notices_owed",
  "m20261221_000001_supervisor_road",
  "m20261222_000001_job_words",
  "m20261223_000001_deploy_notices_kept",
  "m20261224_000001_person_node_shared",
  "m20261225_000001_supervisor_limits",
  "m20261226_000001_asking_skill_name",
  "m20261227_000001_queue_notices_kept",
  "m20261228_000001_call_names",
  "m20261229_000001_short_slots",
  "m20261230_000001_run_turns",
  "m20261231_000001_double_gone",
  "m20270101_000001_support_auto",
  "m20270102_000001_lent_checkouts",
  "m20270103_000001_gateway_call_keys",
  "m20270104_000001_personal_supply",
  "m20270105_000001_owned_accounts",
  "m20270106_000001_supply_reservations",
  "m20270107_000001_run_cost",
  "m20270108_000001_gate_keys_retired",
  "m20270109_000001_quota_share_gone",
  "m20270110_000001_queue_notices_one_road",
  "m20270111_000001_local_ai",
  "m20270111_000001_registration_owner",
  "m20270112_000002_deploy_due_retired",
  "m20270113_000001_road_advisor",
  "m20270114_000001_follows_asker",
  "m20270115_000001_organisation_supply",
  "m20270116_000001_session_reset_clean",
  "m20270120_000001_supervisor_sleep",
  "m20270121_000001_note_assistant_plain",
  "m20270122_000001_agent_attachments",
  "m20270123_000001_mcp_servers",
];

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
  fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    vec![Box::new(m20270124_000001_baseline::Migration)]
  }

  fn migration_table_name() -> DynIden {
    Alias::new(MIGRATIONS_TABLE).into_iden()
  }
}

/// The store brought to this build's shape: the migrations run, the baseline first; whether the baseline ran
/// now, laying every table down. A database whose record names every migration of the last release before
/// 1.0.0, and nothing else, has that record rewritten to name the baseline alone, touching no table and no
/// row; one whose record names only part of them is refused.
pub async fn up(db: &DatabaseConnection) -> Result<bool, DbErr> {
  adopt(db).await?;

  let laid = Migrator::get_pending_migrations(db)
    .await?
    .iter()
    .any(|pending| pending.name() == BASELINE);

  Migrator::up(db, None).await?;

  Ok(laid)
}

async fn adopt(db: &DatabaseConnection) -> Result<(), DbErr> {
  let tx = db.begin().await?;
  let statement = |sql: String, values: Vec<sea_orm::Value>| {
    Statement::from_sql_and_values(DbBackend::Postgres, sql, values)
  };
  let recorded = tx
    .query_one(statement(
      format!("SELECT to_regclass('{MIGRATIONS_TABLE}') IS NOT NULL AS held"),
      vec![],
    ))
    .await?
    .map(|row| row.try_get::<bool>("", "held"))
    .transpose()?
    .unwrap_or(false);
  if !recorded {
    return Ok(());
  }
  tx.execute_unprepared(&format!(r#"LOCK TABLE "{MIGRATIONS_TABLE}" IN EXCLUSIVE MODE"#))
    .await?;
  let held = tx
    .query_all(statement(
      format!(r#"SELECT "version" FROM "{MIGRATIONS_TABLE}" ORDER BY "version""#),
      vec![],
    ))
    .await?
    .into_iter()
    .map(|row| row.try_get::<String>("", "version"))
    .collect::<Result<Vec<_>, _>>()?;
  if held.is_empty() || !held.iter().all(|version| BEFORE.contains(&version.as_str())) {
    return Ok(());
  }
  if held.len() != BEFORE.len() {
    let reached = held.last().map_or("", String::as_str);
    return Err(DbErr::Migration(format!(
      "the database holds the migrations of a release before 1.0.0 only through {reached}; that release's \
       last migration runs first"
    )));
  }
  tx.execute_unprepared(&format!(r#"DELETE FROM "{MIGRATIONS_TABLE}""#)).await?;
  tx.execute(statement(
    format!(
      r#"INSERT INTO "{MIGRATIONS_TABLE}" ("version", "applied_at") VALUES ($1, EXTRACT(EPOCH FROM now())::bigint)"#
    ),
    vec![BASELINE.into()],
  ))
  .await?;
  tx.commit().await
}
