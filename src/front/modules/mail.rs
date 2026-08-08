use std::ops::DerefMut;
use std::collections::HashMap;
use gloo_timers::callback::Timeout;
use leptoaster::{expect_toaster, ToasterContext};
use leptos::callback::Callback;
use leptos::children::ViewFn;
use leptos::prelude::{use_context, CollectView, StyleAttribute, Write};
use leptos::prelude::{ClassAttribute, ElementChild, GetUntracked, Update};
use leptos::prelude::{AnyView, ArcRwSignal, Get, IntoAny, OnAttribute, RwSignal};
use leptos::reactive::spawn_local_scoped;
use leptos::{component, view, IntoView};
use serde::{Deserialize, Serialize};
use time::UtcDateTime;
use crate::api::modules::components::{ModuleContent, ModuleID};
use crate::api::proxys::imap::{API_proxys_imap_getFullUnsee, API_proxys_imap_getMailContent, API_proxys_imap_getUnseeSince, API_proxys_imap_listbox, API_proxys_imap_setMailSee};
use crate::api::proxys::imap_components::{imap_connector, imap_connector_extra, Attachment, ImapMail};
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
	pub imap: imap_connector,
}
impl Default for MailConfig
{
	fn default() -> Self
	{
		Self {
			title: "".to_string(),
			mailAsTag: "".to_string(),
			imap: imap_connector::default(),
		}
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
}

#[derive(Clone, Debug, Default)]
struct MailsContent
{
	lastUpdate: u64,
	mailsData: HashMap<u64, ImapMail>,
	mailsContent: HashMap<u64, String>,
	boxs: Vec<String>,
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
	fn draw_config(getBoxsMailConfig: ArcRwSignal<MailConfig>, getBoxsMailsCache: ArcRwSignal<MailsContent>, update: ArcRwSignal<Cache>) -> AnyView
	{
		let getBoxsMailConfigInner = getBoxsMailConfig.clone();
		let getBoxsMailsCacheInner = getBoxsMailsCache.clone();
		let toaster = expect_toaster();
		let getBoxsFn = move |_| {
			let toaster = toaster.clone();
			let getBoxsMailConfig = getBoxsMailConfigInner.clone();
			let getBoxsMailContent = getBoxsMailsCacheInner.clone();
			spawn_local_scoped(async move {
				if let Some(result) = toaster_api(&toaster,API_proxys_imap_listbox(getBoxsMailConfig.get_untracked().imap.clone()).await, None).await
				{
					getBoxsMailContent.update(|mailContent|{
						mailContent.boxs = result.iter().map(|boxcontent| boxcontent.name.clone()).collect();
					});
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

		view!{
			<div class="module_mail_config">
				{titleF.draw()}
				{mailAsTagF.draw()}
				{hostF.draw()}:{portF.draw()}<br/>
				{usernameF.draw()}<br/>
				{passwordF.draw()}<br/>
				<button on:click={getBoxsFn}><Translate key="MODULE_MAIL_GETBOXS"/></button>
				{
					let boxConfig = getBoxsMailConfig.clone();
					let boxConfigCache = update.clone();
					let switchBoxFn = move |boxName:String,isDisabled:bool| {
						boxConfig.update(|mailContent|{
							if(mailContent.imap.extra.is_none()) {mailContent.imap.extra = Some(imap_connector_extra::default())}

							if(isDisabled)
							{
								mailContent.imap.extra.as_mut().unwrap().boxBlackList.retain(|boxcontent| boxcontent != &boxName);
							}
							else
							{
								mailContent.imap.extra.as_mut().unwrap().boxBlackList.push(boxName.clone());
							}

							boxConfigCache.update(|cache|{
								cache.update();
							});
						});
					};

					let mailsCache = getBoxsMailsCache.clone().get();
					let configBoxContent = getBoxsMailConfig.clone().get();
					if(!mailsCache.boxs.is_empty())
					{
						view!{
							<hr/>
							<Translate key="MODULE_MAIL_BOXS_LIST"/><br/>
							{mailsCache.boxs.iter().map(|boxcontent| {
								let switchBoxFn = switchBoxFn.clone();
								let mut isDisabled = false;
								if let Some(s) = &configBoxContent.imap.extra
								{
									if(s.boxBlackList.contains(boxcontent)){
										isDisabled = true;
									}
								}
								let boxcontent = boxcontent.clone();
								view!{<span class={if isDisabled {"disabled boxmail"} else {"boxmail"}} on:click={move |_|switchBoxFn(boxcontent.clone(),isDisabled)}>{boxcontent.clone()}</span>}
							}).collect_view()}
						}.into_any()
					}
					else {view!{}.into_any()}
				}
			</div>
			}.into_any()
	}

	fn mail_mark_see(imapConnector: imap_connector, toaster: ToasterContext, mailId: ImapMail, mailsContent: ArcRwSignal<MailsContent>)
	{
		spawn_local_scoped(async move {
			let mailUid = mailId.uid as u64;
			// we remove the old data sooner to improve reactivity and re-add them later if something gone wrong
			let oldMailsData;
			let oldMailsContent;
			{
				let Some(mut binding) = mailsContent.try_write()
				else
				{
					return;
				};
				let mailsDatas: &mut MailsContent = binding.deref_mut();
				oldMailsData = mailsDatas.mailsData.remove(&mailUid);
				oldMailsContent = mailsDatas.mailsContent.remove(&mailUid);
			}

			if toaster_api(&toaster, API_proxys_imap_setMailSee(imapConnector, mailId.clone().into()).await, None).await.is_none()
			{
				mailsContent.update(|mailContent|{
					if let Some(oldData) = oldMailsData {
						mailContent.mailsData.insert(mailUid, oldData);
					}
					if let Some(oldDataContent) = oldMailsContent {
						mailContent.mailsContent.insert(mailUid, oldDataContent);
					}
				});
				return
			};


		});

	}

	fn mail_view_content(imapConnector: imap_connector, toaster: ToasterContext, dialogManager: DialogManager, mailIdContent: ImapMail, mailsCache: ArcRwSignal<MailsContent>)
	{
		spawn_local_scoped(async move {
			let Some(mailContent) = toaster_api(&toaster, API_proxys_imap_getMailContent(imapConnector.clone(), mailIdContent.clone().into()).await, None).await else {return};

			let toasterBody = toaster.clone();
			let mailIdContentBody = mailIdContent.clone();

			let dialogContent = DialogData::new()
				.setTitle(mailIdContent.subject.clone().map(|subject|format!("€{}", subject)).unwrap_or("MODULE_MAIL_NO_SUBJECT".to_string()))
				.setBody(move || {
					let mailId = mailIdContentBody.clone();
					let mailContent = mailContent.clone();

					let downloadAttachement = move |attachement: Attachment, toaster: ToasterContext| {
						download_attachment(attachement,toaster);
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
								let views = mailContent.attachement.iter().map(|att| {
									let attInner = att.clone();
									return match &att.filename {
										None => {view!{{" "}<span class="attachement" on:click={
												let toasterInner = toasterInner.clone();
												move |_| downloadAttachement(attInner.clone(),toasterInner.clone())
											}><i class="iconoir-doc-magnifying-glass"/>{" "}<Translate key="MODULE_MAIL_NO_SUBJECT"/></span>}}.into_any(),
										Some(filename) => {
											view!{{" "}<span class="attachement"  on:click={
												let toasterInner = toasterInner.clone();
												move |_| downloadAttachement(attInner.clone(),toasterInner.clone())
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
							<div style="flex-grow: 1; border: none; margin: 0; padding: 0;margin-top: 0.5em">
						        <iframe srcdoc={mailContent.content.unwrap_or_default(&mailContent.parts)} sandbox="allow-popups allow-popups-to-escape-sandbox" referrerpolicy="no-referrer" style="width:100%; height:100%; background:white; border: none; margin: 0; padding: 0;"></iframe>
							</div>
						</div>
					}.into_any()
				})
				.setButtonValidateTitle(Some("MODULE_MAIL_MAILCONTENTSEEN"))
				.setOnValidate(Callback::new(move |_| {
					Self::mail_mark_see(imapConnector.clone(), toaster.clone(), mailIdContent.clone(), mailsCache.clone());
					return true;
				}))
				.setIsLarger(true);

			dialogManager.open(dialogContent);
		});

	}

	async fn sync(toaster: ToasterContext, mailContentRaw: ArcRwSignal<MailsContent>, config: ArcRwSignal<MailConfig>)
	{
		let mailContent = mailContentRaw.get_untracked();
		let config = config.get_untracked();
		let (mailsToAdd,mailToUpdate) = if(mailContent.mailsData.is_empty())
		{
			let Some(allmails) = toaster_api(&toaster, API_proxys_imap_getFullUnsee(config.imap.clone()).await, None).await else {
				toastingErr(&toaster, "MODULE_MAIL_SYNCERROR".to_string()).await;
				return;
			};
			(allmails,HashMap::new())
		}
		else
		{
			let Some((newmails,mailToUpdate)) = toaster_api(&toaster, API_proxys_imap_getUnseeSince(config.imap.clone(),
			                                                                         mailContent.lastUpdate,
			                                                                         mailContent.mailsData.keys()
				                                                                         .map(|e| *e as u32)
				                                                                         .collect::<Vec<u32>>()).await,
			                                                                        None).await
			else {
				toastingErr(&toaster, "MODULE_MAIL_SYNCERROR".to_string()).await;
				return;
			};
			(newmails,mailToUpdate)
		};
		mailContentRaw.update(move |mailContent| {
		for mailToAdd in mailsToAdd {
			if(mailContent.lastUpdate<mailToAdd.date as u64) {
				mailContent.lastUpdate=mailToAdd.date as u64;
			}
			mailContent.mailsData.insert(mailToAdd.uid as u64, mailToAdd);
		}

		for (uid,content) in &mailToUpdate {
			if(content.flags.contains(&"SEEN".to_string())) {
				mailContent.mailsData.remove(&(*uid as u64));
				mailContent.mailsContent.remove(&(*uid as u64));
			}
			//let Some(foundMailToUpdate) = mailContent.mailsData.get_mut(&(*uid as u64)) else {continue};
		}
	})
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

	fn draw(&self, editMode: RwSignal<bool>, moduleActions: ModuleActionFn, _: ModuleID) -> ViewFn {
		let configInner = self.config.clone();
		let clientCacheInner = self.mailsClientCache.clone();
		let updateInner = self._update.clone();
		ViewFn::from(move || {
			view! {
				<MailDraw config=configInner.clone() mailsClientCache=clientCacheInner.clone() update=updateInner.clone() editMode=editMode/>
			}.into_any()
		})

	}

	fn refresh_time(&self) -> RefreshTime {
		RefreshTime::MINUTES(30)
	}

	fn refresh(&self, moduleActions: ModuleActionFn, moduleId: ModuleID, toaster: ToasterContext) -> Option<BoxFuture> {
		let config = self.config.clone();
		let mailsCache = self.mailsClientCache.clone();
		let tmp = Self::sync(toaster, mailsCache, config);
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
            editMode: RwSignal<bool>) -> impl IntoView
{
	let Some(dialogManager) = use_context::<DialogManager>() else {
		HWebTrace!("cannot get dialogManager in link");
		return view!{}.into_any();
	};
	let toaster = expect_toaster();

	let imapConnector = config.clone();
	let toasterInner = toaster.clone();
	let mailsCache = mailsClientCache.clone();
	let viewContentFn = move |mailIdcontent:ImapMail| {
		Mail::mail_view_content(imapConnector.get_untracked().imap.clone(), toasterInner, dialogManager, mailIdcontent, mailsCache);
	};

	let imapConnector = config.clone();
	let mailsCache = mailsClientCache.clone();
	let markViewFn = move |mailIdcontent:ImapMail| {
		Mail::mail_mark_see(imapConnector.get_untracked().imap.clone(), toaster, mailIdcontent, mailsCache);
	};

	/*let refreshMail = self.config.clone();
	let actualContentRefresh = self.mailContent.clone();
	let testSinceFn = move |_| {
		let refreshMail = refreshMail.clone();
		let actualContentRefresh = actualContentRefresh.clone();
		spawn_local(async move {
			let _ = API_proxys_imap_getUnseeSince(refreshMail.get_untracked().imap.clone(),actualContentRefresh.get_untracked().lastUpdate).await;
		});
	};*/


	view!{{move || {
			let editMode = editMode.get();
			if(editMode)
			{
				Mail::draw_config(config.clone(), mailsClientCache.clone(), update.clone())
			}
			else
			{
				let config = config.clone();
				let mailsCache = mailsClientCache.clone();
				let mailConfig = config.get();
				let mailTagIsActive = mailConfig.mail_tag_is_active();
				/*
					<button on:click={testFn}>MAIL</button>
					<button on:click={testSinceFn}>MAIL SINCE</button>
				 */
				view!{
					{draw_title_if_present(mailConfig.title.clone())}
					<div class="module_rss_upper">
						<table class="module_rss_table module_mail_table">{
							let markVueCacheInner = mailsCache.clone();
							let mails = mailsCache.get().mailsData.clone();
							let mut mailsContent = mails.values().cloned().collect::<Vec<_>>();
							mailsContent.sort_by(|a,b| a.date.cmp(&b.date).reverse());
							mailsContent.iter().enumerate()
								//.filter(|(num,_)| *num <= 10)
								.map(|(_,mail)|{
									let id = mail.uid;
									let mailId = mail.clone();
									let mailIdMark = mail.clone();
									let viewContentFn = viewContentFn.clone();
									let markViewFn = markViewFn.clone();
									let markVueCacheInner = markVueCacheInner.clone();
									let mailTag = if(mailTagIsActive) {mailConfig.mail_tag(mail)} else {None};
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
											<td class="module_mail_subject mail_pointer alttext_upper" on:click={move |_| viewContentFn.clone()(mailId.clone())}>{mail.subject.clone()}{Mail::utils_mailOverlay(&mailId)}</td>
											<td class="module_mail_status">{
												if(mail.confirmVue)
												{
													view!{<i class="iconoir-mail-out-solid" on:click={move |_| markViewFn.clone()(mailIdMark.clone())}/>}.into_any()
												}
												else
												{
													view!{<i class="iconoir-mail-open" on:click={move |_| {
														let markVueCacheInnerInner = markVueCacheInner.clone();
														markVueCacheInner.update(|mailCache|{
															if let Some(thismail) = mailCache.mailsData.get_mut(&(id as u64))
															{
																thismail.confirmVue = true;
																Timeout::new(5000, move || {
																        markVueCacheInnerInner.update(|mailCache|{
																			if let Some(thismail) = mailCache.mailsData.get_mut(&(id as u64))
																			{
																				thismail.confirmVue = false;
																			}
																		});
																    }
																).forget();
															}
														});
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
	use super::{MailConfig, MailTag};
	use crate::api::proxys::imap_components::ImapMail;

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
}
