use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use leptoaster::ToasterContext;
use leptos::prelude::{ArcRwSignal, Set, Update, WithUntracked};
use leptos::reactive::{spawn_local_scoped};
use crate::api::modules::{API_module_remove, API_module_retrieve, API_modules_retrieve, API_modules_update, ModuleApiError, ModuleReturnRetrieve};
use crate::api::modules::components::{ApiModulesID, ModuleContent, ModuleID};
use crate::front::modules::components::{API_return_apply, ApiCall, Backable, Cacheable, ModuleName, PausableStocker, RefreshTime};
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

#[cfg(test)]
mod tests
{
	use crate::api::modules::ModuleApiError;
	use crate::front::modules::components::API_return_apply;
	use crate::front::utils::all_front_enum::AllFrontErrorEnum;
	use crate::front::utils::users_data::ClientCryptoContext;

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
}

pub struct ModuleHolder
{
	_links: LinksHolder,
	_blocks: HashMap<ModuleID, ArcRwSignal<ModulePositions<ModuleType>>>,
	_crons: HashMap<ModuleID, PausableStocker>,
	_moduleActions: Option<module_actions::ModuleActionFn>,
	_blockNb: usize,
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
		}
	}

	pub fn moduleActions_set(&mut self, ma: module_actions::ModuleActionFn)
	{
		self._moduleActions = Some(ma);
	}

	fn network_apply(&mut self, mut toApply: API_return_apply,toaster: ToasterContext)
	{
		toApply.retrieve.into_iter().for_each(|f| f(self));
		toApply.update.into_iter().for_each(|f| f(self));

		self.module_refresh(toApply.moduleIdToRefresh.drain(..).collect(), toaster);
	}

	pub fn network_deferredCall(moduleHolder: ArcRwSignal<ModuleHolder>, toaster: ToasterContext, apiCall: impl FnOnce(ArcRwSignal<ModuleHolder>) -> Option<ApiCall>, toastingSuccess: Option<AllFrontUIEnum>) -> impl Future<Output = ()>
	{
		async move {
			let Some(apiCall) = apiCall(moduleHolder.clone()) else {return;};
			let mut apiResult = apiCall.await;
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
				}
			};

			if (authenticationRequired)
			{
				if (ClientState::expect().local_clear().is_err())
				{
					toastingErr(&toaster, AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
				}
			}

			if (!hasErrors)
			{
				if let Some(toastingSuccess) = toastingSuccess
				{
					toaster_helpers::toastingSuccess(&toaster, toastingSuccess).await;
				}
			}

			moduleHolder.update(|holder| {
				holder.network_apply(apiResult,toaster);
			});
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
		return Self::network_deferredCall_inner(moduleHolder, |holder, crypto| holder.network_modules_update_prepare(crypto), Self::network_modules_update_async);
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

	async fn network_modules_update_async(moduleToUpdate: Vec<ModuleContent>) -> API_return_apply
	{
		if(moduleToUpdate.len()==0) {return API_return_apply::default();}

		let mut apiReturn = API_return_apply::default();

		let apiReturnModules = match API_modules_update(moduleToUpdate, true).await
		{
			Ok(r) => r,
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
		return Self::network_deferredCall_inner(moduleHolder, move |holder, crypto| holder.network_module_update_prepare(module.clone(), crypto), Self::network_modules_update_async);
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

	/// try to get the module from the server,
	/// but only if its the most recent version.
	/// if the local version is the most recent, the module is updated onto the server
	pub async fn module_getOrUpdate(
		&mut self,
		login: String,
		moduleId: ModuleID,
	) -> Option<AllFrontErrorEnum>
	{
		return None;
		/*let Some(oneModule) = self._blocks.get_mut(&moduleId)
		else
		{
			return None;
		};

		if(oneModule.inner().cache_mustUpdate())
		{
			let mut exportedModule = oneModule.export();
			exportedModule.name = moduleId.clone();
			return Self::inner_update(login, exportedModule).await;
		}

		return Self::inner_retrieve(
			login.clone(),
			moduleId.clone(),
			oneModule,
			|module, moduleContent| {

				if(moduleContent.timestamp>module.inner().cache_getUpdate().get_untracked().get()) {
					module.import(moduleContent);
				}
			},
		).await;*/
	}

	pub fn module_refresh(&self, modulesId: Vec<ModuleID>, toaster: ToasterContext)
	{
		let mut allBoxedFutur = vec![];
		for moduleId in modulesId {
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
					allBoxedFutur.push(refreshFutur);
				}
			}
		}

		spawn_local_scoped(async move {
			for oneFutur in allBoxedFutur {
				oneFutur.await;
			}
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

	/// This function is used to update the module on the server.
	/// It will encrypt the content of the module before sending it to the server.
	/// It will return an error if the module is outdated or if the server returns an error.
	pub async fn module_update(&mut self, login: String, moduleId: ModuleID)
	                           -> Option<AllFrontErrorEnum>
	{
		return None;
		/*let Some(oneModule) = self._blocks.get(&moduleId)
		else
		{
			return None;
		};

		if (!oneModule.inner().cache_mustUpdate())
		{
			return None;
		}

		let mut module = oneModule.export();
		module.name = moduleId.clone();
		return Self::inner_update(login, module).await;*/
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
		self._blocks
			.iter()
			.map(|(id, module)| (id.clone(), module.clone()))
			.collect()
	}

	pub fn blocks_insert(&mut self, newmodule: ModulePositions<ModuleType>)
	{
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
