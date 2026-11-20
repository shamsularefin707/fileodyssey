//! The Read tab's viewer: one component per kind of document, sharing a
//! slim toolbar.

mod image;
mod pdf;
mod prose;

use super::AppCtx;
use crate::bridge::{copy_text, reader_url};
use fo_types::{ReaderBody, ReaderView};
use fo_ui::strings::*;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

#[derive(serde::Serialize)]
pub(crate) struct DocArg {
    pub doc: u64,
}

#[component]
pub fn Reader() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    move || {
        ctx.reading.get().map(|v: ReaderView| match v.body.clone() {
            ReaderBody::Pdf(info) => view! { <pdf::PdfView view=v info=info/> }.into_any(),
            ReaderBody::Book(info) => {
                view! { <prose::ProseView view=v book=Some(info) html=None/> }.into_any()
            }
            ReaderBody::Page(html) => {
                view! { <prose::ProseView view=v book=None html=Some(html)/> }.into_any()
            }
            ReaderBody::Image(info) => view! { <image::ImageView view=v info=info/> }.into_any(),
        })
    }
}

/// Back button and title; the view's own controls go in `children`.
#[component]
pub(crate) fn Bar(title: String, children: Children) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let hover = title.clone();
    view! {
        <header class="reader-bar">
            <button class="back" title=BACK_TO_FILES_LABEL aria-label=BACK_TO_FILES_LABEL on:click=move |_| ctx.close_reader()>
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14.5 6 8.5 12l6 6" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg>
                <span>{BACK_TO_FILES}</span>
            </button>
            <h1 class="reader-title" title=hover>{title}</h1>
            <div class="reader-tools">{children()}</div>
        </header>
    }
}

/// A round icon button with a text label for assistive tech.
pub(crate) fn icon(d: &'static str) -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d=d fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
    }
}

pub(crate) const MINUS: &str = "M6 12h12";
pub(crate) const PLUS: &str = "M6 12h12M12 6v12";
pub(crate) const UP: &str = "M6 15l6-6 6 6";
pub(crate) const DOWN: &str = "M6 9l6 6 6-6";

/// Points `img[data-fo-src]` at the document's `reader://` resources.
pub(crate) fn wire_resources(root: &web_sys::Element, doc: u64) {
    let Ok(list) = root.query_selector_all("img[data-fo-src]") else {
        return;
    };
    for i in 0..list.length() {
        let Some(el) = list
            .item(i)
            .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
        else {
            continue;
        };
        if let Some(p) = el.get_attribute("data-fo-src") {
            let _ = el.set_attribute("loading", "lazy");
            let _ = el.set_attribute("src", &reader_url(&format!("{doc}/res/{p}")));
        }
    }
}

/// "This link goes to …" with a copy button. Links are never opened.
#[component]
pub(crate) fn LinkNote(url: RwSignal<Option<String>>) -> impl IntoView {
    let copied = RwSignal::new(false);
    view! {
        <Show when=move || url.get().is_some()>
            <div class="link-note" role="status">
                <p>{move || link_note(&url.get().unwrap_or_default())}</p>
                <button class="link" on:click=move |_| {
                    copy_text(&url.get_untracked().unwrap_or_default());
                    copied.set(true);
                }>{move || if copied.get() { COPIED } else { COPY_ADDRESS }}</button>
                <button class="link" on:click=move |_| { url.set(None); copied.set(false); }>{DISMISS}</button>
            </div>
        </Show>
    }
}
