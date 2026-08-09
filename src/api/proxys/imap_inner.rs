use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ammonia::{Builder, UrlRelative};
use base64ct::{Base64, Encoding};
use imap::types::{Fetch, Fetches, Uid};
use imap::{Connection, Error, Session};
use mailparse::{parse_mail, DispositionType, MailHeaderMap, ParsedMail};

use crate::api::proxys::imap_components::{
	imap_connector,
	Attachment,
	Attributs,
	BoxName,
	ImapMailboxSync,
	ImapMailboxSyncState,
	ImapMail,
	ImapMailContentType,
	ImapMailKey,
	ImapSyncRequest,
};
use crate::api::proxys::imap_error::ImapError;
use crate::api::proxys::outbound_policy::ValidatedImapDestination;

struct ImapLimits;

impl ImapLimits
{
	const ATTACHMENT_MAXIMUM: usize = 64;
	const ATTACHMENT_FILENAME_MAXIMUM_BYTES: usize = 1_024;
	const ATTACHMENT_MIME_MAXIMUM_BYTES: usize = 255;
	const ATTACHMENT_CONTENT_ID_MAXIMUM_BYTES: usize = 1_024;
	const ATTACHMENT_INDIVIDUAL_MAXIMUM_BYTES: usize = 16 * 1024 * 1024;
	const ATTACHMENT_TOTAL_MAXIMUM_BYTES: usize = 16 * 1024 * 1024;
	const BLACKLIST_ENTRY_MAXIMUM: usize = 128;
	const BOX_NAME_MAXIMUM_BYTES: usize = 1_024;
	const CREDENTIAL_MAXIMUM_BYTES: usize = 4_096;
	const HEADER_MAXIMUM_BYTES: usize = 64 * 1024;
	const MAILBOX_MAXIMUM: usize = 64;
	const MAIL_CONTENT_MAXIMUM_BYTES: usize = 16 * 1024 * 1024;
	const MAIL_HEADER_MAXIMUM: usize = 100;
	const MAIL_RENDERED_CONTENT_MAXIMUM_BYTES: usize = 24 * 1024 * 1024;
	const MAIL_RENDERED_INLINE_MAXIMUM_BYTES: usize = 8 * 1024 * 1024;
	const OPERATION_TIMEOUT: Duration = Duration::from_secs(90);
	const SYNC_KNOWN_UID_MAXIMUM: usize = Self::MAIL_HEADER_MAXIMUM;
}

struct ImapWorkBudget
{
	deadline: Instant,
	remainingHeaders: usize,
}

struct ImapUidSet
{
	uids: Vec<Uid>,
}

impl ImapUidSet
{
	fn new(mut uids: Vec<Uid>) -> Self
	{
		uids.sort_unstable();
		uids.dedup();
		return Self { uids };
	}

	fn contains(&self, uid: Uid) -> bool
	{
		return self.uids.binary_search(&uid).is_ok();
	}

	fn is_empty(&self) -> bool
	{
		return self.uids.is_empty();
	}

	fn sequence_get(&self) -> String
	{
		let mut parts = Vec::new();
		let Some(first) = self.uids.first().copied()
		else
		{
			return String::new();
		};
		let mut rangeStart = first;
		let mut previous = first;

		for uid in self.uids.iter().copied().skip(1)
		{
			if (uid == previous.saturating_add(1))
			{
				previous = uid;
				continue;
			}
			parts.push(Self::range_format(rangeStart,previous));
			rangeStart = uid;
			previous = uid;
		}
		parts.push(Self::range_format(rangeStart,previous));
		return parts.join(",");
	}

	fn range_format(start: Uid, end: Uid) -> String
	{
		if (start == end)
		{
			return start.to_string();
		}
		return format!("{}:{}",start,end);
	}
}

struct ImapMailboxDelta
{
	newUids: HashSet<Uid>,
	removedUids: Vec<Uid>,
}

impl ImapMailboxDelta
{
	fn new(state: Option<&ImapMailboxSyncState>, uidValidity: u32, unseenUids: HashSet<Uid>) -> Self
	{
		let knownUids = state
			.filter(|state| state.uidValidity == Some(uidValidity))
			.map(|state| state.knownUids.iter().copied().collect::<HashSet<_>>())
			.unwrap_or_default();
		let newUids = unseenUids.difference(&knownUids).copied().collect::<HashSet<_>>();
		let mut removedUids = knownUids.difference(&unseenUids).copied().collect::<Vec<_>>();
		removedUids.sort_unstable();
		return Self { newUids, removedUids };
	}
}

impl ImapWorkBudget
{
	fn new() -> Self
	{
		return Self {
			deadline: Instant::now() + ImapLimits::OPERATION_TIMEOUT,
			remainingHeaders: ImapLimits::MAIL_HEADER_MAXIMUM,
		};
	}

	fn active_require(&self) -> Result<(), ImapError>
	{
		if (Instant::now() >= self.deadline)
		{
			return Err(ImapError::RESOURCE_LIMIT);
		}
		return Ok(());
	}

	fn trackedUids_get(&mut self, results: HashSet<Uid>) -> Result<HashSet<Uid>, ImapError>
	{
		self.active_require()?;
		if (results.contains(&0))
		{
			return Err(ImapError::SERVER_ERROR);
		}
		let mut uids = results.into_iter().collect::<Vec<_>>();
		uids.sort_unstable_by(|left, right| right.cmp(left));
		uids.truncate(self.remainingHeaders);
		self.remainingHeaders = self.remainingHeaders.saturating_sub(uids.len());
		return Ok(uids.into_iter().collect());
	}

}

pub(super) struct ImapProxy
{
	config: imap_connector,
	destination: ValidatedImapDestination,
}

impl ImapProxy
{
	pub(super) fn new(config: imap_connector, destination: ValidatedImapDestination) -> Result<Self, ImapError>
	{
		let proxy = Self { config, destination };
		proxy.config_validate()?;
		return Ok(proxy);
	}

	pub(super) fn listbox_get(self) -> Result<Vec<BoxName>, ImapError>
	{
		let (mut session, isGmail) = self.connection_get("list")?;
		let result = self.mailboxes_get(&mut session,isGmail,"list");
		let _ = session.logout();
		return result;
	}

