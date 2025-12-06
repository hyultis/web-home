use leptos::prelude::{AnyView, ArcRwSignal, Get, IntoAny, RwSignal};
use leptos::view;
use serde::{Deserialize, Serialize};
use crate::api::modules::components::ModuleContent;
use crate::front::modules::components::{Backable, Cache, Cacheable};
use crate::front::modules::module_actions::ModuleActionFn;

#[derive(Serialize,Deserialize,Debug)]
#[derive(Clone)]
struct MailConfig
{
	pub host: String,
	pub login: String,
	pub pwd: String,
}
impl Default for MailConfig
{
	fn default() -> Self
	{
		Self {
			host: "".to_string(),
			login: "".to_string(),
			pwd: "".to_string(),
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
#[derive(Default)]
pub struct Mail
{
	config: ArcRwSignal<MailConfig>,
	#[serde(skip_serializing,skip_deserializing)]
	rssContent: ArcRwSignal<Option<(u64,String)>>,
	_update: ArcRwSignal<Cache>,
	_sended: ArcRwSignal<Cache>,
}


impl Backable for Mail
{
	fn typeModule(&self) -> String {
		"MAIL".to_string()
	}

	fn draw(&self, editMode: RwSignal<bool>, moduleActions: ModuleActionFn, currentName: String) -> AnyView {


		view!{
		}.into_any()
	}

	fn refresh_time(&self) -> u64 {
		1000*60*60
	}

	fn refresh(&self, moduleActions: ModuleActionFn, currentName: String) {

	}

	fn export(&self) -> ModuleContent {
		todo!()
	}

	fn import(&mut self, import: ModuleContent) {
		todo!()
	}

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self>
	where
		Self: Sized
	{
		todo!()
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