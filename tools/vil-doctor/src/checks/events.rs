use super::{admin_note, collect, sysroot};
use crate::model::*;
use crate::text::{days_human, trunc};

pub const TITLE: &str = "Журнал событий и сбои";

const PS: &str = r##"
$w7 = (Get-Date).AddDays(-7); $m30 = (Get-Date).AddDays(-30)
$sys = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; Level = @(1, 2); StartTime = $w7 } -MaxEvents 5000)
$top = @($sys | Group-Object ProviderName | Sort-Object Count -Descending | Select-Object -First 10 | ForEach-Object {
  $e = $_.Group[0]; [ordered]@{ src = $_.Name; count = $_.Count; id = $e.Id; msg = (L $e.Message 170); last = (T $e.TimeCreated) } })
$kp = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-Kernel-Power'; Id = 41; StartTime = $m30 } -MaxEvents 50 | ForEach-Object { T $_.TimeCreated })
$bsod = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WER-SystemErrorReporting'; Id = 1001; StartTime = $m30 } -MaxEvents 20 | ForEach-Object { [ordered]@{ t = (T $_.TimeCreated); msg = (L $_.Message 220) } })
$whea = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WHEA-Logger'; StartTime = $m30 } -MaxEvents 50 | ForEach-Object { [ordered]@{ t = (T $_.TimeCreated); lvl = [int]$_.Level; msg = (L $_.Message 170) } })
$disk = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = @('disk', 'Ntfs'); Level = @(1, 2, 3); StartTime = $m30 } -MaxEvents 200 |
  Where-Object { -not ($_.ProviderName -eq 'Ntfs' -and $_.Level -eq 3) } | ForEach-Object { [ordered]@{ t = (T $_.TimeCreated); src = $_.ProviderName; id = $_.Id; lvl = [int]$_.Level; msg = (L $_.Message 170) } })
function Top($log, $prov, $id) {
  @(Get-WinEvent -FilterHashtable @{ LogName = $log; ProviderName = $prov; Id = $id; StartTime = $w7 } -MaxEvents 500 |
    ForEach-Object { [pscustomobject]@{ app = [string]$_.Properties[0].Value; mod = [string]$_.Properties[3].Value } } |
    Group-Object app | Sort-Object Count -Descending | Select-Object -First 8 | ForEach-Object {
      [ordered]@{ app = $_.Name; count = $_.Count; mod = (@($_.Group | Group-Object mod | Sort-Object Count -Descending)[0]).Name } })
}
$r = [ordered]@{ sys_total = $sys.Count; top = $top; kp = $kp; bsod = $bsod; whea = $whea; disk = $disk
  crash = (Top 'Application' 'Application Error' 1000); hang = (Top 'Application' 'Application Hang' 1002) }
"##;

