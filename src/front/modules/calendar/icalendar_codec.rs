use icalendar::{Calendar, CalendarDateTime, Component, DatePerhapsTime, Event, EventLike, Property};
use time::{Date, OffsetDateTime, PrimitiveDateTime, UtcOffset};

use super::domain::{
	CalendarCreateInput, CalendarCreateMoment, CalendarEditScope, CalendarEvent, CalendarEventIdentity, CalendarMoment,
	CalendarRecurrenceEnd, CalendarRecurrenceFrequency, CalendarRejectedEvent,
	CalendarRejectedReason, CALENDAR_MAX_REJECTED_SAMPLES,
};

const CALENDAR_MAX_ICALENDAR_BYTES: usize = 2 * 1024 * 1024;
const CALENDAR_MAX_EVENTS_PER_RESOURCE: usize = 2_048;
const CALENDAR_MAX_UID_BYTES: usize = 512;
const CALENDAR_MAX_TITLE_BYTES: usize = 4_096;
const CALENDAR_MAX_LOCATION_BYTES: usize = 8_192;
const CALENDAR_MAX_DESCRIPTION_BYTES: usize = 128 * 1024;

#[derive(Clone)]
pub(super) struct CalendarResourceSource
{
	pub collectionHref: String,
	pub collectionName: String,
	pub collectionColor: Option<String>,
	pub resourceHref: String,
	pub etag: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CalendarParsedEvents
{
	pub events: Vec<CalendarEvent>,
	pub rejectedCount: usize,
	pub rejectedSamples: Vec<CalendarRejectedEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CalendarCodecError
{
	InputTooLarge,
	InvalidCalendar,
	TooManyEvents,
	MissingUid,
	MissingStart,
	UnsupportedDateTime,
	InvalidEnd,
	FieldTooLarge,
	InvalidCreateInput,
	MasterEventNotFound,
	EventIsNotRecurring,
}

pub(super) struct CalendarBuiltEvent
{
	pub uid: String,
	pub content: String,
}

pub(super) fn parse_expanded_events(
	input: &str,
	source: &CalendarResourceSource,
	floatingTimezone: &str,
) -> Result<CalendarParsedEvents,CalendarCodecError>
{
	if (input.len() > CALENDAR_MAX_ICALENDAR_BYTES)
	{
		return Err(CalendarCodecError::InputTooLarge);
	}
	let calendar: Calendar = input.parse().map_err(|_| CalendarCodecError::InvalidCalendar)?;
	let mut events = Vec::new();
	let mut rejectedCount = 0;
	let mut rejectedSamples = Vec::new();
	for (index,event) in calendar.events().enumerate()
	{
		if (index >= CALENDAR_MAX_EVENTS_PER_RESOURCE)
		{
			return Err(CalendarCodecError::TooManyEvents);
		}
		match event_parse(event,source,floatingTimezone)
		{
			Ok(event) => events.push(event),
			Err(error) =>
			{
				rejectedCount += 1;
				if (rejectedSamples.len() < CALENDAR_MAX_REJECTED_SAMPLES)
				{
					rejectedSamples.push(CalendarRejectedEvent {
						collectionName: source.collectionName.clone(),
						title: diagnostic_title_get(event),
						reason: CalendarRejectedReason::from(error),
					});
				}
			},
		}
	}
	CalendarEvent::sort_deterministically(&mut events);
	return Ok(CalendarParsedEvents {events,rejectedCount,rejectedSamples});
}

impl From<CalendarCodecError> for CalendarRejectedReason
{
	fn from(error: CalendarCodecError) -> Self
	{
		return match error
		{
			CalendarCodecError::MissingUid => Self::MissingUid,
			CalendarCodecError::MissingStart => Self::MissingStart,
			CalendarCodecError::UnsupportedDateTime => Self::UnsupportedDateTime,
			CalendarCodecError::InvalidEnd => Self::InvalidEnd,
			CalendarCodecError::FieldTooLarge => Self::FieldTooLarge,
			_ => Self::UnsupportedEvent,
		};
	}
}

fn diagnostic_title_get(event: &Event) -> String
{
	return event.get_summary().unwrap_or_default().chars().take(160).collect();
}

fn event_parse(
	event: &Event,
	source: &CalendarResourceSource,
	floatingTimezone: &str,
) -> Result<CalendarEvent,CalendarCodecError>
{
	let uid = bounded_field(event.get_uid().ok_or(CalendarCodecError::MissingUid)?,CALENDAR_MAX_UID_BYTES)?;
	let start = moment_from_icalendar(event.get_start().ok_or(CalendarCodecError::MissingStart)?,floatingTimezone)?;
	let end = match event.get_end()
	{
		Some(end) => moment_from_icalendar(end,floatingTimezone)?,
		None => match start
		{
			CalendarMoment::AllDay(date) => CalendarMoment::AllDay(date.next_day().ok_or(CalendarCodecError::InvalidEnd)?),
			CalendarMoment::Timed(timestamp) => CalendarMoment::Timed(timestamp),
		},
	};
	if (!moments_are_compatible(&start,&end) || end.sort_key() < start.sort_key())
	{
		return Err(CalendarCodecError::InvalidEnd);
	}
	let occurrenceProperty = event.properties().get("RECURRENCE-ID");
	let occurrenceId = occurrenceProperty.map(property_identity);
	let occurrence = occurrenceProperty
		.map(|property| DatePerhapsTime::from_property(property).ok_or(CalendarCodecError::UnsupportedDateTime))
		.transpose()?
		.map(|moment| moment_from_icalendar(moment,floatingTimezone))
		.transpose()?;
	let title = bounded_field(event.get_summary().unwrap_or_default(),CALENDAR_MAX_TITLE_BYTES)?;
	let description = bounded_field(event.get_description().unwrap_or_default(),CALENDAR_MAX_DESCRIPTION_BYTES)?;
	let location = bounded_field(event.get_location().unwrap_or_default(),CALENDAR_MAX_LOCATION_BYTES)?;

	return Ok(CalendarEvent {
		identity: CalendarEventIdentity {
			collectionHref: source.collectionHref.clone(),
			resourceHref: source.resourceHref.clone(),
			uid,
			occurrenceId: occurrenceId.clone(),
		},
		collectionName: source.collectionName.clone(),
		collectionColor: source.collectionColor.clone(),
		title,
		description,
		location,
		start,
		end,
		occurrence,
		recurrent: occurrenceId.is_some() || event.property_value("RRULE").is_some(),
		etag: source.etag.clone(),
	});
}

fn bounded_field(value: &str, maxBytes: usize) -> Result<String,CalendarCodecError>
{
	if (value.len() > maxBytes)
	{
		return Err(CalendarCodecError::FieldTooLarge);
	}
	return Ok(value.to_string());
}

fn property_identity(property: &Property) -> String
{
	let mut identity = property.value().to_string();
	for (name,value) in property.params()
	{
		identity.push('|');
		identity.push_str(name);
		identity.push('=');
		identity.push_str(value.value());
	}
	return identity;
}

fn moments_are_compatible(start: &CalendarMoment, end: &CalendarMoment) -> bool
{
	matches!((start,end),(CalendarMoment::AllDay(_),CalendarMoment::AllDay(_)) | (CalendarMoment::Timed(_),CalendarMoment::Timed(_)))
}

fn moment_from_icalendar(
	moment: DatePerhapsTime,
	floatingTimezone: &str,
) -> Result<CalendarMoment,CalendarCodecError>
{
	return match moment
	{
		DatePerhapsTime::Date(date) => chrono_date_into_time(date.to_string()).map(CalendarMoment::AllDay),
		DatePerhapsTime::DateTime(CalendarDateTime::Utc(dateTime)) => Ok(CalendarMoment::Timed(dateTime.timestamp())),
		DatePerhapsTime::DateTime(CalendarDateTime::Floating(dateTime)) =>
		{
			CalendarDateTime::WithTimezone {date_time: dateTime,tzid: floatingTimezone.to_string()}
				.try_into_utc()
				.map(|dateTime| CalendarMoment::Timed(dateTime.timestamp()))
				.ok_or(CalendarCodecError::UnsupportedDateTime)
		},
		DatePerhapsTime::DateTime(dateTime @ CalendarDateTime::WithTimezone {..}) => dateTime
			.try_into_utc()
			.map(|dateTime| CalendarMoment::Timed(dateTime.timestamp()))
			.ok_or(CalendarCodecError::UnsupportedDateTime),
	};
}

fn chrono_date_into_time(value: String) -> Result<Date,CalendarCodecError>
{
	let mut parts = value.split('-');
	let year = parts.next().and_then(|part| part.parse::<i32>().ok()).ok_or(CalendarCodecError::UnsupportedDateTime)?;
	let month = parts.next().and_then(|part| part.parse::<u8>().ok()).and_then(|month| month.try_into().ok()).ok_or(CalendarCodecError::UnsupportedDateTime)?;
	let day = parts.next().and_then(|part| part.parse::<u8>().ok()).ok_or(CalendarCodecError::UnsupportedDateTime)?;
	if (parts.next().is_some())
	{
		return Err(CalendarCodecError::UnsupportedDateTime);
	}
	return Date::from_calendar_date(year,month,day).map_err(|_| CalendarCodecError::UnsupportedDateTime);
}

pub(super) fn build_event(
	input: &CalendarCreateInput,
	uid: String,
	now: OffsetDateTime,
) -> Result<CalendarBuiltEvent,CalendarCodecError>
{
	input.validate().map_err(|_| CalendarCodecError::InvalidCreateInput)?;
	if (uid.is_empty() || uid.len() > CALENDAR_MAX_UID_BYTES
		|| input.title.len() > CALENDAR_MAX_TITLE_BYTES
		|| input.location.len() > CALENDAR_MAX_LOCATION_BYTES
		|| input.description.len() > CALENDAR_MAX_DESCRIPTION_BYTES)
	{
		return Err(CalendarCodecError::FieldTooLarge);
	}

	let mut event = Event::with_uid(&uid);
	event.summary(input.title.trim());
	if (!input.description.is_empty())
	{
		event.description(&input.description);
	}
	if (!input.location.is_empty())
	{
		event.location(&input.location);
	}
	event.append_property(Property::new("DTSTAMP",format_utc(now)));
	event.sequence(0);
	match (&input.start,&input.end)
	{
		(CalendarCreateMoment::AllDay(start),CalendarCreateMoment::AllDay(end)) =>
		{
			event.starts(*start).ends(*end);
		},
		(CalendarCreateMoment::Local(start),CalendarCreateMoment::Local(end)) =>
		{
			event.append_property(local_property("DTSTART",*start,&input.timezone));
			event.append_property(local_property("DTEND",*end,&input.timezone));
		},
		_ => return Err(CalendarCodecError::InvalidCreateInput),
	}
	if let Some(recurrence) = &input.recurrence
	{
		event.append_property(Property::new("RRULE",recurrence_format(recurrence,&input.start)?));
	}

	let mut calendar = Calendar::new();
	calendar.properties.retain(|property| property.key() != "PRODID");
	calendar.append_property(Property::new("PRODID","-//WebHome//Calendar//EN"));
	if (matches!(input.start,CalendarCreateMoment::Local(_)))
	{
		calendar.timezone(input.timezone.clone());
	}
	calendar.push(event);
	let content = calendar.to_string();
	if (content.len() > CALENDAR_MAX_ICALENDAR_BYTES)
	{
		return Err(CalendarCodecError::InputTooLarge);
	}
	return Ok(CalendarBuiltEvent {uid,content});
}

pub(super) fn update_event(
	input: &str,
	uid: &str,
	occurrenceId: Option<&str>,
	occurrence: Option<&CalendarMoment>,
	editScope: CalendarEditScope,
	update: &CalendarCreateInput,
	now: OffsetDateTime,
) -> Result<String,CalendarCodecError>
{
	if (input.len() > CALENDAR_MAX_ICALENDAR_BYTES)
	{
		return Err(CalendarCodecError::InputTooLarge);
	}
	update_fields_validate(update,uid)?;
	let mut calendar: Calendar = input.parse().map_err(|_| CalendarCodecError::InvalidCalendar)?;
	match editScope
	{
		CalendarEditScope::Event | CalendarEditScope::Series =>
		{
			let event = calendar.events_mut()
				.find(|event| event.get_uid() == Some(uid) && event.get_recurrence_id().is_none())
				.ok_or(CalendarCodecError::MasterEventNotFound)?;
			event_fields_update(event,update,now)?;
		},
		CalendarEditScope::Occurrence =>
		{
			let occurrence = occurrence.ok_or(CalendarCodecError::UnsupportedDateTime)?;
			let masterIsRecurring = calendar.events()
				.any(|event| event.get_uid() == Some(uid)
					&& event.get_recurrence_id().is_none()
					&& event.property_value("RRULE").is_some());
			if (!masterIsRecurring)
			{
				return Err(CalendarCodecError::EventIsNotRecurring);
			}
			let existing = calendar.events().any(|event| {
				event_occurrence_matches(event,uid,occurrenceId,occurrence,&update.timezone)
			});
			if (existing)
			{
				let event = calendar.events_mut()
					.find(|event| event_occurrence_matches(event,uid,occurrenceId,occurrence,&update.timezone))
					.ok_or(CalendarCodecError::InvalidCalendar)?;
				event_fields_update(event,update,now)?;
			}
			else
			{
				let mut event = Event::with_uid(uid);
				event.append_property(moment_property("RECURRENCE-ID",occurrence)?);
				event_fields_update(&mut event,update,now)?;
				calendar.push(event);
			}
		},
	}
	if (matches!(update.start,CalendarCreateMoment::Local(_)))
	{
		calendar.properties.retain(|property| property.key() != "X-WR-TIMEZONE");
		calendar.timezone(update.timezone.clone());
	}
	let content = calendar.to_string();
	if (content.len() > CALENDAR_MAX_ICALENDAR_BYTES)
	{
		return Err(CalendarCodecError::InputTooLarge);
	}
	return Ok(content);
}

fn update_fields_validate(input: &CalendarCreateInput,uid: &str) -> Result<(),CalendarCodecError>
{
	input.validate().map_err(|_| CalendarCodecError::InvalidCreateInput)?;
	if (uid.is_empty() || uid.len() > CALENDAR_MAX_UID_BYTES
		|| input.title.len() > CALENDAR_MAX_TITLE_BYTES
		|| input.location.len() > CALENDAR_MAX_LOCATION_BYTES
		|| input.description.len() > CALENDAR_MAX_DESCRIPTION_BYTES)
	{
		return Err(CalendarCodecError::FieldTooLarge);
	}
	return Ok(());
}

fn event_fields_update(
	event: &mut Event,
	input: &CalendarCreateInput,
	now: OffsetDateTime,
) -> Result<(),CalendarCodecError>
{
	event.summary(input.title.trim());
	if (input.description.is_empty())
	{
		event.remove_property("DESCRIPTION");
	}
	else
	{
		event.description(&input.description);
	}
	if (input.location.is_empty())
	{
		event.remove_property("LOCATION");
	}
	else
	{
		event.location(&input.location);
	}
	event.remove_property("DTSTART");
	event.remove_property("DTEND");
	event.remove_property("DURATION");
	match (&input.start,&input.end)
	{
		(CalendarCreateMoment::AllDay(start),CalendarCreateMoment::AllDay(end)) =>
		{
			event.starts(*start).ends(*end);
		},
		(CalendarCreateMoment::Local(start),CalendarCreateMoment::Local(end)) =>
		{
			event.append_property(local_property("DTSTART",*start,&input.timezone));
			event.append_property(local_property("DTEND",*end,&input.timezone));
		},
		_ => return Err(CalendarCodecError::InvalidCreateInput),
	}
	let sequence = event.get_sequence().unwrap_or(0).saturating_add(1);
	event.sequence(sequence);
	event.append_property(Property::new("DTSTAMP",format_utc(now)));
	return Ok(());
}

fn event_occurrence_matches(
	event: &Event,
	uid: &str,
	occurrenceId: Option<&str>,
	occurrence: &CalendarMoment,
	floatingTimezone: &str,
) -> bool
{
	if (event.get_uid() != Some(uid)) {return false;}
	let Some(property) = event.properties().get("RECURRENCE-ID") else {return false};
	if (occurrenceId.is_some_and(|occurrenceId| property_identity(property) == occurrenceId))
	{
		return true;
	}
	return DatePerhapsTime::from_property(property)
		.and_then(|moment| moment_from_icalendar(moment,floatingTimezone).ok())
		.as_ref() == Some(occurrence);
}

fn recurrence_format(
	recurrence: &super::domain::CalendarRecurrence,
	start: &CalendarCreateMoment,
) -> Result<String,CalendarCodecError>
{
	let frequency = match recurrence.frequency
	{
		CalendarRecurrenceFrequency::Daily => "DAILY",
		CalendarRecurrenceFrequency::Weekly => "WEEKLY",
		CalendarRecurrenceFrequency::Monthly => "MONTHLY",
		CalendarRecurrenceFrequency::Yearly => "YEARLY",
	};
	let mut value = format!("FREQ={frequency};INTERVAL={}",recurrence.interval);
	match recurrence.end
	{
		CalendarRecurrenceEnd::Never => {},
		CalendarRecurrenceEnd::Count(count) => value.push_str(&format!(";COUNT={count}")),
		CalendarRecurrenceEnd::Until {date,utcEndTimestamp} =>
		{
			match start
			{
				CalendarCreateMoment::AllDay(_) => value.push_str(&format!(";UNTIL={}",format_date(date))),
				CalendarCreateMoment::Local(_) =>
				{
					let until = OffsetDateTime::from_unix_timestamp(utcEndTimestamp)
						.map_err(|_| CalendarCodecError::InvalidCreateInput)?;
					value.push_str(&format!(";UNTIL={}",format_utc(until)));
				},
			}
		},
	}
	return Ok(value);
}

fn local_property(name: &str, dateTime: PrimitiveDateTime, timezone: &str) -> Property
{
	Property::new(name,format_local(dateTime)).add_parameter("TZID",timezone).done()
}

pub(super) fn exclude_occurrence(
	input: &str,
	uid: &str,
	occurrenceId: Option<&str>,
	occurrence: &CalendarMoment,
	now: OffsetDateTime,
) -> Result<String,CalendarCodecError>
{
	if (input.len() > CALENDAR_MAX_ICALENDAR_BYTES)
	{
		return Err(CalendarCodecError::InputTooLarge);
	}
	let mut calendar: Calendar = input.parse().map_err(|_| CalendarCodecError::InvalidCalendar)?;
	let event = calendar.events_mut()
		.find(|event| event.get_uid() == Some(uid) && event.get_recurrence_id().is_none())
		.ok_or(CalendarCodecError::MasterEventNotFound)?;
	if (event.property_value("RRULE").is_none())
	{
		return Err(CalendarCodecError::EventIsNotRecurring);
	}
	let exclusion = occurrence_property(occurrence)?;
	let alreadyExcluded = event.multi_properties().get("EXDATE")
		.map(|values| values.iter().any(|value| value.value() == exclusion.value() && value.params() == exclusion.params()))
		.unwrap_or(false);
	if (!alreadyExcluded)
	{
		event.append_multi_property(exclusion);
	}
	let sequence = event.get_sequence().unwrap_or(0).saturating_add(1);
	event.sequence(sequence);
	event.append_property(Property::new("DTSTAMP",format_utc(now)));
	calendar.components.retain(|component| {
		let Some(event) = component.as_event() else {return true};
		return !event_occurrence_matches(event,uid,occurrenceId,occurrence,"UTC");
	});
	let content = calendar.to_string();
	if (content.len() > CALENDAR_MAX_ICALENDAR_BYTES)
	{
		return Err(CalendarCodecError::InputTooLarge);
	}
	return Ok(content);
}

#[cfg(test)]
pub(super) fn event_is_recurring(input: &str, uid: &str) -> Result<bool,CalendarCodecError>
{
	if (input.len() > CALENDAR_MAX_ICALENDAR_BYTES)
	{
		return Err(CalendarCodecError::InputTooLarge);
	}
	if (uid.is_empty() || uid.len() > CALENDAR_MAX_UID_BYTES)
	{
		return Err(CalendarCodecError::MissingUid);
	}
	let calendar: Calendar = input.parse().map_err(|_| CalendarCodecError::InvalidCalendar)?;
	let event = calendar.events()
		.find(|event| event.get_uid() == Some(uid) && event.get_recurrence_id().is_none())
		.ok_or(CalendarCodecError::MasterEventNotFound)?;
	return Ok(event.property_value("RRULE").is_some());
}

fn occurrence_property(occurrence: &CalendarMoment) -> Result<Property,CalendarCodecError>
{
	return moment_property("EXDATE",occurrence);
}

fn moment_property(name: &str,moment: &CalendarMoment) -> Result<Property,CalendarCodecError>
{
	return match moment
	{
		CalendarMoment::AllDay(date) => Ok(Property::new(name,format_date(*date)).add_parameter("VALUE","DATE").done()),
		CalendarMoment::Timed(timestamp) =>
		{
			let dateTime = OffsetDateTime::from_unix_timestamp(*timestamp).map_err(|_| CalendarCodecError::UnsupportedDateTime)?;
			Ok(Property::new(name,format_utc(dateTime)))
		},
	};
}

fn format_date(date: Date) -> String
{
	format!("{:04}{:02}{:02}",date.year(),u8::from(date.month()),date.day())
}

fn format_local(dateTime: PrimitiveDateTime) -> String
{
	format!(
		"{}T{:02}{:02}{:02}",
		format_date(dateTime.date()),dateTime.hour(),dateTime.minute(),dateTime.second()
	)
}

fn format_utc(dateTime: OffsetDateTime) -> String
{
	let dateTime = dateTime.to_offset(UtcOffset::UTC);
	format!(
		"{}T{:02}{:02}{:02}Z",
		format_date(dateTime.date()),dateTime.hour(),dateTime.minute(),dateTime.second()
	)
}

#[cfg(test)]
mod tests
{
	use super::{
		CalendarResourceSource, build_event, event_is_recurring, exclude_occurrence, parse_expanded_events,
		update_event,
	};
	use crate::front::modules::calendar::domain::{
		CalendarCreateInput, CalendarCreateMoment, CalendarEditScope, CalendarMoment, CalendarRecurrence,
		CalendarRecurrenceEnd, CalendarRecurrenceFrequency, CalendarRejectedReason,
	};
	use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

	fn source() -> CalendarResourceSource
	{
		CalendarResourceSource {
			collectionHref: "https://calendar.invalid/test/".to_string(),
			collectionName: "Test".to_string(),
			collectionColor: Some("#336699".to_string()),
			resourceHref: "https://calendar.invalid/test/event.ics".to_string(),
			etag: Some("\"test-etag\"".to_string()),
		}
	}

	#[test]
	fn expandedCalendar_parsesAndSortsOccurrences()
	{
		let input = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:series\r\nRECURRENCE-ID:20260814T080000Z\r\nDTSTART:20260814T080000Z\r\nDTEND:20260814T090000Z\r\nSUMMARY:Second\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:single\r\nDTSTART;VALUE=DATE:20260813\r\nDTEND;VALUE=DATE:20260814\r\nSUMMARY:A very long folded\r\n title\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

		let parsed = parse_expanded_events(input,&source(),"UTC").unwrap();

		assert_eq!(parsed.rejectedCount,0);
		assert_eq!(parsed.events.len(),2);
		assert_eq!(parsed.events[0].title,"A very long foldedtitle");
		assert_eq!(parsed.events[1].identity.occurrenceId.as_deref(),Some("20260814T080000Z"));
		assert!(parsed.events[1].recurrent);
	}

	#[test]
	fn expandedCalendar_convertsIanaAndFloatingTimes()
	{
		let input = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:iana\r\nDTSTART;TZID=Europe/Paris:20260813T093000\r\nDTEND;TZID=Europe/Paris:20260813T103000\r\nSUMMARY:IANA\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:floating\r\nDTSTART:20260813T093000\r\nDTEND:20260813T103000\r\nSUMMARY:Floating\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
		let expected = PrimitiveDateTime::new(
			Date::from_calendar_date(2026,Month::August,13).unwrap(),
			Time::from_hms(9,30,0).unwrap(),
		).assume_offset(UtcOffset::from_hms(2,0,0).unwrap()).unix_timestamp();

		let parsed = parse_expanded_events(input,&source(),"Europe/Paris").unwrap();

		assert_eq!(parsed.rejectedCount,0);
		assert_eq!(parsed.events.len(),2);
		assert!(parsed.events.iter().all(|event| event.start == CalendarMoment::Timed(expected)));
	}

	#[test]
	fn expandedCalendar_reportsUnknownTimezoneWithoutContent()
	{
		let input = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:test\r\nDTSTART;TZID=Unknown/Zone:20260813T093000\r\nSUMMARY:Unsupported appointment\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

		let parsed = parse_expanded_events(input,&source(),"UTC").unwrap();

		assert!(parsed.events.is_empty());
		assert_eq!(parsed.rejectedCount,1);
		assert_eq!(parsed.rejectedSamples.len(),1);
		assert_eq!(parsed.rejectedSamples[0].title,"Unsupported appointment");
		assert_eq!(parsed.rejectedSamples[0].reason,CalendarRejectedReason::UnsupportedDateTime);
	}

	#[test]
	fn buildRecurringEvent_writesTimezoneEscapedTextAndRule()
	{
		let startDate = Date::from_calendar_date(2026,Month::August,13).unwrap();
		let input = CalendarCreateInput {
			title: "Planning, équipe".to_string(),
			description: "Line one\nLine two".to_string(),
			location: "Room; 2".to_string(),
			start: CalendarCreateMoment::Local(PrimitiveDateTime::new(startDate,Time::from_hms(9,30,0).unwrap())),
			end: CalendarCreateMoment::Local(PrimitiveDateTime::new(startDate,Time::from_hms(10,30,0).unwrap())),
			timezone: "Europe/Paris".to_string(),
			recurrence: Some(CalendarRecurrence {
				frequency: CalendarRecurrenceFrequency::Weekly,
				interval: 2,
				end: CalendarRecurrenceEnd::Count(5),
			}),
		};

		let built = build_event(&input,"test-uid".to_string(),OffsetDateTime::from_unix_timestamp(1_786_608_000).unwrap()).unwrap();

		assert_eq!(built.uid,"test-uid");
		assert!(built.content.contains("DTSTART;TZID=Europe/Paris:20260813T093000"));
		assert!(built.content.contains("RRULE:FREQ=WEEKLY;INTERVAL=2;COUNT=5"));
		assert!(built.content.contains("SUMMARY:Planning\\, équipe"));
		assert!(built.content.contains("DESCRIPTION:Line one\\nLine two"));
	}

	#[test]
	fn excludeOccurrence_preservesMasterAndIncrementsSequence()
	{
		let input = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\nBEGIN:VEVENT\r\nUID:series\r\nDTSTART:20260813T080000Z\r\nDTEND:20260813T090000Z\r\nRRULE:FREQ=DAILY\r\nSEQUENCE:2\r\nSUMMARY:Daily\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:series\r\nRECURRENCE-ID:20260814T080000Z\r\nDTSTART:20260814T100000Z\r\nDTEND:20260814T110000Z\r\nSUMMARY:Overridden\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

		let updated = exclude_occurrence(input,"series",None,&CalendarMoment::Timed(1_786_694_400),OffsetDateTime::from_unix_timestamp(1_786_608_000).unwrap()).unwrap();

		assert!(updated.contains("EXDATE:20260814T080000Z"));
		assert!(updated.contains("SEQUENCE:3"));
		assert!(updated.contains("SUMMARY:Daily"));
		assert!(!updated.contains("SUMMARY:Overridden"));
	}

	#[test]
	fn masterRecurrence_isDetectedWithoutExpandingIt()
	{
		let recurring = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:series\r\nDTSTART:20260813T080000Z\r\nRRULE:FREQ=DAILY\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
		let simple = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:single\r\nDTSTART:20260813T080000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

		assert_eq!(event_is_recurring(recurring,"series"),Ok(true));
		assert_eq!(event_is_recurring(simple,"single"),Ok(false));
	}

	#[test]
	fn seriesUpdate_preservesRecurrenceAndExclusions()
	{
		let source = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:series\r\nDTSTART:20260813T080000Z\r\nDTEND:20260813T090000Z\r\nRRULE:FREQ=DAILY\r\nEXDATE:20260814T080000Z\r\nSUMMARY:Old\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
		let input = CalendarCreateInput {
			title: "Updated".to_string(),
			description: "Description".to_string(),
			location: "Office".to_string(),
			start: CalendarCreateMoment::Local(PrimitiveDateTime::new(
				Date::from_calendar_date(2026,Month::August,13).unwrap(),Time::from_hms(10,0,0).unwrap(),
			)),
			end: CalendarCreateMoment::Local(PrimitiveDateTime::new(
				Date::from_calendar_date(2026,Month::August,13).unwrap(),Time::from_hms(11,0,0).unwrap(),
			)),
			timezone: "Europe/Paris".to_string(),
			recurrence: None,
		};
		let updated = update_event(
			source,"series",None,None,CalendarEditScope::Series,&input,
			OffsetDateTime::from_unix_timestamp(1_786_608_000).unwrap(),
		).unwrap();

		assert!(updated.contains("UID:series"));
		assert!(updated.contains("RRULE:FREQ=DAILY"));
		assert!(updated.contains("EXDATE:20260814T080000Z"));
		assert!(updated.contains("SUMMARY:Updated"));
		assert!(updated.contains("DTSTART;TZID=Europe/Paris:20260813T100000"));
		assert_eq!(updated.matches("X-WR-TIMEZONE:Europe/Paris").count(),1);
	}

	#[test]
	fn occurrenceUpdate_createsOneExplicitException()
	{
		let source = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:series\r\nDTSTART:20260813T080000Z\r\nDTEND:20260813T090000Z\r\nRRULE:FREQ=DAILY\r\nSUMMARY:Old\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
		let input = CalendarCreateInput {
			title: "Moved occurrence".to_string(),
			description: String::new(),
			location: String::new(),
			start: CalendarCreateMoment::Local(PrimitiveDateTime::new(
				Date::from_calendar_date(2026,Month::August,14).unwrap(),Time::from_hms(11,0,0).unwrap(),
			)),
			end: CalendarCreateMoment::Local(PrimitiveDateTime::new(
				Date::from_calendar_date(2026,Month::August,14).unwrap(),Time::from_hms(12,0,0).unwrap(),
			)),
			timezone: "Europe/Paris".to_string(),
			recurrence: None,
		};
		let updated = update_event(
			source,"series",None,Some(&CalendarMoment::Timed(1_786_694_400)),
			CalendarEditScope::Occurrence,&input,
			OffsetDateTime::from_unix_timestamp(1_786_608_000).unwrap(),
		).unwrap();

		assert_eq!(updated.matches("UID:series").count(),2);
		assert!(updated.contains("RECURRENCE-ID:20260814T080000Z"));
		assert!(updated.contains("SUMMARY:Moved occurrence"));
		assert!(updated.contains("RRULE:FREQ=DAILY"));
	}
}
