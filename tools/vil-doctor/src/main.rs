//! VIL Doctor — read-only Windows health check with a detailed text report.

mod checks;
mod console;
mod model;
mod paths;
mod ps;
mod report;
mod text;
mod win;

use std::collections::VecDeque;
use std::io::{BufRead, IsTerminal};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use console::Ui;
use model::{Ctx, Section, Sev};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const WORKERS: usize = 3;

struct Opts {
    deep: Option<bool>,
    elevate: bool,
    out: Option<String>,
    open: bool,
    pause: Option<bool>,
}

fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts { deep: None, elevate: true, out: None, open: true, pause: None };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--deep" => o.deep = Some(true),
            "--quick" => o.deep = Some(false),
            "--no-elevate" | "--elevated" => o.elevate = false,
            "--no-open" => o.open = false,
            "--no-pause" => o.pause = Some(false),
            "--pause" => o.pause = Some(true),
            "--out" => o.out = Some(it.next().ok_or("после --out укажите папку")?.clone()),
            "-h" | "--help" | "/?" => return Err(String::new()),
            "-V" | "--version" => {
                println!("VIL Doctor {VERSION}");
                std::process::exit(0);
            }
            other => return Err(format!("неизвестный параметр: {other}")),
        }
    }
    Ok(o)
}

const HELP: &str = "VIL Doctor — диагностика Windows (только чтение)

Запуск: vil-doctor.exe [параметры]
  --deep        глубокая проверка образа Windows (DISM ScanHealth, 5–20 минут)
  --quick       без вопроса о глубокой проверке
  --out ПАПКА   куда сохранить отчёт (по умолчанию — Рабочий стол)
  --no-elevate  не запрашивать права администратора
  --no-open     не открывать отчёт в Блокноте
  --no-pause    не ждать Enter в конце
  --version     версия";

fn ask_yes(ui: &Ui, q: &str) -> bool {
    ui.println(q);
    print!("  > ");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    matches!(line.trim().to_lowercase().as_str(), "д" | "да" | "y" | "yes" | "l")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}\n");
            }
            println!("{HELP}");
            std::process::exit(if e.is_empty() { 0 } else { 2 });
        }
    };
    let ui = Ui::new();
    let pause = opts.pause.unwrap_or_else(win::owns_console);
    win::set_title("VIL Doctor — диагностика Windows");

    let mut admin = win::is_admin();
    if !admin && opts.elevate && cfg!(windows) {
        let mut relaunch = args.clone();
        relaunch.push("--elevated".into());
        if opts.out.is_none() {
            if let Some(d) = win::desktop_dir() {
                relaunch.extend(["--out".into(), d]);
            }
        }
        if pause {
            relaunch.push("--pause".into());
        }
        ui.println("");
        ui.println("  Для полной проверки нужны права администратора — подтвердите запрос Windows.");
        if win::relaunch_elevated(&relaunch) {
            return;
        }
        ui.println(&ui.dim("  Запрос отклонён — продолжаю без прав администратора (часть проверок будет пропущена)."));
    }
    admin = admin || win::is_admin();

    ui.println("");
    ui.println(&format!("  {}", ui.bold(&format!("VIL Doctor {VERSION} · диагностика Windows"))));
    ui.println(&ui.dim("  Только чтение — программа ничего не меняет в системе."));
    ui.println("");

    let deep = match opts.deep {
        Some(d) => d && admin,
        None if admin && std::io::stdin().is_terminal() => {
            let yes = ask_yes(&ui, "  Выполнить глубокую проверку образа Windows? Это 5–20 минут. [д/Н]");
            ui.println("");
            yes
        }
        None => false,
    };

    let started = Instant::now();
    let ctx = Arc::new(Ctx { admin, deep, today: text::today_utc() });
    let jobs = checks::all();
    let n = jobs.len();
    let labels: Vec<&'static str> = jobs.iter().map(|(l, _)| *l).collect();

    // Checks run on a small pool; results are shown strictly in order.
    let queue = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<VecDeque<_>>()));
    let (txs, rxs): (Vec<_>, Vec<_>) = (0..n).map(|_| mpsc::channel::<Section>()).unzip();
    let txs = Arc::new(txs);
    for _ in 0..WORKERS {
        let (q, txs, ctx) = (queue.clone(), txs.clone(), ctx.clone());
        std::thread::spawn(move || loop {
            let Some((i, (label, job))) = q.lock().unwrap().pop_front() else { break };
            let section = std::panic::catch_unwind(|| job(&ctx)).unwrap_or_else(|_| {
                let mut s = Section::new(label);
                s.error("внутренняя ошибка проверки");
                s
            });
            let _ = txs[i].send(section);
        });
    }

    let mut sections = Vec::new();
    for (i, rx) in rxs.into_iter().enumerate() {
        let spin = ui.start(i + 1, n, labels[i]);
        let s = rx.recv_timeout(Duration::from_secs(45 * 60)).unwrap_or_else(|_| {
            let mut s = Section::new(labels[i]);
            s.error("проверка не завершилась");
            s
        });
        let (sev, summary) = summarize(&s);
        spin.finish(sev, &summary);
        sections.push(s);
    }

    let computer = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "—".into());
    let os = sections
        .iter()
        .flat_map(|s| s.facts.iter())
        .find(|(k, _)| k == "Windows")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "Windows".into());
    let (y, mo, d, h, mi) = win::local_time();
    let meta = report::Meta {
        computer,
        os,
        when: format!("{d:02}.{mo:02}.{y} {h:02}:{mi:02}"),
        admin,
        deep,
        seconds: started.elapsed().as_secs(),
        version: VERSION,
    };
    let body = report::render(&meta, &sections);
    let name = format!("VIL-Doctor_{y}-{mo:02}-{d:02}_{h:02}-{mi:02}.txt");
    let saved = save_report(opts.out.as_deref(), &name, &body);

    let t = report::totals(&sections);
    ui.println("");
    let verdict_sev = if t.critical > 0 { Sev::Critical } else if t.warning > 0 { Sev::Warning } else { Sev::Ok };
    ui.println(&format!("  {} · индекс здоровья {}/100", ui.sev(verdict_sev, report::verdict(&t)), t.score));
    ui.println(&format!(
        "  {}   {}   {}",
        ui.sev(Sev::Critical, &format!("Критично: {}", t.critical)),
        ui.sev(Sev::Warning, &format!("Внимание: {}", t.warning)),
        ui.sev(Sev::Info, &format!("Советы: {}", t.info)),
    ));
    ui.println("");
    match &saved {
        Ok(path) => {
            ui.println(&format!("  Подробный отчёт: {}", ui.bold(path)));
            if opts.open && cfg!(windows) {
                let _ = std::process::Command::new("notepad.exe").arg(path).spawn();
            }
        }
        Err(e) => ui.println(&ui.sev(Sev::Critical, &format!("  Не удалось сохранить отчёт: {e}"))),
    }
    if pause {
        ui.println("");
        ui.println(&ui.dim("  Нажмите Enter, чтобы закрыть окно."));
        let mut s = String::new();
        let _ = std::io::stdin().lock().read_line(&mut s);
    }
    std::process::exit(if saved.is_err() { 3 } else if t.critical > 0 { 2 } else if t.warning > 0 { 1 } else { 0 });
}

fn summarize(s: &Section) -> (Option<Sev>, String) {
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
