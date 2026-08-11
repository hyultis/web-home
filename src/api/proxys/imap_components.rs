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
	#[serde(default)]
	pub is_no_select: bool,
	pub is_junk: bool,
	pub is_trash: bool,
	pub is_archive: bool,
	pub is_sent: bool,
	pub is_draft: bool,
}

impl Attributs
{
	#[cfg(feature = "ssr")]
	pub(super) fn add<'a>(&mut self, attribute: &'a imap_proto::NameAttribute<'a>)
	{
		match attribute
		{
			imap_proto::NameAttribute::NoSelect => self.is_no_select = true,
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
		self.is_no_select || self.is_junk || self.is_trash || self.is_sent || self.is_draft || self.is_archive
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
	#[cfg(any(feature="ssr",test))]
	pub fn isGmail(&self) -> bool
	{
		let host = self.host.trim_end_matches('.').to_ascii_lowercase();
		return host == "gmail.com" || host.ends_with(".gmail.com");
	}

	pub fn isBoxSelected(&self, boxName: &str) -> bool
	{
		let Some(extra) = &self.extra
		else
		{
			return false;
		};
		if let Some(boxAllowList) = &extra.boxAllowList
		{
			return boxAllowList.iter().any(|selectedBox| selectedBox == boxName);
		}
		return !extra.boxBlackList.iter().any(|blacklistedBox| blacklistedBox == boxName);
	}

	pub fn selectedBoxNames_get(&self) -> Option<&[String]>
	{
		return self.extra.as_ref()?.boxAllowList.as_deref();
	}

	pub fn boxSelection_set(&mut self, boxName: String, selected: bool)
	{
		let extra = self.extra.get_or_insert_with(imap_connector_extra::default);
		let selectedBoxes = extra.boxAllowList.get_or_insert_with(Vec::new);
		selectedBoxes.retain(|selectedBox| selectedBox != &boxName);
		if (selected)
		{
			selectedBoxes.push(boxName);
			selectedBoxes.sort_unstable();
			selectedBoxes.dedup();
		}
	}

	pub fn boxSelection_migrate(&mut self, boxes: &[BoxName]) -> bool
	{
		let Some(extra) = &mut self.extra
		else
		{
			return false;
		};
		if let Some(selectedBoxes) = &mut extra.boxAllowList
		{
			let previousSelection = selectedBoxes.clone();
			selectedBoxes.retain(|selectedBox| {
				return boxes.iter().any(|boxContent| {
					return &boxContent.name == selectedBox && !boxContent.attributes.is_uninteresting();
				});
			});
			selectedBoxes.sort_unstable();
			selectedBoxes.dedup();
			return *selectedBoxes != previousSelection;
		}
		let mut selectedBoxes = boxes.iter()
			.filter(|boxContent| !boxContent.attributes.is_uninteresting())
			.filter(|boxContent| !extra.boxBlackList.iter().any(|boxName| boxName == &boxContent.name))
			.map(|boxContent| boxContent.name.clone())
			.collect::<Vec<_>>();
		selectedBoxes.sort_unstable();
		selectedBoxes.dedup();
		extra.boxAllowList = Some(selectedBoxes);
		return true;
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
	pub boxAllowList: Option<Vec<String>>,
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
	pub date: i64,
	#[serde(default)]
	pub parts: Vec<Attachment>,
	pub attachement: Vec<Attachment>,
	#[serde(default)]
	pub confirmVue: bool,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, Eq, Hash, PartialEq)]
pub struct ImapMailKey
{
	pub boxName: String,
	pub uidValidity: u32,
	pub uid: u32,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ImapMailboxSyncState
{
	pub boxName: String,
	pub uidValidity: Option<u32>,
	#[serde(default)]
	pub knownUids: Vec<u32>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ImapSyncRequest
{
	pub mailboxes: Option<Vec<ImapMailboxSyncState>>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct ImapMailboxSync
{
	pub boxName: String,
	pub uidValidity: u32,
	pub removedUids: Vec<u32>,
	pub mails: Vec<ImapMail>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Attachment
{
	pub filename: Option<String>,
	pub content_type: String,
	pub content_id: Option<String>,
	#[serde(with = "attachmentDataBase64")]
	pub data: Vec<u8>,
}

mod attachmentDataBase64
{
	use base64ct::{Base64, Encoding};
	use serde::{Deserialize, Deserializer, Serializer};

	const DATA_MAXIMUM_BYTES: usize = 16 * 1024 * 1024;
	const BASE64_MAXIMUM_BYTES: usize = (DATA_MAXIMUM_BYTES + 2) / 3 * 4;

	#[derive(Deserialize)]
	#[serde(untagged)]
	enum AttachmentData
	{
		Base64(String),
		LegacyBytes(Vec<u8>),
	}

	pub(super) fn serialize<S: Serializer>(data: &Vec<u8>, serializer: S) -> Result<S::Ok,S::Error>
	{
		if (data.len() > DATA_MAXIMUM_BYTES)
		{
			return Err(serde::ser::Error::custom("attachment is too large"));
		}
		return serializer.serialize_str(&Base64::encode_string(data));
	}

	pub(super) fn deserialize<'de,D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>,D::Error>
	{
		return match AttachmentData::deserialize(deserializer)?
		{
			AttachmentData::Base64(data) if data.len() <= BASE64_MAXIMUM_BYTES => Base64::decode_vec(&data)
				.map_err(|_| serde::de::Error::custom("invalid attachment base64"))
				.and_then(|data| {
					return (data.len() <= DATA_MAXIMUM_BYTES)
						.then_some(data)
						.ok_or_else(|| serde::de::Error::custom("attachment is too large"));
				}),
			AttachmentData::LegacyBytes(data) if data.len() <= DATA_MAXIMUM_BYTES => Ok(data),
			_ => Err(serde::de::Error::custom("attachment is too large")),
		};
	}
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

impl ImapMailContentType
{
	#[cfg(any(feature="ssr",test))]
	pub fn is_none(&self) -> bool
	{
		matches!(self, Self::None)
	}

	#[cfg(any(feature="ssr",test))]
	pub fn is_not_html(&self) -> bool
	{
		!matches!(self, Self::Html(_))
	}

	pub fn is_html(&self) -> bool
	{
		matches!(self, Self::Html(_))
	}
}

#[cfg(test)]
mod tests
{
	use super::{Attachment, Attributs, BoxName, ImapMailboxSyncState, imap_connector, imap_connector_extra};

	fn boxName_get(name: &str, attributes: Attributs) -> BoxName
	{
		return BoxName {name: name.to_string(),attributes};
	}

	#[test]
	fn connectorExtra_deserializesLegacyConfigWithoutAllowList()
	{
		let extra: imap_connector_extra = serde_json::from_str(
			r#"{"boxBlackList":["Blocked"],"flagBlackList":[]}"#,
		).unwrap();

		assert_eq!(extra.boxAllowList,None);
		assert_eq!(extra.boxBlackList,vec!["Blocked"]);
	}

	#[test]
	fn connectorWithoutExtraDoesNotSelectAnyMailbox()
	{
		let connector = imap_connector::default();

		assert!(!connector.isBoxSelected("Alerts"));
		assert_eq!(connector.selectedBoxNames_get(),None);
	}

	#[test]
	fn connectorRecognizesOnlyGmailHostSuffixCaseInsensitively()
	{
		let mut connector = imap_connector::default();
		connector.host = "IMAP.GMAIL.COM.".to_string();
		assert!(connector.isGmail());

		connector.host = "gmail.com.attacker.example".to_string();
		assert!(!connector.isGmail());
	}

	#[test]
	fn connectorMigratesLegacyBlacklistToExplicitInterestingSelection()
	{
		let mut connector = imap_connector::default();
		connector.extra = Some(imap_connector_extra {
			boxBlackList: vec!["Blocked".to_string()],
			..Default::default()
		});
		let boxes = vec![
			boxName_get("Alerts",Attributs::default()),
			boxName_get("Blocked",Attributs::default()),
			boxName_get("Archive",Attributs {is_archive: true,..Default::default()}),
		];

		assert!(connector.boxSelection_migrate(&boxes));
		assert_eq!(connector.selectedBoxNames_get(),Some(["Alerts".to_string()].as_slice()));
		assert!(connector.isBoxSelected("Alerts"));
		assert!(!connector.isBoxSelected("Blocked"));
		assert!(!connector.isBoxSelected("Archive"));
		assert!(!connector.boxSelection_migrate(&boxes));
	}

	#[test]
	fn connectorRemovesNonSelectableMailboxFromExplicitSelection()
	{
		let mut connector = imap_connector::default();
		connector.extra = Some(imap_connector_extra {
			boxAllowList: Some(vec!["[Gmail]".to_string(),"Alerts".to_string()]),
			..Default::default()
		});
		let boxes = vec![
			boxName_get("[Gmail]",Attributs {is_no_select: true,..Default::default()}),
			boxName_get("Alerts",Attributs::default()),
		];

		assert!(connector.boxSelection_migrate(&boxes));
		assert_eq!(connector.selectedBoxNames_get(),Some(["Alerts".to_string()].as_slice()));
		assert!(!connector.boxSelection_migrate(&boxes));
	}

	#[cfg(feature = "ssr")]
	#[test]
	fn mailboxNoSelectAttributeIsNeverInteresting()
	{
		let mut attributes = Attributs::default();
		attributes.add(&imap_proto::NameAttribute::NoSelect);

		assert!(attributes.is_no_select);
		assert!(attributes.is_uninteresting());
	}

	#[test]
	fn mailboxAttributesReadLegacyContractWithoutNoSelect()
	{
		let attributes: Attributs = serde_json::from_str(
			r#"{"is_junk":false,"is_trash":false,"is_archive":false,"is_sent":false,"is_draft":false}"#,
		).unwrap();

		assert!(!attributes.is_no_select);
	}

	#[test]
	fn mailboxSyncStateReadsMissingKnownUidsAsEmpty()
	{
		let state: ImapMailboxSyncState = serde_json::from_str(
			r#"{"boxName":"Alerts","uidValidity":null}"#,
		).unwrap();

		assert!(state.knownUids.is_empty());
	}

	#[test]
	fn connectorCreatesExplicitSelectionOnFirstUserChoice()
	{
		let mut connector = imap_connector::default();

		connector.boxSelection_set("Alerts".to_string(),true);
		assert_eq!(connector.selectedBoxNames_get(),Some(["Alerts".to_string()].as_slice()));
		connector.boxSelection_set("Alerts".to_string(),false);
		assert_eq!(connector.selectedBoxNames_get(),Some([].as_slice()));
	}

	#[test]
	fn attachmentUsesCompactBase64AndReadsLegacyByteArray()
	{
		let attachment = Attachment {
			filename: Some("test.bin".to_string()),
			content_type: "application/octet-stream".to_string(),
			content_id: None,
			data: vec![1,2,3],
		};

		let serialized = serde_json::to_value(&attachment).unwrap();
		assert_eq!(serialized.get("data").and_then(serde_json::Value::as_str),Some("AQID"));
		let roundTrip: Attachment = serde_json::from_value(serialized).unwrap();
		assert_eq!(roundTrip.data,vec![1,2,3]);

		let legacy: Attachment = serde_json::from_value(serde_json::json!({
			"filename": null,
			"content_type": "application/octet-stream",
			"content_id": null,
			"data": [1,2,3]
		})).unwrap();
		assert_eq!(legacy.data,vec![1,2,3]);
	}
}
