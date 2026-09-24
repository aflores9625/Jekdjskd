//! Runs Windows PowerShell 5.1 snippets and returns their JSON result.

use serde_json::Value;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const PRELUDE: &str = r##"
$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
function D($t) { if ($t) { [math]::Round(((Get-Date) - [datetime]$t).TotalDays, 2) } else { $null } }
function L($s, $n = 200) { if ($s) { $x = ([string]$s -split "`r?`n")[0].Trim(); if ($x.Length -gt $n) { $x.Substring(0, $n) + '…' } else { $x } } else { $null } }
function T($t) { if ($t) { ([datetime]$t).ToString('dd.MM.yyyy HH:mm') } else { $null } }
"##;

pub fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn powershell_exe() -> String {
    #[cfg(windows)]
    {
        if let Ok(root) = std::env::var("SystemRoot") {
            let p = format!(r"{root}\System32\WindowsPowerShell\v1.0\powershell.exe");
            if std::path::Path::new(&p).exists() {
                return p;
            }
        }
        "powershell.exe".into()
    }
    #[cfg(not(windows))]
    {
        std::env::var("VIL_PWSH").unwrap_or_else(|_| "pwsh".into())
    }
}

/// The snippet must assign its result to `$r`. A failing statement is skipped
/// (trap/continue) so one broken cmdlet doesn't discard the whole section;
/// its message comes back in `_errs`.
pub fn run_json(script: &str, timeout: Duration) -> Result<Value, String> {
    let full = format!(
        r#"{PRELUDE}
$__e = New-Object System.Collections.ArrayList
trap {{ [void]$__e.Add([string]$_.Exception.Message); continue }}
$r = $null
{script}
if ($null -eq $r -and $__e.Count) {{ $r = @{{ _error = $__e[0] }} }}
elseif ($r -is [System.Collections.IDictionary] -and $__e.Count) {{ $r['_errs'] = @($__e | Select-Object -First 3) }}
ConvertTo-Json -InputObject $r -Depth 6 -Compress
"#
    );
    let out = run_raw(&full, timeout)?;
    let start = out.find(['{', '[']).ok_or_else(|| {
        if out.trim().is_empty() || out.trim() == "null" {
            "PowerShell не вернул данных".to_string()
        } else {
            format!("неожиданный ответ PowerShell: {}", crate::text::trunc(&out, 200))
        }
    })?;
    let v: Value = serde_json::from_str(out[start..].trim())
        .map_err(|e| format!("не удалось разобрать ответ PowerShell: {e}"))?;
    if let Some(err) = v.get("_error").and_then(|e| e.as_str()) {
        return Err(format!("ошибка PowerShell: {}", crate::text::trunc(err, 300)));
    }
    Ok(v)
}

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// Removes the temporary script however the run ends.
struct TempScript(std::path::PathBuf);
impl Drop for TempScript {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn run_raw(script: &str, timeout: Duration) -> Result<String, String> {
    // A readable temp file plus a plain -Command avoids opaque base64 command
    // lines (an antivirus red flag) and isn't subject to execution policy.
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let file = TempScript(std::env::temp_dir().join(format!("vil-doctor-{}-{n}.ps1", std::process::id())));
    std::fs::write(&file.0, format!("\u{feff}{script}")).map_err(|e| format!("не удалось создать временный скрипт: {e}"))?;
    let quoted = file.0.to_string_lossy().replace('\'', "''");
    let mut cmd = Command::new(powershell_exe());
    cmd.args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(format!("& ([scriptblock]::Create([IO.File]::ReadAllText('{quoted}')))"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("не удалось запустить PowerShell: {e}"))?;

    // Drain pipes on threads so a chatty child can't deadlock on a full buffer.
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let t_out = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = so.read_to_end(&mut b);
        b
    });
    let t_err = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = se.read_to_end(&mut b);
        b
    });

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("превышено время ожидания ({} с)", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(format!("ошибка ожидания PowerShell: {e}")),
        }
    }
    let out = String::from_utf8_lossy(&t_out.join().unwrap_or_default()).into_owned();
    let err = String::from_utf8_lossy(&t_err.join().unwrap_or_default()).into_owned();
    if out.trim().is_empty() && !err.trim().is_empty() {
        return Err(format!("PowerShell: {}", crate::text::trunc(&err, 300)));
    }
    Ok(out)
}
