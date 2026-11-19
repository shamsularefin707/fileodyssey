use super::AppCtx;
use fo_ui::strings::*;
use leptos::prelude::*;

#[component]
pub fn ReportDrawer() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let close = move |_| ctx.report.set(None);
    view! {
        <aside
            class="drawer"
            class:open=move || ctx.report.get().is_some()
            aria-hidden=move || ctx.report.get().is_none().to_string()
            aria-label=REPORT_TITLE
        >
            {move || ctx.report.get().map(|(name, r)| {
                let sim = r.text_similarity.map(|s| format!("{:.1}%", s * 100.0));
                let empty = r.warnings.is_empty() && r.metrics.iter().all(|m| m.after >= m.before);
                view! {
                    <header class="drawer-head">
                        <h2>{REPORT_TITLE}</h2>
                        <button class="link" on:click=close>{CLOSE}</button>
                    </header>
                    <p class="drawer-file">{name}</p>
                    <table class="metrics">
                        <thead><tr><th scope="col"></th><th scope="col">{REPORT_BEFORE}</th><th scope="col">{REPORT_AFTER}</th></tr></thead>
                        <tbody>
                            {r.metrics.iter().map(|m| {
                                let lost = m.after < m.before;
                                view! { <tr class:lost=lost><th scope="row">{m.name.clone()}</th><td>{m.before}</td><td>{m.after}</td></tr> }
                            }).collect_view()}
                        </tbody>
                    </table>
                    {sim.map(|s| view! { <p class="similarity">{REPORT_TEXT} " " <strong>{s}</strong></p> })}
                    {if empty {
                        view! { <p class="muted">{REPORT_NONE}</p> }.into_any()
                    } else {
                        view! {
                            <h3>{REPORT_NOTES}</h3>
                            <ul class="warnings">{r.warnings.iter().map(|w| view! { <li>{w.clone()}</li> }).collect_view()}</ul>
                        }.into_any()
                    }}
                }
            })}
        </aside>
    }
}