	pub(super) fn sync_get(self, request: ImapSyncRequest) -> Result<Vec<ImapMailboxSync>, ImapError>
	{
		self.syncRequest_validate(&request)
			.map_err(|error| error.trace("sync","request_validation",None))?;
		let configuredMailboxes = self.config.selectedBoxNames_get().map(|boxNames| {
			return boxNames.iter().map(|boxName| ImapMailboxSyncState {
				boxName: boxName.clone(),
				uidValidity: None,
				knownUids: Vec::new(),
			}).collect::<Vec<_>>();
		});
		if (self.config.extra.is_none()
			|| request.mailboxes.as_ref().is_some_and(Vec::is_empty)
			|| configuredMailboxes.as_ref().is_some_and(Vec::is_empty))
		{
			return Ok(Vec::new());
		}

		let mut budget = ImapWorkBudget::new();
		let (mut session, isGmail) = self.connection_get("sync")?;
		let mut mailboxStates = match request.mailboxes
		{
			Some(mailboxes) => mailboxes,
			None => match configuredMailboxes
			{
				Some(mailboxes) => mailboxes,
				None => self.mailboxes_get(&mut session,isGmail,"sync")?
					.into_iter()
					.filter(|mailbox| !mailbox.attributes.is_uninteresting())
					.map(|mailbox| ImapMailboxSyncState {
						boxName: mailbox.name,
						uidValidity: None,
						knownUids: Vec::new(),
					})
					.collect(),
			},
		};
		mailboxStates.sort_unstable_by(|left,right| left.boxName.cmp(&right.boxName));
		mailboxStates.dedup_by(|left,right| left.boxName == right.boxName);

		let syncResult = mailboxStates.into_iter().enumerate().try_fold(Vec::new(), |mut result,(mailboxIndex,mailboxState)| {
			budget.active_require()
				.map_err(|error| error.trace("sync","budget",Some(mailboxIndex)))?;
			if (Self::mailbox_isAutomaticallyExcluded(&mailboxState.boxName,isGmail)
				|| !self.config.isBoxSelected(&mailboxState.boxName))
			{
				return Ok(result);
			}
			if let Some(mailbox) = Self::mailboxSync_get(&mut session,&mailboxState,&mut budget,mailboxIndex)?
			{
				result.push(mailbox);
			}
			return Ok::<Vec<ImapMailboxSync>,ImapError>(result);
		});
		let _ = session.logout();
		return syncResult;
	}

	fn mailboxSync_get<T: Read + Write>(
		session: &mut Session<T>,
		state: &ImapMailboxSyncState,
		budget: &mut ImapWorkBudget,
		mailboxIndex: usize,
	) -> Result<Option<ImapMailboxSync>, ImapError>
	{
		let mailbox = match session.select(&state.boxName)
		{
			Ok(mailbox) => mailbox,
			Err(error @ (Error::No(_) | Error::Bad(_))) =>
			{
				use Htrace::components::level::Level;
				use Htrace::HTrace;

				HTrace!(
					(Level::WARNING)
					"[IMAP proxy] operation=sync stage=mailbox_select mailbox_index={} source={} action=skip",
					mailboxIndex,
					ImapError::imapSource_get(&error)
				);
				return Ok(None);
			},
			Err(error) =>
			{
				return Err(ImapError::fromImapAt(error,"sync","mailbox_select",Some(mailboxIndex)));
			},
		};
		let uidValidity = mailbox.uid_validity.filter(|uidValidity| *uidValidity > 0)
			.ok_or_else(|| ImapError::SERVER_ERROR.trace("sync","uid_validity",Some(mailboxIndex)))?;
		let unseenUids = session.uid_search(
			"UNSEEN UNKEYWORD $Junk UNKEYWORD $Spam UNDELETED UNANSWERED UNDRAFT",
		).map_err(|error| ImapError::fromImapAt(error,"sync","mailbox_search",Some(mailboxIndex)))?;
		let trackedUids = budget.trackedUids_get(unseenUids)
			.map_err(|error| error.trace("sync","search_result",Some(mailboxIndex)))?;
		let delta = ImapMailboxDelta::new(Some(state),uidValidity,trackedUids);
		let mails = ImapMailParser::headers_get(session,delta.newUids,budget,mailboxIndex)?;
		return Ok(Some(ImapMailboxSync {
			boxName: state.boxName.clone(),
			uidValidity,
			removedUids: delta.removedUids,
			mails,
		}));
	}

	pub(super) fn mailContent_get(self, mail: ImapMailKey) -> Result<ImapMail, ImapError>
	{
		Self::mailKey_validate(&mail)
			.map_err(|error| error.trace("content","mail_key",None))?;
		let budget = ImapWorkBudget::new();
		let (mut session, _) = self.connection_get("content")?;
		let result = Self::mailContentFromSession_get(&mut session,&mail,&budget);
		let _ = session.logout();
		return result;
	}

	fn mailContentFromSession_get<T: Read + Write>(
		session: &mut Session<T>,
		mail: &ImapMailKey,
		budget: &ImapWorkBudget,
	) -> Result<ImapMail, ImapError>
	{
		budget.active_require()
			.map_err(|error| error.trace("content","budget",None))?;
		let mailbox = session.select(&mail.boxName)
			.map_err(|error| ImapError::fromImapAt(error,"content","mailbox_select",None))?;
		Self::mailKeyValidity_require(mail,mailbox.uid_validity)
			.map_err(|error| error.trace("content","uid_validity",None))?;
		let query = format!(
			"(RFC822.SIZE BODY.PEEK[]<0.{}>)",
			ImapLimits::MAIL_CONTENT_MAXIMUM_BYTES + 1,
		);
		let results = session.uid_fetch(mail.uid.to_string(), query)
			.map_err(|error| {
				let _ = ImapError::fromImapAt(error,"content","mail_fetch",None);
				return ImapError::MAIL_NOT_FOUND;
			})?;
		let mail = ImapMailParser::fromFetch(
			mail.uid,
			results,
			Some(ImapLimits::MAIL_CONTENT_MAXIMUM_BYTES),
		).map_err(|error| error.trace("content","mail_parse",None))?
			.ok_or_else(|| ImapError::MAIL_NOT_FOUND.trace("content","mail_missing",None))?;
		return Ok(mail);
	}

	pub(super) fn mailSeen_set(self, mail: ImapMailKey) -> Result<(), ImapError>
	{
		Self::mailKey_validate(&mail)
			.map_err(|error| error.trace("seen","mail_key",None))?;
		let budget = ImapWorkBudget::new();
		let (mut session, _) = self.connection_get("seen")?;
		let result = Self::mailSeenInSession_set(&mut session,&mail,&budget);
		let _ = session.logout();
		return result;
	}

