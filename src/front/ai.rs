use crate::api::modules::components::{ModuleContent,ModuleID};
use crate::front::modules::components::Cache;
use serde::{Deserialize,Serialize};
use std::fmt::{Debug,Display,Formatter};
use url::Url;

pub(crate) mod chat;
pub(crate) mod configuration;
pub(crate) mod automation;
pub(crate) mod inbox;
pub(crate) mod provider;
pub(crate) mod workspace;

pub(crate) const AI_CONFIG_MAXIMUM_BYTES: usize = 512 * 1024;
pub(crate) const AI_CREDENTIAL_MAXIMUM_BYTES: usize = 16 * 1024;
pub(crate) const AI_MODEL_MAXIMUM_BYTES: usize = 256;
pub(crate) const AI_URL_MAXIMUM_BYTES: usize = 4 * 1024;
pub(crate) const AI_OUTPUT_TOKENS_MINIMUM: u32 = 256;
pub(crate) const AI_OUTPUT_TOKENS_MAXIMUM: u32 = 8_192;
pub(crate) const AI_OUTPUT_TOKENS_DEFAULT: u32 = 2_048;
const AI_CONFIG_VERSION: u8 = 1;

#[derive(Clone,Debug,Default,Eq,PartialEq)]
pub(crate) struct AiAllowedOrigins
{
	origins: Vec<String>,
}

impl AiAllowedOrigins
{
	pub(crate) fn new(mut origins: Vec<String>) -> Self
	{
		origins.sort();
		origins.dedup();
		return Self {origins};
	}

	#[cfg(any(feature="hydrate",test))]
	pub(crate) fn endpoint_isAllowed(&self,endpoint: &Url) -> bool
	{
		let origin = endpoint.origin().ascii_serialization();
		return self.origins.binary_search(&origin).is_ok();
	}
}

#[derive(Clone,Copy,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum AiProvider
{
	#[default]
	OpenAI,
	Anthropic,
	Gemini,
	Mistral,
	Ollama,
}

impl AiProvider
{
	pub(crate) const ALL: [Self;5] = [
		Self::OpenAI,
		Self::Anthropic,
		Self::Gemini,
		Self::Mistral,
		Self::Ollama,
	];

	pub(crate) fn id_get(self) -> &'static str
	{
		return match self
		{
			Self::OpenAI => "openai",
			Self::Anthropic => "anthropic",
			Self::Gemini => "gemini",
			Self::Mistral => "mistral",
			Self::Ollama => "ollama",
		};
	}

	pub(crate) fn fromId(value: &str) -> Option<Self>
	{
		return Self::ALL.into_iter().find(|provider| provider.id_get() == value);
	}

	fn credential_isRequired(self) -> bool
	{
		return self != Self::Ollama;
	}
}

#[derive(Clone,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct AiProfile
{
	pub(crate) provider: AiProvider,
	pub(crate) model: String,
	pub(crate) credential: String,
	pub(crate) baseUrl: String,
	pub(crate) maxOutputTokens: u32,
}

impl Default for AiProfile
{
	fn default() -> Self
	{
		return Self {
			provider: AiProvider::default(),
			model: String::new(),
			credential: String::new(),
			baseUrl: String::new(),
			maxOutputTokens: AI_OUTPUT_TOKENS_DEFAULT,
		};
	}
}

impl Debug for AiProfile
{
	fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result
	{
		return formatter.debug_struct("AiProfile")
			.field("provider",&self.provider)
			.field("model",&self.model)
			.field("credential",&self.credential.is_empty().then_some("[EMPTY]").unwrap_or("[REDACTED]"))
			.field("baseUrl",&self.baseUrl)
			.field("maxOutputTokens",&self.maxOutputTokens)
			.finish();
	}
}

impl AiProfile
{
	pub(crate) fn validate(&self) -> Result<(),AiConfigError>
	{
		if (!Self::model_isValid(&self.model))
		{
			return Err(AiConfigError::InvalidModel);
		}
		self.connection_validate()?;
		if (!(AI_OUTPUT_TOKENS_MINIMUM..=AI_OUTPUT_TOKENS_MAXIMUM).contains(&self.maxOutputTokens))
		{
			return Err(AiConfigError::InvalidOutputTokens);
		}
		return Ok(());
	}

	fn connection_validate(&self) -> Result<(),AiConfigError>
	{
		if (self.credential.len() > AI_CREDENTIAL_MAXIMUM_BYTES
			|| (self.provider.credential_isRequired() && self.credential.is_empty())
			|| self.credential.chars().any(char::is_control))
		{
			return Err(AiConfigError::InvalidCredential);
		}
		if (self.provider == AiProvider::Ollama)
		{
			Self::baseUrl_validate(&self.baseUrl)?;
		}
		return Ok(());
	}

