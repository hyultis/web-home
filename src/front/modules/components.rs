use leptos::prelude::{AnyView, ArcRwSignal, RwSignal};
use time::UtcDateTime;
use serde::{Deserialize, Serialize};
use crate::api::modules::components::ModuleContent;
use crate::front::modules::module_actions::ModuleActionFn;

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
	fn draw(&self, editMode: RwSignal<bool>,moduleActions: ModuleActionFn,currentName: String) -> AnyView;

	fn export(&self) -> ModuleContent;
	fn import(&mut self, import: ModuleContent);

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self> where Self: Sized;
}

// represent each unity of time, from the lowest to the highest
// (60,"SEC") => the first element is the max unity of time (0 = no max), the second element is the name of the unity of time
const ORDERED_TIME: [(u8, &str);6] = [
	(60,"SEC"),
	(60,"MIN"),
	(24,"HOUR"),
	(30,"DAY"),
	(12,"MON"),
	(0,"YEAR"),
];

pub enum DISTANT_TIME_RESULT
{
	FUTUR(u64,String),
	PAST(u64,String),
}

/// return a string representing the distance between now and the timestamp, in the form of "12 DAYS" or "12 MONTHS" or "12 YEARS" or "12 HOURS" or "12 MINUTES" or "12 SECONDS"
pub fn distant_time(timestamp: i64) -> DISTANT_TIME_RESULT
{
	let now = UtcDateTime::now().unix_timestamp();
	let distance = now-timestamp;

	let ifpast = distance<0;
	let mut distance = distance.abs();
	let mut key = ORDERED_TIME.get(0).unwrap().1.to_string();

	for (max,I18lKey) in ORDERED_TIME {
		key = I18lKey.to_string();
		if(distance<max as i64)	{break;}
		distance = distance/max as i64;
	}

	return match ifpast {
		true => DISTANT_TIME_RESULT::PAST(distance as u64,format!("DISTANT_TIME_RESULT_{}",key)),
		false => DISTANT_TIME_RESULT::FUTUR(distance as u64,format!("DISTANT_TIME_RESULT_{}",key)),
	};
}