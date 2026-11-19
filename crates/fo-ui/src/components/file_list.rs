use super::{AppCtx, Entry};
use crate::bridge::invoke;
use fo_types::{JobRequest, ToolInfo};
use fo_ui::strings::*;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

#[derive(Serialize)]
struct StartArgs {
    req: JobRequest,
}

#[derive(Serialize)]
struct NoArgs {}

fn human_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{} KB", b >> 10),
        b => format!("{b} bytes"),
    }
}

fn tools_in_tab(ctx: AppCtx, e: &Entry) -> Vec<ToolInfo> {
    let tab = ctx.tab.get();
    e.tools
        .iter()
        .filter(|t| t.category == tab)
        .cloned()
        .collect()
}

/// The selected "tool|target" for a file, defaulting to the first option.
fn choice_for(ctx: AppCtx, e: &Entry) -> Option<(ToolInfo, Option<String>)> {
    let tools = tools_in_tab(ctx, e);
    let chosen = ctx.choices.get().get(&e.file.path).cloned();
    let parse = |s: &str| {
        let (id, target) = s.split_once('|')?;
        let t = tools.iter().find(|t| t.id == id)?.clone();
        Some((t, (!target.is_empty()).then(|| target.to_owned())))
    };
    chosen.as_deref().and_then(parse).or_else(|| {
        let t = tools.first()?.clone();
        let target = t.targets.first().cloned();
        Some((t, target))
    })
}

#[component]
fn Row(entry: Entry) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let path = entry.file.path.clone();
    let e = StoredValue::new(entry.clone());
    let options = move || {
        tools_in_tab(ctx, &e.get_value())
            .into_iter()
            .flat_map(|t| {
                let targets = if t.targets.is_empty() {
                    vec![String::new()]
                } else {
                    t.targets.clone()
                };
                targets.into_iter().map(move |target| {
                    let value = format!("{}|{}", t.id, target);
                    let label = if target.is_empty() {
                        t.label.clone()
                    } else {
                        target.to_uppercase()
                    };
                    (value, label)
                })
            })
            .collect::<Vec<_>>()
    };
    let current = move || choice_for(ctx, &e.get_value());
    let selected_value = move || {
        current()
            .map(|(t, tg)| format!("{}|{}", t.id, tg.unwrap_or_default()))
            .unwrap_or_default()
    };
    let best_effort = move || current().is_some_and(|(t, _)| t.best_effort);
    let p_change = path.clone();
    let p_remove = path.clone();

    view! {
        <li class="file-row">
            <div class="file-meta">
                <span class="file-name">{entry.file.name.clone()}</span>
                <span class="file-kind">{entry.file.kind.label()} ", " {human_size(entry.file.size)}</span>
            </div>
            <div class="file-action">
                <Show
                    when=move || !options().is_empty()
                    fallback=|| view! { <span class="muted">{NO_TOOLS}</span> }
                >
                    <label class="target">
                        <span>{TO}</span>
                        <select
                            prop:value=selected_value
                            on:change={
                                let p = p_change.clone();
                                move |ev| {
                                    let v = event_target_value(&ev);
                                    ctx.choices.update(|c| { c.insert(p.clone(), v); });
                                }
                            }
                        >
                            {move || options().into_iter().map(|(v, l)| {
                                let sel = v == selected_value();
                                view! { <option value=v selected=sel>{l}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                    <Show when=best_effort>
                        <span class="best-effort" title=BEST_EFFORT_HINT>{BEST_EFFORT}</span>
                    </Show>
                </Show>
            </div>
            <button
                class="remove"
                aria-label=format!("{REMOVE} {}", entry.file.name)
                title=REMOVE
                on:click=move |_| ctx.entries.update(|l| l.retain(|x| x.file.path != p_remove))
            >
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7 7 17" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
            </button>
        </li>
    }
}

#[component]
pub fn FileList() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let runnable = move || {
        ctx.entries
            .get()
            .iter()
            .filter_map(|e| choice_for(ctx, e).map(|c| (e.file.path.clone(), c)))
            .collect::<Vec<_>>()
    };
    let run_all = move |_| {
        let out_dir = ctx.out_dir.get_untracked();
        for (path, (tool, target)) in runnable() {
            let req = JobRequest {
                tool_id: tool.id,
                inputs: vec![path],
                target,
                options: serde_json::json!({}),
                out_dir: out_dir.clone(),
            };
            spawn_local(async move {
                let _ = invoke::<_, String>("start_job", &StartArgs { req }).await;
            });
        }
    };
    let change_dir = move |_| {
        spawn_local(async move {
            if let Ok(Some(d)) = invoke::<_, Option<String>>("pick_folder", &NoArgs {}).await {
                ctx.out_dir.set(d);
            }
        })
    };

    view! {
        <Show when=move || !ctx.errors.get().is_empty()>
            <ul class="errors" role="alert">
                {move || ctx.errors.get().into_iter().map(|e| view! { <li>{e}</li> }).collect_view()}
            </ul>
        </Show>
        <Show when=move || !ctx.entries.get().is_empty()>
            <ul class="files">
                <For
                    each=move || ctx.entries.get()
                    key=|e| e.file.path.clone()
                    children=|e| view! { <Row entry=e/> }
                />
            </ul>
            <div class="run-bar">
                <p class="save-to">
                    {SAVE_TO} " " <span class="dir">{move || ctx.out_dir.get()}</span> " "
                    <button class="link" on:click=change_dir>{CHANGE}</button>
                </p>
                <button class="primary" on:click=run_all disabled=move || runnable().is_empty()>{RUN_ALL}</button>
            </div>
        </Show>
    }
}
