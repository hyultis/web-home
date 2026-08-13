#![cfg_attr(not(any(feature = "hydrate",test)),allow(dead_code))]

use serde::Deserialize;
use std::collections::BTreeMap;
use time::{Date,Month};

use super::domain::CalendarHolidayError;
#[cfg(any(feature = "hydrate",test))]
use super::domain::CalendarPeriod;

const HOLIDAY_MAX_RESPONSE_BYTES: usize = 256 * 1024;
const HOLIDAY_MAX_ITEMS_PER_YEAR: usize = 256;
const HOLIDAY_MAX_NAME_BYTES: usize = 512;

#[derive(Clone,Debug,Eq,PartialEq)]
struct CalendarHoliday
{
	date: Date,
	name: String,
}

#[derive(Deserialize)]
struct NagerHoliday
{
	date: String,
	#[serde(rename="localName")]
	localName: String,
	name: String,
	#[serde(rename="countryCode")]
	countryCode: String,
	global: bool,
}

fn holidays_parse(input: &str, country: &str, year: i32) -> Result<Vec<CalendarHoliday>,CalendarHolidayError>
{
	if (input.len() > HOLIDAY_MAX_RESPONSE_BYTES)
	{
		return Err(CalendarHolidayError::ResponseTooLarge);
	}
	let source: Vec<NagerHoliday> = serde_json::from_str(input).map_err(|_| CalendarHolidayError::InvalidResponse)?;
	if (source.len() > HOLIDAY_MAX_ITEMS_PER_YEAR)
	{
		return Err(CalendarHolidayError::TooManyItems);
	}
	let mut holidays = Vec::new();
	for holiday in source
	{
		if (holiday.countryCode != country)
		{
			return Err(CalendarHolidayError::InvalidResponse);
		}
		if (!holiday.global)
		{
			continue;
		}
		let date = date_parse(&holiday.date)?;
		if (date.year() != year)
		{
			return Err(CalendarHolidayError::InvalidResponse);
		}
		let name = if (!holiday.localName.trim().is_empty())
		{
			holiday.localName.trim()
		}
		else
		{
			holiday.name.trim()
		};
		if (name.is_empty() || name.len() > HOLIDAY_MAX_NAME_BYTES)
		{
			return Err(CalendarHolidayError::InvalidResponse);
		}
		holidays.push(CalendarHoliday {date,name: name.to_string()});
	}
	holidays.sort_by(|left,right| left.date.cmp(&right.date).then_with(|| left.name.cmp(&right.name)));
	holidays.dedup();
	return Ok(holidays);
}

fn date_parse(value: &str) -> Result<Date,CalendarHolidayError>
{
	let mut parts = value.split('-');
	let year = parts.next().and_then(|part| part.parse::<i32>().ok()).ok_or(CalendarHolidayError::InvalidResponse)?;
	let month = parts.next().and_then(|part| part.parse::<u8>().ok()).and_then(|month| Month::try_from(month).ok())
		.ok_or(CalendarHolidayError::InvalidResponse)?;
	let day = parts.next().and_then(|part| part.parse::<u8>().ok()).ok_or(CalendarHolidayError::InvalidResponse)?;
	if (parts.next().is_some())
	{
		return Err(CalendarHolidayError::InvalidResponse);
	}
	return Date::from_calendar_date(year,month,day).map_err(|_| CalendarHolidayError::InvalidResponse);
}

#[cfg(any(feature = "hydrate",test))]
fn holidays_period_get(
	holidays: impl IntoIterator<Item = CalendarHoliday>,
	period: CalendarPeriod,
) -> BTreeMap<Date,Vec<String>>
{
	let mut result = BTreeMap::<Date,Vec<String>>::new();
	for holiday in holidays
	{
		if (holiday.date >= period.start && holiday.date < period.endExclusive)
		{
			result.entry(holiday.date).or_default().push(holiday.name);
		}
	}
	for names in result.values_mut()
	{
		names.sort();
		names.dedup();
	}
	return result;
}

#[cfg(feature = "hydrate")]
mod browser
{
	use super::*;
	use async_lock::Mutex;
	use gloo_timers::callback::Timeout;
	use js_sys::Uint8Array;
	use std::collections::{BTreeSet,HashMap};
	use std::sync::LazyLock;
	use wasm_bindgen::JsCast;
	use wasm_bindgen_futures::JsFuture;
	use web_sys::{
		AbortController,ReadableStreamDefaultReader,ReadableStreamReadResult,ReferrerPolicy,
		Request,RequestCredentials,RequestInit,RequestMode,RequestRedirect,Response,
	};

