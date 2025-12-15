use leptoaster::ToasterContext;
use leptos::callback::Callback;
use leptos::prelude::{ArcRwSignal, Write};
use leptos::reactive::spawn_local;
use crate::front::modules::ModuleHolder;
use crate::front::utils::toaster_helpers::{toastingErr, toastingSuccess};
use crate::front::utils::users_data::UserData;
use std::ops::DerefMut;
use crate::front::utils::all_front_enum::AllFrontUIEnum;

#[derive(Clone)]
pub struct ModuleActionFn
{
	/// (moduleName/key, login)
	pub updateFn: Callback<(String),()>,
	pub reloadFn: Callback<(String),()>,
	pub removeFn: Callback<(String),()>,
	pub refreshFn: Callback<(String),()>
}

impl ModuleActionFn
{
	pub fn new(moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
	           toasterInnerValidate: ToasterContext) -> Self
	{
		Self {
			updateFn: Callback::new(Self::module_update(moduleContentInnerValidate.clone(), toasterInnerValidate.clone())),
			reloadFn: Callback::new(Self::module_reload(moduleContentInnerValidate.clone(), toasterInnerValidate.clone())),
			removeFn: Callback::new(Self::module_remove(moduleContentInnerValidate.clone(), toasterInnerValidate.clone())),
			refreshFn: Callback::new(Self::module_refresh(moduleContentInnerValidate.clone(), toasterInnerValidate.clone())),
		}
	}

	fn module_update(
		moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
		toasterInnerValidate: ToasterContext,
		//dialog: DialogManager
	) -> impl Fn((String)) -> ()
	{
		return move |(moduleName)| {
			let moduleContentInnerValidate = moduleContentInnerValidate.clone();
			let toasterInnerValidate = toasterInnerValidate.clone();

			spawn_local(async move {
				let Some(mut guard) = moduleContentInnerValidate.try_write()
				else
				{
					return;
				};
				let Some((login, _)) = UserData::loginLang_get_from_cookie()
				else
				{
					return;
				};
				let modules: &mut ModuleHolder = guard.deref_mut();
				let error = (*modules).module_update(login, moduleName).await;

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

	fn module_reload(
		moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
		toasterInnerValidate: ToasterContext,
		//dialog: DialogManager
	) -> impl Fn((String)) -> ()
	{
		return move |(moduleName)| {
			let moduleContentInnerValidate = moduleContentInnerValidate.clone();
			let toasterInnerValidate = toasterInnerValidate.clone();

			spawn_local(async move {
				let Some(mut guard) = moduleContentInnerValidate.try_write()
				else
				{
					return;
				};
				let Some((login, _)) = UserData::loginLang_get_from_cookie()
				else
				{
					return;
				};
				let modules: &mut ModuleHolder = guard.deref_mut();
				let error = (*modules).module_update(login, moduleName).await;

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

	fn module_remove(
		moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
		toasterInnerValidate: ToasterContext,
		//dialog: DialogManager
	) -> impl Fn((String)) -> ()
	{
		return move |(moduleName)| {
			let moduleContentInnerValidate = moduleContentInnerValidate.clone();
			let toasterInnerValidate = toasterInnerValidate.clone();

			spawn_local(async move {
				let Some(mut guard) = moduleContentInnerValidate.try_write()
				else
				{
					return;
				};
				let Some((login, _)) = UserData::loginLang_get_from_cookie()
				else
				{
					return;
				};
				let modules: &mut ModuleHolder = guard.deref_mut();
				let error = (*modules).module_remove(login, moduleName).await;

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
	) -> impl Fn((String)) -> ()
	{
		return move |(moduleName)| {
			let moduleContentInnerValidate = moduleContentInnerValidate.clone();
			let toaster = toaster.clone();
			spawn_local(async move {
				let mut guard = moduleContentInnerValidate.write();
				let modules: &mut ModuleHolder = guard.deref_mut();
				(*modules).module_refresh(moduleName, toaster).await;
			});
		};
	}
}