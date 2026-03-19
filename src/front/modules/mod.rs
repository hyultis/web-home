use crate::api::modules::components::{ApiModulesID, ModuleContent, ModuleID};
use crate::api::modules::{
	API_module_remove, API_modules_retrieve, API_modules_update,
	ModuleReturnRetrieve,
};
use crate::front::modules::components::{API_return_apply, Backable, Cacheable, ModuleName, PausableStocker, RefreshTime};
use crate::front::modules::link::LinksHolder;
use crate::front::utils::all_front_enum::AllFrontErrorEnum;
use crate::front::utils::users_data::UserData;
use leptoaster::ToasterContext;
use leptos::logging::log;
use leptos::prelude::{ArcRwSignal, Callable, ReadUntracked, RwSignal, Update, With};
use leptos_use::use_interval_fn;
use module_positions::ModulePositions;
use module_type::ModuleType;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use leptos::reactive::spawn_local;
use crate::front::utils::toaster_helpers::toastingErr;

pub mod components;
pub mod link;
mod mail;
pub mod module_actions;
pub mod module_positions;
pub(crate) mod module_type;
pub mod rss;
pub mod todo;
pub mod weather;

pub trait moduleContent: ModuleName + Backable + Cacheable {}
type ApiCall = Pin<Box<dyn Future<Output = API_return_apply>>>;


pub struct ModuleHolder
{
	_links: LinksHolder,
	_blocks: HashMap<ModuleID, ModulePositions<ModuleType>>,
	_crons: HashMap<ModuleID, PausableStocker>,
	_moduleActions: Option<module_actions::ModuleActionFn>,
	_blockNb: usize,
}

impl ModuleHolder
{
	pub fn new() -> Self
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

	pub fn reset(&mut self)
	{
		self._blocks = HashMap::new();
		self._links = LinksHolder::new();
		self._blockNb = 0;
	}

	fn network_apply(&mut self, mut toApply: API_return_apply,toaster: ToasterContext)
	{
		toApply.retrieve.into_iter().for_each(|f| f(self));
		toApply.update.into_iter().for_each(|f| f(self));

		self.module_refresh(toApply.moduleId.drain(..).collect(), toaster);
	}

	pub fn network_editMode_defferedCall(moduleHolder: ArcRwSignal<ModuleHolder>, toaster: ToasterContext, validate: bool) -> impl Future<Output = ()>
	{
		async move {
			let apiCall: Option<ApiCall> = moduleHolder.with(|holder|{
				let Some((login, _)) = UserData::loginLang_get_from_cookie()
				else
				{
					return None;
				};

				if(validate)
				{
					let preparedVar = holder.network_validate_prepare();
					return Some(
						Box::pin(async move {
							ModuleHolder::network_validate_prepare_async(login, preparedVar).await
						}) as ApiCall
					);
				}

				let preparedVar = holder.network_cancel_prepare(true);
				return Some(
					Box::pin(async move {
						ModuleHolder::network_cancel_prepare_async(login, preparedVar).await
					}) as ApiCall
				);
			});

			let Some(apiCall) = apiCall else {return;};
			let mut apiResult = apiCall.await;

			// if they are some error
			for err in apiResult.error.drain(..) {
				toastingErr(&toaster, err).await;
			};

			moduleHolder.update(|holder| {
				holder.network_apply(apiResult,toaster);
			});
		}
	}

	////////////////////////////////////////
	// START VALID EDITMODE ZONE ---
	////////////////////////////////////////

	fn network_validate_prepare(
		&self
	) -> Vec<ModuleContent>
	{
		let mut moduleToRetrieveData = vec![];

		let mut thisModuleContent = self._links.export();
		Self::export_crypt_content(&mut thisModuleContent);
		moduleToRetrieveData.push(thisModuleContent);

		for (_, oneModule) in self._blocks.iter()
		{
			let mut thisModuleContent =oneModule.export();
			Self::export_crypt_content(&mut thisModuleContent);
			moduleToRetrieveData.push(thisModuleContent);
		}

		return moduleToRetrieveData;
	}

