#![cfg_attr(not(any(feature = "hydrate",test)),allow(dead_code))]

use quick_xml::events::{BytesRef, Event as XmlEvent};
use quick_xml::name::{Namespace, ResolveResult};
use quick_xml::reader::NsReader;
#[cfg(feature = "hydrate")]
use std::collections::HashSet;
#[cfg(feature = "hydrate")]
use time::OffsetDateTime;
#[cfg(any(feature = "hydrate",test))]
use url::Url;

#[cfg(feature = "hydrate")]
use super::domain::{
	CALENDAR_MAX_COLLECTIONS,CALENDAR_MAX_COLLECTION_NAME_BYTES,CALENDAR_MAX_REJECTED_SAMPLES,CALENDAR_MAX_URL_BYTES,
	CalendarCollection,CalendarConfig,CalendarEvent,CalendarPeriod,
};
use super::domain::CalendarConfigError;
#[cfg(any(feature = "hydrate",test))]
use super::icalendar_codec::CalendarCodecError;
#[cfg(feature = "hydrate")]
use super::icalendar_codec::{
	CalendarParsedEvents,CalendarResourceSource,build_event,event_is_recurring,exclude_occurrence,parse_expanded_events,
};
#[cfg(feature = "hydrate")]
use super::domain::CalendarCreateInput;

const DAV_NAMESPACE: &[u8] = b"DAV:";
const CALDAV_NAMESPACE: &[u8] = b"urn:ietf:params:xml:ns:caldav";
const APPLE_ICAL_NAMESPACE: &[u8] = b"http://apple.com/ns/ical/";
const CALDAV_MAX_XML_BYTES: usize = 8 * 1024 * 1024;
const CALDAV_MAX_RESPONSES: usize = 4_096;
const CALDAV_MAX_XML_DEPTH: usize = 32;
#[cfg(feature = "hydrate")]
const CALDAV_MAX_EVENTS: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(all(test,not(feature = "hydrate")),allow(dead_code))]
pub(super) enum CalDavError
{
	InvalidConfiguration,
	Configuration(CalendarConfigError),
	InsecureTransport,
	InvalidBasicUsername,
	Transport,
	Unauthorized,
	Forbidden,
	NotFound,
	Conflict,
	ResponseTooLarge,
	TooManyItems,
	InvalidResponse,
	InvalidCalendar,
	MissingEtag,
}

#[cfg(any(feature = "hydrate",test))]
impl From<CalendarCodecError> for CalDavError
{
	fn from(error: CalendarCodecError) -> Self
	{
		return match error
		{
			CalendarCodecError::InputTooLarge | CalendarCodecError::FieldTooLarge => Self::ResponseTooLarge,
			CalendarCodecError::TooManyEvents => Self::TooManyItems,
			_ => Self::InvalidCalendar,
		};
	}
}

#[derive(Clone, Default)]
#[cfg_attr(test, derive(Debug))]
struct DavPropertySet
{
	currentUserPrincipal: Option<String>,
	calendarHomeSet: Option<String>,
	isCalendar: bool,
	displayName: Option<String>,
	color: Option<String>,
	etag: Option<String>,
	calendarData: Option<String>,
}

impl DavPropertySet
{
	fn merge(&mut self, other: Self)
	{
		if (other.currentUserPrincipal.is_some()) {self.currentUserPrincipal = other.currentUserPrincipal;}
		if (other.calendarHomeSet.is_some()) {self.calendarHomeSet = other.calendarHomeSet;}
		if (other.displayName.is_some()) {self.displayName = other.displayName;}
		if (other.color.is_some()) {self.color = other.color;}
		if (other.etag.is_some()) {self.etag = other.etag;}
		if (other.calendarData.is_some()) {self.calendarData = other.calendarData;}
		self.isCalendar |= other.isCalendar;
	}
}

#[derive(Clone, Default)]
#[cfg_attr(test, derive(Debug))]
struct DavResponse
{
	href: String,
	properties: DavPropertySet,
	hasSuccessfulProperties: bool,
}

