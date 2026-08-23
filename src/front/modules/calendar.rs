mod caldav;
mod domain;
#[cfg(any(feature = "hydrate",test))]
mod holiday;
#[cfg(any(feature = "hydrate",test))]
mod icalendar_codec;
mod view;

use crate::api::modules::components::{ModuleContent,ModuleID};
use crate::front::ai::automation::{
	AiActionApplyResult,AiActionCapability,AiActionFuture,AiAutomationCapable,AiCapabilityCatalog,
	AiModuleGrant,AiNamedValue,AiTextChoice,AiValidatedAction,AiValue,AiValueDefinition,
};
use crate::front::modules::components::{
	Backable,BoxFuture,Cache,Cacheable,ModuleConfigViewFn,ModuleName,ModuleSizeContrainte,
	RefreshTime,moduleContent,
};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::browser;
use caldav::CalDavError;
#[cfg(feature = "hydrate")]
use caldav::CalDavClient;
use domain::{
	CalendarCollection,CalendarConfig,CalendarEvent,CalendarHolidayError,CalendarPeriod,
	CalendarCreateInput,CalendarCreateMoment,CalendarRecurrence,CalendarRecurrenceEnd,
	CalendarRecurrenceFrequency,CalendarRejectedEvent,CalendarViewMode,
};
#[cfg(feature = "hydrate")]
use domain::CALENDAR_MAX_REJECTED_SAMPLES;
#[cfg(feature = "hydrate")]
use holiday::holidays_get;
use view::{CalendarConfigDraw,CalendarDraw};
use leptoaster::ToasterContext;
use leptos::prelude::{ArcRwSignal,Get,GetUntracked,IntoAny,RwSignal,Update,ViewFn};
use leptos::view;
use std::fmt;
use std::collections::BTreeMap;
use std::sync::Arc;
use time::{Date,Duration,Month,OffsetDateTime,PrimitiveDateTime,Time};

