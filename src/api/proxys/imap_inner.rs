use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use imap::types::{Fetches, Uid};
use imap::{Connection, Session};
use mailparse::{parse_mail, DispositionType, MailHeaderMap, ParsedMail};
use time::format_description;

use crate::api::proxys::imap_components::{
	imap_connector,
	Attachment,
	Attributs,
	BoxName,
	ImapMail,
	ImapMailContentType,
	ImapMailIdentifier,
	ImapMailUpdate,
};
use crate::api::proxys::imap_error::ImapError;
use crate::api::proxys::outbound_policy::ValidatedImapDestination;

struct ImapLimits;

impl ImapLimits
{
	const ATTACHMENT_MAXIMUM: usize = 64;
	const ATTACHMENT_TOTAL_MAXIMUM_BYTES: usize = 16 * 1024 * 1024;
	const BLACKLIST_ENTRY_MAXIMUM: usize = 128;
	const BOX_NAME_MAXIMUM_BYTES: usize = 1_024;
	const CREDENTIAL_MAXIMUM_BYTES: usize = 4_096;
	const FLAG_MAXIMUM: usize = 64;
	const FLAG_MAXIMUM_BYTES: usize = 256;
	const FLAG_UPDATE_MAXIMUM: usize = 500;
	const HEADER_MAXIMUM_BYTES: usize = 64 * 1024;
	const MAILBOX_MAXIMUM: usize = 64;
	const MAIL_CONTENT_MAXIMUM_BYTES: usize = 16 * 1024 * 1024;
	const MAIL_HEADER_MAXIMUM: usize = 100;
	const OPERATION_TIMEOUT: Duration = Duration::from_secs(90);
}

struct ImapWorkBudget
{
	deadline: Instant,
	remainingHeaders: usize,
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

	fn headerUids_get(&mut self, results: HashSet<Uid>) -> Result<Vec<Uid>, ImapError>
	{
		self.active_require()?;
		let mut uids = results.into_iter().collect::<Vec<_>>();
		uids.sort_unstable_by(|left, right| right.cmp(left));
		uids.truncate(self.remainingHeaders);
		self.remainingHeaders = self.remainingHeaders.saturating_sub(uids.len());
		return Ok(uids);
	}

	fn headersAvailable_get(&self) -> bool
	{
		return self.remainingHeaders > 0;
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
		let (mut session, isGmail) = self.connection_get()?;
		let result = self.mailboxes_get(&mut session, isGmail);
		let _ = session.logout();
		return result;
	}

	pub(super) fn fullUnseen_get(self) -> Result<Vec<ImapMail>, ImapError>
	{
		let mut budget = ImapWorkBudget::new();
		let (mut session, isGmail) = self.connection_get()?;
		let mailboxes = self.mailboxes_get(&mut session, isGmail)?;
		let mut mails = Vec::new();
		for mailbox in mailboxes
		{
			budget.active_require()?;
			if (!budget.headersAvailable_get())
			{
				break;
			}
			if (mailbox.attributes.is_uninteresting() || self.config.isBoxBlacklisted(&mailbox.name))
			{
				continue;
			}
			if (session.select(&mailbox.name).is_err())
			{
				continue;
			}
			let Ok(results) = session.uid_search("UNSEEN UNKEYWORD $Junk UNKEYWORD $Spam UNDELETED UNANSWERED UNDRAFT")
			else
			{
				continue;
			};
			mails.append(&mut ImapMailParser::headers_get(&mut session, results, &mailbox.name, &mut budget)?);
		}
		let _ = session.logout();
		return Ok(mails);
	}

