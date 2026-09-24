use super::collect;
use crate::model::*;
use crate::text::days_human;

pub const TITLE: &str = "Обновления Windows";

const PS: &str = r##"
$rb = @()
if (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending') { $rb += 'CBS' }
if (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired') { $rb += 'WU' }
if ((Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager').PendingFileRenameOperations) { $rb += 'PFRO' }
$au = (New-Object -ComObject Microsoft.Update.AutoUpdate).Results
$hf = Get-HotFix | Where-Object { $_.InstalledOn } | Sort-Object InstalledOn -Descending | Select-Object -First 1
$wu = Get-CimInstance Win32_Service -Filter "Name='wuauserv'"
$pol = Get-ItemProperty 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU'
$ux = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings'
$pause = $null
if ($ux.PauseUpdatesExpiryTime) { $p = [datetime]$ux.PauseUpdatesExpiryTime; if ($p -gt (Get-Date)) { $pause = $p.ToString('dd.MM.yyyy') } }
$r = [ordered]@{
  reboot = @($rb); last_install = (D $au.LastInstallationSuccessDate); last_search = (D $au.LastSearchSuccessDate)
  hotfix = $hf.HotFixID; hotfix_days = (D $hf.InstalledOn); wu_mode = $wu.StartMode; wu_state = $wu.State
  no_auto = $pol.NoAutoUpdate; pause = $pause
}
"##;

pub fn run(ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let Some(v) = collect(&mut s, PS, 90) else { return s };
    let valid = |d: Option<f64>| d.filter(|x| *x >= 0.0 && *x < 9000.0);

    let install = valid(v.n("last_install"));
    let hotfix = valid(v.n("hotfix_days"));
    let newest = match (install, hotfix) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    if let Some(d) = install {
        s.fact("Последняя установка обновлений", format!("{} назад", days_human(d)));
    }
    if let (Some(id), Some(d)) = (v.s("hotfix"), hotfix) {
        s.fact("Последнее исправление", format!("{id}, {} назад", days_human(d)));
    }
    if let Some(d) = valid(v.n("last_search")) {
        s.fact("Последний поиск обновлений", format!("{} назад", days_human(d)));
    }

    let fix = "Параметры → Центр обновления Windows → «Проверить наличие обновлений», установите всё и перезагрузитесь.";
    match newest {
        Some(d) if d > 90.0 => {
            s.add(Sev::Critical, format!("Windows не обновлялась {}", days_human(d))).fix(fix);
        }
        Some(d) if d > 45.0 => {
            s.add(Sev::Warning, format!("Windows не обновлялась {}", days_human(d))).fix(fix);
        }
        Some(_) => s.ok("Обновления устанавливаются регулярно"),
        None => {}
    }

    if v.s("wu_mode").as_deref() == Some("Disabled") {
        s.add(Sev::Critical, "Служба «Центр обновления Windows» отключена")
            .fix("Win+R → services.msc → «Центр обновления Windows» → тип запуска «Вручную». Отключённые обновления — главная причина заражений.");
    }
    if v.n("no_auto").unwrap_or(0.0) >= 1.0 {
        s.add(Sev::Warning, "Автоматические обновления отключены групповой политикой")
            .fix("gpedit.msc → Конфигурация компьютера → Административные шаблоны → Компоненты Windows → Центр обновления Windows → «Настройка автоматического обновления» → «Не задано».");
    }
    if let Some(p) = v.s("pause") {
        s.add(Sev::Info, format!("Обновления приостановлены до {p}")).fix("Возобновить можно в Параметры → Центр обновления Windows.");
    }

    let rb = str_list(&v, "reboot");
    if rb.iter().any(|r| r == "CBS" || r == "WU") {
        s.add(Sev::Warning, "Обновления ждут перезагрузки").fix("Перезагрузите компьютер, чтобы завершить установку обновлений.");
    } else if rb.iter().any(|r| r == "PFRO") {
        s.add(Sev::Info, "Есть файлы, которые будут заменены при следующей перезагрузке")
            .fix("Обычно это остатки установки программ; перезагрузка завершит операцию.");
    }
    if !ctx.admin && s.findings.is_empty() {
        s.error("часть сведений об обновлениях доступна только администратору");
    }
    s
}
