//! What a scanned code holds: web link, Wi-Fi network, contact, email,
//! phone, SMS, place, or plain text. Links that aren't http(s) are text, so
//! nothing like `javascript:` or `intent:` can ever be opened.

use fo_types::Payload;

/// Case-insensitive prefix strip.
fn strip<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    (s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix))
        .then(|| &s[prefix.len()..])
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `KEY:value;KEY:value;;` fields with `\` escaping `;` `:` `,` `\`.
fn fields(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = body.chars();
    let mut push = |cur: &mut String| {
        if let Some((k, v)) = split_unescaped(cur) {
            out.push((k.to_ascii_uppercase(), v));
        }
        cur.clear();
    };
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                cur.push('\\');
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            ';' => push(&mut cur),
            _ => cur.push(c),
        }
    }
    push(&mut cur);
    out
}

/// "K:v" split at the first unescaped ':'; the value unescaped.
fn split_unescaped(f: &str) -> Option<(String, String)> {
    let (k, v) = f.split_once(':')?;
    let mut val = String::new();
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                val.push(n);
            }
        } else {
            val.push(c);
        }
    }
    Some((k.trim().to_owned(), val))
}

fn web_link(s: &str) -> Option<String> {
    if s.chars().any(char::is_whitespace) {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if (lower.starts_with("http://") || lower.starts_with("https://")) && s.len() > 8 {
        return Some(s.to_owned());
    }
    if lower.starts_with("www.") && s[4..].contains('.') {
        return Some(format!("https://{s}"));
    }
    None
}

fn vcard(s: &str) -> Payload {
    let (mut name, mut org) = (String::new(), String::new());
    let (mut phones, mut emails) = (Vec::new(), Vec::new());
    let mut n_field = String::new();
    for line in s.lines() {
        let line = line.trim_end_matches('\r');
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.split(';').next().unwrap_or("").to_ascii_uppercase();
        let value = value.trim().to_owned();
        match key.as_str() {
            "FN" => name = value,
            "N" => n_field = value,
            "ORG" => org = value.replace(';', " ").trim().to_owned(),
            "TEL" => phones.push(value),
            "EMAIL" => emails.push(value),
            _ => {}
        }
    }
    if name.is_empty() {
        // N is "Family;Given;…".
        let parts: Vec<&str> = n_field.split(';').collect();
        name = [parts.get(1), parts.first()]
            .into_iter()
            .flatten()
            .filter(|p| !p.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
    }
    Payload::Contact {
        name,
        phones,
        emails,
        org,
        vcard: s.replace("\r\n", "\n").replace('\n', "\r\n"),
    }
}

fn mecard(body: &str) -> Payload {
    let (mut name, mut org) = (String::new(), String::new());
    let (mut phones, mut emails) = (Vec::new(), Vec::new());
    for (k, v) in fields(body) {
        match k.as_str() {
            // "Family,Given".
            "N" => {
                let mut p: Vec<&str> = v.split(',').map(str::trim).collect();
                p.reverse();
                name = p.join(" ").trim().to_owned();
            }
            "TEL" => phones.push(v),
            "EMAIL" => emails.push(v),
            "ORG" => org = v,
            _ => {}
        }
    }
    let mut card = format!("BEGIN:VCARD\r\nVERSION:3.0\r\nFN:{name}\r\n");
    if !org.is_empty() {
        card.push_str(&format!("ORG:{org}\r\n"));
    }
    for p in &phones {
        card.push_str(&format!("TEL:{p}\r\n"));
    }
    for e in &emails {
        card.push_str(&format!("EMAIL:{e}\r\n"));
    }
    card.push_str("END:VCARD\r\n");
    Payload::Contact {
        name,
        phones,
        emails,
        org,
        vcard: card,
    }
}

/// What `content` holds.
pub fn classify(content: &str) -> Payload {
    let s = content.trim();
    let text = || Payload::Text {
        text: content.to_owned(),
    };
    if let Some(url) = web_link(s) {
        return Payload::Url { url };
    }
    if let Some(body) = strip(s, "WIFI:") {
        let (mut ssid, mut password, mut security, mut hidden) =
            (String::new(), String::new(), String::new(), false);
        for (k, v) in fields(body) {
            match k.as_str() {
                "S" => ssid = v,
                "P" => password = v,
                "T" => security = v,
                "H" => hidden = v.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }
        if ssid.is_empty() {
            return text();
        }
        return Payload::Wifi {
            ssid,
            password,
            security,
            hidden,
        };
    }
    if strip(s, "BEGIN:VCARD").is_some() {
        return vcard(s);
    }
    if let Some(body) = strip(s, "MECARD:") {
        return mecard(body);
    }
    if let Some(rest) = strip(s, "mailto:") {
        let (to, query) = rest.split_once('?').unwrap_or((rest, ""));
        let (mut subject, mut body) = (String::new(), String::new());
        for pair in query.split('&') {
            match pair.split_once('=') {
                Some((k, v)) if k.eq_ignore_ascii_case("subject") => subject = percent_decode(v),
                Some((k, v)) if k.eq_ignore_ascii_case("body") => body = percent_decode(v),
                _ => {}
            }
        }
        return Payload::Email {
            to: percent_decode(to),
            subject,
            body,
        };
    }
    if let Some(n) = strip(s, "tel:") {
        return Payload::Phone {
            number: n.to_owned(),
        };
    }
    if let Some(rest) = strip(s, "SMSTO:").or_else(|| strip(s, "sms:")) {
        let (number, body) = rest.split_once(':').unwrap_or((rest, ""));
        return Payload::Sms {
            number: number.to_owned(),
            body: body.to_owned(),
        };
    }
    if let Some(rest) = strip(s, "geo:") {
        let coords = rest.split(['?', ';']).next().unwrap_or("");
        if let Some((a, b)) = coords.split_once(',') {
            if let (Ok(lat), Ok(lon)) = (
                a.trim().parse(),
                b.split(',').next().unwrap_or("").trim().parse(),
            ) {
                return Payload::Geo { lat, lon };
            }
        }
    }
    text()
}