	pub(super) fn unseenSince_get(self, date: u64, toUpdate: Vec<u32>) -> Result<(Vec<ImapMail>, HashMap<u32,ImapMailUpdate>), ImapError>
	{
		let mut toUpdate = toUpdate;
		toUpdate.sort_unstable_by(|left, right| right.cmp(left));
		toUpdate.truncate(ImapLimits::FLAG_UPDATE_MAXIMUM);
		let date = i64::try_from(date)
			.ok()
			.and_then(|date| time::UtcDateTime::from_unix_timestamp(date).ok())
			.ok_or(ImapError::INVALID_DATE)?;
		let format = format_description::parse("[day padding:zero]-[month repr:short]-[year]")
			.map_err(|_| ImapError::INVALID_DATE)?;
		let dateFormatted = date.format(&format).map_err(|_| ImapError::INVALID_DATE)?;
		let allUid = toUpdate.iter().map(|uid| uid.to_string()).collect::<Vec<_>>().join(",");

		let mut budget = ImapWorkBudget::new();
		let (mut session, isGmail) = self.connection_get()?;
		let mailboxes = self.mailboxes_get(&mut session, isGmail)?;
		let mut mails = Vec::new();
		let mut updatedMails = HashMap::new();
		for mailbox in mailboxes
		{
			budget.active_require()?;
			if (mailbox.attributes.is_uninteresting() || self.config.isBoxBlacklisted(&mailbox.name))
			{
				continue;
			}
			if (session.select(&mailbox.name).is_err())
			{
				continue;
			}
			if (budget.headersAvailable_get())
			{
				let query = format!(
					"UNSEEN UNKEYWORD $Junk UNKEYWORD $Spam UNDELETED UNANSWERED UNDRAFT SINCE {}",
					dateFormatted,
				);
				if let Ok(results) = session.uid_search(query)
				{
					mails.append(&mut ImapMailParser::headers_get(&mut session, results, &mailbox.name, &mut budget)?);
				}
			}
			if (!allUid.is_empty())
			{
				let Ok(results) = session.uid_fetch(&allUid, "(FLAGS)")
				else
				{
					continue;
				};
				for mail in results.iter()
				{
					budget.active_require()?;
					let Some(uid) = mail.uid
					else
					{
						continue;
					};
					let flags = mail.flags().iter()
						.map(|flag| flag.to_string().replace('\\', "").to_uppercase())
						.filter(|flag| flag.len() <= ImapLimits::FLAG_MAXIMUM_BYTES)
						.take(ImapLimits::FLAG_MAXIMUM)
						.collect::<Vec<_>>();
					updatedMails.insert(uid, ImapMailUpdate { flags, boxName: mailbox.clone() });
				}
			}
		}
		let _ = session.logout();
		return Ok((mails, updatedMails));
	}

	pub(super) fn mailContent_get(self, mail: ImapMailIdentifier) -> Result<ImapMail, ImapError>
	{
		Self::boxName_validate(&mail.boxName)?;
		let budget = ImapWorkBudget::new();
		let (mut session, _) = self.connection_get()?;
		budget.active_require()?;
		session.select(&mail.boxName)?;
		let query = format!(
			"(FLAGS INTERNALDATE RFC822.SIZE BODY.PEEK[]<0.{}>)",
			ImapLimits::MAIL_CONTENT_MAXIMUM_BYTES + 1,
		);
		let results = session.uid_fetch(mail.uid.to_string(), query)
			.map_err(|_| ImapError::MAIL_NOT_FOUND)?;
		let mail = ImapMailParser::fromFetch(
			mail.uid,
			results,
			&mail.boxName,
			Some(ImapLimits::MAIL_CONTENT_MAXIMUM_BYTES),
		)?.ok_or(ImapError::MAIL_NOT_FOUND)?;
		let _ = session.logout();
		return Ok(mail);
	}

