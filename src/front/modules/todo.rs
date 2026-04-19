use std::fmt::{Debug, Formatter};
use leptoaster::ToasterContext;
use leptos::prelude::{OnTargetAttribute, Set};
use leptos::prelude::{ElementChild, GetUntracked, PropAttribute, Update};
use leptos::prelude::{ArcRwSignal, ClassAttribute, Get, IntoAny, RwSignal};
use leptos::{component, view, IntoView};
use leptos::children::ViewFn;
use serde::{Deserialize, Serialize};
use crate::api::modules::components::{ModuleContent, ModuleID};
use crate::front::modules::components::{Backable, BoxFuture, Cache, Cacheable, ModuleName, ModuleSizeContrainte, RefreshTime};
use leptos::logging::log;
use leptos_use::watch_debounced;
use crate::front::modules::module_actions::ModuleActionFn;

static MAX_LENGTH: usize = 100000;

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
			content: ArcRwSignal::new("".to_string()),
			_update: ArcRwSignal::new(Default::default()),
			_sended: Default::default(),
		}
	}
}

impl Debug for Todo
{
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Todo")
			.field("content", &self.content.get_untracked())
			.field("_update", &self._update.get_untracked())
			.field("_sended", &self._sended.get_untracked())
			.finish()
	}
}

impl Cacheable for Todo
{
	fn cache_time(&self) -> i64 {
		self._update.get_untracked().get()
	}

	fn cache_mustUpdate(&self) -> bool
	{
		return self._update.get_untracked().isNewer(&self._sended.get_untracked());
	}

	fn cache_getUpdate(&self) -> ArcRwSignal<Cache> {
		return self._update.clone();
	}

	fn cache_getSended(&self) -> ArcRwSignal<Cache> {
		return self._sended.clone();
	}
}

impl ModuleName for Todo
{
	const MODULE_NAME: &'static str = "TODO";
}

impl Backable for Todo
{
	fn module_name(&self) -> String {
		Todo::MODULE_NAME.to_string()
	}

	fn draw(&self, _: RwSignal<bool>,moduleActions: ModuleActionFn, moduleId: ModuleID) -> ViewFn
	{
		let contentInner = self.content.clone();
		let updateInner = self._update.clone();
		ViewFn::from(move || {
			view!{
				<TodoDraw contentTocheck=contentInner.clone() cache=updateInner.clone() moduleActions=moduleActions.clone() moduleId=moduleId.clone()/>
			}.into_any()
		})
	}

	fn refresh_time(&self) -> RefreshTime {
		RefreshTime::MINUTES(1)
	}

	fn refresh(&self,moduleActions: ModuleActionFn, moduleId: ModuleID, toaster: ToasterContext) -> Option<BoxFuture> {

		let cacheSended = self._sended.clone();
		let cacheUpdate = self._update.clone();
		return Some(Box::pin(async move {
			log!("TODO refreshing");
			(moduleActions.clone().getFn)((moduleId.clone()));
		}));
	}

	fn export(&self) -> ModuleContent
	{
		return ModuleContent{
			id: ModuleID::new(),
			typeModule: self.module_name(),
			timestamp: self._update.get_untracked().get(),
			content: serde_json::to_string(&self.content.get_untracked()).unwrap_or_default(),
			..Default::default()
		};
	}

	fn import(&mut self, import: ModuleContent)
	{
		let Ok(importContent) = serde_json::from_str(&import.content.clone()) else {return};

		self.content.update(|content|{
			*content = importContent;
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
		let Ok(content) = serde_json::from_str(&from.content) else {return None};
		Some(Self {
			content: ArcRwSignal::new(content),
			_update: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
			_sended: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
		})
	}

	fn size(&self) -> ModuleSizeContrainte {
		ModuleSizeContrainte::default()
	}
}

#[component]
fn TodoDraw(contentTocheck: ArcRwSignal<String>, cache: ArcRwSignal<Cache>, moduleActions: ModuleActionFn, moduleId: ModuleID) -> impl IntoView
{
	let contentWatcher = contentTocheck.clone();
	let newWatcher = watch_debounced(
		move || {
			log!("contentWatcher deps");
			contentWatcher.get()
		},
		move |a, b, _| {
			log!("changed! {} {:?}",a,b);
			(moduleActions.clone().updateFn)((moduleId.clone()));
		},
		5000.0,
	);
	
	let contentGet = contentTocheck.clone();
	let contentWrite = contentTocheck.clone();
	let contentLen = contentTocheck.clone();
	return view!{
			<textarea class="module_todo"
                prop:value=move || contentGet.get()
				on:input:target=move |ev| {
					cache.update(|cache|{
						cache.update();
					});
					let mut newContent: String = ev.target().value();
					newContent.truncate(MAX_LENGTH);
					contentWrite.set(newContent);
				}></textarea>
			<span class="module_todo_counter">{move || contentLen.get().len()}/{MAX_LENGTH}c</span>
		}.into_any();
}