const CALENDAR_AI_ACTION_CREATE: &str = "calendar.event.create";
const CALENDAR_AI_ARGUMENT_COLLECTION: &str = "collection";
const CALENDAR_AI_ARGUMENT_TITLE: &str = "title";
const CALENDAR_AI_ARGUMENT_START: &str = "start";
const CALENDAR_AI_ARGUMENT_END: &str = "end";
const CALENDAR_AI_ARGUMENT_ALL_DAY: &str = "all_day";
const CALENDAR_AI_ARGUMENT_TIMEZONE: &str = "timezone";
const CALENDAR_AI_ARGUMENT_DESCRIPTION: &str = "description";
const CALENDAR_AI_ARGUMENT_LOCATION: &str = "location";
const CALENDAR_AI_ARGUMENT_RECURRENCE_FREQUENCY: &str = "recurrence_frequency";
const CALENDAR_AI_ARGUMENT_RECURRENCE_INTERVAL: &str = "recurrence_interval";
const CALENDAR_AI_ARGUMENT_RECURRENCE_UNTIL: &str = "recurrence_until";
const CALENDAR_AI_ARGUMENT_RECURRENCE_COUNT: &str = "recurrence_count";
const CALENDAR_AI_MOMENT_PATTERN: &str = r"^[0-9]{4}-[0-9]{2}-[0-9]{2}(T[0-9]{2}:[0-9]{2}(:[0-9]{2})?)?$";
const CALENDAR_AI_DATE_PATTERN: &str = r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$";
const CALENDAR_AI_TIMEZONE_PATTERN: &str = r"^[A-Za-z0-9_+-]+(/[A-Za-z0-9_+-]+)*$";
const CALENDAR_AI_MOMENT_FORMAT: &str = "Use YYYY-MM-DD when all_day is true, or YYYY-MM-DDTHH:MM / YYYY-MM-DDTHH:MM:SS when all_day is false. Never append Z or a UTC offset.";

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

	fn aiArgument_get<'a>(arguments: &'a [AiNamedValue],id: &str) -> Option<&'a AiValue>
	{
		return arguments.iter().find(|argument| argument.id == id).map(|argument| &argument.value);
	}

	fn aiText_get(arguments: &[AiNamedValue],id: &str) -> Option<String>
	{
		return match Self::aiArgument_get(arguments,id)
		{
			Some(AiValue::Text(value)) => Some(value.clone()),
			_ => None,
		};
	}

	fn aiInteger_get(arguments: &[AiNamedValue],id: &str) -> Option<i64>
	{
		return match Self::aiArgument_get(arguments,id)
		{
			Some(AiValue::Integer(value)) => Some(*value),
			_ => None,
		};
	}

	fn aiBoolean_get(arguments: &[AiNamedValue],id: &str) -> Option<bool>
	{
		return match Self::aiArgument_get(arguments,id)
		{
			Some(AiValue::Boolean(value)) => Some(*value),
			_ => None,
		};
	}

	fn aiDate_parse(value: &str) -> Option<Date>
	{
		let mut parts = value.split('-');
		let year = parts.next()?;
		let month = parts.next()?;
		let day = parts.next()?;
		if (parts.next().is_some()
			|| year.len() != 4 || month.len() != 2 || day.len() != 2
			|| !year.bytes().all(|value| value.is_ascii_digit())
			|| !month.bytes().all(|value| value.is_ascii_digit())
			|| !day.bytes().all(|value| value.is_ascii_digit()))
		{
			return None;
		}
		let month: Month = month.parse::<u8>().ok()?.try_into().ok()?;
		return Date::from_calendar_date(year.parse().ok()?,month,day.parse().ok()?).ok();
	}

	fn aiDateTime_parse(value: &str) -> Option<PrimitiveDateTime>
	{
		let (date,time) = value.split_once('T')?;
		let date = Self::aiDate_parse(date)?;
		let mut parts = time.split(':');
		let hour = parts.next()?.parse().ok()?;
		let minute = parts.next()?.parse().ok()?;
		let second = match parts.next()
		{
			Some(value) => value.parse().ok()?,
			None => 0,
		};
		if (parts.next().is_some()) {return None;}
		return Time::from_hms(hour,minute,second).ok()
			.map(|time| PrimitiveDateTime::new(date,time));
	}

	fn aiUid_get(actionKey: &str) -> String
	{
		let token = actionKey.trim_end_matches('=')
			.replace('+',"-")
			.replace('/',"_");
		return format!("webhome-ai-{}",token);
	}

	fn aiCollectionLabel_get(collection: &CalendarCollection) -> String
	{
		let name = collection.name.trim();
		if (name.is_empty()
			|| name.len() > domain::CALENDAR_MAX_COLLECTION_NAME_BYTES
			|| name.chars().any(char::is_control))
		{
			return collection.href.clone();
		}
		return name.to_string();
	}

	fn aiRecurrence_get(arguments: &[AiNamedValue]) -> Option<Option<CalendarRecurrence>>
	{
		let frequency = Self::aiText_get(arguments,CALENDAR_AI_ARGUMENT_RECURRENCE_FREQUENCY);
		let interval = Self::aiInteger_get(arguments,CALENDAR_AI_ARGUMENT_RECURRENCE_INTERVAL);
		let until = Self::aiText_get(arguments,CALENDAR_AI_ARGUMENT_RECURRENCE_UNTIL);
		let count = Self::aiInteger_get(arguments,CALENDAR_AI_ARGUMENT_RECURRENCE_COUNT);
		let Some(frequency) = frequency
		else
		{
			return (interval.is_none() && until.is_none() && count.is_none()).then_some(None);
		};
		let frequency = match frequency.as_str()
		{
			"daily" => CalendarRecurrenceFrequency::Daily,
			"weekly" => CalendarRecurrenceFrequency::Weekly,
			"monthly" => CalendarRecurrenceFrequency::Monthly,
			"yearly" => CalendarRecurrenceFrequency::Yearly,
			_ => return None,
		};
		let interval = u16::try_from(interval.unwrap_or(1)).ok()?;
		let end = match (until,count)
		{
			(None,None) => CalendarRecurrenceEnd::Never,
			(Some(until),None) => {
				let date = Self::aiDate_parse(&until)?;
				let utcEndTimestamp = PrimitiveDateTime::new(date,Time::from_hms(23,59,59).ok()?)
					.assume_utc().unix_timestamp();
				CalendarRecurrenceEnd::Until {date,utcEndTimestamp}
			},
			(None,Some(count)) => CalendarRecurrenceEnd::Count(u16::try_from(count).ok()?),
			(Some(_),Some(_)) => return None,
		};
		return Some(Some(CalendarRecurrence {frequency,interval,end}));
	}

	fn aiCreateInput_get(config: &CalendarConfig,action: &AiValidatedAction) -> Option<(CalendarCollection,CalendarCreateInput)>
	{
		if (action.action != CALENDAR_AI_ACTION_CREATE
			|| !config.aiGrant.action_allows(CALENDAR_AI_ACTION_CREATE))
		{
			return None;
		}
		config.ready_validate().ok()?;
		let collectionHref = Self::aiText_get(&action.arguments,CALENDAR_AI_ARGUMENT_COLLECTION)?;
		let collection = config.collections.iter()
			.find(|collection| collection.href == collectionHref).cloned()?;
		let title = Self::aiText_get(&action.arguments,CALENDAR_AI_ARGUMENT_TITLE)?;
		let start = Self::aiText_get(&action.arguments,CALENDAR_AI_ARGUMENT_START)?;
		let end = Self::aiText_get(&action.arguments,CALENDAR_AI_ARGUMENT_END);
		let allDay = Self::aiBoolean_get(&action.arguments,CALENDAR_AI_ARGUMENT_ALL_DAY)?;
		let timezone = Self::aiText_get(&action.arguments,CALENDAR_AI_ARGUMENT_TIMEZONE)
			.unwrap_or_else(browser::timezone_get);
		let (start,end) = if (allDay)
		{
			let start = Self::aiDate_parse(&start)?;
			let end = match end
			{
				Some(end) => Self::aiDate_parse(&end)?,
				None => start.next_day()?,
			};
			(CalendarCreateMoment::AllDay(start),CalendarCreateMoment::AllDay(end))
		}
		else
		{
			let start = Self::aiDateTime_parse(&start)?;
			let end = match end
			{
				Some(end) => Self::aiDateTime_parse(&end)?,
				None => start.checked_add(Duration::hours(1))?,
			};
			(CalendarCreateMoment::Local(start),CalendarCreateMoment::Local(end))
		};
		let input = CalendarCreateInput {
			title,
			description: Self::aiText_get(&action.arguments,CALENDAR_AI_ARGUMENT_DESCRIPTION).unwrap_or_default(),
			location: Self::aiText_get(&action.arguments,CALENDAR_AI_ARGUMENT_LOCATION).unwrap_or_default(),
			start,
			end,
			timezone,
			recurrence: Self::aiRecurrence_get(&action.arguments)?,
		};
		input.validate().ok()?;
		return Some((collection,input));
	}
}

