use leptos::prelude::{CollectView, OnTargetAttribute, StyleAttribute};
use leptos::prelude::{ClassAttribute, ElementChild, GetUntracked, PropAttribute, Update};
use leptos::prelude::{AnyView, ArcRwSignal, Get, IntoAny, OnAttribute, RwSignal};
use leptos::view;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;
use crate::api::modules::components::ModuleContent;
use crate::api::proxys::imap::{imap_connector, imap_connector_extra, API_proxys_imap_listbox};
use crate::front::modules::components::{Backable, Cache, Cacheable, ModuleSizeContrainte};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::translate::Translate;

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
	mails: Vec<String>,
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
		let getBoxsMailConfig = self.config.clone();
		let getBoxsMailContent = self.mailContent.clone();
		let getBoxsFn = move |_| {
			let getBoxsMailConfig = getBoxsMailConfig.clone();
			let getBoxsMailContent = getBoxsMailContent.clone();
			spawn_local(async move {
				match API_proxys_imap_listbox(getBoxsMailConfig.get_untracked().imap.clone()).await {
					Ok(result) => {
						getBoxsMailContent.update(|mailContent|{
							mailContent.boxs = result.iter().map(|boxcontent| boxcontent.name.clone()).collect();
						});
					}
					Err(_) => {}
				};
			});
		};

		let configHost = self.config.clone();
		let configPort = self.config.clone();
		let configUsername = self.config.clone();
		let configPassword = self.config.clone();
		let cacheHost = self._update.clone();
		let cachePort = self._update.clone();
		let cacheUsername = self._update.clone();
		let cachePassword = self._update.clone();

		view!{
				<div class="module_mail_config">
					<label for="mail_host"><Translate key="MODULE_MAIL_HOST"/></label><br/>
					<input type="text" name="weather_latitude" prop:value={configHost.get().imap.host} on:input:target=move |ev| {
						configHost.update(|inner|inner.imap.host = ev.target().value());
						cacheHost.update(|cache| cache.update());
					} />:
					<input type="number" min="1" max="65535" prop:value={configPort.get().imap.port}  on:input:target=move |ev| {
						configPort.update(|inner|inner.imap.port = ev.target().value().parse::<u16>().unwrap_or(993));
						cachePort.update(|cache| cache.update());
					}/><br/>
					<label for="mail_username"><Translate key="MODULE_MAIL_USERNAME"/></label><input type="text" name="mail_username" prop:value={configUsername.get().imap.username}  on:input:target=move |ev| {
						configUsername.update(|inner|inner.imap.username = ev.target().value());
						cacheUsername.update(|cache| cache.update());
					}/><br/>
					<label for="mail_password"><Translate key="MODULE_MAIL_PASSWORD"/></label><input type="password" name="mail_password" prop:value={configPassword.get().imap.password}  on:input:target=move |ev| {
						configPassword.update(|inner|inner.imap.password = ev.target().value());
						cachePassword.update(|cache| cache.update());
					}/><br/>
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
}


impl Backable for Mail
{
	fn typeModule(&self) -> String {
		"MAIL".to_string()
	}

	fn draw(&self, editMode: RwSignal<bool>, moduleActions: ModuleActionFn, currentName: String) -> AnyView {

		let refreshMail = self.config.clone();
		let testFn = move |_| {
			let refreshMail = refreshMail.clone();
			spawn_local(async move {
				API_proxys_imap_listbox(refreshMail.get_untracked().imap.clone()).await;
			});
		};

		view!{{
			if(editMode.get())
			{
				self.draw_config()
			}
			else
			{
				view!{<div class="module_mail">{" "
					/*self.weatherContent.get().map(|haveContent| {
						let units = haveContent.unit.clone();
						haveContent.days.iter().map(|days| {
							let date = UtcDateTime::from_unix_timestamp(days.timestampDay as i64).unwrap_or(UtcDateTime::now());
							view!{
								<div class="day">
									{format!("{:0>2}",date.day())}/{format!("{:0>2}",date.month() as u8)}<br/>
									<img src={format!("weather/{}.png",days.codeIntoImg())} alt={days.codeIntoImg()} /><br/>
									<Translate key={days.codeIntoTranslate()}/><br/>
									<span style={Self::celsiusToColor(days.temp_min)}>{days.temp_min}{units.clone().temp}</span>{" - "}<span style={Self::celsiusToColor(days.temp_max)}>{days.temp_max}{units.clone().temp}</span><br/>
									<i class="iconoir-wind"/>{" "}{days.wind}{units.clone().wind}<br/>
									<i class="iconoir-heavy-rain"/>{" "}{days.precipitation}{units.clone().precipitation}
								</div>
							}
						}).collect_view()
					})*/
				
				}
				<button on:click={testFn}>MAIL</button>
				</div>}.into_any()
			}
		}}.into_any()
	}

	fn refresh_time(&self) -> u64 {
		1000*60*60
	}

	fn refresh(&self, moduleActions: ModuleActionFn, currentName: String) {

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