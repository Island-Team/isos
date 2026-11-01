#[path = "app/auth.rs"]
mod auth;
#[path = "domain/backend.rs"]
mod backend;
#[path = "app/cli/mod.rs"]
mod cli;
#[path = "app/config.rs"]
mod config;
#[path = "runtime/control_grants.rs"]
mod control_grants;
#[path = "runtime/core_bins.rs"]
mod core_bins;
#[path = "runtime/core_setup.rs"]
mod core_setup;
#[path = "runtime/dependent_stop.rs"]
mod dependent_stop;
#[path = "runtime/edge_admission.rs"]
mod edge_admission;
#[path = "app/entry.rs"]
mod entry;
#[path = "runtime/event_policy.rs"]
mod event_policy;
#[path = "app/foreground.rs"]
mod foreground;
#[path = "runtime/generated_v2.rs"]
mod generated_v2;
#[path = "runtime/graph_compat.rs"]
mod graph_compat;
#[path = "runtime/graph_migration.rs"]
mod graph_migration;
#[path = "app/http.rs"]
mod http;
#[path = "app/isosui.rs"]
mod isosui;
#[path = "runtime/lock.rs"]
mod lock;
#[path = "runtime/log_store.rs"]
mod log_store;
#[path = "runtime/manager/mod.rs"]
mod manager;
#[path = "runtime/manager_log.rs"]
mod manager_log;
#[path = "domain/mujoco_catalog.rs"]
mod mujoco_catalog;
#[path = "domain/node_options.rs"]
mod node_options;
#[path = "domain/node_sets.rs"]
mod node_sets;
#[path = "domain/packages.rs"]
mod packages;
#[path = "runtime/preflight.rs"]
mod preflight;
#[path = "runtime/process.rs"]
mod process;
#[path = "runtime/reader_features.rs"]
mod reader_features;
#[path = "runtime/readiness.rs"]
mod readiness;
#[path = "domain/release.rs"]
mod release;
#[path = "domain/services.rs"]
mod services;
#[path = "domain/starter.rs"]
mod starter;

fn main() {
    if let Err(error) = entry::run() {
        eprintln!("isos: {error:#}");
        std::process::exit(1);
    }
}
