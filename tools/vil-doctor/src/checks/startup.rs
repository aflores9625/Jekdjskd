use super::collect;
use crate::model::*;
use crate::paths::{fs_exists, resolve_command, resolve_path, system_env, Env, Res};

pub const TITLE: &str = "Автозагрузка";

const PS: &str = r##"
$disabled = @{}
foreach ($k in 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run', 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run', 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32', 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder') {
  $i = Get-Item -LiteralPath $k
  if ($i) { foreach ($n in $i.GetValueNames()) { $b = $i.GetValue($n); if ($b -and ($b[0] -band 1)) { $disabled[$n] = $true } } }
}
$run = @()
foreach ($k in 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run', 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run', 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce', 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce') {
  $i = Get-Item -LiteralPath $k
  if ($i) { foreach ($n in $i.GetValueNames()) { if ($n) { $run += [ordered]@{ where = ($k -replace '^(HK..):.*\\', '$1\'); name = $n; cmd = [string]$i.GetValue($n); off = [bool]$disabled[$n] } } } }
}
$sh = New-Object -ComObject WScript.Shell
$folder = @()
foreach ($f in 'Startup', 'CommonStartup') {
  $d = [Environment]::GetFolderPath($f)
  if ($d -and (Test-Path -LiteralPath $d)) { Get-ChildItem -LiteralPath $d -File | Where-Object { $_.Name -ne 'desktop.ini' } | ForEach-Object {
    $t = if ($_.Extension -eq '.lnk') { $sh.CreateShortcut($_.FullName).TargetPath } else { $_.FullName }
    $folder += [ordered]@{ name = $_.Name; target = $t; off = [bool]$disabled[$_.Name] } } }
}
$wl = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
$wlu = Get-ItemProperty 'HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
$ifeo = @(Get-ChildItem 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options' | ForEach-Object {
  $d = (Get-ItemProperty -LiteralPath $_.PSPath).Debugger; if ($d) { [ordered]@{ exe = $_.PSChildName; dbg = [string]$d } } })
$r = [ordered]@{ run = $run; folder = $folder; shell = $wl.Shell; userinit = $wl.Userinit; user_shell = $wlu.Shell; ifeo = $ifeo }
"##;

pub fn winlogon_problems(shell: Option<&str>, userinit: Option<&str>, user_shell: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(sh) = shell {
        let t = sh.trim().trim_end_matches(',').to_ascii_lowercase();
        if t != "explorer.exe" && !t.ends_with(r"\explorer.exe") {
            out.push(format!("Shell = «{sh}» (должно быть explorer.exe)"));
        }
    }
    if let Some(ui) = userinit {
        let parts: Vec<&str> = ui.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
        let ok = parts.len() == 1 && parts[0].to_ascii_lowercase().ends_with("userinit.exe");
        if !ok {
            out.push(format!("Userinit = «{ui}» (должно быть C:\\Windows\\system32\\userinit.exe,)"));
        }
    }
    if let Some(us) = user_shell.filter(|s| !s.trim().is_empty()) {
        out.push(format!("Для пользователя задана своя оболочка: «{us}»"));
    }
    out
}

pub fn run(_ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let Some(v) = collect(&mut s, PS, 60) else { return s };
    let (var, search) = system_env();
    let env = Env { var: &*var, exists: &fs_exists, search };

    let mut active = Vec::new();
    let mut missing = Vec::new();
    let mut listing = Vec::new();
    for e in v.list("run") {
        let name = e.s("name").unwrap_or_default();
        let cmd = e.s("cmd").unwrap_or_default();
        let off = e.b("off") == Some(true);
        listing.push(format!("{}{} [{}] {}", name, if off { " (отключено)" } else { "" }, e.s("where").unwrap_or_default(), cmd));
        if let Res::Missing(m) = resolve_command(&cmd, &env) {
            missing.push(format!("{name} → {m}{}", if off { " (отключено)" } else { "" }));
        } else if !off {
            active.push(name);
        }
    }
    for e in v.list("folder") {
        let name = e.s("name").unwrap_or_default();
        let off = e.b("off") == Some(true);
        let target = e.s("target").unwrap_or_default();
        listing.push(format!("{}{} [папка «Автозагрузка»] {}", name, if off { " (отключено)" } else { "" }, target));
        if let Res::Missing(m) = resolve_path(&target, &env) {
            missing.push(format!("{name} → {m}"));
        } else if !off {
            active.push(name);
        }
    }
    s.fact("Программ в автозагрузке", active.len().to_string());

    let any_missing = !missing.is_empty();
    if any_missing {
        s.add(Sev::Warning, format!("Автозагрузка ссылается на несуществующие файлы: {}", missing.len()))
            .detail("При входе в систему это вызывает ошибки или пустые окна.")
            .list(missing, 12)
            .fix("Диспетчер задач (Ctrl+Shift+Esc) → «Автозагрузка приложений» — отключите эти записи; полностью удалить можно через Autoruns от Microsoft.");
    }
    if active.len() > 15 {
        s.add(Sev::Warning, format!("В автозагрузке много программ: {}", active.len()))
            .fix("Отключите ненужные в Диспетчере задач → «Автозагрузка приложений» — компьютер будет включаться быстрее.");
    } else if active.len() > 8 {
        s.add(Sev::Info, format!("В автозагрузке {} программ", active.len())).fix("Проверьте список ниже и отключите то, чем не пользуетесь каждый день.");
    } else if !any_missing && v.has("run") && s.errors.is_empty() {
        s.ok("Все программы автозагрузки на месте");
    }

    let wl = winlogon_problems(v.s("shell").as_deref(), v.s("userinit").as_deref(), v.s("user_shell").as_deref());
    if !wl.is_empty() {
        s.add(Sev::Critical, "Изменены системные параметры входа в Windows (Winlogon)")
            .detail("Так часто закрепляются вирусы и майнеры.")
            .list(wl, 5)
            .fix("Выполните полную и автономную проверку Microsoft Defender, затем восстановите значения в HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Winlogon.");
    }
    let ifeo: Vec<String> = v.list("ifeo").into_iter().map(|x| format!("{} → {}", x.s("exe").unwrap_or_default(), x.s("dbg").unwrap_or_default())).collect();
    if !ifeo.is_empty() {
        s.add(Sev::Warning, "Запуск некоторых программ перехвачен (Image File Execution Options → Debugger)")
            .detail("Так иногда заменяют Диспетчер задач (Process Explorer), но этим же пользуются вирусы, чтобы блокировать антивирусы.")
            .list(ifeo, 8)
            .fix("Если вы не настраивали это сами — удалите значение Debugger в HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options\\<программа>.");
    }
    if !listing.is_empty() {
        s.appendix.push(("Автозагрузка — все записи".into(), listing));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winlogon() {
        assert!(winlogon_problems(Some("explorer.exe"), Some(r"C:\Windows\system32\userinit.exe,"), None).is_empty());
        assert_eq!(winlogon_problems(Some("explorer.exe, miner.exe"), None, None).len(), 1);
        assert_eq!(winlogon_problems(None, Some(r"C:\Windows\system32\userinit.exe,C:\x\evil.exe,"), None).len(), 1);
        assert_eq!(winlogon_problems(None, None, Some("cmd.exe")).len(), 1);
    }
}
