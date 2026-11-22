//! Browser-preview stand-ins for the Scan tab: the camera, the document
//! scanner and the scan commands. `?qr=wifi|url|contact|text` picks what the
//! camera "sees"; `?noplay=1` acts like a phone without Play services.
//! Test hooks: `window.__fo_last_url`, `window.__fo_last_save`.

use fo_types::*;
use serde::Serialize;
use std::cell::RefCell;
use wasm_bindgen::JsValue;

thread_local! {
    static HISTORY: RefCell<Vec<ScanRecord>> = const { RefCell::new(Vec::new()) };
    static NEXT: RefCell<u64> = const { RefCell::new(1) };
}

const WIFI: &str = "WIFI:T:WPA;S:River House;P:heron-2026;;";
const URL: &str = "https://example.org/survey";
const CONTACT: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Maisha Mehjabin\r\nORG:IIT\r\nTEL:+880 1711 000000\r\nEMAIL:maisha@example.org\r\nEND:VCARD\r\n";
const TEXT: &str = "Meet at the boathouse at 7";

fn to_js<T: Serialize>(v: &T) -> JsValue {
    v.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}

fn arg<T: serde::de::DeserializeOwned>(args: &JsValue, key: &str) -> Option<T> {
    let v = js_sys::Reflect::get(args, &JsValue::from_str(key)).ok()?;
    serde_wasm_bindgen::from_value(v).ok()
}

fn hook(name: &str, v: &str) {
    if let Some(w) = web_sys::window() {
        let _ = js_sys::Reflect::set(&w, &JsValue::from_str(name), &JsValue::from_str(v));
    }
}

fn record(kind: ScanKind) -> ScanRecord {
    let id = NEXT.with(|n| {
        let mut n = n.borrow_mut();
        *n += 1;
        *n
    });
    let r = ScanRecord {
        id: id.to_string(),
        at: (js_sys::Date::now() / 1000.0) as u64,
        kind,
    };
    HISTORY.with(|h| h.borrow_mut().insert(0, r.clone()));
    r
}

fn classify(c: &str) -> Payload {
    match c {
        WIFI => Payload::Wifi {
            ssid: "River House".into(),
            password: "heron-2026".into(),
            security: "WPA".into(),
            hidden: false,
        },
        URL => Payload::Url { url: URL.into() },
        CONTACT => Payload::Contact {
            name: "Maisha Mehjabin".into(),
            phones: vec!["+880 1711 000000".into()],
            emails: vec!["maisha@example.org".into()],
            org: "IIT".into(),
            vcard: CONTACT.into(),
        },
        other => Payload::Text { text: other.into() },
    }
}

fn page_svg(n: usize) -> String {
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 210 297'><rect width='210' height='297' fill='%23fbfbf7'/>\
         <text x='20' y='40' font-size='16' font-family='serif'>Field notes, page {n}</text>\
         <g stroke='%23999' stroke-width='3'><path d='M20 70h170M20 90h150M20 110h165M20 130h120M20 170h170M20 190h140'/></g></svg>"
    );
    format!("data:image/svg+xml;utf8,{svg}")
}

async fn wait(ms: i32) {
    let p = js_sys::Promise::new(&mut |res, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&res, ms);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
}

pub async fn invoke(cmd: &str, args: &JsValue) -> Option<Result<JsValue, String>> {
    let q = crate::mock::query;
    Some(Ok(match cmd {
        "plugin:fo-android|scanner_available" => {
            to_js(&serde_json::json!({ "available": q("noplay").is_none() }))
        }
        "plugin:barcode-scanner|request_permissions" => {
            to_js(&serde_json::json!({ "camera": "granted" }))
        }
        "plugin:barcode-scanner|scan" => {
            wait(700).await;
            let content = match q("qr").as_deref() {
                Some("url") => URL,
                Some("contact") => CONTACT,
                Some("text") => TEXT,
                _ => WIFI,
            };
            to_js(&serde_json::json!({ "content": content, "format": "QR_CODE" }))
        }
        "plugin:barcode-scanner|cancel" | "plugin:barcode-scanner|open_app_settings" => {
            JsValue::NULL
        }
        "plugin:fo-android|add_wifi" => to_js(&serde_json::json!({ "mode": "dialog" })),
        "scan_pages_dir" => to_js(&"/data/app/scans/files/1"),
        "plugin:fo-android|scan_document" => {
            if q("noplay").is_some() {
                return Some(Err("unavailable".into()));
            }
            let pages: Vec<String> = (1..=3)
                .map(|n| format!("/data/app/scans/files/1/page-{n}.jpg"))
                .collect();
            to_js(&serde_json::json!({ "pages": pages }))
        }
        "scan_thumbnail" => {
            let p: String = arg(args, "path").unwrap_or_default();
            let n = p
                .rsplit('-')
                .next()
                .and_then(|s| s.trim_end_matches(".jpg").parse().ok())
                .unwrap_or(1);
            to_js(&page_svg(n))
        }
        "scan_save_document" => {
            let order: Vec<usize> = arg(args, "order").unwrap_or_default();
            let stem: String = arg(args, "stem").unwrap_or_default();
            let pdf: bool = arg(args, "pdf").unwrap_or(true);
            let jpegs: bool = arg(args, "jpegs").unwrap_or(false);
            hook(
                "__fo_last_save",
                &serde_json::json!({ "order": order, "pdf": pdf, "jpegs": jpegs }).to_string(),
            );
            let dir = "/home/you/Documents/FileOdyssey";
            let mut files = Vec::new();
            if pdf {
                files.push(format!("{dir}/{stem}.pdf"));
            }
            if jpegs {
                files.extend((1..=order.len()).map(|n| format!("{dir}/{stem} (page {n}).jpg")));
            }
            record(ScanKind::Document {
                files: files.iter().map(Into::into).collect(),
            });
            to_js(&files)
        }
        "scan_record_code" => {
            let content: String = arg(args, "content").unwrap_or_default();
            let format: String = arg(args, "format").unwrap_or_default();
            to_js(&record(ScanKind::Code { content, format }))
        }
        "scan_classify" => {
            let c: String = arg(args, "content").unwrap_or_default();
            to_js(&classify(&c))
        }
        "scan_codes_in_image" => to_js(&vec![TEXT]),
        "scan_history" => to_js(&HISTORY.with(|h| h.borrow().clone())),
        "scan_delete" => {
            let id: String = arg(args, "id").unwrap_or_default();
            HISTORY.with(|h| h.borrow_mut().retain(|r| r.id != id));
            JsValue::NULL
        }
        "scan_clear" => {
            HISTORY.with(|h| h.borrow_mut().clear());
            JsValue::NULL
        }
        "scan_open_url" => {
            let u: String = arg(args, "url").unwrap_or_default();
            hook("__fo_last_url", &u);
            JsValue::NULL
        }
        "scan_save_contact" => to_js(&"/home/you/Documents/FileOdyssey/Maisha Mehjabin.vcf"),
        _ => return None,
    }))
}
