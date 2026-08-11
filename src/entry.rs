use leptoaster::{expect_toaster, provide_toaster, Toaster};
use leptos::view;
use leptos::IntoView;
use leptos::prelude::*;
use leptos::reactive::spawn_local_scoped;
use leptos_meta::{provide_meta_context, Link, Meta, MetaTags, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::{hooks, path};
use leptos_use::use_locales;
use crate::api::runtimeConfig_set;
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
				<HydrationScripts options islands=true/>
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
		// injects a stylesheet into the document <head>
		// id=leptos means cargo-leptos will hot-reload this stylesheet
		<Stylesheet id="leptos" href="/pkg/webhome.css"/>
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
		<Toaster stacked={false} />

		<div id="body">
			// modules for this welcome page
			<Router>
				<section>
					<Routes fallback=|| Page404>
						<Route path=path!("/") view=Connection/>
						<Route path=path!("/newuser") view=Inscription/>
						<Route path=path!("/home") view=Home/>
					</Routes>
				</section>
			</Router>
            <DialogHost manager=dialog_manager />
		</div>
	}
}


#[island]
pub fn Page404() -> impl IntoView {
	view!{
		<h2><Translate key="page404_title"/></h2>
		<article>
			<Translate key="page404_content"/> {" "}
			<A href="/"><Translate key="menu_home"/></A>{"."}
		</article>
	}
}
