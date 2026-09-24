//! Runs all checks on a small worker pool and saves the report.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{json, Value};

use crate::model::{Ctx, Section, Sev};
use crate::{checks, report, text, win};

const WORKERS: usize = 3;

pub enum Event {
    Started(usize),
    Done(usize, Section),
}

pub fn labels() -> Vec<&'static str> {
    checks::all().into_iter().map(|(l, _)| l).collect()
}

/// Starts every check; events arrive as workers pick up and finish jobs.
pub fn spawn(ctx: Arc<Ctx>) -> Receiver<Event> {
    let queue = Arc::new(Mutex::new(checks::all().into_iter().enumerate().collect::<VecDeque<_>>()));
    let (tx, rx) = mpsc::channel();
    for _ in 0..WORKERS {
        let (q, tx, ctx) = (queue.clone(), tx.clone(), ctx.clone());
        std::thread::spawn(move || loop {
            let Some((i, (label, job))) = q.lock().unwrap().pop_front() else { break };
            let _ = tx.send(Event::Started(i));
            let section = std::panic::catch_unwind(|| job(&ctx)).unwrap_or_else(|_| {
                let mut s = Section::new(label);
                s.error("внутренняя ошибка проверки");
                s
            });
            let _ = tx.send(Event::Done(i, section));
        });
    }
    rx
}

/// Short status for a finished section, e.g. "2 замечания".
pub fn summarize(s: &Section) -> (Option<Sev>, String) {
    let (c, w, i) = (s.count(Sev::Critical), s.count(Sev::Warning), s.count(Sev::Info));
    if c > 0 {
        return (Some(Sev::Critical), format!("{c} {}", text::plural(c as u64, "критичная", "критичные", "критичных")));
    }
    if w > 0 {
        return (Some(Sev::Warning), format!("{w} {}", text::plural(w as u64, "замечание", "замечания", "замечаний")));
    }
    if !s.errors.is_empty() {
        return (None, if s.findings.is_empty() { "нет данных".into() } else { "частично".into() });
    }
    if s.skipped.is_some() && s.findings.is_empty() {
        return (None, "пропущено".into());
    }
    if i > 0 {
        return (Some(Sev::Info), format!("OK, {i} {}", text::plural(i as u64, "совет", "совета", "советов")));
    }
    (Some(Sev::Ok), "OK".into())
}

pub struct Finished {
    pub totals: report::Totals,
    pub saved: Result<String, String>,
}

pub fn finish(ctx: &Ctx, sections: &[Section], started: Instant, out: Option<&str>) -> Finished {
    let computer = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "—".into());
    let os = os_name(sections).unwrap_or_else(|| "Windows".into());
    let (y, mo, d, h, mi) = win::local_time();
    let meta = report::Meta {
        computer,
        os,
        when: format!("{d:02}.{mo:02}.{y} {h:02}:{mi:02}"),
        admin: ctx.admin,
        deep: ctx.deep,
        seconds: started.elapsed().as_secs(),
        version: crate::VERSION,
    };
    let body = report::render(&meta, sections);
    let name = format!("VIL-Doctor_{y}-{mo:02}-{d:02}_{h:02}-{mi:02}.txt");
    Finished { totals: report::totals(sections), saved: save_report(out, &name, &body) }
}

pub fn os_name(sections: &[Section]) -> Option<String> {
    sections.iter().flat_map(|s| s.facts.iter()).find(|(k, _)| k == "Windows").map(|(_, v)| v.clone())
}

fn save_report(out: Option<&str>, name: &str, body: &str) -> Result<String, String> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(o) = out {
        dirs.push(o.into());
    }
    if let Some(d) = win::desktop_dir() {
        dirs.push(d.into());
    }
    if let Some(d) = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())) {
        dirs.push(d);
    }
    dirs.push(std::env::temp_dir());
    let mut last = String::from("нет доступной папки");
    for d in dirs {
        let p = d.join(name);
        match std::fs::write(&p, body) {
            Ok(()) => return Ok(p.to_string_lossy().into_owned()),
            Err(e) => last = format!("{}: {e}", p.display()),
        }
    }
    Err(last)
}

fn sev_key(s: Sev) -> &'static str {
    match s {
        Sev::Critical => "critical",
        Sev::Warning => "warning",
        Sev::Info => "info",
        Sev::Ok => "ok",
    }
}

/// Section as JSON for the window UI; findings are sorted worst first.
pub fn section_json(i: usize, s: &Section) -> Value {
    let (sev, summary) = summarize(s);
    let mut findings: Vec<_> = s.findings.iter().collect();
    findings.sort_by_key(|f| f.sev);
    json!({
        "index": i,
        "title": s.title,
        "status": sev.map(sev_key).unwrap_or("none"),
        "summary": summary,
        "facts": s.facts.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
        "findings": findings.iter().map(|f| json!({
            "sev": sev_key(f.sev),
            "title": f.title,
            "details": f.details,
            "fix": f.fix,
        })).collect::<Vec<_>>(),
        "errors": s.errors,
    })
}

pub fn totals_json(t: &report::Totals) -> Value {
    json!({
        "critical": t.critical,
        "warning": t.warning,
        "info": t.info,
        "ok": t.ok,
        "score": t.score,
        "verdict": report::verdict(t),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_json_sorts_and_summarizes() {
        let mut s = Section::new("Диски");
        s.ok("SMART в норме");
        s.add(Sev::Warning, "Мало места").fix("Очистите диск");
        let v = section_json(1, &s);
        assert_eq!(v["status"], "warning");
        assert_eq!(v["summary"], "1 замечание");
        assert_eq!(v["findings"][0]["sev"], "warning");
        assert_eq!(v["findings"][1]["fix"], Value::Null);
    }
}
