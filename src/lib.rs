#![allow(unused_parens)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(non_camel_case_types)]


pub mod entry;
mod front;
mod api;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
	leptos::mount::hydrate_islands();
}