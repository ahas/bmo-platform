//! The server's tests: its ground, each piece asked as a handler or a later part of the server asks it.

mod account_isolation;
mod account_mark;
mod one_road_turn;
mod pin_model;
mod prompt_layers;
mod relays;

mod agent_fold;
mod agent_roles;
mod apart_session;
mod apps;
mod attachments;
mod cards;
mod chat_runs;
mod chat_stand_in;
mod clock;
mod composition;
mod conversation_session;
mod node_tools;
mod own_agents;
mod role_runs;
mod role_sessions;
mod command_reading {
  mod habits;
  mod job_guard;
  mod reading;
  mod records;
  mod removal;
  mod shell;
  mod writes;
}

mod setting_handovers;
mod skills;
mod sleep;
mod supervisor_homes;
mod supervisor_limits;
mod supervisor_prompt_change;
mod supervisor_turn {
  mod going;
  mod guard;
  mod reset;
  mod run;
  mod sessions;
}
mod config;
mod control {
  mod access;
  mod account_routes;
  mod agent_roles;
  mod base_prompts;
  mod brain_reads;
  mod chat_documents;
  mod claude_account_changes;
  mod claude_accounts;
  mod claude_logins;
  mod console_reads;
  mod consumer_calls;
  mod conversation_routes;
  mod drain;
  mod endpoint_accounts;
  mod events_publish;
  mod executions;
  mod fallback;
  mod front;
  mod gate_kinds;
  mod gateway_members;
  mod harness;
  mod item_filings;
  mod job_cut;
  mod job_end;
  mod job_log;
  mod job_notes;
  mod job_reaper;
  mod job_restart;
  mod job_resume;
  mod job_reviews;
  mod job_roads;
  mod job_stops;
  mod jobs;
  mod loans;
  mod mcp;
  mod models;
  mod own_llm;
  mod owned_accounts;
  mod owner_removal;
  mod pane_death;
  mod person_node_routes;
  mod personal_models;
  mod personal_supply;
  mod pickup_restart;
  mod preamble;
  mod registry;
  mod repository_addition;
  mod repository_removal;
  mod resources;
  mod room_session_reads;
  mod run_launch;
  mod session_ops;
  mod session_reset_clean;
  mod sessions;
  mod supervisor_reset;
  mod supervisor_road;
  mod supervisor_settings;
  mod supply_test;
  mod support_growth;
  mod turn_restart;
  mod worker_runtime;
}
mod conversational;
mod data_move;
mod doctor;

mod door {
  mod apart;
  mod aside;
  mod confirm;
  mod endpoint;
  mod execution;
  mod guard;
  mod inbox;
  mod liveness;
  mod looks;
  mod node_take;
  mod place_run;
  mod record;
  mod record_secrets;
  mod rollout;
  mod stall;
  mod stand;
  mod tool_clock;
  mod tool_clock_notes;
  mod turn;
  mod written_call;
}
mod events;
mod ground;
mod handoff;
mod herdr_backend;
mod hooks;
mod host_run;
mod job_handoff;

// The job run members built so far; the reset stands beside this and is declared with its module.
mod job_leases;
mod job_list;
mod job_notices;
mod job_stall;
mod outbox;
mod job_run {
  mod claude {
    mod failure;
    mod turn;
  }
  mod openrouter;
  mod openrouter_tap;
}
mod job_words;
mod label_values;
mod keepalive {
  mod logins;
}
mod order_run {
  mod places;
}
mod catalogue;
mod peers;
mod person_nodes;
mod personal_apps;

mod lines {
  mod advisor;
  mod agent_plugin;
  mod carries;
  mod chat;
  mod claude;
  mod codex;
  mod dsh;
  mod ordered;
  mod persona;
  mod place;
  mod prefix;
  mod runner;
  mod standing;
  mod token;
  mod trees;
  mod worker;
}

// The places built so far; the others stand beside these and are declared with their modules.
mod places {
  mod base_mains;
  mod branch;
  mod checkouts;
  mod claims;
  mod follow;
  mod gate_places;
  mod ground;
  mod handed_tokens;
  mod home;
  mod idle_supports;
  mod item_sweep;
  mod locks;
  mod nodes;
  mod workbench;
}
mod queue {
  mod cancels;
  mod leftovers;
  mod orderer;
  mod paths;
  mod reconcile;
  mod routes;
  mod settings;
  mod workers;
}
mod keepalive_agents;
mod readiness;
mod rest;
mod road;
mod role_slots;
mod rooms;
mod run_clock;
mod run_groups;
mod run_list;
mod run_sweep;
mod seeds;
mod session_output;
mod session_questions;
mod session_tools;
mod slot_events;
mod streaming;
mod supply;
mod translate;
mod turn_close;
mod turn_keys;
mod turn_pickup;
mod turn_pickup_clocks;
mod usage_balances;
mod words;
mod worker_english;

mod run_log {
  mod codex;
  mod json;
  mod lines;
  mod pane_calls;
  mod secrets;
  mod spans;
  mod writer;
}

mod verbs {
  mod claude_batch;
  mod node_pane;
  mod node_service;
  mod quiet;
  mod reload;
  mod work_node_setup;
}

mod watches {
  mod background_work;
  mod closes;
  mod deploy_failures;
  mod deploy_flush;
  mod job_events;
  mod links;
  mod machine;
  mod machine_turn;
  mod notices_owed;
  mod pressure;
  mod pull_requests;
  mod queue_idle;
}
mod wording;

mod worker_session {
  mod fold;
  mod held;
  mod records;
  mod told;
  mod warmth;
}