	async fn network_validate_prepare_async(login: String, moduleToRetrieve: Vec<ModuleContent>) -> API_return_apply
	{
		let mut apiReturn = API_return_apply::default();

		let apiReturnModules = match API_modules_update(login.clone(), moduleToRetrieve, true).await
		{
			Ok(r) => r,
			Err(err) => {
				// TODO use translate key
				apiReturn.error.push(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}", err)));
				return apiReturn;
			}
		};

		return apiReturn;
	}

	////////////////////////////////////////
	// START VALID EDITMODE ZONE ---
	////////////////////////////////////////

	////////////////////////////////////////
	// START CANCEL EDITMODE ZONE ---
	////////////////////////////////////////

	fn network_cancel_prepare(
		&self,
		forceUpdate: bool,
	) -> Vec<ApiModulesID>
	{
		let mut moduleToRetrieveData = vec![];
		if (forceUpdate || self._links.cache_mustUpdate())
		{
			moduleToRetrieveData.push(ApiModulesID{ key: self._links.name_get(), timestamp: self._links.cache_time() });
		}

		for (key, oneModule) in self._blocks.iter()
		{
			if (forceUpdate || oneModule.inner().cache_mustUpdate())
			{
				moduleToRetrieveData.push(ApiModulesID{ key: key.clone(), timestamp: oneModule.inner().cache_time() });
			}
		}

		return moduleToRetrieveData;
	}

	async fn network_cancel_prepare_async(login: String, moduleToRetrieve: Vec<ApiModulesID>) -> API_return_apply
	{
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
			let ModuleReturnRetrieve::UPDATED(mut content) = moduleResult else {continue;};

			Self::import_decrypt_content(&mut content);
			apiReturn.moduleId.push(moduleId);

			if (content.typeModule == LinksHolder::MODULE_NAME)
			{
				let addReturnWork = move |moduleHolder: &mut ModuleHolder| {
					moduleHolder._links.name_set(content.name.clone());
					moduleHolder._links.import(content);
				};
				apiReturn.retrieve.push(Box::new(addReturnWork));
				continue;
			}

			let addReturnWork = move |moduleHolder: &mut ModuleHolder| {
				let Some(moduleType) = ModuleType::newFromModuleContent(&content) else {return;};

				moduleHolder._blocks.insert(content.name.clone(), ModulePositions::newFromModuleContent(content, moduleType));
			};
			apiReturn.retrieve.push(Box::new(addReturnWork));

		}

