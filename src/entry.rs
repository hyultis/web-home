use std::sync::atomic::AtomicBool;
use leptoaster::{provide_toaster, Toaster};
use leptos::view;
use leptos::IntoView;
use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Link, Meta, MetaTags, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::{hooks, path};
use leptos_use::use_locales;
use leptos::__reexports::wasm_bindgen_futures::spawn_local;
use crate::api::{ALLOW_REGISTRATION, IS_TRACE_FRONT_LOG};
use crate::front::pages::home::Home;
use crate::front::pages::connection::Connection;
use crate::front::pages::inscription::Inscription;
use crate::front::utils::dialog::{DialogHost, DialogManager};
use crate::front::utils::translate::Translate;
use crate::front::utils::users_data::UserData;

pub fn shell((options,trace_front_log,allowRegistration): (LeptosOptions, bool, bool)) -> impl IntoView {
	//	<meta http-equiv="Content-Security-Policy" modules="default-src https: * 'unsafe-inline' 'unsafe-eval' 'strict-dynamic' 'wasm-unsafe-eval'; script-src-elem *"/>

	view! {
		<!DOCTYPE html>
		<html lang="en">
			<head>
				<meta http-equiv="modules-type" content="text/html; charset=UTF-8"/>
				<meta name="viewport" content="width=device-width, initial-scale=1"/>
				<meta http-equiv="Referrer-Policy" content="no-referrer, strict-origin-when-cross-origin"/>
				<meta lang="fr" name="description" content="Webhome"/>
				<meta lang="en" name="description" content="Webhome"/>
				//<meta http-equiv="Content-Security-Policy" modules="script-src https: 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval'"/> // actuellement instable avec leptos ?
				<AutoReload options=options.clone() />
				<HydrationScripts options islands=true/>
				<MetaTags/>
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

	let dialog_manager = DialogManager::new();
	provide_context(dialog_manager.clone());
	let (userDataSignal, setUserData) = UserData::cookie_signalGet();

	let is_initialized = RwSignal::new(false);
	Effect::new(move || {
		if(is_initialized.get_untracked()) {
			return;
		}
		is_initialized.set(true);
		let _ = IS_TRACE_FRONT_LOG.set(AtomicBool::new(traceFrontLog));
		let _ = ALLOW_REGISTRATION.set(AtomicBool::new(allowRegistration));

		// set default userData
		if (userDataSignal.read_untracked().is_none())
		{
			let locales = use_locales();
			setUserData.set(Some(UserData::new(locales.get().first().unwrap_or(&"EN".to_string()))));
		}

		// if user is connected, he directly go to is home page
		let navigate = hooks::use_navigate();
		spawn_local(async move {
			if let Some(userData) = &*userDataSignal.read_untracked()
			{
				if(userData.login_isConnected()) {
					navigate("/home", Default::default());
				}
			}
		});
	});

	view! {
		// injects a stylesheet into the document <head>
		// id=leptos means cargo-leptos will hot-reload this stylesheet
		<Stylesheet id="leptos" href="/pkg/webhome.css"/>
		<Stylesheet id="iconoir" href="https://cdn.jsdelivr.net/gh/iconoir-icons/iconoir@main/css/iconoir.css"/>

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
			<Translate key="page404_content" ><A href="/"><Translate key="menu_home"/></A></Translate>
		</article>
	}
}