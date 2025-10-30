use crate::front::modules::components::{Backable, Cacheable};
use crate::front::modules::link::{LinksHolder};

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