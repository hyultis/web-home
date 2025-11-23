use leptos::prelude::{AnyView, ArcRwSignal, RwSignal};
use time::UtcDateTime;
use serde::{Deserialize, Serialize};
use crate::api::modules::components::ModuleContent;

#[derive(Clone, Debug, Serialize,Deserialize)]
pub struct Cache
{
	lastUpdate: i64
}

impl Cache
{
	pub fn update(&mut self)
	{
		self.lastUpdate = UtcDateTime::now().unix_timestamp_nanos() as i64;
	}

	pub fn update_from(&mut self, value: i64)
	{
		self.lastUpdate = value;
	}

	pub fn newFrom(value: i64) -> Self
	{
		Self {
			lastUpdate: value
		}
	}

	/// if this cache is older than other
	pub fn isOlder(&self, other: &Cache) -> bool
	{
		return self.lastUpdate < other.lastUpdate;
	}

	/// if this cache is newer than other
	pub fn isNewer(&self, other: &Cache) -> bool
	{
		return self.lastUpdate > other.lastUpdate;
	}

	pub fn get(&self) -> i64
	{
		self.lastUpdate
	}
}

impl Default for Cache
{
	fn default() -> Self {
		Self {
			lastUpdate: UtcDateTime::now().unix_timestamp_nanos() as i64,
		}
	}
}

/// struct that can manage cache
pub trait Cacheable
{
	fn cache_mustUpdate(&self) -> bool;
	fn cache_getUpdate(&self) -> ArcRwSignal<Cache>;
	fn cache_getSended(&self) -> ArcRwSignal<Cache>;
}

/// struct that can be sent to / retrieved from backend
pub trait Backable
{
	fn typeModule(&self) -> String;
	fn draw(&self, editMode: RwSignal<bool>) -> AnyView;

	fn export(&self) -> ModuleContent;
	fn import(&mut self, import: ModuleContent);

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self> where Self: Sized;
}