		return apiReturn;
	}

	////////////////////////////////////////
	// END CANCEL EDITMODE ZONE ---
	////////////////////////////////////////

	/*
	pub async fn editMode_cancel_old(
		&mut self,
		login: String,
		forceUpdate: bool,
	) -> Option<AllFrontErrorEnum>
	{
		if (forceUpdate || self._links.cache_mustUpdate())
		{
			let moduleName = self._links.module_name();

			if let Some(error) = Self::inner_retrieve(
				login.clone(),
				moduleName.clone(),
				self,
				|moduleHolder, moduleContent| {
					moduleHolder._links.import(moduleContent);
				},
			)
			.await
			{
				if(error!=AllFrontErrorEnum::MODULE_NOTEXIST) {
					return Some(error);
				}
			}
		}

		for (key, oneModule) in self._blocks.iter_mut()
		{
			if (forceUpdate || oneModule.inner().cache_mustUpdate())
			{
				let moduleName = format!("{}_{}", key, oneModule.inner().module_name());
				if let Some(error) = Self::inner_retrieve(
					login.clone(),
					moduleName.clone(),
					oneModule,
					|module, moduleContent| {
						module.import(moduleContent);
					},
				)
				.await
				{
					return Some(error);
				}

				if let Some(actions) = &self._moduleActions
				{
					Self::add_cron(oneModule, moduleName, &mut self._crons, actions.clone());
				}
			}
		}

		let foundModules =
			match API_module_retrieveMissingModule(login.clone(), vec!["links".to_string()]).await
			{
				Ok(foundModules) => foundModules,
				Err(err) =>
				{
					return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}", err)));
				}
			};

		for oneNewModuleName in foundModules
		{
			let split = oneNewModuleName.split("_").collect::<Vec<&str>>();
			if let Some(rawNbFound) = split.get(0)
			{
				if let Ok(nbFound) = rawNbFound.parse::<usize>()
				{
					if (self._blockNb <= nbFound)
					{
						self._blockNb = nbFound + 1;
					}
				}
			}

			if let Some(error) = Self::inner_retrieve(
				login.clone(),
				oneNewModuleName.clone(),
				self,
				|this, moduleContent| {
					let Some(moduleType) = ModuleType::newFromModuleContent(&moduleContent)
					else
					{
						return;
					};
					let mut oneModule =
						ModulePositions::newFromModuleContent(moduleContent, moduleType);

					if let Some(actions) = &this._moduleActions
					{
						Self::add_cron(
							&mut oneModule,
							oneNewModuleName.clone(),
							&mut this._crons,
							actions.clone(),
						);
						this._blocks.insert(oneNewModuleName, oneModule);
					}
				},
			)
			.await
			{
				return Some(error);
			}
		}

		return None;
	}
	*/

	pub async fn module_remove(
		&mut self,
		login: String,
		moduleId: ModuleID,
	) -> Option<AllFrontErrorEnum>
	{
		let Some(oneModule) = self._blocks.get_mut(&moduleId)
		else
		{
			return None;
		};

		if let Err(err) = API_module_remove(login.clone(), moduleId.clone()).await
		{
			return Some(AllFrontErrorEnum::SERVER_ERROR(
				"Impossible to remove module".to_string(),
			));
		}

		if let Some(oneModule) = self._crons.get_mut(&moduleId)
		{
			(oneModule.pause)();
		}

		return None;
	}

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
				let tmp = oneModule
					.inner()
					.refresh(actions.clone(), moduleId.clone(), toaster.clone());
				if let Some(refreshFutur) = tmp
				{
					allBoxedFutur.push(refreshFutur);
				}
			}
		}

		spawn_local(async move {
			for oneFutur in allBoxedFutur {
				oneFutur.await;
			}
		});
	}

	fn add_cron(
		module: &mut ModulePositions<ModuleType>,
		moduleId: ModuleID,
		crons: &mut HashMap<ModuleID, PausableStocker>,
		moduleActions: module_actions::ModuleActionFn,
	)
	{
		let refreshTime = match module.inner().refresh_time()
		{
			RefreshTime::NONE => None,
			RefreshTime::MINUTES(i) => Some(i as u64),
			RefreshTime::HOURS(h) => Some(h as u64 * 60),
		};

		if let Some(timeMinute) = refreshTime
		{
			let timeMillisecond = timeMinute * 60 * 1000;

			if let Some(cronModule) = crons.get_mut(&moduleId)
			{
				cronModule
					.interval
					.update(|oldInterval| *oldInterval = timeMillisecond);
				(cronModule.resume)();
			}
			else
			{
				let intervalS = RwSignal::new(timeMillisecond);
				let moduleNameInner = moduleId.clone();
				let moduleActionsInner = moduleActions.clone();
				let pausable = use_interval_fn(
					move || {
						log!(
							"cron module {:?} refresh to {}",
							moduleNameInner,
							timeMillisecond
						);
						moduleActionsInner.refreshFn.run(moduleNameInner.clone());
					},
					intervalS.clone(),
				);
				let pause = pausable.pause;
				let resume = pausable.resume;
				crons.insert(
					moduleId.clone(),
					PausableStocker {
						interval: intervalS,
						pause: Arc::new(move || pause()),
						resume: Arc::new(move || resume()),
					},
				);
			}
		}
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

	pub fn blocks_get(&self) -> &HashMap<ModuleID, ModulePositions<ModuleType>>
	{
		return &self._blocks;
	}

	pub fn blocks_insert(&mut self, newmodule: ModulePositions<ModuleType>)
	{
		newmodule.depth_set(self._blockNb as u32);
		self._blocks.insert(ModuleID::new(), newmodule);
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
