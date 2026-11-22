//! What a scanned code holds, and which actions are safe for it.
use fo_core::scan::payload::classify;
use fo_types::Payload;

#[test]
fn web_links_only() {
    assert_eq!(
        classify("https://example.org/a?b=1"),
        Payload::Url {
            url: "https://example.org/a?b=1".into()
        }
    );
    assert_eq!(
        classify("HTTP://EXAMPLE.ORG"),
        Payload::Url {
            url: "HTTP://EXAMPLE.ORG".into()
        }
    );
    assert_eq!(
        classify("www.example.org"),
        Payload::Url {
            url: "https://www.example.org".into()
        }
    );
    for bad in [
        "javascript:alert(1)",
        "intent://x#Intent;end",
        "file:///etc/passwd",
        "data:text/html,hi",
        "https://exa mple.org",
    ] {
        assert_eq!(classify(bad), Payload::Text { text: bad.into() }, "{bad}");
    }
}

#[test]
fn wifi_with_escapes() {
    assert_eq!(
        classify(r"WIFI:T:WPA;S:My\;Net;P:pa\:ss\\w;H:true;;"),
        Payload::Wifi {
            ssid: "My;Net".into(),
            password: r"pa:ss\w".into(),
            security: "WPA".into(),
            hidden: true
        }
    );
    assert_eq!(
        classify("WIFI:S:Cafe;T:nopass;;"),
        Payload::Wifi {
            ssid: "Cafe".into(),
            password: "".into(),
            security: "nopass".into(),
            hidden: false
        }
    );
}

#[test]
fn contacts_from_vcard_and_mecard() {
    let v = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Arefin Hossain\r\nORG:IIT\r\nTEL;TYPE=CELL:+880 1711 000000\r\nEMAIL:a@example.org\r\nEND:VCARD";
    match classify(v) {
        Payload::Contact {
            name,
            phones,
            emails,
            org,
            vcard,
        } => {
            assert_eq!(name, "Arefin Hossain");
            assert_eq!(phones, vec!["+880 1711 000000".to_string()]);
            assert_eq!(emails, vec!["a@example.org".to_string()]);
            assert_eq!(org, "IIT");
            assert!(vcard.starts_with("BEGIN:VCARD") && vcard.contains("FN:Arefin Hossain"));
        }
        other => panic!("{other:?}"),
    }
    match classify("MECARD:N:Alam,Tofazzol;TEL:01711;EMAIL:t@example.org;;") {
        Payload::Contact {
            name,
            phones,
            emails,
            vcard,
            ..
        } => {
            assert_eq!(name, "Tofazzol Alam");
            assert_eq!(
                (phones, emails),
                (vec!["01711".to_string()], vec!["t@example.org".to_string()])
            );
            assert!(vcard.contains("FN:Tofazzol Alam") && vcard.contains("TEL:01711"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn email_phone_sms_geo_and_text() {
    assert_eq!(
        classify("mailto:a@example.org?subject=Hello%20there&body=Hi"),
        Payload::Email {
            to: "a@example.org".into(),
            subject: "Hello there".into(),
            body: "Hi".into()
        }
    );
    assert_eq!(
        classify("tel:+8801711000000"),
        Payload::Phone {
            number: "+8801711000000".into()
        }
    );
    assert_eq!(
        classify("SMSTO:01711:See you at 5"),
        Payload::Sms {
            number: "01711".into(),
            body: "See you at 5".into()
        }
    );
    assert_eq!(
        classify("geo:23.7337,90.3929"),
        Payload::Geo {
            lat: 23.7337,
            lon: 90.3929
        }
    );
    assert_eq!(
        classify("Just some words"),
        Payload::Text {
            text: "Just some words".into()
        }
    );
    assert_eq!(
        classify("4006381333931"),
        Payload::Text {
            text: "4006381333931".into()
        }
    );
}
