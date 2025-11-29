use leptos::prelude::{AnyView, ArcRwSignal, RwSignal};
use strum_macros::EnumDiscriminants;
use crate::api::modules::components::ModuleContent;
use crate::front::modules::components::{Backable, Cache, Cacheable};
use crate::front::modules::{moduleContent};
use crate::front::modules::todo::Todo;
use strum_macros::EnumIter;
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::modules::rss::Rss;

#[derive(EnumDiscriminants,Debug)]
#[strum_discriminants(derive(strum_macros::Display,EnumIter))]
pub enum ModuleType
{
	#[strum(to_string = "RSS")]
	RSS(Rss),
	#[strum(to_string = "TODO")]
	TODO(Todo)
}

impl Backable for ModuleType {
	fn typeModule(&self) -> String {
		match self {
			ModuleType::RSS(rss) => rss.typeModule(),
			ModuleType::TODO(todo) => todo.typeModule()
		}
	}

	fn draw(&self, editMode: RwSignal<bool>,moduleActions: ModuleActionFn,currentName:String) -> AnyView {
		match self {
			ModuleType::RSS(rss) => rss.draw(editMode,moduleActions,currentName),
			ModuleType::TODO(todo) => todo.draw(editMode,moduleActions,currentName)
		}
	}

	fn export(&self) -> ModuleContent {
		return match self {
			ModuleType::RSS(rss) => rss.export(),
			ModuleType::TODO(todo) => todo.export()
		}
	}

	fn import(&mut self, import: ModuleContent) {
		match self {
			ModuleType::RSS(rss) => rss.import(import),
			ModuleType::TODO(todo) => todo.import(import)
		}
	}

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self> {
		match from.typeModule.as_str() {
			"RSS" => {
				Rss::newFromModuleContent(from).map(|content| Self::RSS(content))
			},
			"TODO" => {
				Todo::newFromModuleContent(from).map(|content| Self::TODO(content))
			},
			&_ => panic!("ModuleType::newFromModuleContent : unknown module type {}", from.typeModule)
		}
	}
}

impl Cacheable for ModuleType {
	fn cache_mustUpdate(&self) -> bool {
		match self {
			ModuleType::RSS(rss) => rss.cache_mustUpdate(),
			ModuleType::TODO(todo) => todo.cache_mustUpdate()
		}
	}

	fn cache_getUpdate(&self) -> ArcRwSignal<Cache> {
		match self {
			ModuleType::RSS(rss) => rss.cache_getUpdate(),
			ModuleType::TODO(todo) => todo.cache_getUpdate()
		}
	}

	fn cache_getSended(&self) -> ArcRwSignal<Cache> {
		match self {
			ModuleType::RSS(rss) => rss.cache_getSended(),
			ModuleType::TODO(todo) => todo.cache_getSended()
		}
	}
}

impl moduleContent for ModuleType
{

}