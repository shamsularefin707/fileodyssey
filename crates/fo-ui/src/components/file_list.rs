use super::{AppCtx, Entry};
use crate::bridge::invoke;
use fo_types::{JobRequest, OptionKind, OptionSpec, ToolInfo};
use fo_ui::state::{can_run, merged_options, visible_options};
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
        .filter(|t| t.category == tab && !t.combines)
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
    // One group per tool, so "PNG" under Convert and under Resize are distinct.
    let groups = move || {
        tools_in_tab(ctx, &e.get_value())
            .into_iter()
            .map(|t| {
                let targets = if t.targets.is_empty() {
                    vec![String::new()]
                } else {
                    t.targets.clone()
                };
                let items = targets
                    .into_iter()
                    .map(|target| {
                        let value = format!("{}|{}", t.id, target);
                        let label = if target.is_empty() {
                            t.label.clone()
                        } else {
                            target_label(&target)
                        };
                        (value, label)
                    })
                    .collect::<Vec<_>>();
                (t.label.clone(), items)
            })
            .collect::<Vec<_>>()
    };
    let options = move || {
        groups()
            .into_iter()
            .flat_map(|(_, items)| items)
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
    let p_opts = path.clone();
    let open = RwSignal::new(false);
    let specs = move || current().map(|(t, _)| t.options).unwrap_or_default();
    let target = move || current().and_then(|(_, tg)| tg).unwrap_or_default();
    let values = {
        let p = path.clone();
        Signal::derive(move || merged_options(&specs(), &target(), ctx.options.get().get(&p)))
    };
    let has_options =
        Signal::derive(move || !visible_options(&specs(), &target(), &values.get()).is_empty());

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
                            {move || {
                                let gs = groups();
                                let single = gs.len() == 1;
                                gs.into_iter().map(|(group, items)| {
                                    let opts = items.into_iter().map(|(v, l)| {
                                        let sel = v == selected_value();
                                        view! { <option value=v selected=sel>{l}</option> }
                                    }).collect_view();
                                    if single {
                                        opts.into_any()
                                    } else {
                                        view! { <optgroup label=group>{opts}</optgroup> }.into_any()
                                    }
                                }).collect_view()
                            }}
                        </select>
                    </label>
                    <Show when=best_effort>
                        <span class="best-effort" title=BEST_EFFORT_HINT>{BEST_EFFORT}</span>
                    </Show>
                    <Show when=move || has_options.get()>
                        <button class="link" aria-expanded=move || open.get().to_string() on:click=move |_| open.update(|o| *o = !*o)>
                            {move || if open.get() { HIDE_OPTIONS } else { OPTIONS }}
                        </button>
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
            <Show when=move || open.get() && has_options.get()>
                <OptionsPanel path=p_opts.clone() specs=Signal::derive(specs) target=Signal::derive(target) values=values/>
            </Show>
        </li>
    }
}

/// Generic controls for a tool's `OptionSpec`s; values go into `ctx.options[path]`.
#[component]
fn OptionsPanel(
    path: String,
    specs: Signal<Vec<OptionSpec>>,
    target: Signal<String>,
    values: Signal<serde_json::Value>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let set = {
        let path = path.clone();
        move |key: String, v: serde_json::Value| {
            ctx.options.update(|m| {
                let entry = m
                    .entry(path.clone())
                    .or_insert_with(|| serde_json::json!({}));
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert(key, v);
                }
            });
        }
    };
    view! {
        <div class="options" role="group" aria-label=OPTIONS>
            {move || {
                let all = specs.get();
                let vals = values.get();
                visible_options(&all, &target.get(), &vals)
                    .into_iter()
                    .cloned()
                    .map(|o| control(o, &vals, set.clone()))
                    .collect_view()
            }}
        </div>
    }
}

fn control(
    o: OptionSpec,
    vals: &serde_json::Value,
    set: impl Fn(String, serde_json::Value) + Clone + 'static,
) -> AnyView {
    let key = o.key.clone();
    let current = vals.get(&key).cloned().unwrap_or(serde_json::Value::Null);
    let id = format!("opt-{key}");
    let for_id = id.clone();
    match o.kind {
        OptionKind::Slider {
            min,
            max,
            step,
            unit,
            ..
        } => {
            let v = current.as_u64().unwrap_or(min as u64);
            view! {
                <label class="opt" for=for_id>
                    <span>{o.label}</span>
                    <input id=id type="range" min=min max=max step=step prop:value=v.to_string()
                        on:input=move |ev| set(key.clone(), event_target_value(&ev).parse::<u64>().unwrap_or(v).into())/>
                    <output>{v} {unit}</output>
                </label>
            }
            .into_any()
        }
        OptionKind::Number { min, max, unit, .. } => {
            let v = current.as_u64().unwrap_or(min as u64);
            view! {
                <label class="opt" for=for_id>
                    <span>{o.label}</span>
                    <input id=id type="number" min=min max=max prop:value=v.to_string()
                        on:change=move |ev| {
                            let n = event_target_value(&ev).parse::<u64>().unwrap_or(v).clamp(min as u64, max as u64);
                            set(key.clone(), n.into())
                        }/>
                    <span class="unit">{unit}</span>
                </label>
            }
            .into_any()
        }
        OptionKind::Toggle { .. } => {
            let v = current.as_bool().unwrap_or(false);
            view! {
                <label class="opt toggle" for=for_id>
                    <input id=id type="checkbox" prop:checked=v on:change=move |ev| set(key.clone(), event_target_checked(&ev).into())/>
                    <span>{o.label}</span>
                </label>
            }
            .into_any()
        }
        OptionKind::Choice { choices, .. } => {
            let v = current.as_str().unwrap_or("").to_owned();
            view! {
                <label class="opt" for=for_id>
                    <span>{o.label}</span>
                    <select id=id prop:value=v.clone() on:change=move |ev| set(key.clone(), event_target_value(&ev).into())>
                        {choices.into_iter().map(|(cv, cl)| {
                            let sel = cv == v;
                            view! { <option value=cv selected=sel>{cl}</option> }
                        }).collect_view()}
                    </select>
                </label>
            }
            .into_any()
        }
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
        if !can_run(&out_dir, 1) {
            return;
        }
        let user = ctx.options.get_untracked();
        for (path, (tool, target)) in runnable() {
            let options = merged_options(
                &tool.options,
                target.as_deref().unwrap_or(""),
                user.get(&path),
            );
            let req = JobRequest {
                tool_id: tool.id,
                inputs: vec![path],
                target,
                options,
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
                    <Show when=move || ctx.platform.get() == fo_types::Platform::Desktop>
                        <button class="link" on:click=change_dir>{CHANGE}</button>
                    </Show>
                </p>
                <button class="primary" on:click=run_all disabled=move || !can_run(&ctx.out_dir.get(), runnable().len())>{RUN_ALL}</button>
            </div>
        </Show>
    }
}
