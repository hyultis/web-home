use leptos::prelude::{GetUntracked, OnTargetAttribute, ReadUntracked, Signal, StyleAttribute, WriteSignal};
use leptos::prelude::{CollectView, Get, PropAttribute};
use crate::front::modules::components::Backable;
use crate::front::modules::ModuleHolder;
use crate::front::utils::all_front_enum::{AllFrontLoginEnum, AllFrontUIEnum};
use crate::front::utils::dialog::{DialogData, DialogManager};
use crate::front::utils::toaster_helpers::{toastingErr, toastingSuccess};
use crate::front::utils::users_data::UserData;
use crate::{HWebTrace};
use leptoaster::{expect_toaster, ToasterContext};
use leptos::task::spawn_local;
use leptos::ev::MouseEvent;
use leptos::prelude::ElementChild;
use leptos::prelude::{
	use_context, ArcRwSignal, Callback, ClassAttribute, Effect, IntoAny, OnAttribute, Read,
	RenderHtml, RwSignal, Set, Update, Write,
};
use leptos::{island, view, IntoView};
use leptos_router::{hooks, NavigateOptions};
use std::ops::DerefMut;
use leptos::logging::log;
use leptos_use::use_interval_fn;
use strum::IntoEnumIterator;
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::modules::module_positions::ModulePositions;
use crate::front::modules::module_type::{ModuleTypeDiscriminants, StringToModuleType};
// https://iconoir.com/
// plus