	fn model_isValid(value: &str) -> bool
	{
		return !value.is_empty()
			&& value.len() <= AI_MODEL_MAXIMUM_BYTES
			&& value.trim() == value
			&& !value.chars().any(|character| character.is_whitespace() || character.is_control());
	}

	pub(crate) fn baseUrl_validate(value: &str) -> Result<Url,AiConfigError>
	{
		if (value.is_empty() || value.len() > AI_URL_MAXIMUM_BYTES || value.trim() != value)
		{
			return Err(AiConfigError::InvalidUrl);
		}
		let url = Url::parse(value).map_err(|_| AiConfigError::InvalidUrl)?;
		if (!matches!(url.scheme(),"http" | "https")
			|| url.host_str().is_none()
			|| !url.username().is_empty()
			|| url.password().is_some()
			|| !matches!(url.path(),"" | "/")
			|| url.query().is_some()
			|| url.fragment().is_some())
		{
			return Err(AiConfigError::InvalidUrl);
		}
		return Ok(url);
	}
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct AiConfigDocument
{
	version: u8,
	pub(crate) profile: Option<AiProfile>,
	pub(crate) contexts: Vec<automation::AiAutomationContext>,
	pub(crate) history: Vec<automation::AiAutomationHistoryEntry>,
}

impl Default for AiConfigDocument
{
	fn default() -> Self
	{
		return Self {
			version: AI_CONFIG_VERSION,
			profile: None,
			contexts: Vec::new(),
			history: Vec::new(),
		};
	}
}

impl AiConfigDocument
{
	pub(crate) fn automationRuntime_reconcile(&mut self,current: &Self)
	{
		for context in &mut self.contexts
		{
			let currentContext = current.contexts.iter().find(|candidate| candidate.id == context.id);
			context.checkpoint_reconcile(currentContext);
		}
		self.history = current.history.clone();
	}

	pub(crate) fn automationHistory_add(
		&mut self,
		entry: automation::AiAutomationHistoryEntry,
	) -> Result<(),AiConfigError>
	{
		automation::automationHistory_add(&mut self.history,entry)?;
		return Ok(());
	}

	pub(crate) fn validate(&self) -> Result<(),AiConfigError>
	{
		if (self.version != AI_CONFIG_VERSION)
		{
			return Err(AiConfigError::UnsupportedVersion);
		}
		if let Some(profile) = &self.profile
		{
			profile.validate()?;
		}
		automation::automationContexts_validate(&self.contexts)?;
		automation::automationHistory_validate(&self.history)?;
		let content = serde_json::to_string(self).map_err(|_| AiConfigError::InvalidDocument)?;
		if (content.len() > AI_CONFIG_MAXIMUM_BYTES)
		{
			return Err(AiConfigError::DocumentTooLarge);
		}
		return Ok(());
	}

	pub(crate) fn serialize(&self) -> Result<String,AiConfigError>
	{
		self.validate()?;
		return serde_json::to_string(self).map_err(|_| AiConfigError::InvalidDocument);
	}

	fn deserialize(content: &str) -> Result<Self,AiConfigError>
	{
		if (content.len() > AI_CONFIG_MAXIMUM_BYTES)
		{
			return Err(AiConfigError::DocumentTooLarge);
		}
		let document = serde_json::from_str::<Self>(content).map_err(|_| AiConfigError::InvalidDocument)?;
		document.validate()?;
		return Ok(document);
	}
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub(crate) enum AiConfigError
{
	Automation(automation::AiAutomationError),
	UnsupportedVersion,
	InvalidDocument,
	DocumentTooLarge,
	InvalidModel,
	InvalidCredential,
	InvalidUrl,
	InvalidOutputTokens,
}

impl AiConfigError
{
	pub(crate) fn translateKey_get(self) -> &'static str
	{
		return match self
		{
			Self::Automation(error) => error.translateKey_get(),
			Self::InvalidModel => "FRONTAI_CONFIG_MODEL_INVALID",
			Self::InvalidCredential => "FRONTAI_CONFIG_CREDENTIAL_INVALID",
			Self::InvalidUrl => "FRONTAI_CONFIG_URL_INVALID",
			Self::InvalidOutputTokens => "FRONTAI_CONFIG_TOKENS_INVALID",
			Self::DocumentTooLarge => "FRONTAI_CONFIG_TOO_LARGE",
			Self::UnsupportedVersion | Self::InvalidDocument => "FRONTAI_CONFIG_INVALID",
		};
	}
}

impl From<automation::AiAutomationError> for AiConfigError
{
	fn from(error: automation::AiAutomationError) -> Self
	{
		return Self::Automation(error);
	}
}

pub(crate) struct AiConfigHolder
{
	id: ModuleID,
	document: AiConfigDocument,
	update: Cache,
	sended: Cache,
}

impl AiConfigHolder
{
	pub(crate) const MODULE_ID: &'static str = "AI_CONFIG";
	pub(crate) const MODULE_NAME: &'static str = "AI_CONFIG";

