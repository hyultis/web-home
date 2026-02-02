use std::ops::DerefMut;
use std::collections::HashMap;
use gloo_timers::callback::Timeout;
use leptoaster::{expect_toaster, ToasterContext};
use leptos::callback::Callback;
use leptos::prelude::{use_context, CollectView, StyleAttribute, Write};
use leptos::prelude::{ClassAttribute, ElementChild, GetUntracked, Update};
use leptos::prelude::{AnyView, ArcRwSignal, Get, IntoAny, OnAttribute, RwSignal};
use leptos::view;
use serde::{Deserialize, Serialize};
use time::UtcDateTime;
use wasm_bindgen_futures::spawn_local;
use crate::api::modules::components::ModuleContent;
use crate::api::proxys::imap::{API_proxys_imap_getFullUnsee, API_proxys_imap_getMailContent, API_proxys_imap_getUnseeSince, API_proxys_imap_listbox, API_proxys_imap_setMailSee};
use crate::api::proxys::imap_components::{imap_connector, imap_connector_extra, Attachment, ImapMail};
use crate::front::modules::components::{distant_time_simpler, Backable, BoxFuture, Cache, Cacheable, FieldHelper, FieldHelperType, ModuleSizeContrainte, RefreshTime};
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
	pub imap: imap_connector,
}
impl Default for MailConfig
{
	fn default() -> Self
	{
		Self {
			title: "".to_string(),
			imap: imap_connector::default(),
		}
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
	pub fn draw_config(&self) -> AnyView
	{
		let toaster = expect_toaster();
		let getBoxsMailConfig = self.config.clone();
		let getBoxsMailsCache = self.mailsClientCache.clone();
		let getBoxsFn = move |_| {
			let toaster = toaster.clone();
			let getBoxsMailConfig = getBoxsMailConfig.clone();
			let getBoxsMailContent = getBoxsMailsCache.clone();
			spawn_local(async move {
				if let Some(result) = toaster_api(&toaster,API_proxys_imap_listbox(getBoxsMailConfig.get_untracked().imap.clone()).await, None).await
				{
					getBoxsMailContent.update(|mailContent|{
						mailContent.boxs = result.iter().map(|boxcontent| boxcontent.name.clone()).collect();
					});
				}
			});
		};


		let mut titleF = FieldHelper::new(&self.config,&self._update,"MODULE_TITLE_CONF",
		                                  |d| d.get().title,
		                                  |ev,inner| inner.title = ev.target().value());
		titleF.setFullSize(true);
		let hostF = FieldHelper::new(&self.config,&self._update,"MODULE_MAIL_HOST",
		                                  |d| d.get().imap.host,
		                                  |ev,inner| inner.imap.host = ev.target().value());
		let mut portF = FieldHelper::new(&self.config,&self._update,"",
		                              |d| d.get().imap.port.to_string(),
		                              |ev,inner| inner.imap.port = ev.target().value().parse::<u16>().unwrap_or(993));
		portF.setInputType(FieldHelperType::NUMBER(1,65535));
		portF.setStyle("width:90px");
		let usernameF = FieldHelper::new(&self.config,&self._update,"MODULE_MAIL_USERNAME",
		                              |d| d.get().imap.username,
		                              |ev,inner| inner.imap.username = ev.target().value());
		let mut passwordF = FieldHelper::new(&self.config,&self._update,"MODULE_MAIL_PASSWORD",
		                              |d| d.get().imap.password,
		                              |ev,inner| inner.imap.password = ev.target().value());
		passwordF.setInputType(FieldHelperType::PASSWORD);

		view!{
			<div class="module_mail_config">
				{titleF.draw()}
				{hostF.draw()}:{portF.draw()}<br/>
				{usernameF.draw()}<br/>
				{passwordF.draw()}<br/>
				<button on:click={getBoxsFn}><Translate key="MODULE_MAIL_GETBOXS"/></button>
				{
					let boxConfig = self.config.clone();
					let boxConfigCache = self._update.clone();
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

					let mailsCache = self.mailsClientCache.clone().get();
					let configBoxContent = self.config.clone().get();
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
		spawn_local(async move {
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

			let Some(mailContent) = toaster_api(&toaster, API_proxys_imap_setMailSee(imapConnector, mailId.clone().into()).await, None).await else {
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
		spawn_local(async move {
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

	async fn sync(toaster: ToasterContext, mailContent: ArcRwSignal<MailsContent>, config: ArcRwSignal<MailConfig>)
	{
		let mailsToAdd = if(mailContent.get_untracked().mailsData.is_empty())
		{
			let Some(allmails) = toaster_api(&toaster, API_proxys_imap_getFullUnsee(config.get_untracked().imap.clone()).await, None).await else {
				toastingErr(&toaster, "MODULE_MAIL_SYNCERROR".to_string()).await;
				return;
			};
			allmails
		}
		else
		{
			let Some(newmails) = toaster_api(&toaster, API_proxys_imap_getUnseeSince(config.get_untracked().imap.clone(),mailContent.get_untracked().lastUpdate).await, None).await else {
				toastingErr(&toaster, "MODULE_MAIL_SYNCERROR".to_string()).await;
				return;
			};
			newmails
		};
		mailContent.update(|mailContent| {
		for mailToAdd in mailsToAdd {
			if(mailContent.lastUpdate<mailToAdd.date as u64) {
				mailContent.lastUpdate=mailToAdd.date as u64;
			}
			mailContent.mailsData.insert(mailToAdd.uid as u64, mailToAdd);
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


impl Backable for Mail
{
	fn typeModule(&self) -> String {
		"MAIL".to_string()
	}

	fn draw(&self, editMode: RwSignal<bool>, moduleActions: ModuleActionFn, currentName: String) -> AnyView {

		let Some(dialogManager) = use_context::<DialogManager>() else {
			HWebTrace!("cannot get dialogManager in link");
			return view!{}.into_any();
		};
		let toaster = expect_toaster();

		/*let refreshMail = self.config.clone();
		let refreshMailContent = self.mailContent.clone();
		let testFn = move |_| {
			let refreshMail = refreshMail.clone();
			let refreshMailContent = refreshMailContent.clone();
			spawn_local(async move {
				if let Ok(mail) = API_proxys_imap_getFullUnsee(refreshMail.get_untracked().imap.clone()).await
				{
					refreshMailContent.update(|mailcontent|{
						for x in mail {
							mailcontent.mails.insert(x.uid as u64, x);
						}
					});
				}
			});
		};*/


		let imapConnector = self.config.clone();
		let toasterInner = toaster.clone();
		let mailsCache = self.mailsClientCache.clone();
		let viewContentFn = move |mailIdcontent:ImapMail| {
			Self::mail_view_content(imapConnector.get_untracked().imap.clone(), toasterInner, dialogManager, mailIdcontent, mailsCache);
		};

		let imapConnector = self.config.clone();
		let mailsCache = self.mailsClientCache.clone();
		let markViewFn = move |mailIdcontent:ImapMail| {
			Self::mail_mark_see(imapConnector.get_untracked().imap.clone(), toaster, mailIdcontent, mailsCache);
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


		view!{{
			if(editMode.get())
			{
				self.draw_config()
			}
			else
			{
				let config = self.config.clone();
				let mailsCache = self.mailsClientCache.clone();
				/*
					<button on:click={testFn}>MAIL</button>
					<button on:click={testSinceFn}>MAIL SINCE</button>
				 */
				view!{
					{draw_title_if_present(config.get().title.clone())}
					<div class="module_rss_upper">
						<table class="module_rss_table">{
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
									view!{
										<tr>
											<td>{distant_time_simpler(mail.date)}</td>
											<td class="mail_pointer alttext_upper" on:click={move |_| viewContentFn.clone()(mailId.clone())}>{mail.subject.clone()}{Self::utils_mailOverlay(&mailId)}</td>
											<td>{
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
		}}.into_any()
	}

	fn refresh_time(&self) -> RefreshTime {
		RefreshTime::HOURS(1)
	}

	fn refresh(&self, moduleActions: ModuleActionFn, currentName: String, toaster: ToasterContext) -> Option<BoxFuture> {
		let config = self.config.clone();
		let mailsCache = self.mailsClientCache.clone();
		let tmp = Self::sync(toaster, mailsCache, config);
		return Some(Box::pin(async move {
			tmp.await;
		}));
	}


	fn export(&self) -> ModuleContent {
		return ModuleContent{
			name: self.typeModule(),
			typeModule: self.typeModule(),
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
	fn cache_mustUpdate(&self) -> bool
	{
		return self._update.get().isNewer(&self._sended.get());
	}

	fn cache_getUpdate(&self) -> ArcRwSignal<Cache> {
		return self._update.clone();
	}

	fn cache_getSended(&self) -> ArcRwSignal<Cache> {
		return self._sended.clone();
	}
}