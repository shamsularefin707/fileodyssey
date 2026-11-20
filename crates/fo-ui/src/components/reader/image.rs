//! A single image on a quiet stage, with its camera details beside it.

use super::{icon, Bar, MINUS, PLUS};
use crate::bridge::reader_url;
use fo_types::{ImageInfo, ReaderView};
use fo_ui::reading::{zoom_in, zoom_out};
use fo_ui::strings::*;
use leptos::prelude::*;

#[component]
pub fn ImageView(view: ReaderView, info: ImageInfo) -> impl IntoView {
    let src = reader_url(&format!("{}/image", view.doc));
    let (w, h) = (info.width, info.height);
    // None = fit the stage; Some(z) = z × actual size.
    let zoom = RwSignal::new(None::<f64>);
    let style = move || match zoom.get() {
        None => "max-width:100%;max-height:100%".to_string(),
        Some(z) => format!(
            "width:{}px;height:{}px;max-width:none",
            (w as f64 * z).round(),
            (h as f64 * z).round()
        ),
    };
    let details = info.exif.clone();
    let empty = details.is_empty();

    view! {
        <section class="reader image-reader">
            <Bar title=view.name.clone()>
                <div class="group" role="group" aria-label="Zoom">
                    <button class="round" title=ZOOM_OUT aria-label=ZOOM_OUT on:click=move |_| zoom.update(|z| *z = Some(zoom_out(z.unwrap_or(1.0))))>{icon(MINUS)}</button>
                    <button class="zoom-label" title=FIT aria-pressed=move || zoom.get().is_none().to_string() on:click=move |_| zoom.set(None)>
                        {move || zoom.get().map(zoom_label).unwrap_or_else(|| FIT.to_string())}
                    </button>
                    <button class="round" title=ZOOM_IN aria-label=ZOOM_IN on:click=move |_| zoom.update(|z| *z = Some(zoom_in(z.unwrap_or(1.0))))>{icon(PLUS)}</button>
                </div>
                <button class="tool-text" aria-pressed=move || (zoom.get() == Some(1.0)).to_string() on:click=move |_| zoom.set(Some(1.0))>{ACTUAL_SIZE}</button>
            </Bar>
            <div class="reader-body">
                <div class="stage" class:zoomed=move || zoom.get().is_some()>
                    <img src=src alt=view.name.clone() style=style draggable="false"/>
                </div>
                <aside class="details" aria-label=DETAILS>
                    <h2>{DETAILS}</h2>
                    <dl>
                        <dt>{SIZE}</dt>
                        <dd>{format!("{w} × {h} px")}</dd>
                        {details.into_iter().map(|(k, v)| view! { <dt>{k}</dt><dd>{v}</dd> }).collect_view()}
                    </dl>
                    <Show when=move || empty>
                        <p class="muted">{NO_CAMERA_DETAILS}</p>
                    </Show>
                </aside>
            </div>
        </section>
    }
}
