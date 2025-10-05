use std::time::Duration;
use leptoaster::{provide_toaster, Toaster};
use leptos::view;
use leptos::IntoView;
use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Link, Meta, MetaTags, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::{hooks, path};
use leptos_use::{use_locales};
use leptos::__reexports::wasm_bindgen_futures::spawn_local;
use crate::api::login::{API_user_isConnected, LoginStatus};
use crate::front::pages::home::Home;
use crate::front::pages::connection::Connection;
use crate::front::pages::inscription::Inscription;
use crate::front::utils::translate::{Translate};
use crate::front::utils::usersData::{UserData};
use crate::HWebTrace;

pub fn shell(options: LeptosOptions) -> impl IntoView {
	//	<meta http-equiv="Content-Security-Policy" content="default-src https: * 'unsafe-inline' 'unsafe-eval' 'strict-dynamic' 'wasm-unsafe-eval'; script-src-elem *"/>
	view! {
		<!DOCTYPE html>
		<html lang="en">
			<head>
				<meta http-equiv="content-type" content="text/html; charset=UTF-8"/>
				<meta name="viewport" content="width=device-width, initial-scale=1"/>
				<meta http-equiv="Referrer-Policy" content="no-referrer, strict-origin-when-cross-origin"/>
				<meta lang="fr" name="description" content="Webhome"/>
				<meta lang="en" name="description" content="Webhome"/>
				//<meta http-equiv="Content-Security-Policy" content="script-src https: 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval'"/> // actuellement instable avec leptos ?
				<AutoReload options=options.clone() />
				<HydrationScripts options islands=true/>
				<MetaTags/>
			</head>
			<body>
				<App/>
			</body>
		</html>
	}
}

#[island]
pub fn App() -> impl IntoView {
	// Provides context that manages stylesheets, titles, meta tags, etc.
	provide_meta_context();
	provide_toaster();


	Effect::new(move |_| {
		let (userData, setUserData) = UserData::cookie_signalGet();
		if (userData.read().is_none())
		{
			let locales = use_locales();
			setUserData.set(Some(UserData::new(locales.get().first().unwrap_or(&"EN".to_string()))));
		}

		let (userData, setUserData) = UserData::cookie_signalGet();

		let navigate = hooks::use_navigate();
		spawn_local(async move {
			let result = API_user_isConnected().await;
			match result {
				Ok(LoginStatus::USER_IS_CONNECTED(true)) => {
					let (userData, setUserData) = UserData::cookie_signalGet();
					let mut userData = userData.read().clone().unwrap_or(UserData::new(&"EN".to_string()));
					userData.login_force_connect();
					setUserData.set(Some(userData));
					navigate("/home", Default::default());
				}
				_ => {}
			};
		});
	});

	// <nav class="mainmenu">
	// 	<ul>
	// 	<li></li>
	// 	<li><span class="clickable"><A href="/"><Translate key="menu_home"/></A></span></li>
	// 	<li><span class="unclickable"><TranslateCurrentLang/></span>
	// 	<ul>
	// 	<li on:click=move |_| userData.write().lang_set("EN")><span class="clickable"><Translate key="swap_to_EN"/></span></li>
	// 	<li on:click=move |_| userData.write().lang_set("FR")><span class="clickable"><Translate key="swap_to_FR"/></span></li>
	// 	</ul>
	// 	</li>
	// 	</ul>
	// 	</nav>

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
			// content for this welcome page
			<Router>
				<section>
					<Routes fallback=|| Page404>
						<Route path=path!("/") view=Connection/>
						<Route path=path!("/newuser") view=Inscription/>
						<Route path=path!("/home") view=Home/>
					</Routes>
				</section>
			</Router>
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