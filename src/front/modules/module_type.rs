use leptoaster::ToasterContext;
use leptos::prelude::{ArcRwSignal, RwSignal, ViewFn};
use strum_macros::EnumDiscriminants;
use crate::api::modules::components::{ModuleContent, ModuleID};
use crate::front::modules::components::{moduleContent, Backable, BoxFuture, Cache, Cacheable, ModuleConfigViewFn, ModuleName, ModuleSizeContrainte, RefreshTime};
use crate::front::modules::calendar::Calendar;
use crate::front::modules::todo::Todo;
use strum_macros::EnumIter;
use crate::front::modules::mail::Mail;
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::modules::rss::Rss;
use crate::front::modules::weather::Weather;
use crate::front::ai::automation::{
	AiActionFuture,AiActionPersistence,AiAutomationCapable,AiAutomationError,AiAutomationEvent,
	AiCapabilityCatalog,AiExposureFuture,AiExposureRequest,AiEventReservation,AiModuleGrant,
	AiValidatedAction,
};

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
	#[strum(to_string = "CALENDAR")]
	CALENDAR(Calendar),
}

impl ModuleTypeDiscriminants
{
	pub(crate) fn translateKey_get(&self) -> &'static str
	{
		return match self
		{
			Self::RSS => "MODULE_TYPE_RSS",
			Self::TODO => "MODULE_TYPE_TODO",
			Self::MAIL => "MODULE_TYPE_MAIL",
			Self::WEATHER => "MODULE_TYPE_WEATHER",
			Self::CALENDAR => "MODULE_TYPE_CALENDAR",
		};
	}
}

impl ModuleType {
	fn intoAiCapable(&self) -> &dyn AiAutomationCapable
	{
		return match self
		{
			ModuleType::RSS(module) => module,
			ModuleType::TODO(module) => module,
			ModuleType::MAIL(module) => module,
			ModuleType::WEATHER(module) => module,
			ModuleType::CALENDAR(module) => module,
		};
	}

	pub fn intoBackable(&self) -> Box<&dyn Backable> {
		match self {
			ModuleType::RSS(x) => Box::new(x),
			ModuleType::TODO(x) => Box::new(x),
			ModuleType::MAIL(x) => Box::new(x),
			ModuleType::WEATHER(x) => Box::new(x),
			ModuleType::CALENDAR(x) => Box::new(x),
		}
	}

	pub fn intoBackableMut(&mut self) -> Box<&mut dyn Backable> {
		match self {
			ModuleType::RSS(x) => Box::new(x),
			ModuleType::TODO(x) => Box::new(x),
			ModuleType::MAIL(x) => Box::new(x),
			ModuleType::WEATHER(x) => Box::new(x),
			ModuleType::CALENDAR(x) => Box::new(x),
		}
	}

	pub fn intoCachable(&self) -> Box<&dyn Cacheable> {
		match self {
			ModuleType::RSS(x) => Box::new(x),
			ModuleType::TODO(x) => Box::new(x),
			ModuleType::MAIL(x) => Box::new(x),
			ModuleType::WEATHER(x) => Box::new(x),
			ModuleType::CALENDAR(x) => Box::new(x),
		}
	}
}

impl AiAutomationCapable for ModuleType
{
	fn ai_capabilities(&self) -> AiCapabilityCatalog
	{
		return self.intoAiCapable().ai_capabilities();
	}

	fn ai_grant(&self) -> AiModuleGrant
	{
		return self.intoAiCapable().ai_grant();
	}

	fn ai_exposure(&self,request: AiExposureRequest) -> Option<AiExposureFuture>
	{
		return self.intoAiCapable().ai_exposure(request);
	}

	fn ai_action_apply(&self,action: AiValidatedAction) -> Option<AiActionFuture>
	{
		return self.intoAiCapable().ai_action_apply(action);
	}

	fn ai_actionPersistence_prepare(
		&self,
		action: &AiValidatedAction,
		base: Option<&ModuleContent>,
	) -> Result<AiActionPersistence,AiAutomationError>
	{
		return self.intoAiCapable().ai_actionPersistence_prepare(action,base);
	}

	fn ai_actionPersistence_saved(&self,content: &ModuleContent) -> Result<(),AiAutomationError>
	{
		return self.intoAiCapable().ai_actionPersistence_saved(content);
	}

	fn ai_eventRetry(&self,event: &AiAutomationEvent)
	{
		self.intoAiCapable().ai_eventRetry(event);
	}

	fn ai_eventReservation_prepare(
		&self,
		event: &AiAutomationEvent,
		base: Option<&ModuleContent>,
	) -> Result<AiEventReservation,AiAutomationError>
	{
		return self.intoAiCapable().ai_eventReservation_prepare(event,base);
	}

	fn ai_eventReservation_saved(
		&self,
		content: &ModuleContent,
	) -> Result<(),AiAutomationError>
	{
		return self.intoAiCapable().ai_eventReservation_saved(content);
	}
}

impl Backable for ModuleType {
	fn module_name(&self) -> String {
		return self.intoBackable().module_name();
	}

	fn draw(&self, editMode: RwSignal<bool>,moduleActions: ModuleActionFn, moduleId: ModuleID) -> ViewFn {
		return self.intoBackable().draw(editMode,moduleActions,moduleId);
	}

	fn draw_config(&self,moduleActions: ModuleActionFn,moduleId: ModuleID) -> Option<ModuleConfigViewFn>
	{
		return self.intoBackable().draw_config(moduleActions,moduleId);
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

	fn isOlderThan(&self, other: &ModuleContent) -> bool
	{
		return match self {
			ModuleType::RSS(x) => x.isOlderThan(other),
			ModuleType::TODO(x) => x.isOlderThan(other),
			ModuleType::MAIL(x) => x.isOlderThan(other),
			ModuleType::WEATHER(x) => x.isOlderThan(other),
			ModuleType::CALENDAR(x) => x.isOlderThan(other),
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
			"WEATHER" => {
				Weather::newFromModuleContent(from).map(|content| Self::WEATHER(content))
			},
			"MAIL" => {
				Mail::newFromModuleContent(from).map(|content| Self::MAIL(content))
			},
			"CALENDAR" => {
				Calendar::newFromModuleContent(from).map(|content| Self::CALENDAR(content))
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
		"CALENDAR" => Some(ModuleType::CALENDAR(Calendar::new())),
		&_ => None
	}
}
