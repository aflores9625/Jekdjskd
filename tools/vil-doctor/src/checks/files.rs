use super::sysroot;
use crate::model::*;
use crate::paths::fs_exists;

pub const TITLE: &str = "Системные файлы Windows";

const CORE: &[&str] = &[
    r"System32\ntoskrnl.exe", r"System32\hal.dll", r"System32\ntdll.dll", r"System32\kernel32.dll", r"System32\kernelbase.dll",
    r"System32\user32.dll", r"System32\gdi32.dll", r"System32\advapi32.dll", r"System32\sechost.dll", r"System32\rpcrt4.dll",
    r"System32\combase.dll", r"System32\ole32.dll", r"System32\oleaut32.dll", r"System32\shell32.dll", r"System32\shlwapi.dll",
    r"System32\msvcrt.dll", r"System32\ucrtbase.dll", r"System32\comctl32.dll", r"System32\comdlg32.dll", r"System32\ws2_32.dll",
    r"System32\winhttp.dll", r"System32\wininet.dll", r"System32\crypt32.dll", r"System32\bcrypt.dll", r"System32\dwmapi.dll",
    r"System32\uxtheme.dll", r"System32\winmm.dll", r"System32\dxgi.dll", r"System32\d3d9.dll", r"System32\d3d11.dll",
    r"System32\d3d12.dll", r"System32\opengl32.dll", r"System32\dsound.dll", r"System32\xinput1_4.dll", r"System32\msi.dll",
    r"System32\svchost.exe", r"System32\lsass.exe", r"System32\csrss.exe", r"System32\smss.exe", r"System32\winlogon.exe",
    r"System32\services.exe", r"System32\wininit.exe", r"System32\userinit.exe", r"System32\dwm.exe", r"System32\conhost.exe",
    r"System32\cmd.exe", r"System32\rundll32.exe", r"System32\regsvr32.exe", r"System32\msiexec.exe", r"System32\taskmgr.exe",
    r"System32\sfc.exe", r"System32\dism.exe", r"System32\dllhost.exe", r"System32\wbem\WmiPrvSE.exe",
    r"System32\WindowsPowerShell\v1.0\powershell.exe", r"System32\drivers\ntfs.sys",
    r"System32\config\SYSTEM", r"System32\config\SOFTWARE", r"explorer.exe",
];

const WOW64: &[&str] = &[
    r"SysWOW64\ntdll.dll", r"SysWOW64\kernel32.dll", r"SysWOW64\user32.dll", r"SysWOW64\gdi32.dll", r"SysWOW64\msvcrt.dll",
    r"SysWOW64\ucrtbase.dll", r"SysWOW64\ws2_32.dll", r"SysWOW64\d3d9.dll", r"SysWOW64\d3d11.dll", r"SysWOW64\dxgi.dll",
    r"SysWOW64\opengl32.dll", r"SysWOW64\winmm.dll", r"SysWOW64\xinput1_4.dll", r"SysWOW64\cmd.exe",
];

const CHECK_HEALTH: &str = r##"
$h = Repair-WindowsImage -Online -CheckHealth
$r = [ordered]@{ state = "$($h.ImageHealthState)" }
"##;

const SCAN_HEALTH: &str = r##"
$h = Repair-WindowsImage -Online -ScanHealth
$r = [ordered]@{ state = "$($h.ImageHealthState)" }
"##;

pub fn run(ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let root = sysroot();
    let wow = fs_exists(&format!(r"{root}\SysWOW64"));
    let mut list: Vec<&str> = CORE.to_vec();
    if wow {
        list.extend_from_slice(WOW64);
    }

    let (mut missing, mut empty) = (Vec::new(), Vec::new());
    for rel in &list {
        let p = format!(r"{root}\{rel}");
        match std::fs::metadata(&p) {
            Ok(m) if m.is_file() && m.len() == 0 => empty.push(p),
            Ok(_) => {}
            Err(_) if fs_exists(&p) => {} // present but locked (registry hives)
            Err(_) => missing.push(p),
        }
    }
    s.fact("Проверено ключевых файлов", list.len().to_string());
    let repair = "Откройте командную строку от имени администратора и выполните по очереди: DISM /Online /Cleanup-Image /RestoreHealth, затем sfc /scannow. После — перезагрузка.";
    if !missing.is_empty() {
        s.add(Sev::Critical, format!("Отсутствуют системные файлы Windows: {}", missing.len())).list(missing, 15).fix(repair);
    }
    if !empty.is_empty() {
        s.add(Sev::Critical, format!("Системные файлы повреждены (нулевой размер): {}", empty.len())).list(empty, 15).fix(repair);
    }

    let mf = format!(r"{root}\System32\mfplat.dll");
    if !fs_exists(&mf) {
        s.add(Sev::Warning, "Нет компонентов мультимедиа (Media Foundation) — это редакция Windows «N»")
            .detail("Без них не работают видео в некоторых программах, играх и мессенджерах.")
            .fix("Параметры → Приложения → Дополнительные компоненты → «Пакет дополнительных компонентов мультимедиа».");
    }
    if !fs_exists(&format!(r"{root}\System32\vulkan-1.dll")) {
        s.add(Sev::Info, "Нет библиотеки Vulkan (vulkan-1.dll)")
            .fix("Её ставит драйвер видеокарты. Обновите драйвер с сайта NVIDIA / AMD / Intel, если игры жалуются на Vulkan.");
    }

    if ctx.admin {
        match crate::ps::run_json(CHECK_HEALTH, crate::ps::secs(180)) {
            Ok(v) => image_state(&mut s, &v.s("state").unwrap_or_default(), "Быстрая проверка образа Windows (DISM CheckHealth)"),
            Err(e) => s.error(format!("DISM CheckHealth: {e}")),
        }
        if ctx.deep {
            match crate::ps::run_json(SCAN_HEALTH, crate::ps::secs(40 * 60)) {
                Ok(v) => image_state(&mut s, &v.s("state").unwrap_or_default(), "Глубокая проверка образа Windows (DISM ScanHealth)"),
                Err(e) => s.error(format!("DISM ScanHealth: {e}")),
            }
        } else {
            s.add(Sev::Info, "Глубокая проверка образа не запускалась")
                .fix("Для полной проверки запустите программу с параметром --deep или выполните sfc /scannow в командной строке администратора.");
        }
    } else {
        s.add(Sev::Info, "Проверка целостности образа (DISM) пропущена — нужны права администратора")
            .fix("Запустите VIL Doctor от имени администратора.");
    }

    if s.count(Sev::Critical) == 0 {
        s.ok("Ключевые системные файлы на месте");
    }
    s
}

fn image_state(s: &mut Section, state: &str, what: &str) {
    let repair = "Выполните в командной строке администратора: DISM /Online /Cleanup-Image /RestoreHealth, затем sfc /scannow.";
    match state {
        "Healthy" | "0" => s.ok(format!("{what}: повреждений нет")),
        "Repairable" | "2" => {
            s.add(Sev::Critical, format!("{what}: найдены повреждения, их можно исправить")).fix(repair);
        }
        "NonRepairable" | "1" => {
            s.add(Sev::Critical, format!("{what}: образ повреждён и не восстанавливается автоматически"))
                .fix("Понадобится восстановление с установочного образа Windows (DISM /Source) или переустановка «с сохранением файлов» через Media Creation Tool.");
        }
        other => s.error(format!("{what}: неизвестный ответ «{other}»")),
    }
}
