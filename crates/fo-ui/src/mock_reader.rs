//! Browser-preview stand-ins for the reader commands and `reader://` URLs.
//! Pages are drawn as SVG with the same text the mock text layer returns,
//! so selection and search can be tried without the app.

use fo_types::*;
use serde::Serialize;
use wasm_bindgen::JsValue;

const PDF_PAGES: usize = 12;
const LANDSCAPE: usize = 4;

const LINES: [&str; 14] = [
    "Revenue grew in every region this quarter, led by the river",
    "valley stores, where the new opening hours drew more evening",
    "visitors than any season before. Costs stayed close to plan.",
    "",
    "The warehouse move finished two weeks early. Deliveries now",
    "leave from one building, which shortens the morning route and",
    "frees the old site for the community workshop in spring.",
    "",
    "Next quarter we expect slower sales while the bridge is under",
    "repair. The team will use the quiet weeks to train new staff",
    "and to finish the stock count that was paused in the summer.",
    "",
    "Questions about these figures can go to the finance desk; the",
    "full tables are in the appendix at the end of this report.",
];

fn page_size(i: usize) -> PageSize {
    if i == LANDSCAPE {
        PageSize { w: 842.0, h: 595.0 }
    } else {
        PageSize { w: 595.0, h: 842.0 }
    }
}

fn heading(i: usize) -> String {
    match i {
        0 => "Quarterly report".into(),
        LANDSCAPE => "Sales by region".into(),
        n => format!("Section {n}"),
    }
}

/// (text, x, y, w, h) as page fractions.
fn line_spans(i: usize) -> Vec<(String, f32, f32, f32, f32)> {
    let s = page_size(i);
    let (lh, fs) = (18.0 / s.h, 11.0 / s.h);
    let head = heading(i);
    let hw = head.len() as f32 * 12.0 / s.w;
    let mut out = vec![(head, 0.12, 0.08, hw, 26.0 / s.h)];
    for (n, l) in LINES.iter().enumerate() {
        if l.is_empty() {
            continue;
        }
        let w = l.len() as f32 * 5.6 / s.w;
        out.push((l.to_string(), 0.12, 0.16 + n as f32 * lh, w, fs * 1.25));
    }
    out
}

fn svg_page(i: usize) -> String {
    let s = page_size(i);
    let mut body = String::new();
    for (n, (t, x, y, w, h)) in line_spans(i).into_iter().enumerate() {
        let (size, weight) = if n == 0 { (22.0, 700) } else { (11.0, 400) };
        // textLength makes the drawn text exactly as wide as the mock span.
        body.push_str(&format!(
            r##"<text x="{}" y="{}" textLength="{}" lengthAdjust="spacingAndGlyphs" font-size="{size}" font-weight="{weight}" font-family="Georgia, serif" fill="#222">{}</text>"##,
            x * s.w,
            (y + h * 0.85) * s.h,
            w * s.w,
            fo_ui_escape(&t)
        ));
    }
    if i == LANDSCAPE {
        for r in 0..6 {
            for c in 0..5 {
                body.push_str(&format!(
                    r##"<rect x="{}" y="{}" width="128" height="34" fill="{}" stroke="#bbb"/>"##,
                    100 + c * 130,
                    330 + r * 36,
                    if r == 0 { "#e8ece4" } else { "#fff" }
                ));
            }
        }
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}"><rect width="100%" height="100%" fill="#fff"/>{body}<text x="{cx}" y="{fy}" font-size="10" fill="#888" text-anchor="middle">{n}</text></svg>"##,
        w = s.w,
        h = s.h,
        cx = s.w / 2.0,
        fy = s.h - 30.0,
        n = i + 1
    )
}

