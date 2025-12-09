use crate::api::proxys::imap_components::{imap_connector, Attributs, BoxName};
use crate::api::proxys::imap_error::ImapError;
use imap::types::Mailbox;
use imap::{Connection, Session};

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
