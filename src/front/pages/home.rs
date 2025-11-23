use leptos::prelude::OnTargetAttribute;
use leptos::prelude::{CollectView, Get, PropAttribute};
use crate::front::modules::components::Backable;
use crate::front::modules::ModuleHolder;
use crate::front::utils::all_front_enum::{AllFrontLoginEnum, AllFrontUIEnum};
use crate::front::utils::dialog::DialogManager;
use crate::front::utils::toaster_helpers::{toastingErr, toastingSuccess};
use crate::front::utils::users_data::UserData;
use crate::{HWebTrace};
use leptoaster::{expect_toaster, ToasterContext};
use leptos::__reexports::wasm_bindgen_futures::spawn_local;
use leptos::ev::MouseEvent;
use leptos::prelude::ElementChild;
use leptos::prelude::{
	use_context, ArcRwSignal, Callback, ClassAttribute, Effect, IntoAny, OnAttribute, Read,
	RenderHtml, RwSignal, Set, Update, Write,
};
use leptos::{island, view, IntoView};
use leptos_router::hooks;
use std::ops::DerefMut;
use strum::IntoEnumIterator;
use crate::front::modules::module_positions::ModulePositions;
use crate::front::modules::module_type::{ModuleType, ModuleTypeDiscriminants};
use crate::front::modules::todo::Todo;
// https://iconoir.com/
// plus

#[island]
pub fn Home() -> impl IntoView
{
	let editMode = RwSignal::new(false);
	let moduleContent = ArcRwSignal::new(ModuleHolder::new());
	let dialog = use_context::<DialogManager>().expect("DialogManager missing");
	let toaster = expect_toaster();

	let editModeValidateFn = editMode_validate(
		moduleContent.clone(),
		editMode.clone(),
		toaster.clone(),
		dialog.clone(),
	);

	let editModeCancelFn = editMode_cancel(
		moduleContent.clone(),
		editMode.clone(),
		toaster.clone(),
		dialog.clone(),
	);

	let editModeActivateFn = move |_| {
		editMode.update(|content| {
			*content = true;
		});
	};

	let forceResyncFn = move |_| {
	};

	let editModeAddModuleFn = editMode_AddBlock(moduleContent.clone(), dialog.clone());

	let toasterInnerDisconnect = toaster.clone();
	let disconnectFn = move |_| {
		let navigate = hooks::use_navigate();
		let toaster = toasterInnerDisconnect.clone();

		dialog.openSimple(
			AllFrontLoginEnum::LOGIN_USER_WANT_DISCONNECTED,
			Some(Callback::new(move |_| {
				let navigate = navigate.clone();
				let toaster = toaster.clone();
				spawn_local(async move {
					let (userData, setUserData) = UserData::cookie_signalGet();
					let mut userData = userData
						.read()
						.clone()
						.unwrap_or(UserData::new(&"EN".to_string()));
					userData.login_disconnect().await;
					toastingSuccess(toaster, AllFrontLoginEnum::LOGIN_USER_DISCONNECTED).await;
					HWebTrace!("user disconnected");
					setUserData.set(Some(userData));
					navigate("/", Default::default());
				});
				return true;
			})),
			None,
		);
	};

	let moduleContentInnerInitialLoad = moduleContent.clone();
	let toasterInnerInitialLoad = toaster.clone();
	Effect::new(move || {
		let moduleContentInnerInitialLoad = moduleContentInnerInitialLoad.clone();
		let toasterInnerInitialLoad = toasterInnerInitialLoad.clone();

		spawn_local(async move {
			HWebTrace!("home spawn_local IN");
			let Some(mut guard) = moduleContentInnerInitialLoad.try_write()
			else
			{
				HWebTrace!("home spawn_local KO");
				return;
			};
			let holder: &mut ModuleHolder = guard.deref_mut();

			let Some((login, lang)) = UserData::loginLang_get_from_cookie()
			else
			{
				return;
			};
			let error = (*holder).editMode_cancel(login, true).await;
			if let Some(err) = error
			{
				toastingErr(toasterInnerInitialLoad, err.to_string()).await;
			}
		});
	});

	let moduleContentInnerView = moduleContent.clone();
	let moduleContentInnerModuleView = moduleContent.clone();
	view! {
		<div class="home_body">
			<div class="header">
				<div class="left">
					{move || {
						let Some(binding) = moduleContentInnerView.clone().try_read() else {return view!{<span/>}.into_any()};
						let tmp = binding.links_get();
						tmp.draw(editMode)
					}}
				</div>
				<div class="right">
					<i class="iconoir-key" on:click=disconnectFn></i>
					<i class="iconoir-reload-window" on:click=forceResyncFn></i>
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
					let Some(binding) = moduleContentInnerModuleView.clone().try_read() else {return view!{<span/>}.into_any()};
					binding.blocks_get().iter().map( |(_,d)|d.draw(editMode)).collect_view().into_any()
				}}
			</div>
		</div>
	}
}

fn editMode_cancel(
	moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
	editModeInnerValidate: RwSignal<bool>,
	toasterInnerValidate: ToasterContext,
	dialog: DialogManager,
) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let moduleContentInnerValidate = moduleContentInnerValidate.clone();
		let editModeInnerValidate = editModeInnerValidate.clone();
		let toasterInnerValidate = toasterInnerValidate.clone();

		dialog.open(
			"Annuler les changements ?",
			move || {
				view! {
					<span/>
				}
				.into_any()
			},
			Some(Callback::new(move |_| {
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
						toastingErr(toasterInnerValidate, err).await;
					}
					else
					{
						toastingSuccess(toasterInnerValidate, AllFrontUIEnum::VALID).await;
					}
				});
				return true;
			})),
			None,
		);
	};
}

fn editMode_validate(
	moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
	editModeInnerValidate: RwSignal<bool>,
	toasterInnerValidate: ToasterContext,
	dialog: DialogManager,
) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let moduleContentInnerValidate = moduleContentInnerValidate.clone();
		let editModeInnerValidate = editModeInnerValidate.clone();
		let toasterInnerValidate = toasterInnerValidate.clone();

		dialog.open(
			"Enregistrer les changements ?",
			move || {
				view! {
					<span/>
				}
				.into_any()
			},
			Some(Callback::new(move |_| {
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
						toastingErr(toasterInnerValidate, err.to_string()).await;
					}
					else
					{
						toastingSuccess(toasterInnerValidate, AllFrontUIEnum::UPDATE).await;
					}
				});
				return true;
			})),
			None,
		);
	};
}

fn editMode_AddBlock(moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
                     dialog: DialogManager) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let selectedType = ArcRwSignal::new("".to_string());

		let selectedTypeInnerView = selectedType.clone();
		let moduleContentInnerValidate = moduleContentInnerValidate.clone();
		dialog.open(
			"Type du nouveau bloc ?",
			move || {
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
							      innerSelectedType.set(ev.target().value().parse().unwrap());
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
			},
			Some(Callback::new(move |_| {
				let moduleContentInnerValidate = moduleContentInnerValidate.clone();
				let selectedType = selectedType.clone().get();

				moduleContentInnerValidate.update(|modules| {

					let moduleType = match selectedType.as_str() {
						"TODO" => ModuleType::TODO(Todo::default()),
						_ => return
					};

					modules.blocks_insert(ModulePositions::new(moduleType));
				});

				return true;
			})),
			None,
		);
	}
}
