use super::AppCtx;
use fo_ui::strings::*;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Organic outline (viewBox 1000×400, stretched to fit). CSS morphs it to a
/// second shape on each drop; see `.pond.shift .pond-path` in app.css.
const POND_A: &str = "M140 60C300 0 620 10 820 40C960 62 1000 170 980 250C955 350 800 395 600 388C420 382 200 400 90 330C0 272 10 110 140 60Z";

/// The drop zone. Its one moment of motion is a ripple when files land.
#[component]
pub fn Pond() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let hovering = RwSignal::new(false);

    // Browser preview only: Tauri intercepts real drops and sends paths via
    // the `files-dropped` event instead.
    let on_drop = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        hovering.set(false);
        let Some(dt) = ev.data_transfer() else { return };
        let Some(files) = dt.files() else { return };
        let paths = (0..files.length())
            .filter_map(|i| files.get(i))
            .map(|f| format!("/home/you/Dropped/{}", f.name()))
            .collect();
        ctx.add_paths(paths);
    };

    view! {
        <section
            class="pond"
            class:hover=move || hovering.get()
            class:shift=move || ctx.ripple.get() % 2 == 1
            aria-label=POND_TITLE
            on:dragover=move |ev: web_sys::DragEvent| {
                ev.prevent_default();
                hovering.set(true);
            }
            on:dragleave=move |_| hovering.set(false)
            on:drop=on_drop
        >
            <svg class="pond-shape" viewBox="0 0 1000 400" preserveAspectRatio="none" aria-hidden="true">
                <defs>
                    <radialGradient id="pond-light" cx="0.32" cy="0.3" r="0.75">
                        <stop offset="0" class="pond-glow"/>
                        <stop offset="1" class="pond-glow-end"/>
                    </radialGradient>
                    <clipPath id="pond-clip">
                        <path class="pond-path" d=POND_A/>
                    </clipPath>
                </defs>
                <path class="pond-path" d=POND_A/>
                <path class="pond-path pond-light" d=POND_A/>
                <g clip-path="url(#pond-clip)">
                    {move || {
                        // Re-created on every drop so the animation restarts.
                        let n = ctx.ripple.get();
                        (n > 0).then(|| view! { <ellipse class="ripple" data-n=n cx="500" cy="200" rx="40" ry="40"/> })
                    }}
                </g>
            </svg>
            <p class="pond-title">{POND_TITLE}</p>
            <p class="pond-sub">
                {POND_OR} " "
                <button class="link" on:click=move |ev| {
                    if let Some(t) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok()) {
                        let _ = t.blur();
                    }
                    ctx.pick_files();
                }>{CHOOSE_FILES}</button>
            </p>
            <p class="pond-note">{POND_PRIVATE}</p>
        </section>
    }
}
