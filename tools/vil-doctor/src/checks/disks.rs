use super::collect;
use crate::model::*;
use crate::text::{gb, num};

pub const TITLE: &str = "Диски и накопители";

const PS: &str = r##"
$ld = @(Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' | ForEach-Object {
  [ordered]@{ id = $_.DeviceID; label = $_.VolumeName; fs = $_.FileSystem; size = [math]::Round($_.Size / 1GB, 2); free = [math]::Round($_.FreeSpace / 1GB, 2) } })
$dirty = @(Get-CimInstance Win32_Volume | Where-Object { $_.DriveLetter -and $_.DirtyBitSet } | ForEach-Object { $_.DriveLetter })
$pd = @(Get-PhysicalDisk | ForEach-Object {
  $rc = $_ | Get-StorageReliabilityCounter
  [ordered]@{ name = $_.FriendlyName; media = "$($_.MediaType)"; bus = "$($_.BusType)"; size = [math]::Round($_.Size / 1GB)
    health = "$($_.HealthStatus)"; op = (@($_.OperationalStatus) | ForEach-Object { "$_" }) -join ', '
    wear = $rc.Wear; temp = $rc.Temperature; rerr = $rc.ReadErrorsUncorrected; werr = $rc.WriteErrorsUncorrected; hours = $rc.PowerOnHours } })
$smart = @(Get-CimInstance -Namespace root\wmi -ClassName MSStorageDriver_FailurePredictStatus | Where-Object { $_.PredictFailure } | ForEach-Object { $_.InstanceName })
$r = [ordered]@{ logical = $ld; dirty = $dirty; physical = $pd; smart_fail = $smart; sysdrive = $env:SystemDrive }
"##;

pub fn run(_ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let Some(v) = collect(&mut s, PS, 60) else { return s };
    let sysdrive = v.s("sysdrive").unwrap_or_else(|| "C:".into()).to_ascii_uppercase();

    let mut space_ok = true;
    for d in v.list("logical") {
        let id = d.s("id").unwrap_or_default();
        let letter = id.trim_end_matches(':').to_string();
        let (size, free) = (d.n("size").unwrap_or(0.0), d.n("free").unwrap_or(0.0));
        if size < 2.0 {
            continue; // service partitions with a letter
        }
        let pct = 100.0 * free / size;
        let label = d.s("label").map(|l| format!(" «{l}»")).unwrap_or_default();
        s.fact(&format!("Диск {letter}{label}"), format!("{} свободно из {} ({}%), {}", gb(free), gb(size), pct.round(), d.s("fs").unwrap_or_default()));
        let system = id.eq_ignore_ascii_case(&sysdrive);
        let (crit, warn) = if system { (free < 5.0 || pct < 5.0, free < 15.0 || pct < 10.0) } else { (pct < 3.0, pct < 10.0) };
        let cleanup = if system {
            "Параметры → Система → Память → Временные файлы; удалите ненужные программы и перенесите личные файлы на другой диск. Windows нужно не меньше 15–20 ГБ свободного места для обновлений."
        } else {
            "Удалите или перенесите ненужные файлы; почти полный диск работает медленнее и хуже переносит сбои."
        };
        if crit {
            space_ok = false;
            s.add(Sev::Critical, format!("Почти нет места на диске {letter}: осталось {} ({}%)", gb(free), pct.round())).fix(cleanup);
        } else if warn {
            space_ok = false;
            s.add(Sev::Warning, format!("Мало места на диске {letter}: осталось {} ({}%)", gb(free), pct.round())).fix(cleanup);
        }
    }
    if space_ok && !v.list("logical").is_empty() {
        s.ok("Свободного места на дисках достаточно");
    }

    let dirty = str_list(&v, "dirty");
    if !dirty.is_empty() {
        s.add(Sev::Warning, format!("Файловая система помечена как повреждённая: {}", dirty.join(", ")))
            .fix(format!("Запустите проверку диска от имени администратора: chkdsk {} /f, затем перезагрузитесь.", dirty[0]));
    }

    let smart = str_list(&v, "smart_fail");
    if !smart.is_empty() {
        s.add(Sev::Critical, "SMART предсказывает скорый отказ накопителя")
            .list(smart, 5)
            .fix("Срочно сделайте резервную копию важных данных и замените диск.");
    }

    let mut disks_ok = true;
    for d in v.list("physical") {
        let name = d.s("name").unwrap_or_else(|| "Накопитель".into());
        let mut desc = vec![format!("{} ГБ", d.n("size").unwrap_or(0.0) as i64)];
        for k in ["media", "bus"] {
            if let Some(x) = d.s(k).filter(|x| x != "Unspecified" && x != "0") {
                desc.push(x);
            }
        }
        if let Some(t) = d.n("temp").filter(|t| *t > 0.0) {
            desc.push(format!("{t} °C"));
        }
        if let Some(w) = d.n("wear") {
            desc.push(format!("износ {w}%"));
        }
        if let Some(h) = d.n("hours").filter(|h| *h > 0.0) {
            desc.push(format!("наработка {} тыс. ч", num(h / 1000.0, 1)));
        }
        let health = d.s("health").unwrap_or_default();
        desc.push(format!("состояние: {}", health_ru(&health)));
        s.fact(&name, desc.join(", "));

        let backup = "Сделайте резервную копию важных данных. Подробности — в программе производителя диска или CrystalDiskInfo.";
        match health.as_str() {
            "Unhealthy" | "2" => {
                disks_ok = false;
                s.add(Sev::Critical, format!("Накопитель «{name}» неисправен")).fix(format!("{backup} Диск стоит заменить."));
            }
            "Warning" | "1" => {
                disks_ok = false;
                s.add(Sev::Warning, format!("Накопитель «{name}» сообщает о проблемах")).fix(backup);
            }
            _ => {}
        }
        if let Some(w) = d.n("wear") {
            if w >= 90.0 {
                disks_ok = false;
                s.add(Sev::Critical, format!("SSD «{name}» изношен на {w}%")).fix(format!("{backup} Ресурс почти исчерпан — готовьте замену."));
            } else if w >= 70.0 {
                disks_ok = false;
                s.add(Sev::Warning, format!("SSD «{name}» изношен на {w}%")).fix(backup);
            }
        }
        if let Some(t) = d.n("temp") {
            if t >= 70.0 {
                disks_ok = false;
                s.add(Sev::Warning, format!("Накопитель «{name}» перегревается: {t} °C"))
                    .fix("Проверьте охлаждение корпуса; для NVMe помогает радиатор.");
            }
        }
        let errs = d.n("rerr").unwrap_or(0.0) + d.n("werr").unwrap_or(0.0);
        if errs > 0.0 {
            disks_ok = false;
            s.add(Sev::Warning, format!("Накопитель «{name}»: {} неисправимых ошибок чтения/записи", errs as i64)).fix(backup);
        }
    }
    if disks_ok && !v.list("physical").is_empty() {
        s.ok("Физические накопители исправны");
    }
    s
}

fn health_ru(h: &str) -> &str {
    match h {
        "Healthy" | "0" => "исправен",
        "Warning" | "1" => "предупреждение",
        "Unhealthy" | "2" => "неисправен",
        "" => "неизвестно",
        other => other,
    }
}
