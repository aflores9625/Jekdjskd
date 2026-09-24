use super::collect;
use crate::model::*;
use crate::paths::{fs_exists, resolve_command, resolve_path, system_env, Env, Res};

pub const TITLE: &str = "Битые ссылки и остатки программ";

const PS: &str = r##"
$sh = New-Object -ComObject WScript.Shell
$lnk = New-Object System.Collections.ArrayList
foreach ($f in 'Desktop', 'CommonDesktopDirectory', 'Programs', 'CommonPrograms') {
  $d = [Environment]::GetFolderPath($f)
  if ($d -and (Test-Path -LiteralPath $d)) {
    Get-ChildItem -LiteralPath $d -Filter *.lnk -Recurse -File | Select-Object -First 3000 | ForEach-Object {
      [void]$lnk.Add([ordered]@{ where = $f; file = $_.FullName; name = $_.BaseName; target = $sh.CreateShortcut($_.FullName).TargetPath }) } }
}
$un = New-Object System.Collections.ArrayList
foreach ($k in 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall', 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall') {
  Get-ChildItem -LiteralPath $k | ForEach-Object {
    $p = Get-ItemProperty -LiteralPath $_.PSPath
    if ($p.DisplayName -and -not $p.SystemComponent -and -not $p.ParentKeyName) {
      [void]$un.Add([ordered]@{ name = $p.DisplayName; ver = $p.DisplayVersion; pub = $p.Publisher; un = $p.UninstallString; loc = $p.InstallLocation; size = $p.EstimatedSize; msi = [int]$p.WindowsInstaller }) } }
}
$tasks = @(Get-ScheduledTask | Where-Object { $_.State -ne 'Disabled' } | ForEach-Object { $t = $_
  foreach ($a in @($t.Actions)) { if ($a.Execute) { [ordered]@{ name = ($t.TaskPath + $t.TaskName); exe = $a.Execute } } } })
$r = [ordered]@{ lnk = @($lnk); un = @($un); tasks = $tasks }
"##;

pub fn run(_ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let (var, search) = system_env();
    let env = Env { var: &*var, exists: &fs_exists, search };

    // PATH entries that point nowhere slow down every program launch lookup.
    if let Ok(path) = std::env::var("PATH") {
        let root = super::sysroot().to_ascii_lowercase();
        let entries: Vec<&str> = path.split(';').map(str::trim).filter(|p| !p.is_empty()).collect();
        if !entries.iter().any(|p| p.trim_end_matches('\\').to_ascii_lowercase() == format!(r"{root}\system32")) {
            s.add(Sev::Critical, "В переменной PATH нет папки System32")
                .fix(r"Win+R → sysdm.cpl → Дополнительно → Переменные среды → Path (системная): добавьте %SystemRoot%\system32 и %SystemRoot%.");
        }
        let dead: Vec<String> = entries.iter().filter(|p| matches!(resolve_path(p, &env), Res::Missing(_))).map(|p| p.to_string()).collect();
        if !dead.is_empty() {
            s.add(Sev::Info, format!("В переменной PATH есть несуществующие папки: {}", dead.len()))
                .list(dead, 10)
                .fix("Остались от удалённых программ. Удалите их: Win+R → sysdm.cpl → Дополнительно → Переменные среды → Path.");
        }
    }

    let Some(v) = collect(&mut s, PS, 150) else { return s };

    let mut dead_lnk = Vec::new();
    let lnk = v.list("lnk");
    for l in &lnk {
        let target = l.s("target").unwrap_or_default();
        if let Res::Missing(m) = resolve_path(&target, &env) {
            let place = if l.s("where").is_some_and(|w| w.contains("Desktop")) { "Рабочий стол" } else { "меню Пуск" };
            dead_lnk.push(format!("{} ({place}) → {m}", l.s("name").unwrap_or_default()));
        }
    }
    if v.has("lnk") {
        s.fact("Проверено ярлыков", lnk.len().to_string());
    }
    if dead_lnk.is_empty() && v.has("lnk") {
        s.ok("Ярлыки на рабочем столе и в меню Пуск рабочие");
    } else {
        s.add(Sev::Warning, format!("Ярлыки, ведущие к удалённым файлам: {}", dead_lnk.len()))
            .list(dead_lnk, 15)
            .fix("Удалите эти ярлыки или переустановите соответствующие программы.");
    }

    let progs = v.list("un");
    let mut broken = Vec::new();
    let mut all = Vec::new();
    for p in &progs {
        let name = p.s("name").unwrap_or_default();
        let ver = p.s("ver").map(|v| format!(" {v}")).unwrap_or_default();
        let publisher = p.s("pub").map(|x| format!(" — {x}")).unwrap_or_default();
        all.push(format!("{name}{ver}{publisher}"));
        let Some(cmd) = p.s("un") else { continue };
        if cmd.to_ascii_lowercase().contains("msiexec") {
            continue;
        }
        if let Res::Missing(m) = resolve_command(&cmd, &env) {
            let loc_gone = p.s("loc").is_some_and(|l| matches!(resolve_path(&l, &env), Res::Missing(_)));
            broken.push(format!("{name}{ver} — нет {m}{}", if loc_gone { " (папка программы тоже удалена)" } else { "" }));
        }
    }
    all.sort_by_key(|a| a.to_lowercase());
    all.dedup();
    s.fact("Установлено программ", all.len().to_string());
    if !broken.is_empty() {
        s.add(Sev::Warning, format!("Программы в списке установленных, у которых пропал деинсталлятор: {}", broken.len()))
            .detail("Программа удалена не полностью или повреждена — её нельзя штатно удалить или обновить.")
            .list(broken, 15)
            .fix("Попробуйте переустановить программу поверх и затем удалить. Для «зависших» записей Microsoft предлагает средство «Program Install and Uninstall troubleshooter».");
    }

    let (mut dead_task, mut dead_ms) = (Vec::new(), Vec::new());
    for t in v.list("tasks") {
        let exe = t.s("exe").unwrap_or_default();
        if let Res::Missing(m) = resolve_path(&exe, &env) {
            let name = t.s("name").unwrap_or_default();
            if name.starts_with(r"\Microsoft\") {
                dead_ms.push(format!("{name} → {m}"));
            } else {
                dead_task.push(format!("{name} → {m}"));
            }
        }
    }
    if !dead_task.is_empty() {
        s.add(Sev::Warning, format!("Задания планировщика запускают удалённые программы: {}", dead_task.len()))
            .list(dead_task, 12)
            .fix("Win+R → taskschd.msc → найдите задание → «Отключить» или «Удалить».");
    }
    if !dead_ms.is_empty() {
        s.add(Sev::Info, format!("Системные задания Microsoft без исполняемого файла: {}", dead_ms.len()))
            .list(dead_ms, 8)
            .fix("Обычно это компоненты, которые не установлены в вашей редакции Windows. Если их много — выполните sfc /scannow.");
    }
    if !all.is_empty() {
        s.appendix.push(("Установленные программы".into(), all));
    }
    s
}
