use leptoaster::{expect_toaster, ToasterContext};
use leptos::prelude::{use_context, ArcRwSignal, Callback, ClassAttribute, Effect, IntoAny, OnAttribute, Read, RenderHtml, RwSignal, Set, Update, Write};
use leptos::{island, view, IntoView};
use leptos::__reexports::wasm_bindgen_futures::spawn_local;
use leptos::prelude::ElementChild;
use leptos_router::hooks;
use crate::front::modules::components::Backable;
use crate::front::modules::link::Link;
use crate::front::modules::ModuleHolder;
use crate::front::utils::dialog::DialogManager;
use crate::front::utils::users_data::UserData;
use crate::HWebTrace;
use std::ops::DerefMut;
use leptos::ev::MouseEvent;
use crate::front::utils::all_front_enum::AllFrontUIEnum;
use crate::front::utils::fluent::FluentManager::FluentManager;
use crate::front::utils::toaster_helpers::{toastingErr, toastingSuccess};
// https://iconoir.com/
// plus

#[island]
pub fn Home() -> impl IntoView {
	let editMode = RwSignal::new(false);
	let moduleContent = ArcRwSignal::new(ModuleHolder::new());
	let dialog = use_context::<DialogManager>().expect("DialogManager missing");
	let toaster = expect_toaster();

	let editModeValidateFn = editMode_validate(moduleContent.clone(), editMode.clone(), toaster.clone(), dialog.clone());

	let editModeCancelFn = editMode_cancel(moduleContent.clone(), editMode.clone(), toaster.clone(), dialog.clone());

	let editModeActivateFn = move |_| {
		editMode.update(|content|{
			*content = true;
		});
	};

	let toasterInnerDisconnect = toaster.clone();
	let disconnectFn = move |_| {
		let navigate = hooks::use_navigate();
		let toaster = toasterInnerDisconnect.clone();

		spawn_local(async move {
			let (userData, setUserData) = UserData::cookie_signalGet();
			let mut userData = userData.read().clone().unwrap_or(UserData::new(&"EN".to_string()));
			userData.login_disconnect().await;
			toastingSuccess(toaster,"LOGIN_USER_DISCONNECTED").await;
			HWebTrace!("user disconnected");
			setUserData.set(Some(userData));
			navigate("/", Default::default());
		});
	};

	let moduleContentInnerInitialLoad = moduleContent.clone();
	Effect::new(move || {
		moduleContentInnerInitialLoad.update(|holder|{
			let holder = holder.links_get_mut();
				holder.push(Link::new("google".to_string(),"https://google.fr".to_string()));
				holder.push(Link::new("reddit".to_string(),"https://reddit.com".to_string()));
			holder.push(Link::new("moncul".to_string(),"https://reddit.com".to_string()));
			holder.push(Link::new("data".to_string(),"https://reddit.com".to_string()));
		});
	});

	let moduleContentInnerView = moduleContent.clone();
	view! {
		<div class="home_header">
			<div class="left">
				{move || {
					//HWebTrace!("home {}",*editMode.read());
					let Some(binding) = moduleContentInnerView.try_read() else {return view!{<span/>}.into_any()};
					let tmp = binding.links_get();
					tmp.draw(editMode)
				}}
			</div>
			<div class="right">
				<i class="iconoir-key" on:click=disconnectFn></i>
				{move || {
                    let editModeValidateFn = editModeValidateFn.clone();
                    let editModeCancelFn = editModeCancelFn.clone();
					if *editMode.read()
					{
						view!{
							<i class="iconoir-check button_ok" on:click=editModeValidateFn></i>
							<i class="iconoir-xmark button_danger" on:click=editModeCancelFn></i>
						}.into_any()
					}
					else
					{
						view!{<i class="iconoir-edit-pencil" on:click=editModeActivateFn></i>}.into_any()
					}
				}}
			</div>
			<hr style="clear: both;"/>
		</div>
	}
}

fn editMode_cancel(moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
                     editModeInnerValidate: RwSignal<bool>,
                     toasterInnerValidate: ToasterContext,
                     dialog: DialogManager) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let moduleContentInnerValidate = moduleContentInnerValidate.clone();
		let editModeInnerValidate = editModeInnerValidate.clone();
		let toasterInnerValidate = toasterInnerValidate.clone();

		dialog.open("Annuler les changements ?", move || {
			view!{
				<span/>
			}.into_any()
		}, Some(Callback::new(move |_| {
			let moduleContentInnerValidate = moduleContentInnerValidate.clone();
			let editModeInnerValidate = editModeInnerValidate.clone();
			let toasterInnerValidate = toasterInnerValidate.clone();
			spawn_local(async move {
				let Some(mut guard) = moduleContentInnerValidate.try_write() else {return};
				let (userData, setUserData) = UserData::cookie_signalGet();
				let userData = userData.read().clone().unwrap_or(UserData::new(&"EN".to_string()));
				let Some(login) = userData.login_get() else {return};
				let modules: &mut ModuleHolder = guard.deref_mut();
				let error = (*modules).editMode_validate(login).await;
				editModeInnerValidate.update(|content| {
					*content = false;
				});

				if let Some(err) = error
				{
					let translated = FluentManager::singleton().translateParamsLess(userData.lang_get(),err.to_string()).await;
					toastingErr(toasterInnerValidate,translated).await;
				}
				else
				{
					let translated = FluentManager::singleton().translateParamsLess(userData.lang_get(),AllFrontUIEnum::VALID.to_string()).await;
					toastingSuccess(toasterInnerValidate,translated).await;
				}
			});

		})), None);
	};
}

fn editMode_validate(moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
                     editModeInnerValidate: RwSignal<bool>,
                     toasterInnerValidate: ToasterContext,
                     dialog: DialogManager) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let moduleContentInnerValidate = moduleContentInnerValidate.clone();
		let editModeInnerValidate = editModeInnerValidate.clone();
		let toasterInnerValidate = toasterInnerValidate.clone();

		dialog.open("Enregistrer les changements ?", move || {
			view!{
				<span/>
			}.into_any()
		}, Some(Callback::new(move |_| {
			let moduleContentInnerValidate = moduleContentInnerValidate.clone();
			let editModeInnerValidate = editModeInnerValidate.clone();
			let toasterInnerValidate = toasterInnerValidate.clone();
			spawn_local(async move {
				let Some(mut guard) = moduleContentInnerValidate.try_write() else {return};
				let (userData, setUserData) = UserData::cookie_signalGet();
				let userData = userData.read().clone().unwrap_or(UserData::new(&"EN".to_string()));
				let Some(login) = userData.login_get() else {return};
				let modules: &mut ModuleHolder = guard.deref_mut();
				let error = (*modules).editMode_validate(login).await;
				editModeInnerValidate.update(|content| {
					*content = false;
				});

				if let Some(err) = error
				{
					let translated = FluentManager::singleton().translateParamsLess(userData.lang_get(),err.to_string()).await;
					toastingErr(toasterInnerValidate,translated).await;
				}
				else
				{
					let translated = FluentManager::singleton().translateParamsLess(userData.lang_get(),AllFrontUIEnum::VALID.to_string()).await;
					toastingSuccess(toasterInnerValidate,translated).await;
				}
			});

		})), None);
	};
}