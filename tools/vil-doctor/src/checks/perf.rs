use super::{collect, sysroot};
use crate::model::*;
use crate::text::{gb, num};
use std::time::Instant;

pub const TITLE: &str = "Производительность и мусор";

const PS: &str = r##"
$pf = @(Get-CimInstance Win32_PageFileUsage | ForEach-Object { [ordered]@{ name = $_.Name; alloc = $_.AllocatedBaseSize; peak = $_.PeakUsage } })
$auto = (Get-CimInstance Win32_ComputerSystem).AutomaticManagedPagefile
$plan = (Get-CimInstance -Namespace root\cimv2\power -ClassName Win32_PowerPlan -Filter 'IsActive=True').ElementName
$top = @(Get-Process | Sort-Object WorkingSet64 -Descending | Select-Object -First 8 | ForEach-Object { [ordered]@{ name = $_.ProcessName; mb = [math]::Round($_.WorkingSet64 / 1MB) } })
$os = Get-CimInstance Win32_OperatingSystem
$fast = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power').HiberbootEnabled
$r = [ordered]@{ pagefile = $pf; auto = $auto; plan = $plan; top = $top; ram_mb = [math]::Round($os.TotalVisibleMemorySize / 1024); fast = $fast }
"##;

/// Folder size with an entry cap so a huge temp dir can't stall the run.
fn dir_size(path: &str, cap: usize) -> (u64, usize, bool) {
    let mut stack = vec![std::path::PathBuf::from(path)];
    let (mut total, mut count) = (0u64, 0usize);
    let started = Instant::now();
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            count += 1;
            if count > cap || started.elapsed().as_secs() > 20 {
                return (total, count, true);
            }
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(e.path());
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    (total, count, false)
}

pub fn run(_ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let root = sysroot();

    let user_temp = std::env::var("TEMP").unwrap_or_default();
    let mut junk = 0f64;
    for (label, path) in [
        ("Временные файлы пользователя", user_temp.clone()),
        ("Временные файлы Windows", format!(r"{root}\Temp")),
        ("Кэш загрузок обновлений", format!(r"{root}\SoftwareDistribution\Download")),
    ] {
        if path.is_empty() {
            continue;
        }
        let (bytes, _, capped) = dir_size(&path, 400_000);
        let g = bytes as f64 / 1_073_741_824.0;
        junk += g;
        s.fact(label, format!("{}{}", if capped { "более " } else { "" }, gb(g)));
    }
    if junk >= 10.0 {
        s.add(Sev::Warning, format!("Накопилось много временных файлов: {}", gb(junk)))
            .fix("Параметры → Система → Память → Временные файлы → отметьте «Временные файлы» и «Очистка обновлений Windows» → «Удалить файлы». Или Win+R → cleanmgr.");
    } else if junk >= 3.0 {
        s.add(Sev::Info, format!("Временные файлы занимают {}", gb(junk)))
            .fix("Их можно безопасно удалить: Параметры → Система → Память → Временные файлы.");
    }

    let Some(v) = collect(&mut s, PS, 60) else { return s };
    s.fact("Схема электропитания", v.s("plan").unwrap_or_default());
    let ram = v.n("ram_mb").unwrap_or(0.0);
    let pf = v.list("pagefile");
    let pf_total: f64 = pf.iter().filter_map(|p| p.n("alloc")).sum();
    if pf.is_empty() && v.has("pagefile") {
        let sev = if ram > 0.0 && ram < 16_000.0 { Sev::Warning } else { Sev::Info };
        s.add(sev, "Файл подкачки отключён")
            .detail("Без него программы могут внезапно закрываться при нехватке памяти, а после синего экрана не сохраняется дамп.")
            .fix("Win+R → sysdm.cpl → Дополнительно → Быстродействие → Параметры → Дополнительно → Виртуальная память → «Автоматически выбирать объём файла подкачки».");
    } else if !pf.is_empty() {
        let auto = if v.b("auto") == Some(true) { ", автоматически" } else { "" };
        s.fact("Файл подкачки", format!("{} ГБ{auto}", num(pf_total / 1024.0, 1)));
        let peak: f64 = pf.iter().filter_map(|p| p.n("peak")).sum();
        if pf_total > 0.0 && peak / pf_total > 0.9 {
            s.add(Sev::Warning, "Файл подкачки почти полностью использовался")
                .fix("Памяти не хватает: закройте тяжёлые программы, включите автоматический размер файла подкачки или добавьте ОЗУ.");
        }
    }
    if v.n("fast").unwrap_or(0.0) >= 1.0 {
        s.fact("Быстрый запуск", "включён (выключение ≠ полная перезагрузка)");
    }

    let top: Vec<String> = v.list("top").into_iter().map(|p| format!("{} — {} МБ", p.s("name").unwrap_or_default(), p.n("mb").unwrap_or(0.0) as i64)).collect();
    if !top.is_empty() {
        s.fact("Больше всего памяти занимают", top.iter().take(5).cloned().collect::<Vec<_>>().join("; "));
    }
    if s.errors.is_empty() && s.worst().is_none_or(|w| w > Sev::Warning) {
        s.ok("Серьёзных проблем с производительностью не найдено");
    }
    s
}