fn fo_ui_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn photo_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1600 1067"><defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f2c27a"/><stop offset=".55" stop-color="#c9805a"/><stop offset="1" stop-color="#5b4a63"/></linearGradient><linearGradient id="w" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#7d6f86"/><stop offset="1" stop-color="#2c3346"/></linearGradient></defs><rect width="1600" height="640" fill="url(#s)"/><circle cx="1080" cy="560" r="90" fill="#ffe2a8" opacity=".9"/><path d="M0 560C200 520 380 600 620 560S1100 500 1600 580V700H0Z" fill="#3c4a3e"/><rect y="640" width="1600" height="427" fill="url(#w)"/><path d="M1040 700h80M1010 760h140M1050 830h60" stroke="#ffd79a" stroke-width="6" opacity=".6"/></svg>"##.into()
}

fn small_svg(label: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 300"><rect width="600" height="300" rx="24" fill="#cfd9cc"/><path d="M40 240C160 120 260 200 340 150S520 90 560 120" stroke="#4E7A4A" stroke-width="10" fill="none"/><text x="40" y="60" font-size="28" fill="#1E3A2F" font-family="Georgia, serif">{label}</text></svg>"##
    )
}

fn data_url(svg: &str) -> String {
    format!(
        "data:image/svg+xml;charset=utf-8,{}",
        String::from(js_sys::encode_uri_component(svg))
    )
}

/// Preview version of `reader://{path}`.
pub fn url(path: &str) -> String {
    let route = path.split_once('/').map(|(_, r)| r).unwrap_or("");
    if let Some(p) = route.strip_prefix("page/") {
        let n: usize = p
            .split('?')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        return data_url(&svg_page(n));
    }
    if route == "image" {
        return data_url(&photo_svg());
    }
    data_url(&small_svg("A map of the walk"))
}

const CHAPTERS: [&str; 4] = [
    "One: The valley road",
    "",
    "Two: Crossing at dusk",
    "Three: Home by the river",
];

fn chapter_html(i: usize) -> String {
    let para = "<p>The path left the village before the fog had lifted, and for the first hour there was nothing to see but the hedges and the grey shape of the hill. Somewhere ahead a dog barked twice and was quiet.</p><p>By noon the sun had burned through. The river showed itself in pieces between the alders, bright and slow, and the walkers stopped where the bank flattened into a shelf of warm stones.</p>";
    match i {
        0 => format!("<h1 id=\"fo-top\">The valley road</h1>{para}<p>They would cross at the ford, if the water allowed it; see <a href=\"fo-ch:2#fo-ford\">the crossing</a>, or read about the old <a href=\"fo-ext:https://example.org/ferries\">river ferries</a>.</p><img data-fo-src=\"OEBPS/img/map.svg\" alt=\"A sketch map of the walk\">{para}"),
        1 => format!("<p>Later, they argued about which bridge it had been.</p>{para}"),
        2 => format!("<h1>Crossing at dusk</h1>{para}<h2 id=\"fo-ford\">The ford</h2>{para}<blockquote><p>Cold to the knee, and then to the waist, and then it was over.</p></blockquote>"),
        _ => format!("<h1>Home by the river</h1>{para}<p>Back to <a href=\"fo-ch:0\">the start</a>.</p>"),
    }
}

fn docx_html() -> String {
    "<h1 id=\"fo-quarterly-river-report\">ত্রৈমাসিক প্রতিবেদন</h1><p>এই প্রতিবেদনে বছরের প্রথম তিন মাসের নদী জরিপের ফলাফল দেওয়া হয়েছে। পানির স্তর প্রতি মাসে বেড়েছে।</p><h2>Readings</h2><table><thead><tr><th>Month</th><th>Level</th><th>Note</th></tr></thead><tbody><tr><td>March</td><td>2.1</td><td>Clear</td></tr><tr><td>April</td><td>2.9</td><td>After rain</td></tr></tbody></table><p>Full data is on the <a href=\"fo-ext:https://example.org/data\">survey website</a>.</p>".into()
}