	fn mailSeenInSession_set<T: Read + Write>(
		session: &mut Session<T>,
		mail: &ImapMailKey,
		budget: &ImapWorkBudget,
	) -> Result<(), ImapError>
	{
		budget.active_require()
			.map_err(|error| error.trace("seen","budget",None))?;
		let mailbox = session.select(&mail.boxName)
			.map_err(|error| ImapError::fromImapAt(error,"seen","mailbox_select",None))?;
		Self::mailKeyValidity_require(mail,mailbox.uid_validity)
			.map_err(|error| error.trace("seen","uid_validity",None))?;
		session.uid_store(mail.uid.to_string(), "+FLAGS.SILENT (\\Seen)")
			.map_err(|error| ImapError::fromImapAt(error,"seen","mail_store",None))?;
		return Ok(());
	}

	fn config_validate(&self) -> Result<(), ImapError>
	{
		if (self.config.username.len() > ImapLimits::CREDENTIAL_MAXIMUM_BYTES
			|| self.config.password.len() > ImapLimits::CREDENTIAL_MAXIMUM_BYTES)
		{
			return Err(ImapError::RESOURCE_LIMIT);
		}
		if let Some(extra) = &self.config.extra
		{
			if (extra.boxBlackList.len() > ImapLimits::BLACKLIST_ENTRY_MAXIMUM
				|| extra.flagBlackList.len() > ImapLimits::BLACKLIST_ENTRY_MAXIMUM
				|| extra.boxAllowList.as_ref().is_some_and(|boxAllowList| {
					return boxAllowList.len() > ImapLimits::MAILBOX_MAXIMUM
						|| boxAllowList.iter().any(|entry| {
							return entry.is_empty() || entry.len() > ImapLimits::BOX_NAME_MAXIMUM_BYTES;
						});
				})
				|| extra.boxBlackList.iter().chain(extra.flagBlackList.iter())
					.any(|entry| entry.len() > ImapLimits::BOX_NAME_MAXIMUM_BYTES))
			{
				return Err(ImapError::RESOURCE_LIMIT);
			}
		}
		return Ok(());
	}

	fn syncRequest_validate(&self, request: &ImapSyncRequest) -> Result<(), ImapError>
	{
		let Some(mailboxes) = &request.mailboxes
		else
		{
			return Ok(());
		};
		if (mailboxes.len() > ImapLimits::MAILBOX_MAXIMUM)
		{
			return Err(ImapError::RESOURCE_LIMIT);
		}
		let mut boxNames = HashSet::new();
		let mut knownUidCount = 0usize;
		for mailbox in mailboxes
		{
			Self::boxName_validate(&mailbox.boxName)?;
			if (!boxNames.insert(mailbox.boxName.as_str())
				|| (mailbox.uidValidity.is_none() && !mailbox.knownUids.is_empty())
				|| mailbox.knownUids.contains(&0))
			{
				return Err(ImapError::RESOURCE_LIMIT);
			}
			knownUidCount = knownUidCount.saturating_add(mailbox.knownUids.len());
		}
		if (knownUidCount > ImapLimits::SYNC_KNOWN_UID_MAXIMUM)
		{
			return Err(ImapError::RESOURCE_LIMIT);
		}
		return Ok(());
	}

	fn connection_get(&self, operation: &'static str) -> Result<(Session<Connection>, bool), ImapError>
	{
		let isGmail = self.config.isGmail();
		let connection = self.destination.connection_get()
			.map_err(|error| ImapError::fromImapAt(error,operation,"connection",None))?;
		let mut client = imap::Client::new(connection);
		client.read_greeting()
			.map_err(|error| ImapError::fromImapAt(error,operation,"greeting",None))?;
		let session = client
			.login(self.config.username.clone(), self.config.password.clone())
			.map_err(|error| ImapError::fromImapAt(error.0,operation,"login",None))?;
		return Ok((session, isGmail));
	}

	fn mailboxes_get(
		&self,
		session: &mut Session<Connection>,
		isGmail: bool,
		operation: &'static str,
	) -> Result<Vec<BoxName>, ImapError>
	{
		let names = session.list(None, Some("*"))
			.map_err(|error| ImapError::fromImapAt(error,operation,"mailbox_list",None))?;
		let mut mailboxes = Vec::new();
		for result in names.iter()
		{
			if (Self::mailbox_isAutomaticallyExcluded(result.name(),isGmail))
			{
				continue;
			}
			Self::boxName_validate(result.name())
				.map_err(|error| error.trace(operation,"mailbox_name",None))?;
			let mut attributes = Attributs::default();
			result.attributes().iter().for_each(|attribute| attributes.add(attribute));
			mailboxes.push(BoxName { name: result.name().to_string(), attributes });
			if (mailboxes.len() >= ImapLimits::MAILBOX_MAXIMUM)
			{
				break;
			}
		}
		return Ok(mailboxes);
	}

	fn mailbox_isAutomaticallyExcluded(boxName: &str, isGmail: bool) -> bool
	{
		return isGmail && boxName.eq_ignore_ascii_case("INBOX");
	}

	fn boxName_validate(boxName: &str) -> Result<(), ImapError>
	{
		if (boxName.is_empty() || boxName.len() > ImapLimits::BOX_NAME_MAXIMUM_BYTES)
		{
			return Err(ImapError::RESOURCE_LIMIT);
		}
		return Ok(());
	}

	fn mailKey_validate(mail: &ImapMailKey) -> Result<(), ImapError>
	{
		Self::boxName_validate(&mail.boxName)?;
		if (mail.uid == 0 || mail.uidValidity == 0)
		{
			return Err(ImapError::MAIL_NOT_FOUND);
		}
		return Ok(());
	}

	fn mailKeyValidity_require(mail: &ImapMailKey, uidValidity: Option<u32>) -> Result<(), ImapError>
	{
		if (uidValidity != Some(mail.uidValidity))
		{
			return Err(ImapError::MAIL_NOT_FOUND);
		}
		return Ok(());
	}
}

struct ImapMailParser;

impl ImapMailParser
{
	fn headers_get<T: Read + Write>(
		session: &mut Session<T>,
		results: HashSet<Uid>,
		budget: &mut ImapWorkBudget,
		mailboxIndex: usize,
	) -> Result<Vec<ImapMail>, ImapError>
	{
		let selectedUids = ImapUidSet::new(results.into_iter().collect());
		if (selectedUids.is_empty())
		{
			return Ok(Vec::new());
		}
		budget.active_require()
			.map_err(|error| error.trace("sync","header_budget",Some(mailboxIndex)))?;
		let query = format!(
			"(INTERNALDATE BODY.PEEK[HEADER.FIELDS (SUBJECT FROM TO)]<0.{}>)",
			ImapLimits::HEADER_MAXIMUM_BYTES + 1,
		);
		let fetches = session.uid_fetch(selectedUids.sequence_get(),query)
			.map_err(|error| ImapError::fromImapAt(error,"sync","header_fetch",Some(mailboxIndex)))?;
		let mut mails = Vec::new();
		for message in fetches.iter()
		{
			budget.active_require()
				.map_err(|error| error.trace("sync","header_parse_budget",Some(mailboxIndex)))?;
			let Some(uid) = message.uid.filter(|uid| selectedUids.contains(*uid))
			else
			{
				continue;
			};
			match Self::fromMessage(uid,message,None)
			{
				Ok(mail) => mails.push(mail),
				Err(ImapError::RESOURCE_LIMIT) => {},
				Err(error) => return Err(error.trace("sync","header_parse",Some(mailboxIndex))),
			}
		}
		return Ok(mails);
	}

