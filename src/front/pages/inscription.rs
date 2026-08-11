use leptoaster::{expect_toaster};
use leptos::ev::SubmitEvent;
use leptos::prelude::{AriaAttributes, ElementChild, GetUntracked, GlobalAttributes, IntoAny};
use leptos::prelude::BindAttribute;
use leptos::prelude::{signal, ClassAttribute, OnAttribute, RenderHtml};
use leptos::{island, view, IntoView};
use leptos::reactive::spawn_local_scoped;
use leptos_router::components::A;
use leptos_router::*;
use crate::front::utils::all_front_enum::AllFrontLoginEnum;
use crate::front::utils::toaster_helpers::{toastingErr, toastingSuccess};
use crate::front::utils::translate::{Translate, TranslateText};
use crate::front::utils::users_data::ClientCryptoContext;
use crate::HWebTrace;

#[island]
pub fn Inscription() -> impl IntoView {
	let login = signal("".to_string());
	let pwd = signal("".to_string());

	let submit = move |event: SubmitEvent| {
		event.prevent_default();
		let navigate = hooks::use_navigate();
		let login = login.0.get_untracked().clone();
		let pwd = pwd.0.get_untracked().clone();
		let toaster = expect_toaster();


		spawn_local_scoped(async move {
			let allowRegistration = crate::api::ALLOW_REGISTRATION.get().map(|ab| ab.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false);
			if(!allowRegistration) {
				toastingErr(&toaster,AllFrontLoginEnum::SIGN_DISABLED).await;
				return;
			}

			if let Err(reason) = ClientCryptoContext::signUp(login, pwd).await
			{
				HWebTrace!("user NOT logged because {}",&reason);
				toastingErr(&toaster,reason).await;
			} else {
				toastingSuccess(&toaster,AllFrontLoginEnum::LOGIN_USER_SIGNEDUP).await;
				navigate("/", Default::default());
			}
		});
	};

	view! {
		<div class="page_layout">
			<main class="auth_page">
				<section class="centered_box auth_card" aria-labelledby="inscription-title">
					<img src="/webhome.png" alt="WebHome" class="auth_logo" width="88" height="88"/>
					<h1 id="inscription-title"><Translate key="pageRoot_title_signup"/></h1>
					<form class="login_box" on:submit=submit>
						<div class="auth_field">
							<label for="inscription-login"><Translate key="pageRoot_form_login"/></label>
							<input id="inscription-login" type="text" name="login" autocomplete="username" bind:value=login/>
						</div>
						<div class="auth_field">
							<label for="inscription-pwd"><Translate key="pageRoot_form_pwd"/></label>
							<input id="inscription-pwd" type="password" name="pwd" autocomplete="new-password" bind:value=pwd/>
						</div>
						<div class="auth_actions">
							<button class="auth_submit" type="submit"><TranslateText key="pageRoot_form_submit_signup"/></button>
						</div>
					</form>
					<div class="auth_secondary"><A href="/"><Translate key="menu_home"/></A></div>
				</section>
			</main>
			<footer class="site_footer">
				<Translate key="pageRoot_foot"/>
			</footer>
		</div>
	}.into_any()
}
