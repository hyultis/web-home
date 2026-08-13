use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use leptoaster::ToasterContext;
use leptos::prelude::{ArcRwSignal, Owner, Set, Update, With, WithUntracked};
use leptos::reactive::spawn_local_scoped_with_cancellation;
use crate::api::modules::{API_module_remove, API_module_retrieve, API_modules_retrieve, API_modules_update, ModuleApiError, ModuleReturnRetrieve};
use crate::api::modules::components::{ApiModulesID, ModuleContent, ModuleID};
use crate::front::modules::components::{API_return_apply, ApiCall, Backable, BoxFuture, Cacheable, ModuleName, PausableStocker, RefreshTime};
use crate::front::modules::link::LinksHolder;
use crate::front::modules::module_actions;
use crate::front::modules::module_positions::ModulePositions;
use crate::front::modules::module_type::ModuleType;
use crate::front::utils::all_front_enum::{AllFrontErrorEnum, AllFrontUIEnum};
use crate::front::utils::toaster_helpers;
use crate::front::utils::toaster_helpers::toastingErr;
use crate::front::utils::users_data::{ClientCryptoContext, ClientState};

thread_local! {
    static MODULE_HOLDER_SINGLETON: RefCell<Option<ArcRwSignal<ModuleHolder>>> =
        const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ModuleHolderEpoch(u64);

struct ModuleRefreshTask
{
	owner: Owner,
	futures: Vec<BoxFuture>,
}

impl ModuleRefreshTask
{
	fn spawn(self)
	{
		self.owner.with(|| {
			spawn_local_scoped_with_cancellation(async move {
				for oneFuture in self.futures
				{
					oneFuture.await;
				}
			});
		});
	}
}

#[cfg(test)]
mod tests
{
	use std::sync::Arc;
	use std::sync::atomic::{AtomicBool, Ordering};

	use crate::api::modules::ModuleApiError;
	use crate::api::modules::components::{ModuleContent, ModuleID};
	use crate::front::modules::components::{API_return_apply, PausableStocker};
	use crate::front::modules::module_actions::ModuleActionFn;
	use crate::front::modules::module_positions::ModulePositions;
	use crate::front::modules::module_type::ModuleType;
	use crate::front::modules::todo::Todo;
	use crate::front::utils::all_front_enum::AllFrontErrorEnum;
	use crate::front::utils::users_data::ClientCryptoContext;
	use leptoaster::ToasterContext;
	use leptos::prelude::{ArcRwSignal, Owner};

	use super::ModuleHolder;

	#[test]
	fn networkError_authRequiredMarksLocalSessionInvalid()
	{
		let mut result = API_return_apply::default();
		ModuleHolder::network_error_apply(&mut result, ModuleApiError::AUTH_REQUIRED);

		assert!(result.authenticationRequired);
		assert!(matches!(result.error.as_slice(), [AllFrontErrorEnum::SESSION_EXPIRED]));
	}

	#[test]
	fn moduleUploadPreparation_containsOnlyEncryptedContent()
	{
		let holder = ModuleHolder::new();
		let crypto = ClientCryptoContext::test_get();
		let prepared = holder.network_modules_update_prepare(&crypto).unwrap();

		assert!(!prepared.is_empty());
		assert!(prepared.iter().all(|content| crypto.decrypt(&content.content).is_ok()));
	}

	#[test]
	fn localCryptoError_doesNotScheduleModuleMutation()
	{
		let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
		let result = runtime.block_on(ModuleHolder::network_localError_get(AllFrontErrorEnum::CRYPTO_CONTEXT_MISSING));

		assert!(matches!(result.error.as_slice(), [AllFrontErrorEnum::CRYPTO_CONTEXT_MISSING]));
		assert!(result.retrieve.is_empty());
		assert!(result.update.is_empty());
		assert!(result.moduleIdToRefresh.is_empty());
	}

	#[test]
	fn lifecycleClose_resetsDecryptedStateAndOwnedResources()
	{
		let parentOwner = Owner::new();
		parentOwner.with(|| {
			let mut holder = ModuleHolder::new();
			let (epoch, previousOwner) = holder.lifecycle_open_inner();
			assert!(previousOwner.is_none());

			let moduleId = ModuleID {id: "account-a-module".to_string()};
			holder._links.id_set(ModuleID {id: "account-a-links".to_string()});
			holder._blocks.insert(
				moduleId.clone(),
				ArcRwSignal::new(ModulePositions::new(ModuleType::TODO(Todo::new()))),
			);
			holder._crons.insert(moduleId, PausableStocker::test_paused());
			holder._moduleActions = Some(ModuleActionFn::test_get(epoch));
			holder._blockNb = 7;

			let ownerWasCleaned = Arc::new(AtomicBool::new(false));
			let ownerWasCleanedInner = ownerWasCleaned.clone();
			holder._taskOwner.as_ref().unwrap().with(|| {
				Owner::on_cleanup(move || ownerWasCleanedInner.store(true, Ordering::Relaxed));
			});

			let taskOwner = holder.lifecycle_close_inner().unwrap();
			assert!(!holder.lifecycle_epoch_isActive(epoch));
			assert!(holder._blocks.is_empty());
			assert!(holder._crons.is_empty());
			assert!(holder._moduleActions.is_none());
			assert_eq!(holder._blockNb, 0);
			assert_ne!(holder._links.id_get().id, "account-a-links");

			taskOwner.cleanup();
			assert!(ownerWasCleaned.load(Ordering::Relaxed));
		});
		parentOwner.cleanup();
	}

	#[test]
	fn accountTransition_rejectsOldCloseAndLateNetworkResult()
	{
		let parentOwner = Owner::new();
		parentOwner.with(|| {
			let mut holder = ModuleHolder::new();
			let (accountAEpoch, _) = holder.lifecycle_open_inner();
			holder._blockNb = 8;

			let (accountBEpoch, accountAOwner) = holder.lifecycle_open_inner();
			accountAOwner.unwrap().cleanup();
			assert!(!holder.lifecycle_epoch_isActive(accountAEpoch));
			assert!(holder.lifecycle_epoch_isActive(accountBEpoch));
			assert_eq!(holder._blockNb, 0);

			let (oldCloseApplied, oldOwner) = holder.lifecycle_closeIf_inner(accountAEpoch);
			assert!(!oldCloseApplied);
			assert!(oldOwner.is_none());
			assert!(holder.lifecycle_epoch_isActive(accountBEpoch));

			let mut staleResult = API_return_apply::default();
			staleResult.retrieve.push(Box::new(|holder| holder._blockNb = 13));
			holder.network_apply(accountAEpoch, staleResult, ToasterContext::default());
			assert_eq!(holder._blockNb, 0);

			let mut currentResult = API_return_apply::default();
			currentResult.retrieve.push(Box::new(|holder| holder._blockNb = 21));
			holder.network_apply(accountBEpoch, currentResult, ToasterContext::default());
			assert_eq!(holder._blockNb, 21);

			holder.lifecycle_close_inner().unwrap().cleanup();
		});
		parentOwner.cleanup();
	}

	#[test]
	fn networkSuspension_invalidatesInflightGeneration()
	{
		let parentOwner = Owner::new();
		parentOwner.with(|| {
			let mut holder = ModuleHolder::new();
			let (epoch, _) = holder.lifecycle_open_inner();
			let generation = holder.network_generation_get(epoch).unwrap();

			holder.network_suspend_inner();
			assert!(holder.network_generation_get(epoch).is_none());
			assert!(!holder.network_generation_isActive(epoch,generation));

			holder.network_resume_inner();
			let resumedGeneration = holder.network_generation_get(epoch).unwrap();
			assert_ne!(resumedGeneration,generation);
			assert!(holder.network_generation_isActive(epoch,resumedGeneration));
		});
		parentOwner.cleanup();
	}

	#[test]
	fn blocksView_ordersModulesFromTopToBottomThenLeftToRight()
	{
		let parentOwner = Owner::new();
		parentOwner.with(|| {
			let mut holder = ModuleHolder::new();
			for (id,position,depth) in [
				("lower",[0,100],0),
				("upper-right",[200,0],0),
				("upper-left-b",[0,0],2),
				("upper-left-a",[0,0],1),
			]
			{
				let moduleId = ModuleID {id: id.to_string()};
				let moduleContent = ModuleContent {
					id: moduleId.clone(),
					pos: position,
					depth,
					..Default::default()
				};
				holder._blocks.insert(
					moduleId,
					ArcRwSignal::new(ModulePositions::newFromModuleContent(moduleContent,ModuleType::TODO(Todo::new()))),
				);
			}

			let orderedIds = holder.blocks_view().into_iter()
				.map(|(id,_)| id.id)
				.collect::<Vec<_>>();

			assert_eq!(orderedIds,["upper-left-a","upper-left-b","upper-right","lower"]);
		});
		parentOwner.cleanup();
	}
}

pub struct ModuleHolder
{
	_links: LinksHolder,
	_blocks: HashMap<ModuleID, ArcRwSignal<ModulePositions<ModuleType>>>,
	_crons: HashMap<ModuleID, PausableStocker>,
	_moduleActions: Option<module_actions::ModuleActionFn>,
	_blockNb: usize,
	_epochCounter: u64,
	_activeEpoch: Option<ModuleHolderEpoch>,
	_taskOwner: Option<Owner>,
	_networkGeneration: u64,
	_networkSuspended: bool,
}

impl ModuleHolder
{
	pub fn getSingleton() -> ArcRwSignal<ModuleHolder> {
		MODULE_HOLDER_SINGLETON.with(|slot| {
			let mut slot = slot.borrow_mut();
			slot.get_or_insert_with(|| ArcRwSignal::new(ModuleHolder::new())).clone()
		})
	}

	fn new() -> Self
	{
		Self {
			_links: LinksHolder::new(),
			_blocks: HashMap::new(),
			_crons: Default::default(),
			_moduleActions: None,
			_blockNb: 0,
			_epochCounter: 0,
			_activeEpoch: None,
			_taskOwner: None,
			_networkGeneration: 0,
			_networkSuspended: false,
		}
	}

	pub(crate) fn lifecycle_open() -> ModuleHolderEpoch
	{
		let (epoch, previousOwner) = Self::getSingleton()
			.try_update(|holder| holder.lifecycle_open_inner())
			.expect("the permanent ModuleHolder singleton must remain writable while Home opens");
		if let Some(previousOwner) = previousOwner
		{
			previousOwner.cleanup();
		}
		return epoch;
	}

	pub(crate) fn lifecycle_close()
	{
		let owner = Self::getSingleton()
			.try_update(|holder| holder.lifecycle_close_inner())
			.flatten();
		if let Some(owner) = owner
		{
			owner.cleanup();
		}
	}

	pub(crate) fn lifecycle_closeIf(epoch: ModuleHolderEpoch) -> bool
	{
		let Some((wasClosed, owner)) = Self::getSingleton()
			.try_update(|holder| holder.lifecycle_closeIf_inner(epoch))
		else
		{
			return false;
		};
		if let Some(owner) = owner
		{
			owner.cleanup();
		}
		return wasClosed;
	}

	pub(crate) fn task_spawn(epoch: ModuleHolderEpoch, task: impl Future<Output = ()> + 'static)
	{
		let owner = Self::getSingleton().with_untracked(|holder| holder.lifecycle_owner_get(epoch));
		if let Some(owner) = owner
		{
			owner.with(|| spawn_local_scoped_with_cancellation(task));
		}
	}

	pub(crate) fn lifecycle_isActive(epoch: ModuleHolderEpoch) -> bool
	{
		return Self::getSingleton().with_untracked(|holder| holder.lifecycle_epoch_isActive(epoch));
	}

	pub(crate) fn network_isActive(epoch: ModuleHolderEpoch) -> bool
	{
		return Self::getSingleton().with_untracked(|holder| holder.network_generation_get(epoch).is_some());
	}

	pub(crate) fn lifecycle_isOpen() -> bool
	{
		return Self::getSingleton().with_untracked(|holder| holder._activeEpoch.is_some());
	}

	pub(crate) fn network_suspend()
	{
		let _ = Self::getSingleton().try_update(|holder| holder.network_suspend_inner());
	}

	pub(crate) fn network_resume()
	{
		let _ = Self::getSingleton().try_update(|holder| holder.network_resume_inner());
	}

	fn network_suspend_inner(&mut self)
	{
		if (self._networkSuspended)
		{
			return;
		}
		self._networkGeneration = self._networkGeneration.checked_add(1)
			.expect("ModuleHolder network generation exhausted");
		self._networkSuspended = true;
		for cron in self._crons.values_mut()
		{
			cron.pause();
		}
	}

	fn network_resume_inner(&mut self)
	{
		if (!self._networkSuspended)
		{
			return;
		}
		self._networkSuspended = false;
		for cron in self._crons.values_mut()
		{
			cron.resume();
		}
	}

	fn lifecycle_open_inner(&mut self) -> (ModuleHolderEpoch, Option<Owner>)
	{
		let previousOwner = self.lifecycle_close_inner();
		self._epochCounter = self._epochCounter.checked_add(1)
			.expect("ModuleHolder lifecycle epoch exhausted");
		let epoch = ModuleHolderEpoch(self._epochCounter);
		self._activeEpoch = Some(epoch);
		self._taskOwner = Some(Owner::new());
		return (epoch, previousOwner);
	}

	fn lifecycle_close_inner(&mut self) -> Option<Owner>
	{
		self._activeEpoch = None;
		self._networkGeneration = self._networkGeneration.checked_add(1)
			.expect("ModuleHolder network generation exhausted");
		self._networkSuspended = false;
		let owner = self._taskOwner.take();
		self._crons.clear();
		self._moduleActions = None;
		self._blocks.clear();
		self._links = LinksHolder::new();
		self._blockNb = 0;
		return owner;
	}

	fn lifecycle_closeIf_inner(&mut self, epoch: ModuleHolderEpoch) -> (bool, Option<Owner>)
	{
		if (!self.lifecycle_epoch_isActive(epoch))
		{
			return (false, None);
		}
		return (true, self.lifecycle_close_inner());
	}

	fn lifecycle_epoch_isActive(&self, epoch: ModuleHolderEpoch) -> bool
	{
		return self._activeEpoch == Some(epoch);
	}

	fn network_generation_get(&self, epoch: ModuleHolderEpoch) -> Option<u64>
	{
		if (!self.lifecycle_epoch_isActive(epoch) || self._networkSuspended)
		{
			return None;
		}
		return Some(self._networkGeneration);
	}

	fn network_generation_isActive(&self, epoch: ModuleHolderEpoch, generation: u64) -> bool
	{
		return self.lifecycle_epoch_isActive(epoch)
			&& !self._networkSuspended
			&& self._networkGeneration == generation;
	}

	fn lifecycle_owner_get(&self, epoch: ModuleHolderEpoch) -> Option<Owner>
	{
		if (!self.lifecycle_epoch_isActive(epoch))
		{
			return None;
		}
		return self._taskOwner.clone();
	}

	pub(crate) fn moduleActions_set(&mut self, epoch: ModuleHolderEpoch, ma: module_actions::ModuleActionFn)
	{
		if (self.lifecycle_epoch_isActive(epoch))
		{
			self._moduleActions = Some(ma);
		}
	}

	fn network_apply(&mut self, epoch: ModuleHolderEpoch, mut toApply: API_return_apply,toaster: ToasterContext) -> Option<ModuleRefreshTask>
	{
		if (!self.lifecycle_epoch_isActive(epoch))
		{
			return None;
		}
		toApply.retrieve.into_iter().for_each(|f| f(self));
		toApply.update.into_iter().for_each(|f| f(self));

		return self.module_refresh_prepare(epoch, toApply.moduleIdToRefresh.drain(..).collect(), toaster);
	}

	pub(crate) fn network_deferredCall(moduleHolder: ArcRwSignal<ModuleHolder>, epoch: ModuleHolderEpoch, toaster: ToasterContext, apiCall: impl FnOnce(ArcRwSignal<ModuleHolder>) -> Option<ApiCall>, toastingSuccess: Option<AllFrontUIEnum>) -> impl Future<Output = ()>
	{
		async move {
			let Some(networkGeneration) = moduleHolder.with_untracked(|holder| holder.network_generation_get(epoch)) else {return};
			let Some(apiCall) = apiCall(moduleHolder.clone()) else {return;};
			let mut apiResult = apiCall.await;
			if (!moduleHolder.with_untracked(|holder| holder.network_generation_isActive(epoch,networkGeneration)))
			{
				return;
			}
			let hasErrors = !apiResult.error.is_empty();
			let authenticationRequired = apiResult.authenticationRequired;
			let authenticationWasLocal = if (authenticationRequired)
			{
				ClientState::expect().login_isConnected_untracked()
			}
			else
			{
				false
			};

			// if they are some error
			for err in apiResult.error.drain(..) {
				if (!authenticationRequired || authenticationWasLocal)
				{
					toastingErr(&toaster, err).await;
					if (!moduleHolder.with_untracked(|holder| holder.network_generation_isActive(epoch,networkGeneration)))
					{
						return;
					}
				}
			};

			if (authenticationRequired)
			{
				if (ClientState::expect().local_clear().is_err())
				{
					toastingErr(&toaster, AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
				}
				Self::lifecycle_closeIf(epoch);
				return;
			}

			if (!hasErrors)
			{
				if let Some(toastingSuccess) = toastingSuccess
				{
					toaster_helpers::toastingSuccess(&toaster, toastingSuccess).await;
					if (!moduleHolder.with_untracked(|holder| holder.network_generation_isActive(epoch,networkGeneration)))
					{
						return;
					}
				}
			}

			let refreshTask = moduleHolder
				.try_update(|holder| holder.network_apply(epoch, apiResult,toaster))
				.flatten();
			if let Some(refreshTask) = refreshTask
			{
				refreshTask.spawn();
			}
		}
	}

	fn network_error_apply(apiReturn: &mut API_return_apply, error: ModuleApiError)
	{
		if (error == ModuleApiError::AUTH_REQUIRED)
		{
			apiReturn.authenticationRequired = true;
		}
		apiReturn.error.push(error.into());
	}

	fn network_deferredCall_inner<AsyncCaller, AsyncReturn, DataType, DataPrepare>(moduleHolder: ArcRwSignal<ModuleHolder>, prepare: DataPrepare, async_caller: AsyncCaller) -> Option<ApiCall>
	where
		DataPrepare: Fn(&ModuleHolder, &ClientCryptoContext) -> Result<DataType, AllFrontErrorEnum> + 'static,
		AsyncCaller: Fn(DataType) -> AsyncReturn + 'static,
		AsyncReturn: Future<Output = API_return_apply> + 'static,
		DataType: 'static,
	{
		let clientState = ClientState::expect();
		if (!clientState.login_isConnected_untracked())
		{
			return None;
		}
		let Some(crypto) = clientState.crypto_get()
		else
		{
			return Some(Self::network_localError_get(AllFrontErrorEnum::CRYPTO_CONTEXT_MISSING));
		};

		return moduleHolder.with_untracked(|holder| {
			if (holder._networkSuspended)
			{
				return None;
			}
			let preparedVar = match prepare(holder, &crypto)
			{
				Ok(preparedVar) => preparedVar,
				Err(error) => return Some(Self::network_localError_get(error)),
			};
			return Some(
				Box::pin(async move {
					return async_caller(preparedVar).await;
				}) as ApiCall
			);
		});
	}

	fn network_localError_get(error: AllFrontErrorEnum) -> ApiCall
	{
		return Box::pin(async move {
			let mut result = API_return_apply::default();
			result.error.push(error);
			return result;
		});
	}

	////////////////////////////////////////
	// START MODULES UPDATE ZONE ---
	////////////////////////////////////////

	pub fn network_modules_update_caller(moduleHolder: ArcRwSignal<ModuleHolder>) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(
			moduleHolder,
			|holder, crypto| holder.network_modules_update_prepare(crypto).map(|modules| (modules, true)),
			Self::network_modules_update_async,
		);
	}

	fn network_modules_update_prepare(
		&self,
		crypto: &ClientCryptoContext,
	) -> Result<Vec<ModuleContent>, AllFrontErrorEnum>
	{
		let mut moduleToUpdateData = vec![];

		let mut thisModuleContent = self._links.export();
		Self::export_crypt_content(&mut thisModuleContent, crypto)?;
		thisModuleContent.id = self._links.id_get();
		moduleToUpdateData.push(thisModuleContent);

		for (key, oneModule) in self._blocks.iter()
		{
			let mut thisModuleContent =oneModule.with_untracked(|module| module.export());
			Self::export_crypt_content(&mut thisModuleContent, crypto)?;
			thisModuleContent.id = key.clone();
			moduleToUpdateData.push(thisModuleContent);
		}

		return Ok(moduleToUpdateData);
	}

	async fn network_modules_update_async((moduleToUpdate, overwrite): (Vec<ModuleContent>, bool)) -> API_return_apply
	{
		if(moduleToUpdate.len()==0) {return API_return_apply::default();}

		let mut apiReturn = API_return_apply::default();

		match API_modules_update(moduleToUpdate, overwrite).await
		{
			Ok(_) => {},
			Err(err) => {
				Self::network_error_apply(&mut apiReturn, err);
				return apiReturn;
			}
		};

		return apiReturn;
	}

	////////////////////////////////////////
	// START MODULES UPDATE ZONE ---
	////////////////////////////////////////

	////////////////////////////////////////
	// START MONO MODULE UPDATE ZONE ---
	////////////////////////////////////////

	pub fn network_module_update_caller(moduleHolder: ArcRwSignal<ModuleHolder>, module: ModuleID) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(
			moduleHolder,
			move |holder, crypto| holder.network_module_update_prepare(module.clone(), crypto).map(|modules| (modules, false)),
			Self::network_modules_update_async,
		);
	}

	fn network_module_update_prepare(&self, moduleId: ModuleID, crypto: &ClientCryptoContext) -> Result<Vec<ModuleContent>, AllFrontErrorEnum>
	{
		let mut moduleToRetrieveData = vec![];

		for (key, oneModule) in self._blocks.iter()
			.filter(|(moduleIdSearch, _)| *moduleIdSearch == &moduleId)
		{
			let mut thisModuleContent =oneModule.with_untracked(|module| module.export());
			Self::export_crypt_content(&mut thisModuleContent, crypto)?;
			thisModuleContent.id = key.clone();
			moduleToRetrieveData.push(thisModuleContent);
		}

		return Ok(moduleToRetrieveData);
	}

	////////////////////////////////////////
	// END MONO MODULE UPDATE ZONE ---
	////////////////////////////////////////

	////////////////////////////////////////
	// START MODULES RETRIEVE ZONE ---
	////////////////////////////////////////

	pub fn network_modules_retrieve_caller(moduleHolder: ArcRwSignal<ModuleHolder>, forceUpdate: bool) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(moduleHolder, move |holder, crypto| Ok((holder.network_modules_retrieve_prepare(forceUpdate), crypto.clone())), Self::network_modules_retrieve_async);
	}

	fn network_modules_retrieve_prepare(
		&self,
		forceUpdate: bool,
	) -> Vec<ApiModulesID>
	{
		let mut moduleToRetrieveData = vec![];
		if (forceUpdate || self._links.cache_mustUpdate())
		{
			moduleToRetrieveData.push(ApiModulesID{ key: self._links.id_get(), timestamp: self._links.cache_time() });
		}

		for (key, oneModule) in self._blocks.iter()
		{
			let (cacheMustUpdate,cacheTime) = oneModule.with_untracked(|module| (module.inner().cache_mustUpdate(),module.inner().cache_time()));
			if (forceUpdate || cacheMustUpdate)
			{
				moduleToRetrieveData.push(ApiModulesID{ key: key.clone(), timestamp: cacheTime });
			}
		}

		return moduleToRetrieveData;
	}

	async fn network_modules_retrieve_async((moduleToRetrieve, crypto): (Vec<ApiModulesID>, ClientCryptoContext)) -> API_return_apply
	{
		if(moduleToRetrieve.len()==0) {return API_return_apply::default();}

		let mut apiReturn = API_return_apply::default();

		let apiReturnModules = match API_modules_retrieve(moduleToRetrieve).await
		{
			Ok(r) => r,
			Err(err) => {
				Self::network_error_apply(&mut apiReturn, err);
				return apiReturn;
			}
		};

		for (moduleId, moduleResult) in apiReturnModules {
			let ModuleReturnRetrieve::UPDATED(content) = moduleResult else {continue;};
			if (Self::module_inner_retrieve(&mut apiReturn, content, moduleId.clone(), &crypto))
			{
				apiReturn.moduleIdToRefresh.push(moduleId);
			}
		}

		return apiReturn;
	}

	////////////////////////////////////////
	// END MODULES RETRIEVE ZONE ---
	////////////////////////////////////////

	////////////////////////////////////////
	// START MONO MODULE RETRIEVE ZONE ---
	////////////////////////////////////////

	pub fn network_module_retrieve_caller(moduleHolder: ArcRwSignal<ModuleHolder>, module: ModuleID, forceUpdate: bool) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(moduleHolder, move |holder, crypto| Ok((holder.network_module_retrieve_prepare(module.clone(), forceUpdate), crypto.clone())), Self::network_module_retrieve_async);
	}

	fn network_module_retrieve_prepare(
		&self,
		moduleId: ModuleID,
		forceUpdate: bool,
	) -> Option<ApiModulesID>
	{
		for (key, oneModule) in self._blocks.iter()
			.filter(|(moduleIdSearch, _)| *moduleIdSearch == &moduleId)
		{
			let (cacheMustUpdate,cacheTime) = oneModule.with_untracked(|module| (module.inner().cache_mustUpdate(),module.inner().cache_time()));
			if (forceUpdate || cacheMustUpdate)
			{
				return Some(ApiModulesID{ key: key.clone(), timestamp: cacheTime });
			}
		}

		return None;
	}

	// do not apply auto module refresh
	async fn network_module_retrieve_async((moduleToRetrieveRaw, crypto): (Option<ApiModulesID>, ClientCryptoContext)) -> API_return_apply
	{
		let Some(moduleToRetrieve) = moduleToRetrieveRaw else {return API_return_apply::default();};

		let mut apiReturn = API_return_apply::default();

		let moduleId = moduleToRetrieve.key.clone();
		let moduleResult = match API_module_retrieve(moduleToRetrieve).await
		{
			Ok(r) => r,
			Err(err) => {
				Self::network_error_apply(&mut apiReturn, err);
				return apiReturn;
			}
		};

		let ModuleReturnRetrieve::UPDATED(content) = moduleResult else {return apiReturn};
		Self::module_inner_retrieve(&mut apiReturn, content, moduleId, &crypto);
		return apiReturn;
	}

	fn module_inner_retrieve(apiReturn: &mut API_return_apply, mut content: ModuleContent, moduleId: ModuleID, crypto: &ClientCryptoContext) -> bool
	{
		if (Self::import_decrypt_content(&mut content, crypto).is_err())
		{
			apiReturn.error.push(AllFrontErrorEnum::CRYPTO_DECRYPT_FAILED);
			return false;
		}

		if (content.typeModule == LinksHolder::MODULE_NAME)
		{
			let addReturnWork = move |moduleHolder: &mut ModuleHolder| {
				moduleHolder._links.id_set(content.id.clone());
				moduleHolder._links.import(content);
			};
			apiReturn.retrieve.push(Box::new(addReturnWork));
			return true;
		}

		let addReturnWork = move |moduleHolder: &mut ModuleHolder| {
			if let Some(foundModule) = moduleHolder._blocks.get_mut(&moduleId)
			{
				foundModule.update(|module| module.import(content.clone()));
			}
			else
			{
				let Some(moduleType) = ModuleType::newFromModuleContent(&content) else {return;};
				let thisModule = ModulePositions::newFromModuleContent(content, moduleType);
				if let Some(existing) = moduleHolder._blocks.get_mut(&moduleId)
				{
					existing.set(thisModule);
				}
				else
				{moduleHolder._blocks.insert(moduleId.clone(), ArcRwSignal::new(thisModule));}
			}

			let refreshTime = moduleHolder._blocks.get_mut(&moduleId).unwrap().with_untracked(|module| module.inner().refresh_time());
			if let Some(actions) = &moduleHolder._moduleActions
			{
				Self::add_cron(
					refreshTime,
					moduleId.clone(),
					&mut moduleHolder._crons,
					actions.clone(),
				);
			}
		};
		apiReturn.retrieve.push(Box::new(addReturnWork));
		return true;
	}

	////////////////////////////////////////
	// END MONO MODULE RETRIEVE ZONE ---
	////////////////////////////////////////

	////////////////////////////////////////
	// START MODULE REMOVE ZONE ---
	////////////////////////////////////////


	pub fn network_module_remove_caller(moduleHolder: ArcRwSignal<ModuleHolder>, moduleToRemove: ModuleID) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(moduleHolder, move |_, _| Ok(moduleToRemove.clone()), Self::network_module_remove_async);
	}

	async fn network_module_remove_async(moduleToRetrieve: ModuleID) -> API_return_apply
	{
		let mut apiReturn = API_return_apply::default();

		match API_module_remove(moduleToRetrieve.clone()).await
		{
			Ok(_) => {},
			Err(err) => {
				Self::network_error_apply(&mut apiReturn, err);
				return apiReturn;
			}
		};

		let addReturnWork = move |moduleHolder: &mut ModuleHolder| {
			 moduleHolder._crons.remove(&moduleToRetrieve);
			moduleHolder._blocks.remove(&moduleToRetrieve);
		};
		apiReturn.retrieve.push(Box::new(addReturnWork));

		return apiReturn;
	}


	////////////////////////////////////////
	// START MODULE REMOVE ZONE ---
	////////////////////////////////////////

	pub(super) fn module_refresh(epoch: ModuleHolderEpoch, modulesId: Vec<ModuleID>, toaster: ToasterContext)
	{
		let refreshTask = Self::getSingleton().with_untracked(|holder| {
			holder.module_refresh_prepare(epoch, modulesId, toaster)
		});
		if let Some(refreshTask) = refreshTask
		{
			refreshTask.spawn();
		}
	}

	fn module_refresh_prepare(&self, epoch: ModuleHolderEpoch, modulesId: Vec<ModuleID>, toaster: ToasterContext) -> Option<ModuleRefreshTask>
	{
		if (self._networkSuspended)
		{
			return None;
		}
		let owner = self.lifecycle_owner_get(epoch)?;
		let mut allBoxedFuture = vec![];
		for moduleId in modulesId
		{
			let Some(oneModule) = self._blocks.get(&moduleId)
			else
			{
				continue;
			};

			if let Some(actions) = &self._moduleActions
			{
				let tmp = oneModule.with_untracked(|module| module.inner()
					.refresh(actions.clone(), moduleId.clone(), toaster.clone()));
				if let Some(refreshFutur) = tmp
				{
					allBoxedFuture.push(refreshFutur);
				}
			}
		}
		if (allBoxedFuture.is_empty())
		{
			return None;
		}
		return Some(ModuleRefreshTask {
			owner,
			futures: allBoxedFuture,
		});
	}

	fn add_cron(
		refreshTimeRaw: RefreshTime,
		moduleId: ModuleID,
		crons: &mut HashMap<ModuleID, PausableStocker>,
		moduleActions: module_actions::ModuleActionFn,
	)
	{
		let timeMinute = match refreshTimeRaw
		{
			RefreshTime::NONE => {
				crons.remove(&moduleId);
				return;
			},
			RefreshTime::MINUTES(i) => i as u32,
			RefreshTime::HOURS(h) => h as u32 * 60,
		};

		let timeMillisecond = timeMinute * 60 * 1000;



		if let Some(cron) = crons.get_mut(&moduleId) {
			cron.set_interval(timeMillisecond);
			return;
		}

		let refresh_fn = moduleActions.refreshFn.clone();
		let tick_module_id = moduleId.clone();
		let tick = Arc::new(move || {
			(refresh_fn)(tick_module_id.clone());
		});

		crons.insert(moduleId, PausableStocker::new(timeMillisecond, tick));

	}

	pub fn links_get(&self) -> &LinksHolder
	{
		return &self._links;
	}

	pub fn links_get_mut(&mut self) -> &mut LinksHolder
	{
		return &mut self._links;
	}

	pub fn blocks_get(&self) -> &HashMap<ModuleID, ArcRwSignal<ModulePositions<ModuleType>>>
	{
		return &self._blocks;
	}

	pub fn blocks_view(&self) -> Vec<(ModuleID, ArcRwSignal<ModulePositions<ModuleType>>)> {
		let mut blocks = self._blocks
			.iter()
			.map(|(id, module)| (id.clone(), module.clone()))
			.collect::<Vec<_>>();
		blocks.sort_by(|(leftId,leftModule),(rightId,rightModule)| {
			let leftOrder = leftModule.with(|module| module.visual_order_get());
			let rightOrder = rightModule.with(|module| module.visual_order_get());
			return leftOrder.cmp(&rightOrder).then_with(|| leftId.cmp(rightId));
		});
		return blocks;
	}

	pub(crate) fn blocks_insert(&mut self, epoch: ModuleHolderEpoch, newmodule: ModulePositions<ModuleType>)
	{
		if (!self.lifecycle_epoch_isActive(epoch))
		{
			return;
		}
		newmodule.depth_set(self._blockNb as u32);
		self._blocks.insert(ModuleID::new(), ArcRwSignal::new(newmodule));
		self._blockNb += 1;
	}

	/// This function is used to decrypt the content of a moduleContent before generating the module
	/// return if the content have been correctly decrypted
	fn import_decrypt_content(moduleContent: &mut ModuleContent, crypto: &ClientCryptoContext) -> Result<(), AllFrontErrorEnum>
	{
		moduleContent.content = crypto.decrypt(&moduleContent.content)
			.map_err(|_| AllFrontErrorEnum::CRYPTO_DECRYPT_FAILED)?;
		return Ok(());
	}

	/// This function is used to encrypt the content of a moduleContent before sending it to the server
	/// return if the content have been correctly encrypted
	fn export_crypt_content(moduleContent: &mut ModuleContent, crypto: &ClientCryptoContext) -> Result<(), AllFrontErrorEnum>
	{
		moduleContent.content = crypto.encrypt(&moduleContent.content)
			.map_err(|_| AllFrontErrorEnum::CRYPTO_ENCRYPT_FAILED)?;
		return Ok(());
	}
}