#[derive(Clone, Default)]
struct DavPropStat
{
	properties: DavPropertySet,
	status: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DavElement
{
	Response,
	Href,
	PropStat,
	Status,
	CurrentUserPrincipal,
	CalendarHomeSet,
	ResourceType,
	Calendar,
	DisplayName,
	CalendarColor,
	Etag,
	CalendarData,
	Other,
}

fn multistatus_parse(input: &str) -> Result<Vec<DavResponse>,CalDavError>
{
	if (input.len() > CALDAV_MAX_XML_BYTES)
	{
		return Err(CalDavError::ResponseTooLarge);
	}
	let mut reader = NsReader::from_str(input);
	reader.config_mut().trim_text(false);
	let mut stack = Vec::new();
	let mut response = None::<DavResponse>;
	let mut propStat = None::<DavPropStat>;
	let mut responses = Vec::new();

	loop
	{
		let (namespace,event) = reader.read_resolved_event().map_err(|_| CalDavError::InvalidResponse)?;
		match event
		{
			XmlEvent::Start(element) =>
			{
				let kind = element_kind(namespace,element.local_name().as_ref());
				if (stack.len() >= CALDAV_MAX_XML_DEPTH)
				{
					return Err(CalDavError::InvalidResponse);
				}
				if (kind == DavElement::Response)
				{
					if (response.is_some()) {return Err(CalDavError::InvalidResponse);}
					response = Some(DavResponse::default());
				}
				if (kind == DavElement::PropStat)
				{
					if (propStat.is_some()) {return Err(CalDavError::InvalidResponse);}
					propStat = Some(DavPropStat::default());
				}
				if (kind == DavElement::Calendar && stack.last() == Some(&DavElement::ResourceType))
				{
					property_set_mut(&mut response,&mut propStat)?.isCalendar = true;
				}
				stack.push(kind);
			},
			XmlEvent::Empty(element) =>
			{
				let kind = element_kind(namespace,element.local_name().as_ref());
				if (kind == DavElement::Calendar && stack.last() == Some(&DavElement::ResourceType))
				{
					property_set_mut(&mut response,&mut propStat)?.isCalendar = true;
				}
			},
			XmlEvent::Text(text) =>
			{
				let text = text.xml10_content().map_err(|_| CalDavError::InvalidResponse)?;
				text_append(&stack,&mut response,&mut propStat,&text)?;
			},
			XmlEvent::CData(text) =>
			{
				let text = text.decode().map_err(|_| CalDavError::InvalidResponse)?;
				text_append(&stack,&mut response,&mut propStat,&text)?;
			},
			XmlEvent::GeneralRef(reference) =>
			{
				let text = reference_resolve(&reference)?;
				text_append(&stack,&mut response,&mut propStat,&text)?;
			},
			XmlEvent::End(element) =>
			{
				let kind = element_kind(namespace,element.local_name().as_ref());
				if (stack.pop() != Some(kind))
				{
					return Err(CalDavError::InvalidResponse);
				}
				if (kind == DavElement::PropStat)
				{
					let completed = propStat.take().ok_or(CalDavError::InvalidResponse)?;
					if (http_status_is_success(&completed.status))
					{
						let current = response.as_mut().ok_or(CalDavError::InvalidResponse)?;
						current.properties.merge(completed.properties);
						current.hasSuccessfulProperties = true;
					}
				}
				if (kind == DavElement::Response)
				{
					if (responses.len() >= CALDAV_MAX_RESPONSES)
					{
						return Err(CalDavError::TooManyItems);
					}
					responses.push(response.take().ok_or(CalDavError::InvalidResponse)?);
				}
			},
			XmlEvent::Eof => break,
			XmlEvent::DocType(_) => return Err(CalDavError::InvalidResponse),
			_ => {},
		}
	}
	if (!stack.is_empty() || response.is_some() || propStat.is_some())
	{
		return Err(CalDavError::InvalidResponse);
	}
	return Ok(responses);
}

fn element_kind(namespace: ResolveResult<'_>, localName: &[u8]) -> DavElement
{
	let namespace = match namespace
	{
		ResolveResult::Bound(Namespace(value)) => value,
		_ => return DavElement::Other,
	};
	return match (namespace,localName)
	{
		(DAV_NAMESPACE,b"response") => DavElement::Response,
		(DAV_NAMESPACE,b"href") => DavElement::Href,
		(DAV_NAMESPACE,b"propstat") => DavElement::PropStat,
		(DAV_NAMESPACE,b"status") => DavElement::Status,
		(DAV_NAMESPACE,b"current-user-principal") => DavElement::CurrentUserPrincipal,
		(DAV_NAMESPACE,b"resourcetype") => DavElement::ResourceType,
		(DAV_NAMESPACE,b"displayname") => DavElement::DisplayName,
		(DAV_NAMESPACE,b"getetag") => DavElement::Etag,
		(CALDAV_NAMESPACE,b"calendar-home-set") => DavElement::CalendarHomeSet,
		(CALDAV_NAMESPACE,b"calendar") => DavElement::Calendar,
		(CALDAV_NAMESPACE,b"calendar-data") => DavElement::CalendarData,
		(APPLE_ICAL_NAMESPACE,b"calendar-color") => DavElement::CalendarColor,
		_ => DavElement::Other,
	};
}

fn property_set_mut<'a>(
	response: &'a mut Option<DavResponse>,
	propStat: &'a mut Option<DavPropStat>,
) -> Result<&'a mut DavPropertySet,CalDavError>
{
	if let Some(propStat) = propStat.as_mut()
	{
		return Ok(&mut propStat.properties);
	}
	return response.as_mut().map(|response| &mut response.properties).ok_or(CalDavError::InvalidResponse);
}

fn text_append(
	stack: &[DavElement],
	response: &mut Option<DavResponse>,
	propStat: &mut Option<DavPropStat>,
	text: &str,
) -> Result<(),CalDavError>
{
	let Some(current) = stack.last() else {return Ok(());};
	let parent = stack.iter().rev().nth(1).copied();
	match (*current,parent)
	{
		(DavElement::Href,Some(DavElement::Response)) =>
			response.as_mut().ok_or(CalDavError::InvalidResponse)?.href.push_str(text),
		(DavElement::Href,Some(DavElement::CurrentUserPrincipal)) =>
			property_option_append(&mut property_set_mut(response,propStat)?.currentUserPrincipal,text),
		(DavElement::Href,Some(DavElement::CalendarHomeSet)) =>
			property_option_append(&mut property_set_mut(response,propStat)?.calendarHomeSet,text),
		(DavElement::DisplayName,_) =>
			property_option_append(&mut property_set_mut(response,propStat)?.displayName,text),
		(DavElement::CalendarColor,_) =>
			property_option_append(&mut property_set_mut(response,propStat)?.color,text),
		(DavElement::Etag,_) =>
			property_option_append(&mut property_set_mut(response,propStat)?.etag,text),
		(DavElement::CalendarData,_) =>
		{
			let properties = property_set_mut(response,propStat)?;
			property_option_append(&mut properties.calendarData,text);
			if (properties.calendarData.as_ref().map(String::len).unwrap_or(0) > CALDAV_MAX_XML_BYTES)
			{
				return Err(CalDavError::ResponseTooLarge);
			}
		},
		(DavElement::Status,Some(DavElement::PropStat)) =>
			propStat.as_mut().ok_or(CalDavError::InvalidResponse)?.status.push_str(text),
		_ => {},
	}
	return Ok(());
}

fn property_option_append(value: &mut Option<String>, text: &str)
{
	value.get_or_insert_with(String::new).push_str(text);
}

fn reference_resolve(reference: &BytesRef<'_>) -> Result<String,CalDavError>
{
	if let Some(value) = reference.resolve_char_ref().map_err(|_| CalDavError::InvalidResponse)?
	{
		return Ok(value.to_string());
	}
	return match reference.decode().map_err(|_| CalDavError::InvalidResponse)?.as_ref()
	{
		"amp" => Ok("&".to_string()),
		"lt" => Ok("<".to_string()),
		"gt" => Ok(">".to_string()),
		"quot" => Ok("\"".to_string()),
		"apos" => Ok("'".to_string()),
		_ => Err(CalDavError::InvalidResponse),
	};
}

fn http_status_is_success(status: &str) -> bool
{
	status.split_ascii_whitespace().nth(1)
		.and_then(|code| code.parse::<u16>().ok())
		.map(|code| (200..300).contains(&code))
		.unwrap_or(false)
}

#[cfg(any(feature = "hydrate",test))]
fn eventCreateStatus_validate(status: u16,alreadyExistingIsSuccess: bool) -> Result<(),CalDavError>
{
	if (alreadyExistingIsSuccess && status == 412)
	{
		return Ok(());
	}
	if (matches!(status,201 | 204))
	{
		return Ok(());
	}
	return Err(match status
	{
		401 => CalDavError::Unauthorized,
		403 => CalDavError::Forbidden,
		404 => CalDavError::NotFound,
		409 | 412 => CalDavError::Conflict,
		_ => CalDavError::InvalidResponse,
	});
}

#[cfg(any(feature = "hydrate",test))]
fn href_resolve(base: &Url, href: &str) -> Result<Url,CalDavError>
{
	let href = base.join(href.trim()).map_err(|_| CalDavError::InvalidResponse)?;
	if (!matches!(href.scheme(),"https" | "http")
		|| href.origin().ascii_serialization() != base.origin().ascii_serialization()
		|| !href.username().is_empty()
		|| href.password().is_some()
		|| href.query().is_some()
		|| href.fragment().is_some())
	{
		return Err(CalDavError::InvalidResponse);
	}
	return Ok(href);
}

#[cfg(any(feature = "hydrate",test))]
fn collection_resource_contains(collectionUrl: &Url, resourceUrl: &Url) -> bool
{
	let mut collectionPath = collectionUrl.path().trim_end_matches('/').to_string();
	collectionPath.push('/');
	return resourceUrl.path().len() > collectionPath.len()
		&& resourceUrl.path().starts_with(&collectionPath);
}

#[cfg(feature = "hydrate")]
fn collection_directory_get(collectionUrl: &Url) -> Url
{
	let mut directory = collectionUrl.clone();
	let mut path = directory.path().trim_end_matches('/').to_string();
	path.push('/');
	directory.set_path(&path);
	return directory;
}

#[cfg(feature = "hydrate")]
mod browser
{
	use super::*;
	use base64ct::{Base64,Encoding};
	use gloo_timers::callback::Timeout;
	use js_sys::Uint8Array;
	use wasm_bindgen::{JsCast,JsValue};
	use wasm_bindgen_futures::JsFuture;
	use web_sys::{
		AbortController, Headers, ReadableStreamDefaultReader, ReadableStreamReadResult, ReferrerPolicy,
		Request, RequestCredentials, RequestInit, RequestMode, RequestRedirect, Response,
	};

	const CALDAV_TIMEOUT_MS: u32 = 20_000;
	const CALDAV_MAX_DISCOVERY_BYTES: usize = 512 * 1024;
	const CALDAV_MAX_RESOURCE_BYTES: usize = 2 * 1024 * 1024;

	pub(in crate::front::modules::calendar) struct CalDavClient
	{
		baseUrl: Url,
		authorization: String,
	}

	struct DavHttpResponse
	{
		status: u16,
		etag: Option<String>,
		body: String,
	}

	impl CalDavClient
	{
		pub fn new(config: &CalendarConfig) -> Result<Self,CalDavError>
		{
			let baseUrl = config.connection_validate().map_err(CalDavError::Configuration)?;
			if (baseUrl.scheme() == "http")
			{
				let pageProtocol = web_sys::window()
					.and_then(|window| window.location().protocol().ok())
					.ok_or(CalDavError::InsecureTransport)?;
				if (pageProtocol != "http:")
				{
					return Err(CalDavError::InsecureTransport);
				}
			}
			if (config.username.contains(':'))
			{
				return Err(CalDavError::InvalidBasicUsername);
			}
			let credentials = Base64::encode_string(format!("{}:{}",config.username,config.password).as_bytes());
			return Ok(Self {baseUrl,authorization: format!("Basic {credentials}")});
		}

		pub async fn collections_discover(&self) -> Result<Vec<CalendarCollection>,CalDavError>
		{
			let principalResponse = self.request(
				"PROPFIND",&self.baseUrl,
				&[("Depth","0"),("Content-Type","application/xml; charset=utf-8")],
				Some(CURRENT_USER_PRINCIPAL_BODY),CALDAV_MAX_DISCOVERY_BYTES,
			).await?;
			status_expect(principalResponse.status,&[207])?;
			let principalHref = multistatus_parse(&principalResponse.body)?.into_iter()
				.filter(|response| response.hasSuccessfulProperties)
				.find_map(|response| response.properties.currentUserPrincipal)
				.ok_or(CalDavError::InvalidResponse)?;
			let principalUrl = href_resolve(&self.baseUrl,&principalHref)?;

			let homeResponse = self.request(
				"PROPFIND",&principalUrl,
				&[("Depth","0"),("Content-Type","application/xml; charset=utf-8")],
				Some(CALENDAR_HOME_SET_BODY),CALDAV_MAX_DISCOVERY_BYTES,
			).await?;
			status_expect(homeResponse.status,&[207])?;
			let homeHref = multistatus_parse(&homeResponse.body)?.into_iter()
				.filter(|response| response.hasSuccessfulProperties)
				.find_map(|response| response.properties.calendarHomeSet)
				.ok_or(CalDavError::InvalidResponse)?;
			let homeUrl = href_resolve(&self.baseUrl,&homeHref)?;

			let collectionsResponse = self.request(
				"PROPFIND",&homeUrl,
				&[("Depth","1"),("Content-Type","application/xml; charset=utf-8")],
				Some(COLLECTIONS_BODY),CALDAV_MAX_DISCOVERY_BYTES,
			).await?;
			status_expect(collectionsResponse.status,&[207])?;
			let mut seen = HashSet::new();
			let mut collections = Vec::new();
			for response in multistatus_parse(&collectionsResponse.body)?
			{
				if (!response.hasSuccessfulProperties || !response.properties.isCalendar)
				{
					continue;
				}
				let collectionUrl = href_resolve(&self.baseUrl,&response.href)?;
				let href = collectionUrl.to_string();
				let name = response.properties.displayName.unwrap_or_default().trim().to_string();
				if (href.len() > CALENDAR_MAX_URL_BYTES || name.len() > CALENDAR_MAX_COLLECTION_NAME_BYTES)
				{
					return Err(CalDavError::ResponseTooLarge);
				}
				if (!seen.insert(href.clone())) {continue;}
				if (collections.len() >= CALENDAR_MAX_COLLECTIONS)
				{
					return Err(CalDavError::TooManyItems);
				}
				collections.push(CalendarCollection {
					href,
					name,
					color: CalendarCollection::color_normalize(response.properties.color),
				});
			}
			collections.sort_by(|left,right| left.name.to_lowercase().cmp(&right.name.to_lowercase()).then_with(|| left.href.cmp(&right.href)));
			return Ok(collections);
		}

		pub async fn events_get(
			&self,
			collection: &CalendarCollection,
			period: CalendarPeriod,
			floatingTimezone: &str,
		) -> Result<CalendarParsedEvents,CalDavError>
		{
			let collectionUrl = href_resolve(&self.baseUrl,&collection.href)?;
			let body = calendar_query_body(&period);
			let response = self.request(
				"REPORT",&collectionUrl,
				&[("Depth","1"),("Content-Type","application/xml; charset=utf-8")],
				Some(&body),CALDAV_MAX_XML_BYTES,
			).await?;
			status_expect(response.status,&[207])?;
			let mut events = Vec::new();
			let mut rejectedCount = 0;
			let mut rejectedSamples = Vec::new();
			for resource in multistatus_parse(&response.body)?
			{
				if (!resource.hasSuccessfulProperties) {continue;}
				let Some(calendarData) = resource.properties.calendarData else {continue;};
				let resourceUrl = href_resolve(&self.baseUrl,&resource.href)?;
				if (!collection_resource_contains(&collectionUrl,&resourceUrl))
				{
					return Err(CalDavError::InvalidResponse);
				}
				let source = CalendarResourceSource {
					collectionHref: collection.href.clone(),
					collectionName: collection.name.clone(),
					collectionColor: collection.color.clone(),
					resourceHref: resourceUrl.to_string(),
					etag: resource.properties.etag.map(|etag| etag.trim().to_string()),
				};
				let parsed = parse_expanded_events(&calendarData,&source,floatingTimezone)?;
				rejectedCount += parsed.rejectedCount;
				let remainingSamples = CALENDAR_MAX_REJECTED_SAMPLES.saturating_sub(rejectedSamples.len());
				rejectedSamples.extend(parsed.rejectedSamples.into_iter().take(remainingSamples));
				events.extend(parsed.events);
				if (events.len() > CALDAV_MAX_EVENTS)
				{
					return Err(CalDavError::TooManyItems);
				}
			}
			CalendarEvent::sort_deterministically(&mut events);
			return Ok(CalendarParsedEvents {events,rejectedCount,rejectedSamples});
		}

		pub async fn event_create(
			&self,
			collection: &CalendarCollection,
			input: &CalendarCreateInput,
		) -> Result<(),CalDavError>
		{
			let uid = uuid::Uuid::new_v4().to_string();
			return self.event_createWithUid(collection,input,&uid,false).await;
		}

		pub async fn event_createIdempotent(
			&self,
			collection: &CalendarCollection,
			input: &CalendarCreateInput,
			uid: &str,
		) -> Result<(),CalDavError>
		{
			return self.event_createWithUid(collection,input,uid,true).await;
		}

		async fn event_createWithUid(
			&self,
			collection: &CalendarCollection,
			input: &CalendarCreateInput,
			uid: &str,
			alreadyExistingIsSuccess: bool,
		) -> Result<(),CalDavError>
		{
			let collectionUrl = href_resolve(&self.baseUrl,&collection.href)?;
			let built = build_event(input,uid.to_string(),OffsetDateTime::now_utc())?;
			let resourceUrl = collection_directory_get(&collectionUrl)
				.join(&format!("{}.ics",built.uid)).map_err(|_| CalDavError::InvalidConfiguration)?;
			if (!collection_resource_contains(&collectionUrl,&resourceUrl))
			{
				return Err(CalDavError::InvalidConfiguration);
			}
			let response = self.request(
				"PUT",&resourceUrl,
				&[("Content-Type","text/calendar; charset=utf-8"),("If-None-Match","*")],
				Some(&built.content),CALDAV_MAX_DISCOVERY_BYTES,
			).await?;
			return eventCreateStatus_validate(response.status,alreadyExistingIsSuccess);
		}

		pub async fn event_delete_series(&self, event: &CalendarEvent) -> Result<(),CalDavError>
		{
			let resourceUrl = self.event_resource_get(event)?;
			let etag = event.etag.as_deref().ok_or(CalDavError::MissingEtag)?;
			let response = self.request("DELETE",&resourceUrl,&[("If-Match",etag)],None,CALDAV_MAX_DISCOVERY_BYTES).await?;
			return status_expect(response.status,&[200,204]);
		}

		pub async fn event_delete_occurrence(&self, event: &CalendarEvent) -> Result<(),CalDavError>
		{
			let resourceUrl = self.event_resource_get(event)?;
			let response = self.request("GET",&resourceUrl,&[],None,CALDAV_MAX_RESOURCE_BYTES).await?;
			status_expect(response.status,&[200])?;
			let etag = response.etag.or_else(|| event.etag.clone()).ok_or(CalDavError::MissingEtag)?;
			let occurrence = event.occurrence.as_ref().unwrap_or(&event.start);
			let updated = exclude_occurrence(&response.body,&event.identity.uid,occurrence,OffsetDateTime::now_utc())?;
			let response = self.request(
				"PUT",&resourceUrl,
				&[("Content-Type","text/calendar; charset=utf-8"),("If-Match",etag.as_str())],
				Some(&updated),CALDAV_MAX_DISCOVERY_BYTES,
			).await?;
			return status_expect(response.status,&[200,201,204]);
		}

		pub async fn event_recurrence_get(&self, event: &CalendarEvent) -> Result<bool,CalDavError>
		{
			let resourceUrl = self.event_resource_get(event)?;
			let response = self.request("GET",&resourceUrl,&[],None,CALDAV_MAX_RESOURCE_BYTES).await?;
			status_expect(response.status,&[200])?;
			return event_is_recurring(&response.body,&event.identity.uid).map_err(Into::into);
		}

		fn event_resource_get(&self, event: &CalendarEvent) -> Result<Url,CalDavError>
		{
			let collectionUrl = href_resolve(&self.baseUrl,&event.identity.collectionHref)?;
			let resourceUrl = href_resolve(&self.baseUrl,&event.identity.resourceHref)?;
			if (!collection_resource_contains(&collectionUrl,&resourceUrl))
			{
				return Err(CalDavError::InvalidResponse);
			}
			return Ok(resourceUrl);
		}

		async fn request(
			&self,
			method: &str,
			url: &Url,
			headers: &[(&str,&str)],
			body: Option<&str>,
			maxBytes: usize,
		) -> Result<DavHttpResponse,CalDavError>
		{
			if (url.origin().ascii_serialization() != self.baseUrl.origin().ascii_serialization())
			{
				return Err(CalDavError::InvalidConfiguration);
			}
			let requestHeaders = Headers::new().map_err(|_| CalDavError::Transport)?;
			requestHeaders.set("Authorization",&self.authorization).map_err(|_| CalDavError::Transport)?;
			for (name,value) in headers
			{
				requestHeaders.set(name,value).map_err(|_| CalDavError::Transport)?;
			}
			let controller = AbortController::new().map_err(|_| CalDavError::Transport)?;
			let requestInit = RequestInit::new();
			requestInit.set_method(method);
			requestInit.set_headers_headers(&requestHeaders);
			requestInit.set_credentials(RequestCredentials::Omit);
			requestInit.set_mode(RequestMode::Cors);
			requestInit.set_redirect(RequestRedirect::Error);
			requestInit.set_referrer_policy(ReferrerPolicy::NoReferrer);
			requestInit.set_signal(Some(&controller.signal()));
			if let Some(body) = body
			{
				requestInit.set_body(&JsValue::from_str(body));
			}
			let request = Request::new_with_str_and_init(url.as_str(),&requestInit).map_err(|_| CalDavError::Transport)?;
			let abortController = controller.clone();
			let _timeout = Timeout::new(CALDAV_TIMEOUT_MS,move || abortController.abort());
			let response = web_sys::window().ok_or(CalDavError::Transport)?
				.fetch_with_request(&request);
			let response = JsFuture::from(response).await.map_err(|_| CalDavError::Transport)?;
			let response: Response = response.dyn_into().map_err(|_| CalDavError::Transport)?;
			if let Some(contentLength) = response.headers().get("Content-Length").map_err(|_| CalDavError::InvalidResponse)?
				&& contentLength.parse::<usize>().ok().map(|length| length > maxBytes).unwrap_or(false)
			{
				return Err(CalDavError::ResponseTooLarge);
			}
			let body = response_body_read(&response,maxBytes).await?;
			let etag = response.headers().get("ETag").map_err(|_| CalDavError::InvalidResponse)?;
			return Ok(DavHttpResponse {status: response.status(),etag,body});
		}
	}

	async fn response_body_read(response: &Response, maxBytes: usize) -> Result<String,CalDavError>
	{
		let Some(stream) = response.body() else {return Ok(String::new());};
		let reader = ReadableStreamDefaultReader::new(&stream).map_err(|_| CalDavError::Transport)?;
		let mut bytes = Vec::new();
		loop
		{
			let result = JsFuture::from(reader.read()).await.map_err(|_| CalDavError::Transport)?;
			let result: ReadableStreamReadResult = result.unchecked_into();
			if (result.get_done().unwrap_or(false)) {break;}
			let chunk = Uint8Array::new(&result.get_value());
			let chunkLength = chunk.length() as usize;
			if (bytes.len().saturating_add(chunkLength) > maxBytes)
			{
				let _ = reader.cancel();
				return Err(CalDavError::ResponseTooLarge);
			}
			let start = bytes.len();
			bytes.resize(start + chunkLength,0);
			chunk.copy_to(&mut bytes[start..]);
		}
		reader.release_lock();
		return String::from_utf8(bytes).map_err(|_| CalDavError::InvalidResponse);
	}

	const CURRENT_USER_PRINCIPAL_BODY: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?><d:propfind xmlns:d=\"DAV:\"><d:prop><d:current-user-principal/></d:prop></d:propfind>";
	const CALENDAR_HOME_SET_BODY: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?><d:propfind xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\"><d:prop><c:calendar-home-set/></d:prop></d:propfind>";
	const COLLECTIONS_BODY: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?><d:propfind xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\" xmlns:i=\"http://apple.com/ns/ical/\"><d:prop><d:resourcetype/><d:displayname/><i:calendar-color/></d:prop></d:propfind>";

	fn calendar_query_body(period: &CalendarPeriod) -> String
	{
		format!("<?xml version=\"1.0\" encoding=\"utf-8\"?><c:calendar-query xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\"><d:prop><d:getetag/><c:calendar-data><c:expand start=\"{}\" end=\"{}\"/></c:calendar-data></d:prop><c:filter><c:comp-filter name=\"VCALENDAR\"><c:comp-filter name=\"VEVENT\"><c:time-range start=\"{}\" end=\"{}\"/></c:comp-filter></c:comp-filter></c:filter></c:calendar-query>",period.query_start_utc(),period.query_end_utc(),period.query_start_utc(),period.query_end_utc())
	}

	fn status_expect(status: u16, expected: &[u16]) -> Result<(),CalDavError>
	{
		if (expected.contains(&status)) {return Ok(());}
		return Err(match status
		{
			401 => CalDavError::Unauthorized,
			403 => CalDavError::Forbidden,
			404 => CalDavError::NotFound,
			409 | 412 => CalDavError::Conflict,
			_ => CalDavError::InvalidResponse,
		});
	}
}

#[cfg(feature = "hydrate")]
pub(super) use browser::CalDavClient;

#[cfg(test)]
mod tests
{
	use super::{CalDavError,collection_resource_contains,eventCreateStatus_validate,href_resolve,multistatus_parse};
	use url::Url;

	#[test]
	fn multistatus_readsNamespacedDiscoveryAndEscapedText()
	{
		let input = "<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\" xmlns:i=\"http://apple.com/ns/ical/\"><d:response><d:href>/user/calendar/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><c:calendar/></d:resourcetype><d:displayname>Work &amp; Home</d:displayname><i:calendar-color>#336699</i:calendar-color><d:getetag>&quot;abc&quot;</d:getetag><c:calendar-data>BEGIN:VCALENDAR&#13;&#10;END:VCALENDAR&#13;&#10;</c:calendar-data></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>";

		let parsed = multistatus_parse(input).unwrap();

		assert_eq!(parsed.len(),1);
		assert!(parsed[0].properties.isCalendar);
		assert_eq!(parsed[0].properties.displayName.as_deref(),Some("Work & Home"));
		assert_eq!(parsed[0].properties.etag.as_deref(),Some("\"abc\""));
		assert_eq!(parsed[0].properties.calendarData.as_deref(),Some("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n"));
	}

	#[test]
	fn multistatus_ignoresFailedPropstat()
	{
		let input = "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/test/</d:href><d:propstat><d:prop><d:displayname>Hidden</d:displayname></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response></d:multistatus>";

		let parsed = multistatus_parse(input).unwrap();

		assert!(!parsed[0].hasSuccessfulProperties);
		assert!(parsed[0].properties.displayName.is_none());
	}

	#[test]
	fn multistatus_rejectsDoctype()
	{
		let input = "<!DOCTYPE x [<!ENTITY secret 'value'>]><d:multistatus xmlns:d=\"DAV:\"/>";

		assert_eq!(multistatus_parse(input).unwrap_err(),CalDavError::InvalidResponse);
	}

	#[test]
	fn resourceMustBeARealChildOfItsCollection()
	{
		let base = Url::parse("https://calendar.invalid/").unwrap();
		let collection = href_resolve(&base,"/user/cal").unwrap();
		let child = href_resolve(&base,"/user/cal/event.ics").unwrap();
		let siblingPrefix = href_resolve(&base,"/user/calendar/event.ics").unwrap();

		assert!(collection_resource_contains(&collection,&child));
		assert!(!collection_resource_contains(&collection,&siblingPrefix));
		assert!(!collection_resource_contains(&collection,&collection));
	}

	#[test]
	fn developmentHttpHrefsRemainOnTheConfiguredOrigin()
	{
		let base = Url::parse("http://192.168.1.20:5232/root/").unwrap();

		assert_eq!(
			href_resolve(&base,"/test/calendar/").unwrap().as_str(),
			"http://192.168.1.20:5232/test/calendar/",
		);
		assert_eq!(
			href_resolve(&base,"https://calendar.invalid/test/").unwrap_err(),
			CalDavError::InvalidResponse,
		);
	}

	#[test]
	fn idempotentCreateAcceptsOnlyItsOwnPreconditionConflict()
	{
		assert!(eventCreateStatus_validate(201,true).is_ok());
		assert!(eventCreateStatus_validate(204,true).is_ok());
		assert!(eventCreateStatus_validate(412,true).is_ok());
		assert_eq!(eventCreateStatus_validate(412,false),Err(CalDavError::Conflict));
		assert_eq!(eventCreateStatus_validate(409,true),Err(CalDavError::Conflict));
		assert_eq!(eventCreateStatus_validate(500,true),Err(CalDavError::InvalidResponse));
	}
}
