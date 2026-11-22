//! The Scan tab (Android): documents through ML Kit, QR codes and barcodes
//! through the camera (the page goes see-through over the camera preview),
//! codes from image files, and the scan history.

use super::{file_name, AppCtx, NoArgs};
use crate::bridge::{camera_mode, copy_text, invoke};
use fo_types::{Category, Payload, ScanKind, ScanRecord};
use fo_ui::strings::*;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

/// Every format the camera scanner knows.
const FORMATS: &[&str] = &[
    "QR_CODE",
    "EAN_13",
    "EAN_8",
    "UPC_A",
    "UPC_E",
    "CODE_128",
    "CODE_39",
    "CODE_93",
    "CODABAR",
    "ITF",
    "DATA_MATRIX",
    "PDF_417",
    "AZTEC",
];

#[derive(Clone, PartialEq)]
enum Screen {
    Home,
    Camera,
    /// Pages from the scanner; `order` is what's shown, by index.
    Review {
        pages: Vec<String>,
        order: Vec<usize>,
    },
    Saved {
        files: Vec<String>,
    },
}

#[derive(Serialize)]
struct ScanArgs {
    windowed: bool,
    formats: Vec<&'static str>,
}

#[derive(Deserialize)]
struct Scanned {
    content: String,
    #[serde(default)]
    format: String,
}

#[derive(Deserialize)]
struct Perm {
    camera: String,
}

#[derive(Deserialize)]
struct Available {
    available: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DocArgs {
    dir: String,
    page_limit: u32,
}

#[derive(Deserialize)]
struct DocPages {
    pages: Vec<String>,
}

#[derive(Serialize)]
struct Content {
    content: String,
}

#[derive(Serialize)]
struct Record {
    content: String,
    format: String,
}

#[derive(Serialize)]
struct PathArg {
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveDoc {
    pages: Vec<String>,
    order: Vec<usize>,
    out_dir: String,
    stem: String,
    pdf: bool,
    jpegs: bool,
}

#[derive(Serialize)]
struct Id {
    id: String,
}

#[derive(Serialize)]
struct Url {
    url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Contact {
    vcard: String,
    out_dir: String,
}

#[derive(Serialize)]
struct Wifi {
    ssid: String,
    password: String,
    security: String,
}

#[derive(Deserialize)]
struct WifiMode {
    mode: String,
}

fn cancelled(e: &str) -> bool {
    e.to_lowercase().contains("cancel")
}

/// "Scan 2026-11-22 14.05", in local time.
fn stem_now() -> String {
    let d = js_sys::Date::new_0();
    format!(
        "Scan {:04}-{:02}-{:02} {:02}.{:02}",
        d.get_full_year(),
        d.get_month() + 1,
        d.get_date(),
        d.get_hours(),
        d.get_minutes()
    )
}

fn when(at: u64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(at as f64 * 1000.0));
    String::from(d.to_locale_string("default", &wasm_bindgen::JsValue::UNDEFINED))
}

#[component]
pub fn Scan() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let screen = RwSignal::new(Screen::Home);
    let result = RwSignal::new(None::<Payload>);
    let notice = RwSignal::new(None::<String>);
    let history = RwSignal::new(Vec::<ScanRecord>::new());
    let scanner_ok = RwSignal::new(true);
    let thumbs = RwSignal::new(std::collections::HashMap::<String, String>::new());
    let want_pdf = RwSignal::new(true);
    let want_jpegs = RwSignal::new(false);
    let copied = RwSignal::new(false);

    let reload = move || {
        spawn_local(async move {
            if let Ok(h) = invoke::<_, Vec<ScanRecord>>("scan_history", &NoArgs {}).await {
                history.set(h);
            }
        });
    };
    reload();
    spawn_local(async move {
        if let Ok(a) =
            invoke::<_, Available>("plugin:fo-android|scanner_available", &NoArgs {}).await
        {
            scanner_ok.set(a.available);
        }
    });

    let show_payload = move |content: String| {
        spawn_local(async move {
            copied.set(false);
            if let Ok(p) = invoke::<_, Payload>("scan_classify", &Content { content }).await {
                result.set(Some(p));
            }
        });
    };

