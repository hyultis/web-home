use std::collections::HashMap;
use std::sync::Arc;
use leptos::{component, view, IntoView};
use leptos::children::ChildrenFn;
use leptos::html::InnerHtmlAttribute;
use leptos::prelude::{Get, IntoAny, Read};
use leptos::suspense::Transition;
use leptos::prelude::ElementChild;
use crate::front::utils::fluent::FluentManager::FluentManager;
use crate::front::utils::users_data::UserData;

#[component]
pub fn TranslateCurrentLang() -> impl IntoView {
	let (userData, _) = UserData::cookie_signalGet();

	view! { <TranslateFn key=move || {
		let mut lang = "EN".to_string();
		if let Some(userDataContent) = &*userData.read()
		{
			lang = userDataContent.lang_get().clone();
		}
		return format!("swap_to_{}",lang);
	}/> }.into_any()
}

#[component]
pub fn Translate(#[prop(into)] key: String,
                 #[prop(optional)]
                 params: HashMap<String,String>,
                 #[prop(optional)]
                 children: Option<ChildrenFn>) -> impl IntoView {

	if let Some(children) = children {
		return view!{
			<TranslateFn key=move || key.clone() params=params children=children/>
		}.into_any()
	}

	return view!{
		<TranslateFn key=move || key.clone() params=params/>
	}.into_any();
}

#[component]
pub fn TranslateFn(
	key: impl Fn() -> String + Send + Sync + 'static,
    #[prop(optional)]
	mut params: HashMap<String,String>,
	#[prop(optional)]
	children: Option<ChildrenFn>) -> impl IntoView {

	let key = Arc::new(key);
	let splitted= "{--$chidren--}";

	if(children.is_some())
	{
		params.insert("children".to_string(),splitted.to_string());
	}

	let params = Arc::new(params);
	let translate = FluentManager::getAsResource(key(),params);

	let altkey = key.clone();
	view! {
		<Transition fallback=move || view! { <span>{format!("{}_fallback",altkey.clone()())}</span> }.into_any()>
			{move || {
				translate.get().map(|translated|{
					if(translated.contains(splitted))
					{
						let splitVar = translated.split_once(splitted);
						let (prefix,suffix) = splitVar.unwrap();
						let prefix = prefix.to_string();
						let suffix = suffix.to_string();
						if let Some(children) = &children
						{
							view! { <span>""<span inner_html=move || prefix.clone()/>{children()}<span inner_html=move || suffix.clone()/></span> }.into_any()
						}
						else
						{
							view! { <span>""<span inner_html=move || prefix.clone()/><span inner_html=move || suffix.clone()/></span> }.into_any()
						}
					}
					else
					{
						// first "" is important to fix the hydration bug from fallback
						view! { <span>""<span inner_html=move || translated.clone()/></span> }.into_any()
					}
				})
			}}
		</Transition>
	}
}