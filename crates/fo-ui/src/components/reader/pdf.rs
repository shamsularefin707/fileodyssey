//! PDF pages in a scrolling column. Only pages near the viewport exist in
//! the DOM; each is a bitmap with an invisible, selectable text layer and
//! search highlights on top. Matches are also marked along the right edge.

use super::{icon, Bar, DOWN, MINUS, PLUS, UP};
use crate::bridge::{fit_spans, invoke, reader_url};
use fo_types::{PdfInfo, Platform, ReaderView, SearchHit, TextSpan};
use fo_ui::reading::{bitmap_width, current_page, hit_mark, layout, zoom_in, zoom_out, Layout};
use fo_ui::strings::*;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use send_wrapper::SendWrapper;
use std::collections::HashMap;

const GAP: f64 = 18.0;
const PAD: f64 = 24.0;

#[derive(serde::Serialize)]
struct PageArgs {
    doc: u64,
    page: u32,
}

#[derive(serde::Serialize)]
struct SearchArgs {
    doc: u64,
    query: String,
}

#[component]
pub fn PdfView(view: ReaderView, info: PdfInfo) -> impl IntoView {
    let ctx = expect_context::<super::AppCtx>();
    let doc = view.doc;
    let n = info.pages.len();
    let pages = StoredValue::new(info.pages.clone());
    let title = info.title.clone().unwrap_or_else(|| view.name.clone());
    let dpr = window().device_pixel_ratio();
    let narrow = ctx.platform.get_untracked() == Platform::Android
        || window()
            .inner_width()
            .ok()
            .and_then(|w| w.as_f64())
            .unwrap_or(1200.0)
            < 900.0;

    let scroller = NodeRef::<html::Div>::new();
    let search_box = NodeRef::<html::Input>::new();
    let avail = RwSignal::new(0.0f64);
    let view_h = RwSignal::new(600.0f64);
    let scroll_top = RwSignal::new(0.0f64);
    let zoom = RwSignal::new(1.0f64);
    let thumbs = RwSignal::new(!narrow && n > 1);
    let query = RwSignal::new(String::new());
    let searched = RwSignal::new(String::new());
    let hits = RwSignal::new(Vec::<SearchHit>::new());
    let hit_now = RwSignal::new(0usize);
    let spans = RwSignal::new(HashMap::<u32, Vec<TextSpan>>::new());

    let page_w = Memo::new(move |_| ((avail.get() - 2.0 * PAD).clamp(200.0, 880.0)) * zoom.get());
    let lay = Memo::new(move |_| layout(&pages.get_value(), page_w.get(), GAP));
    let current = Memo::new(move |_| {
        current_page(&lay.get(), (scroll_top.get() - PAD).max(0.0), view_h.get())
    });
    let visible = Memo::new(move |_| {
        let l = lay.get();
        let (a, b) = (
            scroll_top.get() - view_h.get(),
            scroll_top.get() + 2.0 * view_h.get(),
        );
        (0..n)
            .filter(|&i| l.tops[i] + PAD + l.heights[i] >= a && l.tops[i] + PAD <= b)
            .collect::<Vec<_>>()
    });

    let measure = move || {
        if let Some(el) = scroller.get_untracked() {
            avail.set(el.client_width() as f64);
            view_h.set(el.client_height() as f64);
        }
    };
    Effect::new(move |_| {
        if scroller.get().is_some() {
            measure();
        }
    });
    let resize = SendWrapper::new(window_event_listener(leptos::ev::resize, move |_| {
        measure()
    }));
    let keys = SendWrapper::new(window_event_listener(leptos::ev::keydown, move |ev| {
        if (ev.ctrl_key() || ev.meta_key()) && ev.key().eq_ignore_ascii_case("f") {
            ev.prevent_default();
            if let Some(i) = search_box.get_untracked() {
                let _ = i.focus();
                i.select();
            }
        }
    }));
    on_cleanup(move || {
        resize.take().remove();
        keys.take().remove();
    });

    let scroll_to = move |top: f64| {
        if let Some(el) = scroller.get_untracked() {
            el.set_scroll_top(top.max(0.0) as i32);
        }
    };
    let go_page = move |p: usize| {
        let l = lay.get_untracked();
        if let Some(t) = l.tops.get(p.min(n.saturating_sub(1))) {
            scroll_to(t + PAD - 8.0);
        }
    };
    // Keep the same spot of the same page in view across zoom changes.
    let set_zoom = move |z: f64| {
        let l = lay.get_untracked();
        let p = current.get_untracked();
        let frac = ((scroll_top.get_untracked() - PAD - l.tops[p]) / l.heights[p]).clamp(0.0, 1.0);
        zoom.set(z);
        request_animation_frame(move || {
            let l = lay.get_untracked();
            scroll_to(l.tops[p] + PAD + frac * l.heights[p]);
        });
    };
    let show_hit = move |i: usize| {
        if let Some(h) = hits.with_untracked(|hs| hs.get(i).cloned()) {
            let l = lay.get_untracked();
            let p = h.page as usize;
            let y = h.rects.first().map(|r| r.y as f64).unwrap_or(0.0);
            scroll_to(l.tops[p] + PAD + y * l.heights[p] - view_h.get_untracked() / 3.0);
        }
    };
    let find = move |back: bool| {
        let q = query.get_untracked();
        if q.trim().is_empty() {
            hits.set(vec![]);
            searched.set(String::new());
            return;
        }
        if q != searched.get_untracked() {
            spawn_local(async move {
                let found: Vec<SearchHit> = invoke(
                    "reader_search",
                    &SearchArgs {
                        doc,
                        query: q.clone(),
                    },
                )
                .await
                .unwrap_or_default();
                searched.set(q);
                hit_now.set(0);
                hits.set(found);
                show_hit(0);
            });
        } else {
            let len = hits.with_untracked(Vec::len);
            if len == 0 {
                return;
            }
            let i = hit_now.get_untracked();
            let i = if back {
                (i + len - 1) % len
            } else {
                (i + 1) % len
            };
            hit_now.set(i);
            show_hit(i);
        }
    };

    view! {
        <section class="reader pdf-reader">
            <Bar title=title>
                <Show when=move || { n > 1 }>
                    <button class="tool-text" aria-pressed=move || thumbs.get().to_string() on:click=move |_| thumbs.update(|t| *t = !*t)>{PAGES}</button>
                </Show>
                <label class="page-jump">
                    <span class="sr-only">{PAGE}</span>
                    <input
                        type="number" min="1" max=n.to_string()
                        prop:value=move || (current.get() + 1).to_string()
                        on:change=move |ev| {
                            if let Ok(p) = event_target_value(&ev).parse::<usize>() {
                                go_page(p.saturating_sub(1));
                            }
                        }
                    />
                    <span class="of">{of_pages(n)}</span>
                </label>
                <div class="group" role="group" aria-label="Zoom">
                    <button class="round" title=ZOOM_OUT aria-label=ZOOM_OUT on:click=move |_| set_zoom(zoom_out(zoom.get_untracked()))>{icon(MINUS)}</button>
                    <button class="zoom-label" title=FIT_WIDTH on:click=move |_| set_zoom(1.0)>{move || zoom_label(zoom.get())}</button>
                    <button class="round" title=ZOOM_IN aria-label=ZOOM_IN on:click=move |_| set_zoom(zoom_in(zoom.get_untracked()))>{icon(PLUS)}</button>
                </div>
                <div class="find" role="search">
                    <input
                        type="search" node_ref=search_box placeholder=FIND aria-label=FIND
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            if ev.key() == "Enter" {
                                ev.prevent_default();
                                find(ev.shift_key());
                            }
                        }
                    />
                    <span class="find-count" aria-live="polite">
                        {move || {
                            let len = hits.with(Vec::len);
                            if searched.get().is_empty() { String::new() }
                            else if len == 0 { NO_MATCHES.to_string() }
                            else { match_count(hit_now.get(), len) }
                        }}
                    </span>
                    <button class="round" title=FIND_PREV aria-label=FIND_PREV on:click=move |_| find(true)>{icon(UP)}</button>
                    <button class="round" title=FIND_NEXT aria-label=FIND_NEXT on:click=move |_| find(false)>{icon(DOWN)}</button>
                </div>
            </Bar>
            <div class="reader-body">
                <Show when=move || thumbs.get()>
                    <nav class="thumbs" aria-label=PAGES>
                        {(0..n).map(|i| {
                            let p = pages.with_value(|p| p[i]);
                            let ratio = format!("{} / {}", p.w, p.h);
                            view! {
                                <button
                                    class="thumb"
                                    aria-current=move || (current.get() == i).then_some("page")
                                    aria-label=format!("{PAGE} {}", i + 1)
                                    on:click=move |_| go_page(i)
                                >
                                    <img src=reader_url(&format!("{doc}/page/{i}?w=256")) alt="" loading="lazy" style:aspect-ratio=ratio/>
                                    <span>{i + 1}</span>
                                </button>
                            }
                        }).collect_view()}
                    </nav>
                </Show>
                <div class="pdf-scroll" node_ref=scroller on:scroll=move |_| {
                    if let Some(el) = scroller.get_untracked() {
                        scroll_top.set(el.scroll_top() as f64);
                    }
                }>
                    <div
                        class="pdf-column"
                        style:width=move || format!("{}px", page_w.get())
                        style:height=move || format!("{}px", lay.get().total + 2.0 * PAD)
                    >
                        <For
                            each=move || visible.get()
                            key=|i| *i
                            children=move |i| view! {
                                <Page doc=doc index=i lay=lay page_w=page_w spans=spans hits=hits hit_now=hit_now dpr=dpr/>
                            }
                        />
                    </div>
                </div>
                <div class="hit-map" aria-hidden="true">
                    {move || {
                        let l = lay.get();
                        let now = hit_now.get();
                        hits.get().iter().enumerate().map(|(k, h)| {
                            let y = h.rects.first().map(|r| r.y as f64).unwrap_or(0.0);
                            let top = format!("{:.3}%", hit_mark(&l, h.page as usize, y) * 100.0);
                            view! { <i class:now=k == now style:top=top></i> }
                        }).collect_view()
                    }}
                </div>
            </div>
        </section>
    }
}