    let scan_code = move |_| {
        notice.set(None);
        spawn_local(async move {
            let perm: Result<Perm, String> =
                invoke("plugin:barcode-scanner|request_permissions", &NoArgs {}).await;
            if !matches!(&perm, Ok(p) if p.camera == "granted") {
                notice.set(Some(NEEDS_CAMERA.into()));
                return;
            }
            screen.set(Screen::Camera);
            camera_mode(true);
            let r: Result<Scanned, String> = invoke(
                "plugin:barcode-scanner|scan",
                &ScanArgs {
                    windowed: true,
                    formats: FORMATS.to_vec(),
                },
            )
            .await;
            camera_mode(false);
            screen.set(Screen::Home);
            match r {
                Ok(s) => {
                    let _ = invoke::<_, ScanRecord>(
                        "scan_record_code",
                        &Record {
                            content: s.content.clone(),
                            format: s.format,
                        },
                    )
                    .await;
                    reload();
                    show_payload(s.content);
                }
                Err(e) if cancelled(&e) => {}
                Err(e) => notice.set(Some(e)),
            }
        });
    };
    let cancel_camera = move |_| {
        spawn_local(async move {
            let _ = invoke::<_, ()>("plugin:barcode-scanner|cancel", &NoArgs {}).await;
        });
    };

    let scan_document = move |_| {
        notice.set(None);
        spawn_local(async move {
            let Ok(dir) = invoke::<_, String>("scan_pages_dir", &NoArgs {}).await else {
                return;
            };
            let r: Result<DocPages, String> = invoke(
                "plugin:fo-android|scan_document",
                &DocArgs { dir, page_limit: 0 },
            )
            .await;
            match r {
                Ok(d) if !d.pages.is_empty() => {
                    let order = (0..d.pages.len()).collect();
                    for p in d.pages.clone() {
                        spawn_local(async move {
                            if let Ok(t) =
                                invoke::<_, String>("scan_thumbnail", &PathArg { path: p.clone() })
                                    .await
                            {
                                thumbs.update(|m| {
                                    m.insert(p, t);
                                });
                            }
                        });
                    }
                    screen.set(Screen::Review {
                        pages: d.pages,
                        order,
                    });
                }
                Ok(_) => {}
                Err(e) if cancelled(&e) => {}
                Err(e) => notice.set(Some(
                    if e.contains("unavailable") || !scanner_ok.get_untracked() {
                        NEEDS_PLAY.into()
                    } else {
                        e
                    },
                )),
            }
        });
    };

    let read_image = move |_| {
        notice.set(None);
        spawn_local(async move {
            let picked: Vec<String> = invoke("pick_files", &NoArgs {}).await.unwrap_or_default();
            let Some(path) = picked.into_iter().next() else {
                return;
            };
            match invoke::<_, Vec<String>>("scan_codes_in_image", &PathArg { path }).await {
                Ok(codes) if !codes.is_empty() => {
                    let c = codes[0].clone();
                    let _ = invoke::<_, ScanRecord>(
                        "scan_record_code",
                        &Record {
                            content: c.clone(),
                            format: "IMAGE".into(),
                        },
                    )
                    .await;
                    reload();
                    show_payload(c);
                }
                Ok(_) => notice.set(Some(NO_CODE_FOUND.into())),
                Err(e) => notice.set(Some(e)),
            }
        });
    };

    let move_page = move |i: usize, delta: isize| {
        screen.update(|s| {
            if let Screen::Review { order, .. } = s {
                let j = i as isize + delta;
                if j >= 0 && (j as usize) < order.len() {
                    order.swap(i, j as usize);
                }
            }
        });
    };
    let remove_page = move |i: usize| {
        screen.update(|s| {
            if let Screen::Review { order, .. } = s {
                order.remove(i);
            }
        });
    };
    let save = move |_| {
        let Screen::Review { pages, order } = screen.get_untracked() else {
            return;
        };
        spawn_local(async move {
            let args = SaveDoc {
                pages,
                order,
                out_dir: ctx.out_dir.get_untracked(),
                stem: stem_now(),
                pdf: want_pdf.get_untracked(),
                jpegs: want_jpegs.get_untracked(),
            };
            match invoke::<_, Vec<String>>("scan_save_document", &args).await {
                Ok(files) => {
                    reload();
                    screen.set(Screen::Saved { files });
                }
                Err(e) => notice.set(Some(e)),
            }
        });
    };