pub fn run(ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let root = sysroot();

    // Crash dumps: the strongest evidence of blue screens, independent of logs.
    let mut dumps: Vec<(String, f64)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(format!(r"{root}\Minidump")) {
        for e in rd.flatten() {
            let age = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).map(|d| d.as_secs_f64() / 86400.0);
            dumps.push((e.file_name().to_string_lossy().into_owned(), age.unwrap_or(9999.0)));
        }
    }
    dumps.sort_by(|a, b| a.1.total_cmp(&b.1));
    let recent_dumps = dumps.iter().filter(|d| d.1 <= 30.0).count();
    if !dumps.is_empty() {
        s.fact("Файлы аварийных дампов", format!("{} (последний — {} назад)", dumps.len(), days_human(dumps[0].1)));
    }

    let Some(v) = collect(&mut s, PS, 150) else { return s };
    let bsod = v.list("bsod");
    if !bsod.is_empty() || recent_dumps > 0 {
        let n = bsod.len().max(recent_dumps);
        let f = s.add(Sev::Critical, format!("Синие экраны (BSOD) за последние 30 дней: {n}"));
        f.list(bsod.iter().map(|b| format!("{} — {}", b.s("t").unwrap_or_default(), b.s("msg").unwrap_or_default())).collect::<Vec<_>>(), 5);
        f.detail(format!(r"Дампы памяти лежат в {root}\Minidump — их можно открыть в WinDbg или BlueScreenView."));
        f.fix("Частые причины: драйверы (видеокарта, сеть, антивирус), перегрев, оперативная память. Обновите драйверы и проверьте память: Win+R → mdsched.");
    }

    let kp = str_list(&v, "kp");
    if !kp.is_empty() {
        let sev = if kp.len() >= 3 { Sev::Critical } else { Sev::Warning };
        s.add(sev, format!("Неожиданные выключения/перезагрузки за 30 дней: {}", kp.len()))
            .list(kp, 6)
            .fix("Компьютер выключался без корректного завершения: зависание, перегрев, проблемы с блоком питания или пропадание электричества. Если это не отключения света — проверьте температуры и питание.");
    }

    let whea = v.list("whea");
    if !whea.is_empty() {
        let fatal = whea.iter().any(|w| w.n("lvl").unwrap_or(4.0) <= 2.0);
        s.add(if fatal { Sev::Critical } else { Sev::Warning }, format!("Аппаратные ошибки (WHEA) за 30 дней: {}", whea.len()))
            .list(whea.iter().map(|w| format!("{} — {}", w.s("t").unwrap_or_default(), w.s("msg").unwrap_or_default())).collect::<Vec<_>>(), 5)
            .fix("Сигнал о неполадках процессора, памяти, PCIe или разгона. Верните настройки BIOS по умолчанию (отключите разгон/XMP для проверки), обновите BIOS.");
    }

    let disk = v.list("disk");
    if !disk.is_empty() {
        let errors = disk.iter().any(|d| d.n("lvl").unwrap_or(4.0) <= 2.0);
        s.add(if errors { Sev::Critical } else { Sev::Warning }, format!("Ошибки диска и файловой системы за 30 дней: {}", disk.len()))
            .list(disk.iter().map(|d| format!("{} {} (код {}) — {}", d.s("t").unwrap_or_default(), d.s("src").unwrap_or_default(), d.s("id").unwrap_or_default(), d.s("msg").unwrap_or_default())).collect::<Vec<_>>(), 5)
            .fix("Сделайте резервную копию, проверьте диск (chkdsk C: /scan) и его SMART-состояние. Проверьте кабели SATA.");
    }

    let crashes: Vec<String> = v
        .list("crash")
        .into_iter()
        .filter(|c| c.n("count").unwrap_or(0.0) >= 3.0)
        .map(|c| {
            let module = c.s("mod").map(|m| format!(", сбойный модуль {m}")).unwrap_or_default();
            format!("{} — {} раз{module}", c.s("app").unwrap_or_default(), c.n("count").unwrap_or(0.0) as i64)
        })
        .collect();
    if !crashes.is_empty() {
        s.add(Sev::Warning, "Программы, которые регулярно аварийно закрываются (за 7 дней)")
            .list(crashes, 8)
            .fix("Обновите или переустановите эти программы. Если сбойный модуль — системная библиотека (ntdll.dll, ucrtbase.dll), выполните sfc /scannow.");
    }
    let hangs: Vec<String> = v
        .list("hang")
        .into_iter()
        .filter(|c| c.n("count").unwrap_or(0.0) >= 3.0)
        .map(|c| format!("{} — {} раз", c.s("app").unwrap_or_default(), c.n("count").unwrap_or(0.0) as i64))
        .collect();
    if !hangs.is_empty() {
        s.add(Sev::Info, "Программы, которые часто зависают (за 7 дней)").list(hangs, 8).fix("Обновите их; при зависаниях многих программ сразу проверьте свободную память и диск.");
    }

    if s.errors.is_empty() {
        let total = v.n("sys_total").unwrap_or(0.0) as i64;
        s.fact("Ошибок в системном журнале за 7 дней", total.to_string());
    }
    let top: Vec<String> = v
        .list("top")
        .into_iter()
        .map(|t| format!("{} ×{} (код {}, последняя {}) — {}", t.s("src").unwrap_or_default(), t.n("count").unwrap_or(0.0) as i64, t.s("id").unwrap_or_default(), t.s("last").unwrap_or_default(), trunc(&t.s("msg").unwrap_or_default(), 140)))
        .collect();
    if !top.is_empty() {
        s.appendix.push(("Частые ошибки системного журнала за 7 дней".into(), top));
    }
    if s.errors.is_empty() && s.worst().is_none_or(|w| w > Sev::Warning) {
        s.ok(format!("Серьёзных сбоев системы не зафиксировано{}", admin_note(ctx)));
    }
    s
}
