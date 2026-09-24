use super::collect;
use crate::model::*;
use crate::text::{days_human, num, Date};

pub const TITLE: &str = "Система";

const PS: &str = r##"
$os = Get-CimInstance Win32_OperatingSystem
$cs = Get-CimInstance Win32_ComputerSystem
$cpu = @(Get-CimInstance Win32_Processor)[0]
$bios = Get-CimInstance Win32_BIOS
$cv = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$winApp = @('55c92734', 'd682', '4d71', '983e', 'd6ec3f16059f') -join '-'
$lic = Get-CimInstance SoftwareLicensingProduct -Filter "ApplicationID='$winApp' AND PartialProductKey IS NOT NULL" | Select-Object -First 1
$stab = Get-CimInstance Win32_ReliabilityStabilityMetrics | Sort-Object TimeGenerated -Descending | Select-Object -First 1
$r = [ordered]@{
  caption = $os.Caption; build = $os.BuildNumber; ubr = $cv.UBR; display = $cv.DisplayVersion; edition = $cv.EditionID
  arch = $os.OSArchitecture; install_days = (D $os.InstallDate); uptime_days = (D $os.LastBootUpTime)
  ram_total_mb = [math]::Round($os.TotalVisibleMemorySize / 1024); ram_free_mb = [math]::Round($os.FreePhysicalMemory / 1024)
  maker = $cs.Manufacturer; model = $cs.Model; cpu = $cpu.Name; cores = $cpu.NumberOfCores; threads = $cpu.NumberOfLogicalProcessors
  bios = ('{0} {1}' -f $bios.Manufacturer, $bios.SMBIOSBIOSVersion).Trim(); bios_days = (D $bios.ReleaseDate)
  license = $(if ($lic) { [int]$lic.LicenseStatus } else { $null })
  stability = $(if ($stab) { [math]::Round($stab.SystemStabilityIndex, 1) } else { $null })
  ps = $PSVersionTable.PSVersion.ToString()
}
"##;

pub fn run(ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let Some(v) = collect(&mut s, PS, 90) else { return s };

    let build = v.n("build").unwrap_or(0.0) as u32;
    let caption = v.s("caption").unwrap_or_else(|| "Windows".into());
    let mut os = caption.replace("Microsoft ", "");
    if let Some(d) = v.s("display") {
        os.push_str(&format!(" {d}"));
    }
    if build > 0 {
        os.push_str(&format!(" (сборка {build}{})", v.s("ubr").map(|u| format!(".{u}")).unwrap_or_default()));
    }
    s.fact("Windows", os);
    s.fact("Разрядность", v.s("arch").unwrap_or_default());
    let hw = [v.s("maker"), v.s("model")].into_iter().flatten().collect::<Vec<_>>().join(" ");
    s.fact("Компьютер", hw);
    if let Some(cpu) = v.s("cpu") {
        let cores = match (v.n("cores"), v.n("threads")) {
            (Some(c), Some(t)) => format!(" — {} ядер / {} потоков", c as u32, t as u32),
            _ => String::new(),
        };
        s.fact("Процессор", format!("{}{cores}", cpu.split_whitespace().collect::<Vec<_>>().join(" ")));
    }
    let total = v.n("ram_total_mb").unwrap_or(0.0);
    let free = v.n("ram_free_mb").unwrap_or(0.0);
    if total > 0.0 {
        s.fact("Оперативная память", format!("{} ГБ, свободно {} ГБ", num(total / 1024.0, 1), num(free / 1024.0, 1)));
    }
    s.fact("BIOS/UEFI", v.s("bios").unwrap_or_default());
    if let Some(d) = v.n("install_days") {
        s.fact("Windows установлена", format!("{} назад", days_human(d)));
    }
    if let Some(d) = v.n("uptime_days") {
        s.fact("Без перезагрузки", days_human(d));
    }
    s.fact("PowerShell", v.s("ps").unwrap_or_default());

    let edition = v.s("edition").unwrap_or_default();
    if let Some((sev, title, fix)) = support_status(build, &edition, &caption, ctx.today) {
        let f = s.add(sev, title);
        if let Some(fix) = fix {
            f.fix(fix);
        }
    }

    match v.n("license").map(|x| x as i64) {
        Some(1) => s.ok("Windows активирована"),
        Some(code) => {
            let state = match code {
                0 => "не лицензирована",
                2..=4 => "работает в льготном периоде",
                5 => "требуется активация (уведомление)",
                6 => "продлённый льготный период",
                _ => "неизвестное состояние",
            };
            s.add(Sev::Warning, format!("Windows не активирована: {state}"))
                .fix("Параметры → Система → Активация. Без активации часть настроек недоступна.");
        }
        None => {}
    }

    if let Some(up) = v.n("uptime_days") {
        if up > 30.0 {
            s.add(Sev::Warning, format!("Компьютер не перезагружался {}", days_human(up)))
                .fix("Перезагрузите компьютер через Пуск → Перезагрузка. «Завершение работы» при включённом быстром запуске не сбрасывает систему полностью.");
        } else if up > 7.0 {
            s.add(Sev::Info, format!("Компьютер не перезагружался {}", days_human(up)))
                .fix("Перезагрузка раз в неделю применяет обновления и освобождает память. Используйте Пуск → Перезагрузка.");
        }
    }

    if total > 0.0 {
        let used = 100.0 * (1.0 - free / total);
        if used >= 90.0 {
            s.add(Sev::Warning, format!("Оперативная память занята на {}%", used.round()))
                .fix("Закройте лишние программы и вкладки браузера; самые «тяжёлые» процессы перечислены в разделе «Производительность».");
        }
        let gb = total / 1024.0;
        if gb < 3.8 {
            s.add(Sev::Warning, format!("Мало оперативной памяти: {} ГБ", num(gb, 1)))
                .fix("Для Windows 10/11 комфортно от 8 ГБ. Рассмотрите увеличение объёма памяти.");
        } else if gb < 7.5 {
            s.add(Sev::Info, format!("Оперативной памяти {} ГБ — впритык для современной Windows", num(gb, 1)))
                .fix("Если компьютер тормозит при работе браузера и программ, 16 ГБ заметно улучшат отзывчивость.");
        }
    }

    if let Some(st) = v.n("stability") {
        s.fact("Индекс стабильности", format!("{} из 10", num(st, 1)));
        if st < 5.0 {
            s.add(Sev::Warning, format!("Низкий индекс стабильности системы: {} из 10", num(st, 1)))
                .fix("Откройте «Монитор стабильности системы» (Win+R → perfmon /rel), чтобы увидеть, какие сбои повторяются.");
        }
    }
    s
}