	pub(crate) fn new() -> Self
	{
		let cache = Cache::default();
		return Self {
			id: ModuleID {id: Self::MODULE_ID.to_string()},
			document: AiConfigDocument::default(),
			update: cache.clone(),
			sended: cache,
		};
	}

	pub(crate) fn document_get(&self) -> AiConfigDocument
	{
		return self.document.clone();
	}

	pub(crate) fn id_get(&self) -> ModuleID
	{
		return self.id.clone();
	}

	pub(crate) fn cache_time(&self) -> i64
	{
		return self.update.get();
	}

	pub(crate) fn cache_mustUpdate(&self) -> bool
	{
		return self.update.isNewer(&self.sended);
	}

	pub(crate) fn timestamp_next(&self) -> i64
	{
		return Cache::now().max(self.update.get().saturating_add(1));
	}

	pub(crate) fn export_document(&self, document: &AiConfigDocument, timestamp: i64) -> Result<ModuleContent,AiConfigError>
	{
		return Ok(ModuleContent {
			id: self.id.clone(),
			typeModule: Self::MODULE_NAME.to_string(),
			timestamp,
			content: document.serialize()?,
			..Default::default()
		});
	}

	pub(crate) fn import(&mut self, content: ModuleContent) -> Result<(),AiConfigError>
	{
		if (content.id.id != Self::MODULE_ID || content.typeModule != Self::MODULE_NAME)
		{
			return Err(AiConfigError::InvalidDocument);
		}
		let document = AiConfigDocument::deserialize(&content.content)?;
		self.document = document;
		self.update.update_from(content.timestamp);
		self.sended.update_from(content.timestamp);
		return Ok(());
	}

