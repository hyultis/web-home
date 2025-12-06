use leptos::prelude::OnTargetAttribute;
use leptos::prelude::{ClassAttribute, CollectView, ElementChild, PropAttribute};
use feed_rs::model::{Feed, Link, Text};
use feed_rs::parser;
use leptos::prelude::{AnyView, ArcRwSignal, Get, GetUntracked, IntoAny, RwSignal, Update};
use leptos::view;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::{spawn_local};
use crate::api::modules::components::ModuleContent;
use crate::api::proxys::wget::{proxys_return, API_proxys_wget};
use crate::front::modules::components::{distant_time, Backable, Cache, Cacheable, DISTANT_TIME_RESULT};
use crate::front::modules::module_actions::ModuleActionFn;
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

	// TODO : ajouter affichage d'erreur toaster
	async fn sync(rssContent: ArcRwSignal<Option<(u64,Feed)>>, config: ArcRwSignal<RssConfig>)
	{

		let url = config.get_untracked().link.clone();
		// 1. fetch(...)
		let Ok(window) = web_sys::window().ok_or("no window") else {return};
		let oldTime = rssContent.get_untracked().map(|content| content.0);
		let Ok(returnData) = API_proxys_wget(url.to_string(),oldTime).await else {return};
		if(returnData==proxys_return::BLANKURL) {
			return;
		}
		let proxys_return::UPDATED(time,text) = returnData else {return};
		let Ok(feed) = parser::parse(text.as_bytes()) else {return};

		rssContent.update(|rssContent| {
			*rssContent = Some((time,feed));
		})
	}

	fn refreshFn(&self)
	{
		let config = self.config.clone();
		let rssContent = self.rssContent.clone();
		spawn_local( async move {
			Self::sync(rssContent,config).await;
		});
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
		self.refreshFn();

		view! {{
			if(editMode.get())
			{
				let rssDataTitle = self.config.clone();
				let rssDataLink = self.config.clone();
				let rssDataMax = self.config.clone();
				let cacheTitle = self._update.clone();
				let cacheLink = self._update.clone();
				let cacheMax = self._update.clone();
				view!{
					<label for="rss_title"><Translate key="module_rss_title"/></label><input type="text" name="rss_title" prop:value={rssDataTitle.get().title} on:input:target=move |ev| {
						rssDataTitle.update(|inner|inner.title = ev.target().value());
						cacheTitle.update(|cache| cache.update());
					} />
					<label for="rss_link"><Translate key="module_rss_link"/></label><input type="text" name="rss_link" prop:value={rssDataLink.get().link}  on:input:target=move |ev| {
						rssDataLink.update(|inner|inner.link = ev.target().value());
						cacheLink.update(|cache| cache.update());
					}/><br/>
					<label for="rss_maxline"><Translate key="module_rss_maxline"/></label><input type="number" min="1" max="50" name="rss_maxline" prop:value={rssDataMax.get().maxline}  on:input:target=move |ev| {
						rssDataMax.update(|inner|inner.maxline = ev.target().value().parse::<u8>().unwrap_or(10));
						cacheMax.update(|cache| cache.update());
					}/><br/>
					<Translate key="module_rss_demo"/><br/>
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
								view!{
									<tr>
										<td>{match distant_time(entry.published.clone().unwrap().timestamp()){
											DISTANT_TIME_RESULT::FUTUR(time,key) => {view!{{time}<Translate key={key}/>}}
											DISTANT_TIME_RESULT::PAST(time,key) => {view!{{time}<Translate key={key}/>}}
										}}</td>
										<td><a href={entry.links.first().clone().unwrap().href.clone()} rel="noopener noreferrer nofollow" target="_blank">{entry.title.clone().unwrap().content}</a></td>
									</tr>
								}
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

	fn refresh_time(&self) -> u64 {
		return 1000*60*5;
	}

	fn refresh(&self,moduleActions: ModuleActionFn,currentName:String) {
		self.refreshFn();
	}

	fn export(&self) -> ModuleContent
	{
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
}