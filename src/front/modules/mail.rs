use std::ops::DerefMut;
use std::collections::{HashMap,HashSet};
use std::sync::Arc;
use gloo_timers::callback::Timeout;
use leptoaster::{expect_toaster, ToasterContext};
use leptos::children::ViewFn;
use leptos::prelude::{use_context, CollectView, StyleAttribute, Write};
use leptos::prelude::{ClassAttribute, ElementChild, GetUntracked, Update};
use leptos::prelude::{AnyView, ArcRwSignal, Get, IntoAny, OnAttribute, RwSignal, Set};
use leptos::{component, view, IntoView};
use serde::{Deserialize, Serialize};
use time::UtcDateTime;
use crate::api::modules::components::{ModuleContent, ModuleID};
use crate::api::proxys::imap::{API_proxys_imap_getMailContent, API_proxys_imap_listbox, API_proxys_imap_setMailSee, API_proxys_imap_sync};
use crate::api::proxys::imap_components::{imap_connector, Attachment, BoxName, ImapMailboxSync, ImapMailboxSyncState, ImapMail, ImapMailContentType, ImapMailKey, ImapSyncRequest};
use crate::front::modules::components::{distant_time_simpler, Backable, BoxFuture, Cache, Cacheable, FieldHelper, FieldHelperType, ModuleName, ModuleSizeContrainte, RefreshTime};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::contentDownloader::download_attachment;
use crate::front::utils::dialog::{DialogData, DialogManager};
use crate::front::utils::draw_title_if_present;
use crate::front::utils::toaster_helpers::{toaster_api, toastingErr};
use crate::front::utils::translate::Translate;
use crate::HWebTrace;

