use std::collections::HashSet;
use crate::api::proxys::imap_components::{imap_connector, Attachment, Attributs, BoxName, ImapMail, ImapMailContentType};
use crate::api::proxys::imap_error::ImapError;
use imap::types::{Fetches, Mailbox, Uid};
use imap::{Connection, Session};
use mailparse::{parse_mail, DispositionType, MailHeaderMap, ParsedMail};

pub fn connect_imap(config: &imap_connector) -> Result<(Session<Connection>, Mailbox, bool), ImapError>
{
	let isGmail = config.isGmail();
	let client = imap::ClientBuilder::new(config.host.clone(), config.port).connect()?; // /imap/ssl

	let mut imap_session = client
		.login(config.username.clone(), config.password.clone())
		.map_err(|e| e.0)?;

	let mailbox = imap_session.select("INBOX")?;

	return Ok((imap_session, mailbox, isGmail));
}

pub fn listbox(imap_session: &mut Session<Connection>, isGmail: bool) -> Result<Vec<BoxName>, ImapError>
{
	let mut returning = Vec::new();
	let results = imap_session.list(None, Some("*"));
	if let Ok(names) = results
	{
		for result in names.iter()
		{
			// special gmail case
			if(isGmail && result.name().to_string()=="INBOX") {continue};

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

	for message in message.iter()
	{
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
			body_content_extract(&mut mailData,&parsed);
			for part in &parsed.subparts {
				body_content_extract(&mut mailData, part);
			}
		}

		if let Some(date) = message.internal_date() {
			mailData.date = date.timestamp();
		}
	}

	return Some(mailData);
}


pub fn body_content_extract(mailData: &mut ImapMail,body: &ParsedMail)
{
	//println!("--- part {} ----",body.ctype.mimetype);
	match body.ctype.mimetype.as_str() {
		"text/plain" => {
			if let Ok(text) = body.get_body() {
				if mailData.content.is_none() {
					mailData.content = ImapMailContentType::Text(text);
				}
			}
		}
		"text/html" => {
			if let Ok(text) = body.get_body() {
				if mailData.content.is_not_html() {
					mailData.content = ImapMailContentType::Html(text);
				}
			}
		}
		"multipart/alternative" | "multipart/mixed" | "multipart/related" => {
			//println!("alternative {}",body.ctype.mimetype);
			for subpart in body.subparts.iter() {
				body_content_extract(mailData,subpart);
			}
		}
		_ => {
			let cd = body.get_content_disposition();

			let is_attachment = cd.disposition == DispositionType::Attachment;
			let is_inline = cd.disposition == DispositionType::Inline;

			if is_attachment || is_inline {
				if let Ok(data) = body.get_body_raw() {
					let attachment = Attachment {
						filename: cd.params.iter().filter_map(|((key,value))| {
							if(key=="filename") {return Some(value.clone());}
							return None;
						}).next(),
						content_type: body.ctype.mimetype.clone(),
						content_id: body.headers.get_first_value("Content-ID").map(|s| s.trim().trim_start_matches('<').trim_end_matches('>').to_string()),
						data,
					};

					if is_inline {
						if let Some(content_id) = attachment.content_id.clone() {
							if mailData.parts.iter().filter(|att| att.content_id.is_some() && att.content_id.as_ref().unwrap()==&content_id).next().is_none() {
								mailData.parts.push(attachment);
							}
						}
						else {
							mailData.parts.push(attachment);
						}
					} else {
						if let Some(filename) = attachment.filename.clone() {
							if mailData.attachement.iter().filter(|att| att.filename.is_some() && att.filename.as_ref().unwrap()==&filename).next().is_none() {
								mailData.attachement.push(attachment);
							}
						}
						else {
							mailData.attachement.push(attachment);
						}
					}
				}
			}
		}
	}
	//println!("--- end part {} ----",body.ctype.mimetype);
}