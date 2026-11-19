use super::AppCtx;
use fo_types::Category;
use fo_ui::state::visible_tabs;
use fo_ui::strings::{tab_label, APP_NAME};
use leptos::prelude::*;

fn glyph(c: Category) -> impl IntoView {
    // Hand-drawn-feeling stroke glyphs, 24px grid.
    let d = match c {
        Category::Convert => "M5 8c3-3 9-3 12 0M17 8l-2-3M17 8l-3 1M19 16c-3 3-9 3-12 0M7 16l2 3M7 16l3-1",
        Category::Compress => "M7 4c2 2 8 2 10 0M7 20c2-2 8-2 10 0M12 7v4M12 17v-4M9.5 9.5 12 12l2.5-2.5M9.5 14.5 12 12l2.5 2.5",
        Category::Images => "M4 17c3-4 5-5 8-2s5 1 8-3M5 5h14c.6 0 1 .4 1 1v12c0 .6-.4 1-1 1H5c-.6 0-1-.4-1-1V6c0-.6.4-1 1-1zM15.5 9.5h.01",
        Category::Read => "M12 7c-2-2-5-2.5-8-2v13c3-.5 6 0 8 2 2-2 5-2.5 8-2V5c-3-.5-6 0-8 2zM12 7v13",
        Category::Scan => "M4 9V6c0-1 1-2 2-2h3M15 4h3c1 0 2 1 2 2v3M20 15v3c0 1-1 2-2 2h-3M9 20H6c-1 0-2-1-2-2v-3M7 12h10",
    };
    view! {
        <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true">
            <path d=d fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
    }
}

#[component]
pub fn Rail() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    view! {
        <nav class="rail" aria-label="Tools">
            <div class="brand" title=APP_NAME>
                <svg viewBox="0 0 64 64" aria-hidden="true" class="brand-mark">
                    <path d="M33 9c10 0 19 5 22 14 3 10-1 21-10 26-8 6-21 6-29 1C8 45 6 34 9 25 13 16 23 10 33 9z" fill="var(--river)"/>
                    <ellipse cx="50" cy="51" rx="9" ry="6.6" transform="rotate(-18 50 51)" fill="var(--pine)"/>
                </svg>
                <span class="brand-name">{APP_NAME}</span>
            </div>
            <ul>
                <For
                    each=move || visible_tabs(ctx.platform.get())
                    key=|c| *c
                    children=move |c| {
                        view! {
                            <li>
                                <button
                                    class="tab"
                                    aria-current=move || (ctx.tab.get() == c).then_some("page")
                                    on:click=move |_| ctx.tab.set(c)
                                >
                                    {glyph(c)}
                                    <span>{tab_label(c)}</span>
                                </button>
                            </li>
                        }
                    }
                />
            </ul>
        </nav>
    }
}