	const HOLIDAY_API_ORIGIN: &str = "https://date.nager.at";
	const HOLIDAY_TIMEOUT_MS: u32 = 10_000;
	const HOLIDAY_FAILURE_CACHE_MS: f64 = 5.0 * 60.0 * 1_000.0;

	#[derive(Clone,Debug,Eq,Hash,PartialEq)]
	struct HolidayCacheKey
	{
		country: String,
		year: i32,
	}

	#[derive(Clone)]
	enum HolidayCacheEntry
	{
		Ready(Vec<CalendarHoliday>),
		Failed {
			error: CalendarHolidayError,
			retryAfterMilliseconds: f64,
		},
	}

	static HOLIDAY_CACHE: LazyLock<Mutex<HashMap<HolidayCacheKey,HolidayCacheEntry>>> =
		LazyLock::new(|| Mutex::new(HashMap::new()));

	pub(in crate::front::modules::calendar) async fn holidays_get(
		country: &str,
		period: CalendarPeriod,
	) -> Result<BTreeMap<Date,Vec<String>>,CalendarHolidayError>
	{
		let country = country_normalize(country)?;
		let mut years = BTreeSet::new();
		years.insert(period.start.year());
		if let Some(lastDate) = period.endExclusive.previous_day()
		{
			years.insert(lastDate.year());
		}
		let mut holidays = Vec::new();
		for year in years
		{
			holidays.extend(year_get(&country,year).await?);
		}
		return Ok(holidays_period_get(holidays,period));
	}

	fn country_normalize(country: &str) -> Result<String,CalendarHolidayError>
	{
		let country = country.trim().to_ascii_uppercase();
		if (country.len() != 2 || !country.bytes().all(|value| value.is_ascii_alphabetic()))
		{
			return Err(CalendarHolidayError::InvalidCountry);
		}
		return Ok(country);
	}

	async fn year_get(country: &str, year: i32) -> Result<Vec<CalendarHoliday>,CalendarHolidayError>
	{
		let key = HolidayCacheKey {country: country.to_string(),year};
		let mut cache = HOLIDAY_CACHE.lock().await;
		if let Some(entry) = cache.get(&key)
		{
			match entry
			{
				HolidayCacheEntry::Ready(holidays) => return Ok(holidays.clone()),
				HolidayCacheEntry::Failed {error,retryAfterMilliseconds}
					if (js_sys::Date::now() < *retryAfterMilliseconds) => return Err(*error),
				HolidayCacheEntry::Failed {..} => {},
			}
		}
		let result = year_fetch(country,year).await;
		let entry = match &result
		{
			Ok(holidays) => HolidayCacheEntry::Ready(holidays.clone()),
			Err(error) => HolidayCacheEntry::Failed {
				error: *error,
				retryAfterMilliseconds: js_sys::Date::now() + HOLIDAY_FAILURE_CACHE_MS,
			},
		};
		cache.insert(key,entry);
		return result;
	}

	async fn year_fetch(country: &str, year: i32) -> Result<Vec<CalendarHoliday>,CalendarHolidayError>
	{
		let url = format!("{HOLIDAY_API_ORIGIN}/api/v3/PublicHolidays/{year}/{country}");
		let controller = AbortController::new().map_err(|_| CalendarHolidayError::Transport)?;
		let requestInit = RequestInit::new();
		requestInit.set_method("GET");
		requestInit.set_credentials(RequestCredentials::Omit);
		requestInit.set_mode(RequestMode::Cors);
		requestInit.set_redirect(RequestRedirect::Error);
		requestInit.set_referrer_policy(ReferrerPolicy::NoReferrer);
		requestInit.set_signal(Some(&controller.signal()));
		let request = Request::new_with_str_and_init(&url,&requestInit).map_err(|_| CalendarHolidayError::Transport)?;
		let abortController = controller.clone();
		let _timeout = Timeout::new(HOLIDAY_TIMEOUT_MS,move || abortController.abort());
		let response = web_sys::window().ok_or(CalendarHolidayError::Transport)?
			.fetch_with_request(&request);
		let response = JsFuture::from(response).await.map_err(|_| CalendarHolidayError::Transport)?;
		let response: Response = response.dyn_into().map_err(|_| CalendarHolidayError::Transport)?;
		if (response.status() == 404)
		{
			return Err(CalendarHolidayError::InvalidCountry);
		}
		if (response.status() != 200)
		{
			return Err(CalendarHolidayError::Unavailable);
		}
		if let Some(contentLength) = response.headers().get("Content-Length").map_err(|_| CalendarHolidayError::InvalidResponse)?
			&& contentLength.parse::<usize>().ok().map(|length| length > HOLIDAY_MAX_RESPONSE_BYTES).unwrap_or(false)
		{
			return Err(CalendarHolidayError::ResponseTooLarge);
		}
		let body = response_body_read(&response).await?;
		return holidays_parse(&body,country,year);
	}