	pub(super) fn mailSeen_set(self, mail: ImapMailIdentifier) -> Result<(), ImapError>
	{
		Self::boxName_validate(&mail.boxName)?;
		let budget = ImapWorkBudget::new();
		let (mut session, _) = self.connection_get()?;
		budget.active_require()?;
		session.select(&mail.boxName)?;
		session.uid_store(mail.uid.to_string(), "+FLAGS (\\Seen)")?;
		let _ = session.logout();
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
				|| extra.boxBlackList.iter().chain(extra.flagBlackList.iter())
					.any(|entry| entry.len() > ImapLimits::BOX_NAME_MAXIMUM_BYTES))
			{
				return Err(ImapError::RESOURCE_LIMIT);
			}
		}
		return Ok(());
	}

	fn connection_get(&self) -> Result<(Session<Connection>, bool), ImapError>
	{
		let isGmail = self.config.isGmail();
		let mut client = imap::Client::new(self.destination.connection_get()?);
		client.read_greeting()?;
		let session = client
			.login(self.config.username.clone(), self.config.password.clone())
			.map_err(|error| error.0)?;
		return Ok((session, isGmail));
	}

	fn mailboxes_get(&self, session: &mut Session<Connection>, isGmail: bool) -> Result<Vec<BoxName>, ImapError>
	{
		let names = session.list(None, Some("*"))?;
		let mut mailboxes = Vec::new();
		for result in names.iter()
		{
			if (isGmail && result.name() == "INBOX")
			{
				continue;
			}
			Self::boxName_validate(result.name())?;
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

	fn boxName_validate(boxName: &str) -> Result<(), ImapError>
	{
		if (boxName.is_empty() || boxName.len() > ImapLimits::BOX_NAME_MAXIMUM_BYTES)
		{
			return Err(ImapError::RESOURCE_LIMIT);
		}
		return Ok(());
	}
}

struct ImapMailParser;

impl ImapMailParser
{
	fn headers_get(
		session: &mut Session<Connection>,
		results: HashSet<Uid>,
		boxName: &str,
		budget: &mut ImapWorkBudget,
	) -> Result<Vec<ImapMail>, ImapError>
	{
		let mut mails = Vec::new();
		for uid in budget.headerUids_get(results)?
		{
			budget.active_require()?;
			let query = format!(
				"(FLAGS INTERNALDATE BODY.PEEK[HEADER]<0.{}>)",
				ImapLimits::HEADER_MAXIMUM_BYTES + 1,
			);
			let Ok(fetches) = session.uid_fetch(uid.to_string(), query)
			else
			{
				continue;
			};
			match Self::fromFetch(uid, fetches, boxName, None)
			{
				Ok(Some(mail)) => mails.push(mail),
				Ok(None) | Err(ImapError::RESOURCE_LIMIT) => {},
				Err(error) => return Err(error),
			}
		}
		return Ok(mails);
	}

	fn fromFetch(
		uid: u32,
		messages: Fetches,
		boxName: &str,
		bodyMaximumBytes: Option<usize>,
	) -> Result<Option<ImapMail>, ImapError>
	{
		let mut mailData = ImapMail {
			uid,
			subject: None,
			boxName: boxName.to_string(),
			..Default::default()
		};
		let mut found = false;
		for message in messages.iter()
		{
			found = true;
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
					}
				}
			}
			if let Some(date) = message.internal_date()
			{
				mailData.date = date.timestamp();
			}
		}
		if (!found)
		{
			return Ok(None);
		}
		return Ok(Some(mailData));
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
		let attachmentBytes = mailData.parts.iter().chain(mailData.attachement.iter())
			.fold(0usize, |total, attachment| total.saturating_add(attachment.data.len()));
		if (data.len() > ImapLimits::ATTACHMENT_TOTAL_MAXIMUM_BYTES.saturating_sub(attachmentBytes))
		{
			return Err(ImapError::RESOURCE_LIMIT);
		}
		let attachment = Attachment {
			filename: disposition.params.get("filename").cloned(),
			content_type: body.ctype.mimetype.clone(),
			content_id: body.headers.get_first_value("Content-ID")
				.map(|contentId| contentId.trim().trim_start_matches('<').trim_end_matches('>').to_string()),
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
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn workBudget_keepsMostRecentUidsWithinLimit()
	{
		let mut budget = ImapWorkBudget::new();
		let results = (1..=150).collect::<HashSet<_>>();
		let uids = budget.headerUids_get(results).unwrap();
		assert_eq!(uids.len(), ImapLimits::MAIL_HEADER_MAXIMUM);
		assert_eq!(uids.first(), Some(&150));
		assert_eq!(uids.last(), Some(&51));
		assert!(!budget.headersAvailable_get());
	}

	#[test]
	fn workBudget_rejectsExpiredOperation()
	{
		let budget = ImapWorkBudget { deadline: Instant::now() - Duration::from_millis(1), remainingHeaders: 1 };
		assert_eq!(budget.active_require().unwrap_err(), ImapError::RESOURCE_LIMIT);
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
}
