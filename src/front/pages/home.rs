use leptoaster::{expect_toaster, ToastBuilder, ToastLevel};
use leptos::prelude::{ClassAttribute, Effect, IntoAny, OnAttribute, Read, RenderHtml, RwSignal, Set, Update, Write};
use leptos::{island, view, IntoView};
use leptos::__reexports::wasm_bindgen_futures::spawn_local;
use leptos::prelude::ElementChild;
use leptos_router::hooks;
use crate::front::modules::components::Backable;
use crate::front::modules::link::Link;
use crate::front::modules::ModuleHolder;
use crate::front::utils::fluent::FluentManager::FluentManager;
use crate::front::utils::usersData::UserData;
use crate::HWebTrace;

// https://iconoir.com/
// plus

#[island]
pub fn Home() -> impl IntoView {
	let editMode = RwSignal::new(false);
	let moduleContent = RwSignal::new(ModuleHolder::new());

	let editModeSwap = move |_| {
		let mut content = editMode.write();
		*content = !*content;
	};

	let disconnect = move |_| {
		let navigate = hooks::use_navigate();
		let toaster = expect_toaster();

		spawn_local(async move {
			let (userData, setUserData) = UserData::cookie_signalGet();
			let mut userData = userData.read().clone().unwrap_or(UserData::new(&"EN".to_string()));
			userData.login_disconnect().await;
			toaster.toast(ToastBuilder::new(FluentManager::singleton().translateParamsLess(userData.lang_get(), "LOGIN_USER_DISCONNECTED").await)
				.with_expiry(Some(5_000))
				.with_level(ToastLevel::Success));
			HWebTrace!("user disconnected");
			setUserData.set(Some(userData));
			navigate("/", Default::default());
		});
	};

	Effect::new(move || {
		moduleContent.update(|holder|{
			let holder = holder.links_get_mut();
				holder.push(Link::new("google".to_string(),"https://google.fr".to_string()));
				holder.push(Link::new("reddit".to_string(),"https://reddit.com".to_string()));
		});
	});

	view! {
		<div class="home_header">
			<div class="left">
				{move || {
					//HWebTrace!("home {}",*editMode.read());
					let binding = moduleContent.read();
					let tmp = binding.links_get();
					tmp.draw(editMode)
				}}
			</div>
			<div class="right">
				<i class="iconoir-key" on:click=disconnect></i>
				{move || {
					if *editMode.read()
					{
						view!{
							<i class="iconoir-check button_ok" on:click=editModeSwap></i>
							<i class="iconoir-xmark button_danger" on:click=editModeSwap></i>
						}.into_any()
					}
					else
					{
						view!{<i class="iconoir-edit-pencil" on:click=editModeSwap></i>}.into_any()
					}
				}}
			</div>
			<hr style="clear: both;"/>
		</div>
	}
}