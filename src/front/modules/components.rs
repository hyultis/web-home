use leptos::prelude::{AnyView, ArcRwSignal, RwSignal};
use time::UtcDateTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize,Deserialize)]
pub struct Cache
{
	lastUpdate: i128
}

impl Cache
{
	pub fn update(&mut self)
	{
		self.lastUpdate = UtcDateTime::now().unix_timestamp_nanos();
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
}

impl Default for Cache
{
	fn default() -> Self {
		Self {
			lastUpdate: UtcDateTime::now().unix_timestamp_nanos(),
		}
	}
}

/// struct that can manage cache
pub trait Cacheable
{
	fn cache_get(&self) -> ArcRwSignal<Cache>;
}

/// struct that can be sent to / retrieved from backend
pub trait Backable
{
	fn name() -> String where Self: Sized;
	fn draw(&self, editMode: RwSignal<bool>) -> AnyView;

	fn export(&self) -> String;
	fn import(&mut self, importSerial: String);
}