#[island]
pub fn Home() -> impl IntoView
{
	let editMode = RwSignal::new(false);
	let moduleContent = ArcRwSignal::new(ModuleHolder::new());
	let Some(dialogManager) = use_context::<DialogManager>() else {
		HWebTrace!("cannot get dialogManager in home");
		panic!("cannot get dialogManager in home");
	};
	let toaster = expect_toaster();

	// user data checker to force disconnect
	let toasterInner = toaster.clone();
	let (userDataSignal, setUserData) = UserData::cookie_signalGet();
	Effect::new(move || {
		let mut is_connected = false;
		if let Some(userDataInner) = userDataSignal.get()
		{
			is_connected = userDataInner.login_isConnected();
		}

		if userDataSignal.get().is_none() || !is_connected
		{
			let callback = user_disconnected(hooks::use_navigate(), toasterInner.clone(), userDataSignal.clone(), setUserData.clone(), false);
			callback(());
		}
	});

	// auto refresh cookie every 2 hour
	use_interval_fn(
		move || {
			let (userDataSignal, setUserData) = UserData::cookie_signalGet();
			setUserData.set(userDataSignal.get_untracked());
			log!("auto refresh cookie");
		},
		2 * 3600 * 1000,
	);

	// pre init ModuleHolder
	let moduleActions = ModuleActionFn::new(moduleContent.clone(),toaster.clone());
	let innerModuleActions = moduleActions.clone();
	moduleContent.update(|modules|{
		modules.moduleActions_set(innerModuleActions);
	});

	// initialise ModuleHolder
	let moduleContentInnerInitialLoad = moduleContent.clone();
	let toasterInnerInitialLoad = toaster.clone();
	let is_initialized = RwSignal::new(false);
	Effect::new(move || {
		if(is_initialized.get_untracked()) {
			return;
		}
		is_initialized.set(true);

		let moduleContentInnerInitialLoad = moduleContentInnerInitialLoad.clone();
		let toasterInnerInitialLoad = toasterInnerInitialLoad.clone();

		spawn_local(async move {
			let mut haveBeenCorrectlyInit = false;
			if let Some(mut guard) = moduleContentInnerInitialLoad.try_write()
			{
				let holder: &mut ModuleHolder = guard.deref_mut();

				let Some((login, _)) = UserData::loginLang_get_from_cookie()
				else
				{
					return;
				};
				let error = (*holder).editMode_cancel(login, true).await;
				if let Some(err) = error
				{
					toastingErr(&toasterInnerInitialLoad, err.to_string()).await;
				}
				else
				{
					haveBeenCorrectlyInit = true;
				}
			}

			if(haveBeenCorrectlyInit)
			{
				let holder = moduleContentInnerInitialLoad.read_untracked();
				let keys: Vec<String> = holder.blocks_get().keys().cloned().collect();
				for key in keys {
					holder.module_refresh(key, toasterInnerInitialLoad.clone()).await;
				}
			}
		});
	});

	let editModeValidateFn = editMode_validate(
		moduleContent.clone(),
		editMode.clone(),
		toaster.clone(),
		dialogManager.clone(),
	);

	let editModeCancelFn = editMode_cancel(
		moduleContent.clone(),
		editMode.clone(),
		toaster.clone(),
		dialogManager.clone(),
	);

	let editModeActivateFn = move |_| {

		HWebTrace!("editModeActivateFn");
		editMode.update(|content| {
			*content = true;
		});
	};

	let editModeAddModuleFn = editMode_AddBlock(moduleContent.clone(), dialogManager.clone());

	// disconnect func
	let toasterInner = toaster.clone();
	let disconnectFn = move |_| {
		let dialogContent = DialogData::new()
			.setTitle(AllFrontLoginEnum::LOGIN_USER_WANT_DISCONNECTED)
			.setOnValidate(Callback::new(user_disconnected(hooks::use_navigate(), toasterInner.clone(), userDataSignal.clone(), setUserData.clone(), true)));

		dialogManager.open(dialogContent);
	};

	let moduleContentInnerView = moduleContent.clone();
	let moduleContentInnerModuleView = moduleContent.clone();
	let moduleActionsInnerModuleView = moduleActions.clone();
	view! {
		<div class="home_body">
			<div class="header">
				<div class="left">
					{move || {
						let Some(binding) = moduleContentInnerView.clone().try_read() else {return view!{}.into_any()};
						let tmp = binding.links_get();
						tmp.draw(editMode,moduleActionsInnerModuleView.clone(),"links".to_string())
					}}
				</div>
				<div class="right">
					<i class="iconoir-key" on:click=disconnectFn></i>
					{move || {
						let editModeValidateFn = editModeValidateFn.clone();
						let editModeCancelFn = editModeCancelFn.clone();
						let editModeActivateFn = editModeActivateFn.clone();
						let editModeAddModuleFn = editModeAddModuleFn.clone();
						if *editMode.read()
						{
							view!{
								<i class="iconoir-plus-circle" on:click=editModeAddModuleFn></i>
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
			<div class="modules">
				{move || {
					let Some(binding) = moduleContentInnerModuleView.clone().try_read() else {return view!{<img src="cog03.svg" style="filter: invert(100%) sepia(0%) saturate(0%) hue-rotate(87deg) brightness(103%) contrast(101%);"/>}.into_any()};
					binding.blocks_get().iter().map( |(currentName,d)|d.draw(editMode,moduleActions.clone(),currentName.clone())).collect_view().into_any()
				}}
			</div>
		</div>
	}
}

fn editMode_cancel(
	moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
	editModeInnerValidate: RwSignal<bool>,
	toasterInnerValidate: ToasterContext,
	dialogManager: DialogManager,
) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let moduleContentInnerValidate = moduleContentInnerValidate.clone();
		let editModeInnerValidate = editModeInnerValidate.clone();
		let toasterInnerValidate = toasterInnerValidate.clone();

		let dialogContent = DialogData::new()
			.setTitle(AllFrontUIEnum::HOME_CHANGE_CANCEL)
			.setOnValidate(Callback::new(move |_| {
				let moduleContentInnerValidate = moduleContentInnerValidate.clone();
				let editModeInnerValidate = editModeInnerValidate.clone();
				let toasterInnerValidate = toasterInnerValidate.clone();
				spawn_local(async move {
					let Some(mut guard) = moduleContentInnerValidate.try_write()
					else
					{
						return;
					};
					let Some((login, lang)) = UserData::loginLang_get_from_cookie()
					else
					{
						return;
					};
					let modules: &mut ModuleHolder = guard.deref_mut();
					let error = (*modules).editMode_cancel(login, false).await;
					editModeInnerValidate.update(|content| {
						*content = false;
					});

					if let Some(err) = error
					{
						toastingErr(&toasterInnerValidate, err).await;
					}
					else
					{
						toastingSuccess(&toasterInnerValidate, AllFrontUIEnum::VALID).await;
					}
				});
				return true;
			}));

		dialogManager.open(dialogContent);
	};
}

fn editMode_validate(
	moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
	editModeInnerValidate: RwSignal<bool>,
	toasterInnerValidate: ToasterContext,
	dialogManager: DialogManager,
) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let moduleContentInnerValidate = moduleContentInnerValidate.clone();
		let editModeInnerValidate = editModeInnerValidate.clone();
		let toasterInnerValidate = toasterInnerValidate.clone();

		let dialogContent = DialogData::new()
			.setTitle(AllFrontUIEnum::HOME_CHANGE_OK)
			.setOnValidate(Callback::new(move |_| {
				let moduleContentInnerValidate = moduleContentInnerValidate.clone();
				let editModeInnerValidate = editModeInnerValidate.clone();
				let toasterInnerValidate = toasterInnerValidate.clone();
				spawn_local(async move {
					let Some((login, lang)) = UserData::loginLang_get_from_cookie()
					else
					{
						return;
					};
					let Some(mut guard) = moduleContentInnerValidate.try_write()
					else
					{
						return;
					};
					let modules: &mut ModuleHolder = guard.deref_mut();
					let error = (*modules).editMode_validate(login).await;
					editModeInnerValidate.update(|content| {
						*content = false;
					});

					if let Some(err) = error
					{
						toastingErr(&toasterInnerValidate, err.to_string()).await;
					}
					else
					{
						toastingSuccess(&toasterInnerValidate, AllFrontUIEnum::UPDATE).await;
					}
				});
				return true;
			}));

		dialogManager.open(dialogContent);
	};
}