struct Release {
    build: u32,
    name: &'static str,
    consumer: Date,
    enterprise: Date,
}

const WIN11: [Release; 5] = [
    Release { build: 22000, name: "21H2", consumer: Date::new(2023, 10, 10), enterprise: Date::new(2024, 10, 8) },
    Release { build: 22621, name: "22H2", consumer: Date::new(2024, 10, 8), enterprise: Date::new(2025, 10, 14) },
    Release { build: 22631, name: "23H2", consumer: Date::new(2025, 11, 11), enterprise: Date::new(2026, 11, 10) },
    Release { build: 26100, name: "24H2", consumer: Date::new(2026, 10, 13), enterprise: Date::new(2027, 10, 12) },
    Release { build: 26200, name: "25H2", consumer: Date::new(2027, 10, 12), enterprise: Date::new(2028, 10, 10) },
];

const WIN10_EOL: Date = Date::new(2025, 10, 14);

/// Whether this Windows release still receives security updates.
pub fn support_status(build: u32, edition: &str, caption: &str, today: Date) -> Option<(Sev, String, Option<String>)> {
    if build == 0 {
        return None;
    }
    let ed = edition.to_ascii_lowercase();
    if ed.ends_with("s") && ed.contains("enterprise") || caption.contains("LTSC") || caption.contains("LTSB") {
        return None; // LTSC editions follow a separate, longer lifecycle
    }
    let upgrade = "Установите последнюю версию через Параметры → Центр обновления Windows (или «Помощник по установке Windows 11»).".to_string();
    if build < 10240 {
        return Some((Sev::Critical, "Эта версия Windows давно не поддерживается и не получает исправлений безопасности".into(), Some(upgrade)));
    }
    if build < 22000 {
        if today > WIN10_EOL {
            return Some((
                Sev::Warning,
                format!("Windows 10 больше не получает бесплатных обновлений безопасности (поддержка закончилась {})", WIN10_EOL.ru()),
                Some("Перейдите на Windows 11, если компьютер подходит, или подключите программу расширенных обновлений (ESU).".into()),
            ));
        }
        if build < 19045 {
            return Some((Sev::Warning, "Устаревшая версия Windows 10".into(), Some(upgrade)));
        }
        return Some((Sev::Ok, "Версия Windows поддерживается".into(), None));
    }
    let rel = WIN11.iter().rev().find(|r| build >= r.build)?;
    if build >= 26300 {
        return Some((Sev::Ok, "Версия Windows 11 новее известных программе — поддерживается".into(), None));
    }
    let business = ed.contains("enterprise") || ed.contains("education");
    let eol = if business { rel.enterprise } else { rel.consumer };
    let left = eol.days() - today.days();
    if left < 0 {
        Some((Sev::Warning, format!("Windows 11 {} больше не поддерживается (с {})", rel.name, eol.ru()), Some(upgrade)))
    } else if left <= 60 {
        Some((Sev::Info, format!("Поддержка Windows 11 {} заканчивается {}", rel.name, eol.ru()), Some(upgrade)))
    } else {
        Some((Sev::Ok, format!("Windows 11 {} поддерживается до {}", rel.name, eol.ru()), None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_lifecycle() {
        let today = Date::new(2026, 9, 24);
        assert_eq!(support_status(19045, "Professional", "Windows 10 Pro", today).unwrap().0, Sev::Warning);
        assert!(support_status(19044, "EnterpriseS", "Windows 10 Enterprise LTSC 2021", today).is_none());
        assert_eq!(support_status(22631, "Professional", "", today).unwrap().0, Sev::Warning);
        assert_eq!(support_status(22631, "Enterprise", "", today).unwrap().0, Sev::Info);
        assert_eq!(support_status(26100, "Core", "", today).unwrap().0, Sev::Info);
        assert_eq!(support_status(26100, "Core", "", Date::new(2026, 1, 1)).unwrap().0, Sev::Ok);
        assert_eq!(support_status(26200, "Professional", "", today).unwrap().0, Sev::Ok);
        assert_eq!(support_status(9600, "", "", today).unwrap().0, Sev::Critical);
    }
}
