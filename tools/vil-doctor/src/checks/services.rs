use super::collect;
use crate::model::*;
use crate::paths::{fs_exists, resolve_command, system_env, Env, Res};

pub const TITLE: &str = "Службы";

const PS: &str = r##"
$svc = @(Get-CimInstance Win32_Service | ForEach-Object { [ordered]@{ n = $_.Name; d = $_.DisplayName; m = $_.StartMode; st = $_.State; p = $_.PathName; dl = $_.DelayedAutoStart } })
$drv = @(Get-CimInstance Win32_SystemDriver | Where-Object { $_.StartMode -ne 'Disabled' } | ForEach-Object { [ordered]@{ n = $_.Name; d = $_.DisplayName; m = $_.StartMode; p = $_.PathName } })
$r = [ordered]@{ svc = $svc; drv = $drv; uptime_min = [math]::Round(((Get-Date) - (Get-CimInstance Win32_OperatingSystem).LastBootUpTime).TotalMinutes) }
"##;

/// (service, what it does, severity when broken)
const IMPORTANT: &[(&str, &str, Sev)] = &[
    ("RpcSs", "удалённый вызов процедур — основа Windows", Sev::Critical),
    ("DcomLaunch", "запуск компонентов DCOM", Sev::Critical),
    ("EventLog", "журнал событий", Sev::Critical),
    ("Winmgmt", "инструментарий WMI", Sev::Critical),
    ("Dnscache", "DNS-клиент — без него не работает интернет", Sev::Critical),
    ("BFE", "базовый модуль фильтрации (нужен брандмауэру)", Sev::Critical),
    ("mpssvc", "брандмауэр Windows", Sev::Critical),
    ("CryptSvc", "службы криптографии (обновления, сертификаты)", Sev::Critical),
    ("Schedule", "планировщик заданий", Sev::Critical),
    ("ProfSvc", "служба профилей пользователей", Sev::Critical),
    ("PlugPlay", "Plug and Play — подключение устройств", Sev::Critical),
    ("Power", "электропитание", Sev::Critical),
    ("wscsvc", "центр безопасности (отключают вредоносные программы)", Sev::Warning),
    ("Audiosrv", "звук Windows", Sev::Warning),
    ("AudioEndpointBuilder", "аудиоустройства", Sev::Warning),
    ("Dhcp", "DHCP-клиент — получение сетевого адреса", Sev::Warning),
    ("nsi", "служба сетевых интерфейсов", Sev::Warning),
    ("LanmanWorkstation", "доступ к сетевым папкам", Sev::Warning),
    ("Themes", "темы оформления", Sev::Warning),
    ("BITS", "фоновая передача (обновления, Store)", Sev::Warning),
    ("TrustedInstaller", "установщик модулей Windows (обновления)", Sev::Warning),
    ("msiserver", "установщик Windows — установка программ", Sev::Warning),
    ("W32Time", "синхронизация времени", Sev::Warning),
    ("WSearch", "поиск Windows", Sev::Info),
    ("Spooler", "печать", Sev::Info),
];

/// Auto-start services that routinely stop on their own; never worth reporting.
const QUIET: &[&str] = &[
    "gupdate", "gupdatem", "edgeupdate", "edgeupdatem", "microsoftedgeelevationservice", "sppsvc", "remoteregistry", "mapsbroker",
    "wuauserv", "bits", "trustedinstaller", "wbiosrvc", "cdpsvc", "usosvc", "dosvc", "waasmedicsvc", "tiledatamodelsvc",
    "shellhwdetection", "sysmain", "clr_optimization_v4.0.30319_32", "clr_optimization_v4.0.30319_64", "googleupdaterservice",
    "googleupdaterinternalservice", "wsearch", "stisvc", "brokerinfrastructure", "intelaudioservice", "onesyncsvc", "cdpusersvc",
    "wpnuserservice", "gameinputsvc", "sense", "wmansvc", "igfxcuiservice2.0.0.0", "officesvcmgr",
];

