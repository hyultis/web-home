use base64ct::{Base64, Encoding};
use leptos::logging::log;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

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
		match attribute
		{
			imap_proto::NameAttribute::Archive => self.is_archive = true,
			imap_proto::NameAttribute::Drafts => self.is_draft = true,
			imap_proto::NameAttribute::Junk => self.is_junk = true,
			imap_proto::NameAttribute::Sent => self.is_sent = true,
			imap_proto::NameAttribute::Trash => self.is_trash = true,
			_ =>
			{}
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

impl imap_connector
{
	pub fn isGmail(&self) -> bool
	{
		self.host.contains("gmail.com")
	}

	pub fn isBoxBlacklisted(&self, boxName: impl ToString) -> bool
	{
		let Some(extra) = &self.extra
		else
		{
			return true;
		};
		return extra.boxBlackList.contains(&boxName.to_string());
	}
}

impl Default for imap_connector
{
	fn default() -> Self
	{
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
	pub content: ImapMailContentType,
	pub files: Option<Vec<String>>,
	pub date: i64,
	pub boxName: String,
	pub parts: Vec<Attachment>,
	pub attachement: Vec<Attachment>,
	pub confirmVue: bool,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Attachment
{
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ImapMailContentType
{
	None,
	Text(String),
	Html(String),
}

impl Default for ImapMailContentType
{
	fn default() -> Self
	{
		Self::None
	}
}

static CI_CONTENT: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#"(?i)src\s*=\s*("cid:[^"]+"|'cid:[^']+')"#).unwrap());
static A_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)<a\b[^>]*>"#).unwrap());
static A_TAG_TARGET: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#"(?i)\btarget\s*=\s*"[^"]*"|\btarget\s*=\s*'[^']*'"#).unwrap());

impl ImapMailContentType
{
	pub fn is_none(&self) -> bool
	{
		matches!(self, Self::None)
	}

	pub fn is_not_html(&self) -> bool
	{
		!matches!(self, Self::Html(_))
	}

	/// do not panic, just return an empty string in case of None
	/// in case of Text, convert return line into <br/>
	pub fn unwrap_or_default(&self, parts: &Vec<Attachment>) -> String
	{
		match self
		{
			Self::Text(text) => text.clone().replace("\n", "<br/>"),
			Self::Html(html) =>
			{
				let result = CI_CONTENT
					.replace_all(html, |caps: &regex::Captures| {
						let full = caps.get(1).unwrap().as_str(); // ex: "cid:image@id"
						let quote = &full[0..1]; // " ou '
						let cid = &full[5..full.len() - 1]; // enlève "cid: et la quote finale
						//log!("cid: {} quote: {}", cid, quote);

						let filter = |part: &&Attachment| {
							//log!("filter: {:?} == {}", part.content_id,cid);
							if let Some(partcid) = &part.content_id
							{
								return partcid == &cid.to_string();
							}
							return false;
						};

						if let Some((mime, bytes)) = parts
							.iter()
							.filter(filter)
							.next()
							.map(|part| (part.content_type.clone(), part.data.clone()))
						{
							//log!("cid found");
							let b64 = Base64::encode_string(bytes.as_slice());
							format!(r#"src={quote}data:{mime};base64,{b64}{quote}"#)
						}
						else
						{
							// on laisse tel quel (comme ton return $match[0])
							caps.get(0).unwrap().as_str().to_string()
						}
					})
					.into_owned();

				Self::normalize_a_targets(&html)
			}
			_ => "".to_string(),
		}
	}

	pub fn normalize_a_targets(content: &String) -> String
	{
		A_TAG_RE
			.replace_all(content, |caps: &regex::Captures| {
				let tag = caps.get(0).unwrap().as_str();
				//log!("tag: {}", tag);

				if A_TAG_TARGET.is_match(tag)
				{
					// remplace le target existant
					let returned =  A_TAG_TARGET
						.replace_all(tag, r#"target="_blank" rel=\"noopener noreferrer\""#)
						.into_owned();
					//log!("returned: {}", returned);
					return returned;
				}

				// injecte avant '>'
				let insert_pos = tag.rfind('>').unwrap();
				let returned = format!(
					"{} target=\"_blank\" rel=\"noopener noreferrer\">",
					&tag[..insert_pos]
				);
				//log!("returned: {}", returned);
				returned
			})
			.into_owned()
	}
}

impl From<ImapMail> for ImapMailIdentifier
{
	fn from(value: ImapMail) -> Self
	{
		ImapMailIdentifier {
			uid: value.uid,
			boxName: value.boxName,
		}
	}
}

impl From<&ImapMail> for ImapMailIdentifier
{
	fn from(value: &ImapMail) -> Self
	{
		ImapMailIdentifier {
			uid: value.uid,
			boxName: value.boxName.clone(),
		}
	}
}
