use leptos::prelude::{AriaAttributes, ClassAttribute, CollectView, ElementChild, GlobalAttributes};
use feed_rs::model::{Feed, Link, Text};
use feed_rs::parser;
use leptoaster::{ToasterContext};
use leptos::children::ViewFn;
use leptos::prelude::{AnyView, ArcRwSignal, Get, GetUntracked, IntoAny, RwSignal, Update};
use leptos::{component, view, IntoView};
use serde::{Deserialize, Serialize};
use crate::api::modules::components::{ModuleContent, ModuleID};
use crate::api::proxys::wget::{API_proxys_wget};
use crate::front::modules::components::{distant_time_simpler, Backable, BoxFuture, Cache, Cacheable, FieldHelper, ModuleName, ModuleSizeContrainte, RefreshTime};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::toaster_helpers::toaster_api;
use crate::front::utils::translate::{Translate, TranslateText};
use crate::front::utils::SafeExternalUrl;

#[derive(Serialize,Deserialize,Debug)]
#[derive(Clone)]
struct RssConfig
{
	pub title: String,
	pub link: String,
	#[serde(default = "maxline_default")]
	pub maxline: u8,
}

fn maxline_default() -> u8{
	10
}

impl Default for RssConfig
{
	fn default() -> Self
	{
		Self {
			title: "".to_string(),
			link: "".to_string(),
			maxline: maxline_default(),
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
#[derive(Default)]
pub struct Rss
{
	config: ArcRwSignal<RssConfig>,
	#[serde(skip_serializing,skip_deserializing)]
	rssContent: ArcRwSignal<Option<(u64,Feed)>>,
	_update: ArcRwSignal<Cache>,
	_sended: ArcRwSignal<Cache>,
}

impl Rss
{
	pub fn new() -> Self
	{
		Self {
			config: Default::default(),
			rssContent: Default::default(),
			_update: Default::default(),
			_sended: Default::default(),
		}
	}

	async fn sync(toaster: ToasterContext, rssContent: ArcRwSignal<Option<(u64,Feed)>>, config: ArcRwSignal<RssConfig>, moduleActions: ModuleActionFn)
	{
		if (!moduleActions.lifecycle_isActive())
		{
			return;
		}
		let url = config.get_untracked().link.clone();
		let oldTime = rssContent.get_untracked().map(|content| content.0);
		let apiResult = API_proxys_wget(url.to_string(),oldTime).await;
		if (!moduleActions.lifecycle_isActive())
		{
			return;
		}
		let Some((time,text)) = toaster_api(&toaster,apiResult, None).await else {return}; // TODO: return must throw error toaster
		if (!moduleActions.lifecycle_isActive())
		{
			return;
		}
		let Ok(feed) = parser::parse(text.as_bytes()) else {return};

		rssContent.update(|rssContent| {
			*rssContent = Some((time,feed));
		})
	}

	fn utils_title(title: String, entryTitle: Option<Text>) -> AnyView
	{
		if(!title.is_empty()) {
			return view!{{title}}.into_any();
		}

		if let Some(innertitle) = entryTitle
		{
			return view!{{innertitle.content}}.into_any();
		}

		return view!{<TranslateText key="MODULE_RSS_NO_TITLE"/>}.into_any();
	}

	fn utils_desc(descRaw: &Option<Text>) -> AnyView
	{
		let Some(desc) = descRaw else {
			return view!{}.into_any();
		};


		return view!{
			<span class="module_title_action module_title_info alttext_upper" tabindex="0">
				<i class="iconoir-info-circle" aria-hidden="true"></i>
				<span class="visually_hidden"><TranslateText key="MODULE_RSS_DESCRIPTION"/></span>
				<span class="alttext" role="tooltip">{desc.content.clone()}</span>
			</span>
		}.into_any();
	}

	fn utils_link(entryTitle: Vec<Link>) -> AnyView
	{
		let Some(link) = entryTitle.first() else {
			return view!{}.into_any();
		};
		let Some(url) = SafeExternalUrl::parse(&link.href) else {
			return view!{}.into_any();
		};

		return view!{
			<a class="module_title_action" href={url.into_string()} rel="noopener noreferrer nofollow" target="_blank">
				<i class="iconoir-link" aria-hidden="true"></i>
				<span class="visually_hidden"><TranslateText key="MODULE_RSS_OPEN_FEED_ACTION"/></span>
			</a>
		}.into_any();
	}
}

impl Cacheable for Rss
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

impl ModuleName for Rss
{
	const MODULE_NAME: &'static str = "RSS";
}

impl Backable for Rss
{
	fn module_name(&self) -> String {
		Rss::MODULE_NAME.to_string()
	}

	fn draw(&self, editMode: RwSignal<bool>, _moduleActions: ModuleActionFn, _: ModuleID) -> ViewFn
	{
		let configInner = self.config.clone();
		let contentInner = self.rssContent.clone();
		let updateInner = self._update.clone();
		ViewFn::from(move || {
			view! {
				<RssDraw config=configInner.clone() content=contentInner.clone() update=updateInner.clone() editMode=editMode/>
			}.into_any()
		})
	}

	fn refresh_time(&self) -> RefreshTime {
		return RefreshTime::MINUTES(10);
	}

	fn refresh(&self,moduleActions: ModuleActionFn, _moduleId: ModuleID, toaster: ToasterContext) -> Option<BoxFuture> {
		let config = self.config.clone();
		let rssContent = self.rssContent.clone();
		let tmp = Self::sync(toaster,rssContent,config,moduleActions);
		return Some(Box::pin(async move {
			tmp.await;
		}));
	}

	fn export(&self) -> ModuleContent
	{
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
		let Ok(content): Result<RssConfig,_> = serde_json::from_str(&import.content.clone()) else {return};

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
		let Ok(content): Result<RssConfig,_> = serde_json::from_str(&from.content) else {return None};
		Some(Self {
			config: ArcRwSignal::new(content),
			rssContent: Default::default(),
			_update: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
			_sended: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
		})
	}

	fn size(&self) -> ModuleSizeContrainte {
		ModuleSizeContrainte::default()
	}
}

#[component]
fn RssDraw(config: ArcRwSignal<RssConfig>,
           content: ArcRwSignal<Option<(u64,Feed)>>,
           update: ArcRwSignal<Cache>,
           editMode: RwSignal<bool>) -> impl IntoView
{
	view! {{move || {
		let editMode = editMode.get();
			if editMode
			{
				let mut titleF = FieldHelper::new(&config,&update,"MODULE_TITLE_CONF",
					|d| d.get().title,
					|ev,inner| inner.title = ev.target().value());
				titleF.setFullSize();
				let mut linkF = FieldHelper::new(&config,&update,"MODULE_RSS_LINK",
					|d| d.get().link,
					|ev,inner| inner.link = ev.target().value());
				linkF.setFullSize();
				let maxLineF = FieldHelper::new(&config,&update,"MODULE_RSS_MAXLINE",
					|d| d.get().maxline.to_string(),
					|ev,inner| inner.maxline = ev.target().value().parse::<u8>().unwrap_or(10));

				view!{
					<div class="module_config module_rss_config">
						{titleF.draw()}
						{linkF.draw()}
						{maxLineF.draw()}
						<div class="module_config_preview">
							<span class="module_config_section_title"><Translate key="MODULE_RSS_DEMO"/></span>
							<table class="module_rss_table" aria-hidden="true"><tbody><tr>
								<td class="module_rss_age">{"0d"}</td><td>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Suspendisse nulla nisi, faucibus ut eros non, porttitor posuere ante.</td>
							</tr></tbody></table>
						</div>
					</div>
				}.into_any()
			}
			else
			{
				view!{{
					let config = config.clone();
					content.get().map(|(_,mut rssContent)|{

					view!{
						<>
						<div class="module_titlebar">
							<h2 class="module_title">{Rss::utils_title(config.get().title,rssContent.title)}</h2>
							<div class="module_title_actions">{Rss::utils_desc(&rssContent.description)}{Rss::utils_link(rssContent.links)}</div>
						</div>
						<div class="module_rss_upper">
						<table class="module_rss_table"><tbody>
						{   rssContent.entries.sort_by(|a,b| a.published.cmp(&b.published).reverse());
							rssContent.entries.iter().enumerate()
							.filter(|(num,_)| *num <= config.get().maxline as usize)
							.map(|(_,entry)|{
								if let Some(link) = &entry.links.first() && let Some(title) = &entry.title
								{
									let title = title.content.clone();
									let titleView = if let Some(url) = SafeExternalUrl::parse(&link.href)
									{
										view!{<a href={url.into_string()} rel="noopener noreferrer nofollow" target="_blank">{title}</a>}.into_any()
									}
									else
									{
										view!{<span>{title}</span>}.into_any()
									};
									view!{
										<tr class="module_rss_row">
											<td class="module_rss_age">{distant_time_simpler(entry.published.clone().unwrap_or_default().timestamp())}</td>
											<td>{titleView}</td>
										</tr>
									}.into_any()
								}
								else {view!{}.into_any()}
							}).collect_view()
						}
						</tbody></table>
						</div>
						</>
					}.into_any()
				})
				}
				}.into_any()
			}
		}}}.into_any()
}
