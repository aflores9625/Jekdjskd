use super::collect;
use crate::model::*;
use crate::text::days_human;

pub const TITLE: &str = "Устройства и драйверы";

const PS: &str = r##"
$bad = @(Get-CimInstance Win32_PnPEntity -Filter 'ConfigManagerErrorCode<>0' | ForEach-Object { [ordered]@{ name = $_.Name; code = [int]$_.ConfigManagerErrorCode; cls = $_.PNPClass; id = $_.PNPDeviceID } })
$gpu = @(Get-CimInstance Win32_VideoController | ForEach-Object { [ordered]@{ name = $_.Name; ver = $_.DriverVersion; days = (D $_.DriverDate); status = $_.Status } })
$bat = $null
$full = (Get-CimInstance -Namespace root\wmi -ClassName BatteryFullChargedCapacity | Select-Object -First 1).FullChargedCapacity
$design = (Get-CimInstance -Namespace root\wmi -ClassName BatteryStaticData | Select-Object -First 1).DesignedCapacity
if ($full -and $design) { $bat = [ordered]@{ full = $full; design = $design } }
$r = [ordered]@{ bad = $bad; gpu = $gpu; battery = $bat }
"##;

fn code_ru(code: i64) -> (&'static str, Sev) {
    match code {
        1 => ("устройство не настроено", Sev::Warning),
        3 => ("драйвер повреждён или не хватает памяти", Sev::Warning),
        10 => ("устройство не может запуститься", Sev::Warning),
        12 => ("не хватает свободных ресурсов", Sev::Warning),
        14 => ("нужна перезагрузка", Sev::Info),
        18 => ("драйверы нужно переустановить", Sev::Warning),
        19 => ("повреждены сведения в реестре", Sev::Warning),
        21 => ("устройство удаляется", Sev::Info),
        22 => ("устройство отключено вручную", Sev::Info),
        28 => ("драйвер не установлен", Sev::Warning),
        29 => ("отключено в прошивке (BIOS)", Sev::Info),
        31 => ("Windows не может загрузить драйвер", Sev::Warning),
        32 => ("служба драйвера отключена", Sev::Warning),
        37 => ("драйвер не инициализировался", Sev::Warning),
        39 => ("драйвер повреждён или отсутствует", Sev::Warning),
        40 => ("повреждена запись службы драйвера", Sev::Warning),
        41 => ("драйвер загружен, но устройство не найдено", Sev::Warning),
        43 => ("устройство сообщило о сбое и остановлено", Sev::Warning),
        48 => ("драйвер заблокирован как несовместимый", Sev::Warning),
        52 => ("у драйвера нет действительной цифровой подписи", Sev::Warning),
        _ => ("ошибка устройства", Sev::Warning),
    }
}

pub fn run(_ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let Some(v) = collect(&mut s, PS, 60) else { return s };

    let mut shown = 0;
    for d in v.list("bad") {
        let code = d.n("code").unwrap_or(0.0) as i64;
        if matches!(code, 24 | 45) {
            continue; // "not present" / "not connected": unplugged devices
        }
        let (what, sev) = code_ru(code);
        let name = d.s("name").unwrap_or_else(|| "Неизвестное устройство".into());
        shown += 1;
        let fix = match code {
            22 => "Если устройство нужно — Диспетчер устройств → правый клик → «Включить устройство».".to_string(),
            28 | 18 | 39 | 31 | 1 => "Установите драйвер с сайта производителя ноутбука/материнской платы или через Параметры → Центр обновления → Дополнительные обновления.".to_string(),
            43 | 10 => "Переподключите устройство (другой порт USB), обновите или откатите драйвер в Диспетчере устройств.".to_string(),
            52 => "Скачайте официальный подписанный драйвер с сайта производителя.".to_string(),
            _ => "Откройте Диспетчер устройств (Win+X → Диспетчер устройств), обновите или переустановите драйвер.".to_string(),
        };
        let f = s.add(sev, format!("{name}: {what} (код {code})"));
        if let Some(id) = d.s("id") {
            f.detail(format!("ID: {id}"));
        }
        f.fix(fix);
    }
    if shown == 0 && v.has("bad") {
        s.ok("Все устройства работают без ошибок");
    }

    for g in v.list("gpu") {
        let name = g.s("name").unwrap_or_default();
        let mut desc = vec![];
        if let Some(ver) = g.s("ver") {
            desc.push(format!("драйвер {ver}"));
        }
        let age = g.n("days");
        if let Some(d) = age {
            desc.push(format!("от {} назад", days_human(d)));
        }
        s.fact(&format!("Видеокарта {name}"), desc.join(", "));
        let low = name.to_ascii_lowercase();
        if low.contains("basic display") || low.contains("базовый видеоадаптер") || low.contains("basic render") {
            s.add(Sev::Warning, "Не установлен драйвер видеокарты (работает «Базовый видеоадаптер Майкрософт»)")
                .fix("Установите драйвер с сайта NVIDIA, AMD или Intel — без него не будет аппаратного ускорения, нормального разрешения и игр.");
        } else if age.is_some_and(|d| d > 730.0) && !low.contains("virtual") && !low.contains("remote") {
            s.add(Sev::Info, format!("Драйвер видеокарты «{name}» старше двух лет"))
                .fix("Обновите драйвер с сайта производителя видеокарты — это исправляет ошибки и ускоряет игры.");
        }
    }

    if let Some(b) = v.get("battery").filter(|b| !b.is_null()) {
        if let (Some(full), Some(design)) = (b.n("full"), b.n("design")) {
            if design > 0.0 {
                let health = (100.0 * full / design).min(100.0).round();
                s.fact("Аккумулятор", format!("{health}% от заводской ёмкости"));
                if health < 60.0 {
                    s.add(Sev::Warning, format!("Аккумулятор сильно изношен: {health}% ёмкости"))
                        .fix("Батарея держит заметно меньше. Подробный отчёт: powercfg /batteryreport; замену лучше делать в сервисе.");
                } else if health < 80.0 {
                    s.add(Sev::Info, format!("Аккумулятор изношен: {health}% ёмкости"));
                }
            }
        }
    }
    s
}
