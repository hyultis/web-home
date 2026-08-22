use std::sync::Arc;
use leptoaster::ToasterContext;
use crate::front::ai::AiAllowedOrigins;
use crate::front::modules::module_holder::{ModuleHolder, ModuleHolderEpoch};
use crate::api::modules::components::ModuleID;
use crate::front::utils::all_front_enum::AllFrontUIEnum;
use crate::front::ai::automation::AiAutomationEvent;

#[derive(Clone)]
pub struct ModuleActionFn
{
	_epoch: ModuleHolderEpoch,
	_toaster: ToasterContext,
	_aiAllowedOrigins: AiAllowedOrigins,
	/// (moduleName/key, login)
	pub updateFn: Arc<dyn Fn(ModuleID) + Send + Sync>,
	pub getFn: Arc<dyn Fn(ModuleID) + Send + Sync>,
	pub removeFn: Arc<dyn Fn(ModuleID) + Send + Sync>,
	pub refreshFn: Arc<dyn Fn(ModuleID) + Send + Sync>
}

impl ModuleActionFn
{
	pub(crate) fn new(
	           toasterInnerValidate: ToasterContext,
	           aiAllowedOrigins: AiAllowedOrigins,
	           epoch: ModuleHolderEpoch) -> Self
	{
		Self {
			_epoch: epoch,
			_toaster: toasterInnerValidate.clone(),
			_aiAllowedOrigins: aiAllowedOrigins,
			updateFn: Arc::new(Self::module_update(toasterInnerValidate.clone(), epoch)),
			getFn: Arc::new(Self::module_get( toasterInnerValidate.clone(), true, epoch)),
			removeFn: Arc::new(Self::module_remove(toasterInnerValidate.clone(), epoch)),
			refreshFn: Arc::new(Self::module_refresh(toasterInnerValidate.clone(), epoch)),
		}
	}

	pub(super) fn task_spawn(&self, task: impl Future<Output = ()> + 'static)
	{
		ModuleHolder::task_spawn(self._epoch, task);
	}

	pub(super) fn lifecycle_isActive(&self) -> bool
	{
		return ModuleHolder::network_isActive(self._epoch);
	}

	pub(super) fn aiAutomation_eventsPublish(&self,events: Vec<AiAutomationEvent>) -> Vec<AiAutomationEvent>
	{
		return ModuleHolder::aiAutomation_eventsPublish(self._epoch,events);
	}

	pub(super) fn aiAutomation_sourceBaselinePersist(&self,event: AiAutomationEvent)
	{
		ModuleHolder::aiAutomation_sourceBaselinePersist(self._epoch,event,self._toaster.clone());
	}

	pub(super) fn aiAutomationUi_get(&self) -> (ToasterContext,AiAllowedOrigins)
	{
		return (self._toaster.clone(),self._aiAllowedOrigins.clone());
	}

	#[cfg(test)]
	pub(super) fn test_get(epoch: ModuleHolderEpoch) -> Self
	{
		return Self {
			_epoch: epoch,
			_toaster: ToasterContext::default(),
			_aiAllowedOrigins: AiAllowedOrigins::default(),
			updateFn: Arc::new(|_| {}),
			getFn: Arc::new(|_| {}),
			removeFn: Arc::new(|_| {}),
			refreshFn: Arc::new(|_| {}),
		};
	}

	fn module_update(
		toasterInnerValidate: ToasterContext,
		epoch: ModuleHolderEpoch,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let toasterInnerValidate = toasterInnerValidate.clone();

			ModuleHolder::task_spawn(
				epoch,
				ModuleHolder::network_deferredCall(ModuleHolder::getSingleton().clone(), epoch, toasterInnerValidate.clone(), |holder|ModuleHolder::network_module_update_caller(holder,moduleId), Some(AllFrontUIEnum::UPDATE))
			);
		};
	}

	fn module_get(
		toasterInnerValidate: ToasterContext,
		force: bool,
		epoch: ModuleHolderEpoch,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let toasterInnerValidate = toasterInnerValidate.clone();

			ModuleHolder::task_spawn(
				epoch,
				ModuleHolder::network_deferredCall(ModuleHolder::getSingleton().clone(), epoch, toasterInnerValidate.clone(), move |holder|ModuleHolder::network_module_retrieve_caller(holder,moduleId,force), None)
			);
		};
	}

	fn module_remove(
		toasterInnerValidate: ToasterContext,
		epoch: ModuleHolderEpoch,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let toasterInnerValidate = toasterInnerValidate.clone();

			ModuleHolder::task_spawn(
				epoch,
				ModuleHolder::network_deferredCall(ModuleHolder::getSingleton(), epoch, toasterInnerValidate.clone(), |holder|ModuleHolder::network_module_remove_caller(holder,moduleId), Some(AllFrontUIEnum::REMOVED))
			);
		};
	}

	fn module_refresh(
		toaster: ToasterContext,
		epoch: ModuleHolderEpoch,
		//dialog: DialogManager
	) -> impl Fn((ModuleID)) -> ()
	{
		return move |(moduleId)| {
			let toaster = toaster.clone();
			ModuleHolder::module_refresh(epoch, vec![moduleId], toaster);
		};
	}
}
