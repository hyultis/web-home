use std::collections::HashMap;
use leptos::{component, view, IntoView};
use leptos::html::InnerHtmlAttribute;
use leptos::prelude::{Get, IntoAny};
use leptos::suspense::Transition;
use leptos::prelude::ElementChild;
use crate::front::utils::fluent::FluentManager::FluentManager;
use crate::front::utils::users_data::ClientState;

#[component]
pub fn TranslateCurrentLang() -> impl IntoView {
	let clientState = ClientState::expect();

	view! { <TranslateFn key=move || {
		let lang = clientState.lang_get();
		return format!("swap_to_{}",lang);
	}/> }.into_any()
}

#[component]
pub fn Translate(#[prop(into)] key: String,
                 #[prop(optional)]
                 params: HashMap<String,String>) -> impl IntoView {

	view!{
		<TranslateFn key=move || key.clone() params=params/>
	}
}

#[component]
pub fn TranslateText(#[prop(into)] key: String,
                     #[prop(optional)]
                     params: HashMap<String,String>) -> impl IntoView {
	let resourceKey = key.clone();
	let translate = FluentManager::getAsResource(move || resourceKey.clone(),params);

	view! {
		<Transition fallback=move || format!("{}_fallback",key)>
			{move || translate.get()}
		</Transition>
	}
}

#[component]
pub fn TranslateFn(
	key: impl Fn() -> String + Send + Sync + Clone + 'static,
    #[prop(optional)]
	params: HashMap<String,String>) -> impl IntoView {

	let translate = FluentManager::getAsHtmlResource(key.clone(),params);

	let altkey = key.clone();
	view! {
		<Transition fallback=move || view! { <span>{format!("{}_fallback",altkey.clone()())}</span> }.into_any()>
			{move || translate.get().map(|translated|{
					// The span is important to keep the hydrated view aligned with the fallback.
					view! { <span inner_html={translated}/> }.into_any()
				})
			}
		</Transition>
	}
}
