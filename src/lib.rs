#![allow(unused_parens)]
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]


pub mod entry;
pub mod front;
mod api;
pub mod global_security;
#[cfg(feature = "ssr")]
pub mod server;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
	leptos::mount::hydrate_islands();
}
