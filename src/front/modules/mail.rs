use std::collections::HashMap;
use leptoaster::{expect_toaster, ToasterContext};
use leptos::callback::Callback;
use leptos::prelude::{use_context, CollectView, StyleAttribute};
use leptos::prelude::{ClassAttribute, ElementChild, GetUntracked, Update};
use leptos::prelude::{AnyView, ArcRwSignal, Get, IntoAny, OnAttribute, RwSignal};
use leptos::view;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;
use crate::api::modules::components::ModuleContent;
use crate::api::proxys::imap::{API_proxys_imap_getFullUnsee, API_proxys_imap_getMailContent, API_proxys_imap_getUnseeSince, API_proxys_imap_listbox};
use crate::api::proxys::imap_components::{imap_connector, imap_connector_extra, ImapMail, ImapMailIdentifier};
use crate::front::modules::components::{distant_time_simpler, Backable, BoxFuture, Cache, Cacheable, FieldHelper, FieldHelperType, ModuleSizeContrainte, RefreshTime};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::dialog::DialogManager;
use crate::front::utils::toaster_helpers::toaster_api;
use crate::front::utils::translate::Translate;
use crate::HWebTrace;

#[derive(Serialize,Deserialize,Debug)]
#[derive(Clone)]
struct MailConfig
{
	pub imap: imap_connector,
}
impl Default for MailConfig
{
	fn default() -> Self
	{
		Self {
			imap: imap_connector::default(),
		}
	}
}

#[derive(Clone, Debug, Default)]
struct MailsContent
{
	lastUpdate: u64,
	mails: HashMap<u64, ImapMail>,
	mailsContent: HashMap<u64, String>,
	boxs: Vec<String>,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Mail
{
	config: ArcRwSignal<MailConfig>,
	#[serde(skip_serializing,skip_deserializing)]
	mailContent: ArcRwSignal<MailsContent>,
	_update: ArcRwSignal<Cache>,
	_sended: ArcRwSignal<Cache>,
}

impl Mail
{
	pub fn draw_config(&self) -> AnyView
	{
		let toaster = expect_toaster();
		let getBoxsMailConfig = self.config.clone();
		let getBoxsMailContent = self.mailContent.clone();
		let getBoxsFn = move |_| {
			let toaster = toaster.clone();
			let getBoxsMailConfig = getBoxsMailConfig.clone();
			let getBoxsMailContent = getBoxsMailContent.clone();
			spawn_local(async move {
				if let Some(result) = toaster_api(&toaster,API_proxys_imap_listbox(getBoxsMailConfig.get_untracked().imap.clone()).await, None).await
				{
					getBoxsMailContent.update(|mailContent|{
						mailContent.boxs = result.iter().map(|boxcontent| boxcontent.name.clone()).collect();
					});
				}
			});
		};


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

					let mailContent = self.mailContent.clone().get();
					let configBoxContent = self.config.clone().get();
					if(!mailContent.boxs.is_empty())
					{
						view!{
							<hr/>
							<Translate key="MODULE_MAIL_BOXS_LIST"/><br/>
							{mailContent.boxs.iter().map(|boxcontent| {
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

	pub fn mail_view_content(imapConnector: imap_connector, toaster: ToasterContext, dialog: DialogManager, mailId: ImapMail)
	{
		spawn_local(async move {
			let Some(mailContent) = toaster_api(&toaster,API_proxys_imap_getMailContent(imapConnector, mailId.clone().into()).await, None).await else {return};

			HWebTrace!("mail content : {:?}",mailContent.content);

			dialog.openLarger(mailId.subject.clone().map(|subject|format!("€{}",subject)).unwrap_or("MODULE_MAIL_NO_SUBJECT".to_string()), move || {
				let mailId = mailId.clone();
				let mailContent = mailContent.clone();

				view!{
				<div class="module_mail_content_parent">
					<h2>{mailId.from}</h2>
					<h2>{mailId.to}</h2>
					<div style="flex-grow: 1; border: none; margin: 0; padding: 0;">
				        <iframe srcdoc={mailContent.content.unwrap_or_default()} sandbox style="width:100%; height:100%; background:white; border: none; margin: 0; padding: 0;"></iframe>
					</div>
				</div>
			}.into_any()
			}, Some(Callback::new(move |_| {
				return true;
			})), Some(Callback::new(move |_| {})));
		});

	}
}


impl Backable for Mail
{
	fn typeModule(&self) -> String {
		"MAIL".to_string()
	}

	fn draw(&self, editMode: RwSignal<bool>, moduleActions: ModuleActionFn, currentName: String) -> AnyView {


		let dialog = use_context::<DialogManager>().expect("DialogManager missing");
		let toaster = expect_toaster();
		let refreshMail = self.config.clone();
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
		};


		let imapConnector = self.config.get_untracked().imap.clone();
		let viewContentFn = move |id:ImapMail| {
			Self::mail_view_content(imapConnector, toaster, dialog, id);
		};

		let imapConnector = self.config.get_untracked().imap.clone();
		let markViewFn = move |id:ImapMail| {
		};

		let refreshMail = self.config.clone();
		let testSinceFn = move |_| {
			let refreshMail = refreshMail.clone();
			spawn_local(async move {
				let _ = API_proxys_imap_getUnseeSince(refreshMail.get_untracked().imap.clone(),1765303313).await;
			});
		};


		view!{{
			if(editMode.get())
			{
				self.draw_config()
			}
			else
			{
				let config = self.config.clone();
				let mailContent = self.mailContent.clone();
				view!{<div class="module_mail">
					<button on:click={testFn}>MAIL</button>
					<button on:click={testSinceFn}>MAIL SINCE</button>
					<div class="module_rss_upper">
						<table class="module_rss_table">{
							let mails = mailContent.get().mails.clone();
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
									view!{
										<tr>
											<td>{distant_time_simpler(mail.date)}</td>
											<td on:click={move |_| viewContentFn.clone()(mailId.clone())}>{mail.subject.clone()}</td>
											<td><i class="iconoir-mail-open" on:click={move |_| markViewFn.clone()(mailIdMark.clone())}></i></td>
										</tr>
									}
								}).collect_view()
							}
						</table>
					</div>
				</div>}.into_any()
			}
		}}.into_any()
	}

	fn refresh_time(&self) -> RefreshTime {
		RefreshTime::HOURS(1)
	}

	fn refresh(&self, moduleActions: ModuleActionFn, currentName: String, toaster: ToasterContext) -> Option<BoxFuture> {
		None
	}


	fn export(&self) -> ModuleContent {
		return ModuleContent{
			name: self.typeModule(),
			typeModule: self.typeModule(),
			timestamp: self._update.get_untracked().get(),
			content: serde_json::to_string(&self.config.get_untracked()).unwrap_or_default(),
			pos: [0,0],
			size: [0,0],
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
			mailContent: Default::default(),
			_update: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
			_sended: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
		})
	}

	fn size(&self) -> ModuleSizeContrainte {
		ModuleSizeContrainte::default()
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