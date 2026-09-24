use super::{collect, sysroot};
use crate::model::*;
use crate::paths::fs_exists;

pub const TITLE: &str = "Библиотеки и компоненты";

const PS: &str = r##"
$ndp = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full'
$n35 = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\NET Framework Setup\NDP\v3.5'
$vc = @('x64', 'x86') | ForEach-Object {
  $k = Get-ItemProperty "HKLM:\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\$_"
  if (-not $k) { $k = Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\$_" }
  [ordered]@{ arch = $_; installed = [int]$k.Installed; version = $k.Version } }
$guid = '{' + (@('F3017226', 'FE2A', '4295', '8BDF', '00C3A9A7E4C5') -join '-') + '}'
$wv = @("HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$guid", "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid", "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid") |
  ForEach-Object { (Get-ItemProperty -LiteralPath $_).pv } | Where-Object { $_ -and $_ -ne '0.0.0.0' } | Select-Object -First 1
$r = [ordered]@{ ndp = $ndp.Release; ndp35 = $n35.Install; vc = @($vc); webview2 = $wv }
"##;

pub fn run(_ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let root = sysroot();
    let wow = fs_exists(&format!(r"{root}\SysWOW64"));
    let sys = |f: &str| fs_exists(&format!(r"{root}\System32\{f}"));
    let x86 = |f: &str| if wow { fs_exists(&format!(r"{root}\SysWOW64\{f}")) } else { sys(f) };

    // Visual C++ 2015–2022: the most common "missing DLL" cause.
    let mut vc_missing = Vec::new();
    for f in ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"] {
        if !sys(f) {
            vc_missing.push(format!("{f} (64-бит)"));
        }
    }
    for f in ["vcruntime140.dll", "msvcp140.dll"] {
        if wow && !x86(f) {
            vc_missing.push(format!("{f} (32-бит)"));
        }
    }
    let v = collect(&mut s, PS, 45);
    if let Some(v) = &v {
        for e in v.list("vc") {
            if let (Some(a), Some(ver)) = (e.s("arch"), e.s("version")) {
                if e.n("installed").unwrap_or(0.0) > 0.0 {
                    s.fact(&format!("Visual C++ 2015–2022 {a}"), ver.trim_start_matches('v').to_string());
                }
            }
        }
    }
    if vc_missing.is_empty() {
        s.ok("Visual C++ 2015–2022 установлен (64 и 32 бит)");
    } else {
        s.add(Sev::Warning, "Не хватает библиотек Visual C++ 2015–2022")
            .detail("Из-за этого программы и игры пишут «не найден VCRUNTIME140.dll / MSVCP140.dll».")
            .list(vc_missing, 6)
            .fix("Установите обе версии с сайта Microsoft: «Visual C++ Redistributable latest supported downloads» — vc_redist.x64.exe и vc_redist.x86.exe.");
    }

    let old: Vec<String> = [("msvcr120.dll", "2013"), ("msvcr110.dll", "2012"), ("msvcr100.dll", "2010")]
        .iter()
        .filter(|(f, _)| !sys(f) || (wow && !x86(f)))
        .map(|(f, y)| format!("Visual C++ {y} ({f})"))
        .collect();
    if !old.is_empty() {
        s.add(Sev::Info, "Не установлены старые версии Visual C++")
            .list(old, 5)
            .fix("Нужны только старым программам и играм. Если такая программа не запускается — установите соответствующий пакет с сайта Microsoft.");
    }

    let dx: Vec<&str> = ["d3dx9_43.dll", "d3dx10_43.dll", "d3dx11_43.dll", "xinput1_3.dll", "xaudio2_7.dll", "d3dcompiler_43.dll"]
        .into_iter()
        .filter(|f| !x86(f))
        .collect();
    if dx.is_empty() {
        s.ok("Старые компоненты DirectX (для игр) установлены");
    } else {
        s.add(Sev::Warning, format!("Не хватает компонентов DirectX 9–11 для игр: {}", dx.len()))
            .detail(format!("Отсутствуют: {}", dx.join(", ")))
            .detail("Частая причина ошибок «d3dx9_43.dll не найден» в играх.")
            .fix("Установите «DirectX End-User Runtime Web Installer» с сайта Microsoft — он добавляет только недостающие файлы.");
    }

    if let Some(v) = &v {
        match v.n("ndp").map(|x| x as u32) {
            Some(r) => {
                let ver = match r {
                    533320.. => "4.8.1",
                    528040.. => "4.8",
                    461808.. => "4.7.2",
                    394802.. => "4.6.2",
                    _ => "ниже 4.6.2",
                };
                s.fact(".NET Framework", ver);
                if r < 528040 {
                    s.add(Sev::Warning, format!("Устаревший .NET Framework ({ver})")).fix("Установите .NET Framework 4.8.1 через Центр обновления Windows или с сайта Microsoft.");
                }
            }
            None => {
                s.add(Sev::Warning, ".NET Framework 4 не найден").fix("Установите .NET Framework 4.8.1 с сайта Microsoft.");
            }
        }
        if v.n("ndp35").unwrap_or(0.0) < 1.0 {
            s.add(Sev::Info, ".NET Framework 3.5 не включён")
                .fix("Нужен старым программам. Включается в «Компоненты Windows» (Win+R → optionalfeatures).");
        }
        match v.s("webview2") {
            Some(ver) => {
                s.fact("WebView2 Runtime", ver);
            }
            None => {
                s.add(Sev::Warning, "Не установлен Microsoft Edge WebView2 Runtime")
                    .detail("Без него не открываются окна многих современных программ (в том числе YouTube Glass).")
                    .fix("Скачайте «Evergreen Bootstrapper» на странице Microsoft Edge WebView2.");
            }
        }
    }

    let mut dotnet = Vec::new();
    for base in [r"C:\Program Files\dotnet\shared", r"C:\Program Files (x86)\dotnet\shared"] {
        for fw in ["Microsoft.NETCore.App", "Microsoft.WindowsDesktop.App", "Microsoft.AspNetCore.App"] {
            if let Ok(rd) = std::fs::read_dir(format!(r"{base}\{fw}")) {
                let mut vers: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
                vers.sort();
                if !vers.is_empty() {
                    let arch = if base.contains("(x86)") { " x86" } else { "" };
                    dotnet.push(format!("{fw}{arch}: {}", vers.join(", ")));
                }
            }
        }
    }
    if dotnet.is_empty() {
        s.add(Sev::Info, "Современный .NET (6/8/9) не установлен")
            .fix("Если программа просит «.NET Desktop Runtime» — скачайте нужную версию на dotnet.microsoft.com.");
    } else {
        s.fact(".NET", dotnet.join("; "));
    }
    s
}
