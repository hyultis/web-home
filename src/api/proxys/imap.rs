use leptos::server;
use crate::api::proxys::imap_components::{imap_connector, ImapMail, BoxName};
use crate::api::proxys::imap_error::ImapError;

#[server]
pub async fn API_proxys_imap_listbox(config: imap_connector) -> Result<Vec<BoxName>, ImapError>
{
	use crate::api::proxys::imap_inner::*;


	// the client we have here is unauthenticated.
	// to do anything useful with the e-mails, we need to log in
	let (mut imap_session,_) = connect_imap(config)?;
	let results = listbox(&mut imap_session)?;

	return Ok(results);
}

#[server]
pub async fn API_proxys_imap_getUnsee(config: imap_connector) -> Result<Vec<ImapMail>, ImapError>
{
	use Htrace::HTrace;
	use crate::api::proxys::imap_inner::*;
	use mailparse::parse_mail;
	use mailparse::MailHeaderMap;

	let (mut imap_session,_) = connect_imap(config)?;
	let results = listbox(&mut imap_session)?;

	let mut listOfMail = vec![];
	for boxName in results
	{
		if(boxName.attributes.is_uninteresting()) {continue};

		let Ok(mailbox) = imap_session.select(&boxName.name) else {continue};
		HTrace!("on BOX : {}",boxName.name);

		let Ok(results) = imap_session.uid_search("UNSEEN UNKEYWORD $Junk UNKEYWORD $Spam UNDELETED UNANSWERED UNDRAFT") else {continue};
		HTrace!("number of result : {}",results.len());

		for result in results.into_iter()
		{
			//HTrace!("try to fetch : {}",result);
			let Ok(message) = imap_session.uid_fetch(&result.to_string(), "(FLAGS INTERNALDATE BODY.PEEK[HEADER])") else {continue};
			//HTrace!("number of submessage : {}",message.len());

			let Some(message) = message.iter().next() else {continue};
			let mut mailData = ImapMail{
				uid: result,
				subject: "<NO_TITLE>".to_string(),
				..Default::default()
			};
			HTrace!("flags : {:?}",message.flags());
			let Some(body) = message.header() else {continue};
			let Ok(parsed) = parse_mail(body) else {continue};

			if let Some(subject) = parsed.headers.get_first_value("Subject") {
				mailData.subject = subject;
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

			break;
		}

	}

	let _ = imap_session.logout();

	return Ok(listOfMail);
}

