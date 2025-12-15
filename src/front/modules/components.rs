use std::pin::Pin;
use std::sync::Arc;
use leptoaster::ToasterContext;
use leptos::ev::Targeted;
use leptos::prelude::{OnTargetAttribute};
use leptos::prelude::{ElementChild, Update};
use leptos::prelude::{AnyView, ArcRwSignal, IntoAny, PropAttribute, RwSignal, StyleAttribute};
use leptos::view;
use time::UtcDateTime;
use serde::{Deserialize, Serialize};
use web_sys::{Event, HtmlInputElement};
use crate::api::modules::components::ModuleContent;
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::translate::Translate;

#[derive(Clone, Debug, Serialize,Deserialize)]
pub struct Cache
{
	lastUpdate: i64
}

impl Cache
{
	pub fn update(&mut self)
	{
		self.lastUpdate = Self::now();
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

	pub fn now() -> i64
	{
		UtcDateTime::now().unix_timestamp_nanos() as i64
	}
}

impl Default for Cache
{
	fn default() -> Self {
		Self {
			lastUpdate: Self::now(),
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


pub type BoxFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// struct that can be sent to / retrieved from backend
pub trait Backable
{
	fn typeModule(&self) -> String;
	fn draw(&self, editMode: RwSignal<bool>,moduleActions: ModuleActionFn,currentName: String) -> AnyView;

	fn refresh_time(&self) -> RefreshTime;
	fn refresh(&self,moduleActions: ModuleActionFn,currentName:String, toaster: ToasterContext) -> Option<BoxFuture>;

	fn export(&self) -> ModuleContent;
	fn import(&mut self, import: ModuleContent);

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self> where Self: Sized;

	fn size(&self) -> ModuleSizeContrainte;
}

pub struct ModuleSizeContrainte
{
	pub x_min: Option<u32>,
	pub x_max: Option<u32>,
	pub y_min: Option<u32>,
	pub y_max: Option<u32>,
}

impl Default for ModuleSizeContrainte
{
	fn default() -> Self {
		Self {
			x_min: None,
			x_max: None,
			y_min: None,
			y_max: None,
		}
	}
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

pub fn distant_time_simpler(timestamp: i64) -> AnyView
{
	match distant_time(timestamp){
		DISTANT_TIME_RESULT::FUTUR(time,key) => {view!{{time}<Translate key={key}/>}}
		DISTANT_TIME_RESULT::PAST(time,key) => {view!{{time}<Translate key={key}/>}}
	}.into_any()
}

pub enum FieldHelperType
{
	TEXT,
	PASSWORD,
	NUMBER(i64,i64),
}

pub struct FieldHelper<A,B, T>
where A: Fn(ArcRwSignal<T>) -> String + Clone + Send,
      B: Fn(Targeted<Event,HtmlInputElement>,&mut T) + Clone,
{
	field: ArcRwSignal<T>,
	update: ArcRwSignal<Cache>,
	TranslateKey: String,
	getField: A,
	updateField: B,
	style: String,
	inputType: FieldHelperType
}

impl<A,B,T> FieldHelper<A,B,T>
where A: Fn(ArcRwSignal<T>) -> String + Clone + Send + 'static,
      B: Fn(Targeted<Event,HtmlInputElement>,&mut T) + Clone + 'static,
      T: Send + Sync + 'static
{
	pub fn new(field: &ArcRwSignal<T>,update: &ArcRwSignal<Cache>,TranslateKey: impl ToString,getField: A,updateField: B) -> Self
	{
		Self {
			field: field.clone(),
			update: update.clone(),
			TranslateKey: TranslateKey.to_string(),
			getField,
			updateField,
			style: "".to_string(),
			inputType: FieldHelperType::TEXT,
		}
	}

	pub fn setInputType(&mut self,inputType: FieldHelperType)
	{
		self.inputType = inputType;
	}

	pub fn setFullSize(&mut self,isFullSize: bool)
	{
		self.style = "display:block;width:100%".to_string();
	}

	pub fn setStyle(&mut self,style: impl ToString)
	{
		self.style = style.to_string();
	}

	pub fn draw(&self) -> AnyView
	{
		if(self.TranslateKey.is_empty())
		{
			return self.drawInput();
		}

		let TranslateKey = self.TranslateKey.clone();
		view!{
			<label for={TranslateKey.clone()}>
				<Translate key={TranslateKey.clone()}/>
				{self.drawInput()}
			</label>
		}.into_any()
	}

	pub fn drawInput(&self) -> AnyView
	{
		let data = self.field.clone();
		let getField = self.getField.clone();
		let getFn = move || getField(data.clone());

		let data = self.field.clone();
		let cache = self.update.clone();
		let updateField = self.updateField.clone();
		let updateFn = move |ev| {
			data.update(|mut inner| updateField(ev, &mut inner));
			cache.update(|cache| cache.update());
		};

		let TranslateKey = self.TranslateKey.clone();

		let style = self.style.clone();

		match self.inputType {
			FieldHelperType::TEXT => {
				view!{
						<input type="text"
							style={style}
							name={TranslateKey.clone()}
							prop:value={getFn}
							on:input:target={updateFn} />
				}.into_any()
			}
			FieldHelperType::PASSWORD => {
				view!{
						<input type="password"
							style={style}
							name={TranslateKey.clone()}
							prop:value={getFn}
							on:input:target={updateFn} />
				}.into_any()
			}
			FieldHelperType::NUMBER(min, max) => {
				view!{
						<input type="number" min={min} max={max}
							style={style}
							name={TranslateKey.clone()}
							prop:value={getFn}
							on:input:target={updateFn} />
				}.into_any()
			}
		}
	}
}

pub enum RefreshTime
{
	NONE,
	MINUTES(u8),
	HOURS(u8),
}

#[derive(Clone)]
pub struct PausableStocker
{
	pub interval: RwSignal<u64>,
	pub pause: Arc<dyn Fn() + Send + Sync>,
	pub resume: Arc<dyn Fn() + Send + Sync>,
}