	fn fromFetch(
		uid: u32,
		messages: Fetches,
		bodyMaximumBytes: Option<usize>,
	) -> Result<Option<ImapMail>, ImapError>
	{
		let message = messages.iter()
			.find(|message| message.uid == Some(uid));
		let Some(message) = message
		else
		{
			return Ok(None);
		};
		return Self::fromMessage(uid,message,bodyMaximumBytes).map(Some);
	}

	fn fromMessage(
		uid: u32,
		message: &Fetch,
		bodyMaximumBytes: Option<usize>,
	) -> Result<ImapMail, ImapError>
	{
		let mut mailData = ImapMail {
			uid,
			subject: None,
			..Default::default()
		};
		if let Some(header) = message.header()
		{
			if (header.len() > ImapLimits::HEADER_MAXIMUM_BYTES)
			{
				return Err(ImapError::RESOURCE_LIMIT);
			}
			if let Ok(parsed) = parse_mail(header)
			{
				mailData.subject = parsed.headers.get_first_value("Subject");
				mailData.from = parsed.headers.get_first_value("From").unwrap_or_default();
				mailData.to = parsed.headers.get_first_value("To").unwrap_or_default();
			}
		}
		if let Some(maximumBytes) = bodyMaximumBytes
		{
			if (message.size.is_some_and(|size| size as usize > maximumBytes))
			{
				return Err(ImapError::RESOURCE_LIMIT);
			}
			if let Some(body) = message.body()
			{
				if (body.len() > maximumBytes)
				{
					return Err(ImapError::RESOURCE_LIMIT);
				}
				if (!body.is_empty())
				{
					let parsed = parse_mail(body).map_err(|_| ImapError::MAIL_NOT_FOUND)?;
					mailData.subject = parsed.headers.get_first_value("Subject").or(mailData.subject);
					mailData.from = parsed.headers.get_first_value("From").unwrap_or(mailData.from);
					mailData.to = parsed.headers.get_first_value("To").unwrap_or(mailData.to);
					Self::bodyContent_apply(&mut mailData, &parsed)?;
					Self::content_finalize(&mut mailData)?;
				}
			}
		}
		if let Some(date) = message.internal_date()
		{
			mailData.date = date.timestamp();
		}
		return Ok(mailData);
	}

	fn bodyContent_apply(mailData: &mut ImapMail, root: &ParsedMail) -> Result<(), ImapError>
	{
		let mut pending = vec![root];
		while let Some(body) = pending.pop()
		{
			match body.ctype.mimetype.as_str()
			{
				"text/plain" =>
				{
					if mailData.content.is_none() && let Ok(text) = body.get_body()
					{
						mailData.content = ImapMailContentType::Text(text);
					}
				},
				"text/html" =>
				{
					if mailData.content.is_not_html() && let Ok(text) = body.get_body()
					{
						mailData.content = ImapMailContentType::Html(text);
					}
				},
				"multipart/alternative" | "multipart/mixed" | "multipart/related" =>
				{
					pending.extend(body.subparts.iter());
				},
				_ => Self::attachment_apply(mailData, body)?,
			}
		}
		return Ok(());
	}

	fn content_finalize(mailData: &mut ImapMail) -> Result<(), ImapError>
	{
		let content = std::mem::take(&mut mailData.content);
		mailData.content = match content
		{
			ImapMailContentType::Html(html) => ImapMailContentType::Html(Self::html_sanitize(&html,&mailData.parts)?),
			ImapMailContentType::Text(text) =>
			{
				if (text.len() > ImapLimits::MAIL_RENDERED_CONTENT_MAXIMUM_BYTES)
				{
					return Err(ImapError::RESOURCE_LIMIT);
				}
				ImapMailContentType::Text(text)
			},
			ImapMailContentType::None => ImapMailContentType::None,
		};
		mailData.parts.clear();
		return Ok(());
	}

