use strum_macros::{EnumDiscriminants, EnumIter};
use module_positions::ModulePositions;
use crate::api::modules::{API_module_retrieve, API_module_update, ModuleReturn};
use crate::front::modules::components::{Backable, Cacheable};
use crate::front::modules::link::LinksHolder;
use crate::front::modules::todo::Todo;
use crate::front::utils::all_front_enum::AllFrontErrorEnum;

pub mod link;
pub mod todo;
pub mod rss;
pub mod components;
pub mod module_positions;

pub trait moduleContent: Backable + Cacheable{}

#[derive(EnumDiscriminants)]
#[strum_discriminants(derive(strum_macros::Display,EnumIter))]
pub enum ModuleType
{
	#[strum(to_string = "RSS")]
	RSS(String),
	#[strum(to_string = "TODO")]
	TODO(Todo)
}

pub struct ModuleHolder
{
	_links: LinksHolder,
	_blocks: Vec<ModulePositions<ModuleType>>
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
				Ok(ModuleReturn::OUTDATED) => {return Some(AllFrontErrorEnum::MODULE_OUTDATED);}
				Err(err) => {return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}",err)));}
				_ => {} // ModuleReturn::OK here to go next stuff
			}
		}

		return None;
	}

	pub async fn editMode_cancel(&mut self, login:String, forceUpdate: bool) -> Option<AllFrontErrorEnum>
	{
		if(forceUpdate || self._links.cache_mustUpdate())
		{
			let moduleName = self._links.name();
			match API_module_retrieve(login, moduleName).await
			{
				Ok(ModuleReturn::UPDATED(moduleContent)) => {

					self._links.import(moduleContent);
					return None;
				},
				Ok(ModuleReturn::OUTDATED) => {return Some(AllFrontErrorEnum::MODULE_OUTDATED);}
				Err(err) => {return Some(AllFrontErrorEnum::SERVER_ERROR(format!("{:?}",err)));}
				_ => {}
			}
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

	pub fn blocks_get(&self) -> &Vec<ModulePositions<ModuleType>>
	{
		return &self._blocks;
	}

	pub fn blocks_get_mut(&mut self) -> &mut Vec<ModulePositions<ModuleType>>
	{
		return &mut self._blocks;
	}
}