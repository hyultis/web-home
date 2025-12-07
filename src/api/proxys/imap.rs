use leptos::server;
use serde::{Deserialize, Serialize};
use crate::api::proxys::imap_error::ImapError;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BoxName
{
	pub name: String,
	pub attributes: Attributs,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Attributs
{
	pub is_junk: bool,
	pub is_trash: bool,
	pub is_archive: bool,
	pub is_sent: bool,
	pub is_draft: bool,
}

impl Attributs
{
	#[cfg(feature = "ssr")]
	pub fn add<'a>(&mut self, attribute: &'a imap_proto::NameAttribute<'a>)
	{
		match attribute {
			imap_proto::NameAttribute::Archive => self.is_archive = true,
			imap_proto::NameAttribute::Drafts => self.is_draft = true,
			imap_proto::NameAttribute::Junk => self.is_junk = true,
			imap_proto::NameAttribute::Sent => self.is_sent = true,
			imap_proto::NameAttribute::Trash => self.is_trash = true,
			_ => {}
		}
	}

	pub fn is_uninteresting(&self) -> bool
	{
		self.is_junk || self.is_trash || self.is_sent || self.is_draft || self.is_archive
	}
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct imap_connector
{
	pub host: String,
	pub port: u16,
	pub username: String,
	pub password: String,
	pub extra: Option<imap_connector_extra>,
}

impl Default for imap_connector
{
	fn default() -> Self {
		Self {
			host: "".to_string(),
			port: 993,
			username: "".to_string(),
			password: "".to_string(),
			extra: None,
		}
	}
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct imap_connector_extra
{
	pub boxBlackList: Vec<String>,
}

#[server]
pub async fn API_proxys_imap_listbox(config: imap_connector) -> Result<Vec<BoxName>, ImapError>
{
	use inner::*;


	// the client we have here is unauthenticated.
	// to do anything useful with the e-mails, we need to log in
	let (mut imap_session,_) = connect_imap(config)?;
	let results = listbox(&mut imap_session)?;

	return Ok(results);
}

#[server]
pub async fn API_proxys_imap_getUnsee(config: imap_connector) -> Result<(), ImapError>
{
	use Htrace::HTrace;
	use inner::*;
	// the client we have here is unauthenticated.
	// to do anything useful with the e-mails, we need to log in
	let (mut imap_session,_) = connect_imap(config)?;
	let results = listbox(&mut imap_session)?;

	for boxName in results
	{
		let Ok(_) = imap_session.select(boxName.name) else {continue};

	}

	let results = imap_session.search("UNSEEN UNKEYWORD $Junk UNKEYWORD $Spam UNDELETED UNANSWERED UNDRAFT");
	HTrace!("list of result : {}",results.unwrap_or_default().len());
	let results = imap_session.search("UNSEEN");
	HTrace!("list of result : {}",results.unwrap_or_default().len());

	return Ok(());
}

#[cfg(feature = "ssr")]
mod inner {
	use imap::{Connection, Session};
	use imap::types::Mailbox;
	pub use super::*;

	pub fn connect_imap(config: imap_connector) -> Result<(Session<Connection>,Mailbox), ImapError>
	{
		let client = imap::ClientBuilder::new(config.host, config.port).connect()?; // /imap/ssl

		let mut imap_session = client
			.login(config.username, config.password)
			.map_err(|e| e.0)?;

		let mailbox = imap_session.select("INBOX")?;

		return Ok((imap_session,mailbox));
	}

	pub fn listbox(imap_session: &mut Session<Connection>) -> Result<Vec<BoxName>, ImapError>
	{

		let mut returning = Vec::new();
		let results = imap_session.list(None,Some("*"));
		if let Ok(names) = results
		{
			for result in names.iter()
			{
				let mut attributs = Attributs::default();
				result.attributes().iter().for_each(|attribute| attributs.add(attribute));

				let name = BoxName{
					name: result.name().to_string(),
					attributes: attributs,
				};
				returning.push(name);
			}
		}

		return Ok(returning);
	}
}