	fn html_sanitize(html: &str, parts: &[Attachment]) -> Result<String, ImapError>
	{
		let inlineParts = Arc::new(Self::inlineParts_get(parts));
		let inlinePartsFilter = inlineParts.clone();
		let renderedInlineBytes = Arc::new(AtomicUsize::new(0));
		let renderedInlineBytesFilter = renderedInlineBytes.clone();
		let styleProperties = [
			"background", "background-color", "background-image", "background-position", "background-repeat", "background-size",
			"border", "border-bottom", "border-collapse", "border-color", "border-left", "border-radius", "border-right",
			"border-spacing", "border-style", "border-top", "border-width", "box-sizing", "color", "display", "font-family",
			"font-size", "font-style", "font-weight", "height", "line-height", "list-style-type", "margin", "margin-bottom",
			"margin-left", "margin-right", "margin-top", "max-height", "max-width", "min-height", "min-width", "overflow",
			"padding", "padding-bottom", "padding-left", "padding-right", "padding-top", "text-align", "text-decoration",
			"vertical-align", "white-space", "width", "word-break", "word-wrap",
		].into_iter().collect::<HashSet<_>>();
		let mut sanitizer = Builder::default();
		sanitizer
			.add_tags(&["tfoot"])
			.add_generic_attributes(&["dir","style"])
			.filter_style_properties(styleProperties)
			.add_url_schemes(&["cid","data"])
			.url_relative(UrlRelative::Deny)
			.set_tag_attribute_value("a","target","_blank")
			.attribute_filter(move |element,attribute,value| {
				if (element == "img" && attribute == "src")
				{
					if let Some(contentId) = Self::cidName_get(value)
					{
						let dataUrl = inlinePartsFilter.get(&contentId)?;
						let accepted = renderedInlineBytesFilter.fetch_update(
							Ordering::Relaxed,
							Ordering::Relaxed,
							|current| current.checked_add(dataUrl.len())
								.filter(|next| *next <= ImapLimits::MAIL_RENDERED_INLINE_MAXIMUM_BYTES),
						).is_ok();
						return accepted.then(|| Cow::Owned(dataUrl.clone()));
					}
					if (Self::dataImage_isAllowed(value))
					{
						return Some(Cow::Borrowed(value));
					}
				}
				if (Self::cidName_get(value).is_some())
				{
					return None;
				}
				if (value.trim_start().get(..5).is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:")))
				{
					return None;
				}
				return Some(Cow::Borrowed(value));
			});

		let sanitized = sanitizer.clean(html).to_string();
		if (sanitized.len() > ImapLimits::MAIL_RENDERED_CONTENT_MAXIMUM_BYTES)
		{
			return Err(ImapError::RESOURCE_LIMIT);
		}
		return Ok(sanitized);
	}

	fn inlineParts_get(parts: &[Attachment]) -> HashMap<String,String>
	{
		let mut result = HashMap::new();
		let mut encodedBytes = 0usize;
		for part in parts
		{
			let Some(contentId) = part.content_id.as_ref()
			else
			{
				continue;
			};
			let Some(contentType) = Self::inlineMime_get(&part.content_type)
			else
			{
				continue;
			};
			let encodedLength = part.data.len().saturating_add(2) / 3 * 4;
			let dataUrlLength = encodedLength.saturating_add(contentType.len()).saturating_add(13);
			if (encodedBytes.saturating_add(dataUrlLength) > ImapLimits::MAIL_RENDERED_INLINE_MAXIMUM_BYTES)
			{
				continue;
			}
			let contentId = contentId.trim().trim_matches(['<','>']).to_ascii_lowercase();
			if (contentId.is_empty())
			{
				continue;
			}
			let dataUrl = format!("data:{};base64,{}",contentType,Base64::encode_string(&part.data));
			encodedBytes = encodedBytes.saturating_add(dataUrl.len());
			result.entry(contentId).or_insert(dataUrl);
		}
		return result;
	}

	fn inlineMime_get(contentType: &str) -> Option<&'static str>
	{
		return match contentType.trim().to_ascii_lowercase().as_str()
		{
			"image/png" => Some("image/png"),
			"image/jpeg" | "image/jpg" => Some("image/jpeg"),
			"image/gif" => Some("image/gif"),
			"image/webp" => Some("image/webp"),
			"image/avif" => Some("image/avif"),
			"image/bmp" => Some("image/bmp"),
			"image/x-icon" | "image/vnd.microsoft.icon" => Some("image/x-icon"),
			_ => None,
		};
	}

	fn cidName_get(value: &str) -> Option<String>
	{
		let value = value.trim();
		if (!value.get(..4).is_some_and(|prefix| prefix.eq_ignore_ascii_case("cid:")))
		{
			return None;
		}
		let contentId = value.get(4..)?.trim().trim_matches(['<','>']);
		return (!contentId.is_empty()).then(|| contentId.to_ascii_lowercase());
	}

	fn dataImage_isAllowed(value: &str) -> bool
	{
		let value = value.trim_start();
		return [
			"data:image/png;base64,",
			"data:image/jpeg;base64,",
			"data:image/gif;base64,",
			"data:image/webp;base64,",
			"data:image/avif;base64,",
			"data:image/bmp;base64,",
			"data:image/x-icon;base64,",
		].iter().any(|prefix| {
			return value.get(..prefix.len()).is_some_and(|valuePrefix| valuePrefix.eq_ignore_ascii_case(prefix));
		});
	}

	fn attachment_apply(mailData: &mut ImapMail, body: &ParsedMail) -> Result<(), ImapError>
	{
		let disposition = body.get_content_disposition();
		let isAttachment = disposition.disposition == DispositionType::Attachment;
		let isInline = disposition.disposition == DispositionType::Inline;
		if (!isAttachment && !isInline)
		{
			return Ok(());
		}
		if (mailData.parts.len().saturating_add(mailData.attachement.len()) >= ImapLimits::ATTACHMENT_MAXIMUM)
		{
			return Err(ImapError::RESOURCE_LIMIT);
		}
		let Ok(data) = body.get_body_raw()
		else
		{
			return Ok(());
		};
		let filename = disposition.params.get("filename").cloned();
		let contentType = body.ctype.mimetype.clone();
		let contentId = body.headers.get_first_value("Content-ID")
			.map(|contentId| contentId.trim().trim_start_matches('<').trim_end_matches('>').to_string());
		if (filename.as_ref().is_some_and(|filename| {
				return filename.len() > ImapLimits::ATTACHMENT_FILENAME_MAXIMUM_BYTES
					|| filename.chars().any(char::is_control);
			})
			|| contentType.is_empty()
			|| contentType.len() > ImapLimits::ATTACHMENT_MIME_MAXIMUM_BYTES
			|| !contentType.is_ascii()
			|| contentType.chars().any(char::is_control)
			|| contentId.as_ref().is_some_and(|contentId| {
				return contentId.len() > ImapLimits::ATTACHMENT_CONTENT_ID_MAXIMUM_BYTES
					|| contentId.chars().any(char::is_control);
			}))
		{
			return Err(ImapError::RESOURCE_LIMIT);
		}
		let attachmentBytes = mailData.parts.iter().chain(mailData.attachement.iter())
			.fold(0usize, |total, attachment| total.saturating_add(attachment.data.len()));
		Self::attachmentSize_require(attachmentBytes,data.len())?;
		let attachment = Attachment {
			filename,
			content_type: contentType,
			content_id: contentId,
			data,
		};
		if (isInline)
		{
			let duplicate = attachment.content_id.as_ref().is_some_and(|contentId| {
				return mailData.parts.iter().any(|part| part.content_id.as_ref() == Some(contentId));
			});
			if (!duplicate)
			{
				mailData.parts.push(attachment);
			}
		}
		else
		{
			let duplicate = attachment.filename.as_ref().is_some_and(|filename| {
				return mailData.attachement.iter().any(|part| part.filename.as_ref() == Some(filename));
			});
			if (!duplicate)
			{
				mailData.attachement.push(attachment);
			}
		}
		return Ok(());
	}

	fn attachmentSize_require(currentBytes: usize, incomingBytes: usize) -> Result<(), ImapError>
	{
		if (incomingBytes > ImapLimits::ATTACHMENT_INDIVIDUAL_MAXIMUM_BYTES
			|| incomingBytes > ImapLimits::ATTACHMENT_TOTAL_MAXIMUM_BYTES.saturating_sub(currentBytes))
		{
			return Err(ImapError::RESOURCE_LIMIT);
		}
		return Ok(());
	}
}

#[cfg(test)]
mod tests
{
	use std::io::Cursor;
	use std::sync::{Arc,Mutex};

