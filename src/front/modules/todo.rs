use leptos::prelude::{OnTargetAttribute, Set};
use leptos::ev::MouseEvent;
use leptos::prelude::{ElementChild, GetUntracked, OnAttribute, PropAttribute, Update};
use leptos::prelude::{AnyView, ArcRwSignal, ClassAttribute, Get, IntoAny, RwSignal};
use leptos::view;
use serde::{Deserialize, Serialize};
use crate::api::modules::components::ModuleContent;
use crate::front::modules::components::{Backable, Cache, Cacheable};
use crate::front::utils::all_front_enum::AllFrontUIEnum;
use crate::front::utils::translate::Translate;

#[derive(Serialize,Deserialize,Default,Debug)]
pub struct Todo
{
	content: ArcRwSignal<String>,
	_update: ArcRwSignal<Cache>,
	_sended: ArcRwSignal<Cache>,
}

impl Todo
{
	pub fn new() -> Self
	{
		Self {
			content: ArcRwSignal::new("".to_string()),
			_update: ArcRwSignal::new(Default::default()),
			_sended: Default::default(),
		}
	}

	pub fn updateFn(&self) -> impl Fn(MouseEvent) + Clone
	{
		return move |_| {

		}
	}
}

impl Cacheable for Todo
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

impl Backable for Todo
{
	fn typeModule(&self) -> String {
		"TODO".to_string()
	}

	fn draw(&self, _: RwSignal<bool>) -> AnyView {

		let updateFn = self.updateFn();
		let content = self.content.clone();
		let contentWrite = self.content.clone();
		let contentCache = self._update.clone();

		view!{
			<>
			<textarea
                prop:value=move || content.get()
				on:input:target=move |ev| {
					contentCache.update(|cache|{
						cache.update();
					});
					contentWrite.set(ev.target().value())
				}>{content.get()}</textarea><br/>
			<button class="validate" on:click=updateFn><Translate key={AllFrontUIEnum::UPDATE.to_string()}/></button>
			</>
		}.into_any()
	}

	fn export(&self) -> ModuleContent
	{
		return ModuleContent{
			name: self.typeModule(),
			typeModule: self.typeModule(),
			timestamp: self._update.get_untracked().get(),
			content: serde_json::to_string(&self.content.get_untracked()).unwrap(),
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
			_update: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
			_sended: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
		})
	}
}