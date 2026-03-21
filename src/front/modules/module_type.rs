use leptoaster::ToasterContext;
use leptos::prelude::{AnyView, ArcRwSignal, RwSignal};
use strum_macros::EnumDiscriminants;
use crate::api::modules::components::{ModuleContent, ModuleID};
use crate::front::modules::components::{moduleContent, Backable, BoxFuture, Cache, Cacheable, ModuleName, ModuleSizeContrainte, RefreshTime};
use crate::front::modules::todo::Todo;
use strum_macros::EnumIter;
use crate::front::modules::mail::Mail;
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::modules::rss::Rss;
use crate::front::modules::weather::Weather;

#[derive(EnumDiscriminants,Debug)]
#[strum_discriminants(derive(strum_macros::Display,EnumIter))]
pub enum ModuleType
{
	#[strum(to_string = "RSS")]
	RSS(Rss),
	#[strum(to_string = "TODO")]
	TODO(Todo),
	#[strum(to_string = "MAIL")]
	MAIL(Mail),
	#[strum(to_string = "WEATHER")]
	WEATHER(Weather),
}

impl ModuleType {
	pub fn intoBackable(&self) -> Box<&dyn Backable> {
		match self {
			ModuleType::RSS(x) => Box::new(x),
			ModuleType::TODO(x) => Box::new(x),
			ModuleType::MAIL(x) => Box::new(x),
			ModuleType::WEATHER(x) => Box::new(x),
		}
	}

	pub fn intoBackableMut(&mut self) -> Box<&mut dyn Backable> {
		match self {
			ModuleType::RSS(x) => Box::new(x),
			ModuleType::TODO(x) => Box::new(x),
			ModuleType::MAIL(x) => Box::new(x),
			ModuleType::WEATHER(x) => Box::new(x),
		}
	}

	pub fn intoCachable(&self) -> Box<&dyn Cacheable> {
		match self {
			ModuleType::RSS(x) => Box::new(x),
			ModuleType::TODO(x) => Box::new(x),
			ModuleType::MAIL(x) => Box::new(x),
			ModuleType::WEATHER(x) => Box::new(x),
		}
	}
}

impl Backable for ModuleType {
	fn module_name(&self) -> String {
		return self.intoBackable().module_name();
	}

	fn draw(&self, editMode: RwSignal<bool>,moduleActions: ModuleActionFn, moduleId: ModuleID) -> AnyView {
		return self.intoBackable().draw(editMode,moduleActions,moduleId);
	}

	fn refresh_time(&self) -> RefreshTime {
		return self.intoBackable().refresh_time();
	}

	fn refresh(&self,moduleActions: ModuleActionFn, moduleId: ModuleID, toaster: ToasterContext) -> Option<BoxFuture> {
		return self.intoBackable().refresh(moduleActions,moduleId,toaster);
	}

	fn export(&self) -> ModuleContent {
		return self.intoBackable().export();
	}

	fn import(&mut self, import: ModuleContent) {
		return self.intoBackableMut().import(import);
	}

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self> {
		match from.typeModule.as_str() {
			"RSS" => {
				Rss::newFromModuleContent(from).map(|content| Self::RSS(content))
			},
			"TODO" => {
				Todo::newFromModuleContent(from).map(|content| Self::TODO(content))
			},
			"WEATHER" => {
				Weather::newFromModuleContent(from).map(|content| Self::WEATHER(content))
			},
			"MAIL" => {
				Mail::newFromModuleContent(from).map(|content| Self::MAIL(content))
			},
			&_ => panic!("ModuleType::newFromModuleContent : unknown module type {}", from.typeModule)
		}
	}

	fn size(&self) -> ModuleSizeContrainte {
		self.intoBackable().size()
	}
}

impl Cacheable for ModuleType {
	fn cache_time(&self) -> i64 {
		self.intoCachable().cache_time()
	}

	fn cache_mustUpdate(&self) -> bool {
		return self.intoCachable().cache_mustUpdate();
	}

	fn cache_getUpdate(&self) -> ArcRwSignal<Cache> {
		return self.intoCachable().cache_getUpdate();
	}

	fn cache_getSended(&self) -> ArcRwSignal<Cache> {
		return self.intoCachable().cache_getSended();
	}
}

impl ModuleName for ModuleType { const MODULE_NAME: &'static str = "MODULETYPE"; }

impl moduleContent for ModuleType
{

}

pub fn StringToModuleType(from: impl AsRef<str>) -> Option<ModuleType>
{
	match from.as_ref() {
		"RSS" => Some(ModuleType::RSS(Default::default())),
		"TODO" => Some(ModuleType::TODO(Default::default())),
		"WEATHER" => Some(ModuleType::WEATHER(Default::default())),
		"MAIL" => Some(ModuleType::MAIL(Default::default())),
		&_ => None
	}
}