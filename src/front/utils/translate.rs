use std::collections::HashMap;
use leptos::{component, view, IntoView};
use leptos::html::InnerHtmlAttribute;
use leptos::prelude::{Get, IntoAny};
use leptos::suspense::Transition;
use leptos::prelude::ElementChild;
use crate::front::utils::fluent::FluentManager::FluentManager;
use crate::front::utils::users_data::ClientState;

fn translationHydratable_get(translation: Option<String>) -> Option<String>
{
	// Preserve the Option<String> view type without ever producing the empty-view
	// placeholder: SSR may already have emitted the translated text.
	return Some(translation.unwrap_or_default());
}

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
			// Never return None here: the SSR response may already contain the
			// translation while the client Resource is not hydrated yet.
			{move || translationHydratable_get(translate.get())}
		</Transition>
	}
}

#[cfg(test)]
mod tests
{
	use super::translationHydratable_get;

	#[test]
	fn textTranslation_neverBecomesEmptyViewDuringHydration()
	{
		assert_eq!(translationHydratable_get(None),Some(String::new()));
		assert_eq!(translationHydratable_get(Some("Translated".to_string())),Some("Translated".to_string()));
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
