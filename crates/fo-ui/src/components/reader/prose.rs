//! Books (chapter by chapter) and single pages of text, Markdown or HTML,
//! set in one reading column.

use super::{icon, wire_resources, Bar, LinkNote, MINUS, PLUS};
use crate::bridge::invoke;
use fo_types::{BookInfo, Platform, ReaderView};
use fo_ui::reading::{chapter_list, chapter_title, link_action, next_label, LinkAction};
use fo_ui::strings::*;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;

#[derive(serde::Serialize)]
struct ChapterArgs {
    doc: u64,
    index: usize,
}

const SIZES: [u32; 7] = [14, 16, 18, 20, 22, 24, 26];

#[component]
pub fn ProseView(view: ReaderView, book: Option<BookInfo>, html: Option<String>) -> impl IntoView {
    let ctx = expect_context::<super::AppCtx>();
    let doc = view.doc;
    let titles = StoredValue::new(
        book.as_ref()
            .map(|b| b.chapters.clone())
            .unwrap_or_default(),
    );
    let is_book = book.is_some();
    let title = book
        .as_ref()
        .and_then(|b| b.title.clone())
        .unwrap_or_else(|| view.name.clone());
    let narrow = ctx.platform.get_untracked() == Platform::Android
        || window()
            .inner_width()
            .ok()
            .and_then(|w| w.as_f64())
            .unwrap_or(1200.0)
            < 900.0;

    let index = RwSignal::new(0usize);
    let body = RwSignal::new(html.unwrap_or_default());
    let size = RwSignal::new(2usize);
    let toc = RwSignal::new(is_book && !narrow);
    let note = RwSignal::new(None::<String>);
    let anchor = RwSignal::new(None::<String>);
    let content = NodeRef::<html::Article>::new();
    let scroller = NodeRef::<html::Div>::new();

    // After new HTML lands: point images at the book, then scroll.
    let settle = move || {
        request_animation_frame(move || {
            if let Some(el) = content.get_untracked() {
                wire_resources(&el, doc);
            }
            let target = anchor
                .get_untracked()
                .and_then(|id| document().get_element_by_id(&id));
            match target {
                Some(t) => t.scroll_into_view(),
                None => {
                    if let Some(s) = scroller.get_untracked() {
                        s.set_scroll_top(0);
                    }
                }
            }
            anchor.set(None);
        });
    };
    if is_book {
        Effect::new(move |_| {
            let i = index.get();
            spawn_local(async move {
                match invoke::<_, String>("reader_chapter", &ChapterArgs { doc, index: i }).await {
                    Ok(h) => body.set(h),
                    Err(e) => body.set(format!(
                        "<p class=\"reader-error\">{}</p>",
                        fo_ui_escape(&e)
                    )),
                }
                settle();
            });
        });
    } else {
        Effect::new(move |_| {
            if content.get().is_some() {
                settle();
            }
        });
    }

    let on_click = move |ev: web_sys::MouseEvent| {
        let Some(t) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        else {
            return;
        };
        let Ok(Some(a)) = t.closest("a") else { return };
        ev.prevent_default();
        match link_action(&a.get_attribute("href").unwrap_or_default()) {
            LinkAction::Chapter(n, at) if is_book && n < titles.with_value(|t| t.len()) => {
                anchor.set(at);
                if index.get_untracked() == n {
                    settle();
                } else {
                    index.set(n);
                }
            }
            LinkAction::Anchor(id) => {
                if let Some(t) = document().get_element_by_id(&id) {
                    t.scroll_into_view();
                }
            }
            LinkAction::External(url) => note.set(Some(url)),
            _ => {}
        }
    };

    let count = titles.with_value(|t| t.len());
    let go = move |i: usize| {
        if narrow {
            toc.set(false);
        }
        index.set(i);
    };

    view! {
        <section class="reader prose-reader">
            <Bar title=title>
                <Show when=move || is_book>
                    <button class="tool-text" aria-pressed=move || toc.get().to_string() on:click=move |_| toc.update(|t| *t = !*t)>{CONTENTS}</button>
                </Show>
                <div class="group" role="group" aria-label="Text size">
                    <button class="round" title=SMALLER_TEXT aria-label=SMALLER_TEXT disabled=move || size.get() == 0 on:click=move |_| size.update(|s| *s = s.saturating_sub(1))>{icon(MINUS)}</button>
                    <span class="aa" aria-hidden="true">"Aa"</span>
                    <button class="round" title=LARGER_TEXT aria-label=LARGER_TEXT disabled=move || size.get() + 1 == SIZES.len() on:click=move |_| size.update(|s| *s = (*s + 1).min(SIZES.len() - 1))>{icon(PLUS)}</button>
                </div>
            </Bar>
            <div class="reader-body">
                <Show when=move || toc.get()>
                    <nav class="toc" aria-label=CONTENTS>
                        <ol>
                            {titles.with_value(|t| chapter_list(t)).into_iter().map(|(i, t)| {
                                let here = move || chapter_title(&titles.get_value(), index.get()) == titles.with_value(|ts| ts[i].clone());
                                view! {
                                    <li><button aria-current=move || here().then_some("true") on:click=move |_| go(i)>{t}</button></li>
                                }
                            }).collect_view()}
                        </ol>
                    </nav>
                </Show>
                <div class="prose-scroll" node_ref=scroller>
                    <article
                        class="prose"
                        style:font-size=move || format!("{}px", SIZES[size.get()])
                        node_ref=content
                        on:click=on_click
                        inner_html=move || body.get()
                    ></article>
                    <Show when=move || is_book>
                        <nav class="chapter-steps" aria-label="Chapters">
                            <Show when=move || { index.get() > 0 }>
                                <button class="link" on:click=move |_| go(index.get_untracked() - 1)>{PREVIOUS_PART}</button>
                            </Show>
                            <span class="spacer"></span>
                            {move || {
                                let i = index.get();
                                (i + 1 < count).then(|| {
                                    let label = titles.with_value(|t| next_label(t, i)).unwrap_or_default();
                                    view! { <button class="primary" on:click=move |_| go(i + 1)>{label}</button> }
                                })
                            }}
                        </nav>
                    </Show>
                </div>
            </div>
            <LinkNote url=note/>
        </section>
    }
}

fn fo_ui_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
