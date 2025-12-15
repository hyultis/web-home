use leptos::server;
use crate::api::proxys::imap_components::{imap_connector, ImapMail, BoxName, ImapMailIdentifier};
use crate::api::proxys::imap_error::ImapError;

#[server]
pub async fn API_proxys_imap_listbox(config: imap_connector) -> Result<Vec<BoxName>, ImapError>
{
	use crate::api::proxys::imap_inner::*;

	let (mut imap_session,_) = connect_imap(config)?;
	let results = listbox(&mut imap_session)?;

	return Ok(results);
}

// get all mail UNSEE
#[server]
pub async fn API_proxys_imap_getFullUnsee(config: imap_connector) -> Result<Vec<ImapMail>, ImapError>
{
	use crate::api::proxys::imap_inner::*;

	let (mut imap_session,_) = connect_imap(config)?;
	let results = listbox(&mut imap_session)?;

	let mut listOfMail = vec![];
	for boxName in results
	{
		if(boxName.attributes.is_uninteresting()) {continue};

		let Ok(mailbox) = imap_session.select(&boxName.name) else {continue};
		let Ok(results) = imap_session.uid_search("UNSEEN UNKEYWORD $Junk UNKEYWORD $Spam UNDELETED UNANSWERED UNDRAFT") else {continue};

		listOfMail.append(&mut extract_ImapMail_from_search(&mut imap_session,results,&boxName.name));
	}

	let _ = imap_session.logout();

	return Ok(listOfMail);
}

// get all mail from the most recent mail
#[server]
pub async fn API_proxys_imap_getUnseeSince(config: imap_connector, date:u64) -> Result<Vec<ImapMail>, ImapError>
{
	use crate::api::proxys::imap_inner::*;
	use time::format_description;

	let (mut imap_session,_) = connect_imap(config)?;
	let results = listbox(&mut imap_session)?;

	let mut listOfMail = vec![];
	for boxName in results
	{
		if(boxName.attributes.is_uninteresting()) {continue};

		let Ok(mailbox) = imap_session.select(&boxName.name) else {continue};

		let Ok(date) = time::UtcDateTime::from_unix_timestamp(date as i64) else {return Err(ImapError::INVALID_DATE)};
		let format = format_description::parse("[day padding:zero]-[month repr:short]-[year]").unwrap();
		let Ok(dateFormatted) = date.format(&format) else {return Err(ImapError::INVALID_DATE)};

		let Ok(results) = imap_session.uid_search(format!("UNSEEN UNKEYWORD $Junk UNKEYWORD $Spam UNDELETED UNANSWERED UNDRAFT SINCE {}",dateFormatted)) else {continue};

		listOfMail.append(&mut extract_ImapMail_from_search(&mut imap_session,results,&boxName.name));
	}

	let _ = imap_session.logout();

	return Ok(listOfMail);
}

#[server]
pub async fn API_proxys_imap_getMailContent(config: imap_connector, mail: ImapMailIdentifier) -> Result<ImapMail, ImapError>
{
	use crate::api::proxys::imap_inner::*;

	let (mut imap_session,_) = connect_imap(config)?;
	imap_session.select(&mail.boxName)?;
	let Ok(results) = imap_session.uid_fetch(&mail.uid.to_string(), "(FLAGS INTERNALDATE BODY.PEEK[])") else {return Err(ImapError::MAIL_NOT_FOUND)};
	let Some(mail) = extract_ImapMail_from_fetch(&mut imap_session,mail.uid,results,&mail.boxName).into_iter().next() else {return Err(ImapError::MAIL_NOT_FOUND)};

	return Ok(mail);
}