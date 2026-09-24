//! Renders the plain-text report: summary first, details after, raw data last.

use crate::model::{Section, Sev};
use crate::text::{plural, width, wrap};

const W: usize = 96;

pub struct Meta {
    pub computer: String,
    pub os: String,
    pub when: String,
    pub admin: bool,
    pub deep: bool,
    pub seconds: u64,
    pub version: &'static str,
}

pub struct Totals {
    pub critical: usize,
    pub warning: usize,
    pub info: usize,
    pub ok: usize,
    pub score: u32,
}

pub fn totals(sections: &[Section]) -> Totals {
    let c = |s: Sev| sections.iter().map(|x| x.count(s)).sum::<usize>();
    let (critical, warning, info, ok) = (c(Sev::Critical), c(Sev::Warning), c(Sev::Info), c(Sev::Ok));
    let penalty = critical * 15 + warning * 5 + info;
    Totals { critical, warning, info, ok, score: 100u32.saturating_sub(penalty.min(100) as u32) }
}

pub fn verdict(t: &Totals) -> &'static str {
    if t.critical > 0 {
        "Есть серьёзные проблемы"
    } else if t.warning > 2 {
        "Требует внимания"
    } else if t.warning > 0 {
        "В целом хорошо, есть замечания"
    } else {
        "Отлично"
    }
}

fn rule(ch: char) -> String {
    ch.to_string().repeat(W)
}

fn kv(out: &mut Vec<String>, indent: &str, k: &str, v: &str, kw: usize) {
    let pad = " ".repeat(kw.saturating_sub(width(k)));
    let first = format!("{indent}{k}:{pad} ");
    let rest = " ".repeat(width(&first));
    out.extend(wrap(v, W, &first, &rest));
}

