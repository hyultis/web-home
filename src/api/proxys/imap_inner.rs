use std::collections::HashSet;
use crate::api::proxys::imap_components::{imap_connector, Attachment, Attributs, BoxName, ImapMail};
use crate::api::proxys::imap_error::ImapError;
use imap::types::{Fetches, Mailbox, Uid};
use imap::{Connection, Session};
use mailparse::{parse_mail, DispositionType, MailHeaderMap};

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
		let Ok(message) = imap_session.uid_fetch(&result.to_string(), "(FLAGS INTERNALDATE BODY.PEEK[HEADER])") else { continue };
		//HTrace!("number of submessage : {}",message.len());

		if let Some(mailData) = extract_ImapMail_from_fetch(imap_session, result, message, boxName)
		{
		listOfMail.push(mailData);
		}
	}
	return listOfMail;
}


pub fn extract_ImapMail_from_fetch(imap_session: &mut Session<Connection>, uid: u32, message: Fetches, boxName: &String) -> Option<ImapMail>
{
	let mut mailData = ImapMail {
		uid,
		subject: None,
		boxName: boxName.clone(),
		..Default::default()
	};

	for message in message.iter() {
		println!("message ok");
		if let Some(header) = message.header() && let Ok(parsed) = parse_mail(header)
		{
			if let Some(subject) = parsed.headers.get_first_value("Subject") {
				mailData.subject = Some(subject);
			}

			if let Some(from) = parsed.headers.get_first_value("From") {
				mailData.from = from;
			}

			if let Some(to) = parsed.headers.get_first_value("To") {
				mailData.to = to;
			}
		}

		//println!("mail parse content : {}", String::from_utf8(message.body().unwrap_or("qdqsdqd".as_bytes()).to_vec()).unwrap_or_default());
		if let Some(body) = message.body() && !body.is_empty() && let Ok(parsed) = parse_mail(body)
		{
			for part in &parsed.subparts {
				println!("--- part {} ----",part.ctype.mimetype);
				match part.ctype.mimetype.as_str() {
					"text/plain" => {
						if let Ok(text) = part.get_body() {
							if mailData.content.is_none() {
								mailData.content = Some(text);
							}
						}
					}
					"text/html" => {
						if let Ok(text) = part.get_body() {
							mailData.content = Some(text);
						}
					}
					"multipart/alternative" => {
						println!("alternative {}",part.ctype.mimetype);
					}
					_ => {
						let cd = part.get_content_disposition();

						let is_attachment = cd.disposition == DispositionType::Attachment;
						let is_inline = cd.disposition == DispositionType::Inline;

						if is_attachment || is_inline {
							if let Ok(data) = part.get_body_raw() {
								let attachment = Attachment {
									filename: cd.params.iter().filter_map(|((key,value))| {
										if(key=="filename") {return Some(value.clone());}
										return None;
									}).next(),
									content_type: part.ctype.mimetype.clone(),
									content_id: part.headers.get_first_value("Content-ID"),
									data,
								};

								/*if is_inline {
									out.inline.push(attachment);
								} else {
									out.attachments.push(attachment);
								}*/
							}
						}
					}
				}
			}
		}

		//HTrace!("body : {}",parsed.subparts.iter().next().unwrap().get_body().unwrap());

		if let Some(date) = message.internal_date() {
			mailData.date = date.timestamp();
		}
	}

	return Some(mailData);
}

/*fn extract_parts(mail: &ParsedMail, out: &mut MailParts) {
	for part in &parsed.subparts {
		println!("--- part {} ----",part.ctype.mimetype);
		match part.ctype.mimetype.as_str() {
			"text/plain" => {
				if let Ok(text) = part.get_body() {
					if mailData.content.is_none() {
						mailData.content = Some(text);
					}
				}
			}
			"text/html" => {
				if let Ok(text) = part.get_body() {
					mailData.content = Some(text);
				}
			}
			"multipart/alternative" => {

			}
			_ => {
				let cd = mail.get_content_disposition();

				let is_attachment =
					cd.as_ref().map(|d| d.disposition == "attachment").unwrap_or(false);

				let is_inline =
					cd.as_ref().map(|d| d.disposition == "inline").unwrap_or(false);

				if is_attachment || is_inline {
					if let Ok(data) = mail.get_body_raw() {
						let attachment = Attachment {
							filename: cd.and_then(|d| d.params.get("filename").cloned()),
							content_type: mail.ctype.mimetype.clone(),
							content_id: mail.headers.get_first_value("Content-ID"),
							data,
						};

						if is_inline {
							out.inline.push(attachment);
						} else {
							out.attachments.push(attachment);
						}
					}
				}
			}
		}
	}
}
*/