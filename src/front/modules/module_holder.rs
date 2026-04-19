use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use leptoaster::ToasterContext;
use leptos::prelude::{ArcRwSignal, ReadUntracked, Set, Update, WithUntracked};
use leptos::reactive::{spawn_local_scoped};
use leptos::leptos_dom::log;
use crate::api::modules::{API_module_remove, API_module_retrieve, API_modules_retrieve, API_modules_update, ModuleReturnRetrieve};
use crate::api::modules::components::{ApiModulesID, ModuleContent, ModuleID};
use crate::front::modules::components::{API_return_apply, ApiCall, Backable, Cacheable, ModuleName, PausableStocker, RefreshTime};
use crate::front::modules::link::LinksHolder;
use crate::front::modules::module_actions;
use crate::front::modules::module_positions::ModulePositions;
use crate::front::modules::module_type::ModuleType;
use crate::front::utils::all_front_enum::{AllFrontErrorEnum, AllFrontUIEnum};
use crate::front::utils::toaster_helpers;
use crate::front::utils::toaster_helpers::toastingErr;
use crate::front::utils::users_data::UserData;

thread_local! {
    static MODULE_HOLDER_SINGLETON: RefCell<Option<ArcRwSignal<ModuleHolder>>> =
        const { RefCell::new(None) };
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

			// if they are some error
			for err in apiResult.error.drain(..) {
				toastingErr(&toaster, err).await;
			};

			if let Some(toastingSuccess) = toastingSuccess {
				toaster_helpers::toastingSuccess(&toaster, toastingSuccess).await;
			}

			moduleHolder.update(|holder| {
				holder.network_apply(apiResult,toaster);
			});
		}
	}

	fn network_deferredCall_inner<AsyncCaller, AsyncReturn, DataType, DataPrepare>(moduleHolder: ArcRwSignal<ModuleHolder>, prepare: DataPrepare, async_caller: AsyncCaller) -> Option<ApiCall>
	where
		DataPrepare: Fn(&ModuleHolder) -> DataType + 'static,
		AsyncCaller: Fn(String, DataType) -> AsyncReturn + 'static,
		AsyncReturn: Future<Output = API_return_apply> + 'static,
		DataType: 'static,
	{
		return moduleHolder.with_untracked(|holder| {
			let Some((login, _)) = UserData::loginLang_get_from_cookie()
			else
			{
				return None;
			};

			let preparedVar = prepare(holder);
			return Some(
				Box::pin(async move {
					return async_caller(login, preparedVar).await;
				}) as ApiCall
			);
		});
	}

	////////////////////////////////////////
	// START MODULES UPDATE ZONE ---
	////////////////////////////////////////

	pub fn network_modules_update_caller(moduleHolder: ArcRwSignal<ModuleHolder>) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(moduleHolder, |holder| holder.network_modules_update_prepare(), Self::network_modules_update_async);
	}

	fn network_modules_update_prepare(
		&self
	) -> Vec<ModuleContent>
	{
		let mut moduleToUpdateData = vec![];

		let mut thisModuleContent = self._links.export();
		Self::export_crypt_content(&mut thisModuleContent);
		thisModuleContent.id = self._links.id_get();
		moduleToUpdateData.push(thisModuleContent);

		for (key, oneModule) in self._blocks.iter()
		{
			let mut thisModuleContent =oneModule.with_untracked(|module| module.export());
			Self::export_crypt_content(&mut thisModuleContent);
			thisModuleContent.id = key.clone();
			moduleToUpdateData.push(thisModuleContent);
		}

		return moduleToUpdateData;
	}

	async fn network_modules_update_async(login: String, moduleToUpdate: Vec<ModuleContent>) -> API_return_apply
	{
		if(moduleToUpdate.len()==0) {return API_return_apply::default();}

		let mut apiReturn = API_return_apply::default();

		let apiReturnModules = match API_modules_update(login.clone(), moduleToUpdate, true).await
		{
			Ok(r) => r,
			Err(err) => {
				apiReturn.error.push(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}", err)));
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
		return Self::network_deferredCall_inner(moduleHolder, move |holder| holder.network_module_update_prepare(module.clone()), Self::network_modules_update_async);
	}

	fn network_module_update_prepare(&self, moduleId: ModuleID) -> Vec<ModuleContent>
	{
		let mut moduleToRetrieveData = vec![];

		for (key, oneModule) in self._blocks.iter()
			.filter(|(moduleIdSearch, _)| *moduleIdSearch == &moduleId)
		{
			let mut thisModuleContent =oneModule.with_untracked(|module| module.export());
			Self::export_crypt_content(&mut thisModuleContent);
			thisModuleContent.id = key.clone();
			moduleToRetrieveData.push(thisModuleContent);
		}

		return moduleToRetrieveData;
	}

	////////////////////////////////////////
	// END MONO MODULE UPDATE ZONE ---
	////////////////////////////////////////

	////////////////////////////////////////
	// START MODULES RETRIEVE ZONE ---
	////////////////////////////////////////

	pub fn network_modules_retrieve_caller(moduleHolder: ArcRwSignal<ModuleHolder>, forceUpdate: bool) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(moduleHolder, move |holder| holder.network_modules_retrieve_prepare(forceUpdate), Self::network_modules_retrieve_async);
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

	async fn network_modules_retrieve_async(login: String, moduleToRetrieve: Vec<ApiModulesID>) -> API_return_apply
	{
		if(moduleToRetrieve.len()==0) {return API_return_apply::default();}

		let mut apiReturn = API_return_apply::default();

		let apiReturnModules = match API_modules_retrieve(login.clone(), moduleToRetrieve).await
		{
			Ok(r) => r,
			Err(err) => {
				apiReturn.error.push(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}", err)));
				return apiReturn;
			}
		};

		for (moduleId, moduleResult) in apiReturnModules {
			let ModuleReturnRetrieve::UPDATED(content) = moduleResult else {continue;};
			apiReturn.moduleIdToRefresh.push(moduleId.clone());
			Self::module_inner_retrieve(&mut apiReturn,content, moduleId);
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
		return Self::network_deferredCall_inner(moduleHolder, move |holder| holder.network_module_retrieve_prepare(module.clone(),forceUpdate), Self::network_module_retrieve_async);
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
	async fn network_module_retrieve_async(login: String, moduleToRetrieveRaw: Option<ApiModulesID>) -> API_return_apply
	{
		let Some(moduleToRetrieve) = moduleToRetrieveRaw else {return API_return_apply::default();};

		let mut apiReturn = API_return_apply::default();

		let moduleId = moduleToRetrieve.key.clone();
		let moduleResult = match API_module_retrieve(login.clone(), moduleToRetrieve).await
		{
			Ok(r) => r,
			Err(err) => {
				apiReturn.error.push(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}", err)));
				return apiReturn;
			}
		};

		let ModuleReturnRetrieve::UPDATED(content) = moduleResult else {return apiReturn};
		Self::module_inner_retrieve(&mut apiReturn,content, moduleId);
		return apiReturn;
	}

	fn module_inner_retrieve(apiReturn: &mut API_return_apply, mut content: ModuleContent, moduleId: ModuleID)
	{
		Self::import_decrypt_content(&mut content);

		if (content.typeModule == LinksHolder::MODULE_NAME)
		{
			let addReturnWork = move |moduleHolder: &mut ModuleHolder| {
				moduleHolder._links.id_set(content.id.clone());
				moduleHolder._links.import(content);
			};
			apiReturn.retrieve.push(Box::new(addReturnWork));
			return;
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
	}

	////////////////////////////////////////
	// END MONO MODULE RETRIEVE ZONE ---
	////////////////////////////////////////

	////////////////////////////////////////
	// START MODULE REMOVE ZONE ---
	////////////////////////////////////////


	pub fn network_module_remove_caller(moduleHolder: ArcRwSignal<ModuleHolder>, moduleToRemove: ModuleID) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(moduleHolder, move |holder| moduleToRemove.clone(), Self::network_module_remove_async);
	}

	async fn network_module_remove_async(login: String, moduleToRetrieve: ModuleID) -> API_return_apply
	{
		let mut apiReturn = API_return_apply::default();

		match API_module_remove(login.clone(), moduleToRetrieve.clone()).await
		{
			Ok(_) => {},
			Err(err) => {
				apiReturn.error.push(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}", err)));
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
	fn import_decrypt_content(moduleContent: &mut ModuleContent) -> bool
	{
		let (userData, _) = UserData::cookie_signalGet();
		let Some(userData) = &*userData.read_untracked()
		else
		{
			return false;
		};
		let Some(result) = userData.decrypt_with_password(&moduleContent.content)
		else
		{
			return false;
		};
		moduleContent.content = result;
		return true;
	}

	/// This function is used to encrypt the content of a moduleContent before sending it to the server
	/// return if the content have been correctly encrypted
	fn export_crypt_content(moduleContent: &mut ModuleContent) -> bool
	{
		let (userData, _) = UserData::cookie_signalGet();
		let Some(userData) = &*userData.read_untracked()
		else
		{
			return false;
		};
		let Some(result) = userData.crypt_with_password(&moduleContent.content)
		else
		{
			return false;
		};
		moduleContent.content = serde_json::to_string(&result).unwrap_or_default();
		return true;
	}
}