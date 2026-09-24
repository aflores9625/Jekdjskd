use super::{collect, sysroot};
use crate::model::*;
use crate::text::{parse_http_date, unix_now};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const TITLE: &str = "Сеть и интернет";

const PS: &str = r##"
$ad = @(Get-NetAdapter | Where-Object Status -eq 'Up' | ForEach-Object { [ordered]@{ name = $_.Name; desc = $_.InterfaceDescription; speed = "$($_.LinkSpeed)" } })
$dns = @(Get-DnsClientServerAddress -AddressFamily IPv4 | Where-Object { $_.ServerAddresses } | ForEach-Object { $_.ServerAddresses } | Select-Object -Unique)
$is = Get-ItemProperty 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Internet Settings'
$r = [ordered]@{ adapters = $ad; dns = $dns; proxy_on = $is.ProxyEnable; proxy = $is.ProxyServer; pac = $is.AutoConfigURL }
"##;

const TIMEOUT: Duration = Duration::from_secs(5);

fn resolve(host: &str) -> Result<(Vec<SocketAddr>, Duration), String> {
    let (tx, rx) = mpsc::channel();
    let h = host.to_string();
    let start = Instant::now();
    std::thread::spawn(move || {
        let _ = tx.send((h.as_str(), 80).to_socket_addrs().map(|a| a.collect::<Vec<_>>()));
    });
    match rx.recv_timeout(TIMEOUT) {
        Ok(Ok(a)) if !a.is_empty() => Ok((a, start.elapsed())),
        Ok(Ok(_)) => Err("пустой ответ".into()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("нет ответа за 5 с".into()),
    }
}

/// Plain-HTTP probe; returns (body contains marker, server Date header).
fn http_probe(addr: SocketAddr, host: &str, path: &str) -> Result<(String, Option<i64>), String> {
    let mut st = TcpStream::connect_timeout(&addr, TIMEOUT).map_err(|e| e.to_string())?;
    st.set_read_timeout(Some(TIMEOUT)).ok();
    st.set_write_timeout(Some(TIMEOUT)).ok();
    write!(st, "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: VIL-Doctor\r\nConnection: close\r\n\r\n").map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    let _ = st.take(64 * 1024).read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf).into_owned();
    let date = text
        .lines()
        .take_while(|l| !l.trim().is_empty())
        .find_map(|l| l.strip_prefix("Date:").or_else(|| l.strip_prefix("date:")))
        .and_then(|d| parse_http_date(d.trim()));
    Ok((text, date))
}

