mod caldav;
mod domain;
#[cfg(any(feature = "hydrate",test))]
mod holiday;
#[cfg(any(feature = "hydrate",test))]
mod icalendar_codec;
mod view;

use crate::api::modules::components::{ModuleContent,ModuleID};
use crate::front::modules::components::{
	Backable,BoxFuture,Cache,Cacheable,ModuleName,ModuleSizeContrainte,RefreshTime,moduleContent,
};
use crate::front::modules::module_actions::ModuleActionFn;
use caldav::CalDavError;
#[cfg(feature = "hydrate")]
use caldav::CalDavClient;
use domain::{
	CalendarCollection,CalendarConfig,CalendarEvent,CalendarHolidayError,CalendarPeriod,
	CalendarRejectedEvent,CalendarViewMode,
};
#[cfg(feature = "hydrate")]
use domain::CALENDAR_MAX_REJECTED_SAMPLES;
#[cfg(feature = "hydrate")]
use holiday::holidays_get;
use view::CalendarDraw;
use leptoaster::ToasterContext;
use leptos::prelude::{ArcRwSignal,Get,GetUntracked,IntoAny,RwSignal,Update,ViewFn};
use leptos::view;
use std::fmt;
use std::collections::BTreeMap;
use time::{Date,OffsetDateTime};

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
#[cfg_attr(not(feature = "hydrate"),allow(dead_code))]
enum CalendarLoadState
{
	Idle,
	Loading,
	Ready,
	ConfigurationRequired,
	Error(CalDavError),
}

#[derive(Clone)]
struct CalendarRuntime
{
	anchor: Option<Date>,
	period: Option<CalendarPeriod>,
	events: Vec<CalendarEvent>,
	loadState: CalendarLoadState,
	partialFailures: usize,
	rejectedEvents: usize,
	rejectedSamples: Vec<CalendarRejectedEvent>,
	holidays: BTreeMap<Date,Vec<String>>,
	holidayCountry: Option<String>,
	holidayLoading: bool,
	holidayError: Option<CalendarHolidayError>,
	stale: bool,
	discoveredCollections: Vec<CalendarCollection>,
	discoveryLoading: bool,
	discoveryError: Option<CalDavError>,
	#[cfg(feature = "hydrate")]
	refreshGeneration: u64,
}

impl Default for CalendarRuntime
{
	fn default() -> Self
	{
		Self {
			anchor: None,
			period: None,
			events: Vec::new(),
			loadState: CalendarLoadState::Idle,
			partialFailures: 0,
			rejectedEvents: 0,
			rejectedSamples: Vec::new(),
			holidays: BTreeMap::new(),
			holidayCountry: None,
			holidayLoading: false,
			holidayError: None,
			stale: false,
			discoveredCollections: Vec::new(),
			discoveryLoading: false,
			discoveryError: None,
			#[cfg(feature = "hydrate")]
			refreshGeneration: 0,
		}
	}
}

impl CalendarRuntime
{
	fn period_set(&mut self, anchor: Date, viewMode: CalendarViewMode) -> bool
	{
		let period = CalendarPeriod::from_anchor(anchor,viewMode);
		if (self.period != Some(period))
		{
			self.anchor = Some(anchor);
			self.period = Some(period);
			self.events.clear();
			self.loadState = CalendarLoadState::Idle;
			self.partialFailures = 0;
			self.rejectedEvents = 0;
			self.rejectedSamples.clear();
			self.holidays.clear();
			self.holidayCountry = None;
			self.holidayLoading = false;
			self.holidayError = None;
			self.stale = false;
			return true;
		}
		self.anchor = Some(anchor);
		return false;
	}

	#[cfg(feature = "hydrate")]
	fn refresh_start(
		&mut self,
		anchor: Date,
		period: CalendarPeriod,
		holidayCountry: Option<String>,
		holidayConfigured: bool,
	) -> u64
	{
		self.refreshGeneration = self.refreshGeneration.wrapping_add(1);
		self.anchor = Some(anchor);
		self.period = Some(period);
		self.stale = !self.events.is_empty();
		self.loadState = CalendarLoadState::Loading;
		self.partialFailures = 0;
		self.rejectedEvents = 0;
		self.rejectedSamples.clear();
		if (self.holidayCountry != holidayCountry)
		{
			self.holidays.clear();
		}
		self.holidayCountry = holidayCountry.clone();
		self.holidayLoading = holidayCountry.is_some();
		self.holidayError = (holidayConfigured && holidayCountry.is_none())
			.then_some(CalendarHolidayError::InvalidCountry);
		return self.refreshGeneration;
	}

	#[cfg(feature = "hydrate")]
	fn refresh_configurationRequired(&mut self)
	{
		self.refreshGeneration = self.refreshGeneration.wrapping_add(1);
		self.events.clear();
		self.loadState = CalendarLoadState::ConfigurationRequired;
		self.partialFailures = 0;
		self.rejectedEvents = 0;
		self.rejectedSamples.clear();
		self.holidays.clear();
		self.holidayCountry = None;
		self.holidayLoading = false;
		self.holidayError = None;
		self.stale = false;
	}