pub fn run(_ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let Some(v) = collect(&mut s, PS, 90) else { return s };
    let svcs = v.list("svc");
    s.fact("Всего служб", svcs.len().to_string());
    let running = svcs.iter().filter(|x| x.s("st").as_deref() == Some("Running")).count();
    s.fact("Запущено", running.to_string());

    let find = |name: &str| svcs.iter().find(|x| x.s("n").is_some_and(|n| n.eq_ignore_ascii_case(name)));
    let mut broken = false;
    for (name, what, sev) in IMPORTANT {
        let Some(x) = find(name) else { continue };
        let mode = x.s("m").unwrap_or_default();
        let state = x.s("st").unwrap_or_default();
        let display = x.s("d").unwrap_or_else(|| name.to_string());
        let problem = if mode == "Disabled" {
            Some("отключена")
        } else if mode == "Auto" && state != "Running" && !matches!(*name, "BITS" | "TrustedInstaller" | "W32Time" | "WSearch") {
            Some("не запущена")
        } else {
            None
        };
        if let Some(p) = problem {
            broken = true;
            s.add(*sev, format!("Служба «{display}» {p}"))
                .detail(format!("Назначение: {what}."))
                .fix(format!("Win+R → services.msc → «{display}» → тип запуска «Автоматически» (или «Вручную»), затем «Запустить»."));
        }
    }
    if !broken && !svcs.is_empty() {
        s.ok("Важные системные службы работают");
    }

    if v.n("uptime_min").unwrap_or(0.0) > 10.0 {
        let stopped: Vec<String> = svcs
            .iter()
            .filter(|x| x.s("m").as_deref() == Some("Auto") && x.s("st").as_deref() != Some("Running"))
            .filter(|x| {
                let n = x.s("n").unwrap_or_default().to_ascii_lowercase();
                let base = n.split('_').next().unwrap_or(&n).to_string();
                !QUIET.contains(&n.as_str()) && !QUIET.contains(&base.as_str()) && !IMPORTANT.iter().any(|(i, _, _)| i.eq_ignore_ascii_case(&n))
            })
            .map(|x| format!("{} ({})", x.s("d").unwrap_or_default(), x.s("n").unwrap_or_default()))
            .collect();
        if !stopped.is_empty() {
            s.add(Sev::Info, format!("Автозапускаемые службы, которые сейчас остановлены: {}", stopped.len()))
                .list(stopped, 12)
                .fix("Часто это нормально (служба запускается по событию). Если связанная программа не работает — запустите службу в services.msc.");
        }
    }

    let (var, search) = system_env();
    let env = Env { var: &*var, exists: &fs_exists, search };
    let mut svc_missing = Vec::new();
    for x in &svcs {
        if x.s("m").as_deref() == Some("Disabled") {
            continue;
        }
        if let Some(Res::Missing(m)) = x.s("p").map(|p| resolve_command(&p, &env)) {
            svc_missing.push(format!("{} — {m}", x.s("d").unwrap_or_default()));
        }
    }
    if !svc_missing.is_empty() {
        s.add(Sev::Warning, format!("Службы ссылаются на несуществующие файлы: {}", svc_missing.len()))
            .detail("Обычно это остатки удалённых программ; при загрузке они вызывают ошибки в журнале.")
            .list(svc_missing, 12)
            .fix("Переустановите программу, которой принадлежит служба, или удалите службу: sc delete ИМЯ (в командной строке администратора, только если уверены).");
    }

    let (mut drv_bad, mut drv_left) = (Vec::new(), Vec::new());
    for x in v.list("drv") {
        let Some(p) = x.s("p") else { continue };
        if let Res::Missing(m) = resolve_command(&p, &env) {
            let line = format!("{} — {m}", x.s("d").or(x.s("n")).unwrap_or_default());
            match x.s("m").as_deref() {
                Some("Boot") | Some("System") | Some("Auto") => drv_bad.push(line),
                _ => drv_left.push(line),
            }
        }
    }
    if !drv_bad.is_empty() {
        s.add(Sev::Warning, format!("Драйверы с автозагрузкой, файлы которых отсутствуют: {}", drv_bad.len()))
            .list(drv_bad, 10)
            .fix("Переустановите устройство или программу, которой принадлежит драйвер (антивирус, эмулятор, VPN).");
    }
    if !drv_left.is_empty() {
        s.add(Sev::Info, format!("Остатки драйверов удалённых программ: {}", drv_left.len()))
            .list(drv_left, 8)
            .fix("Не мешают работе: запускаются только по требованию.");
    }
    s
}
