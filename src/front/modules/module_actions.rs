use std::sync::Arc;
use leptoaster::ToasterContext;
use leptos::prelude::{WithUntracked};
use leptos::reactive::{spawn_local_scoped};
use crate::front::modules::module_holder::ModuleHolder;
use crate::api::modules::components::ModuleID;
use crate::front::utils::all_front_enum::AllFrontUIEnum;

#[derive(Clone)]
pub struct ModuleActionFn
{
	/// (moduleName/key, login)
	pub updateFn: Arc<dyn Fn(ModuleID) + Send + Sync>,
	pub getFn: Arc<dyn Fn(ModuleID) + Send + Sync>,
	pub removeFn: Arc<dyn Fn(ModuleID) + Send + Sync>,
	pub refreshFn: Arc<dyn Fn(ModuleID) + Send + Sync>
}

impl ModuleActionFn
{
	pub fn new(
	           toasterInnerValidate: ToasterContext) -> Self
	{
		Self {
			updateFn: Arc::new(Self::module_update(toasterInnerValidate.clone())),
			getFn: Arc::new(Self::module_get( toasterInnerValidate.clone(), true)),
			removeFn: Arc::new(Self::module_remove(toasterInnerValidate.clone())),
			refreshFn: Arc::new(Self::module_refresh(toasterInnerValidate.clone())),
		}
	}

	fn module_update(
		toasterInnerValidate: ToasterContext,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let toasterInnerValidate = toasterInnerValidate.clone();

			spawn_local_scoped(
				ModuleHolder::network_deferredCall(ModuleHolder::getSingleton().clone(), toasterInnerValidate.clone(), |holder|ModuleHolder::network_module_update_caller(holder,moduleId), Some(AllFrontUIEnum::UPDATE))
			);
		};
	}

	fn module_get(
		toasterInnerValidate: ToasterContext,
		force: bool
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let toasterInnerValidate = toasterInnerValidate.clone();

			spawn_local_scoped(
				ModuleHolder::network_deferredCall(ModuleHolder::getSingleton().clone(), toasterInnerValidate.clone(), move |holder|ModuleHolder::network_module_retrieve_caller(holder,moduleId,force), None)
			);
		};
	}

	fn module_remove(
		toasterInnerValidate: ToasterContext,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let toasterInnerValidate = toasterInnerValidate.clone();

			spawn_local_scoped(
				ModuleHolder::network_deferredCall(ModuleHolder::getSingleton(), toasterInnerValidate.clone(), |holder|ModuleHolder::network_module_remove_caller(holder,moduleId), Some(AllFrontUIEnum::REMOVED))
			);
		};
	}

	fn module_refresh(
		toaster: ToasterContext,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let toaster = toaster.clone();
			ModuleHolder::getSingleton().with_untracked(|modules|{
				modules.module_refresh(vec![moduleId], toaster);
			});
		};
	}
}