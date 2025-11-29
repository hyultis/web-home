use leptos::prelude::{AnyView, ArcRwSignal, IntoAny, RwSignal};
use leptos::view;
use strum_macros::EnumDiscriminants;
use crate::api::modules::components::ModuleContent;
use crate::front::modules::components::{Backable, Cache, Cacheable};
use crate::front::modules::{moduleContent};
use crate::front::modules::todo::Todo;
use strum_macros::EnumIter;
use crate::front::modules::module_actions::ModuleActionFn;

#[derive(EnumDiscriminants,Debug)]
#[strum_discriminants(derive(strum_macros::Display,EnumIter))]
pub enum ModuleType
{
	#[strum(to_string = "RSS")]
	RSS(String),
	#[strum(to_string = "TODO")]
	TODO(Todo)
}

impl Backable for ModuleType {
	fn typeModule(&self) -> String {
		match self {
			ModuleType::RSS(_) => { "RSS".to_string()}
			ModuleType::TODO(todo) => todo.typeModule()
		}
	}

	fn draw(&self, editMode: RwSignal<bool>,moduleActions: ModuleActionFn,currentName:String) -> AnyView {
		match self {
			ModuleType::RSS(_) => view!{<span/>}.into_any(),
			ModuleType::TODO(todo) => todo.draw(editMode,moduleActions,currentName)
		}
	}

	fn export(&self) -> ModuleContent {
		return match self {
			ModuleType::RSS(_) => { ModuleContent::new("RSS".to_string(),"RSS".to_string())}
			ModuleType::TODO(todo) => todo.export()
		}
	}

	fn import(&mut self, import: ModuleContent) {
		match self {
			ModuleType::RSS(_) => {}
			ModuleType::TODO(todo) => todo.import(import)
		}
	}

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self> {
		match from.typeModule.as_str() {
			"RSS" => Some(Self::RSS(from.name.clone())),
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
			ModuleType::RSS(_) => { false }
			ModuleType::TODO(todo) => todo.cache_mustUpdate()
		}
	}

	fn cache_getUpdate(&self) -> ArcRwSignal<Cache> {
		match self {
			ModuleType::RSS(_) => { ArcRwSignal::new(Cache::default()) }
			ModuleType::TODO(todo) => todo.cache_getUpdate()
		}
	}

	fn cache_getSended(&self) -> ArcRwSignal<Cache> {
		match self {
			ModuleType::RSS(_) => { ArcRwSignal::new(Cache::default()) }
			ModuleType::TODO(todo) => todo.cache_getSended()
		}
	}
}

impl moduleContent for ModuleType
{

}