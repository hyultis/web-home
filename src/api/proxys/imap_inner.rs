use std::collections::HashSet;
use Htrace::HTrace;
use crate::api::proxys::imap_components::{imap_connector, Attributs, BoxName, ImapMail};
use crate::api::proxys::imap_error::ImapError;
use imap::types::{Mailbox, Uid};
use imap::{Connection, Session};
use mailparse::{parse_mail, MailHeaderMap};

pub fn connect_imap(config: imap_connector) -> Result<(Session<Connection>, Mailbox), ImapError>
{
	let client = imap::ClientBuilder::new(config.host, config.port).connect()?; // /imap/ssl

	let mut imap_session = client
		.login(config.username, config.password)
		.map_err(|e| e.0)?;

	let mailbox = imap_session.select("INBOX")?;

	return Ok((imap_session, mailbox));
}

pub fn listbox(imap_session: &mut Session<Connection>) -> Result<Vec<BoxName>, ImapError>
{
	let mut returning = Vec::new();
	let results = imap_session.list(None, Some("*"));
	if let Ok(names) = results
	{
		for result in names.iter()
		{
			let mut attributs = Attributs::default();
			result
				.attributes()
				.iter()
				.for_each(|attribute| attributs.add(attribute));

			let name = BoxName {
				name: result.name().to_string(),
				attributes: attributs,
			};
			returning.push(name);
		}
	}

	return Ok(returning);
}

pub fn extract_ImapMail_from_search(imap_session: &mut Session<Connection>,results: HashSet<Uid>,boxName: &String) -> Vec<ImapMail>
{
	let mut listOfMail = vec![];
	for result in results.into_iter()
	{
		//HTrace!("try to fetch : {}",result);
		let Ok(message) = imap_session.uid_fetch(&result.to_string(), "(FLAGS INTERNALDATE BODY.PEEK[HEADER])") else {continue};
		//HTrace!("number of submessage : {}",message.len());

		let Some(message) = message.iter().next() else {continue};
		let mut mailData = ImapMail{
			uid: result,
			subject: None,
			boxName: boxName.clone(),
			..Default::default()
		};
		HTrace!("flags : {:?}",message.flags());
		let Some(body) = message.header() else {continue};
		let Ok(parsed) = parse_mail(body) else {continue};

		if let Some(subject) = parsed.headers.get_first_value("Subject") {
			mailData.subject = Some(subject);
		}

		if let Some(from) = parsed.headers.get_first_value("From") {
			mailData.from = from;
		}

		if let Some(to) = parsed.headers.get_first_value("To") {
			mailData.to = to;
		}

		//HTrace!("body : {}",parsed.subparts.iter().next().unwrap().get_body().unwrap());

		if let Some(date) = message.internal_date() {
			mailData.date = date.timestamp();
		}

		listOfMail.push(mailData);
	}
	return listOfMail;
}