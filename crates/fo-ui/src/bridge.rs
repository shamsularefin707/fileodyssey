//! Calls into the Tauri backend, or into `mock` when running in a plain
//! browser (`trunk serve`), so the UI can be previewed without the app.

use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "
export function has_tauri() { return !!(window.__TAURI__ && window.__TAURI__.core); }
export async function tauri_invoke(cmd, args) { return await window.__TAURI__.core.invoke(cmd, args); }
export function tauri_listen(ev, cb) { window.__TAURI__.event.listen(ev, (e) => cb(e.payload)); }
export function tauri_src(path) { return window.__TAURI__.core.convertFileSrc(path, 'reader'); }
export function fit_spans(root) {
  if (!root) return;
  for (const s of root.querySelectorAll('span[data-w]')) {
    s.style.transform = '';
    const natural = s.getBoundingClientRect().width;
    if (natural > 0) s.style.transform = `scaleX(${parseFloat(s.dataset.w) / natural})`;
  }
}
export function copy_text(t) { if (navigator.clipboard) navigator.clipboard.writeText(t).catch(() => {}); }
")]
extern "C" {
    fn has_tauri() -> bool;
    #[wasm_bindgen(catch)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
    fn tauri_listen(ev: &str, cb: &Closure<dyn Fn(JsValue)>);
    fn tauri_src(path: &str) -> String;
    /// Stretches each transparent text span to the width PDFium measured.
    pub fn fit_spans(root: &web_sys::Element);
    pub fn copy_text(text: &str);
}

/// URL of a `reader://` resource such as `"3/page/0?w=1024"`.
pub fn reader_url(path: &str) -> String {
    if has_tauri() {
        tauri_src(path)
    } else {
        crate::mock_reader::url(path)
    }
}

fn to_js<A: Serialize>(args: &A) -> JsValue {
    // Plain JS objects (not Maps) so Tauri can read the arguments.
    let ser = serde_wasm_bindgen::Serializer::json_compatible();
    args.serialize(&ser).unwrap_or(JsValue::NULL)
}

pub async fn invoke<A: Serialize, R: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, String> {
    let value = if has_tauri() {
        tauri_invoke(cmd, to_js(args))
            .await
            .map_err(|e| e.as_string().unwrap_or_else(|| format!("{e:?}")))?
    } else {
        crate::mock::invoke(cmd, to_js(args)).await?
    };
    serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string())
}

pub fn listen<T: DeserializeOwned + 'static>(event: &str, f: impl Fn(T) + 'static) {
    let handler = move |v: JsValue| {
        if let Ok(t) = serde_wasm_bindgen::from_value::<T>(v) {
            f(t);
        }
    };
    if has_tauri() {
        let cb = Closure::<dyn Fn(JsValue)>::new(handler);
        tauri_listen(event, &cb);
        cb.forget(); // listeners live for the whole app session
    } else {
        crate::mock::listen(event, Box::new(handler));
    }
}

pub fn is_preview() -> bool {
    !has_tauri()
}
