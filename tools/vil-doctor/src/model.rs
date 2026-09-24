use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Sev {
    Critical,
    Warning,
    Info,
    Ok,
}

impl Sev {
    pub fn tag(self) -> &'static str {
        match self {
            Sev::Critical => "КРИТИЧНО",
            Sev::Warning => "ВНИМАНИЕ",
            Sev::Info => "СОВЕТ",
            Sev::Ok => "OK",
        }
    }
}

#[derive(Debug)]
pub struct Finding {
    pub sev: Sev,
    pub title: String,
    pub details: Vec<String>,
    pub fix: Option<String>,
}

impl Finding {
    pub fn detail(&mut self, d: impl Into<String>) -> &mut Self {
        self.details.push(d.into());
        self
    }

    /// Adds up to `cap` lines and a "and N more" tail, so reports stay readable.
    pub fn list<I: IntoIterator<Item = String>>(&mut self, items: I, cap: usize) -> &mut Self {
        let items: Vec<String> = items.into_iter().collect();
        let total = items.len();
        for it in items.into_iter().take(cap) {
            self.details.push(format!("• {it}"));
        }
        if total > cap {
            self.details.push(format!("…и ещё {}", total - cap));
        }
        self
    }

    pub fn fix(&mut self, f: impl Into<String>) -> &mut Self {
        self.fix = Some(f.into());
        self
    }
}

#[derive(Debug)]
pub struct Section {
    pub title: &'static str,
    pub facts: Vec<(String, String)>,
    pub findings: Vec<Finding>,
    pub errors: Vec<String>,
    pub skipped: Option<String>,
    pub appendix: Vec<(String, Vec<String>)>,
}

impl Section {
    pub fn new(title: &'static str) -> Self {
        Section { title, facts: vec![], findings: vec![], errors: vec![], skipped: None, appendix: vec![] }
    }

    pub fn fact(&mut self, k: &str, v: impl Into<String>) {
        let v = v.into();
        if !v.trim().is_empty() {
            self.facts.push((k.to_string(), v));
        }
    }

    pub fn add(&mut self, sev: Sev, title: impl Into<String>) -> &mut Finding {
        self.findings.push(Finding { sev, title: title.into(), details: vec![], fix: None });
        self.findings.last_mut().unwrap()
    }

    pub fn ok(&mut self, title: impl Into<String>) {
        self.add(Sev::Ok, title);
    }

    pub fn error(&mut self, e: impl Into<String>) {
        self.errors.push(e.into());
    }

    pub fn count(&self, sev: Sev) -> usize {
        self.findings.iter().filter(|f| f.sev == sev).count()
    }

    pub fn worst(&self) -> Option<Sev> {
        self.findings.iter().map(|f| f.sev).min()
    }
}

pub struct Ctx {
    pub admin: bool,
    pub deep: bool,
    pub today: crate::text::Date,
}

/// Lenient accessors: PowerShell 5.1 JSON is loosely typed (numbers as strings,
/// single-item arrays collapsed to objects), so every read tolerates that.
pub trait J {
    fn s(&self, k: &str) -> Option<String>;
    fn n(&self, k: &str) -> Option<f64>;
    fn b(&self, k: &str) -> Option<bool>;
    fn list(&self, k: &str) -> Vec<&Value>;
    /// Present and non-null: an empty `[]` is a real "nothing found",
    /// `null` means the collecting statement failed.
    fn has(&self, k: &str) -> bool;
}

impl J for Value {
    fn s(&self, k: &str) -> Option<String> {
        match self.get(k)? {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }

    fn n(&self, k: &str) -> Option<f64> {
        match self.get(k)? {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().replace(',', ".").parse().ok(),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    fn b(&self, k: &str) -> Option<bool> {
        match self.get(k)? {
            Value::Bool(b) => Some(*b),
            Value::Number(n) => n.as_f64().map(|x| x != 0.0),
            Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" => Some(true),
                "false" | "0" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    fn list(&self, k: &str) -> Vec<&Value> {
        as_list(self.get(k))
    }

    fn has(&self, k: &str) -> bool {
        self.get(k).is_some_and(|x| !x.is_null())
    }
}

pub fn as_list(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(a)) => a.iter().filter(|x| !x.is_null()).collect(),
        Some(Value::Null) | None => vec![],
        Some(other) => vec![other],
    }
}

pub fn str_list(v: &Value, k: &str) -> Vec<String> {
    v.list(k)
        .into_iter()
        .filter_map(|x| match x {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lenient_reads() {
        let v = json!({"a":"12,5","b":"True","one":{"x":1},"many":[{"x":1},null,{"x":2}],"e":""});
        assert_eq!(v.n("a"), Some(12.5));
        assert_eq!(v.b("b"), Some(true));
        assert_eq!(v.list("one").len(), 1);
        assert_eq!(v.list("many").len(), 2);
        assert_eq!(v.s("e"), None);
        assert!(v.list("missing").is_empty());
    }
}
