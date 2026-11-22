//! Update banner and the About section. The check runs at start only when
//! the person allows it; errors stay quiet unless they pressed Check now.

use super::{AppCtx, NoArgs};
use crate::bridge::{invoke, latest_release};
use fo_types::{Platform, Settings, UpdateInfo};
use fo_ui::state::offer_update;
use fo_ui::strings::*;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
pub struct UpdateCtx {
    pub settings: RwSignal<Settings>,
    pub version: RwSignal<String>,
    pub available: RwSignal<Option<UpdateInfo>>,
    /// Result of a check the person asked for.
    pub status: RwSignal<Option<String>>,
    pub busy: RwSignal<bool>,
}

#[derive(Serialize)]
struct SettingsArg {
    settings: Settings,
}

#[derive(Deserialize)]
struct Latest {
    tag: String,
    notes: String,
}

/// What's newer than this version, if anything (desktop: the updater;
/// Android: GitHub's API from the webview).
async fn find(platform: Platform) -> Result<Option<UpdateInfo>, String> {
    if platform == Platform::Android {
        let v = if crate::bridge::is_preview() {
            crate::mock::preview_latest()
        } else {
            wasm_bindgen_futures::JsFuture::from(latest_release())
                .await
                .ok()
                .and_then(|v| v.as_string())
        };
        let Some(json) = v else {
            return Err(CHECK_FAILED.into());
        };
        let l: Latest = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        return Ok(Some(UpdateInfo {
            version: l.tag.trim_start_matches('v').to_string(),
            notes: l.notes,
            installable: false,
        }));
    }
    invoke::<_, Option<UpdateInfo>>("update_check", &NoArgs {}).await
}

async fn check(ctx: AppCtx, u: UpdateCtx, asked: bool) {
    u.status.set(asked.then(|| CHECKING.to_string()));
    let current = u.version.get_untracked();
    let skipped = u.settings.get_untracked().skipped;
    match find(ctx.platform.get_untracked()).await {
        Ok(Some(info))
            if offer_update(
                &info.version,
                &current,
                if asked { None } else { skipped.as_deref() },
            ) =>
        {
            u.available.set(Some(info));
            u.status.set(None);
        }
        Ok(_) => u.status.set(asked.then(|| UP_TO_DATE.to_string())),
        Err(_) => u.status.set(asked.then(|| CHECK_FAILED.to_string())),
    }
}

/// Loads settings and version, then checks once if allowed.
pub fn start(ctx: AppCtx) -> UpdateCtx {
    let u = UpdateCtx {
        settings: RwSignal::new(Settings::default()),
        version: RwSignal::new(String::new()),
        available: RwSignal::new(None),
        status: RwSignal::new(None),
        busy: RwSignal::new(false),
    };
    provide_context(u);
    spawn_local(async move {
        if let Ok(v) = invoke::<_, String>("app_version", &NoArgs {}).await {
            u.version.set(v);
        }
        if let Ok(s) = invoke::<_, Settings>("get_settings", &NoArgs {}).await {
            u.settings.set(s);
        }
        // The platform arrives with the first commands; wait for it.
        let _ = invoke::<_, Platform>("platform", &NoArgs {})
            .await
            .map(|p| ctx.platform.set(p));
        if u.settings.get_untracked().check_updates {
            check(ctx, u, false).await;
        }
    });
    u
}

fn save(u: UpdateCtx) {
    let settings = u.settings.get_untracked();
    spawn_local(async move {
        let _ = invoke::<_, ()>("set_settings", &SettingsArg { settings }).await;
    });
}

#[component]
pub fn Banner() -> impl IntoView {
    let u = expect_context::<UpdateCtx>();
    view! {
        {move || u.available.get().map(|info| {
            let v = info.version.clone();
            let install = info.installable;
            view! {
                <div class="update-banner" role="status">
                    <span>{update_available(&info.version)}</span>
                    <button class="link" on:click=move |_| {
                        u.settings.update(|s| s.skipped = Some(v.clone()));
                        save(u);
                        u.available.set(None);
                    }>{NOT_NOW}</button>
                    {if install {
                        view! {
                            <button class="primary" disabled=move || u.busy.get() on:click=move |_| {
                                u.busy.set(true);
                                spawn_local(async move {
                                    if let Err(e) = invoke::<_, ()>("update_install", &NoArgs {}).await {
                                        u.status.set(Some(e));
                                        u.busy.set(false);
                                    }
                                });
                            }>{move || if u.busy.get() { UPDATING } else { UPDATE_RESTART }}</button>
                        }.into_any()
                    } else {
                        view! {
                            <button class="primary" on:click=move |_| spawn_local(async move {
                                let _ = invoke::<_, ()>("open_release_page", &NoArgs {}).await;
                            })>{DOWNLOAD}</button>
                        }.into_any()
                    }}
                </div>
            }
        })}
    }
}

#[component]
pub fn About() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let u = expect_context::<UpdateCtx>();
    view! {
        <details class="about">
            <summary>{ABOUT}</summary>
            <p>{VERSION}" "{move || u.version.get()}
                " " <button class="link" on:click=move |_| spawn_local(async move {
                    let _ = invoke::<_, ()>("open_release_page", &NoArgs {}).await;
                })>{WHATS_NEW}</button>
            </p>
            <label class="about-toggle">
                <input type="checkbox" prop:checked=move || u.settings.get().check_updates
                    on:change=move |e| {
                        let on = event_target_checked(&e);
                        u.settings.update(|s| s.check_updates = on);
                        save(u);
                    }/>
                <span>{CHECK_AT_START}<br/><span class="muted">{CHECK_AT_START_NOTE}</span></span>
            </label>
            <p>
                <button class="link" on:click=move |_| spawn_local(check(ctx, u, true))>{CHECK_NOW}</button>
                " " <span class="muted" role="status">{move || u.status.get()}</span>
            </p>
            <p class="muted">{LICENCE_LINE}</p>
        </details>
    }
}
