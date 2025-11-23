use std::collections::HashMap;
use module_positions::ModulePositions;
use module_type::ModuleType;
use crate::api::modules::{API_module_retrieve, API_module_retrieveMissingModule, API_module_update, ModuleReturnRetrieve, ModuleReturnUpdate};
use crate::api::modules::components::ModuleContent;
use crate::front::modules::components::{Backable, Cacheable};
use crate::front::modules::link::LinksHolder;
use crate::front::utils::all_front_enum::AllFrontErrorEnum;
use crate::HWebTrace;

pub mod link;
pub mod todo;
pub mod rss;
pub mod components;
pub mod module_positions;
pub(crate) mod module_type;

pub trait moduleContent: Backable + Cacheable{}

pub struct ModuleHolder
{
	_links: LinksHolder,
	_blocks: HashMap<String,ModulePositions<ModuleType>>,
	_blockNb: usize
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


	pub async fn editMode_validate(&mut self, login:String) -> Option<AllFrontErrorEnum>
	{
		if(self._links.cache_mustUpdate())
		{
			let module = self._links.export();
			match API_module_update(login.clone(), module).await
			{
				// TODO : when the module is outdated, we should update instead of returning an error
				Ok(ModuleReturnUpdate::OUTDATED) => {return Some(AllFrontErrorEnum::MODULE_OUTDATED);}
				Err(err) => {return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}",err)));}
				_ => {} // ModuleReturn::OK here to go next stuff
			}
		}


		for (key,oneModule) in self._blocks.iter_mut()
		{
			let mut module = oneModule.export();
			module.name = key.clone();
			match API_module_update(login.clone(), module).await
			{
				// TODO : when the module is outdated, we should update instead of returning an error
				Ok(ModuleReturnUpdate::OUTDATED) => {return Some(AllFrontErrorEnum::MODULE_OUTDATED);}
				Err(err) => {return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}",err)));}
				_ => {}
			}
		}

		return None;
	}

	pub async fn editMode_cancel(&mut self, login:String, forceUpdate: bool) -> Option<AllFrontErrorEnum>
	{
		if(forceUpdate || self._links.cache_mustUpdate())
		{
			let moduleName = self._links.typeModule();
			match API_module_retrieve(login.clone(), moduleName).await
			{
				Ok(ModuleReturnRetrieve::EMPTY) => {},
				Ok(ModuleReturnRetrieve::UPDATED(moduleContent)) => self._links.import(moduleContent),
				Err(err) => {return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}",err)));}
			}
		}

		for (key,oneModule) in self._blocks.iter_mut()
		{
			if(forceUpdate || oneModule.inner().cache_mustUpdate())
			{
				let moduleName = format!("{}_{}", key, oneModule.inner().typeModule());
				match API_module_retrieve(login.clone(), moduleName).await
				{
					Ok(ModuleReturnRetrieve::EMPTY) => {},
					Ok(ModuleReturnRetrieve::UPDATED(moduleContent)) => {
						oneModule.import(moduleContent);
					},
					Err(err) => { return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}", err))); }
				}
			}
		}

		let foundModules = match API_module_retrieveMissingModule(login.clone(), vec!["links".to_string()]).await
		{
			Ok(foundModules) => foundModules,
			Err(err) => { return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}", err))); }
		};

		for oneNewModuleName in foundModules
		{
			let split = oneNewModuleName.split("_").collect::<Vec<&str>>();
			if let Some(rawNbFound) =split.get(0)
			{
				if let Ok(nbFound) = rawNbFound.parse::<usize>() {
					if(self._blockNb <= nbFound)
						{self._blockNb = nbFound+1;}
				}
			}

			let oneModule = ModuleContent::newFromName(oneNewModuleName.clone());
			match API_module_retrieve(login.clone(), oneNewModuleName.clone()).await
			{
				Ok(ModuleReturnRetrieve::EMPTY) => {},
				Ok(ModuleReturnRetrieve::UPDATED(moduleContent)) => {
					let Some(moduleType) = ModuleType::newFromModuleContent(&moduleContent) else {continue};
					self._blocks.insert(oneNewModuleName,ModulePositions::newFromModuleContent(moduleContent,moduleType));
				},
				Err(err) => { return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}", err))); }
			}
		}
		HWebTrace!("_blockNb : {:?}",self._blockNb);

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

	pub fn blocks_get(&self) -> &HashMap<String,ModulePositions<ModuleType>>
	{
		return &self._blocks;
	}

	pub fn blocks_insert(&mut self,newmodule: ModulePositions<ModuleType>)
	{
		let name = format!("{}_{}",self._blockNb,newmodule.inner().typeModule());
		self._blocks.insert(name,newmodule);
		self._blockNb+=1;
	}
}