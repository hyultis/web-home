use leptoaster::{expect_toaster};
use leptos::ev::SubmitEvent;
use leptos::prelude::{AriaAttributes, BindAttribute, ClassAttribute, GlobalAttributes, IntoAny};
use leptos::prelude::{signal, ElementChild, Get};
use leptos::prelude::{OnAttribute, RenderHtml};
use leptos::{island, view, IntoView};
use leptos::reactive::spawn_local_scoped;
use leptos_router::components::A;
use leptos_router::hooks;
use crate::front::utils::all_front_enum::{AllFrontErrorEnum, AllFrontLoginEnum};
use crate::front::utils::toaster_helpers::{toastingErr, toastingSuccess};
use crate::front::utils::translate::{Translate, TranslateText};
use crate::front::utils::users_data::{ClientCryptoContext, ClientState};
use crate::front::modules::module_holder::ModuleHolder;
use crate::HWebTrace;

#[island]
pub fn Connection() -> impl IntoView {

	let login = signal("".to_string());
	let pwd = signal("".to_string());

	let submit = move |event: SubmitEvent| {
		event.prevent_default();
		let login = login.0.get().clone();
		let pwd = pwd.0.get().clone();
		let navigate = hooks::use_navigate();
		let toaster = expect_toaster();
		let clientState = ClientState::expect();

		spawn_local_scoped(async move {
			let crypto = match ClientCryptoContext::login_get(login, pwd).await
			{
				Ok(crypto) => crypto,
				Err(reason) => {
					HWebTrace!("user NOT logged because {}", &reason);
					toastingErr(&toaster, reason).await;
					return;
				},
			};
			ModuleHolder::lifecycle_close();
			if (clientState.login_apply(crypto).is_err())
			{
				if (ClientCryptoContext::logout().await.is_some())
				{
					HWebTrace!("server session cleanup failed after local storage error");
				}
				toastingErr(&toaster, AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
				return;
			}
			toastingSuccess(&toaster, AllFrontLoginEnum::LOGIN_USER_CONNECTED).await;
			HWebTrace!("user logged");
			navigate("/home", Default::default());
		});
	};

	view! {
		<div class="page_layout">
			<main class="auth_page">
				<section class="centered_box auth_card" aria-labelledby="connection-title">
					<img src="/webhome.png" alt="WebHome" class="auth_logo" width="88" height="88"/>
					<p class="auth_description">
						<Translate key="pageRoot_desc"/>
					</p>
					<h1 id="connection-title"><Translate key="pageRoot_title_login"/></h1>
					<form class="login_box" on:submit=submit>
						<div class="auth_field">
							<label for="connection-login"><Translate key="pageRoot_form_login"/></label>
							<input id="connection-login" type="text" name="login" autocomplete="username" bind:value=login/>
						</div>
						<div class="auth_field">
							<label for="connection-pwd"><Translate key="pageRoot_form_pwd"/></label>
							<input id="connection-pwd" type="password" name="pwd" autocomplete="current-password" bind:value=pwd/>
						</div>
						<div class="auth_actions">
							<button class="auth_submit" type="submit"><TranslateText key="pageRoot_form_submit_login"/></button>
						</div>
					</form>
					{
						//crate::api::ALLOW_REGISTRATION.wait();
						let allowRegistration = crate::api::ALLOW_REGISTRATION.get().map(|ab| ab.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false);
						if(allowRegistration) {
							view!{<div class="auth_secondary"><A href="/newuser"><Translate key="pageRoot_signup"/></A></div>}.into_any()
						} else {
							view!{}.into_any()
						}
					}
				</section>
			</main>
			<footer class="site_footer">
				<Translate key="pageRoot_foot"/>
			</footer>
		</div>
	}.into_any()
}

#[cfg(test)]
mod authenticationForm_contractTests
{
	const CONNECTION_SOURCE: &str = include_str!("connection.rs");
	const INSCRIPTION_SOURCE: &str = include_str!("inscription.rs");

	fn assertFieldContract(source: &str, fieldId: &str, autocomplete: &str)
	{
		assert!(source.contains(&format!("for=\"{}\"",fieldId)));
		assert!(source.contains(&format!("id=\"{}\"",fieldId)));
		assert!(source.contains(&format!("autocomplete=\"{}\"",autocomplete)));
	}

	#[test]
	fn authenticationFields_haveLabelsAndAutocompleteHints()
	{
		assertFieldContract(CONNECTION_SOURCE,"connection-login","username");
		assertFieldContract(CONNECTION_SOURCE,"connection-pwd","current-password");
		assertFieldContract(INSCRIPTION_SOURCE,"inscription-login","username");
		assertFieldContract(INSCRIPTION_SOURCE,"inscription-pwd","new-password");
	}

	#[test]
	fn authenticationForms_useNativeSubmit()
	{
		let formSignature = ["<form class=\"login_box\"", " on:submit=submit>"].concat();
		let submitType = ["type=\"", "submit\""].concat();
		let legacyButton = ["<input type=\"", "button\""].concat();

		for source in [CONNECTION_SOURCE,INSCRIPTION_SOURCE]
		{
			assert!(source.contains(&formSignature));
			assert!(source.contains(&submitType));
			assert!(!source.contains(&legacyButton));
		}
	}
}