impl AiAutomationCapable for Calendar
{
	fn ai_capabilities(&self) -> AiCapabilityCatalog
	{
		let collectionChoices = self.config.get_untracked().collections.into_iter().map(|collection| {
			let label = Self::aiCollectionLabel_get(&collection);
			return AiTextChoice {value: collection.href,label};
		}).collect();
		let recurrenceChoices = ["daily","weekly","monthly","yearly"].into_iter()
			.map(|value| AiTextChoice {value: value.to_string(),label: value.to_string()})
			.collect();
		return AiCapabilityCatalog {
			events: Vec::new(),
			actions: vec![AiActionCapability {
				id: CALENDAR_AI_ACTION_CREATE,
				translateKey: "MODULE_CALENDAR_AI_CREATE_ACTION",
				arguments: vec![
					AiValueDefinition::textWithFixedChoices(
						CALENDAR_AI_ARGUMENT_COLLECTION,"MODULE_CALENDAR_COLLECTION",
						domain::CALENDAR_MAX_URL_BYTES,collectionChoices,
					),
					AiValueDefinition::text(CALENDAR_AI_ARGUMENT_TITLE,"MODULE_CALENDAR_EVENT_TITLE",true,4_096),
					AiValueDefinition::text(CALENDAR_AI_ARGUMENT_START,"MODULE_CALENDAR_START",true,64)
						.withTextConstraint(CALENDAR_AI_MOMENT_PATTERN,CALENDAR_AI_MOMENT_FORMAT),
					AiValueDefinition::text(CALENDAR_AI_ARGUMENT_END,"MODULE_CALENDAR_END",false,64)
						.withTextConstraint(CALENDAR_AI_MOMENT_PATTERN,CALENDAR_AI_MOMENT_FORMAT),
					AiValueDefinition::boolean(CALENDAR_AI_ARGUMENT_ALL_DAY,"MODULE_CALENDAR_ALL_DAY",true),
					AiValueDefinition::text(CALENDAR_AI_ARGUMENT_TIMEZONE,"MODULE_CALENDAR_AI_TIMEZONE",false,128)
						.withTextConstraint(CALENDAR_AI_TIMEZONE_PATTERN,"Use an IANA time-zone name such as Europe/Paris or UTC."),
					AiValueDefinition::text(CALENDAR_AI_ARGUMENT_DESCRIPTION,"MODULE_CALENDAR_DESCRIPTION",false,32 * 1024),
					AiValueDefinition::text(CALENDAR_AI_ARGUMENT_LOCATION,"MODULE_CALENDAR_LOCATION",false,8 * 1024),
					AiValueDefinition::textWithChoices(
						CALENDAR_AI_ARGUMENT_RECURRENCE_FREQUENCY,"MODULE_CALENDAR_RECURRENCE_FREQUENCY",
						false,16,recurrenceChoices,
					),
					AiValueDefinition::integer(CALENDAR_AI_ARGUMENT_RECURRENCE_INTERVAL,"MODULE_CALENDAR_RECURRENCE_INTERVAL",false),
					AiValueDefinition::text(CALENDAR_AI_ARGUMENT_RECURRENCE_UNTIL,"MODULE_CALENDAR_RECURRENCE_UNTIL_DATE",false,10)
						.withTextConstraint(CALENDAR_AI_DATE_PATTERN,"Use a date formatted YYYY-MM-DD."),
					AiValueDefinition::integer(CALENDAR_AI_ARGUMENT_RECURRENCE_COUNT,"MODULE_CALENDAR_RECURRENCE_OCCURRENCES",false),
				],
				promptRules: vec![
					"Use the exact collection value from allowed_values, never its human-readable label.",
					"Create an event only when source_data clearly describes a concrete calendar item covered by optional_user_instructions, such as an appointment, meeting, reservation, or explicitly requested deadline. An allowed calendar action, a generic message, an isolated date or time, or a source timestamp is never evidence by itself. When the match is missing or ambiguous, return no action.",
					"Do not use an enclosing source item's message, creation, or synchronization timestamp as the calendar start unless the source explicitly identifies that timestamp as the appointment time.",
					"If the appointment start cannot be derived from source_data, return no action. optional_user_instructions may add filters or explicit business rules but may be empty.",
					"When all_day is false, start and any explicit end must be local date-times formatted YYYY-MM-DDTHH:MM or YYYY-MM-DDTHH:MM:SS without a UTC offset.",
					"When all_day is true, start and any explicit end must be dates formatted YYYY-MM-DD, and end is exclusive.",
					"If the source explicitly provides a time zone, timezone must be its IANA name. Otherwise omit timezone and WebHome uses base_context.browser_timezone.",
					"If the source explicitly provides an end or duration, derive end from it. Otherwise omit end and WebHome uses one hour for a timed event or one day for an all-day event.",
					"End must be strictly later than start.",
					"For a non-recurring event, omit all recurrence arguments. For a recurring event, provide recurrence_frequency, use recurrence_interval >= 1, and provide at most one of recurrence_until (YYYY-MM-DD) and recurrence_count (integer >= 1).",
				],
				forcedConfirmation: None,
			}],
		};
	}

