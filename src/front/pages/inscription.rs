use std::sync::Arc;
use leptoaster::{expect_toaster, ToastBuilder, ToastLevel};
use leptos::prelude::{ElementChild, GetUntracked, IntoAny, Transition};
use leptos::prelude::BindAttribute;
use leptos::prelude::{signal, ClassAttribute, Get, OnAttribute, RenderHtml, Set};
use leptos::{island, view, IntoView};
use leptos::__reexports::wasm_bindgen_futures::spawn_local;
use leptos_router::components::A;
use leptos_router::*;
use crate::front::utils::fluent::FluentManager::FluentManager;
use crate::front::utils::translate::Translate;
use crate::front::utils::users_data::UserData;
use crate::HWebTrace;

#[derive(Clone)]
struct StoredNavigateFn {
	pub navigate: Arc<dyn Fn(&str, NavigateOptions)>,
}

#[island]
pub fn Inscription() -> impl IntoView {
	let login = signal("".to_string());
	let pwd = signal("".to_string());

	let submit = move |_| {
		let login = login.0.get_untracked().clone();
		let pwd = pwd.0.get_untracked().clone();
		let navigate = hooks::use_navigate();
		let toaster = expect_toaster();

		spawn_local(async move {
			let (userData, setUserData) = UserData::cookie_signalGet();
			let mut userData = userData.get_untracked().unwrap_or(UserData::new(&"EN".to_string()));
			if let Some(reason) = userData.login_signUp(login, pwd).await
			{
				HWebTrace!("user NOT logged because {:?}",&reason);
				toaster.toast(ToastBuilder::new(FluentManager::singleton().translateParamsLess(userData.lang_get(), reason).await)
					.with_expiry(Some(5_000))
					.with_level(ToastLevel::Error));
			} else {
				toaster.toast(ToastBuilder::new(FluentManager::singleton().translateParamsLess(userData.lang_get(), "LOGIN_USER_SIGNEDUP").await)
					.with_expiry(Some(5_000))
					.with_level(ToastLevel::Success));
				HWebTrace!("user logged");
				setUserData.set(Some(userData));
				navigate("/", Default::default());
			}
		});
	};

	let submitTranslate = FluentManager::getAsResourceParamsLess("pageRoot_form_submit_signup");

	view! {
		<div class="centered_box">
			<img src="/webhome.png" alt="webhome logo" class="logo" style="width: 100px;"/>
			<h1><Translate key="pageRoot_title_signup"/></h1>
			<div class="login_box">
				<label for="login"><Translate key="pageRoot_form_login"/></label><input type="text" name="login" bind:value=login/>
				<label for="pwd"><Translate key="pageRoot_form_pwd"/></label><input type="text" name="pwd" bind:value=pwd/>
				<Transition fallback=move || view! { <span/> }.into_any()>
					{move || {
						submitTranslate.get().map(|translated|{
							view! {<input type="button" on:click=submit value=translated />}
						})
					}}
				</Transition>
			</div>
			<A href="/">"retour"</A>
		</div>

		<footer>
			<Translate key="pageRoot_foot"/>
		</footer>
	}
}