#[derive(Serialize,Deserialize,Debug)]
#[derive(Clone)]
struct MailConfig
{
	#[serde(default)]
	pub title: String,
	#[serde(default)]
	mailAsTag: String,
	#[serde(default)]
	remoteImageSenderAllowList: Vec<String>,
	pub imap: imap_connector,
}
impl Default for MailConfig
{
	fn default() -> Self
	{
		Self {
			title: "".to_string(),
			mailAsTag: "".to_string(),
			remoteImageSenderAllowList: Vec::new(),
			imap: imap_connector::default(),
		}
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MailSenderAddress
{
	normalized: String,
}

impl MailSenderAddress
{
	const MAXIMUM_BYTES: usize = 320;

	fn from_header(addressHeader: &str) -> Option<Self>
	{
		return addressHeader.split([',',';']).find_map(Self::from_candidate);
	}

	fn from_candidate(addressCandidate: &str) -> Option<Self>
	{
		let addressCandidate = addressCandidate.trim();
		let address = if let Some((_,address)) = addressCandidate.rsplit_once('<')
		{
			address.split_once('>').map(|(address,_)| address).unwrap_or(address)
		}
		else
		{
			addressCandidate.split_whitespace()
				.find(|part| part.contains('@'))
				.unwrap_or(addressCandidate)
		};

		let address = address.trim().trim_matches(|character: char| {
			return character.is_whitespace() || matches!(character, '<' | '>' | '"' | '\'' | '(' | ')');
		});
		let (localPart,domain) = address.rsplit_once('@')?;
		let localPart = localPart.trim().trim_matches(|character: char| matches!(character, '"' | '\''));
		let domain = domain.trim().trim_matches(|character: char| {
			return character.is_whitespace() || matches!(character, '<' | '>' | '"' | '\'' | '(' | ')');
		});
		if (localPart.is_empty() || domain.is_empty() || localPart.contains('@') || domain.contains('@'))
		{
			return None;
		}
		if (localPart.chars().chain(domain.chars()).any(|character| character.is_whitespace() || character.is_control()))
		{
			return None;
		}

		let normalized = format!("{}@{}",localPart.to_lowercase(),domain.to_lowercase());
		if (normalized.len() > Self::MAXIMUM_BYTES)
		{
			return None;
		}
		return Some(Self {normalized});
	}

	fn as_str(&self) -> &str
	{
		return &self.normalized;
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MailTag
{
	label: String,
	color: String,
}

impl MailTag
{
	fn from_address_header(addressHeader: &str, suffix: &str) -> Option<Self>
	{
		let suffix = suffix.trim().trim_start_matches('@');
		if(suffix.is_empty())
		{
			return None;
		}

		return addressHeader.split([',',';']).find_map(|addressCandidate| {
			let addressCandidate = addressCandidate.trim();
			let address = if let Some((_,address)) = addressCandidate.rsplit_once('<')
			{
				address.split_once('>').map(|(address,_)| address).unwrap_or(address)
			}
			else
			{
				addressCandidate.split_whitespace()
					.find(|part| part.contains('@'))
					.unwrap_or(addressCandidate)
			};

			let address = address.trim().trim_matches(|character: char| {
				character.is_whitespace() || matches!(character, '<' | '>' | '"' | '\'')
			});
			let (label,domain) = address.rsplit_once('@')?;
			let label = label.trim().trim_matches(|character: char| matches!(character, '"' | '\''));
			let domain = domain.trim().trim_matches(|character: char| {
				character.is_whitespace() || matches!(character, '<' | '>' | '"' | '\'')
			});

			if(label.is_empty() || !domain.eq_ignore_ascii_case(suffix))
			{
				return None;
			}

			return Some(Self::new(label));
		});
	}

	fn new(label: impl ToString) -> Self
	{
		let label = label.to_string();
		let mut hash = 2_166_136_261_u32;
		for byte in label.to_lowercase().bytes()
		{
			hash ^= byte as u32;
			hash = hash.wrapping_mul(16_777_619);
		}

		Self {
			label,
			color: format!("hsl({}, 70%, 70%)",hash % 360),
		}
	}

	fn style(&self) -> String
	{
		return format!("--mail-tag-color:{}",self.color);
	}
}

#[derive(Clone, Debug)]
struct MailContentFrame
{
	content: Arc<ImapMailContentType>,
}

impl MailContentFrame
{
	fn new(content: ImapMailContentType) -> Self
	{
		return Self {content: Arc::new(content)};
	}

	fn remoteImagesControl_isAvailable(&self) -> bool
	{
		return self.content.is_html();
	}

	fn srcdoc_get(&self, remoteImagesAllowed: bool) -> String
	{
		let imageSources = if (remoteImagesAllowed)
		{
			"data: blob: http: https:"
		}
		else
		{
			"data: blob:"
		};
		let body = match self.content.as_ref()
		{
			ImapMailContentType::Text(text) => format!("<div class=\"mail-text\">{}</div>",Self::text_escape(text)),
			ImapMailContentType::Html(html) => html.clone(),
			ImapMailContentType::None => String::new(),
		};
		return format!(
			concat!(
				"<!doctype html><html><head><meta charset=\"utf-8\">",
				"<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; base-uri 'none'; ",
				"form-action 'none'; img-src {}; style-src 'unsafe-inline'\">",
				"<meta name=\"referrer\" content=\"no-referrer\">",
				"<style>html,body{{box-sizing:border-box;max-width:100%;}}body{{margin:.75rem;overflow-wrap:anywhere;}}",
				"img{{max-width:100%;height:auto;}}.mail-text{{white-space:pre-wrap;}}</style>",
				"</head><body>{}</body></html>"
			),
			imageSources,
			body,
		);
	}

	fn text_escape(text: &str) -> String
	{
		let mut escaped = String::with_capacity(text.len());
		for character in text.chars()
		{
			match character
			{
				'&' => escaped.push_str("&amp;"),
				'<' => escaped.push_str("&lt;"),
				'>' => escaped.push_str("&gt;"),
				'"' => escaped.push_str("&quot;"),
				'\'' => escaped.push_str("&#39;"),
				_ => escaped.push(character),
			}
		}
		return escaped;
	}
}

impl MailConfig
{
	fn mail_tag_is_active(&self) -> bool
	{
		return !self.mailAsTag.trim().trim_start_matches('@').is_empty();
	}

	fn mail_tag(&self, mail: &ImapMail) -> Option<MailTag>
	{
		return MailTag::from_address_header(&mail.to,&self.mailAsTag);
	}

	fn remoteImageSender_isAllowed(&self, senderAddress: &MailSenderAddress) -> bool
	{
		return self.remoteImageSenderAllowList.iter()
			.any(|allowedAddress| allowedAddress.eq_ignore_ascii_case(senderAddress.as_str()));
	}

	fn remoteImageSender_allow(&mut self, senderAddress: &MailSenderAddress) -> bool
	{
		if (self.remoteImageSender_isAllowed(senderAddress))
		{
			return false;
		}
		self.remoteImageSenderAllowList.push(senderAddress.as_str().to_string());
		self.remoteImageSenderAllowList.sort_unstable();
		return true;
	}

	fn remoteImageSender_remove(&mut self, senderAddress: &str) -> bool
	{
		let previousLength = self.remoteImageSenderAllowList.len();
		self.remoteImageSenderAllowList.retain(|allowedAddress| {
			return !allowedAddress.eq_ignore_ascii_case(senderAddress);
		});
		return self.remoteImageSenderAllowList.len() != previousLength;
	}

	fn remoteImageSenderAllowList_get(&self) -> Vec<String>
	{
		let mut allowList = self.remoteImageSenderAllowList.clone();
		allowList.sort_unstable_by_key(|address| address.to_lowercase());
		allowList.dedup_by(|left,right| left.eq_ignore_ascii_case(right));
		return allowList;
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MailSyncIdentity
{
	host: String,
	port: u16,
	username: String,
	boxAllowList: Option<Vec<String>>,
	boxBlackList: Option<Vec<String>>,
}

impl MailSyncIdentity
{
	fn new(connector: &imap_connector) -> Self
	{
		let mut boxAllowList = connector.selectedBoxNames_get().map(<[String]>::to_vec);
		if let Some(boxAllowList) = &mut boxAllowList
		{
			boxAllowList.sort_unstable();
			boxAllowList.dedup();
		}
		let mut boxBlackList = connector.extra.as_ref().map(|extra| extra.boxBlackList.clone());
		if let Some(boxBlackList) = &mut boxBlackList
		{
			boxBlackList.sort_unstable();
			boxBlackList.dedup();
		}
		return Self {
			host: connector.host.clone(),
			port: connector.port,
			username: connector.username.clone(),
			boxAllowList,
			boxBlackList,
		};
	}
}

#[derive(Clone, Debug, Default)]
struct MailsContent
{
	mailboxes: Option<HashMap<String,Option<u32>>>,
	mailsData: HashMap<ImapMailKey,ImapMail>,
	pendingSeen: HashSet<ImapMailKey>,
	confirmedSeen: HashSet<ImapMailKey>,
	boxs: Vec<BoxName>,
	syncIdentity: Option<MailSyncIdentity>,
}

impl MailsContent
{
	fn syncRequest_get(&mut self, connector: &imap_connector) -> Option<ImapSyncRequest>
	{
		let syncIdentity = MailSyncIdentity::new(connector);
		if (self.syncIdentity.as_ref() != Some(&syncIdentity))
		{
			self.sync_reset(syncIdentity);
		}
		if (connector.extra.is_none())
		{
			return None;
		}
		if (self.mailboxes.is_none())
		{
			if let Some(selectedBoxNames) = connector.selectedBoxNames_get()
			{
				self.mailboxes = Some(selectedBoxNames.iter()
					.map(|boxName| (boxName.clone(),None))
					.collect());
			}
		}
		if (self.mailboxes.as_ref().is_some_and(HashMap::is_empty))
		{
			return None;
		}

		let mailboxes = self.mailboxes.as_ref().map(|mailboxes| {
			let mut states = mailboxes.iter().map(|(boxName,uidValidity)| {
				let mut knownUids = uidValidity.map(|uidValidity| {
					return self.mailsData.keys().chain(self.pendingSeen.iter())
						.filter(|key| key.boxName == *boxName && key.uidValidity == uidValidity)
						.map(|key| key.uid)
						.collect::<Vec<_>>();
				}).unwrap_or_default();
				knownUids.sort_unstable();
				knownUids.dedup();
				return ImapMailboxSyncState {
					boxName: boxName.clone(),
					uidValidity: *uidValidity,
					knownUids,
				};
			}).collect::<Vec<_>>();
			states.sort_unstable_by(|left,right| left.boxName.cmp(&right.boxName));
			return states;
		});
		return Some(ImapSyncRequest { mailboxes });
	}

	fn sync_apply(&mut self, mailboxes: Vec<ImapMailboxSync>)
	{
		let mut synchronizedMailboxes = HashMap::new();
		for mailbox in mailboxes
		{
			let previousValidity = self.mailboxes.as_ref()
				.and_then(|mailboxes| mailboxes.get(&mailbox.boxName))
				.copied()
				.flatten();
			if (previousValidity != Some(mailbox.uidValidity))
			{
				self.mailsData.retain(|key,_| key.boxName != mailbox.boxName);
				self.pendingSeen.retain(|key| key.boxName != mailbox.boxName);
				self.confirmedSeen.retain(|key| key.boxName != mailbox.boxName);
			}
			for uid in mailbox.removedUids
			{
				let key = ImapMailKey {
					boxName: mailbox.boxName.clone(),
					uidValidity: mailbox.uidValidity,
					uid,
				};
				self.mailsData.remove(&key);
				self.pendingSeen.remove(&key);
				self.confirmedSeen.remove(&key);
			}
			for mail in mailbox.mails
			{
				let key = ImapMailKey {
					boxName: mailbox.boxName.clone(),
					uidValidity: mailbox.uidValidity,
					uid: mail.uid,
				};
				if (!self.pendingSeen.contains(&key))
				{
					self.mailsData.insert(key,mail);
				}
			}
			let reconciledSeen = self.confirmedSeen.iter()
				.filter(|key| key.boxName == mailbox.boxName && key.uidValidity == mailbox.uidValidity)
				.cloned()
				.collect::<Vec<_>>();
			for key in reconciledSeen
			{
				self.confirmedSeen.remove(&key);
				self.pendingSeen.remove(&key);
			}
			synchronizedMailboxes.insert(mailbox.boxName,Some(mailbox.uidValidity));
		}
		self.mailsData.retain(|key,_| {
			return synchronizedMailboxes.get(&key.boxName) == Some(&Some(key.uidValidity));
		});
		self.pendingSeen.retain(|key| {
			return synchronizedMailboxes.get(&key.boxName) == Some(&Some(key.uidValidity));
		});
		self.confirmedSeen.retain(|key| {
			return synchronizedMailboxes.get(&key.boxName) == Some(&Some(key.uidValidity));
		});
		self.mailboxes = Some(synchronizedMailboxes);
	}

	fn topology_set(&mut self, boxes: &[BoxName], connector: &imap_connector)
	{
		self.syncIdentity = Some(MailSyncIdentity::new(connector));
		self.boxs = boxes.to_vec();
		if (connector.extra.is_none())
		{
			self.mailboxes = None;
			self.mailsData.clear();
			self.pendingSeen.clear();
			self.confirmedSeen.clear();
			return;
		}
		let previousMailboxes = self.mailboxes.take().unwrap_or_default();
		let selectedMailboxes = boxes.iter()
			.filter(|boxContent| !boxContent.attributes.is_uninteresting() && connector.isBoxSelected(&boxContent.name))
			.map(|boxContent| {
				return (boxContent.name.clone(),previousMailboxes.get(&boxContent.name).copied().flatten());
			})
			.collect::<HashMap<_,_>>();
		self.mailsData.retain(|key,_| {
			return selectedMailboxes.get(&key.boxName) == Some(&Some(key.uidValidity));
		});
		self.pendingSeen.retain(|key| {
			return selectedMailboxes.get(&key.boxName) == Some(&Some(key.uidValidity));
		});
		self.confirmedSeen.retain(|key| {
			return selectedMailboxes.get(&key.boxName) == Some(&Some(key.uidValidity));
		});
		self.mailboxes = Some(selectedMailboxes);
	}

	fn confirmation_set(&mut self, key: &ImapMailKey, value: bool)
	{
		if let Some(mail) = self.mailsData.get_mut(key)
		{
			mail.confirmVue = value;
		}
	}

	fn mailSeen_begin(&mut self, key: &ImapMailKey) -> Option<ImapMail>
	{
		self.pendingSeen.insert(key.clone());
		return self.mailsData.remove(key);
	}

	fn mailSeen_rollback(&mut self, key: ImapMailKey, mail: Option<ImapMail>)
	{
		self.pendingSeen.remove(&key);
		self.confirmedSeen.remove(&key);
		if let Some(mail) = mail
		{
			self.mailsData.insert(key,mail);
		}
	}

	fn mailSeen_commit(&mut self, key: ImapMailKey)
	{
		if (self.pendingSeen.contains(&key))
		{
			self.confirmedSeen.insert(key);
		}
	}

	fn sync_reset(&mut self, syncIdentity: MailSyncIdentity)
	{
		self.mailboxes = None;
		self.mailsData.clear();
		self.pendingSeen.clear();
		self.confirmedSeen.clear();
		self.boxs.clear();
		self.syncIdentity = Some(syncIdentity);
	}
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Mail
{
	config: ArcRwSignal<MailConfig>,
	#[serde(skip_serializing,skip_deserializing)]
	mailsClientCache: ArcRwSignal<MailsContent>,
	_update: ArcRwSignal<Cache>,
	_sended: ArcRwSignal<Cache>,
}

impl Mail
{
	fn draw_config(getBoxsMailConfig: ArcRwSignal<MailConfig>, getBoxsMailsCache: ArcRwSignal<MailsContent>, update: ArcRwSignal<Cache>, moduleActions: ModuleActionFn) -> AnyView
	{
		let getBoxsMailConfigInner = getBoxsMailConfig.clone();
		let getBoxsMailsCacheInner = getBoxsMailsCache.clone();
		let getBoxsConfigCache = update.clone();
		let toaster = expect_toaster();
		let moduleActionsGetBox = moduleActions.clone();
		let getBoxsFn = move |_| {
			let toaster = toaster.clone();
			let getBoxsMailConfig = getBoxsMailConfigInner.clone();
			let getBoxsMailContent = getBoxsMailsCacheInner.clone();
			let getBoxsConfigCache = getBoxsConfigCache.clone();
			let moduleActionsTask = moduleActionsGetBox.clone();
			moduleActionsGetBox.task_spawn(async move {
				let apiResult = API_proxys_imap_listbox(getBoxsMailConfig.get_untracked().imap.clone()).await;
				if (!moduleActionsTask.lifecycle_isActive())
				{
					return;
				}
				if let Some(result) = toaster_api(&toaster,apiResult, None).await
				{
					if (!moduleActionsTask.lifecycle_isActive())
					{
						return;
					}
					let mut connector = getBoxsMailConfig.get_untracked().imap.clone();
					if (connector.boxSelection_migrate(&result))
					{
						let updatedConnector = connector.clone();
						getBoxsMailConfig.update(|config| config.imap = updatedConnector);
						getBoxsConfigCache.update(|cache| cache.update());
					}
					getBoxsMailContent.update(|mailContent| mailContent.topology_set(&result,&connector));
				}
			});
		};


		let mut titleF = FieldHelper::new(&getBoxsMailConfig,&update,"MODULE_TITLE_CONF",
		                                  |d| d.get().title,
		                                  |ev,inner| inner.title = ev.target().value());
		titleF.setFullSize(true);
		let mut mailAsTagF = FieldHelper::new(&getBoxsMailConfig,&update,"MODULE_MAIL_ASTAG",
		                                  |d| d.get().mailAsTag,
		                                  |ev,inner| inner.mailAsTag = ev.target().value());
		mailAsTagF.setFullSize(true);
		let hostF = FieldHelper::new(&getBoxsMailConfig,&update,"MODULE_MAIL_HOST",
		                                  |d| d.get().imap.host,
		                                  |ev,inner| inner.imap.host = ev.target().value());
		let mut portF = FieldHelper::new(&getBoxsMailConfig,&update,"",
		                              |d| d.get().imap.port.to_string(),
		                              |ev,inner| inner.imap.port = ev.target().value().parse::<u16>().unwrap_or(993));
		portF.setInputType(FieldHelperType::NUMBER(1,65535));
		portF.setStyle("width:90px");
		let usernameF = FieldHelper::new(&getBoxsMailConfig,&update,"MODULE_MAIL_USERNAME",
		                              |d| d.get().imap.username,
		                              |ev,inner| inner.imap.username = ev.target().value());
		let mut passwordF = FieldHelper::new(&getBoxsMailConfig,&update,"MODULE_MAIL_PASSWORD",
		                              |d| d.get().imap.password,
		                              |ev,inner| inner.imap.password = ev.target().value());
		passwordF.setInputType(FieldHelperType::PASSWORD);
		let remoteImageAllowListConfig = getBoxsMailConfig.clone();
		let remoteImageAllowListCache = update.clone();

		view!{
			<div class="module_mail_config">
				{titleF.draw()}
				{mailAsTagF.draw()}
				{hostF.draw()}:{portF.draw()}<br/>
				{usernameF.draw()}<br/>
				{passwordF.draw()}<br/>
				{
					move || {
						let allowedAddresses = remoteImageAllowListConfig.get().remoteImageSenderAllowList_get();
						if (allowedAddresses.is_empty())
						{
							return view!{}.into_any();
						}
						return view!{
							<div class="module_mail_remote_images_allowlist">
								<span class="module_mail_remote_images_allowlist_title">
									<Translate key="MODULE_MAIL_REMOTE_IMAGES_ALLOWLIST"/>
								</span>
								<div class="module_mail_remote_images_allowlist_entries">
									{allowedAddresses.into_iter().map(|senderAddress| {
										let senderAddressContent = senderAddress.clone();
										let remoteImageAllowListConfig = remoteImageAllowListConfig.clone();
										let remoteImageAllowListCache = remoteImageAllowListCache.clone();
										return view!{
											<button type="button" class="module_mail_remote_images_allowlist_entry" on:click={move |_| {
												let mut changed = false;
												remoteImageAllowListConfig.update(|config| {
													changed = config.remoteImageSender_remove(&senderAddress);
												});
												if (changed)
												{
													remoteImageAllowListCache.update(|cache| cache.update());
												}
											}}>
												<span>{senderAddressContent}</span><span class="module_mail_remote_images_allowlist_remove">{"×"}</span>
											</button>
										};
									}).collect_view()}
								</div>
							</div>
						}.into_any();
					}
				}
				<button on:click={getBoxsFn}><Translate key="MODULE_MAIL_GETBOXS"/></button>
				{
					let boxConfig = getBoxsMailConfig.clone();
					let boxConfigCache = update.clone();
					let boxTopology = getBoxsMailsCache.clone();
					let switchBoxFn = move |boxName:String,isSelected:bool| {
						boxConfig.update(|mailContent|{
							mailContent.imap.boxSelection_set(boxName.clone(),!isSelected);
							boxConfigCache.update(|cache|{
								cache.update();
							});
						});
						let connector = boxConfig.get_untracked().imap.clone();
						boxTopology.update(|mailContent| {
							let boxes = mailContent.boxs.clone();
							mailContent.topology_set(&boxes,&connector);
						});
					};

					let mailsCache = getBoxsMailsCache.clone().get();
					let configBoxContent = getBoxsMailConfig.clone().get();
					if(!mailsCache.boxs.is_empty())
					{
						view!{
							<hr/>
							<Translate key="MODULE_MAIL_BOXS_LIST"/><br/>
							{mailsCache.boxs.iter().map(|boxContent| {
								let boxName = boxContent.name.clone();
								if (boxContent.attributes.is_uninteresting())
								{
									return view!{<span class="disabled uninteresting boxmail">{boxName}</span>}.into_any();
								}
								let switchBoxFn = switchBoxFn.clone();
								let isSelected = configBoxContent.imap.isBoxSelected(&boxName);
								let boxNameContent = boxName.clone();
								view!{
									<span class={if isSelected {"boxmail"} else {"disabled boxmail"}}
										on:click={move |_|switchBoxFn(boxName.clone(),isSelected)}>
										{boxNameContent}
									</span>
								}.into_any()
							}).collect_view()}
						}.into_any()
					}
					else {view!{}.into_any()}
				}
			</div>
			}.into_any()
	}

	fn mail_mark_see(imapConnector: imap_connector, toaster: ToasterContext, mailKey: ImapMailKey, mailsContent: ArcRwSignal<MailsContent>, moduleActions: ModuleActionFn)
	{
		// The request survives the dialog Owner, but remains owned by the active ModuleHolder lifecycle.
		let moduleActionsTask = moduleActions.clone();
		moduleActions.task_spawn(async move {
			// we remove the old data sooner to improve reactivity and re-add them later if something gone wrong
			let oldMail;
			{
				let Some(mut binding) = mailsContent.try_write()
				else
				{
					return;
				};
				let mailsDatas: &mut MailsContent = binding.deref_mut();
				oldMail = mailsDatas.mailSeen_begin(&mailKey);
			}

			let apiResult = API_proxys_imap_setMailSee(imapConnector,mailKey.clone()).await;
			if (!moduleActionsTask.lifecycle_isActive())
			{
				return;
			}
			let requestFailed = toaster_api(&toaster, apiResult, None).await.is_none();
			if (!moduleActionsTask.lifecycle_isActive())
			{
				return;
			}
			if (requestFailed)
			{
				mailsContent.update(|mailContent| mailContent.mailSeen_rollback(mailKey,oldMail));
				return;
			};
			mailsContent.update(|mailContent| mailContent.mailSeen_commit(mailKey));

		});
	}

	fn mail_view_content(mailConfig: ArcRwSignal<MailConfig>, configUpdate: ArcRwSignal<Cache>, toaster: ToasterContext, dialogManager: DialogManager, mailKey: ImapMailKey, mailIdContent: ImapMail, mailsCache: ArcRwSignal<MailsContent>, moduleActions: ModuleActionFn, moduleId: ModuleID)
	{
		// The resulting dialog can outlive the mail row Owner, but not the holder lifecycle.
		let imapConnector = mailConfig.get_untracked().imap.clone();
		let moduleActionsTask = moduleActions.clone();
		moduleActions.task_spawn(async move {
			let apiResult = API_proxys_imap_getMailContent(imapConnector.clone(),mailKey.clone()).await;
			if (!moduleActionsTask.lifecycle_isActive())
			{
				return;
			}
			let Some(mailContent) = toaster_api(&toaster, apiResult, None).await else {return};
			if (!moduleActionsTask.lifecycle_isActive())
			{
				return;
			}

			let toasterBody = toaster.clone();
			let mailIdContentBody = mailIdContent.clone();
			let contentFrameBody = MailContentFrame::new(mailContent.content.clone());
			let mailContentBody = Arc::new(mailContent);
			let senderAddressBody = MailSenderAddress::from_header(&mailIdContent.from);
			let remoteImagesInitiallyAllowedBody = senderAddressBody.as_ref().is_some_and(|senderAddress| {
				return mailConfig.get_untracked().remoteImageSender_isAllowed(senderAddress);
			});
			let mailConfigBody = mailConfig.clone();
			let configUpdateBody = configUpdate.clone();
			let moduleActionsBody = moduleActionsTask.clone();
			let moduleIdBody = moduleId.clone();
			let moduleActionsValidate = moduleActionsTask.clone();
			let mailKeyValidate = mailKey.clone();

			let dialogContent = DialogData::new()
				.setTitle(mailIdContent.subject.clone().map(|subject|format!("€{}", subject)).unwrap_or("MODULE_MAIL_NO_SUBJECT".to_string()))
				.setBody(move || {
					let mailId = mailIdContentBody.clone();
					let mailContent = mailContentBody.clone();
					let contentFrame = contentFrameBody.clone();
					let senderAddress = senderAddressBody.clone();
					let mailConfig = mailConfigBody.clone();
					let configUpdate = configUpdateBody.clone();
					let moduleId = moduleIdBody.clone();
					let remoteImagesAllowed = ArcRwSignal::new(remoteImagesInitiallyAllowedBody);

					let moduleActionsDownload = moduleActionsBody.clone();
					let moduleActionsRemoteImages = moduleActionsBody.clone();
					let downloadAttachement = move |attachement: Attachment, toaster: ToasterContext| {
						Self::attachment_download(attachement,toaster,moduleActionsDownload.clone());
					};

					let toasterInner = toasterBody.clone();
					view!{
						<div class="module_mail_content_parent">
							<span><b><Translate key="MODULE_MAIL_FROM"/></b>{" "}{mailId.from}</span>
							<span><b><Translate key="MODULE_MAIL_TO"/></b>{" "}{mailId.to}</span>
							<span><b><Translate key="MODULE_MAIL_DATE"/></b>{" "}{
								let date = UtcDateTime::from_unix_timestamp(mailId.date).unwrap_or(UtcDateTime::now());
								format!("{:0>2}/{:0>2}/{:0>4} {:0>2}:{:0>2}:{:0>2}",date.day(),date.month() as u8,date.year(),date.hour(),date.minute(),date.second())
							}</span>
							{
							let views = mailContent.attachement.iter().enumerate().map(|(attachmentIndex,att)| {
								let attachmentMailContent = mailContent.clone();
								let downloadAttachement = downloadAttachement.clone();
									return match &att.filename {
										None => {view!{{" "}<span class="attachement" on:click={
												let toasterInner = toasterInner.clone();
												move |_| {
													if let Some(attachment) = attachmentMailContent.attachement.get(attachmentIndex).cloned()
													{
														downloadAttachement(attachment,toasterInner.clone());
													}
												}
											}><i class="iconoir-doc-magnifying-glass"/>{" "}<Translate key="MODULE_MAIL_NO_SUBJECT"/></span>}}.into_any(),
										Some(filename) => {
											let attachmentMailContent = attachmentMailContent.clone();
											view!{{" "}<span class="attachement"  on:click={
												let toasterInner = toasterInner.clone();
												move |_| {
													if let Some(attachment) = attachmentMailContent.attachement.get(attachmentIndex).cloned()
													{
														downloadAttachement(attachment,toasterInner.clone());
													}
												}
											}><i class="iconoir-doc-magnifying-glass"/>{" "}{filename.clone()}</span>}.into_any()
										}
									};
								});

								if(views.len() > 0)
								{
									view!{<span><b><Translate key="MODULE_MAIL_ATTACHEMENT"/></b>{views.collect_view()}</span>}.into_any()
								}
								else {view!{}.into_any()}
							}
							{
								let contentFrame = contentFrame.clone();
								let remoteImagesAllowed = remoteImagesAllowed.clone();
								let senderAddress = senderAddress.clone();
								let mailConfig = mailConfig.clone();
								let configUpdate = configUpdate.clone();
								let moduleId = moduleId.clone();
								let moduleActionsRemoteImages = moduleActionsRemoteImages.clone();
								move || {
									if (!contentFrame.remoteImagesControl_isAvailable() || remoteImagesAllowed.get())
									{
										return view!{}.into_any();
									}
									let remoteImagesAllowedOnce = remoteImagesAllowed.clone();
									let senderPersistentAction = if let Some(senderAddress) = senderAddress.clone()
									{
										let remoteImagesAllowed = remoteImagesAllowed.clone();
										let mailConfig = mailConfig.clone();
										let configUpdate = configUpdate.clone();
										let moduleId = moduleId.clone();
										let moduleActionsRemoteImages = moduleActionsRemoteImages.clone();
										view!{
											<button type="button" class="module_mail_remote_images_button module_mail_remote_images_sender_button" on:click={move |_| {
												if (!moduleActionsRemoteImages.lifecycle_isActive())
												{
													return;
												}
												remoteImagesAllowed.set(true);
												let mut changed = false;
												mailConfig.update(|config| {
													changed = config.remoteImageSender_allow(&senderAddress);
												});
								if (changed)
								{
									configUpdate.update(|cache| cache.update());
									(moduleActionsRemoteImages.updateFn)(moduleId.clone());
								}
											}}>
												<Translate key="MODULE_MAIL_ALWAYS_LOAD_REMOTE_IMAGES"/>
											</button>
										}.into_any()
									}
									else
									{
										view!{}.into_any()
									};
									return view!{
										<div class="module_mail_remote_images">
											<span class="module_mail_remote_images_message"><Translate key="MODULE_MAIL_REMOTE_IMAGES_BLOCKED"/></span>
											<div class="module_mail_remote_images_actions">
												<button type="button" class="module_mail_remote_images_button" on:click={move |_|remoteImagesAllowedOnce.set(true)}>
													<Translate key="MODULE_MAIL_LOAD_REMOTE_IMAGES"/>
												</button>
												{senderPersistentAction}
											</div>
										</div>
									}.into_any();
								}
							}
							<div class="module_mail_content_frame">
								{
									let contentFrame = contentFrame.clone();
									let remoteImagesAllowed = remoteImagesAllowed.clone();
									view!{
										<iframe srcdoc={move || contentFrame.srcdoc_get(remoteImagesAllowed.get())}
											sandbox="allow-popups allow-popups-to-escape-sandbox"
											referrerpolicy="no-referrer"></iframe>
									}
								}
							</div>
						</div>
					}.into_any()
				})
				.setButtonValidateTitle(Some("MODULE_MAIL_MAILCONTENTSEEN"))
				.setOnValidate(move |_| {
					Self::mail_mark_see(imapConnector.clone(), toaster.clone(), mailKeyValidate.clone(), mailsCache.clone(), moduleActionsValidate.clone());
					return true;
				})
				.setIsLarger(true);

			dialogManager.open(dialogContent);
		});
	}

	fn attachment_download(attachement: Attachment, toaster: ToasterContext, moduleActions: ModuleActionFn)
	{
		if (!moduleActions.lifecycle_isActive())
		{
			return;
		}
		if (!download_attachment(attachement))
		{
			moduleActions.task_spawn(async move {
				toastingErr(&toaster, "MODULE_MAIL_BLOBCREATORERROR").await;
			});
		}
	}

	async fn sync(toaster: ToasterContext, mailContentRaw: ArcRwSignal<MailsContent>, config: ArcRwSignal<MailConfig>, moduleActions: ModuleActionFn)
	{
		if (!moduleActions.lifecycle_isActive())
		{
			return;
		}
		let config = config.get_untracked();
		let request = {
			let Some(mut mailContent) = mailContentRaw.try_write()
			else
			{
				return;
			};
			mailContent.syncRequest_get(&config.imap)
		};
		let Some(request) = request
		else
		{
			return;
		};
		let apiResult = API_proxys_imap_sync(config.imap.clone(),request).await;
		if (!moduleActions.lifecycle_isActive())
		{
			return;
		}
		let Some(mailboxes) = toaster_api(&toaster,apiResult,None).await
		else
		{
			if (moduleActions.lifecycle_isActive())
			{
				toastingErr(&toaster,"MODULE_MAIL_SYNCERROR".to_string()).await;
			}
			return;
		};
		if (!moduleActions.lifecycle_isActive())
		{
			return;
		}
		mailContentRaw.update(move |mailContent| mailContent.sync_apply(mailboxes));
	}

	fn utils_mailOverlay(mail: &ImapMail) -> AnyView
	{

		return view!{
			<div class="alttext">
				<span><Translate key="MODULE_MAIL_FROM"/>{" "}{mail.from.clone()}</span><br/>
				<span><Translate key="MODULE_MAIL_TO"/>{" "}{mail.to.clone()}</span><br/>
				<span><Translate key="MODULE_MAIL_DATE"/>{" "}{
					let date = UtcDateTime::from_unix_timestamp(mail.date).unwrap_or(UtcDateTime::now());
					format!("{:0>2}/{:0>2}/{:0>4} {:0>2}:{:0>2}:{:0>2}",date.day(),date.month() as u8,date.year(),date.hour(),date.minute(),date.second())
				}</span>
			</div>
		}.into_any();
	}
}

impl ModuleName for Mail
{
	const MODULE_NAME: &'static str = "MAIL";
}

impl Backable for Mail
{
	fn module_name(&self) -> String {
		Mail::MODULE_NAME.to_string()
	}

	fn draw(&self, editMode: RwSignal<bool>, moduleActions: ModuleActionFn, moduleId: ModuleID) -> ViewFn {
		let configInner = self.config.clone();
		let clientCacheInner = self.mailsClientCache.clone();
		let updateInner = self._update.clone();
		ViewFn::from(move || {
			view! {
				<MailDraw config=configInner.clone() mailsClientCache=clientCacheInner.clone() update=updateInner.clone() editMode=editMode moduleActions=moduleActions.clone() moduleId=moduleId.clone()/>
			}.into_any()
		})

	}

	fn refresh_time(&self) -> RefreshTime {
		RefreshTime::MINUTES(30)
	}

	fn refresh(&self, moduleActions: ModuleActionFn, moduleId: ModuleID, toaster: ToasterContext) -> Option<BoxFuture> {
		let config = self.config.clone();
		let mailsCache = self.mailsClientCache.clone();
		let tmp = Self::sync(toaster, mailsCache, config, moduleActions);
		return Some(Box::pin(async move {
			tmp.await;
		}));
	}


	fn export(&self) -> ModuleContent {
		return ModuleContent{
			id: ModuleID::new(),
			typeModule: self.module_name(),
			timestamp: self._update.get_untracked().get(),
			content: serde_json::to_string(&self.config.get_untracked()).unwrap_or_default(),
			..Default::default()
		};
	}

	fn import(&mut self, import: ModuleContent)
	{
		let Ok(content): Result<MailConfig,_> = serde_json::from_str(&import.content.clone()) else {return};

		self.config.update(|config|{
			*config = content;
		});
		self._update.update(|cache|{
			cache.update_from(import.timestamp);
		});
		self._sended.update(|cache|{
			cache.update_from(import.timestamp);
		});
	}

	fn isOlderThan(&self, other: &ModuleContent) -> bool
	{
		return other.timestamp > self._update.get_untracked().get();
	}

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self> {
		let Ok(content): Result<MailConfig,_> = serde_json::from_str(&from.content) else {return None};
		Some(Self {
			config: ArcRwSignal::new(content),
			mailsClientCache: Default::default(),
			_update: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
			_sended: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
		})
	}

	fn size(&self) -> ModuleSizeContrainte {
		ModuleSizeContrainte{
			x_min: Some(250),
			x_max: None,
			y_min: Some(200),
			y_max: None,
		}
	}
}

impl Cacheable for Mail
{
	fn cache_time(&self) -> i64 {
		self._update.get_untracked().get()
	}

	fn cache_mustUpdate(&self) -> bool
	{
		return self._update.get_untracked().isNewer(&self._sended.get());
	}

	fn cache_getUpdate(&self) -> ArcRwSignal<Cache> {
		return self._update.clone();
	}

	fn cache_getSended(&self) -> ArcRwSignal<Cache> {
		return self._sended.clone();
	}
}

#[component]
fn MailDraw(config: ArcRwSignal<MailConfig>,
	            mailsClientCache: ArcRwSignal<MailsContent>,
	            update: ArcRwSignal<Cache>,
	            editMode: RwSignal<bool>,
	            moduleActions: ModuleActionFn,
	            moduleId: ModuleID) -> impl IntoView
{
	let Some(dialogManager) = use_context::<DialogManager>() else {
		HWebTrace!("cannot get dialogManager in link");
		return view!{}.into_any();
	};
	let toaster = expect_toaster();

	let mailConfigView = config.clone();
	let configUpdateView = update.clone();
	let toasterInner = toaster.clone();
	let mailsCache = mailsClientCache.clone();
	let moduleActionsView = moduleActions.clone();
	let moduleIdView = moduleId.clone();
	let viewContentFn = move |mailKey:ImapMailKey,mailData:ImapMail| {
		Mail::mail_view_content(mailConfigView.clone(), configUpdateView.clone(), toasterInner.clone(), dialogManager.clone(), mailKey, mailData, mailsCache.clone(), moduleActionsView.clone(), moduleIdView.clone());
	};

	let imapConnector = config.clone();
	let mailsCache = mailsClientCache.clone();
	let moduleActionsMark = moduleActions.clone();
	let markViewFn = move |mailKey:ImapMailKey| {
		Mail::mail_mark_see(imapConnector.get_untracked().imap.clone(), toaster.clone(), mailKey, mailsCache.clone(), moduleActionsMark.clone());
	};


	view!{{move || {
			let editMode = editMode.get();
			if(editMode)
			{
				Mail::draw_config(config.clone(), mailsClientCache.clone(), update.clone(), moduleActions.clone())
			}
			else
			{
				let config = config.clone();
				let mailsCache = mailsClientCache.clone();
				let mailConfig = config.get();
				let mailTagIsActive = mailConfig.mail_tag_is_active();
				view!{
					{draw_title_if_present(mailConfig.title.clone())}
					<div class="module_rss_upper">
						<table class="module_rss_table module_mail_table">{
							let markVueCacheInner = mailsCache.clone();
							let mails = mailsCache.get().mailsData.clone();
							let mut mailsContent = mails.into_iter().collect::<Vec<_>>();
							mailsContent.sort_by(|(_,left),(_,right)| left.date.cmp(&right.date).reverse());
							mailsContent.into_iter()
								.map(|(mailKey,mail)|{
									let mailKeyView = mailKey.clone();
									let mailKeyMark = mailKey.clone();
									let mailKeyConfirm = mailKey.clone();
									let mailView = mail.clone();
									let viewContentFn = viewContentFn.clone();
									let markViewFn = markViewFn.clone();
									let markVueCacheInner = markVueCacheInner.clone();
									let mailTag = if(mailTagIsActive) {mailConfig.mail_tag(&mail)} else {None};
									view!{
										<tr>
											<td class="module_mail_date">{distant_time_simpler(mail.date)}</td>
											{
												if(mailTagIsActive)
												{
													view!{
														<td class="module_mail_tag_cell">{
															if let Some(mailTag) = mailTag
															{
																let style = mailTag.style();
																view!{
																	<span class="module_mail_tag" style={style}>
																		<span class="module_mail_tag_label">{mailTag.label}</span>
																	</span>
																}.into_any()
															}
															else {view!{}.into_any()}
														}</td>
													}.into_any()
												}
												else {view!{}.into_any()}
											}
							<td class="module_mail_subject mail_pointer alttext_upper" on:click={move |_| viewContentFn.clone()(mailKeyView.clone(),mailView.clone())}>{mail.subject.clone()}{Mail::utils_mailOverlay(&mail)}</td>
							<td class="module_mail_status">{
								if(mail.confirmVue)
								{
									view!{<i class="iconoir-mail-out-solid" on:click={move |_| markViewFn.clone()(mailKeyMark.clone())}/>}.into_any()
								}
								else
								{
									view!{<i class="iconoir-mail-open" on:click={move |_| {
										let markVueCacheInnerInner = markVueCacheInner.clone();
										let mailKeyInner = mailKeyConfirm.clone();
										markVueCacheInner.update(|mailCache|{
											mailCache.confirmation_set(&mailKeyInner,true);
										});
										Timeout::new(5000, move || {
											markVueCacheInnerInner.update(|mailCache| mailCache.confirmation_set(&mailKeyInner,false));
										}).forget();
									}}/>}.into_any()
								}
							}</td>
										</tr>
									}
								}).collect_view()
							}
						</table>
				</div>}.into_any()
			}
	}}}.into_any()
}

#[cfg(test)]
mod tests
{
	use std::collections::HashMap;

	use super::{MailConfig, MailContentFrame, MailSenderAddress, MailSyncIdentity, MailTag, MailsContent};
	use crate::api::proxys::imap_components::{Attributs, BoxName, imap_connector, imap_connector_extra, ImapMailboxSync, ImapMail, ImapMailContentType, ImapMailKey};

	fn config_with_suffix(suffix: &str) -> MailConfig
	{
		return MailConfig {
			mailAsTag: suffix.to_string(),
			..Default::default()
		};
	}

	fn mail_with_addresses(from: &str, to: &str) -> ImapMail
	{
		return ImapMail {
			from: from.to_string(),
			to: to.to_string(),
			..Default::default()
		};
	}

	fn mailKey_get(boxName: &str, uidValidity: u32, uid: u32) -> ImapMailKey
	{
		return ImapMailKey {boxName: boxName.to_string(),uidValidity,uid};
	}

	fn mailboxSync_get(boxName: &str, uidValidity: u32, removedUids: Vec<u32>, mailUids: Vec<u32>) -> ImapMailboxSync
	{
		return ImapMailboxSync {
			boxName: boxName.to_string(),
			uidValidity,
			removedUids,
			mails: mailUids.into_iter().map(|uid| ImapMail {uid,..Default::default()}).collect(),
		};
	}

	fn boxName_get(name: &str, attributes: Attributs) -> BoxName
	{
		return BoxName {name: name.to_string(),attributes};
	}

	#[test]
	fn mail_config_without_mail_as_tag_uses_empty_value()
	{
		let mut serializedConfig = serde_json::to_value(MailConfig::default()).unwrap();
		serializedConfig.as_object_mut().unwrap().remove("mailAsTag");

		let config: MailConfig = serde_json::from_value(serializedConfig).unwrap();

		assert_eq!(config.mailAsTag,"");
		assert!(!config.mail_tag_is_active());
	}

	#[test]
	fn mail_config_without_remote_image_allowlist_uses_empty_list()
	{
		let mut serializedConfig = serde_json::to_value(MailConfig::default()).unwrap();
		serializedConfig.as_object_mut().unwrap().remove("remoteImageSenderAllowList");

		let config: MailConfig = serde_json::from_value(serializedConfig).unwrap();

		assert!(config.remoteImageSenderAllowList.is_empty());
	}

	#[test]
	fn mail_config_roundtrip_keeps_remote_image_allowlist()
	{
		let senderAddress = MailSenderAddress::from_header("Newsletter <news@example.com>").unwrap();
		let mut config = MailConfig::default();
		assert!(config.remoteImageSender_allow(&senderAddress));

		let serializedConfig = serde_json::to_string(&config).unwrap();
		let restoredConfig: MailConfig = serde_json::from_str(&serializedConfig).unwrap();

		assert!(restoredConfig.remoteImageSender_isAllowed(&senderAddress));
	}

	#[test]
	fn mailSenderAddress_normalizesExactAddressWithoutDisplayName()
	{
		let senderAddress = MailSenderAddress::from_header("Newsletter <News@Example.COM>").unwrap();

		assert_eq!(senderAddress.as_str(),"news@example.com");
	}

	#[test]
	fn remoteImageSenderAllowList_matchesOnlyExactNormalizedAddress()
	{
		let senderAddress = MailSenderAddress::from_header("Newsletter <News@Example.COM>").unwrap();
		let sameSenderAddress = MailSenderAddress::from_header("NEWS@example.com").unwrap();
		let otherSenderAddress = MailSenderAddress::from_header("other@example.com").unwrap();
		let mut config = MailConfig::default();

		assert!(config.remoteImageSender_allow(&senderAddress));
		assert!(!config.remoteImageSender_allow(&sameSenderAddress));
		assert!(config.remoteImageSender_isAllowed(&sameSenderAddress));
		assert!(!config.remoteImageSender_isAllowed(&otherSenderAddress));
		assert_eq!(config.remoteImageSenderAllowList_get(),vec!["news@example.com"]);
		assert!(config.remoteImageSender_remove("NEWS@example.com"));
		assert!(!config.remoteImageSender_isAllowed(&senderAddress));
	}

	#[test]
	fn mailSenderAddress_rejectsHeaderWithoutUsableAddress()
	{
		assert_eq!(MailSenderAddress::from_header("Newsletter"),None);
		assert_eq!(MailSenderAddress::from_header("bad@@example.com"),None);
	}

	#[test]
	fn mail_tag_empty_suffix_is_disabled()
	{
		let config = config_with_suffix("  ");
		let mail = mail_with_addresses("sender@site.com","toto@site.com");

		assert!(!config.mail_tag_is_active());
		assert_eq!(config.mail_tag(&mail),None);
	}

	#[test]
	fn mail_tag_uses_matching_to_local_part()
	{
		let config = config_with_suffix("site.com");
		let mail = mail_with_addresses("sender@site.com","toto@site.com");

		assert_eq!(config.mail_tag(&mail).map(|tag| tag.label),Some("toto".to_string()));
	}

	#[test]
	fn mail_tag_accepts_display_name_and_domain_case()
	{
		let config = config_with_suffix("site.com");
		let mail = mail_with_addresses("sender@other.com","Toto <toto@SITE.COM>");

		assert_eq!(config.mail_tag(&mail).map(|tag| tag.label),Some("toto".to_string()));
	}

	#[test]
	fn mail_tag_selects_first_matching_recipient()
	{
		let config = config_with_suffix("@site.com");
		let mail = mail_with_addresses("sender@other.com","Other <other@else.com>, Toto <toto@site.com>; tata@site.com");

		assert_eq!(config.mail_tag(&mail).map(|tag| tag.label),Some("toto".to_string()));
	}

	#[test]
	fn mail_tag_rejects_other_domain_and_ignores_from()
	{
		let config = config_with_suffix("site.com");
		let mail = mail_with_addresses("sender@site.com","toto@other-site.com");

		assert_eq!(config.mail_tag(&mail),None);
	}

	#[test]
	fn mail_tag_color_is_stable_for_same_normalized_label()
	{
		let lowerTag = MailTag::new("toto");
		let upperTag = MailTag::new("TOTO");

		assert_eq!(lowerTag.color,upperTag.color);
		assert_eq!(lowerTag.style(),upperTag.style());
		assert!(lowerTag.style().starts_with("--mail-tag-color:hsl("));
	}

	#[test]
	fn mailContentFrameEscapesPlainTextBeforeSrcdocRendering()
	{
		let frame = MailContentFrame::new(ImapMailContentType::Text(
			"<script>alert('x')</script>\nSafe & sound".to_string(),
		));
		let srcdoc = frame.srcdoc_get(false);

		assert!(!srcdoc.contains("<script>alert"));
		assert!(srcdoc.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"));
		assert!(srcdoc.contains("Safe &amp; sound"));
		assert!(srcdoc.contains("white-space:pre-wrap"));
	}

	#[test]
	fn mailContentFrameAllowsRemoteImagesOnlyAfterLocalChoice()
	{
		let frame = MailContentFrame::new(ImapMailContentType::Html(
			"<img src=\"https://tracker.example/pixel\">".to_string(),
		));
		let blocked = frame.srcdoc_get(false);
		let allowed = frame.srcdoc_get(true);

		assert!(frame.remoteImagesControl_isAvailable());
		assert!(blocked.contains("default-src 'none'"));
		assert!(blocked.contains("form-action 'none'"));
		assert!(blocked.contains("img-src data: blob:;"));
		assert!(!blocked.contains("img-src data: blob: http: https:;"));
		assert!(allowed.contains("img-src data: blob: http: https:;"));
	}

	#[test]
	fn mailCache_keepsSameUidFromDifferentMailboxes()
	{
		let mut cache = MailsContent::default();
		cache.sync_apply(vec![
			mailboxSync_get("Alerts",42,Vec::new(),vec![7]),
			mailboxSync_get("News",99,Vec::new(),vec![7]),
		]);

		assert_eq!(cache.mailsData.len(),2);
		assert!(cache.mailsData.contains_key(&mailKey_get("Alerts",42,7)));
		assert!(cache.mailsData.contains_key(&mailKey_get("News",99,7)));
	}

	#[test]
	fn mailCache_removesOnlyTheMailboxScopedUid()
	{
		let mut cache = MailsContent::default();
		cache.sync_apply(vec![
			mailboxSync_get("Alerts",42,Vec::new(),vec![7]),
			mailboxSync_get("News",99,Vec::new(),vec![7]),
		]);
		cache.sync_apply(vec![
			mailboxSync_get("Alerts",42,vec![7],Vec::new()),
			mailboxSync_get("News",99,Vec::new(),Vec::new()),
		]);

		assert!(!cache.mailsData.contains_key(&mailKey_get("Alerts",42,7)));
		assert!(cache.mailsData.contains_key(&mailKey_get("News",99,7)));
	}

	#[test]
	fn mailCache_rebuildsOnlyMailboxWhoseUidValidityChanged()
	{
		let mut cache = MailsContent::default();
		cache.sync_apply(vec![
			mailboxSync_get("Alerts",42,Vec::new(),vec![7]),
			mailboxSync_get("News",99,Vec::new(),vec![7]),
		]);
		cache.sync_apply(vec![
			mailboxSync_get("Alerts",43,Vec::new(),vec![1]),
			mailboxSync_get("News",99,Vec::new(),Vec::new()),
		]);

		assert!(!cache.mailsData.contains_key(&mailKey_get("Alerts",42,7)));
		assert!(cache.mailsData.contains_key(&mailKey_get("Alerts",43,1)));
		assert!(cache.mailsData.contains_key(&mailKey_get("News",99,7)));
	}

	#[test]
	fn mailCache_acceptsMailThatBecomesUnreadAgain()
	{
		let mut cache = MailsContent::default();
		cache.sync_apply(vec![mailboxSync_get("Alerts",42,Vec::new(),vec![7])]);
		cache.sync_apply(vec![mailboxSync_get("Alerts",42,vec![7],Vec::new())]);
		assert!(cache.mailsData.is_empty());

		cache.sync_apply(vec![mailboxSync_get("Alerts",42,Vec::new(),vec![7])]);
		assert!(cache.mailsData.contains_key(&mailKey_get("Alerts",42,7)));
	}

	#[test]
	fn mailCache_doesNotReinsertMailWhileSeenStoreIsPending()
	{
		let mut cache = MailsContent::default();
		let key = mailKey_get("Alerts",42,7);
		cache.sync_apply(vec![mailboxSync_get("Alerts",42,Vec::new(),vec![7])]);
		let removedMail = cache.mailSeen_begin(&key);
		assert!(!cache.mailsData.contains_key(&key));

		cache.sync_apply(vec![mailboxSync_get("Alerts",42,Vec::new(),vec![7])]);
		assert!(!cache.mailsData.contains_key(&key));
		assert!(cache.pendingSeen.contains(&key));

		cache.mailSeen_rollback(key.clone(),removedMail);
		assert!(cache.mailsData.contains_key(&key));
		assert!(!cache.pendingSeen.contains(&key));
	}

	#[test]
	fn mailCacheRediscoversUnreadMailAfterSeenReconciliation()
	{
		let mut cache = MailsContent::default();
		let key = mailKey_get("Alerts",42,7);
		cache.sync_apply(vec![mailboxSync_get("Alerts",42,Vec::new(),vec![7])]);
		cache.mailSeen_begin(&key);
		cache.mailSeen_commit(key.clone());

		cache.sync_apply(vec![mailboxSync_get("Alerts",42,Vec::new(),Vec::new())]);
		assert!(!cache.pendingSeen.contains(&key));
		assert!(!cache.confirmedSeen.contains(&key));
		assert!(!cache.mailsData.contains_key(&key));

		cache.sync_apply(vec![mailboxSync_get("Alerts",42,Vec::new(),vec![7])]);
		assert!(cache.mailsData.contains_key(&key));
	}

	#[test]
	fn mailCache_groupsKnownUidsByMailboxInSyncRequest()
	{
		let mut connector = imap_connector::default();
		connector.extra = Some(imap_connector_extra::default());
		let mut cache = MailsContent::default();
		cache.syncIdentity = Some(MailSyncIdentity::new(&connector));
		cache.mailboxes = Some([
			("News".to_string(),Some(99)),
			("Alerts".to_string(),Some(42)),
		].into_iter().collect());
		cache.mailsData.insert(mailKey_get("Alerts",42,7),ImapMail {uid: 7,..Default::default()});
		cache.mailsData.insert(mailKey_get("News",99,7),ImapMail {uid: 7,..Default::default()});

		let request = cache.syncRequest_get(&connector).unwrap();
		let mailboxes = request.mailboxes.unwrap();
		assert_eq!(mailboxes.len(),2);
		assert_eq!(mailboxes[0].boxName,"Alerts");
		assert_eq!(mailboxes[0].knownUids,vec![7]);
		assert_eq!(mailboxes[1].boxName,"News");
		assert_eq!(mailboxes[1].knownUids,vec![7]);
	}

	#[test]
	fn mailCache_doesNotSyncBeforeExplicitSelection()
	{
		let connector = imap_connector::default();
		let mut cache = MailsContent::default();

		assert!(cache.syncRequest_get(&connector).is_none());
		assert!(cache.mailboxes.is_none());
	}

	#[test]
	fn mailCache_doesNotSyncAnExplicitEmptySelection()
	{
		let mut connector = imap_connector::default();
		connector.extra = Some(imap_connector_extra {
			boxAllowList: Some(Vec::new()),
			..Default::default()
		});
		let mut cache = MailsContent::default();

		assert!(cache.syncRequest_get(&connector).is_none());
		assert_eq!(cache.mailboxes,Some(HashMap::new()));
	}

	#[test]
	fn mailCacheSeedsExplicitSelectionWithoutMailboxListing()
	{
		let mut connector = imap_connector::default();
		connector.extra = Some(imap_connector_extra {
			boxAllowList: Some(vec!["News".to_string(),"Alerts".to_string()]),
			..Default::default()
		});
		let mut cache = MailsContent::default();

		let request = cache.syncRequest_get(&connector).unwrap();
		let mailboxes = request.mailboxes.unwrap();
		assert_eq!(mailboxes.len(),2);
		assert_eq!(mailboxes[0].boxName,"Alerts");
		assert_eq!(mailboxes[1].boxName,"News");
		assert!(mailboxes.iter().all(|mailbox| mailbox.uidValidity.is_none()));
	}

	#[test]
	fn mailCacheTopologyKeepsOnlySelectedInterestingMailboxes()
	{
		let mut connector = imap_connector::default();
		connector.extra = Some(imap_connector_extra {
			boxAllowList: Some(vec!["Alerts".to_string(),"Archive".to_string()]),
			..Default::default()
		});
		let boxes = vec![
			boxName_get("Alerts",Attributs::default()),
			boxName_get("News",Attributs::default()),
			boxName_get("Archive",Attributs {is_archive: true,..Default::default()}),
		];
		let mut cache = MailsContent::default();

		cache.topology_set(&boxes,&connector);

		assert_eq!(cache.boxs.len(),3);
		assert_eq!(cache.mailboxes,Some([("Alerts".to_string(),None)].into_iter().collect()));
	}
}
