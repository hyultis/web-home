use leptos::ev::MouseEvent;
use leptos::prelude::{ElementChild, GetUntracked, OnAttribute, Update};
use leptos::prelude::{AnyView, ArcRwSignal, ClassAttribute, Get, IntoAny, RwSignal};
use leptos::view;
use serde::{Deserialize, Serialize};
use crate::api::modules::components::ModuleContent;
use crate::front::modules::components::{Backable, Cache, Cacheable};
use crate::front::utils::all_front_enum::AllFrontUIEnum;
use crate::front::utils::translate::Translate;

#[derive(Serialize,Deserialize,Default)]
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
			content: ArcRwSignal::new("Todo!()".to_string()),
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
}

impl Backable for Todo
{
	fn name(&self) -> String {
		"todo".to_string()
	}

	fn draw(&self, _: RwSignal<bool>) -> AnyView {

		let updateFn = self.updateFn();

		view!{
			<>
			<textarea>{self.content.get()}</textarea><br/>
			<button class="validate" on:click=updateFn><Translate key={AllFrontUIEnum::UPDATE.to_string()}/></button>
			</>
		}.into_any()
	}

	fn export(&self) -> ModuleContent
	{
		return ModuleContent{
			name: self.name(),
			timestamp: self._update.get_untracked().get(),
			content: serde_json::to_string(&self.content).unwrap(),
		};
	}

	fn import(&mut self, import: ModuleContent)
	{
		let Ok(content) = serde_json::from_str(&import.content) else {return};

		self.content = content;
		self._update.update(|cache|{
			cache.update_from(import.timestamp);
		});
		self._sended.update(|cache|{
			cache.update_from(import.timestamp);
		});
	}
}