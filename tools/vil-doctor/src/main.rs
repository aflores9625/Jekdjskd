//! VIL Doctor window: the same checks as the console edition behind a WebView2 UI.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    gui::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Оконная версия VIL Doctor работает только в Windows. Для других систем используйте vil-doctor-cli.");
    std::process::exit(1);
}

#[cfg(windows)]
mod gui {
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use serde_json::{json, Value};
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
    use tao::platform::windows::{IconExtWindows, WindowBuilderExtWindows};
    use tao::window::{Icon, Theme, WindowBuilder};
    use wry::{WebContext, WebViewBuilder};

    use vil_doctor::engine::{self, Event as Check};
    use vil_doctor::model::{Ctx, Section};
    use vil_doctor::{text, win, VERSION};

    const HTML: &str = include_str!("gui/index.html");

    enum UserEvent {
        Js(Value),
        Exit,
    }

    fn send(proxy: &EventLoopProxy<UserEvent>, v: Value) {
        let _ = proxy.send_event(UserEvent::Js(v));
    }

    pub fn run() {
        let admin = win::is_admin();
        let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
        let proxy = event_loop.create_proxy();

        let fail = |what: &str| -> ! {
            win::error_box("VIL Doctor", what);
            std::process::exit(1);
        };

        let window = WindowBuilder::new()
            .with_title("VIL Doctor")
            .with_theme(Some(Theme::Light))
            // Icon ordinal 1 is embedded by build.rs from assets/icon.ico.
            .with_window_icon(Icon::from_resource(1, None).ok())
            .with_taskbar_icon(Icon::from_resource(1, None).ok())
            .with_background_color((255, 255, 255, 255))
            .with_inner_size(LogicalSize::new(1080.0, 740.0))
            .with_min_inner_size(LogicalSize::new(900.0, 620.0))
            .build(&event_loop)
            .unwrap_or_else(|e| fail(&format!("Не удалось создать окно: {e}")));

        // Keep the WebView2 profile out of the folder the exe was downloaded to.
        let data_dir = std::env::var_os("LOCALAPPDATA")
            .map(|d| std::path::PathBuf::from(d).join("VIL Doctor").join("WebView2"))
            .unwrap_or_else(|| std::env::temp_dir().join("VIL Doctor WebView2"));
        let mut web_context = WebContext::new(Some(data_dir));

        let report_path: Arc<Mutex<Option<String>>> = Arc::default();
        let busy = Arc::new(Mutex::new(false));

        let webview = WebViewBuilder::new(&window)
            .with_web_context(&mut web_context)
            .with_html(HTML)
            .with_background_color((255, 255, 255, 255))
            .with_devtools(cfg!(debug_assertions))
            .with_navigation_handler(|url| url.starts_with("about:") || url.starts_with("data:"))
            .with_new_window_req_handler(|_| false)
            .with_ipc_handler({
                let (proxy, report_path, busy) = (proxy.clone(), report_path.clone(), busy.clone());
                move |req| handle(req.into_body(), admin, &proxy, &report_path, &busy)
            })
            .build()
            .unwrap_or_else(|e| {
                fail(&format!(
                    "Не удалось запустить WebView2: {e}\n\nУстановите Microsoft Edge WebView2 Runtime (Evergreen) с сайта Microsoft или воспользуйтесь консольной версией vil-doctor-cli.exe."
                ))
            });

        event_loop.run(move |event, _, flow| {
            *flow = ControlFlow::Wait;
            match event {
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } | Event::UserEvent(UserEvent::Exit) => {
                    *flow = ControlFlow::Exit;
                }
                Event::UserEvent(UserEvent::Js(v)) => {
                    let _ = webview.evaluate_script(&format!("window.vil&&window.vil.receive({v})"));
                }
                _ => {}
            }
        });
    }

    fn handle(body: String, admin: bool, proxy: &EventLoopProxy<UserEvent>, report: &Arc<Mutex<Option<String>>>, busy: &Arc<Mutex<bool>>) {
        let msg: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        match msg["cmd"].as_str().unwrap_or("") {
            "ready" => send(
                proxy,
                json!({
                    "type": "init",
                    "admin": admin,
                    "version": VERSION,
                    "computer": std::env::var("COMPUTERNAME").unwrap_or_default(),
                    "labels": engine::labels(),
                }),
            ),
            "elevate" => {
                if win::relaunch_elevated(&["--elevated".to_string()]) {
                    let _ = proxy.send_event(UserEvent::Exit);
                } else {
                    send(proxy, json!({ "type": "elevate-failed" }));
                }
            }
            "start" => {
                let mut b = busy.lock().unwrap();
                if *b {
                    return;
                }
                *b = true;
                let deep = admin && msg["deep"].as_bool().unwrap_or(false);
                let (proxy, report, busy) = (proxy.clone(), report.clone(), busy.clone());
                std::thread::spawn(move || {
                    scan(deep, admin, &proxy, &report);
                    *busy.lock().unwrap() = false;
                });
            }
            "open" => {
                if let Some(p) = report.lock().unwrap().clone() {
                    let _ = std::process::Command::new("notepad.exe").arg(p).spawn();
                }
            }
            "reveal" => {
                if let Some(p) = report.lock().unwrap().clone() {
                    use std::os::windows::process::CommandExt;
                    // explorer needs the raw `/select,"path"` form, which Rust's quoting would break.
                    let _ = std::process::Command::new("explorer.exe").raw_arg(format!("/select,\"{p}\"")).spawn();
                }
            }
            _ => {}
        }
    }

    fn scan(deep: bool, admin: bool, proxy: &EventLoopProxy<UserEvent>, report: &Arc<Mutex<Option<String>>>) {
        let started = Instant::now();
        let ctx = Arc::new(Ctx { admin, deep, today: text::today_utc() });
        let n = engine::labels().len();
        let mut sections: Vec<Option<Section>> = (0..n).map(|_| None).collect();
        let rx = engine::spawn(ctx.clone());
        let mut left = n;
        while left > 0 {
            match rx.recv() {
                Ok(Check::Started(i)) => send(proxy, json!({ "type": "started", "index": i })),
                Ok(Check::Done(i, s)) => {
                    send(proxy, json!({ "type": "section", "section": engine::section_json(i, &s) }));
                    sections[i] = Some(s);
                    left -= 1;
                }
                Err(_) => break,
            }
        }
        let sections: Vec<Section> = sections.into_iter().flatten().collect();
        let fin = engine::finish(&ctx, &sections, started, None);
        let (path, error) = match fin.saved {
            Ok(p) => (Some(p), None),
            Err(e) => (None, Some(e)),
        };
        *report.lock().unwrap() = path.clone();
        send(
            proxy,
            json!({
                "type": "done",
                "totals": engine::totals_json(&fin.totals),
                "report": path,
                "error": error,
                "seconds": started.elapsed().as_secs(),
                "os": engine::os_name(&sections),
            }),
        );
    }
}
