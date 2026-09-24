use super::{admin_note, collect};
use crate::model::*;

pub const TITLE: &str = "Безопасность";

const PS: &str = r##"
$mp = Get-MpComputerStatus
$av = @(Get-CimInstance -Namespace root\SecurityCenter2 -ClassName AntiVirusProduct | ForEach-Object { [ordered]@{ name = $_.displayName; state = [int]$_.productState } })
$fwp = @(Get-CimInstance -Namespace root\SecurityCenter2 -ClassName FirewallProduct | ForEach-Object { $_.displayName })
$fw = @(Get-NetFirewallProfile | ForEach-Object { [ordered]@{ name = "$($_.Name)"; on = ("$($_.Enabled)" -eq 'True') } })
$sb = $null; $sb_err = $null
try { $sb = Confirm-SecureBootUEFI -ErrorAction Stop } catch { $sb_err = $_.Exception.GetType().Name }
$tpm = $null
try { $t = Get-Tpm -ErrorAction Stop; $tpm = [ordered]@{ present = $t.TpmPresent; ready = $t.TpmReady } } catch {}
$threats = @(Get-MpThreat | Where-Object { $_.IsActive -or $_.DidThreatExecute } | ForEach-Object { [ordered]@{ name = $_.ThreatName; active = $_.IsActive } })
$r = [ordered]@{
  def_av = $mp.AntivirusEnabled; def_rtp = $mp.RealTimeProtectionEnabled; def_sig = $mp.AntivirusSignatureAge
  def_quick = $mp.QuickScanAge; def_tamper = $mp.IsTamperProtected; def_mode = "$($mp.AMRunningMode)"
  av = $av; fw_products = $fwp; fw = $fw
  uac = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System').EnableLUA
  secureboot = $sb; secureboot_err = $sb_err; tpm = $tpm
  smb1 = (Get-SmbServerConfiguration).EnableSMB1Protocol
  guest = (Get-LocalUser | Where-Object { $_.SID.Value -like '*-501' }).Enabled
  rdp_deny = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server').fDenyTSConnections
  threats = $threats
}
"##;

fn is_defender(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("defender") || n.contains("windows security") || n.contains("безопасность windows")
}

pub fn run(ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let Some(v) = collect(&mut s, PS, 90) else { return s };

    // productState bit 0x1000 = enabled, 0x10 = signatures out of date.
    let mut third_party = Vec::new();
    for a in v.list("av") {
        let name = a.s("name").unwrap_or_default();
        let state = a.n("state").unwrap_or(0.0) as u32;
        let on = state & 0x1000 != 0;
        s.fact("Антивирус", format!("{name} — {}", if on { "включён" } else { "выключен" }));
        if !is_defender(&name) && on {
            third_party.push(name);
        }
    }

    let def_av = v.b("def_av");
    let def_rtp = v.b("def_rtp");
    let protected = !third_party.is_empty() || (def_av == Some(true) && def_rtp != Some(false));
    if !third_party.is_empty() {
        s.ok(format!("Антивирусная защита активна: {}", third_party.join(", ")));
    } else if def_av.is_none() && v.list("av").is_empty() {
        s.error(format!("не удалось получить состояние антивируса{}", admin_note(ctx)));
    } else if !protected {
        s.add(Sev::Critical, "Компьютер не защищён антивирусом")
            .detail(if def_rtp == Some(false) { "Защита в реальном времени Microsoft Defender выключена." } else { "Microsoft Defender выключен, другого антивируса не найдено." })
            .fix("Откройте «Безопасность Windows» → «Защита от вирусов и угроз» и включите защиту в реальном времени.");
    } else {
        s.ok("Microsoft Defender включён, защита в реальном времени работает");
        if let Some(age) = v.n("def_sig") {
            if age > 7.0 && age < 10000.0 {
                s.add(Sev::Warning, format!("Антивирусные базы Defender устарели на {} дн.", age as i64))
                    .fix("«Безопасность Windows» → «Защита от вирусов и угроз» → «Обновления защиты» → «Проверить наличие обновлений».");
            }
        }
        if v.b("def_tamper") == Some(false) {
            s.add(Sev::Info, "Защита от подделки (Tamper Protection) выключена")
                .fix("«Безопасность Windows» → «Параметры защиты от вирусов и угроз» → «Защита от подделки» — включить.");
        }
    }

    let threats: Vec<String> = v
        .list("threats")
        .into_iter()
        .map(|t| format!("{}{}", t.s("name").unwrap_or_default(), if t.b("active") == Some(true) { " — АКТИВНА" } else { "" }))
        .collect();
    if !threats.is_empty() {
        s.add(Sev::Critical, format!("Defender обнаружил угрозы: {}", threats.len()))
            .list(threats, 10)
            .fix("«Безопасность Windows» → «Журнал защиты»: удалите угрозы и выполните полную проверку. Для надёжности — автономная проверка Microsoft Defender.");
    }

    let fw_products: Vec<String> = str_list(&v, "fw_products").into_iter().filter(|n| !is_defender(n)).collect();
    let off: Vec<String> = v.list("fw").into_iter().filter(|p| p.b("on") == Some(false)).filter_map(|p| p.s("name")).collect();
    if !v.list("fw").is_empty() {
        if off.is_empty() {
            s.ok("Брандмауэр Windows включён для всех сетей");
        } else if !fw_products.is_empty() {
            s.add(Sev::Info, format!("Брандмауэр Windows выключен ({}), но работает другой: {}", off.join(", "), fw_products.join(", ")));
        } else {
            s.add(Sev::Warning, format!("Брандмауэр Windows выключен для профилей: {}", off.join(", ")))
                .fix("«Безопасность Windows» → «Брандмауэр и защита сети» → включите для всех сетей.");
        }
    }

    if v.n("uac") == Some(0.0) {
        s.add(Sev::Warning, "Контроль учётных записей (UAC) отключён")
            .fix("Win+R → UserAccountControlSettings → верните ползунок на уровень по умолчанию и перезагрузитесь.");
    }
    if v.b("smb1") == Some(true) {
        s.add(Sev::Warning, "Включён устаревший протокол SMB1 (через него распространялся WannaCry)")
            .fix("Win+R → optionalfeatures → снимите «Поддержка общего доступа к файлам SMB 1.0/CIFS».");
    }
    if v.b("guest") == Some(true) {
        s.add(Sev::Warning, "Включена учётная запись «Гость»").fix("Отключите её: net user Гость /active:no (в командной строке администратора).");
    }
    if v.n("rdp_deny") == Some(0.0) {
        s.add(Sev::Info, "Разрешено удалённое подключение к рабочему столу (RDP)")
            .fix("Если вы им не пользуетесь — Параметры → Система → Удалённый рабочий стол → Выкл.");
    }
    match (v.b("secureboot"), v.s("secureboot_err").as_deref()) {
        (Some(true), _) => s.fact("Безопасная загрузка", "включена"),
        (Some(false), _) => {
            s.add(Sev::Info, "Безопасная загрузка (Secure Boot) выключена")
                .fix("Включается в настройках UEFI/BIOS. Нужна для Windows 11 и защищает от буткитов.");
        }
        (_, Some("PlatformNotSupportedException")) => s.fact("Безопасная загрузка", "не поддерживается (устаревший режим BIOS)"),
        _ => {}
    }
    if let Some(t) = v.get("tpm").filter(|t| !t.is_null()) {
        let state = match (t.b("present"), t.b("ready")) {
            (Some(true), Some(true)) => "есть, готов",
            (Some(true), _) => "есть, не готов",
            _ => "не обнаружен",
        };
        s.fact("TPM", state);
    }
    s
}
