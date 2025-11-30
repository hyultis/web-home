use crate::api::modules::components::ModuleContent;
use crate::api::modules::{
	API_module_retrieve, API_module_retrieveMissingModule, API_module_update, ModuleReturnRetrieve,
	ModuleReturnUpdate,
};
use crate::front::modules::components::{Backable, Cacheable};
use crate::front::modules::link::LinksHolder;
use crate::front::utils::all_front_enum::AllFrontErrorEnum;
use crate::HWebTrace;
use module_positions::ModulePositions;
use module_type::ModuleType;
use std::collections::HashMap;
use leptos::prelude::Read;
use crate::front::utils::users_data::UserData;

pub mod components;
pub mod link;
pub mod module_positions;
pub(crate) mod module_type;
pub mod rss;
pub mod todo;
pub mod module_actions;
pub mod weather;

pub trait moduleContent: Backable + Cacheable {}

pub struct ModuleHolder
{
	_links: LinksHolder,
	_blocks: HashMap<String, ModulePositions<ModuleType>>,
	_blockNb: usize,
}

impl ModuleHolder
{
	pub fn new() -> Self
	{
		Self {
			_links: LinksHolder::new(),
			_blocks: HashMap::new(),
			_blockNb: 0,
		}
	}

	pub fn reset(&mut self)
	{
		self._blocks = HashMap::new();
		self._links = LinksHolder::new();
		self._blockNb = 0;
	}

	pub async fn editMode_validate(&mut self, login: String) -> Option<AllFrontErrorEnum>
	{
		if (self._links.cache_mustUpdate())
		{
			let module = self._links.export();
			if let Some(error) = Self::inner_update(login.clone(),module).await
			{
				return Some(error);
			}
		}

		for (key, oneModule) in self._blocks.iter_mut()
		{
			if (oneModule.inner().cache_mustUpdate())
			{
				let mut module = oneModule.export();
				module.name = key.clone();
				if let Some(error) = Self::inner_update(login.clone(),module).await
				{
					return Some(error);
				}
			}
		}

		return None;
	}

	pub async fn editMode_cancel(
		&mut self,
		login: String,
		forceUpdate: bool,
	) -> Option<AllFrontErrorEnum>
	{
		if (forceUpdate || self._links.cache_mustUpdate())
		{
			let moduleName = self._links.typeModule();

			if let Some(error) =  Self::inner_retrieve(login.clone(), moduleName.clone(),&mut self._links, |module, moduleContent| {
				module.import(moduleContent);
			}).await
			{
				return Some(error);
			}
		}

		for (key, oneModule) in self._blocks.iter_mut()
		{
			if (forceUpdate || oneModule.inner().cache_mustUpdate())
			{
				let moduleName = format!("{}_{}", key, oneModule.inner().typeModule());
				if let Some(error) =  Self::inner_retrieve(login.clone(), moduleName.clone(),oneModule, |module, moduleContent| {
					module.import(moduleContent);
				}).await
				{
					return Some(error);
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

			if let Some(error) = Self::inner_retrieve(login.clone(), oneNewModuleName.clone(),&mut (), |_,moduleContent| {
				let Some(moduleType) = ModuleType::newFromModuleContent(&moduleContent)
				else
				{
					return;
				};
				self._blocks.insert(
					oneNewModuleName,
					ModulePositions::newFromModuleContent(moduleContent, moduleType),
				);
			}).await
			{
				return Some(error);
			}
		}
		HWebTrace!("_blockNb : {:?}", self._blockNb);

		return None;
	}

	pub async fn module_retrieve(&mut self, login: String, name: String)
	                           -> Option<AllFrontErrorEnum>
	{
		let Some(oneModule) = self._blocks.get_mut(&name)
		else
		{
			return None;
		};

		if (!oneModule.inner().cache_mustUpdate())
		{
			return None;
		}

		return Self::inner_retrieve(login.clone(), name.clone(),oneModule, |module, moduleContent| {
			module.import(moduleContent);
		}).await;
	}

	async fn inner_retrieve<T>(login: String, moduleName: String, sendInner: T, module: impl FnOnce(T,ModuleContent)) -> Option<AllFrontErrorEnum>
	{
		match API_module_retrieve(login.clone(), moduleName).await
		{
			Ok(ModuleReturnRetrieve::EMPTY) =>
				{}
			Ok(ModuleReturnRetrieve::UPDATED(mut moduleContent)) =>
				{
					Self::import_decrypt_content(&mut moduleContent);
					module(sendInner,moduleContent);
				}
			Err(err) =>
				{
					return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}", err)));
				}
		}

		return None;
	}

	/// This function is used to update the module on the server.
	/// It will encrypt the content of the module before sending it to the server.
	/// It will return an error if the module is outdated or if the server returns an error.
	pub async fn module_update(&mut self, login: String, name: String)
		-> Option<AllFrontErrorEnum>
	{
		let Some(oneModule) = self._blocks.get(&name)
		else
		{
			return None;
		};

		if (!oneModule.inner().cache_mustUpdate())
		{
			return None;
		}

		let mut module = oneModule.export();
		module.name = name.clone();

		return Self::inner_update(login,module).await;
	}

	async fn inner_update(login: String, mut module: ModuleContent) -> Option<AllFrontErrorEnum>
	{
		Self::import_crypt_content(&mut module);
		match API_module_update(login.clone(), module).await
		{
			// TODO : when the module is outdated, we should update instead of returning an error
			Ok(ModuleReturnUpdate::OUTDATED) =>
				{
					return Some(AllFrontErrorEnum::MODULE_OUTDATED);
				}
			Err(err) =>
				{
					return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}", err)));
				}
			_ =>
				{}
		}

		return None;
	}

	pub fn links_get(&self) -> &LinksHolder
	{
		return &self._links;
	}

	pub fn links_get_mut(&mut self) -> &mut LinksHolder
	{
		return &mut self._links;
	}

	pub fn blocks_get(&self) -> &HashMap<String, ModulePositions<ModuleType>>
	{
		return &self._blocks;
	}

	pub fn blocks_insert(&mut self, newmodule: ModulePositions<ModuleType>)
	{
		let name = format!("{}_{}", self._blockNb, newmodule.inner().typeModule());
		self._blocks.insert(name, newmodule);
		self._blockNb += 1;
	}

	/// This function is used to decrypt the content of a moduleContent before generating the module
	/// return if the content have been correctly decrypted
	fn import_decrypt_content(moduleContent: &mut ModuleContent) -> bool
	{
		let (userData, _) = UserData::cookie_signalGet();
		let Some(userData) = &*userData.read() else {return false};
		let Some(result) = userData.decrypt_with_password(&moduleContent.content) else {return false};
		moduleContent.content = result;
		return true;
	}

	/// This function is used to encrypt the content of a moduleContent before sending it to the server
	/// return if the content have been correctly encrypted
	fn import_crypt_content(moduleContent: &mut ModuleContent) -> bool
	{
		let (userData, _) = UserData::cookie_signalGet();
		let Some(userData) = &*userData.read() else {return false};
		let Some(result) = userData.crypt_with_password(&moduleContent.content) else {return false};
		moduleContent.content = serde_json::to_string(&result).unwrap_or_default();
		return true;
	}
}