	#[cfg(feature = "hydrate")]
	fn refresh_finish(
		&mut self,
		generation: u64,
		mut events: Vec<CalendarEvent>,
		succeededCollections: usize,
		failures: Vec<CalDavError>,
		rejectedEvents: usize,
		rejectedSamples: Vec<CalendarRejectedEvent>,
	)
	{
		if (self.refreshGeneration != generation)
		{
			return;
		}
		if (succeededCollections > 0)
		{
			CalendarEvent::sort_deterministically(&mut events);
			self.events = events;
			self.loadState = CalendarLoadState::Ready;
			self.stale = false;
		}
		else
		{
			self.loadState = CalendarLoadState::Error(
				failures.first().copied().unwrap_or(CalDavError::InvalidResponse),
			);
			self.stale = !self.events.is_empty();
		}
		self.partialFailures = failures.len();
		self.rejectedEvents = rejectedEvents;
		self.rejectedSamples = rejectedSamples;
	}

	#[cfg(feature = "hydrate")]
	fn holidays_finish(
		&mut self,
		generation: u64,
		country: &str,
		result: Result<BTreeMap<Date,Vec<String>>,CalendarHolidayError>,
	)
	{
		if (self.refreshGeneration != generation || self.holidayCountry.as_deref() != Some(country))
		{
			return;
		}
		self.holidayLoading = false;
		match result
		{
			Ok(holidays) =>
			{
				self.holidays = holidays;
				self.holidayError = None;
			},
			Err(error) => self.holidayError = Some(error),
		}
	}
}

pub struct Calendar
{
	config: ArcRwSignal<CalendarConfig>,
	runtime: ArcRwSignal<CalendarRuntime>,
	_update: ArcRwSignal<Cache>,
	_sended: ArcRwSignal<Cache>,
}

impl fmt::Debug for Calendar
{
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result
	{
		formatter.debug_struct("Calendar").finish_non_exhaustive()
	}
}

impl Default for Calendar
{
	fn default() -> Self
	{
		Self {
			config: ArcRwSignal::new(CalendarConfig::default()),
			runtime: ArcRwSignal::new(CalendarRuntime::default()),
			_update: ArcRwSignal::new(Cache::default()),
			_sended: ArcRwSignal::new(Cache::default()),
		}
	}
}

impl Calendar
{
	pub(super) fn new() -> Self
	{
		Self::default()
	}

	#[cfg(feature = "hydrate")]
	fn refresh_period_get(&self) -> (Date,CalendarPeriod)
	{
		let runtime = self.runtime.get_untracked();
		if let (Some(anchor),Some(period)) = (runtime.anchor,runtime.period)
		{
			return (anchor,period);
		}
		let viewMode = self.config.get_untracked().viewMode;
		let anchor = browser_today_get();
		return (anchor,CalendarPeriod::from_anchor(anchor,viewMode));
	}
}

#[cfg(feature = "hydrate")]
fn browser_today_get() -> Date
{
	let date = js_sys::Date::new_0();
	let month = u8::try_from(date.get_month() + 1).ok().and_then(|month| month.try_into().ok());
	return month.and_then(|month| Date::from_calendar_date(date.get_full_year() as i32,month,date.get_date() as u8).ok())
		.unwrap_or_else(|| OffsetDateTime::now_utc().date());
}

#[cfg(feature = "hydrate")]
fn browserTimezone_get() -> String
{
	use js_sys::{Array,Intl,Object,Reflect};
	let options = Intl::DateTimeFormat::new(&Array::new(),&Object::new()).resolved_options();
	return Reflect::get(&options,&wasm_bindgen::JsValue::from_str("timeZone")).ok()
		.and_then(|timezone| timezone.as_string())
		.filter(|timezone| !timezone.is_empty())
		.unwrap_or_else(|| "UTC".to_string());
}

#[cfg(not(feature = "hydrate"))]
fn browserTimezone_get() -> String
{
	"UTC".to_string()
}

#[cfg(not(feature = "hydrate"))]
fn browser_today_get() -> Date
{
	OffsetDateTime::now_utc().date()
}

impl ModuleName for Calendar
{
	const MODULE_NAME: &'static str = "CALENDAR";
}

impl Backable for Calendar
{
	fn module_name(&self) -> String
	{
		Self::MODULE_NAME.to_string()
	}

	fn draw(&self,editMode: RwSignal<bool>,moduleActions: ModuleActionFn,moduleId: ModuleID) -> ViewFn
	{
		let config = self.config.clone();
		let runtime = self.runtime.clone();
		let update = self._update.clone();
		ViewFn::from(move || view! {
			<CalendarDraw
				config=config.clone()
				runtime=runtime.clone()
				update=update.clone()
				editMode=editMode
				moduleActions=moduleActions.clone()
				moduleId=moduleId.clone()
			/>
		}.into_any())
	}

