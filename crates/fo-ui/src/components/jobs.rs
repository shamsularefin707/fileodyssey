use super::{file_name, AppCtx};
use crate::bridge::invoke;
use fo_types::{JobStatus, Verdict};
use fo_ui::state::JobRow;
use fo_ui::strings::*;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

#[derive(Serialize)]
struct PathArg {
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobArg {
    job_id: String,
}

fn call(cmd: &'static str, path: String) {
    spawn_local(async move {
        let _ = invoke::<_, ()>(cmd, &PathArg { path }).await;
    });
}

#[component]
fn JobItem(row: JobRow) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let title: String = match &row.status {
        JobStatus::Done { outputs, .. } if !outputs.is_empty() => file_name(&outputs[0]),
        _ => file_name(&row.input),
    };
    let from = file_name(&row.input);
    let show_from = title != from;
    let body = match row.status.clone() {
        JobStatus::Queued => view! { <span class="status muted">{QUEUED}</span> }.into_any(),
        JobStatus::Running { fraction } => {
            let pct = (fraction * 100.0).round() as u32;
            let job_id = row.job_id.clone();
            view! {
                <span class="status">{running(pct)}</span>
                <button class="link" on:click=move |_| {
                    let job_id = job_id.clone();
                    spawn_local(async move { let _ = invoke::<_, ()>("cancel_job", &JobArg { job_id }).await; });
                }>{CANCEL}</button>
            }
            .into_any()
        }
        JobStatus::Done { outputs, report } => {
            let verdict = report.verdict();
            let out = outputs.first().cloned().unwrap_or_default();
            let (o1, o2) = (out.clone(), out);
            let name = title.clone();
            view! {
                <span class="status" class:ok=verdict == Verdict::Exact class:warn=verdict == Verdict::Warn>
                    {if verdict == Verdict::Exact { KEPT_EVERYTHING } else { SOMETHING_CHANGED }}
                </span>
                <span class="actions">
                    <button class="link" on:click=move |_| call("open_path", o1.clone())>{OPEN}</button>
                    <button class="link" on:click=move |_| call("reveal_path", o2.clone())>{SHOW_IN_FOLDER}</button>
                    <button class="link" on:click=move |_| ctx.report.set(Some((name.clone(), report.clone())))>{REPORT}</button>
                </span>
            }
            .into_any()
        }
        JobStatus::Failed { message } => {
            view! { <span class="status warn">{failed(&message)}</span> }.into_any()
        }
        JobStatus::Cancelled => view! { <span class="status muted">{CANCELLED}</span> }.into_any(),
    };
    let fraction = match row.status {
        JobStatus::Running { fraction } => fraction,
        JobStatus::Queued => 0.0,
        _ => 1.0,
    };
    let finished = row.status.is_finished();
    view! {
        <li class="job">
            <div class="job-head">
                <span class="job-title">{title}</span>
                {show_from.then(|| view! { <span class="job-from">{from}</span> })}
            </div>
            <div class="job-line" class:finished=finished aria-hidden="true">
                <span style=format!("transform: scaleX({fraction:.3})")></span>
            </div>
            <div class="job-foot">{body}</div>
        </li>
    }
}

#[component]
pub fn Jobs() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    view! {
        <Show when=move || !ctx.jobs.get().is_empty()>
            <section class="results" aria-live="polite">
                <h2>{RESULTS}</h2>
                <ul>
                    {move || ctx.jobs.get().into_iter().rev().map(|row| view! { <JobItem row=row/> }).collect_view()}
                </ul>
            </section>
        </Show>
    }
}