pub fn render(meta: &Meta, sections: &[Section]) -> String {
    let t = totals(sections);
    let mut o: Vec<String> = vec![rule('═'), "  VIL Doctor — отчёт о состоянии Windows".into(), rule('═'), String::new()];
    let access = if meta.admin { "администратор — полная проверка" } else { "обычный пользователь — часть проверок пропущена" };
    let depth = if meta.deep { "глубокая (с DISM ScanHealth)" } else { "стандартная" };
    for (k, v) in [
        ("Компьютер", meta.computer.as_str()),
        ("Система", meta.os.as_str()),
        ("Дата проверки", meta.when.as_str()),
        ("Права", access),
        ("Проверка", depth),
    ] {
        kv(&mut o, "  ", k, v, 14);
    }
    kv(&mut o, "  ", "Длительность", &format!("{} с", meta.seconds), 14);
    o.push(String::new());
    o.push("  Программа только читает данные и ничего не меняет в системе.".into());
    o.push(String::new());

    o.push("  ИТОГ".into());
    o.push("  ────".into());
    o.push(format!("  {} · индекс здоровья {}/100", verdict(&t), t.score));
    o.push(format!(
        "  Критично: {}   Внимание: {}   Советы: {}   В порядке: {}",
        t.critical, t.warning, t.info, t.ok
    ));
    o.push(String::new());

    let mut top: Vec<(&Section, &crate::model::Finding)> = sections
        .iter()
        .flat_map(|s| s.findings.iter().map(move |f| (s, f)))
        .filter(|(_, f)| matches!(f.sev, Sev::Critical | Sev::Warning))
        .collect();
    top.sort_by_key(|(_, f)| f.sev);
    if top.is_empty() {
        o.push("  Серьёзных проблем не найдено. Ниже — подробности по каждому разделу.".into());
    } else {
        o.push("  ЧТО СДЕЛАТЬ В ПЕРВУЮ ОЧЕРЕДЬ".into());
        o.push("  ───────────────────────────".into());
        for (i, (s, f)) in top.iter().enumerate() {
            let head = format!("  {:>2}. [{}] ", i + 1, f.sev.tag());
            let rest = " ".repeat(width(&head));
            o.extend(wrap(&format!("{} ({})", f.title, s.title), W, &head, &rest));
            if let Some(fix) = &f.fix {
                o.extend(wrap(fix, W, &format!("{rest}→ "), &format!("{rest}  ")));
            }
        }
    }
    o.push(String::new());

    o.push(rule('─'));
    o.push("  ПОДРОБНО ПО РАЗДЕЛАМ".into());
    o.push(rule('─'));
    for (i, s) in sections.iter().enumerate() {
        o.push(String::new());
        let status = match s.worst() {
            Some(Sev::Critical) => "есть критичные проблемы",
            Some(Sev::Warning) => "есть замечания",
            _ if !s.errors.is_empty() && s.findings.is_empty() => "не удалось проверить",
            _ if !s.errors.is_empty() => "проверено частично",
            Some(Sev::Info) => "есть советы",
            _ if s.skipped.is_some() => "пропущено",
            _ => "в порядке",
        };
        o.push(format!("  {}. {} — {}", i + 1, s.title.to_uppercase(), status));
        o.push(format!("  {}", "─".repeat(width(s.title) + 4)));
        if let Some(why) = &s.skipped {
            o.extend(wrap(&format!("Пропущено: {why}"), W, "     ", "     "));
        }
        if !s.facts.is_empty() {
            let kw = s.facts.iter().map(|(k, _)| width(k)).max().unwrap_or(0).min(26);
            for (k, v) in &s.facts {
                kv(&mut o, "     ", k, v, kw);
            }
            o.push(String::new());
        }
        let mut fs: Vec<&crate::model::Finding> = s.findings.iter().collect();
        fs.sort_by_key(|f| f.sev);
        for f in fs {
            let head = format!("     [{}] ", f.sev.tag());
            let pad = " ".repeat(width("     [КРИТИЧНО] "));
            o.extend(wrap(&f.title, W, &format!("{head:<w$}", w = width(&pad)), &pad));
            for d in &f.details {
                o.extend(wrap(d, W, &format!("{pad}  "), &format!("{pad}    ")));
            }
            if let Some(fix) = &f.fix {
                o.extend(wrap(fix, W, &format!("{pad}  Что сделать: "), &format!("{pad}               ")));
            }
        }
        for e in &s.errors {
            o.extend(wrap(&format!("Проверка не выполнена полностью: {e}"), W, "     (!) ", "         "));
        }
    }

    let appendix: Vec<&(String, Vec<String>)> = sections.iter().flat_map(|s| s.appendix.iter()).collect();
    if !appendix.is_empty() {
        o.push(String::new());
        o.push(rule('─'));
        o.push("  ПРИЛОЖЕНИЯ".into());
        o.push(rule('─'));
        for (i, (title, lines)) in appendix.iter().enumerate() {
            o.push(String::new());
            let letter = (b'A' + i as u8) as char;
            o.push(format!("  {letter}. {title} ({} {})", lines.len(), plural(lines.len() as u64, "запись", "записи", "записей")));
            for l in lines.iter() {
                o.extend(wrap(l, W, "     ", "       "));
            }
        }
    }

    o.push(String::new());
    o.push(rule('═'));
    o.push(format!("  VIL Doctor {} · отчёт создан автоматически. Советы носят рекомендательный характер;", meta.version));
    o.push("  перед серьёзными изменениями создайте точку восстановления.".into());
    o.push(rule('═'));

    let mut s = String::from("\u{feff}"); // BOM so every Notepad version picks UTF-8
    for line in o {
        s.push_str(line.trim_end());
        s.push_str("\r\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_summary_and_sections() {
        let mut a = Section::new("Диски");
        a.fact("C:", "10 ГБ свободно");
        a.add(Sev::Critical, "Мало места на C:").detail("Осталось 2%").fix("Очистите диск");
        a.ok("SMART в норме");
        let mut b = Section::new("Сеть");
        b.error("timeout");
        b.appendix.push(("Программы".into(), vec!["A 1.0".into()]));
        let meta = Meta { computer: "PC".into(), os: "Windows 11".into(), when: "now".into(), admin: true, deep: false, seconds: 3, version: "1.0" };
        let r = render(&meta, &[a, b]);
        assert!(r.starts_with('\u{feff}'));
        assert!(r.contains("Критично: 1"));
        assert!(r.contains("ЧТО СДЕЛАТЬ В ПЕРВУЮ ОЧЕРЕДЬ"));
        assert!(r.contains("→ Очистите диск"));
        assert!(r.contains("Что сделать: Очистите диск"));
        assert!(r.contains("не удалось проверить"));
        assert!(r.contains("A. Программы (1 запись)"));
        assert!(r.lines().all(|l| width(l.trim_start_matches('\u{feff}')) <= W), "line too wide");
        assert_eq!(totals(&[]).score, 100);
    }
}
