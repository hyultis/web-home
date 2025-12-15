use serde::{Deserialize, Serialize};

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
	#[serde(default)]
	pub boxBlackList: Vec<String>,
	#[serde(default)]
	pub flagBlackList: Vec<String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ImapMail
{
	pub uid: u32,
	pub from: String,
	pub to: String,
	pub subject: Option<String>,
	pub content: Option<String>,
	pub files: Option<Vec<String>>,
	pub date: i64,
	pub boxName: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct MailParts {
	pub text: Option<String>,
	pub html: Option<String>,
	pub attachments: Vec<Attachment>,
	pub inline: Vec<Attachment>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Attachment {
	pub filename: Option<String>,
	pub content_type: String,
	pub content_id: Option<String>,
	pub data: Vec<u8>,
}


#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ImapMailIdentifier
{
	pub uid: u32,
	pub boxName: String,
}

impl From<ImapMail> for ImapMailIdentifier
{
	fn from(value: ImapMail) -> Self {
		ImapMailIdentifier { uid: value.uid, boxName: value.boxName }
	}
}

impl From<&ImapMail> for ImapMailIdentifier
{
	fn from(value: &ImapMail) -> Self {
		ImapMailIdentifier { uid: value.uid, boxName: value.boxName.clone() }
	}
}