	use super::*;

	#[derive(Debug)]
	struct RecordingStream
	{
		responses: Cursor<Vec<u8>>,
		commands: Arc<Mutex<Vec<u8>>>,
	}

	impl RecordingStream
	{
		fn new(responses: Vec<u8>) -> (Self,Arc<Mutex<Vec<u8>>>)
		{
			let commands = Arc::new(Mutex::new(Vec::new()));
			return (Self {responses: Cursor::new(responses), commands: commands.clone()},commands);
		}
	}

	impl Read for RecordingStream
	{
		fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize>
		{
			return self.responses.read(buffer);
		}
	}

	impl Write for RecordingStream
	{
		fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize>
		{
			self.commands.lock().unwrap().extend_from_slice(buffer);
			return Ok(buffer.len());
		}

		fn flush(&mut self) -> std::io::Result<()>
		{
			return Ok(());
		}
	}

	struct MailboxSyncFixture;

	impl MailboxSyncFixture
	{
		fn fetchResponse_get(sequence: u32, uid: u32, subject: &str) -> String
		{
			let header = format!(
				"Subject: {}\r\nFrom: sender@example.com\r\nTo: receiver@example.com\r\n\r\n",
				subject,
			);
			return format!(
				"* {} FETCH (UID {} INTERNALDATE \"09-Aug-2026 10:00:00 +0000\" BODY[HEADER.FIELDS (SUBJECT FROM TO)] {{{}}}\r\n{})\r\n",
				sequence,
				uid,
				header.len(),
				header,
			);
		}

		fn responses_get() -> Vec<u8>
		{
			return format!(
				concat!(
					"a1 OK Logged in\r\n",
					"* FLAGS (\\Seen)\r\n",
					"* 2 EXISTS\r\n",
					"* 0 RECENT\r\n",
					"* OK [UIDVALIDITY 42] UIDs valid\r\n",
					"a2 OK [READ-WRITE] Select completed\r\n",
					"* SEARCH 7 8\r\n",
					"a3 OK Search completed\r\n",
					"{}{}",
					"a4 OK Fetch completed\r\n",
				),
				Self::fetchResponse_get(1,7,"Seven"),
				Self::fetchResponse_get(2,8,"Eight"),
			).into_bytes();
		}

		fn unchangedResponses_get() -> Vec<u8>
		{
			return concat!(
				"a1 OK Logged in\r\n",
				"* FLAGS (\\Seen)\r\n",
				"* 1 EXISTS\r\n",
				"* 0 RECENT\r\n",
				"* OK [UIDVALIDITY 42] UIDs valid\r\n",
				"a2 OK [READ-WRITE] Select completed\r\n",
				"* SEARCH 7\r\n",
				"a3 OK Search completed\r\n",
			).as_bytes().to_vec();
		}

		fn unselectableThenSelectableResponses_get() -> Vec<u8>
		{
			return concat!(
				"a1 OK Logged in\r\n",
				"a2 NO Mailbox is not selectable\r\n",
				"* FLAGS (\\Seen)\r\n",
				"* 1 EXISTS\r\n",
				"* 0 RECENT\r\n",
				"* OK [UIDVALIDITY 42] UIDs valid\r\n",
				"a3 OK [READ-WRITE] Select completed\r\n",
				"* SEARCH 7\r\n",
				"a4 OK Search completed\r\n",
			).as_bytes().to_vec();
		}

		fn seenResponses_get() -> Vec<u8>
		{
			return concat!(
				"a1 OK Logged in\r\n",
				"* FLAGS (\\Seen)\r\n",
				"* 1 EXISTS\r\n",
				"* 0 RECENT\r\n",
				"* OK [UIDVALIDITY 42] UIDs valid\r\n",
				"a2 OK [READ-WRITE] Select completed\r\n",
				"a3 OK Store completed\r\n",
			).as_bytes().to_vec();
		}

		fn legacyCommands_get() -> String
		{
			let headerQuery = format!(
				"(FLAGS INTERNALDATE BODY.PEEK[HEADER]<0.{}>)",
				ImapLimits::HEADER_MAXIMUM_BYTES + 1,
			);
			return format!(
				concat!(
					"a2 SELECT Alerts\r\n",
					"a3 UID SEARCH UNSEEN UNKEYWORD $Junk UNKEYWORD $Spam UNDELETED UNANSWERED UNDRAFT SINCE 09-Aug-2026\r\n",
					"a4 UID FETCH 7 {}\r\n",
					"a5 UID FETCH 8 {}\r\n",
					"a6 UID FETCH 7,8 (FLAGS)\r\n",
				),
				headerQuery,
				headerQuery,
			);
		}
	}

	#[test]
	fn workBudget_tracksOnlyMostRecentUidsWithinGlobalLimit()
	{
		let mut budget = ImapWorkBudget::new();
		let results = (1..=150).collect::<HashSet<_>>();
		let uids = ImapUidSet::new(budget.trackedUids_get(results).unwrap().into_iter().collect());
		assert_eq!(uids.uids.len(), ImapLimits::MAIL_HEADER_MAXIMUM);
		assert!(uids.contains(150));
		assert!(uids.contains(51));
		assert!(!uids.contains(50));
		assert_eq!(budget.remainingHeaders,0);
	}

	#[test]
	fn workBudget_rejectsExpiredOperation()
	{
		let budget = ImapWorkBudget { deadline: Instant::now() - Duration::from_millis(1), remainingHeaders: 1 };
		assert_eq!(budget.active_require().unwrap_err(), ImapError::RESOURCE_LIMIT);
	}

	#[test]
	fn uidSet_compactsConsecutiveValues()
	{
		let uids = ImapUidSet::new(vec![10,2,1,3,8,9,5,5]);
		assert_eq!(uids.sequence_get(),"1:3,5,8:10");
	}

	#[test]
	fn mailboxDelta_rediscoversOldUnreadAndScopesRemovedUids()
	{
		let state = ImapMailboxSyncState {
			boxName: "Alerts".to_string(),
			uidValidity: Some(42),
			knownUids: vec![7,9],
		};
		let delta = ImapMailboxDelta::new(Some(&state),42,[7,8].into_iter().collect());
		assert_eq!(delta.newUids,[8].into_iter().collect());
		assert_eq!(delta.removedUids,vec![9]);

		let changedValidity = ImapMailboxDelta::new(Some(&state),43,[7].into_iter().collect());
		assert_eq!(changedValidity.newUids,[7].into_iter().collect());
		assert!(changedValidity.removedUids.is_empty());
	}

