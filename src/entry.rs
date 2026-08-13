use leptoaster::{expect_toaster, provide_toaster, Toaster};
use leptos::view;
use leptos::IntoView;
use leptos::prelude::*;
use leptos::reactive::spawn_local_scoped;
use leptos_meta::{provide_meta_context, HashedStylesheet, Html, Link, Meta, MetaTags, Title};
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::{hooks, path};
use leptos_use::use_locales;
use crate::api::runtimeConfig_set;
use crate::api::login::components::PasswordRotationError;
use crate::front::modules::module_holder::ModuleHolder;
use crate::front::pages::home::Home;
use crate::front::pages::connection::Connection;
use crate::front::pages::inscription::Inscription;
use crate::front::utils::dialog::{DialogHost, DialogManager};
use crate::front::utils::all_front_enum::AllFrontErrorEnum;
use crate::front::utils::toaster_helpers::{toasterOwner_provide, toastingErr};
use crate::front::utils::translate::Translate;
use crate::front::utils::users_data::ClientState;

pub fn shell((options,trace_front_log,allowRegistration): (LeptosOptions, bool, bool)) -> impl IntoView {
	#[cfg(feature="ssr")]
	if let Some(requestParts) = use_context::<http::request::Parts>()
	{
		if let Some(nonce) = requestParts.extensions.get::<leptos::nonce::Nonce>()
		{
			provide_context(nonce.clone());
		}
	}

	view! {
		<!DOCTYPE html>
		<html lang="en">
			<head>
				<meta http-equiv="modules-type" content="text/html; charset=UTF-8"/>
				<meta name="viewport" content="width=device-width, initial-scale=1"/>
				<meta http-equiv="Referrer-Policy" content="no-referrer, strict-origin-when-cross-origin"/>
				<meta lang="fr" name="description" content="Webhome"/>
				<meta lang="en" name="description" content="Webhome"/>
				<AutoReload options=options.clone() />
				<HashedStylesheet options=options.clone() id="leptos"/>
				<script src="asset_version_monitor.js"></script>
				<HydrationScripts options=options islands=true/>
				<MetaTags/>
				<script type="module" src="setUpWorkers.js"></script>
			</head>
			<body>
				<App traceFrontLog={trace_front_log} allowRegistration={allowRegistration}/>
			</body>
		</html>
	}
}

#[island]
pub fn App(traceFrontLog: bool,allowRegistration: bool) -> impl IntoView {
	// Provides context that manages stylesheets, titles, meta tags, etc.
	provide_meta_context();
	provide_toaster();
	toasterOwner_provide();

	runtimeConfig_set(traceFrontLog,allowRegistration);

	let dialog_manager = DialogManager::new();
	provide_context(dialog_manager.clone());
	let clientState = ClientState::new();
	provide_context(clientState.clone());
	let documentLangState = clientState.clone();
	let documentThemeState = clientState.clone();
	let locales = use_locales();

	let is_initialized = RwSignal::new(false);
	Effect::new(move || {
		if(is_initialized.get_untracked()) {
			return;
		}
		is_initialized.set(true);

		let defaultLang = locales.get_untracked().first().cloned().unwrap_or_else(|| "EN".to_string());
		let initializationResult = clientState.initialize(defaultLang);

		// if user is connected, he directly go to is home page
		let navigate = hooks::use_navigate();
		let toaster = expect_toaster();
		let clientState = clientState.clone();
		spawn_local_scoped(async move {
			if (initializationResult.is_err())
			{
				toastingErr(&toaster, AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
				return;
			}
			if (clientState.login_isConnected_untracked())
			{
				if (clientState.crypto_get().is_some())
				{
					if (clientState.passwordRotation_pendingIsAvailable_untracked())
					{
						ModuleHolder::network_suspend();
						clientState.passwordRotation_runningSet(true);
						let recoveryResult = clientState.passwordRotation_resume().await;
						clientState.passwordRotation_runningSet(false);
						match recoveryResult
						{
							Ok(true) => {
								ModuleHolder::network_resume();
								crate::front::utils::toaster_helpers::toastingSuccess(&toaster,"FRONTUI_OPTIONS_PASSWORD_SUCCESS").await;
							},
							Ok(false) => ModuleHolder::network_resume(),
							Err(PasswordRotationError::AUTH_REQUIRED) => {
								let storageClearFailed = clientState.local_clear().is_err();
								ModuleHolder::lifecycle_close();
								toastingErr(&toaster,PasswordRotationError::AUTH_REQUIRED).await;
								if (storageClearFailed)
								{
									toastingErr(&toaster,AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
								}
								navigate("/",Default::default());
								return;
							},
							Err(error) => {
								if (!clientState.passwordRotation_pendingIsAvailable_untracked())
								{
									ModuleHolder::network_resume();
								}
								toastingErr(&toaster,error).await;
							},
						}
					}
					navigate("/home", Default::default());
				}
				else if let Some(window) = web_sys::window()
				{
					// Temporary migration path for the former /home crypto cookie.
					// Once loaded there, ClientState moves it into localStorage and deletes it.
					let location = window.location();
					if (location.pathname().ok().as_deref() != Some("/home"))
					{
						let _ = location.set_href("/home");
					}
				}
			}
		});
	});

	view! {
		<Html
			{..}
			lang=move || documentLangState.lang_get().to_ascii_lowercase()
			style=move || format!("--theme-primary-hue: {};", documentThemeState.primaryHue_get())
		/>

		<Link
			id="iconoir"
			rel="stylesheet"
			href="https://cdn.jsdelivr.net/npm/iconoir@7.11.1/css/iconoir.css"
			integrity="sha384-luECWXGw+Rk0LDPKZ8m2vuzYJnGiJfFabF16BAqKVf7rdp1/jvaViZ+BFXFuaD5H"
			crossorigin="anonymous"
		/>

		<Link rel="icon" href="/favicon.png" type_="image/png" sizes="64x64" />

		// sets the document title
		<Title text="Web Home"/>
		<Meta name="description" content="Web Home"/>
		<div class="toaster_host" role="status" aria-live="polite" aria-atomic="false">
			<Toaster stacked={false} />
		</div>

		<div id="body">
			// modules for this welcome page
			<Router>
				<div class="route_host">
					<Routes fallback=|| Page404>
						<Route path=path!("/") view=Connection/>
						<Route path=path!("/newuser") view=Inscription/>
						<Route path=path!("/home") view=Home/>
					</Routes>
				</div>
			</Router>
            <DialogHost manager=dialog_manager />
		</div>
	}
}


#[island]
pub fn Page404() -> impl IntoView {
	view!{
		<div class="page_layout">
			<main class="error_page">
				<article class="centered_box error_card" aria-labelledby="page404-title">
					<div class="error_code" aria-hidden="true">"404"</div>
					<h1 id="page404-title"><Translate key="page404_title"/></h1>
					<p>
						<Translate key="page404_content"/> {" "}
						<A href="/"><Translate key="menu_home"/></A>{"."}
					</p>
				</article>
			</main>
			<footer class="site_footer">
				<Translate key="pageRoot_foot"/>
			</footer>
		</div>
	}
}