	pub(crate) fn saved_apply(&mut self, document: AiConfigDocument, timestamp: i64)
	{
		self.document = document;
		self.update.update_from(timestamp);
		self.sended.update_from(timestamp);
	}
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub(crate) enum AiConfigSaveError
{
	Configuration(AiConfigError),
	AUTH_REQUIRED,
	CRYPTO_CONTEXT_MISSING,
	CRYPTO_ENCRYPT_FAILED,
	CRYPTO_DECRYPT_FAILED,
	OUTDATED,
	LIFECYCLE_CLOSED,
	SERVER_ERROR,
}

impl AiConfigSaveError
{
	pub(crate) fn translateKey_get(self) -> &'static str
	{
		return match self
		{
			Self::Configuration(error) => error.translateKey_get(),
			Self::AUTH_REQUIRED => "FRONTAI_SAVE_AUTH_REQUIRED",
			Self::CRYPTO_CONTEXT_MISSING => "FRONTERROR_CRYPTO_CONTEXT_MISSING",
			Self::CRYPTO_ENCRYPT_FAILED => "FRONTERROR_CRYPTO_ENCRYPT_FAILED",
			Self::CRYPTO_DECRYPT_FAILED => "FRONTERROR_CRYPTO_DECRYPT_FAILED",
			Self::OUTDATED => "FRONTAI_SAVE_OUTDATED",
			Self::LIFECYCLE_CLOSED => "FRONTAI_SAVE_INTERRUPTED",
			Self::SERVER_ERROR => "FRONTAI_SAVE_SERVER_ERROR",
		};
	}
}

impl Display for AiConfigSaveError
{
	fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result
	{
		return formatter.write_str(self.translateKey_get());
	}
}

impl From<AiConfigError> for AiConfigSaveError
{
	fn from(error: AiConfigError) -> Self
	{
		return Self::Configuration(error);
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn defaultDocument_isDisabledAndVersioned()
	{
		let content = AiConfigDocument::default().serialize().unwrap();
		let restored = AiConfigDocument::deserialize(&content).unwrap();

		assert!(restored.profile.is_none());
		assert!(restored.contexts.is_empty());
		assert!(restored.history.is_empty());
		assert_eq!(restored.version,AI_CONFIG_VERSION);
	}

	#[test]
	fn legacyDocumentWithoutContextsRemainsReadable()
	{
		let restored = AiConfigDocument::deserialize(r#"{"version":1,"profile":null}"#).unwrap();

		assert!(restored.profile.is_none());
		assert!(restored.contexts.is_empty());
		assert!(restored.history.is_empty());
	}

	#[test]
	fn userConfigurationReconciliationPreservesAppliedActionHistory()
	{
		let action = automation::AiValidatedAction {
			actionKey: "action-1".to_string(),
			executionId: "execution-1".to_string(),
			targetModuleId: ModuleID {id: "calendar-instance".to_string()},
			action: "calendar.event.create".to_string(),
			arguments: vec![automation::AiNamedValue {
				id: "title".to_string(),value: automation::AiValue::Text("Appointment".to_string()),
			}],
			confirmation: automation::AiConfirmationPolicy::Confirm,
		};
		let mut current = AiConfigDocument::default();
		current.automationHistory_add(automation::AiAutomationHistoryEntry::new(
			"Mail appointments","CALENDAR",&action,100,
		).unwrap()).unwrap();
		let mut draft = AiConfigDocument::default();

		draft.automationRuntime_reconcile(&current);

		assert_eq!(draft.history,current.history);
		assert!(draft.serialize().unwrap().contains("Appointment"));
	}

	#[test]
	fn profileValidation_requiresExactOllamaOrigin()
	{
		let mut profile = AiProfile {
			provider: AiProvider::Ollama,
			model: "qwen3".to_string(),
			baseUrl: "http://192.168.1.20:11434".to_string(),
			..Default::default()
		};
		assert!(profile.validate().is_ok());
		profile.baseUrl = "https://ollama.example/api".to_string();
		assert_eq!(profile.validate(),Err(AiConfigError::InvalidUrl));
	}

	#[test]
	fn allowedOrigins_requireAnExactSchemeHostAndPort()
	{
		let origins = AiAllowedOrigins::new(vec![
			"http://192.168.1.20:11434".to_string(),
			"https://ollama.example".to_string(),
			"http://192.168.1.20:11434".to_string(),
		]);

		assert!(origins.endpoint_isAllowed(&Url::parse("http://192.168.1.20:11434/api/chat").unwrap()));
		assert!(origins.endpoint_isAllowed(&Url::parse("https://ollama.example/api/chat").unwrap()));
		assert!(!origins.endpoint_isAllowed(&Url::parse("http://192.168.1.20/api/chat").unwrap()));
		assert!(!origins.endpoint_isAllowed(&Url::parse("https://ollama.example:11434/api/chat").unwrap()));
	}

	#[test]
	fn publicProvider_requiresCredentialAndBoundedTokens()
	{
		let mut profile = AiProfile {
			model: "gpt-5".to_string(),
			..Default::default()
		};
		assert_eq!(profile.validate(),Err(AiConfigError::InvalidCredential));
		profile.credential = "secret".to_string();
		assert!(profile.validate().is_ok());
		profile.maxOutputTokens = AI_OUTPUT_TOKENS_MAXIMUM + 1;
		assert_eq!(profile.validate(),Err(AiConfigError::InvalidOutputTokens));
	}

	#[test]
	fn debugNeverContainsCredential()
	{
		let profile = AiProfile {
			model: "gpt-5".to_string(),
			credential: "credential-that-must-not-leak".to_string(),
			..Default::default()
		};
		let debug = format!("{profile:?}");

		assert!(!debug.contains("credential-that-must-not-leak"));
		assert!(debug.contains("[REDACTED]"));
	}

	#[test]
	fn unknownFieldsAndVersionsFailClosed()
	{
		assert_eq!(
			AiConfigDocument::deserialize(r#"{"version":1,"profile":null,"unknown":true}"#),
			Err(AiConfigError::InvalidDocument),
		);
		assert_eq!(
			AiConfigDocument::deserialize(r#"{"version":2,"profile":null}"#),
			Err(AiConfigError::UnsupportedVersion),
		);
		assert_eq!(
			AiConfigDocument::deserialize(r#"{"version":1,"profile":null,"contexts":[{"version":1,"unknown":true}]}"#),
			Err(AiConfigError::InvalidDocument),
		);
	}

	#[test]
	fn specialContentRejectsAnUnstableIdentity()
	{
		let mut holder = AiConfigHolder::new();
		let content = ModuleContent {
			id: ModuleID {id: "random-module".to_string()},
			typeModule: AiConfigHolder::MODULE_NAME.to_string(),
			content: AiConfigDocument::default().serialize().unwrap(),
			..Default::default()
		};

		assert_eq!(holder.import(content),Err(AiConfigError::InvalidDocument));
	}
}
