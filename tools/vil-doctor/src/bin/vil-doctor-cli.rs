//! VIL Doctor, console edition: same checks and report as the window app.

use std::collections::HashMap;
use std::io::{BufRead, IsTerminal};
use std::sync::Arc;
use std::time::{Duration, Instant};

use vil_doctor::console::Ui;
use vil_doctor::engine::{self, Event};
use vil_doctor::model::{Ctx, Section, Sev};
use vil_doctor::{report, text, win, VERSION};

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
    let labels = engine::labels();
    let n = labels.len();
    let rx = engine::spawn(ctx.clone());

    // Workers finish out of order; the console shows sections strictly in order.
    let mut done: HashMap<usize, Section> = HashMap::new();
    let mut sections = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        let spin = ui.start(i + 1, n, label);
        while !done.contains_key(&i) {
            match rx.recv_timeout(Duration::from_secs(45 * 60)) {
                Ok(Event::Done(k, s)) => {
                    done.insert(k, s);
                }
                Ok(Event::Started(_)) => {}
                Err(_) => {
                    let mut s = Section::new(label);
                    s.error("проверка не завершилась");
                    done.insert(i, s);
                }
            }
        }
        let s = done.remove(&i).unwrap();
        let (sev, summary) = engine::summarize(&s);
        spin.finish(sev, &summary);
        sections.push(s);
    }

    let fin = engine::finish(&ctx, &sections, started, opts.out.as_deref());
    let saved = fin.saved;
    let t = fin.totals;
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
