use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Date {
    pub y: i32,
    pub m: u32,
    pub d: u32,
}

impl Date {
    pub const fn new(y: i32, m: u32, d: u32) -> Self {
        Date { y, m, d }
    }
    pub fn days(self) -> i64 {
        days_from_civil(self.y, self.m, self.d)
    }
    pub fn ru(self) -> String {
        format!("{:02}.{:02}.{}", self.d, self.m, self.y)
    }
}

// Howard Hinnant's civil-date algorithms.
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> Date {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    Date { y: (if m <= 2 { y + 1 } else { y }) as i32, m, d }
}

pub fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn today_utc() -> Date {
    civil_from_days(unix_now().div_euclid(86400))
}

/// Parses an RFC 7231 HTTP date, e.g. "Wed, 24 Sep 2026 16:42:18 GMT".
pub fn parse_http_date(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }
    let d: u32 = parts[1].parse().ok()?;
    let months = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let m = months.iter().position(|x| parts[2].to_ascii_lowercase().starts_with(x))? as u32 + 1;
    let y: i32 = parts[3].parse().ok()?;
    let t: Vec<i64> = parts[4].split(':').filter_map(|x| x.parse().ok()).collect();
    if t.len() != 3 {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86400 + t[0] * 3600 + t[1] * 60 + t[2])
}

pub fn plural<'a>(n: u64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let (m10, m100) = (n % 10, n % 100);
    if m10 == 1 && m100 != 11 {
        one
    } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
        few
    } else {
        many
    }
}

pub fn days_human(days: f64) -> String {
    if days < 1.0 {
        let h = (days * 24.0).round().max(0.0) as u64;
        return format!("{h} {}", plural(h, "час", "часа", "часов"));
    }
    let d = days.floor() as u64;
    format!("{d} {}", plural(d, "день", "дня", "дней"))
}

pub fn num(x: f64, dec: usize) -> String {
    format!("{x:.dec$}").replace('.', ",")
}

pub fn gb(x: f64) -> String {
    if x >= 100.0 {
        format!("{} ГБ", x.round() as i64)
    } else {
        format!("{} ГБ", num(x, 1))
    }
}

pub fn trunc(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>().trim_end())
    }
}

pub fn width(s: &str) -> usize {
    s.chars().count()
}

/// Word-wraps `text` so every line (including indent) fits `max` characters.
pub fn wrap(text: &str, max: usize, first: &str, rest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = first.to_string();
    let mut empty = true;
    for word in text.split_whitespace() {
        let extra = if empty { 0 } else { 1 };
        if !empty && width(&line) + extra + width(word) > max {
            out.push(line);
            line = rest.to_string();
            empty = true;
        }
        if !empty {
            line.push(' ');
        }
        line.push_str(word);
        empty = false;
    }
    if !empty || out.is_empty() {
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_roundtrip() {
        for z in [-1000, 0, 19000, 20720, 30000] {
            let d = civil_from_days(z);
            assert_eq!(d.days(), z);
        }
        assert_eq!(civil_from_days(0), Date::new(1970, 1, 1));
        assert_eq!(Date::new(2026, 9, 24).ru(), "24.09.2026");
    }

    #[test]
    fn http_date() {
        let t = parse_http_date("Thu, 24 Sep 2026 16:42:18 GMT").unwrap();
        assert_eq!(t, Date::new(2026, 9, 24).days() * 86400 + 16 * 3600 + 42 * 60 + 18);
        assert!(parse_http_date("garbage").is_none());
    }

    #[test]
    fn russian_plural() {
        assert_eq!(plural(1, "день", "дня", "дней"), "день");
        assert_eq!(plural(3, "день", "дня", "дней"), "дня");
        assert_eq!(plural(11, "день", "дня", "дней"), "дней");
        assert_eq!(plural(22, "день", "дня", "дней"), "дня");
    }

    #[test]
    fn wraps() {
        let l = wrap("один два три четыре", 12, "  ", "    ");
        assert!(l.iter().all(|x| width(x) <= 12), "{l:?}");
        assert_eq!(l[0], "  один два");
        assert!(l[1].starts_with("    "));
    }
}