    let open_files = move |files: Vec<String>, read: bool| {
        let pick = files
            .iter()
            .find(|f| f.to_lowercase().ends_with(".pdf"))
            .or(files.first())
            .cloned();
        if let Some(f) = pick {
            if read {
                ctx.open_reader(f);
            } else {
                ctx.tab.set(Category::Convert);
                ctx.add_paths(vec![f]);
            }
        }
    };

    view! {
        <section class="scan">
            <Show when=move || screen.get() == Screen::Camera>
                <div class="camera-overlay" role="dialog" aria-label=SCAN_CODE>
                    <p class="camera-hint">{SCAN_POINT}</p>
                    <div class="camera-frame" aria-hidden="true"></div>
                    <button class="primary" on:click=cancel_camera>{CANCEL}</button>
                </div>
            </Show>
            {move || match screen.get() {
                Screen::Review { pages, order } => {
                    let n = order.len();
                    view! {
                        <div class="review">
                            <h2>{REVIEW_PAGES} <span class="muted">{pages_n(n)}</span></h2>
                            <ol class="pages">
                                {order.iter().enumerate().map(|(i, &p)| {
                                    let path = pages[p].clone();
                                    let path2 = path.clone();
                                    view! {
                                        <li class="page">
                                            <img alt=page_n(i + 1) src=move || thumbs.get().get(&path).cloned().unwrap_or_default()/>
                                            <span class="page-n">{page_n(i + 1)}</span>
                                            <span class="page-tools">
                                                <button class="link" disabled=i == 0 on:click=move |_| move_page(i, -1)>{MOVE_UP}</button>
                                                <button class="link" disabled=i + 1 == n on:click=move |_| move_page(i, 1)>{MOVE_DOWN}</button>
                                                <button class="link" on:click=move |_| remove_page(i) data-page=path2.clone()>{REMOVE}</button>
                                            </span>
                                        </li>
                                    }
                                }).collect_view()}
                            </ol>
                            <fieldset class="save-as">
                                <legend>{SAVE_AS}</legend>
                                <label><input type="checkbox" prop:checked=move || want_pdf.get() on:change=move |e| want_pdf.set(event_target_checked(&e))/> {AS_PDF}</label>
                                <label><input type="checkbox" prop:checked=move || want_jpegs.get() on:change=move |e| want_jpegs.set(event_target_checked(&e))/> {AS_IMAGES}</label>
                            </fieldset>
                            <div class="review-actions">
                                <button class="link" on:click=move |_| screen.set(Screen::Home)>{SCAN_MORE}</button>
                                <button class="primary" on:click=save
                                    disabled=move || n == 0 || !(want_pdf.get() || want_jpegs.get())>{SAVE}</button>
                            </div>
                        </div>
                    }.into_any()
                }
                Screen::Saved { files } => {
                    let f1 = files.clone();
                    let f2 = files.clone();
                    view! {
                        <div class="saved">
                            <h2>{SAVED}</h2>
                            <ul>{files.iter().map(|f| view! { <li>{file_name(f)}</li> }).collect_view()}</ul>
                            <div class="review-actions">
                                <button class="link" on:click=move |_| screen.set(Screen::Home)>{CLOSE}</button>
                                <button class="link" on:click=move |_| open_files(f1.clone(), false)>{SEND_TO_CONVERT}</button>
                                <button class="primary" on:click=move |_| open_files(f2.clone(), true)>{OPEN_IN_READER}</button>
                            </div>
                        </div>
                    }.into_any()
                }
                _ => view! {
                    <div class="scan-actions">
                        <button class="scan-action" on:click=scan_document disabled=move || !scanner_ok.get()>
                            <strong>{SCAN_DOCUMENT}</strong><span>{SCAN_DOCUMENT_HINT}</span>
                        </button>
                        <button class="scan-action" on:click=scan_code>
                            <strong>{SCAN_CODE}</strong><span>{SCAN_CODE_HINT}</span>
                        </button>
                        <button class="scan-action quiet" on:click=read_image>
                            <strong>{SCAN_IMAGE}</strong><span>{SCAN_IMAGE_HINT}</span>
                        </button>
                    </div>
                    <Show when=move || !scanner_ok.get()>
                        <p class="muted">{NEEDS_PLAY}</p>
                    </Show>
                }.into_any(),
            }}
            {move || notice.get().map(|n| {
                let camera = n == NEEDS_CAMERA;
                view! {
                    <p class="notice" role="status">{n}
                        {camera.then(|| view! {
                            " " <button class="link" on:click=move |_| spawn_local(async move {
                                let _ = invoke::<_, ()>("plugin:barcode-scanner|open_app_settings", &NoArgs {}).await;
                            })>{OPEN_SETTINGS}</button>
                        })}
                    </p>
                }
            })}
            <Show when=move || matches!(screen.get(), Screen::Home)>
                <section class="history" aria-label=HISTORY>
                    <header class="history-head">
                        <h2>{HISTORY}</h2>
                        <Show when=move || !history.get().is_empty()>
                            <button class="link" on:click=move |_| spawn_local(async move {
                                let _ = invoke::<_, ()>("scan_clear", &NoArgs {}).await;
                                reload();
                            })>{CLEAR_HISTORY}</button>
                        </Show>
                    </header>
                    <Show when=move || history.get().is_empty()>
                        <p class="muted">{NO_HISTORY}</p>
                    </Show>
                    <ul class="history-list">
                        {move || history.get().into_iter().map(|r| {
                            let id = r.id.clone();
                            let (title, show): (String, Box<dyn Fn() + Send + Sync>) = match r.kind.clone() {
                                ScanKind::Code { content, .. } => {
                                    let c = content.clone();
                                    (content.chars().take(80).collect(), Box::new(move || show_payload(c.clone())))
                                }
                                ScanKind::Document { files } => {
                                    let saved: Vec<String> = files
                                        .iter()
                                        .map(|f| f.to_string_lossy().into_owned())
                                        .filter(|f| !f.contains("/scans/files/"))
                                        .collect();
                                    let name = saved.first().map(|f| file_name(f)).unwrap_or_default();
                                    (name, Box::new(move || open_files(saved.clone(), true)))
                                }
                            };
                            view! {
                                <li class="history-item">
                                    <span class="history-text">
                                        <span class="history-title">{title}</span>
                                        <span class="muted">{when(r.at)}</span>
                                    </span>
                                    <button class="link" on:click=move |_| show()>{SHOW}</button>
                                    <button class="link" on:click=move |_| {
                                        let id = id.clone();
                                        spawn_local(async move {
                                            let _ = invoke::<_, ()>("scan_delete", &Id { id }).await;
                                            reload();
                                        });
                                    }>{DELETE}</button>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                </section>
            </Show>
            {move || result.get().map(|p| view! {
                <div class="sheet-backdrop" on:click=move |_| result.set(None)></div>
                <aside class="sheet" role="dialog" aria-modal="true">
                    {payload_view(p, ctx, copied, notice)}
                    <button class="link sheet-close" on:click=move |_| result.set(None)>{CLOSE}</button>
                </aside>
            })}
        </section>
    }
}

fn copy_button(text: String, label: &'static str, copied: RwSignal<bool>) -> impl IntoView {
    view! {
        <button class="link" on:click=move |_| {
            copy_text(&text);
            copied.set(true);
        }>{move || if copied.get() { COPIED } else { label }}</button>
    }
}

fn payload_view(
    p: Payload,
    ctx: AppCtx,
    copied: RwSignal<bool>,
    notice: RwSignal<Option<String>>,
) -> AnyView {
    match p {
        Payload::Url { url } => {
            let u = url.clone();
            view! {
                <h2>{OPEN_LINK}</h2>
                <p class="sheet-value url">{url.clone()}</p>
                <p class="muted">{LINK_FIRST}</p>
                <div class="sheet-actions">
                    {copy_button(url, COPY, copied)}
                    <button class="primary" on:click=move |_| {
                        let url = u.clone();
                        spawn_local(async move {
                            if let Err(e) = invoke::<_, ()>("scan_open_url", &Url { url }).await {
                                notice.set(Some(e));
                            }
                        });
                    }>{OPEN_LINK}</button>
                </div>
            }
            .into_any()
        }
        Payload::Wifi { ssid, password, security, .. } => {
            let args = Wifi {
                ssid: ssid.clone(),
                password: password.clone(),
                security,
            };
            let args = std::sync::Arc::new(args);
            view! {
                <h2>{NETWORK}</h2>
                <p class="sheet-value">{ssid}</p>
                <p><span class="muted">{PASSWORD}": "</span>
                    {if password.is_empty() { NO_PASSWORD.to_string() } else { password.clone() }}</p>
                <div class="sheet-actions">
                    {(!password.is_empty()).then(|| copy_button(password.clone(), COPY_PASSWORD, copied))}
                    <button class="primary" on:click=move |_| {
                        let a = args.clone();
                        spawn_local(async move {
                            let w = Wifi { ssid: a.ssid.clone(), password: a.password.clone(), security: a.security.clone() };
                            match invoke::<_, WifiMode>("plugin:fo-android|add_wifi", &w).await {
                                Ok(m) if m.mode == "settings" => notice.set(Some(WIFI_SETTINGS_OPENED.into())),
                                Ok(_) => {}
                                Err(e) => notice.set(Some(e)),
                            }
                        });
                    }>{JOIN_NETWORK}</button>
                </div>
            }
            .into_any()
        }
        Payload::Contact { name, phones, emails, org, vcard } => view! {
            <h2>{name}</h2>
            {(!org.is_empty()).then(|| view! { <p class="muted">{org}</p> })}
            <ul class="sheet-list">
                {phones.into_iter().chain(emails).map(|v| view! { <li>{v}</li> }).collect_view()}
            </ul>
            <div class="sheet-actions">
                <button class="primary" on:click=move |_| {
                    let vcard = vcard.clone();
                    spawn_local(async move {
                        let out_dir = ctx.out_dir.get_untracked();
                        match invoke::<_, String>("scan_save_contact", &Contact { vcard, out_dir }).await {
                            Ok(f) => notice.set(Some(contact_saved(&f))),
                            Err(e) => notice.set(Some(e)),
                        }
                    });
                }>{SAVE_CONTACT}</button>
            </div>
        }
        .into_any(),
        Payload::Email { to, subject, body } => {
            let all = [to.clone(), subject.clone(), body.clone()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            view! {
                <h2>{to}</h2>
                {(!subject.is_empty()).then(|| view! { <p>{subject}</p> })}
                {(!body.is_empty()).then(|| view! { <p class="muted">{body}</p> })}
                <div class="sheet-actions">{copy_button(all, COPY, copied)}</div>
            }
            .into_any()
        }
        Payload::Phone { number } => view! {
            <h2 class="sheet-value">{number.clone()}</h2>
            <div class="sheet-actions">{copy_button(number, COPY, copied)}</div>
        }
        .into_any(),
        Payload::Sms { number, body } => view! {
            <h2 class="sheet-value">{number.clone()}</h2>
            <p>{body.clone()}</p>
            <div class="sheet-actions">{copy_button(format!("{number}\n{body}"), COPY, copied)}</div>
        }
        .into_any(),
        Payload::Geo { lat, lon } => {
            let t = format!("{lat}, {lon}");
            view! {
                <h2 class="sheet-value">{t.clone()}</h2>
                <div class="sheet-actions">{copy_button(t, COPY, copied)}</div>
            }
            .into_any()
        }
        Payload::Text { text } => view! {
            <p class="sheet-value text">{text.clone()}</p>
            <div class="sheet-actions">{copy_button(text, COPY, copied)}</div>
        }
        .into_any(),
    }
}
