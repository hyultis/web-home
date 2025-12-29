use leptos::prelude::{ClassAttribute, CollectView, ElementChild};
use feed_rs::model::{Feed, Link, Text};
use feed_rs::parser;
use leptoaster::{ToasterContext};
use leptos::prelude::{AnyView, ArcRwSignal, Get, GetUntracked, IntoAny, RwSignal, Update};
use leptos::view;
use serde::{Deserialize, Serialize};
use crate::api::modules::components::ModuleContent;
use crate::api::proxys::wget::{API_proxys_wget};
use crate::front::modules::components::{distant_time_simpler, Backable, BoxFuture, Cache, Cacheable, FieldHelper, ModuleSizeContrainte, RefreshTime};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::toaster_helpers::toaster_api;
use crate::front::utils::translate::Translate;

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

	async fn sync(toaster: ToasterContext, rssContent: ArcRwSignal<Option<(u64,Feed)>>, config: ArcRwSignal<RssConfig>)
	{
		let url = config.get_untracked().link.clone();
		let oldTime = rssContent.get_untracked().map(|content| content.0);
		let Some((time,text)) = toaster_api(&toaster,API_proxys_wget(url.to_string(),oldTime).await, None).await else {return}; // TODO: return must throw error toaster
		let Ok(feed) = parser::parse(text.as_bytes()) else {return};

		rssContent.update(|rssContent| {
			*rssContent = Some((time,feed));
		})
	}

	fn utils_title(title: String, entryTitle: Option<Text>) -> String
	{
		if(!title.is_empty()) {
			return title;
		}

		if let Some(innertitle) = entryTitle
		{
			return innertitle.content;
		}

		return "MODULE_RSS_NO_TITLE".to_string();
	}

	fn utils_desc(descRaw: &Option<Text>) -> AnyView
	{
		let Some(desc) = descRaw else {
			return view!{}.into_any();
		};


		return view!{
			<i class="iconoir-info-circle alttext_upper"><div class="alttext">{desc.content.clone()}</div></i>
		}.into_any();
	}

	fn utils_link(entryTitle: Vec<Link>) -> AnyView
	{
		let Some(link) = entryTitle.first() else {
			return view!{}.into_any();
		};

		return view!{
			<a href={link.href.clone()} rel="noopener noreferrer nofollow" target="_blank"><i class="iconoir-link"></i></a>
		}.into_any();
	}
}

impl Cacheable for Rss
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

impl Backable for Rss
{
	fn typeModule(&self) -> String {
		"RSS".to_string()
	}

	fn draw(&self, editMode: RwSignal<bool>, moduleActions: ModuleActionFn, currentName: String) -> AnyView
	{
		view! {{
			if(editMode.get())
			{
				let mut titleF = FieldHelper::new(&self.config,&self._update,"MODULE_TITLE_CONF",
					|d| d.get().title,
					|ev,inner| inner.title = ev.target().value());
				titleF.setFullSize(true);
				let mut linkF = FieldHelper::new(&self.config,&self._update,"MODULE_RSS_LINK",
					|d| d.get().link,
					|ev,inner| inner.link = ev.target().value());
				linkF.setFullSize(true);
				let maxLineF = FieldHelper::new(&self.config,&self._update,"MODULE_RSS_MAXLINE",
					|d| d.get().maxline.to_string(),
					|ev,inner| inner.maxline = ev.target().value().parse::<u8>().unwrap_or(10));
				
				view!{
					{titleF.draw()}
					{linkF.draw()}
					{maxLineF.draw()}
					<Translate key="MODULE_RSS_DEMO"/><br/>
					<table class="module_rss_table"><tr>
						<td>{"0d"}</td><td>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Suspendisse nulla nisi, faucibus ut eros non, porttitor posuere ante. Nunc faucibus sagittis sodales. Ut consectetur erat urna, id posuere nibh accumsan at. Praesent tincidunt eget lorem in elementum. Suspendisse varius neque sed magna efficitur, vitae varius arcu volutpat.</td>
					</tr></table>
				}.into_any()
			}
			else
			{
				view!{{
					let config = self.config.clone();
					self.rssContent.get().map(|(_,mut rssContent)|{

					view!{
						<>
						<h2>{Rss::utils_title(config.get().title,rssContent.title)}{Self::utils_desc(&rssContent.description)}{Self::utils_link(rssContent.links)}</h2>
						<div class="module_rss_upper">
						<table class="module_rss_table">
						{   rssContent.entries.sort_by(|a,b| a.published.cmp(&b.published).reverse());
							rssContent.entries.iter().enumerate()
							.filter(|(num,_)| *num <= config.get().maxline as usize)
							.map(|(_,entry)|{
								if let Some(link) = &entry.links.first() && let Some(title) = &entry.title
								{
									view!{
										<tr>
											<td>{distant_time_simpler(entry.published.clone().unwrap_or_default().timestamp())}</td>
											<td><a href={link.href.clone()} rel="noopener noreferrer nofollow" target="_blank">{title.content.clone()}</a></td>
										</tr>
									}.into_any()
								}
								else {view!{}.into_any()}
							}).collect_view()
						}
						</table>
						</div>
						</>
					}.into_any()
				})
				}
				}.into_any()
			}
		}}.into_any()
	}

	fn refresh_time(&self) -> RefreshTime {
		return RefreshTime::MINUTES(10);
	}

	fn refresh(&self,moduleActions: ModuleActionFn,currentName:String, toaster: ToasterContext) -> Option<BoxFuture> {
		let config = self.config.clone();
		let rssContent = self.rssContent.clone();
		let tmp = Self::sync(toaster,rssContent,config);
		return Some(Box::pin(async move {
			tmp.await;
		}));
	}

	fn export(&self) -> ModuleContent
	{
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