	fn ai_grant(&self) -> AiModuleGrant
	{
		return self.config.get_untracked().aiGrant;
	}

	fn ai_action_apply(&self,action: AiValidatedAction) -> Option<AiActionFuture>
	{
		let config = self.config.get_untracked();
		let (collection,input) = Self::aiCreateInput_get(&config,&action)?;
		let uid = Self::aiUid_get(&action.actionKey);
		return Some(Box::pin(async move {
			#[cfg(not(feature = "hydrate"))]
			{
				let _ = (config,collection,input,uid);
				return AiActionApplyResult::Rejected;
			}
			#[cfg(feature = "hydrate")]
			{
				let client = match CalDavClient::new(&config)
				{
					Ok(client) => client,
					Err(_) => return AiActionApplyResult::Rejected,
				};
				return match client.event_createIdempotent(&collection,&input,&uid).await
				{
					Ok(()) => AiActionApplyResult::Applied,
					Err(CalDavError::Transport) => AiActionApplyResult::Ambiguous,
					Err(_) => AiActionApplyResult::Rejected,
				};
			}
		}));
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

	fn draw_config(&self,moduleActions: ModuleActionFn,_moduleId: ModuleID) -> Option<ModuleConfigViewFn>
	{
		let config = self.config.clone();
		let runtime = self.runtime.clone();
		let update = self._update.clone();
		return Some(Arc::new(move |session| view! {
			<CalendarConfigDraw
				config=config.clone()
				runtime=runtime.clone()
				update=update.clone()
				moduleActions=moduleActions.clone()
				session
			/>
		}.into_any()));
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
				let floatingTimezone = browser::timezone_get();
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

#[cfg(test)]
mod tests
{
	use super::*;
	use crate::front::ai::automation::AiConfirmationPolicy;

	const COLLECTION_HREF: &str = "https://calendar.invalid/user/personal/";

	fn config_get() -> CalendarConfig
	{
		return CalendarConfig {
			serverUrl: "https://calendar.invalid/user/".to_string(),
			username: "test".to_string(),
			password: "secret".to_string(),
			collections: vec![CalendarCollection {
				href: COLLECTION_HREF.to_string(),
				name: "Personal".to_string(),
				color: None,
			}],
			aiGrant: AiModuleGrant {
				events: Vec::new(),
				actions: vec![CALENDAR_AI_ACTION_CREATE.to_string()],
			},
			..Default::default()
		};
	}

	fn textValue_get(id: &str,value: &str) -> AiNamedValue
	{
		return AiNamedValue {id: id.to_string(),value: AiValue::Text(value.to_string())};
	}

	fn action_get() -> AiValidatedAction
	{
		return AiValidatedAction {
			actionKey: "action-key".to_string(),
			executionId: "execution-id".to_string(),
			targetModuleId: ModuleID {id: "calendar-module".to_string()},
			action: CALENDAR_AI_ACTION_CREATE.to_string(),
			arguments: vec![
				textValue_get(CALENDAR_AI_ARGUMENT_COLLECTION,COLLECTION_HREF),
				textValue_get(CALENDAR_AI_ARGUMENT_TITLE,"Dentist"),
				textValue_get(CALENDAR_AI_ARGUMENT_START,"2026-08-24T10:30"),
				textValue_get(CALENDAR_AI_ARGUMENT_END,"2026-08-24T11:00"),
				AiNamedValue {id: CALENDAR_AI_ARGUMENT_ALL_DAY.to_string(),value: AiValue::Boolean(false)},
				textValue_get(CALENDAR_AI_ARGUMENT_TIMEZONE,"Europe/Paris"),
			],
			confirmation: AiConfirmationPolicy::Confirm,
		};
	}

	fn textArgument_set(action: &mut AiValidatedAction,id: &str,value: &str)
	{
		let argument = action.arguments.iter_mut().find(|argument| argument.id == id).unwrap();
		argument.value = AiValue::Text(value.to_string());
	}

	#[test]
	fn aiCalendarCollectionIsFixedByTheAutomationContext()
	{
		let calendar = Calendar::default();
		calendar.config.update(|config| *config = config_get());
		let action = calendar.ai_capabilities().actions.into_iter().next().unwrap();
		let collection = action.arguments.iter()
			.find(|argument| argument.id == CALENDAR_AI_ARGUMENT_COLLECTION).unwrap();
		let end = action.arguments.iter()
			.find(|argument| argument.id == CALENDAR_AI_ARGUMENT_END).unwrap();
		let timezone = action.arguments.iter()
			.find(|argument| argument.id == CALENDAR_AI_ARGUMENT_TIMEZONE).unwrap();

		assert!(collection.fixedByContext);
		assert_eq!(collection.allowedTextValues.len(),1);
		assert_eq!(collection.allowedTextValues[0].value,COLLECTION_HREF);
		assert!(!end.required);
		assert!(!timezone.required);
		assert!(action.promptRules.iter().any(|rule| rule.contains("An allowed calendar action")));
		assert!(action.promptRules.iter().any(|rule| rule.contains("missing or ambiguous")));
		assert!(action.promptRules.iter().any(|rule| rule.contains("message, creation, or synchronization timestamp")));
		assert!(action.promptRules.iter().any(|rule| rule.contains("return no action")));
		assert!(action.promptRules.iter().any(|rule| rule.contains("base_context.browser_timezone")));
		assert!(action.promptRules.iter().any(|rule| rule.contains("one hour")));
	}

	#[test]
	fn aiCalendarActionBuildsValidatedTimedEvent()
	{
		let mut action = action_get();
		action.arguments.push(textValue_get(CALENDAR_AI_ARGUMENT_LOCATION,"Room 12"));
		let (collection,input) = Calendar::aiCreateInput_get(&config_get(),&action).unwrap();

		assert_eq!(collection.href,COLLECTION_HREF);
		assert_eq!(input.title,"Dentist");
		assert_eq!(input.timezone,"Europe/Paris");
		assert_eq!(input.location,"Room 12");
		assert!(matches!(input.start,CalendarCreateMoment::Local(_)));
		assert!(matches!(input.end,CalendarCreateMoment::Local(_)));
		assert!(input.recurrence.is_none());
	}

	#[test]
	fn aiCalendarActionUsesBrowserDefaultsWhenEndAndTimezoneAreOmitted()
	{
		let mut action = action_get();
		action.arguments.retain(|argument| {
			return !matches!(argument.id.as_str(),CALENDAR_AI_ARGUMENT_END | CALENDAR_AI_ARGUMENT_TIMEZONE);
		});
		let (_,input) = Calendar::aiCreateInput_get(&config_get(),&action).unwrap();

		assert_eq!(input.timezone,"UTC");
		assert_eq!(
			input.start,
			CalendarCreateMoment::Local(Calendar::aiDateTime_parse("2026-08-24T10:30").unwrap()),
		);
		assert_eq!(
			input.end,
			CalendarCreateMoment::Local(Calendar::aiDateTime_parse("2026-08-24T11:30").unwrap()),
		);
	}

	#[test]
	fn aiCalendarActionUsesOneDayForAnAllDayEventWithoutEnd()
	{
		let mut action = action_get();
		textArgument_set(&mut action,CALENDAR_AI_ARGUMENT_START,"2026-08-24");
		action.arguments.retain(|argument| argument.id != CALENDAR_AI_ARGUMENT_END);
		let allDay = action.arguments.iter_mut()
			.find(|argument| argument.id == CALENDAR_AI_ARGUMENT_ALL_DAY).unwrap();
		allDay.value = AiValue::Boolean(true);
		let (_,input) = Calendar::aiCreateInput_get(&config_get(),&action).unwrap();

		assert_eq!(input.start,CalendarCreateMoment::AllDay(Calendar::aiDate_parse("2026-08-24").unwrap()));
		assert_eq!(input.end,CalendarCreateMoment::AllDay(Calendar::aiDate_parse("2026-08-25").unwrap()));
	}

	#[test]
	fn aiCalendarActionRejectsUnknownCollectionAndInvalidPeriod()
	{
		let mut action = action_get();
		textArgument_set(&mut action,CALENDAR_AI_ARGUMENT_COLLECTION,"https://calendar.invalid/user/other/");
		assert!(Calendar::aiCreateInput_get(&config_get(),&action).is_none());

		let mut action = action_get();
		textArgument_set(&mut action,CALENDAR_AI_ARGUMENT_END,"2026-08-24T09:00");
		assert!(Calendar::aiCreateInput_get(&config_get(),&action).is_none());

		let mut action = action_get();
		textArgument_set(&mut action,CALENDAR_AI_ARGUMENT_START,"2026-08-24T10:30:00Z");
		assert!(Calendar::aiCreateInput_get(&config_get(),&action).is_none());
	}

	#[test]
	fn aiCalendarActionRequiresCoherentRecurrence()
	{
		let mut action = action_get();
		action.arguments.extend([
			textValue_get(CALENDAR_AI_ARGUMENT_RECURRENCE_FREQUENCY,"weekly"),
			AiNamedValue {id: CALENDAR_AI_ARGUMENT_RECURRENCE_INTERVAL.to_string(),value: AiValue::Integer(2)},
			AiNamedValue {id: CALENDAR_AI_ARGUMENT_RECURRENCE_COUNT.to_string(),value: AiValue::Integer(4)},
		]);
		let (_,input) = Calendar::aiCreateInput_get(&config_get(),&action).unwrap();
		assert!(matches!(input.recurrence,Some(CalendarRecurrence {
			frequency: CalendarRecurrenceFrequency::Weekly,
			interval: 2,
			end: CalendarRecurrenceEnd::Count(4),
		})));

		action.arguments.push(textValue_get(CALENDAR_AI_ARGUMENT_RECURRENCE_UNTIL,"2026-09-30"));
		assert!(Calendar::aiCreateInput_get(&config_get(),&action).is_none());
	}

	#[test]
	fn aiCalendarActionRequiresValidReadyConfigurationAndPermission()
	{
		let mut config = config_get();
		config.aiGrant.actions.clear();
		assert!(Calendar::aiCreateInput_get(&config,&action_get()).is_none());

		let mut config = config_get();
		config.password.clear();
		assert!(Calendar::aiCreateInput_get(&config,&action_get()).is_none());
	}

	#[test]
	fn aiCalendarUidIsStableAndUrlPathSafe()
	{
		let uid = Calendar::aiUid_get("ab+/cd==");

		assert_eq!(uid,"webhome-ai-ab-_cd");
		assert!(uid.bytes().all(|value| value.is_ascii_alphanumeric() || matches!(value,b'-' | b'_')));
	}

	#[test]
	fn aiCalendarCollectionLabelIsTrimmedAndFallsBackForControlCharacters()
	{
		let mut collection = config_get().collections.remove(0);
		collection.name = "  Personal  ".to_string();
		assert_eq!(Calendar::aiCollectionLabel_get(&collection),"Personal");

		collection.name = "Injected\nrule".to_string();
		assert_eq!(Calendar::aiCollectionLabel_get(&collection),COLLECTION_HREF);
	}
}