	#[test]
	fn mailboxSync_batchesHeadersAndBeatsLegacyBudget()
	{
		let (stream,commands) = RecordingStream::new(MailboxSyncFixture::responses_get());
		let client = imap::Client::new(stream);
		let mut session = client.login("user","password").unwrap();
		commands.lock().unwrap().clear();
		let state = ImapMailboxSyncState {
			boxName: "Alerts".to_string(),
			uidValidity: None,
			knownUids: Vec::new(),
		};
		let result = ImapProxy::mailboxSync_get(&mut session,&state,&mut ImapWorkBudget::new(),0).unwrap().unwrap();
		let commands = String::from_utf8(commands.lock().unwrap().clone()).unwrap();

		assert_eq!(result.uidValidity,42);
		assert_eq!(result.mails.len(),2);
		assert_eq!(commands.lines().count(),3);
		assert_eq!(commands.matches("UID FETCH").count(),1);
		assert!(commands.contains("UID FETCH 7:8"));
		assert!(commands.contains("HEADER.FIELDS (SUBJECT FROM TO)"));
		assert!(!commands.contains("LIST"));
		assert!(!commands.contains("FLAGS"));
		assert!(!commands.contains("SINCE"));
		let legacyCommands = MailboxSyncFixture::legacyCommands_get();
		assert_eq!(legacyCommands.lines().count(),5);
		assert!(commands.len() < legacyCommands.len());
	}

	#[test]
	fn mailboxSyncWithoutNewUidUsesOnlySelectAndSearch()
	{
		let (stream,commands) = RecordingStream::new(MailboxSyncFixture::unchangedResponses_get());
		let client = imap::Client::new(stream);
		let mut session = client.login("user","password").unwrap();
		commands.lock().unwrap().clear();
		let state = ImapMailboxSyncState {
			boxName: "Alerts".to_string(),
			uidValidity: Some(42),
			knownUids: vec![7],
		};

		let result = ImapProxy::mailboxSync_get(&mut session,&state,&mut ImapWorkBudget::new(),0).unwrap().unwrap();
		let commands = String::from_utf8(commands.lock().unwrap().clone()).unwrap();

		assert!(result.mails.is_empty());
		assert!(result.removedUids.is_empty());
		assert_eq!(commands.lines().count(),2);
		assert!(!commands.contains("UID FETCH"));
	}

	#[test]
	fn mailboxSyncSkipsMailboxRejectedBySelectAndContinuesSession()
	{
		Htrace::htracer::HTracer::globalContext_set(Htrace::components::context::Context::default());
		let (stream,commands) = RecordingStream::new(MailboxSyncFixture::unselectableThenSelectableResponses_get());
		let client = imap::Client::new(stream);
		let mut session = client.login("user","password").unwrap();
		commands.lock().unwrap().clear();
		let rejectedState = ImapMailboxSyncState {
			boxName: "Container".to_string(),
			uidValidity: None,
			knownUids: Vec::new(),
		};
		let acceptedState = ImapMailboxSyncState {
			boxName: "Alerts".to_string(),
			uidValidity: Some(42),
			knownUids: vec![7],
		};

		let rejected = ImapProxy::mailboxSync_get(&mut session,&rejectedState,&mut ImapWorkBudget::new(),0).unwrap();
		let accepted = ImapProxy::mailboxSync_get(&mut session,&acceptedState,&mut ImapWorkBudget::new(),1).unwrap();
		let commands = String::from_utf8(commands.lock().unwrap().clone()).unwrap();

		assert!(rejected.is_none());
		assert!(accepted.is_some());
		assert_eq!(commands.lines().count(),3);
		assert!(commands.contains("SELECT Container") || commands.contains("SELECT \"Container\""),"{}",commands);
		assert!(commands.contains("SELECT Alerts") || commands.contains("SELECT \"Alerts\""),"{}",commands);
		assert_eq!(commands.matches("UID SEARCH").count(),1);
		assert!(!commands.contains("UID FETCH"));
	}

	#[test]
	fn mailSeenStoreUsesMailboxUidValidityAndSilentFlagUpdate()
	{
		let (stream,commands) = RecordingStream::new(MailboxSyncFixture::seenResponses_get());
		let client = imap::Client::new(stream);
		let mut session = client.login("user","password").unwrap();
		commands.lock().unwrap().clear();
		let key = ImapMailKey {boxName: "Alerts".to_string(),uidValidity: 42,uid: 7};

		ImapProxy::mailSeenInSession_set(&mut session,&key,&ImapWorkBudget::new()).unwrap();
		let commands = String::from_utf8(commands.lock().unwrap().clone()).unwrap();

		assert_eq!(commands.lines().count(),2);
		assert!(commands.contains("SELECT Alerts") || commands.contains("SELECT \"Alerts\""),"{}",commands);
		assert!(commands.contains("UID STORE 7 +FLAGS.SILENT (\\Seen)"),"{}",commands);
	}

	#[test]
	fn mailSeenStoreRejectsChangedUidValidityBeforeStore()
	{
		let mut responses = MailboxSyncFixture::seenResponses_get();
		let storeResponseStart = responses.windows("a3 OK Store completed\r\n".len())
			.position(|window| window == b"a3 OK Store completed\r\n")
			.unwrap();
		responses.truncate(storeResponseStart);
		let (stream,commands) = RecordingStream::new(responses);
		let client = imap::Client::new(stream);
		let mut session = client.login("user","password").unwrap();
		commands.lock().unwrap().clear();
		let key = ImapMailKey {boxName: "Alerts".to_string(),uidValidity: 41,uid: 7};

		let error = ImapProxy::mailSeenInSession_set(&mut session,&key,&ImapWorkBudget::new()).unwrap_err();
		let commands = String::from_utf8(commands.lock().unwrap().clone()).unwrap();

		assert_eq!(error,ImapError::MAIL_NOT_FOUND);
		assert_eq!(commands.lines().count(),1);
		assert!(!commands.contains("UID STORE"));
	}

