use leptoaster::ToasterContext;
use leptos::callback::Callback;
use leptos::prelude::{ArcRwSignal, With, Write};
use leptos::reactive::spawn_local;
use crate::front::modules::ModuleHolder;
use crate::front::utils::toaster_helpers::{toastingErr, toastingSuccess};
use crate::front::utils::users_data::UserData;
use std::ops::DerefMut;
use crate::api::modules::components::ModuleID;
use crate::front::utils::all_front_enum::AllFrontUIEnum;

#[derive(Clone)]
pub struct ModuleActionFn
{
	/// (moduleName/key, login)
	pub updateFn: Callback<(ModuleID),()>,
	pub getOrUpdateFn: Callback<(ModuleID),()>,
	pub removeFn: Callback<(ModuleID),()>,
	pub refreshFn: Callback<(ModuleID),()>
}

impl ModuleActionFn
{
	pub fn new(moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
	           toasterInnerValidate: ToasterContext) -> Self
	{
		Self {
			updateFn: Callback::new(Self::module_update(moduleContentInnerValidate.clone(), toasterInnerValidate.clone())),
			getOrUpdateFn: Callback::new(Self::module_getOrUpdate(moduleContentInnerValidate.clone(), toasterInnerValidate.clone())),
			removeFn: Callback::new(Self::module_remove(moduleContentInnerValidate.clone(), toasterInnerValidate.clone())),
			refreshFn: Callback::new(Self::module_refresh(moduleContentInnerValidate.clone(), toasterInnerValidate.clone())),
		}
	}

	fn module_update(
		moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
		toasterInnerValidate: ToasterContext,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let moduleContentInnerValidate = moduleContentInnerValidate.clone();
			let toasterInnerValidate = toasterInnerValidate.clone();

			spawn_local(async move {
				let Some(mut guard) = moduleContentInnerValidate.try_write_untracked() else {return};
				let Some((login, _)) = UserData::loginLang_get_from_cookie()
				else
				{
					return;
				};
				let modules: &mut ModuleHolder = guard.deref_mut();
				let error = (*modules).module_update(login, moduleId).await;

				if let Some(err) = error
				{
					toastingErr(&toasterInnerValidate, err).await;
				}
				else
				{
					toastingSuccess(&toasterInnerValidate, AllFrontUIEnum::UPDATE).await;
				}
			});
		};
	}

	fn module_getOrUpdate(
		moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
		toasterInnerValidate: ToasterContext,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let moduleContentInnerValidate = moduleContentInnerValidate.clone();
			let toasterInnerValidate = toasterInnerValidate.clone();

			spawn_local(async move {
				let Some(mut guard) = moduleContentInnerValidate.try_write_untracked() else {return};
				let Some((login, _)) = UserData::loginLang_get_from_cookie()
				else
				{
					return;
				};
				let modules: &mut ModuleHolder = guard.deref_mut();
				let error = (*modules).module_getOrUpdate(login, moduleId).await;

				if let Some(err) = error
				{
					toastingErr(&toasterInnerValidate, err).await;
				}
			});
		};
	}

	fn module_remove(
		moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
		toasterInnerValidate: ToasterContext,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let moduleContentInnerValidate = moduleContentInnerValidate.clone();
			let toasterInnerValidate = toasterInnerValidate.clone();

			spawn_local(async move {
				let Some(mut guard) = moduleContentInnerValidate.try_write_untracked() else {return};
				let Some((login, _)) = UserData::loginLang_get_from_cookie()
				else
				{
					return;
				};
				let modules: &mut ModuleHolder = guard.deref_mut();
				let error = (*modules).module_remove(login, moduleId).await;

				if let Some(err) = error
				{
					toastingErr(&toasterInnerValidate, err).await;
				}
				else
				{
					toastingSuccess(&toasterInnerValidate, AllFrontUIEnum::REMOVED).await;
				}
			});
		};
	}

	fn module_refresh(
		moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
		toaster: ToasterContext,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let moduleContentInnerValidate = moduleContentInnerValidate.clone();
			let toaster = toaster.clone();
			moduleContentInnerValidate.with(|modules|{
				modules.module_refresh(vec![moduleId], toaster);
			});
		};
	}
}