pub fn run(_ctx: &Ctx) -> Section {
    let mut s = Section::new(TITLE);
    let v = collect(&mut s, PS, 45);

    if let Some(v) = &v {
        let ads = v.list("adapters");
        for a in &ads {
            s.fact(&format!("Подключение «{}»", a.s("name").unwrap_or_default()), format!("{}, {}", a.s("desc").unwrap_or_default(), a.s("speed").unwrap_or_default()));
        }
        let dns = str_list(v, "dns");
        s.fact("DNS-серверы", dns.join(", "));
        if ads.is_empty() && v.has("adapters") {
            s.add(Sev::Critical, "Нет ни одного активного сетевого подключения")
                .fix("Проверьте кабель или Wi-Fi, включите адаптер: Параметры → Сеть и Интернет. Если адаптера нет в списке — установите драйвер.");
        }
        if v.n("proxy_on").unwrap_or(0.0) >= 1.0 || v.s("pac").is_some() {
            let what = v.s("proxy").or(v.s("pac")).unwrap_or_default();
            s.add(Sev::Info, format!("Включён прокси-сервер: {what}"))
                .detail("Если вы его не настраивали — так вредоносные программы перехватывают трафик.")
                .fix("Параметры → Сеть и Интернет → Прокси-сервер: отключите ненужные настройки.");
        }
    }

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(TcpStream::connect_timeout(&"1.1.1.1:443".parse().unwrap(), TIMEOUT).is_ok());
    });
    let raw_ip = rx.recv_timeout(TIMEOUT + Duration::from_secs(1)).unwrap_or(false);

    match resolve("www.msftconnecttest.com") {
        Ok((addrs, took)) => {
            s.fact("Время ответа DNS", format!("{} мс", took.as_millis()));
            if took > Duration::from_millis(800) {
                s.add(Sev::Info, format!("DNS отвечает медленно: {} мс", took.as_millis()))
                    .fix("Можно указать быстрые DNS (например 1.1.1.1 и 8.8.8.8) в свойствах подключения.");
            }
            let addr = addrs.iter().find(|a| a.is_ipv4()).copied().unwrap_or(addrs[0]);
            let started = Instant::now();
            match http_probe(addr, "www.msftconnecttest.com", "/connecttest.txt") {
                Ok((body, date)) if body.contains("Microsoft Connect Test") => {
                    s.ok(format!("Интернет работает (ответ за {} мс)", started.elapsed().as_millis()));
                    if let Some(server) = date {
                        let skew = unix_now() - server;
                        if skew.abs() > 120 {
                            s.add(Sev::Warning, format!("Системные часы сбиты на {} мин.", skew.abs() / 60))
                                .fix("Параметры → Время и язык → Дата и время → включите «Установить время автоматически» и нажмите «Синхронизировать». Из-за неверного времени не открываются сайты и не работают обновления.");
                        } else {
                            s.ok("Системное время точное");
                        }
                    }
                }
                Ok(_) => {
                    s.add(Sev::Warning, "Доступ в интернет перехватывается (страница входа Wi-Fi, прокси или фильтр)")
                        .fix("Откройте браузер — возможно, сеть требует авторизации. Проверьте настройки прокси и антивируса.");
                }
                Err(e) => {
                    s.add(Sev::Warning, format!("Сайт проверки связи Microsoft недоступен: {e}"))
                        .fix("Проверьте брандмауэр/антивирус и VPN: они могут блокировать соединения.");
                }
            }
        }
        Err(e) if raw_ip => {
            s.add(Sev::Critical, format!("Интернет есть, но не работает DNS: {e}"))
                .fix("Командная строка администратора: ipconfig /flushdns, затем укажите DNS 1.1.1.1 и 8.8.8.8 в свойствах подключения.");
        }
        Err(e) => {
            s.add(Sev::Critical, format!("Нет доступа в интернет ({e})"))
                .fix("Перезагрузите роутер и компьютер. Если не помогло — Параметры → Сеть и Интернет → Дополнительные параметры → «Сброс сети».");
        }
    }

    let hosts_path = format!(r"{}\System32\drivers\etc\hosts", sysroot());
    if !crate::paths::fs_exists(&hosts_path) {
        s.add(Sev::Info, "Файл hosts отсутствует").fix("Windows работает и без него, но некоторые программы ожидают этот файл. Его можно создать пустым.");
    }
    if let Ok(bytes) = std::fs::read(&hosts_path) {
        let text = String::from_utf8_lossy(&bytes);
        let entries: Vec<String> = text
            .lines()
            .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
            .filter(|l| !l.is_empty())
            .filter(|l| {
                let low = l.to_ascii_lowercase();
                !(low.ends_with(" localhost") || low.ends_with("\tlocalhost") || low.contains("localhost.localdomain"))
            })
            .collect();
        let sensitive = ["microsoft", "windowsupdate", "google", "yandex", "vk.com", "mail.ru", "kaspersky", "drweb", "eset", "avast", "avira", "malwarebytes", "bitdefender", "steam"];
        let bad: Vec<String> = entries.iter().filter(|e| sensitive.iter().any(|w| e.to_ascii_lowercase().contains(w))).cloned().collect();
        if !bad.is_empty() {
            s.add(Sev::Warning, format!("Файл hosts перенаправляет известные сайты: {}", bad.len()))
                .detail("Так вирусы блокируют антивирусы и обновления или подменяют сайты.")
                .list(bad, 10)
                .fix(format!("Если вы не добавляли эти строки сами — откройте Блокнот от имени администратора и удалите их из {hosts_path}."));
        } else if !entries.is_empty() {
            s.add(Sev::Info, format!("В файле hosts есть пользовательские записи: {}", entries.len()))
                .list(entries, 8)
                .fix("Убедитесь, что вы добавляли их сами.");
        }
    }
    s
}
