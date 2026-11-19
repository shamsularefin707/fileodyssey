use super::AppCtx;
use crate::bridge::invoke;
use fo_types::{Category, JobRequest, OptionKind};
use fo_ui::state::can_run;
use fo_ui::strings::*;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

#[derive(Serialize)]
struct StartArgs {
    req: JobRequest,
}

/// On the Images tab, with two or more images listed: one PDF from all of them.
#[component]
pub fn CombineBar() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let page = RwSignal::new(String::from("image"));
    let combinable = move || {
        let entries = ctx.entries.get();
        let tool = entries
            .iter()
            .find_map(|e| e.tools.iter().find(|t| t.combines).cloned());
        let paths: Vec<String> = entries
            .iter()
            .filter(|e| e.tools.iter().any(|t| t.combines))
            .map(|e| e.file.path.clone())
            .collect();
        tool.map(|t| (t, paths))
    };
    let shown = move || {
        ctx.tab.get() == Category::Images && combinable().is_some_and(|(_, p)| p.len() >= 2)
    };
    let run = move |_| {
        let Some((tool, paths)) = combinable() else {
            return;
        };
        let out_dir = ctx.out_dir.get_untracked();
        if !can_run(&out_dir, paths.len()) {
            return;
        }
        let req = JobRequest {
            tool_id: tool.id,
            inputs: paths,
            target: Some("pdf".into()),
            options: serde_json::json!({ "page": page.get_untracked() }),
            out_dir,
        };
        spawn_local(async move {
            let _ = invoke::<_, String>("start_job", &StartArgs { req }).await;
        });
    };
    let choices = move || {
        combinable()
            .and_then(|(t, _)| t.options.into_iter().find(|o| o.key == "page"))
            .and_then(|o| match o.kind {
                OptionKind::Choice { choices, .. } => Some(choices),
                _ => None,
            })
            .unwrap_or_default()
    };
    view! {
        <Show when=shown>
            <section class="combine" aria-label=move || combine_title(combinable().map(|(_, p)| p.len()).unwrap_or(0))>
                <p class="combine-title">{move || combine_title(combinable().map(|(_, p)| p.len()).unwrap_or(0))}</p>
                <label class="target">
                    <span>{COMBINE_PAGE}</span>
                    <select prop:value=move || page.get() on:change=move |ev| page.set(event_target_value(&ev))>
                        {move || choices().into_iter().map(|(v, l)| view! { <option value=v.clone() selected=v == page.get()>{l}</option> }).collect_view()}
                    </select>
                </label>
                <button class="primary" on:click=run>{COMBINE_RUN}</button>
            </section>
        </Show>
    }
}
