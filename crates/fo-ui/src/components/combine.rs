use super::file_list::control;
use super::AppCtx;
use crate::bridge::invoke;
use fo_types::{JobRequest, ToolInfo};
use fo_ui::state::{can_run, combine_candidates, merged_options, visible_options};
use fo_ui::strings::*;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

#[derive(Serialize)]
struct StartArgs {
    req: JobRequest,
}

/// One bar per combining tool in the current tab (images → PDF, files →
/// archive), with its format and options.
#[component]
pub fn CombineBar() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let candidates = move || {
        let files: Vec<(String, Vec<ToolInfo>)> = ctx
            .entries
            .get()
            .iter()
            .map(|e| (e.file.path.clone(), e.tools.clone()))
            .collect();
        combine_candidates(&files, ctx.tab.get())
    };
    view! {
        <For
            each=candidates
            key=|(t, paths)| (t.id.clone(), paths.len())
            children=move |(tool, paths)| view! { <Bar tool=tool paths=paths/> }
        />
    }
}

#[component]
fn Bar(tool: ToolInfo, paths: Vec<String>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let target = RwSignal::new(tool.targets.first().cloned().unwrap_or_default());
    let user = RwSignal::new(serde_json::json!({}));
    let specs = StoredValue::new(tool.options.clone());
    let values = Signal::derive(move || {
        merged_options(&specs.get_value(), &target.get(), Some(&user.get()))
    });
    let set = move |k: String, v: serde_json::Value| {
        user.update(|u| {
            if let Some(o) = u.as_object_mut() {
                o.insert(k, v);
            }
        })
    };
    let n = paths.len();
    let title = combine_title(&tool.id, n);
    let label = title.clone();
    let targets = tool.targets.clone();
    let tool_id = tool.id.clone();
    let busy = RwSignal::new(false);
    let run = {
        let paths = paths.clone();
        move |_| {
            let out_dir = ctx.out_dir.get_untracked();
            if busy.get_untracked() || !can_run(&out_dir, paths.len()) {
                return;
            }
            busy.set(true);
            let req = JobRequest {
                tool_id: tool_id.clone(),
                inputs: paths.clone(),
                target: Some(target.get_untracked()),
                options: values.get_untracked(),
                out_dir,
            };
            spawn_local(async move {
                let _ = invoke::<_, String>("start_job", &StartArgs { req }).await;
                busy.set(false);
            });
        }
    };
    view! {
        <section class="combine" aria-label=label>
            <p class="combine-title">{title}</p>
            <Show when={
                let t = targets.clone();
                move || t.len() > 1
            }>
                <label class="target">
                    <span>{FORMAT}</span>
                    <select prop:value=move || target.get() on:change=move |ev| target.set(event_target_value(&ev))>
                        {targets.clone().into_iter().map(|t| {
                            let label = target_label(&t);
                            view! { <option value=t.clone() selected=move || target.get() == t>{label}</option> }
                        }).collect_view()}
                    </select>
                </label>
            </Show>
            {move || {
                let vals = values.get();
                visible_options(&specs.get_value(), &target.get(), &vals)
                    .into_iter()
                    .cloned()
                    .map(|o| control(o, &vals, set))
                    .collect_view()
            }}
            <button class="primary" on:click=run disabled=move || busy.get()>{combine_action(&tool.id)}</button>
        </section>
    }
}
