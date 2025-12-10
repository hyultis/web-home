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
	pub reloadFn: Callback<(String),()>
}

impl ModuleActionFn
{
	pub fn new(moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
	           toasterInnerValidate: ToasterContext) -> Self
	{
		Self {
			updateFn: Callback::new(Self::module_update(moduleContentInnerValidate.clone(), toasterInnerValidate.clone())),
			reloadFn: Callback::new(Self::module_reload(moduleContentInnerValidate.clone(), toasterInnerValidate.clone())),
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
}