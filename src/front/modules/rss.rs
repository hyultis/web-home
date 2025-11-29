use leptos::prelude::{ClassAttribute, ElementChild, OnAttribute};
use feed_rs::model::Feed;
use feed_rs::parser;
use leptos::prelude::{AnyView, ArcRwSignal, Get, GetUntracked, IntoAny, RwSignal, Update};
use leptos::view;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{MouseEvent, Response};
use crate::api::modules::components::ModuleContent;
use crate::front::modules::components::{Backable, Cache, Cacheable};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::all_front_enum::AllFrontUIEnum;
use crate::front::utils::translate::Translate;

#[derive(Serialize,Deserialize,Default,Debug)]
#[derive(Clone)]
struct RssContent
{
	pub title: String,
	pub link: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[derive(Default)]
pub struct Rss
{
	content: ArcRwSignal<RssContent>,
	#[serde(skip_serializing,skip_deserializing)]
	rssContent: ArcRwSignal<Option<Feed>>,
	rssLastUpdate: ArcRwSignal<Cache>,
	_update: ArcRwSignal<Cache>,
	_sended: ArcRwSignal<Cache>,
}

impl Rss
{
	pub fn new() -> Self
	{
		Self {
			content: Default::default(),
			rssContent: Default::default(),
			rssLastUpdate: Default::default(),
			_update: Default::default(),
			_sended: Default::default(),
		}
	}

	async fn sync(rssContent: ArcRwSignal<Option<Feed>>, rssLastUpdate: ArcRwSignal<Cache>)
	{
		let url = "https://www.lemonde.fr/rss/une.xml";
		// 1. fetch(...)
		let Ok(window) = web_sys::window().ok_or("no window") else {return};
		let Ok(resp_value) = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url)).await else {return};
		let Ok(resp): Result<Response,_> = resp_value.dyn_into() else {return};

		// 2. Lire le body -> ArrayBuffer -> Vec<u8>
		let Ok(buf_promise) = resp.array_buffer() else {return};
		let Ok(buf) = wasm_bindgen_futures::JsFuture::from(buf_promise).await else {return};
		let array = js_sys::Uint8Array::new(&buf);
		let mut bytes = vec![0; array.length() as usize];
		array.copy_to(&mut bytes);

		// 3. Convertir en &str
		let Ok(text) = str::from_utf8(&bytes) else {return};

		// 4. Parser WASM-compatible
		let Ok(feed) = parser::parse(text.as_bytes()) else {return};

		rssContent.update(|rssContent| {
			*rssContent = Some(feed);
			rssLastUpdate.update(|cache|{
				cache.update();
			});
		})
	}

	fn refreshFn(&self) -> impl Fn(MouseEvent) + Clone
	{
		let content = self.content.clone();
		let rssContent = self.rssContent.clone();
		let rssLastUpdate = self.rssLastUpdate.clone();
		return move |_| {
			let rssContent = rssContent.clone();
			let rssLastUpdate = rssLastUpdate.clone();
			spawn_local( async move {
				Self::sync(rssContent.clone(),rssLastUpdate.clone()).await;
			});
		}
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

	fn draw(&self, editMode: RwSignal<bool>, moduleActions: ModuleActionFn, currentName: String) -> AnyView {

		let refreshFn = self.refreshFn();

		view! {{
				self.rssContent.get().map(|rssContent|{
					let title = rssContent.title.clone();
					let link = rssContent.title.clone().map(|title|title.src).flatten();

					view!{
						<>
						<h1>{rssContent.title.clone().map(|title|title.content)}</h1>
						{ link.map(|link|{view!{<a href={link.clone()}>{link.clone()}</a>}})}
						<span>{rssContent.description.map(|desc| desc.content)}</span>
						</>
					}.into_any()
				})
			}
			<button class="validate" on:click=refreshFn><Translate key={AllFrontUIEnum::UPDATE.to_string()}/></button>
		}.into_any()
	}

	fn export(&self) -> ModuleContent
	{
		return ModuleContent{
			name: self.typeModule(),
			typeModule: self.typeModule(),
			timestamp: self._update.get_untracked().get(),
			content: serde_json::to_string(&self.content.get_untracked()).unwrap_or_default(),
			pos: [0,0],
			size: [0,0],
		};
	}

	fn import(&mut self, import: ModuleContent)
	{
		let Ok(content) = serde_json::from_str(&import.content.clone()) else {return};

		self.content = content;
		self._update.update(|cache|{
			cache.update_from(import.timestamp);
		});
		self._sended.update(|cache|{
			cache.update_from(import.timestamp);
		});
	}

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self> {
		let Ok(content) = serde_json::from_str(&from.content) else {return None};
		Some(Self {
			content: ArcRwSignal::new(content),
			rssContent: Default::default(),
			rssLastUpdate: Default::default(),
			_update: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
			_sended: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
		})
	}
}