#[component]
fn Page(
    doc: u64,
    index: usize,
    lay: Memo<Layout>,
    page_w: Memo<f64>,
    spans: RwSignal<HashMap<u32, Vec<TextSpan>>>,
    hits: RwSignal<Vec<SearchHit>>,
    hit_now: RwSignal<usize>,
    dpr: f64,
) -> impl IntoView {
    let key = index as u32;
    if !spans.with_untracked(|m| m.contains_key(&key)) {
        spawn_local(async move {
            let found: Vec<TextSpan> = invoke("reader_text", &PageArgs { doc, page: key })
                .await
                .unwrap_or_default();
            spans.update(|m| {
                m.insert(key, found);
            });
        });
    }
    let layer = NodeRef::<html::Div>::new();
    Effect::new(move |_| {
        spans.track();
        page_w.track();
        if let Some(el) = layer.get() {
            request_animation_frame(move || fit_spans(&el));
        }
    });
    let height = move || lay.with(|l| l.heights[index]);
    let src = move || {
        reader_url(&format!(
            "{doc}/page/{index}?w={}",
            bitmap_width(page_w.get(), dpr)
        ))
    };

    view! {
        <div
            class="pdf-page"
            style:top=move || format!("{}px", lay.with(|l| l.tops[index]) + PAD)
            style:height=move || format!("{}px", height())
        >
            <img src=src alt=format!("{PAGE} {}", index + 1) decoding="async" draggable="false"/>
            <div class="text-layer" node_ref=layer>
                {move || {
                    let (w, h) = (page_w.get(), height());
                    spans.with(|m| m.get(&key).cloned().unwrap_or_default()).into_iter().map(|s| {
                        let style = format!(
                            "left:{:.1}px;top:{:.1}px;font-size:{:.1}px",
                            s.rect.x as f64 * w,
                            s.rect.y as f64 * h,
                            (s.rect.h as f64 * h * 0.92).max(1.0)
                        );
                        view! { <span data-w=format!("{:.1}", s.rect.w as f64 * w) style=style>{s.text}</span> }
                    }).collect_view()
                }}
            </div>
            <div class="hits" aria-hidden="true">
                {move || {
                    let now = hit_now.get();
                    hits.get().iter().enumerate().filter(|(_, h)| h.page == key).flat_map(|(k, h)| {
                        h.rects.iter().map(move |r| {
                            let style = format!(
                                "left:{:.3}%;top:{:.3}%;width:{:.3}%;height:{:.3}%",
                                r.x * 100.0, r.y * 100.0, r.w * 100.0, r.h * 100.0
                            );
                            view! { <i class:now=k == now style=style></i> }
                        }).collect::<Vec<_>>()
                    }).collect_view()
                }}
            </div>
        </div>
    }
}