fn editMode_AddBlock(moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
                     dialogManager: DialogManager) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let selectedType = ArcRwSignal::new("".to_string());

		let selectedTypeInnerView = selectedType.clone();
		let moduleContentInnerValidate = moduleContentInnerValidate.clone();
		let dialogContent = DialogData::new()
			.setTitle(AllFrontUIEnum::HOME_CHANGE_NEW)
			.setBody(move || {
				let innerSelectedType = RwSignal::new("".to_string());

				let selectedTypeEffect = selectedTypeInnerView.clone();
				Effect::new(move |_| {
					selectedTypeEffect.clone().update(|e| *e = innerSelectedType.get());
				});

				view!{
					<div>
						<label>
							<span>Type</span>
							<select on:change:target=move |ev| {
							      innerSelectedType.set(ev.target().value().parse().unwrap_or_default());
							    }
							    prop:value=move || innerSelectedType.get().to_string()>
								{move ||{
									ModuleTypeDiscriminants::iter().map(|x| {
										view!{<option value={x.to_string()}>{x.to_string()}</option>}.into_any()
									}).collect_view()
								}}
							</select>
						</label>
					</div>
				}.into_any()
			})
			.setOnValidate(Callback::new(move |_| {
				let moduleContentInnerValidate = moduleContentInnerValidate.clone();
				let selectedType = selectedType.clone().get();

				moduleContentInnerValidate.update(|modules| {

					let Some(moduleType) = StringToModuleType(selectedType) else {return;};
					modules.blocks_insert(ModulePositions::new(moduleType));
				});

				return true;
			}));

		dialogManager.open(dialogContent);
	}
}

fn user_disconnected(navigate: impl Fn(&str, NavigateOptions) + Clone + 'static, toaster: ToasterContext, userData: Signal<Option<UserData>>, setUserData: WriteSignal<Option<UserData>>, withToaster: bool) -> impl Fn(()) -> bool + Clone
{
	return move |_| {
		let navigate = navigate.clone();
		let toaster = toaster.clone();
		spawn_local(async move {
			if let Some(mut userData) = userData.get_untracked() {
				userData.login_disconnect().await;
				setUserData.set(None);
			}

			if(withToaster) {
				toastingSuccess(&toaster, AllFrontLoginEnum::LOGIN_USER_DISCONNECTED).await;
			}
			HWebTrace!("user disconnected");
			navigate("/", Default::default());
		});
		return true;
	};
}