	async fn response_body_read(response: &Response) -> Result<String,CalendarHolidayError>
	{
		let Some(stream) = response.body() else {return Err(CalendarHolidayError::InvalidResponse);};
		let reader = ReadableStreamDefaultReader::new(&stream).map_err(|_| CalendarHolidayError::Transport)?;
		let mut bytes = Vec::new();
		loop
		{
			let result = JsFuture::from(reader.read()).await.map_err(|_| CalendarHolidayError::Transport)?;
			let result: ReadableStreamReadResult = result.unchecked_into();
			if (result.get_done().unwrap_or(false)) {break;}
			let chunk = Uint8Array::new(&result.get_value());
			let chunkLength = chunk.length() as usize;
			if (bytes.len().saturating_add(chunkLength) > HOLIDAY_MAX_RESPONSE_BYTES)
			{
				let _ = reader.cancel();
				return Err(CalendarHolidayError::ResponseTooLarge);
			}
			let start = bytes.len();
			bytes.resize(start + chunkLength,0);
			chunk.copy_to(&mut bytes[start..]);
		}
		reader.release_lock();
		return String::from_utf8(bytes).map_err(|_| CalendarHolidayError::InvalidResponse);
	}
}

#[cfg(feature = "hydrate")]
pub(in crate::front::modules::calendar) use browser::holidays_get;

#[cfg(test)]
mod tests
{
	use super::{CalendarHolidayError,holidays_parse,holidays_period_get};
	use crate::front::modules::calendar::domain::{CalendarPeriod,CalendarViewMode};
	use time::{Date,Month};

	#[test]
	fn response_keepsGlobalHolidaysAndPrefersLocalName()
	{
		let input = r#"[
			{"date":"2026-01-01","localName":"Jour de l’an","name":"New Year's Day","countryCode":"FR","global":true},
			{"date":"2026-12-26","localName":"Saint Étienne","name":"St. Stephen's Day","countryCode":"FR","global":false}
		]"#;

		let holidays = holidays_parse(input,"FR",2026).unwrap();

		assert_eq!(holidays.len(),1);
		assert_eq!(holidays[0].date,Date::from_calendar_date(2026,Month::January,1).unwrap());
		assert_eq!(holidays[0].name,"Jour de l’an");
	}

	#[test]
	fn response_rejectsUnexpectedCountryAndYear()
	{
		let country = r#"[{"date":"2026-01-01","localName":"Test","name":"Test","countryCode":"BE","global":true}]"#;
		let year = r#"[{"date":"2025-01-01","localName":"Test","name":"Test","countryCode":"FR","global":true}]"#;

		assert_eq!(holidays_parse(country,"FR",2026),Err(CalendarHolidayError::InvalidResponse));
		assert_eq!(holidays_parse(year,"FR",2026),Err(CalendarHolidayError::InvalidResponse));
	}

	#[test]
	fn weekPeriod_keepsHolidayInsideVisibleWeek()
	{
		let input = r#"[
			{"date":"2026-08-15","localName":"Assomption","name":"Assumption Day","countryCode":"FR","global":true}
		]"#;
		let holidayDate = Date::from_calendar_date(2026,Month::August,15).unwrap();
		let anchor = Date::from_calendar_date(2026,Month::August,13).unwrap();
		let period = CalendarPeriod::from_anchor(anchor,CalendarViewMode::Week);

		let holidays = holidays_period_get(holidays_parse(input,"FR",2026).unwrap(),period);

		assert_eq!(holidays.get(&holidayDate),Some(&vec!["Assomption".to_string()]));
	}
}
