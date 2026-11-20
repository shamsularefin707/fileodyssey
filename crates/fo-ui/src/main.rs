#[cfg(target_arch = "wasm32")]
mod bridge;
#[cfg(target_arch = "wasm32")]
mod components;
#[cfg(target_arch = "wasm32")]
mod mock;
#[cfg(target_arch = "wasm32")]
mod mock_reader;

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(components::App);
}

/// The UI only runs in a webview; natively there is nothing to start.
#[cfg(not(target_arch = "wasm32"))]
fn main() {}