	fn refresh_time(&self) -> RefreshTime
	{
		RefreshTime::MINUTES(10)
	}

	fn refresh(&self,moduleActions: ModuleActionFn,_moduleId: ModuleID,_toaster: ToasterContext) -> Option<BoxFuture>
	{
		#[cfg(not(feature = "hydrate"))]
		{
			let _ = moduleActions;
			return None;
		}
		#[cfg(feature = "hydrate")]
		{
			let config = self.config.get_untracked();
			let runtime = self.runtime.clone();
			let (anchor,period) = self.refresh_period_get();
			if (config.ready_validate().is_err())
			{
				runtime.update(|runtime| runtime.refresh_configurationRequired());
				return None;
			}
			let holidayConfigured = !config.holidayCountry.trim().is_empty();
			let holidayCountry = config.holidayCountry_get();
			let generation = runtime.try_update(|runtime| runtime.refresh_start(
				anchor,period,holidayCountry.clone(),holidayConfigured,
			))?;
			return Some(Box::pin(async move {
				let client = match CalDavClient::new(&config)
				{
					Ok(client) => client,
					Err(error) =>
					{
						runtime.update(|runtime| runtime.refresh_finish(
							generation,Vec::new(),0,vec![error],0,Vec::new(),
						));
						if let Some(country) = holidayCountry
						{
							let result = holidays_get(&country,period).await;
							if (moduleActions.lifecycle_isActive())
							{
								runtime.update(|runtime| runtime.holidays_finish(generation,&country,result));
							}
						}
						return;
					},
				};
				let mut events = Vec::new();
				let mut failures = Vec::new();
				let mut succeededCollections = 0;
				let mut rejectedEvents = 0;
				let mut rejectedSamples = Vec::new();
				let floatingTimezone = browserTimezone_get();
				for collection in &config.collections
				{
					if (!moduleActions.lifecycle_isActive())
					{
						return;
					}
					match client.events_get(collection,period,&floatingTimezone).await
					{
						Ok(parsed) =>
						{
							succeededCollections += 1;
							rejectedEvents += parsed.rejectedCount;
							let remainingSamples = CALENDAR_MAX_REJECTED_SAMPLES.saturating_sub(rejectedSamples.len());
							rejectedSamples.extend(parsed.rejectedSamples.into_iter().take(remainingSamples));
							events.extend(parsed.events);
						},
						Err(error) => failures.push(error),
					}
				}
				if (!moduleActions.lifecycle_isActive())
				{
					return;
				}
				runtime.update(|runtime| runtime.refresh_finish(
					generation,events,succeededCollections,failures,rejectedEvents,rejectedSamples,
				));
				if let Some(country) = holidayCountry
				{
					let result = holidays_get(&country,period).await;
					if (!moduleActions.lifecycle_isActive())
					{
						return;
					}
					runtime.update(|runtime| runtime.holidays_finish(generation,&country,result));
				}
			}));
		}
	}

	fn export(&self) -> ModuleContent
	{
		ModuleContent {
			id: ModuleID::new(),
			typeModule: self.module_name(),
			timestamp: self._update.get_untracked().get(),
			content: serde_json::to_string(&self.config.get_untracked()).unwrap_or_default(),
			..Default::default()
		}
	}

	fn import(&mut self,import: ModuleContent)
	{
		let Ok(config): Result<CalendarConfig,_> = serde_json::from_str(&import.content)
		else
		{
			return;
		};
		self.config.update(|current| *current = config);
		self.runtime.update(|runtime| *runtime = CalendarRuntime::default());
		self._update.update(|cache| cache.update_from(import.timestamp));
		self._sended.update(|cache| cache.update_from(import.timestamp));
	}

	fn isOlderThan(&self,other: &ModuleContent) -> bool
	{
		other.timestamp > self._update.get_untracked().get()
	}

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self>
	{
		let config = serde_json::from_str(&from.content).ok()?;
		Some(Self {
			config: ArcRwSignal::new(config),
			runtime: ArcRwSignal::new(CalendarRuntime::default()),
			_update: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
			_sended: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
		})
	}

	fn size(&self) -> ModuleSizeContrainte
	{
		ModuleSizeContrainte {
			x_min: Some(420),
			x_max: None,
			y_min: Some(360),
			y_max: None,
		}
	}
}

impl Cacheable for Calendar
{
	fn cache_time(&self) -> i64
	{
		self._update.get_untracked().get()
	}

	fn cache_mustUpdate(&self) -> bool
	{
		self._update.get_untracked().isNewer(&self._sended.get())
	}

	fn cache_getUpdate(&self) -> ArcRwSignal<Cache>
	{
		self._update.clone()
	}

	fn cache_getSended(&self) -> ArcRwSignal<Cache>
	{
		self._sended.clone()
	}
}

impl moduleContent for Calendar {}