fn md_html() -> String {
    "<h1 id=\"fo-meeting-notes\">Meeting notes</h1><p><strong>Present:</strong> Rumana, Tanvir, Sadia</p><h2>Decisions</h2><ul><li>Move the stock count to the first week of March.</li><li>Keep the evening hours at the river valley store.</li></ul><h2>Numbers</h2><table><thead><tr><th>Store</th><th>Visitors</th></tr></thead><tbody><tr><td>Valley</td><td>4,210</td></tr><tr><td>Bridge</td><td>2,980</td></tr></tbody></table><pre><code>total = valley + bridge</code></pre><p>Agenda for next time: <a href=\"fo-ext:https://example.org/agenda\">shared agenda</a>.</p>".into()
}

fn to_js<T: Serialize>(v: &T) -> JsValue {
    v.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}

fn arg<T: serde::de::DeserializeOwned>(args: &JsValue, key: &str) -> Option<T> {
    let v = js_sys::Reflect::get(args, &JsValue::from_str(key)).ok()?;
    serde_wasm_bindgen::from_value(v).ok()
}

fn name_of(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}

/// Handles reader commands; `None` for anything else.
pub fn invoke(cmd: &str, args: &JsValue) -> Option<Result<JsValue, String>> {
    Some(match cmd {
        "reader_open" => {
            let path: String = arg(args, "path").unwrap_or_default();
            let name = name_of(&path);
            let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            let (doc, body) = match ext.as_str() {
                "pdf" => (
                    1,
                    ReaderBody::Pdf(PdfInfo {
                        pages: (0..PDF_PAGES).map(page_size).collect(),
                        title: Some("Quarterly report".into()),
                    }),
                ),
                "epub" => (
                    2,
                    ReaderBody::Book(BookInfo {
                        title: Some("The Long Walk".into()),
                        chapters: CHAPTERS.iter().map(|s| s.to_string()).collect(),
                    }),
                ),
                "md" => (3, ReaderBody::Page(md_html())),
                "jpg" | "jpeg" | "png" => (
                    4,
                    ReaderBody::Image(ImageInfo {
                        width: 1600,
                        height: 1067,
                        exif: [
                            ("Camera", "Fujifilm X-T30"),
                            ("Lens", "XF18-55mm F2.8-4 R LM OIS"),
                            ("Taken", "2026-03-14 18:42:10"),
                            ("Exposure", "1/250 s"),
                            ("Aperture", "f/5.6"),
                            ("ISO", "200"),
                            ("Focal length", "35 mm"),
                            ("Location", "23.72981, 90.40101"),
                        ]
                        .iter()
                        .map(|(a, b)| (a.to_string(), b.to_string()))
                        .collect(),
                    }),
                ),
                "docx" | "odt" | "rtf" => (5, ReaderBody::Page(docx_html())),
                _ => return Some(Err("FileOdyssey can't read this kind of file.".into())),
            };
            Ok(to_js(&ReaderView { doc, name, body }))
        }
        "reader_chapter" => {
            let i: usize = arg(args, "index").unwrap_or(0);
            Ok(to_js(&chapter_html(i)))
        }
        "reader_text" => {
            let page: usize = arg(args, "page").unwrap_or(0);
            let spans: Vec<TextSpan> = line_spans(page)
                .into_iter()
                .map(|(text, x, y, w, h)| TextSpan {
                    text,
                    rect: Rect { x, y, w, h },
                })
                .collect();
            Ok(to_js(&spans))
        }
        "reader_search" => {
            let q: String = arg::<String>(args, "query")
                .unwrap_or_default()
                .to_lowercase();
            let mut hits = Vec::new();
            if !q.trim().is_empty() {
                for page in 0..PDF_PAGES {
                    for (text, x, y, w, h) in line_spans(page) {
                        let lower = text.to_lowercase();
                        if let Some(at) = lower.find(&q) {
                            let per = w / text.len().max(1) as f32;
                            hits.push(SearchHit {
                                page: page as u32,
                                rects: vec![Rect {
                                    x: x + per * at as f32,
                                    y,
                                    w: per * q.len() as f32,
                                    h,
                                }],
                            });
                        }
                    }
                }
            }
            Ok(to_js(&hits))
        }
        "reader_close" => Ok(JsValue::NULL),
        _ => return None,
    })
}
