//! Turns command lines and shortcut targets from the registry into file paths
//! and decides whether the file behind them really is missing.
//! Paths are handled as Windows strings so the logic is testable anywhere.

#[derive(Debug, PartialEq)]
pub enum Res {
    Found(String),
    Missing(String),
    /// Can't be judged reliably (network, removable drive, unknown variable…).
    Skip,
}

pub struct Env<'a> {
    pub var: &'a dyn Fn(&str) -> Option<String>,
    pub exists: &'a dyn Fn(&str) -> bool,
    pub search: Vec<String>,
}

const EXTS: [&str; 12] = [".exe", ".com", ".bat", ".cmd", ".sys", ".dll", ".scr", ".cpl", ".msc", ".vbs", ".ps1", ".lnk"];

/// Expands %VAR% references; `None` if a variable is unknown.
pub fn expand_env(s: &str, var: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) if j > 0 && !after[..j].contains(['\\', ' ', '"']) => {
                out.push_str(&var(&after[..j])?);
                rest = &after[j + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Some(out)
}

fn find_ext_end(s: &str) -> Option<usize> {
    let low = s.to_ascii_lowercase();
    let mut best: Option<usize> = None;
    for ext in EXTS {
        let mut from = 0;
        while let Some(i) = low[from..].find(ext) {
            let end = from + i + ext.len();
            let next = low[end..].chars().next();
            if matches!(next, None | Some(' ') | Some('\t') | Some(',') | Some('"')) {
                best = Some(best.map_or(end, |b: usize| b.min(end)));
                break;
            }
            from = end;
        }
    }
    best
}

/// Program part of a command line: `"C:\a b\x.exe" -k` → `C:\a b\x.exe`.
pub fn extract_program(cmd: &str) -> Option<(String, String)> {
    let t = cmd.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(stripped) = t.strip_prefix('"') {
        return Some(match stripped.find('"') {
            Some(j) => (stripped[..j].to_string(), stripped[j + 1..].trim().to_string()),
            None => (stripped.to_string(), String::new()),
        });
    }
    if let Some(end) = find_ext_end(t) {
        return Some((t[..end].to_string(), t[end..].trim().to_string()));
    }
    let end = t.find(char::is_whitespace).unwrap_or(t.len());
    Some((t[..end].to_string(), t[end..].trim().to_string()))
}

pub fn normalize(p: &str, env: &Env) -> Option<String> {
    let mut p = p.trim().trim_matches('"').to_string();
    if let Some(s) = p.strip_prefix(r"\??\") {
        p = s.to_string();
    }
    let low = p.to_ascii_lowercase();
    if low.starts_with(r"\systemroot\") {
        p = format!(r"%SystemRoot%\{}", &p[12..]);
    } else if low.starts_with(r"system32\") || low.starts_with(r"syswow64\") {
        p = format!(r"%SystemRoot%\{p}");
    }
    let p = expand_env(&p, env.var)?;
    Some(p.replace('/', "\\"))
}

fn has_ext(name: &str) -> bool {
    name.rsplit('\\').next().is_some_and(|f| f.contains('.'))
}

/// Checks a bare file path (shortcut target, task action, extracted program).
pub fn resolve_path(raw: &str, env: &Env) -> Res {
    let raw = raw.trim().trim_matches('"').trim();
    if raw.is_empty() || raw.starts_with("\\\\") || raw.contains("://") || raw.starts_with("::") {
        return Res::Skip;
    }
    let Some(p) = normalize(raw, env) else { return Res::Skip };
    let low = p.to_ascii_lowercase();
    if low.contains(r"\windowsapps\") || p.starts_with("\\\\") {
        return Res::Skip;
    }
    if !p.contains('\\') && !p.contains(':') {
        for dir in &env.search {
            let mut cand = format!(r"{}\{}", dir.trim_end_matches('\\'), p);
            if (env.exists)(&cand) {
                return Res::Found(cand);
            }
            if !has_ext(&p) {
                cand.push_str(".exe");
                if (env.exists)(&cand) {
                    return Res::Found(cand);
                }
            }
        }
        return Res::Skip;
    }
    let b = p.as_bytes();
    if b.len() < 3 || b[1] != b':' || b[2] != b'\\' {
        return Res::Skip; // relative path: depends on a working directory we don't know
    }
    if !(env.exists)(&p[..3]) {
        return Res::Skip; // drive not connected right now
    }
    if (env.exists)(&p) {
        return Res::Found(p);
    }
    // 32-bit installers sometimes record the other Program Files directory.
    for (a, b) in [(r"c:\program files\", r"C:\Program Files (x86)\"), (r"c:\program files (x86)\", r"C:\Program Files\")] {
        if low.starts_with(a) {
            let alt = format!("{b}{}", &p[a.len()..]);
            if (env.exists)(&alt) {
                return Res::Found(alt);
            }
        }
    }
    Res::Missing(p)
}

/// Checks a full command line, including the DLL of `rundll32 x.dll,Entry`.
pub fn resolve_command(cmd: &str, env: &Env) -> Res {
    let Some((prog, args)) = extract_program(cmd) else { return Res::Skip };
    let r = resolve_path(&prog, env);
    let name = prog.rsplit('\\').next().unwrap_or("").to_ascii_lowercase();
    if matches!(r, Res::Found(_)) && (name == "rundll32.exe" || name == "rundll32") {
        if let Some((dll, _)) = extract_program(&args) {
            let dll = dll.split(',').next().unwrap_or("").to_string();
            if let Res::Missing(m) = resolve_path(&dll, env) {
                return Res::Missing(m);
            }
        }
    }
    r
}

/// Real-filesystem existence where "access denied" counts as present,
/// so protected folders never show up as missing files.
pub fn fs_exists(p: &str) -> bool {
    match std::fs::metadata(p) {
        Ok(_) => true,
        Err(e) => e.kind() != std::io::ErrorKind::NotFound && e.raw_os_error() != Some(3),
    }
}

pub type VarFn = Box<dyn Fn(&str) -> Option<String>>;

pub fn system_env() -> (VarFn, Vec<String>) {
    let var = Box::new(|k: &str| std::env::var(k).ok());
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let mut search = vec![
        format!(r"{root}\System32"),
        root.clone(),
        format!(r"{root}\System32\Wbem"),
        format!(r"{root}\System32\WindowsPowerShell\v1.0"),
    ];
    if let Ok(path) = std::env::var("PATH") {
        search.extend(path.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
    }
    (var, search)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn with_env<R>(files: &[&str], f: impl FnOnce(&Env) -> R) -> R {
        let set: HashSet<String> = files.iter().map(|s| s.to_ascii_lowercase()).collect();
        let exists = move |p: &str| set.contains(&p.to_ascii_lowercase());
        let var = |k: &str| match k.to_ascii_lowercase().as_str() {
            "systemroot" | "windir" => Some(r"C:\Windows".to_string()),
            "programfiles" => Some(r"C:\Program Files".to_string()),
            _ => None,
        };
        let env = Env { var: &var, exists: &exists, search: vec![r"C:\Windows\System32".into()] };
        f(&env)
    }

    #[test]
    fn extracts_programs() {
        assert_eq!(extract_program(r#""C:\a b\x.exe" -k 1"#).unwrap().0, r"C:\a b\x.exe");
        assert_eq!(extract_program(r"C:\Program Files\Foo\foo.exe /background").unwrap().0, r"C:\Program Files\Foo\foo.exe");
        assert_eq!(extract_program(r"C:\WINDOWS\system32\svchost.exe -k netsvcs -p").unwrap().0, r"C:\WINDOWS\system32\svchost.exe");
        assert_eq!(extract_program("msiexec /x{1234}").unwrap().0, "msiexec");
        assert_eq!(extract_program(r"C:\x.executor\run.exe").unwrap().0, r"C:\x.executor\run.exe");
    }

    #[test]
    fn expands_env() {
        let var = |k: &str| (k == "SystemRoot").then(|| r"C:\Windows".to_string());
        assert_eq!(expand_env(r"%SystemRoot%\x.exe", &var).unwrap(), r"C:\Windows\x.exe");
        assert_eq!(expand_env("100% done", &var).unwrap(), "100% done");
        assert!(expand_env(r"%NOPE%\x", &var).is_none());
    }

    #[test]
    fn resolves() {
        let files = [r"C:\", r"C:\Windows\System32\svchost.exe", r"C:\Windows\System32\drivers\ok.sys", r"C:\Windows\System32\rundll32.exe", r"C:\Program Files (x86)\App\app.exe"];
        with_env(&files, |env| {
            assert!(matches!(resolve_command(r"%SystemRoot%\System32\svchost.exe -k x", env), Res::Found(_)));
            assert!(matches!(resolve_command(r"\SystemRoot\System32\drivers\ok.sys", env), Res::Found(_)));
            assert!(matches!(resolve_command(r"System32\drivers\ok.sys", env), Res::Found(_)));
            assert_eq!(resolve_command(r"\??\C:\Windows\System32\drivers\gone.sys", env), Res::Missing(r"C:\Windows\System32\drivers\gone.sys".into()));
            assert_eq!(resolve_command(r#""C:\Old\tool.exe" /tray"#, env), Res::Missing(r"C:\Old\tool.exe".into()));
            assert_eq!(resolve_command(r"D:\Games\x.exe", env), Res::Skip, "absent drive");
            assert_eq!(resolve_command(r"\\server\share\x.exe", env), Res::Skip);
            assert_eq!(resolve_command(r"%UNKNOWN%\x.exe", env), Res::Skip);
            assert!(matches!(resolve_command("svchost.exe", env), Res::Found(_)));
            assert_eq!(resolve_command("mystery.exe", env), Res::Skip);
            assert_eq!(
                resolve_command(r"rundll32.exe C:\Program Files\Old\helper.dll,Start", env),
                Res::Missing(r"C:\Program Files\Old\helper.dll".into())
            );
            assert!(matches!(resolve_command(r"%ProgramFiles%\App\app.exe", env), Res::Found(_)), "x86 fallback");
            assert_eq!(resolve_command(r"C:\Program Files\WindowsApps\x\y.exe", env), Res::Skip);
        });
    }
}
