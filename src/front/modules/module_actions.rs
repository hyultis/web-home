use leptoaster::ToasterContext;
use leptos::callback::Callback;
use leptos::prelude::{ArcRwSignal, With};
use leptos::reactive::spawn_local;
use crate::front::modules::module_holder::ModuleHolder;
use crate::api::modules::components::ModuleID;
use crate::front::utils::all_front_enum::AllFrontUIEnum;

#[derive(Clone)]
pub struct ModuleActionFn
{
	/// (moduleName/key, login)
	pub updateFn: Callback<(ModuleID),()>,
	pub getFn: Callback<(ModuleID),()>,
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
			getFn: Callback::new(Self::module_get(moduleContentInnerValidate.clone(), toasterInnerValidate.clone())),
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

			spawn_local(
				ModuleHolder::network_deferredCall(moduleContentInnerValidate.clone(), toasterInnerValidate.clone(), |holder|ModuleHolder::network_module_update_caller(holder,moduleId), Some(AllFrontUIEnum::UPDATE))
			);
		};
	}

	fn module_get(
		moduleContentInnerValidate: ArcRwSignal<ModuleHolder>,
		toasterInnerValidate: ToasterContext,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let moduleContentInnerValidate = moduleContentInnerValidate.clone();
			let toasterInnerValidate = toasterInnerValidate.clone();

			spawn_local(
				ModuleHolder::network_deferredCall(moduleContentInnerValidate.clone(), toasterInnerValidate.clone(), |holder|ModuleHolder::network_module_retrieve_caller(holder,moduleId,false), None)
			);
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

			spawn_local(
				ModuleHolder::network_deferredCall(moduleContentInnerValidate.clone(), toasterInnerValidate.clone(), |holder|ModuleHolder::network_module_remove_caller(holder,moduleId), Some(AllFrontUIEnum::REMOVED))
			);
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