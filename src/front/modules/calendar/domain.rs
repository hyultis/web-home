#![cfg_attr(not(any(feature = "hydrate",test)),allow(dead_code))]

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use time::{Date, Duration, Month, PrimitiveDateTime, Time};
use url::Url;
use crate::front::ai::automation::AiModuleGrant;

pub(super) const CALENDAR_CONFIG_VERSION: u8 = 1;
pub(super) const CALENDAR_MAX_COLLECTIONS: usize = 16;
pub(super) const CALENDAR_MAX_COLLECTION_NAME_BYTES: usize = 1_024;
pub(super) const CALENDAR_MAX_REJECTED_SAMPLES: usize = 16;
pub(super) const CALENDAR_MAX_PASSWORD_BYTES: usize = 4_096;
pub(super) const CALENDAR_MAX_RECURRENCE_COUNT: u16 = 1_000;
pub(super) const CALENDAR_MAX_RECURRENCE_INTERVAL: u16 = 365;
pub(super) const CALENDAR_MAX_URL_BYTES: usize = 4_096;
pub(super) const CALENDAR_MAX_USERNAME_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum CalendarViewMode
{
	#[default]
	Month,
	Week,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct CalendarCollection
{
	pub href: String,
	#[serde(default)]
	pub name: String,
	#[serde(default)]
	pub color: Option<String>,
}

impl CalendarCollection
{
	pub fn color_normalize(color: Option<String>) -> Option<String>
	{
		let color = color?.trim().to_string();
		let validLength = matches!(color.len(),4 | 7 | 9);
		if (validLength && color.starts_with('#') && color[1..].bytes().all(|value| value.is_ascii_hexdigit()))
		{
			return Some(color);
		}
		return None;
	}

	pub fn color_get(&self) -> Option<String>
	{
		Self::color_normalize(self.color.clone())
	}
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct CalendarConfig
{
	#[serde(default = "calendar_config_version")]
	pub version: u8,
	#[serde(default)]
	pub title: String,
	#[serde(default)]
	pub serverUrl: String,
	#[serde(default)]
	pub username: String,
	#[serde(default)]
	pub password: String,
	#[serde(default)]
	pub collections: Vec<CalendarCollection>,
	#[serde(default)]
	pub viewMode: CalendarViewMode,
	#[serde(default)]
	pub highlightWeekends: bool,
	#[serde(default)]
	pub holidayCountry: String,
	#[serde(default)]
	pub aiGrant: AiModuleGrant,
}

fn calendar_config_version() -> u8
{
	CALENDAR_CONFIG_VERSION
}

impl Default for CalendarConfig
{
	fn default() -> Self
	{
		Self {
			version: CALENDAR_CONFIG_VERSION,
			title: String::new(),
			serverUrl: String::new(),
			username: String::new(),
			password: String::new(),
			collections: Vec::new(),
			viewMode: CalendarViewMode::Month,
			highlightWeekends: false,
			holidayCountry: String::new(),
			aiGrant: AiModuleGrant::default(),
		}
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CalendarConfigError
{
	UnsupportedVersion,
	MissingServer,
	InvalidServer,
	MissingUsername,
	MissingPassword,
	NoCollection,
	TooManyCollections,
	InvalidCollection,
	DuplicateCollection,
}

impl CalendarConfig
{
	pub fn holidayCountry_get(&self) -> Option<String>
	{
		let country = self.holidayCountry.trim().to_ascii_uppercase();
		if (country.len() == 2 && country.bytes().all(|value| value.is_ascii_alphabetic()))
		{
			return Some(country);
		}
		return None;
	}

	pub fn connection_validate(&self) -> Result<Url, CalendarConfigError>
	{
		if (self.version != CALENDAR_CONFIG_VERSION)
		{
			return Err(CalendarConfigError::UnsupportedVersion);
		}
		if (self.serverUrl.trim().is_empty())
		{
			return Err(CalendarConfigError::MissingServer);
		}
		if (self.serverUrl.len() > CALENDAR_MAX_URL_BYTES)
		{
			return Err(CalendarConfigError::InvalidServer);
		}
		let serverUrl = Url::parse(self.serverUrl.trim()).map_err(|_| CalendarConfigError::InvalidServer)?;
		if (!matches!(serverUrl.scheme(),"https" | "http")
			|| serverUrl.host_str().is_none()
			|| !serverUrl.username().is_empty()
			|| serverUrl.password().is_some()
			|| serverUrl.query().is_some()
			|| serverUrl.fragment().is_some())
		{
			return Err(CalendarConfigError::InvalidServer);
		}
		if (self.username.is_empty() || self.username.len() > CALENDAR_MAX_USERNAME_BYTES)
		{
			return Err(CalendarConfigError::MissingUsername);
		}
		if (self.password.is_empty() || self.password.len() > CALENDAR_MAX_PASSWORD_BYTES)
		{
			return Err(CalendarConfigError::MissingPassword);
		}
		return Ok(serverUrl);
	}

	pub fn ready_validate(&self) -> Result<Url, CalendarConfigError>
	{
		let serverUrl = self.connection_validate()?;
		if (self.collections.is_empty())
		{
			return Err(CalendarConfigError::NoCollection);
		}
		if (self.collections.len() > CALENDAR_MAX_COLLECTIONS)
		{
			return Err(CalendarConfigError::TooManyCollections);
		}

		let serverOrigin = serverUrl.origin().ascii_serialization();
		let mut collectionUrls = HashSet::new();
		for collection in &self.collections
		{
			if (collection.href.len() > CALENDAR_MAX_URL_BYTES
				|| collection.name.len() > CALENDAR_MAX_COLLECTION_NAME_BYTES)
			{
				return Err(CalendarConfigError::InvalidCollection);
			}
			let collectionUrl = Url::parse(&collection.href).map_err(|_| CalendarConfigError::InvalidCollection)?;
			if (!matches!(collectionUrl.scheme(),"https" | "http")
				|| collectionUrl.origin().ascii_serialization() != serverOrigin
				|| !collectionUrl.username().is_empty()
				|| collectionUrl.password().is_some()
				|| collectionUrl.query().is_some()
				|| collectionUrl.fragment().is_some())
			{
				return Err(CalendarConfigError::InvalidCollection);
			}
			if (!collectionUrls.insert(collectionUrl.to_string()))
			{
				return Err(CalendarConfigError::DuplicateCollection);
			}
		}
		return Ok(serverUrl);
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CalendarRejectedReason
{
	MissingUid,
	MissingStart,
	UnsupportedDateTime,
	InvalidEnd,
	FieldTooLarge,
	UnsupportedEvent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(feature = "hydrate"),allow(dead_code))]
pub(super) enum CalendarHolidayError
{
	InvalidCountry,
	Transport,
	Unavailable,
	ResponseTooLarge,
	TooManyItems,
	InvalidResponse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CalendarRejectedEvent
{
	pub collectionName: String,
	pub title: String,
	pub reason: CalendarRejectedReason,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct CalendarEventIdentity
{
	pub collectionHref: String,
	pub resourceHref: String,
	pub uid: String,
	pub occurrenceId: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CalendarMoment
{
	AllDay(Date),
	Timed(i64),
}

impl CalendarMoment
{
	pub(super) fn sort_key(&self) -> (i64, u8)
	{
		return match self
		{
			Self::AllDay(date) => (PrimitiveDateTime::new(*date,Time::MIDNIGHT).assume_utc().unix_timestamp(),0),
			Self::Timed(timestamp) => (*timestamp,1),
		};
	}

}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CalendarEvent
{
	pub identity: CalendarEventIdentity,
	pub collectionName: String,
	pub collectionColor: Option<String>,
	pub title: String,
	pub description: String,
	pub location: String,
	pub start: CalendarMoment,
	pub end: CalendarMoment,
	pub occurrence: Option<CalendarMoment>,
	pub recurrent: bool,
	pub etag: Option<String>,
}

impl CalendarEvent
{
	pub fn sort_deterministically(events: &mut [Self])
	{
		events.sort_by(|left,right| {
			left.start.sort_key()
				.cmp(&right.start.sort_key())
				.then_with(|| left.end.sort_key().cmp(&right.end.sort_key()))
				.then_with(|| left.identity.cmp(&right.identity))
		});
	}

}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CalendarPeriod
{
	pub start: Date,
	pub endExclusive: Date,
}

impl CalendarPeriod
{
	pub fn from_anchor(anchor: Date, viewMode: CalendarViewMode) -> Self
	{
		return match viewMode
		{
			CalendarViewMode::Week =>
			{
				let start = anchor - Duration::days(anchor.weekday().number_days_from_monday() as i64);
				Self {start,endExclusive: start + Duration::days(7)}
			},
			CalendarViewMode::Month =>
			{
				let monthStart = Date::from_calendar_date(anchor.year(),anchor.month(),1)
					.expect("the first day of an existing month is valid");
				let nextMonth = if (anchor.month() == Month::December)
				{
					Date::from_calendar_date(anchor.year() + 1,Month::January,1)
				}
				else
				{
					Date::from_calendar_date(anchor.year(),anchor.month().next(),1)
				}.expect("the first day of the next month is valid");
				let start = monthStart - Duration::days(monthStart.weekday().number_days_from_monday() as i64);
				let trailingDays = (7 - nextMonth.weekday().number_days_from_monday()) % 7;
				let endExclusive = nextMonth + Duration::days(trailingDays as i64);
				Self {start,endExclusive}
			},
		};
	}

	pub fn query_start_utc(&self) -> String
	{
		format_date_time_utc(self.start - Duration::DAY,Time::MIDNIGHT)
	}

	pub fn query_end_utc(&self) -> String
	{
		format_date_time_utc(self.endExclusive + Duration::DAY,Time::MIDNIGHT)
	}

	pub fn days(&self) -> impl Iterator<Item = Date>
	{
		let start = self.start;
		let count = (self.endExclusive - self.start).whole_days();
		return (0..count).map(move |offset| start + Duration::days(offset));
	}
}

fn format_date_time_utc(date: Date, time: Time) -> String
{
	format!(
		"{:04}{:02}{:02}T{:02}{:02}{:02}Z",
		date.year(),u8::from(date.month()),date.day(),time.hour(),time.minute(),time.second()
	)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CalendarCreateMoment
{
	AllDay(Date),
	Local(PrimitiveDateTime),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CalendarRecurrenceFrequency
{
	Daily,
	Weekly,
	Monthly,
	Yearly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CalendarRecurrenceEnd
{
	Never,
	Until {
		date: Date,
		utcEndTimestamp: i64,
	},
	Count(u16),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CalendarRecurrence
{
	pub frequency: CalendarRecurrenceFrequency,
	pub interval: u16,
	pub end: CalendarRecurrenceEnd,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CalendarCreateInput
{
	pub title: String,
	pub description: String,
	pub location: String,
	pub start: CalendarCreateMoment,
	pub end: CalendarCreateMoment,
	pub timezone: String,
	pub recurrence: Option<CalendarRecurrence>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CalendarEditScope
{
	Event,
	Occurrence,
	Series,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CalendarCreateError
{
	MissingTitle,
	InvalidPeriod,
	InvalidTimezone,
	InvalidRecurrence,
}

impl CalendarCreateInput
{
	pub fn validate(&self) -> Result<(), CalendarCreateError>
	{
		if (self.title.trim().is_empty())
		{
			return Err(CalendarCreateError::MissingTitle);
		}
		match (&self.start,&self.end)
		{
			(CalendarCreateMoment::AllDay(start),CalendarCreateMoment::AllDay(end)) if end > start => {},
			(CalendarCreateMoment::Local(start),CalendarCreateMoment::Local(end)) if end > start =>
			{
				if (!timezone_is_safe(&self.timezone))
				{
					return Err(CalendarCreateError::InvalidTimezone);
				}
			},
			_ => return Err(CalendarCreateError::InvalidPeriod),
		}

		if let Some(recurrence) = &self.recurrence
		{
			if (recurrence.interval == 0 || recurrence.interval > CALENDAR_MAX_RECURRENCE_INTERVAL)
			{
				return Err(CalendarCreateError::InvalidRecurrence);
			}
			match recurrence.end
			{
				CalendarRecurrenceEnd::Count(count) if count == 0 || count > CALENDAR_MAX_RECURRENCE_COUNT =>
					return Err(CalendarCreateError::InvalidRecurrence),
				CalendarRecurrenceEnd::Until {date,..} =>
				{
					let startDate = match self.start
					{
						CalendarCreateMoment::AllDay(date) => date,
						CalendarCreateMoment::Local(dateTime) => dateTime.date(),
					};
					if (date < startDate)
					{
						return Err(CalendarCreateError::InvalidRecurrence);
					}
				},
				_ => {},
			}
		}
		return Ok(());
	}
}

fn timezone_is_safe(timezone: &str) -> bool
{
	if (timezone.is_empty() || timezone.len() > 128 || timezone.starts_with('/') || timezone.ends_with('/'))
	{
		return false;
	}
	return timezone.bytes().all(|value| value.is_ascii_alphanumeric() || matches!(value,b'/' | b'_' | b'-' | b'+'));
}

#[cfg(test)]
mod tests
{
	use super::{
		CalendarCollection, CalendarConfig, CalendarEvent, CalendarEventIdentity, CalendarMoment,
		CalendarPeriod, CalendarViewMode,
	};
	use time::{Date, Month};

	#[test]
	fn legacyConfig_deserializesWithDefaults()
	{
		let config: CalendarConfig = serde_json::from_str("{}").unwrap();

		assert_eq!(config.version,1);
		assert_eq!(config.viewMode,CalendarViewMode::Month);
		assert!(config.collections.is_empty());
		assert!(!config.highlightWeekends);
		assert!(config.holidayCountry.is_empty());
		assert!(config.aiGrant.events.is_empty());
		assert!(config.aiGrant.actions.is_empty());
	}

	#[test]
	fn holidayCountry_requiresIsoAlphaTwoShape()
	{
		let mut config = CalendarConfig {holidayCountry: " fr ".to_string(),..Default::default()};

		assert_eq!(config.holidayCountry_get().as_deref(),Some("FR"));
		config.holidayCountry = "FRA".to_string();
		assert!(config.holidayCountry_get().is_none());
		config.holidayCountry = "F1".to_string();
		assert!(config.holidayCountry_get().is_none());
	}

	#[test]
	fn readyConfig_rejectsCollectionFromAnotherOrigin()
	{
		let config = CalendarConfig {
			serverUrl: "https://calendar.invalid/root/".to_string(),
			username: "test".to_string(),
			password: "secret".to_string(),
			collections: vec![CalendarCollection {
				href: "https://other.invalid/test/".to_string(),
				name: "Test".to_string(),
				color: None,
			}],
			..Default::default()
		};

		assert!(config.ready_validate().is_err());
	}

	#[test]
	fn connectionConfig_acceptsStructurallyValidHttpForDevelopmentPolicy()
	{
		let config = CalendarConfig {
			serverUrl: "http://192.168.1.20:5232/user/".to_string(),
			username: "test".to_string(),
			password: "secret".to_string(),
			..Default::default()
		};

		assert_eq!(config.connection_validate().unwrap().scheme(),"http");
	}

	#[test]
	fn readyConfig_rejectsFutureVersionAndDuplicateCollections()
	{
		let collection = CalendarCollection {
			href: "https://calendar.invalid/test/".to_string(),
			name: "Test".to_string(),
			color: None,
		};
		let mut config = CalendarConfig {
			serverUrl: "https://calendar.invalid/".to_string(),
			username: "test".to_string(),
			password: "secret".to_string(),
			collections: vec![collection.clone(),collection],
			..Default::default()
		};

		assert!(config.ready_validate().is_err());
		config.collections.truncate(1);
		config.version = 2;
		assert!(config.ready_validate().is_err());
	}

	#[test]
	fn monthPeriod_includesCompleteMondayToSundayGrid()
	{
		let anchor = Date::from_calendar_date(2026,Month::August,13).unwrap();
		let period = CalendarPeriod::from_anchor(anchor,CalendarViewMode::Month);

		assert_eq!(period.start,Date::from_calendar_date(2026,Month::July,27).unwrap());
		assert_eq!(period.endExclusive,Date::from_calendar_date(2026,Month::September,7).unwrap());
		assert_eq!(period.days().count(),42);
		assert_eq!(period.query_start_utc(),"20260726T000000Z");
		assert_eq!(period.query_end_utc(),"20260908T000000Z");
	}

	#[test]
	fn weekPeriod_includesEveryDayRegardlessOfCurrentTime()
	{
		let anchor = Date::from_calendar_date(2026,Month::August,13).unwrap();
		let period = CalendarPeriod::from_anchor(anchor,CalendarViewMode::Week);

		assert_eq!(period.start,Date::from_calendar_date(2026,Month::August,10).unwrap());
		assert_eq!(period.endExclusive,Date::from_calendar_date(2026,Month::August,17).unwrap());
		assert_eq!(period.days().count(),7);
		assert_eq!(period.query_start_utc(),"20260809T000000Z");
		assert_eq!(period.query_end_utc(),"20260818T000000Z");
	}

	#[test]
	fn events_sortByStartThenStableIdentity()
	{
		let date = Date::from_calendar_date(2026,Month::August,13).unwrap();
		let event = |uid: &str,start: CalendarMoment| CalendarEvent {
			identity: CalendarEventIdentity {
				collectionHref: "https://calendar.invalid/a/".to_string(),
				resourceHref: format!("https://calendar.invalid/a/{uid}.ics"),
				uid: uid.to_string(),
				occurrenceId: None,
			},
			collectionName: "A".to_string(),
			collectionColor: None,
			title: uid.to_string(),
			description: String::new(),
			location: String::new(),
			start: start.clone(),
			end: start,
			occurrence: None,
			recurrent: false,
			etag: None,
		};
		let timedStart = time::PrimitiveDateTime::new(date,time::Time::MIDNIGHT).assume_utc().unix_timestamp();
		let mut events = vec![event("b",CalendarMoment::Timed(timedStart)),event("a",CalendarMoment::Timed(timedStart)),event("all",CalendarMoment::AllDay(date))];

		CalendarEvent::sort_deterministically(&mut events);

		assert_eq!(events.iter().map(|event| event.title.as_str()).collect::<Vec<_>>(),["all","a","b"]);
	}
}
