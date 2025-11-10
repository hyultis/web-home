use crate::api::modules::{API_module_retrieve, API_module_update, ModuleReturn};
use crate::front::modules::components::{Backable, Cacheable};
use crate::front::modules::link::{LinksHolder};
use crate::front::utils::all_front_enum::AllFrontErrorEnum;

pub mod link;
pub mod todo;
pub mod rss;
pub mod components;

pub trait moduleContent: Backable + Cacheable{}

pub enum ModuleType
{
	RSS(String),
	TODO(String)
}

pub struct ModuleHolder
{
	_links: LinksHolder,
	_blocks: Vec<ModuleType>
}

impl ModuleHolder
{
	pub fn new() -> Self
	{
		Self {
			_links: LinksHolder::new(),
			_blocks: vec![],
		}
	}

	pub fn reset(&mut self)
	{
		self._blocks = vec![];
		self._links = LinksHolder::new();
	}


	pub async fn editMode_validate(&mut self, login:String) -> Option<AllFrontErrorEnum>
	{
		if(self._links.cache_mustUpdate())
		{
			let module = self._links.export();
			match API_module_update(login, module).await
			{
				Ok(ModuleReturn::OK) => return None,
				Ok(ModuleReturn::OUTDATED) => {return Some(AllFrontErrorEnum::MODULE_OUTDATED);}
				Err(err) => {return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}",err)));}
				_ => {}
			}
		}

		return Some(AllFrontErrorEnum::MODULE_NOTEXIST);
	}

	pub async fn editMode_cancel(&mut self, login:String) -> Option<AllFrontErrorEnum>
	{
		if(self._links.cache_mustUpdate())
		{
			let module = self._links.export();
			match API_module_retrieve(login, module).await
			{
				Ok(ModuleReturn::OK) => return None,
				Ok(ModuleReturn::OUTDATED) => {return Some(AllFrontErrorEnum::MODULE_OUTDATED);}
				Err(err) => {return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}",err)));}
				_ => {}
			}
		}

		return Some(AllFrontErrorEnum::MODULE_NOTEXIST);
	}

	pub fn links_get(&self) -> &LinksHolder
	{
		return &self._links;
	}

	pub fn links_get_mut(&mut self) -> &mut LinksHolder
	{
		return &mut self._links;
	}

	pub fn blocks_get_mut(&mut self) -> &mut Vec<ModuleType>
	{
		return &mut self._blocks;
	}
}