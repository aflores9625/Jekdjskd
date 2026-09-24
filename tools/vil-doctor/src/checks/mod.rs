use crate::model::{Ctx, Section};

mod devices;
mod disks;
mod events;
mod files;
mod network;
mod perf;
mod refs;
mod runtimes;
mod security;
mod services;
mod startup;
mod system;
mod updates;

pub type Job = fn(&Ctx) -> Section;

/// Report order: the machine first, then what breaks it most often.
pub fn all() -> Vec<(&'static str, Job)> {
    vec![
        (system::TITLE, system::run as Job),
        (disks::TITLE, disks::run),
        (files::TITLE, files::run),
        (runtimes::TITLE, runtimes::run),
        (updates::TITLE, updates::run),
        (security::TITLE, security::run),
        (services::TITLE, services::run),
        (devices::TITLE, devices::run),
        (events::TITLE, events::run),
        (startup::TITLE, startup::run),
        (refs::TITLE, refs::run),
        (network::TITLE, network::run),
        (perf::TITLE, perf::run),
    ]
}

/// Runs a PowerShell collector, recording failures on the section.
pub fn collect(s: &mut Section, script: &str, secs: u64) -> Option<serde_json::Value> {
    match crate::ps::run_json(script, crate::ps::secs(secs)) {
        Ok(v) => {
            for e in crate::model::str_list(&v, "_errs") {
                s.error(format!("часть данных не получена: {}", crate::text::trunc(&e, 200)));
            }
            Some(v)
        }
        Err(e) => {
            s.error(e);
            None
        }
    }
}

pub fn sysroot() -> String {
    std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())
}

pub fn admin_note(ctx: &Ctx) -> &'static str {
    if ctx.admin {
        ""
    } else {
        " (без прав администратора данные могут быть неполными)"
    }
}