	#[test]
	fn mailParser_extractsTextWithoutRecursiveTraversal()
	{
		let raw = concat!(
			"Content-Type: multipart/alternative; boundary=test\r\n\r\n",
			"--test\r\nContent-Type: text/plain\r\n\r\nplain body\r\n",
			"--test\r\nContent-Type: text/html\r\n\r\n<b>html body</b>\r\n",
			"--test--\r\n",
		);
		let parsed = parse_mail(raw.as_bytes()).unwrap();
		let mut mail = ImapMail::default();
		ImapMailParser::bodyContent_apply(&mut mail, &parsed).unwrap();
		assert!(matches!(mail.content, ImapMailContentType::Html(ref content) if content.contains("html body")));
	}

	#[test]
	fn mailParserSanitizesHtmlAndForcesSafeLinks()
	{
		let html = concat!(
			"<script>alert(1)</script>",
			"<img src=\"https://tracker.example/pixel\" onerror=\"alert(2)\">",
			"<a href=\"javascript:alert(3)\" target=\"_self\">bad</a>",
			"<a href=\"https://example.com\">safe</a>",
			"<div style=\"position:fixed;color:red\">text</div>",
		);

		let sanitized = ImapMailParser::html_sanitize(html,&[]).unwrap();

		assert!(!sanitized.contains("script"));
		assert!(!sanitized.contains("onerror"));
		assert!(!sanitized.contains("javascript:"));
		assert!(!sanitized.contains("position"));
		assert!(sanitized.contains("https://tracker.example/pixel"));
		assert!(sanitized.contains("href=\"https://example.com\""));
		assert!(sanitized.contains("target=\"_blank\""));
		assert!(sanitized.contains("rel=\"noopener noreferrer\""));
		assert!(sanitized.contains("color:red") || sanitized.contains("color: red"));
	}

	#[test]
	fn mailParserReplacesSafeCidAndDropsInlineBinaryFromContract()
	{
		let mut mail = ImapMail {
			content: ImapMailContentType::Html("<img src=\"CID:Logo\">".to_string()),
			parts: vec![Attachment {
				filename: None,
				content_type: "image/png".to_string(),
				content_id: Some("logo".to_string()),
				data: vec![1,2,3],
			}],
			..Default::default()
		};

		ImapMailParser::content_finalize(&mut mail).unwrap();

		assert!(mail.parts.is_empty());
		assert!(matches!(mail.content,ImapMailContentType::Html(ref html)
			if html.contains("src=\"data:image/png;base64,AQID\"")));
	}

	#[test]
	fn mailParserRejectsExecutableCidMimeAndRelativeImages()
	{
		let parts = vec![Attachment {
			filename: None,
			content_type: "image/svg+xml".to_string(),
			content_id: Some("vector".to_string()),
			data: b"<svg onload='alert(1)'></svg>".to_vec(),
		}];

		let sanitized = ImapMailParser::html_sanitize(
			"<img src=\"cid:vector\"><img src=\"/local/session/path\">",
			&parts,
		).unwrap();

		assert!(!sanitized.contains("image/svg"));
		assert!(!sanitized.contains("cid:"));
		assert!(!sanitized.contains("/local/session/path"));
	}

	#[test]
	fn mailParserRejectsOversizedAttachmentMetadata()
	{
		let raw = format!(
			"Content-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"{}\"\r\n\r\ndata",
			"x".repeat(ImapLimits::ATTACHMENT_FILENAME_MAXIMUM_BYTES + 1),
		);
		let parsed = parse_mail(raw.as_bytes()).unwrap();
		let mut mail = ImapMail::default();

		assert_eq!(ImapMailParser::attachment_apply(&mut mail,&parsed).unwrap_err(),ImapError::RESOURCE_LIMIT);
	}

	#[test]
	fn mailParserRejectsOversizedAttachmentPayloadWithoutAllocatingIt()
	{
		assert!(ImapMailParser::attachmentSize_require(0,ImapLimits::ATTACHMENT_INDIVIDUAL_MAXIMUM_BYTES).is_ok());
		assert_eq!(
			ImapMailParser::attachmentSize_require(0,ImapLimits::ATTACHMENT_INDIVIDUAL_MAXIMUM_BYTES + 1).unwrap_err(),
			ImapError::RESOURCE_LIMIT,
		);
		assert_eq!(
			ImapMailParser::attachmentSize_require(ImapLimits::ATTACHMENT_TOTAL_MAXIMUM_BYTES,1).unwrap_err(),
			ImapError::RESOURCE_LIMIT,
		);
	}

	#[test]
	fn proxyConstructor_rejectsOversizedCredentials()
	{
		let mut config = imap_connector::default();
		config.password = "x".repeat(ImapLimits::CREDENTIAL_MAXIMUM_BYTES + 1);
		let destination = ValidatedImapDestination::test_get(
			"imap.example.com".to_string(),
			vec!["8.8.8.8:993".parse().unwrap()],
		);
		assert!(matches!(ImapProxy::new(config, destination), Err(ImapError::RESOURCE_LIMIT)));
	}

	#[test]
	fn proxyRecognizesGmailInboxCaseInsensitively()
	{
		assert!(ImapProxy::mailbox_isAutomaticallyExcluded("INBOX",true));
		assert!(ImapProxy::mailbox_isAutomaticallyExcluded("Inbox",true));
		assert!(!ImapProxy::mailbox_isAutomaticallyExcluded("INBOX",false));
		assert!(!ImapProxy::mailbox_isAutomaticallyExcluded("Alerts",true));
	}

	#[test]
	fn syncSkipsConnectionForExplicitEmptySelection()
	{
		let mut config = imap_connector::default();
		config.extra = Some(crate::api::proxys::imap_components::imap_connector_extra {
			boxAllowList: Some(Vec::new()),
			..Default::default()
		});
		let destination = ValidatedImapDestination::test_get(
			"imap.example.com".to_string(),
			vec!["8.8.8.8:993".parse().unwrap()],
		);
		let proxy = ImapProxy::new(config,destination).unwrap();

		assert!(proxy.sync_get(ImapSyncRequest::default()).unwrap().is_empty());
	}

	#[test]
	fn proxyConstructor_rejectsTooManySelectedMailboxes()
	{
		let mut config = imap_connector::default();
		config.extra = Some(crate::api::proxys::imap_components::imap_connector_extra {
			boxAllowList: Some((0..=ImapLimits::MAILBOX_MAXIMUM).map(|index| format!("Box{}",index)).collect()),
			..Default::default()
		});
		let destination = ValidatedImapDestination::test_get(
			"imap.example.com".to_string(),
			vec!["8.8.8.8:993".parse().unwrap()],
		);

		assert!(matches!(ImapProxy::new(config,destination),Err(ImapError::RESOURCE_LIMIT)));
	}
}
