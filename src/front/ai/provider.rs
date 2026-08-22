use super::{AiAllowedOrigins,AiConfigError,AiProfile,AI_OUTPUT_TOKENS_MINIMUM};
use serde_json::Value;
use std::fmt::{Debug,Formatter};
use std::rc::Rc;
#[cfg(any(feature="hydrate",test))]
use super::AiProvider;
#[cfg(any(feature="hydrate",test))]
use std::collections::BTreeMap;
#[cfg(any(feature="hydrate",test))]
use serde_json::json;
#[cfg(any(feature="hydrate",test))]
use url::Url;

#[cfg(any(feature="hydrate",test))]
const AI_MESSAGES_MAXIMUM: usize = 64;
#[cfg(any(feature="hydrate",test))]
const AI_MESSAGE_MAXIMUM_BYTES: usize = 256 * 1024;
#[cfg(any(feature="hydrate",test))]
const AI_PROMPT_MAXIMUM_BYTES: usize = 256 * 1024;
#[cfg(any(feature="hydrate",test))]
const AI_REQUEST_MAXIMUM_BYTES: usize = 512 * 1024;
#[cfg(any(feature="hydrate",test))]
const AI_RESPONSE_MAXIMUM_BYTES: usize = 2 * 1024 * 1024;
#[cfg(any(feature="hydrate",test))]
const AI_MODEL_INSTALL_STREAM_MAXIMUM_BYTES: usize = 8 * 1024 * 1024;
#[cfg(any(feature="hydrate",test))]
const AI_RESPONSE_TEXT_MAXIMUM_BYTES: usize = 128 * 1024;
#[cfg(any(feature="hydrate",test))]
const AI_MODELS_MAXIMUM: usize = 1_024;
#[cfg(any(feature="hydrate",test))]
const AI_REQUEST_TIMEOUT_MS: u32 = 180_000;
#[cfg(any(feature="hydrate",test))]
const AI_MODEL_INSTALL_TIMEOUT_MS: u32 = 30 * 60 * 1_000;
#[cfg(any(feature="hydrate",test))]
const AI_OLLAMA_SERVER_TIMEOUT_MS: u32 = 10_000;
#[cfg(any(feature="hydrate",test))]
const AI_STREAM_LINE_MAXIMUM_BYTES: usize = 16 * 1024;
#[cfg(any(feature="hydrate",test))]
const AI_STREAM_EVENTS_MAXIMUM: usize = 65_536;

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
#[cfg(any(feature="hydrate",test))]
enum AiRequestOperation
{
	Completion,
	ModelList,
	OllamaServerTest,
	ModelInstall,
}

#[cfg(any(feature="hydrate",test))]
impl AiRequestOperation
{
	fn timeoutMs_get(self) -> u32
	{
		return match self
		{
			Self::ModelInstall => AI_MODEL_INSTALL_TIMEOUT_MS,
			Self::OllamaServerTest => AI_OLLAMA_SERVER_TIMEOUT_MS,
			Self::Completion | Self::ModelList => AI_REQUEST_TIMEOUT_MS,
		};
	}

	fn timeoutError_get(self) -> AiTransportError
	{
		return match self
		{
			Self::ModelInstall => AiTransportError::ModelInstallTimeout,
			Self::OllamaServerTest => AiTransportError::OllamaServerTimeout,
			Self::Completion | Self::ModelList => AiTransportError::Timeout,
		};
	}

	fn responseMaximumBytes_get(self) -> usize
	{
		if (self == Self::ModelInstall)
		{
			return AI_MODEL_INSTALL_STREAM_MAXIMUM_BYTES;
		}
		return AI_RESPONSE_MAXIMUM_BYTES;
	}
}

#[derive(Clone,Copy,Debug,Default,Eq,PartialEq)]
#[cfg_attr(not(feature="hydrate"),allow(dead_code))]
pub(crate) enum AiModelInstallProgress
{
	#[default]
	Indeterminate,
	Determinate(u8),
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
#[cfg_attr(not(feature="hydrate"),allow(dead_code))]
pub(crate) enum AiMessageRole
{
	System,
	User,
	Assistant,
}

#[derive(Clone,Eq,PartialEq)]
pub(crate) struct AiMessage
{
	pub(crate) role: AiMessageRole,
	pub(crate) content: String,
}

impl Debug for AiMessage
{
	fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result
	{
		return formatter.debug_struct("AiMessage")
			.field("role",&self.role)
			.field("contentBytes",&self.content.len())
			.finish();
	}
}

impl AiMessage
{
	pub(crate) fn system(content: impl Into<String>) -> Self
	{
		return Self {role: AiMessageRole::System,content: content.into()};
	}

	pub(crate) fn user(content: impl Into<String>) -> Self
	{
		return Self {role: AiMessageRole::User,content: content.into()};
	}
}

#[derive(Clone,Eq,PartialEq)]
pub(crate) struct AiCompletionRequest
{
	pub(crate) messages: Vec<AiMessage>,
	pub(crate) maxOutputTokens: u32,
	pub(crate) responseJsonSchema: Option<Value>,
}

impl Debug for AiCompletionRequest
{
	fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result
	{
		let promptBytes = self.messages.iter()
			.fold(0usize,|total,message| total.saturating_add(message.content.len()));
		return formatter.debug_struct("AiCompletionRequest")
			.field("messageCount",&self.messages.len())
			.field("promptBytes",&promptBytes)
			.field("maxOutputTokens",&self.maxOutputTokens)
			.field("structuredOutput",&self.responseJsonSchema.is_some())
			.finish();
	}
}

impl AiCompletionRequest
{
	pub(crate) fn connectionTest_get() -> Self
	{
		return Self {
			messages: vec![AiMessage::user("Reply with OK.")],
			maxOutputTokens: AI_OUTPUT_TOKENS_MINIMUM,
			responseJsonSchema: None,
		};
	}

	#[cfg(any(feature="hydrate",test))]
	fn validate(&self) -> Result<(),AiTransportError>
	{
		if (self.messages.is_empty() || self.messages.len() > AI_MESSAGES_MAXIMUM)
		{
			return Err(AiTransportError::InvalidRequest);
		}
		let mut totalBytes = 0usize;
		for message in &self.messages
		{
			if (message.content.is_empty() || message.content.len() > AI_MESSAGE_MAXIMUM_BYTES)
			{
				return Err(AiTransportError::InvalidRequest);
			}
			totalBytes = totalBytes.saturating_add(message.content.len());
		}
		if (totalBytes > AI_PROMPT_MAXIMUM_BYTES
			|| !(super::AI_OUTPUT_TOKENS_MINIMUM..=super::AI_OUTPUT_TOKENS_MAXIMUM).contains(&self.maxOutputTokens))
		{
			return Err(AiTransportError::InvalidRequest);
		}
		if let Some(schema) = &self.responseJsonSchema
		{
			let schemaBytes = serde_json::to_vec(schema).map_err(|_| AiTransportError::InvalidRequest)?;
			if (!schema.is_object() || schemaBytes.len() > AI_PROMPT_MAXIMUM_BYTES)
			{
				return Err(AiTransportError::InvalidRequest);
			}
		}
		return Ok(());
	}
}

#[derive(Clone,Eq,PartialEq)]
pub(crate) struct AiCompletionResponse
{
	pub(crate) text: String,
}

impl Debug for AiCompletionResponse
{
	fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result
	{
		return formatter.debug_struct("AiCompletionResponse")
			.field("textBytes",&self.text.len())
			.finish();
	}
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) struct AiAvailableModel
{
	id: String,
	created: u64,
}

impl AiAvailableModel
{
	pub(crate) fn id_get(&self) -> &str
	{
		return &self.id;
	}
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
#[cfg_attr(not(feature="hydrate"),allow(dead_code))]
pub(crate) enum AiTransportError
{
	Configuration(AiConfigError),
	InvalidRequest,
	Busy,
	InsecureTransport,
	WebHomeOriginForbidden,
	OriginNotAllowed,
	Transport,
	Timeout,
	Unauthorized,
	RateLimited,
	ModelUnavailable,
	OllamaModelUnavailable,
	OllamaResponseWithoutText,
	OllamaServerUnavailable,
	OllamaServerTimeout,
	OllamaModelInstallUnavailable,
	ModelInstallTimeout,
	ProviderRejected,
	ProviderFailure,
	ResponseTooLarge,
	InvalidResponse,
}

impl AiTransportError
{
	pub(crate) fn translateKey_get(self) -> &'static str
	{
		return match self
		{
			Self::Configuration(error) => error.translateKey_get(),
			Self::InvalidRequest => "FRONTAI_TEST_REQUEST_INVALID",
			Self::Busy => "FRONTAI_TEST_BUSY",
			Self::InsecureTransport => "FRONTAI_TEST_INSECURE_TRANSPORT",
			Self::WebHomeOriginForbidden => "FRONTAI_TEST_WEBHOME_ORIGIN_FORBIDDEN",
			Self::OriginNotAllowed => "FRONTAI_TEST_ORIGIN_NOT_ALLOWED",
			Self::Transport => "FRONTAI_TEST_TRANSPORT",
			Self::Timeout => "FRONTAI_TEST_TIMEOUT",
			Self::Unauthorized => "FRONTAI_TEST_UNAUTHORIZED",
			Self::RateLimited => "FRONTAI_TEST_RATE_LIMITED",
			Self::ModelUnavailable => "FRONTAI_TEST_MODEL_UNAVAILABLE",
			Self::OllamaModelUnavailable => "FRONTAI_TEST_OLLAMA_MODEL_UNAVAILABLE",
			Self::OllamaResponseWithoutText => "FRONTAI_TEST_OLLAMA_RESPONSE_WITHOUT_TEXT",
			Self::OllamaServerUnavailable => "FRONTAI_OLLAMA_SERVER_UNAVAILABLE",
			Self::OllamaServerTimeout => "FRONTAI_OLLAMA_SERVER_TIMEOUT",
			Self::OllamaModelInstallUnavailable => "FRONTAI_MODEL_INSTALL_UNAVAILABLE",
			Self::ModelInstallTimeout => "FRONTAI_MODEL_INSTALL_TIMEOUT",
			Self::ProviderRejected => "FRONTAI_TEST_REJECTED",
			Self::ProviderFailure => "FRONTAI_TEST_PROVIDER_FAILURE",
			Self::ResponseTooLarge => "FRONTAI_TEST_RESPONSE_TOO_LARGE",
			Self::InvalidResponse => "FRONTAI_TEST_RESPONSE_INVALID",
		};
	}

	pub(crate) fn modelListTranslateKey_get(self) -> &'static str
	{
		return match self
		{
			Self::Unauthorized => "FRONTAI_MODELS_UNAUTHORIZED",
			Self::ModelUnavailable | Self::ProviderRejected => "FRONTAI_MODELS_UNAVAILABLE",
			Self::InvalidResponse => "FRONTAI_MODELS_RESPONSE_INVALID",
			_ => self.translateKey_get(),
		};
	}

	pub(crate) fn modelInstallTranslateKey_get(self) -> &'static str
	{
		return match self
		{
			Self::InvalidRequest => "FRONTAI_MODEL_INSTALL_REQUEST_INVALID",
			Self::ProviderRejected => "FRONTAI_MODEL_INSTALL_REJECTED",
			Self::ProviderFailure => "FRONTAI_MODEL_INSTALL_FAILURE",
			Self::InvalidResponse => "FRONTAI_MODEL_INSTALL_RESPONSE_INVALID",
			_ => self.translateKey_get(),
		};
	}

	pub(crate) fn ollamaServerTranslateKey_get(self) -> &'static str
	{
		return match self
		{
			Self::InvalidRequest => "FRONTAI_OLLAMA_SERVER_REQUEST_INVALID",
			Self::ProviderRejected => "FRONTAI_OLLAMA_SERVER_REJECTED",
			Self::ProviderFailure => "FRONTAI_OLLAMA_SERVER_FAILURE",
			Self::InvalidResponse => "FRONTAI_OLLAMA_SERVER_RESPONSE_INVALID",
			_ => self.translateKey_get(),
		};
	}

	pub(crate) fn chatTranslateKey_get(self) -> &'static str
	{
		return match self
		{
			Self::Configuration(error) => error.translateKey_get(),
			Self::InvalidRequest => "MODULE_CHAT_ERROR_REQUEST_INVALID",
			Self::Busy => "MODULE_CHAT_ERROR_BUSY",
			Self::InsecureTransport => "MODULE_CHAT_ERROR_INSECURE_TRANSPORT",
			Self::WebHomeOriginForbidden => "MODULE_CHAT_ERROR_WEBHOME_ORIGIN",
			Self::OriginNotAllowed => "MODULE_CHAT_ERROR_ORIGIN_NOT_ALLOWED",
			Self::Transport | Self::OllamaServerUnavailable => "MODULE_CHAT_ERROR_TRANSPORT",
			Self::Timeout | Self::OllamaServerTimeout | Self::ModelInstallTimeout => "MODULE_CHAT_ERROR_TIMEOUT",
			Self::Unauthorized => "MODULE_CHAT_ERROR_UNAUTHORIZED",
			Self::RateLimited => "MODULE_CHAT_ERROR_RATE_LIMITED",
			Self::ModelUnavailable | Self::OllamaModelUnavailable => "MODULE_CHAT_ERROR_MODEL_UNAVAILABLE",
			Self::OllamaResponseWithoutText => "MODULE_CHAT_ERROR_OLLAMA_WITHOUT_TEXT",
			Self::ResponseTooLarge => "MODULE_CHAT_ERROR_RESPONSE_TOO_LARGE",
			Self::ProviderRejected | Self::ProviderFailure | Self::OllamaModelInstallUnavailable => "MODULE_CHAT_ERROR_PROVIDER",
			Self::InvalidResponse => "MODULE_CHAT_ERROR_RESPONSE_INVALID",
		};
	}
}

impl From<AiConfigError> for AiTransportError
{
	fn from(error: AiConfigError) -> Self
	{
		return Self::Configuration(error);
	}
}

pub(crate) struct AiProviderClient;

impl AiProviderClient
{
	#[cfg(feature="hydrate")]
	pub(crate) async fn modelList_get(profile: &AiProfile) -> Result<Vec<AiAvailableModel>,AiTransportError>
	{
		return browser::modelList_get(profile).await;
	}

	#[cfg(not(feature="hydrate"))]
	pub(crate) async fn modelList_get(_: &AiProfile) -> Result<Vec<AiAvailableModel>,AiTransportError>
	{
		return Err(AiTransportError::Transport);
	}

	#[cfg(feature="hydrate")]
	pub(crate) async fn modelInstall(profile: &AiProfile, allowedOrigins: &AiAllowedOrigins, onProgress: Rc<dyn Fn(AiModelInstallProgress)>) -> Result<(),AiTransportError>
	{
		return browser::modelInstall(profile,allowedOrigins,onProgress).await;
	}

	#[cfg(not(feature="hydrate"))]
	pub(crate) async fn modelInstall(_: &AiProfile, _: &AiAllowedOrigins, _: Rc<dyn Fn(AiModelInstallProgress)>) -> Result<(),AiTransportError>
	{
		return Err(AiTransportError::Transport);
	}

	#[cfg(feature="hydrate")]
	pub(crate) async fn ollamaServerTest(profile: &AiProfile, allowedOrigins: &AiAllowedOrigins) -> Result<(),AiTransportError>
	{
		return browser::ollamaServerTest(profile,allowedOrigins).await;
	}

	#[cfg(not(feature="hydrate"))]
	pub(crate) async fn ollamaServerTest(_: &AiProfile, _: &AiAllowedOrigins) -> Result<(),AiTransportError>
	{
		return Err(AiTransportError::Transport);
	}

	#[cfg(feature="hydrate")]
	pub(crate) async fn complete(profile: &AiProfile, request: &AiCompletionRequest, allowedOrigins: &AiAllowedOrigins) -> Result<AiCompletionResponse,AiTransportError>
	{
		return browser::complete(profile,request,allowedOrigins).await;
	}

	#[cfg(not(feature="hydrate"))]
	pub(crate) async fn complete(_: &AiProfile, _: &AiCompletionRequest, _: &AiAllowedOrigins) -> Result<AiCompletionResponse,AiTransportError>
	{
		return Err(AiTransportError::Transport);
	}
}

#[cfg(any(feature="hydrate",test))]
fn endpoint_get(profile: &AiProfile) -> Result<Url,AiTransportError>
{
	profile.validate()?;
	return match profile.provider
	{
		AiProvider::OpenAI => Url::parse("https://api.openai.com/v1/responses").map_err(|_| AiTransportError::InvalidRequest),
		AiProvider::Anthropic => Url::parse("https://api.anthropic.com/v1/messages").map_err(|_| AiTransportError::InvalidRequest),
		AiProvider::Gemini => {
			let model = profile.model.strip_prefix("models/").unwrap_or(&profile.model);
			if (model.is_empty() || model.contains('/'))
			{
				return Err(AiTransportError::Configuration(AiConfigError::InvalidModel));
			}
			let mut url = Url::parse("https://generativelanguage.googleapis.com/v1beta/models/")
				.map_err(|_| AiTransportError::InvalidRequest)?;
			url.path_segments_mut().map_err(|_| AiTransportError::InvalidRequest)?
				.push(&format!("{model}:generateContent"));
			Ok(url)
		},
		AiProvider::Mistral => Url::parse("https://api.mistral.ai/v1/chat/completions").map_err(|_| AiTransportError::InvalidRequest),
		AiProvider::Ollama => {
			let mut url = AiProfile::baseUrl_validate(&profile.baseUrl)?;
			url.set_path("/api/chat");
			Ok(url)
		},
	};
}

#[cfg(any(feature="hydrate",test))]
fn modelListEndpoint_get(profile: &AiProfile) -> Result<Url,AiTransportError>
{
	profile.connection_validate()?;
	return match profile.provider
	{
		AiProvider::OpenAI => Url::parse("https://api.openai.com/v1/models").map_err(|_| AiTransportError::InvalidRequest),
		_ => Err(AiTransportError::InvalidRequest),
	};
}

#[cfg(any(feature="hydrate",test))]
fn modelInstallEndpoint_get(profile: &AiProfile) -> Result<Url,AiTransportError>
{
	if (profile.provider != AiProvider::Ollama)
	{
		return Err(AiTransportError::InvalidRequest);
	}
	profile.connection_validate()?;
	if (!AiProfile::model_isValid(&profile.model))
	{
		return Err(AiTransportError::Configuration(AiConfigError::InvalidModel));
	}
	let mut url = AiProfile::baseUrl_validate(&profile.baseUrl)?;
	url.set_path("/api/pull");
	return Ok(url);
}

#[cfg(any(feature="hydrate",test))]
fn ollamaServerEndpoint_get(profile: &AiProfile) -> Result<Url,AiTransportError>
{
	if (profile.provider != AiProvider::Ollama)
	{
		return Err(AiTransportError::InvalidRequest);
	}
	profile.connection_validate()?;
	let mut url = AiProfile::baseUrl_validate(&profile.baseUrl)?;
	url.set_path("/api/version");
	return Ok(url);
}

#[cfg(any(feature="hydrate",test))]
fn endpoint_isWebHomeOrigin(endpoint: &Url, webHomeOrigin: &str) -> bool
{
	return Url::parse(webHomeOrigin).ok()
		.map(|origin| origin.origin() == endpoint.origin())
		.unwrap_or(false);
}

#[cfg(any(feature="hydrate",test))]
fn request_body_get(profile: &AiProfile, request: &AiCompletionRequest) -> Result<String,AiTransportError>
{
	profile.validate()?;
	request.validate()?;
	let value = match profile.provider
	{
		AiProvider::OpenAI => json!({
			"model": profile.model,
			"input": request.messages.iter().map(|message| json!({
				"role": role_openAi_get(message.role),
				"content": message.content,
			})).collect::<Vec<_>>(),
			"max_output_tokens": request.maxOutputTokens,
			"store": false,
		}),
		AiProvider::Anthropic => {
			let system = systemText_get(&request.messages);
			let mut value = json!({
				"model": profile.model,
				"max_tokens": request.maxOutputTokens,
				"messages": chatMessages_get(&request.messages,false),
			});
			if (!system.is_empty())
			{
				value["system"] = Value::String(system);
			}
			value
		},
		AiProvider::Gemini => {
			let system = systemText_get(&request.messages);
			let mut value = json!({
				"contents": request.messages.iter()
					.filter(|message| message.role != AiMessageRole::System)
					.map(|message| json!({
						"role": if message.role == AiMessageRole::Assistant {"model"} else {"user"},
						"parts": [{"text": message.content}],
					})).collect::<Vec<_>>(),
				"generationConfig": {"maxOutputTokens": request.maxOutputTokens},
			});
			if (!system.is_empty())
			{
				value["systemInstruction"] = json!({"parts": [{"text": system}]});
			}
			value
		},
		AiProvider::Mistral => json!({
			"model": profile.model,
			"messages": chatMessages_get(&request.messages,true),
			"max_tokens": request.maxOutputTokens,
			"stream": false,
		}),
		AiProvider::Ollama => {
			let mut value = json!({
				"model": profile.model,
				"messages": chatMessages_get(&request.messages,true),
				"think": false,
				"stream": false,
				"options": {"num_predict": request.maxOutputTokens},
			});
			if let Some(schema) = &request.responseJsonSchema
			{
				value["format"] = schema.clone();
				value["options"]["temperature"] = json!(0);
			}
			value
		},
	};
	let body = serde_json::to_string(&value).map_err(|_| AiTransportError::InvalidRequest)?;
	if (body.len() > AI_REQUEST_MAXIMUM_BYTES)
	{
		return Err(AiTransportError::InvalidRequest);
	}
	return Ok(body);
}

#[cfg(any(feature="hydrate",test))]
fn modelInstallBody_get(profile: &AiProfile) -> Result<String,AiTransportError>
{
	modelInstallEndpoint_get(profile)?;
	let body = serde_json::to_string(&json!({
		"model": profile.model,
		"stream": true,
	})).map_err(|_| AiTransportError::InvalidRequest)?;
	if (body.len() > AI_REQUEST_MAXIMUM_BYTES)
	{
		return Err(AiTransportError::InvalidRequest);
	}
	return Ok(body);
}

#[cfg(any(feature="hydrate",test))]
fn chatMessages_get(messages: &[AiMessage], includeSystem: bool) -> Vec<Value>
{
	return messages.iter()
		.filter(|message| includeSystem || message.role != AiMessageRole::System)
		.map(|message| json!({
			"role": role_chat_get(message.role),
			"content": message.content,
		}))
		.collect();
}

#[cfg(any(feature="hydrate",test))]
fn systemText_get(messages: &[AiMessage]) -> String
{
	return messages.iter()
		.filter(|message| message.role == AiMessageRole::System)
		.map(|message| message.content.as_str())
		.collect::<Vec<_>>()
		.join("\n\n");
}

#[cfg(any(feature="hydrate",test))]
fn role_openAi_get(role: AiMessageRole) -> &'static str
{
	return match role
	{
		AiMessageRole::System => "developer",
		AiMessageRole::User => "user",
		AiMessageRole::Assistant => "assistant",
	};
}

#[cfg(any(feature="hydrate",test))]
fn role_chat_get(role: AiMessageRole) -> &'static str
{
	return match role
	{
		AiMessageRole::System => "system",
		AiMessageRole::User => "user",
		AiMessageRole::Assistant => "assistant",
	};
}

#[cfg(any(feature="hydrate",test))]
fn response_text_get(provider: AiProvider, body: &str) -> Result<String,AiTransportError>
{
	if (body.len() > AI_RESPONSE_MAXIMUM_BYTES)
	{
		return Err(AiTransportError::ResponseTooLarge);
	}
	let value = serde_json::from_str::<Value>(body).map_err(|_| AiTransportError::InvalidResponse)?;
	let fragments = match provider
	{
		AiProvider::OpenAI => value.get("output").and_then(Value::as_array).into_iter().flatten()
			.flat_map(|item| item.get("content").and_then(Value::as_array).into_iter().flatten())
			.filter(|content| content.get("type").and_then(Value::as_str) == Some("output_text"))
			.filter_map(|content| content.get("text").and_then(Value::as_str))
			.collect::<Vec<_>>(),
		AiProvider::Anthropic => value.get("content").and_then(Value::as_array).into_iter().flatten()
			.filter(|content| content.get("type").and_then(Value::as_str) == Some("text"))
			.filter_map(|content| content.get("text").and_then(Value::as_str))
			.collect::<Vec<_>>(),
		AiProvider::Gemini => value.get("candidates").and_then(Value::as_array).into_iter().flatten()
			.flat_map(|candidate| candidate.get("content").and_then(|content| content.get("parts")).and_then(Value::as_array).into_iter().flatten())
			.filter_map(|part| part.get("text").and_then(Value::as_str))
			.collect::<Vec<_>>(),
		AiProvider::Mistral => value.get("choices").and_then(Value::as_array).into_iter().flatten()
			.filter_map(|choice| choice.get("message").and_then(|message| message.get("content")).and_then(Value::as_str))
			.collect::<Vec<_>>(),
		AiProvider::Ollama => {
			let message = value.get("message").ok_or(AiTransportError::InvalidResponse)?;
			let content = message.get("content").and_then(Value::as_str).ok_or(AiTransportError::InvalidResponse)?;
			let thinkingOnly = content.is_empty()
				&& message.get("thinking").and_then(Value::as_str).map(|thinking| !thinking.is_empty()).unwrap_or(false);
			let outputLimitReached = value.get("done_reason").and_then(Value::as_str) == Some("length");
			if (thinkingOnly || outputLimitReached)
			{
				return Err(AiTransportError::OllamaResponseWithoutText);
			}
			vec![content]
		},
	};
	let text = fragments.concat();
	if (text.is_empty())
	{
		return Err(AiTransportError::InvalidResponse);
	}
	if (text.len() > AI_RESPONSE_TEXT_MAXIMUM_BYTES)
	{
		return Err(AiTransportError::ResponseTooLarge);
	}
	return Ok(text);
}

#[cfg(any(feature="hydrate",test))]
fn modelList_responseGet(provider: AiProvider, body: &str) -> Result<Vec<AiAvailableModel>,AiTransportError>
{
	if (body.len() > AI_RESPONSE_MAXIMUM_BYTES)
	{
		return Err(AiTransportError::ResponseTooLarge);
	}
	let value = serde_json::from_str::<Value>(body).map_err(|_| AiTransportError::InvalidResponse)?;
	let entries = match provider
	{
		AiProvider::OpenAI => value.get("data").and_then(Value::as_array),
		_ => None,
	}.ok_or(AiTransportError::InvalidResponse)?;
	let mut modelsById = BTreeMap::<String,u64>::new();
	for entry in entries
	{
		let Some(id) = entry.get("id").and_then(Value::as_str) else {continue;};
		let Some(created) = entry.get("created").and_then(Value::as_u64) else {continue;};
		if (!AiProfile::model_isValid(id)) {continue;}
		modelsById.entry(id.to_string())
			.and_modify(|knownCreated| *knownCreated = (*knownCreated).max(created))
			.or_insert(created);
	}
	let mut models = modelsById.into_iter()
		.map(|(id,created)| AiAvailableModel {id,created})
		.collect::<Vec<_>>();
	models.sort_unstable_by(|left,right| {
		return right.created.cmp(&left.created)
			.then_with(|| left.id.cmp(&right.id));
	});
	models.truncate(AI_MODELS_MAXIMUM);
	if (models.is_empty())
	{
		return Err(AiTransportError::InvalidResponse);
	}
	return Ok(models);
}

#[cfg(any(feature="hydrate",test))]
#[derive(Default)]
struct AiModelInstallStream
{
	buffer: Vec<u8>,
	totalBytes: usize,
	eventCount: usize,
	success: bool,
	lastProgress: Option<AiModelInstallProgress>,
}

#[cfg(any(feature="hydrate",test))]
impl AiModelInstallStream
{
	fn chunk_push(&mut self,chunk: &[u8],onProgress: &dyn Fn(AiModelInstallProgress)) -> Result<(),AiTransportError>
	{
		self.totalBytes = self.totalBytes.checked_add(chunk.len()).ok_or(AiTransportError::ResponseTooLarge)?;
		if (self.totalBytes > AI_MODEL_INSTALL_STREAM_MAXIMUM_BYTES)
		{
			return Err(AiTransportError::ResponseTooLarge);
		}
		self.buffer.extend_from_slice(chunk);
		while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n')
		{
			let mut line = self.buffer.drain(..=end).collect::<Vec<_>>();
			line.pop();
			self.event_read(&line,onProgress)?;
		}
		if (self.buffer.len() > AI_STREAM_LINE_MAXIMUM_BYTES)
		{
			return Err(AiTransportError::ResponseTooLarge);
		}
		return Ok(());
	}

	fn finish(&mut self,onProgress: &dyn Fn(AiModelInstallProgress)) -> Result<(),AiTransportError>
	{
		if (!self.buffer.is_empty())
		{
			let line = std::mem::take(&mut self.buffer);
			self.event_read(&line,onProgress)?;
		}
		if (!self.success)
		{
			return Err(AiTransportError::InvalidResponse);
		}
		return Ok(());
	}

	fn event_read(&mut self,line: &[u8],onProgress: &dyn Fn(AiModelInstallProgress)) -> Result<(),AiTransportError>
	{
		if (line.len() > AI_STREAM_LINE_MAXIMUM_BYTES)
		{
			return Err(AiTransportError::ResponseTooLarge);
		}
		let line = std::str::from_utf8(line).map_err(|_| AiTransportError::InvalidResponse)?.trim();
		if (line.is_empty())
		{
			return Ok(());
		}
		self.eventCount = self.eventCount.checked_add(1).ok_or(AiTransportError::ResponseTooLarge)?;
		if (self.eventCount > AI_STREAM_EVENTS_MAXIMUM || self.success)
		{
			return Err(AiTransportError::InvalidResponse);
		}
		let value = serde_json::from_str::<Value>(line).map_err(|_| AiTransportError::InvalidResponse)?;
		if (value.get("error").is_some())
		{
			return Err(AiTransportError::ProviderFailure);
		}
		let status = value.get("status").and_then(Value::as_str).ok_or(AiTransportError::InvalidResponse)?;
		if (status.is_empty() || status.len() > 256 || status.chars().any(char::is_control))
		{
			return Err(AiTransportError::InvalidResponse);
		}
		if (status == "success")
		{
			self.success = true;
			self.progress_set(AiModelInstallProgress::Determinate(100),onProgress);
			return Ok(());
		}

		let total = match value.get("total")
		{
			Some(value) => Some(value.as_u64().ok_or(AiTransportError::InvalidResponse)?),
			None => None,
		};
		let completed = match value.get("completed")
		{
			Some(value) => Some(value.as_u64().ok_or(AiTransportError::InvalidResponse)?),
			None => None,
		};
		let progress = match (total,completed)
		{
			(Some(total),completed) if total > 0 => {
				let completed = completed.unwrap_or(0).min(total);
				let percentage = ((completed as u128 * 100) / total as u128) as u8;
				AiModelInstallProgress::Determinate(percentage)
			},
			(None,None) | (Some(0),None | Some(0)) => AiModelInstallProgress::Indeterminate,
			_ => return Err(AiTransportError::InvalidResponse),
		};
		self.progress_set(progress,onProgress);
		return Ok(());
	}

	fn progress_set(&mut self,progress: AiModelInstallProgress,onProgress: &dyn Fn(AiModelInstallProgress))
	{
		if (self.lastProgress != Some(progress))
		{
			self.lastProgress = Some(progress);
			onProgress(progress);
		}
	}
}

#[cfg(any(feature="hydrate",test))]
fn ollamaServerResponse_validate(body: &str) -> Result<(),AiTransportError>
{
	if (body.len() > AI_RESPONSE_MAXIMUM_BYTES)
	{
		return Err(AiTransportError::ResponseTooLarge);
	}
	let value = serde_json::from_str::<Value>(body).map_err(|_| AiTransportError::InvalidResponse)?;
	let version = value.get("version").and_then(Value::as_str).ok_or(AiTransportError::InvalidResponse)?;
	if (version.is_empty() || version.len() > 128 || version.trim() != version || version.chars().any(char::is_control))
	{
		return Err(AiTransportError::InvalidResponse);
	}
	return Ok(());
}

#[cfg(any(feature="hydrate",test))]
fn status_error_get(provider: AiProvider,status: u16,operation: AiRequestOperation) -> AiTransportError
{
	return match status
	{
		401 | 403 => AiTransportError::Unauthorized,
		404 if operation == AiRequestOperation::ModelInstall => AiTransportError::OllamaModelInstallUnavailable,
		404 if operation == AiRequestOperation::OllamaServerTest => AiTransportError::OllamaServerUnavailable,
		404 if provider == AiProvider::Ollama && operation == AiRequestOperation::Completion => AiTransportError::OllamaModelUnavailable,
		404 => AiTransportError::ModelUnavailable,
		408 | 504 => operation.timeoutError_get(),
		429 => AiTransportError::RateLimited,
		400..=499 => AiTransportError::ProviderRejected,
		_ => AiTransportError::ProviderFailure,
	};
}

#[cfg(feature="hydrate")]
mod browser
{
	use super::*;
	use gloo_timers::callback::Timeout;
	use js_sys::Uint8Array;
	use leptos::prelude::Owner;
	use std::cell::Cell;
	use wasm_bindgen::{JsCast,JsValue};
	use wasm_bindgen_futures::JsFuture;
	use web_sys::{
		AbortController,Headers,ReadableStreamDefaultReader,ReadableStreamReadResult,ReferrerPolicy,
		Request,RequestCredentials,RequestInit,RequestMode,RequestRedirect,Response,
	};

	const AI_CONCURRENT_REQUESTS_MAXIMUM: u8 = 2;

	thread_local! {
		static ACTIVE_REQUESTS: Cell<u8> = const { Cell::new(0) };
	}

	struct RequestPermit;

	impl RequestPermit
	{
		fn acquire() -> Result<Self,AiTransportError>
		{
			return ACTIVE_REQUESTS.with(|active| {
				let value = active.get();
				if (value >= AI_CONCURRENT_REQUESTS_MAXIMUM)
				{
					return Err(AiTransportError::Busy);
				}
				active.set(value + 1);
				Ok(Self)
			});
		}
	}

	impl Drop for RequestPermit
	{
		fn drop(&mut self)
		{
			ACTIVE_REQUESTS.with(|active| active.set(active.get().saturating_sub(1)));
		}
	}

	struct ActiveResponse
	{
		response: Response,
		didTimeout: Rc<Cell<bool>>,
		operation: AiRequestOperation,
		_permit: RequestPermit,
		_timeout: Timeout,
	}

	impl ActiveResponse
	{
		fn readError_get(&self) -> AiTransportError
		{
			if (self.didTimeout.get())
			{
				return self.operation.timeoutError_get();
			}
			return AiTransportError::Transport;
		}
	}

	pub(super) async fn complete(profile: &AiProfile, request: &AiCompletionRequest, allowedOrigins: &AiAllowedOrigins) -> Result<AiCompletionResponse,AiTransportError>
	{
		let endpoint = endpoint_get(profile)?;
		if (profile.provider == AiProvider::Ollama && !allowedOrigins.endpoint_isAllowed(&endpoint))
		{
			return Err(AiTransportError::OriginNotAllowed);
		}
		let body = request_body_get(profile,request)?;
		let responseBody = request_send(profile,&endpoint,"POST",Some(&body),AiRequestOperation::Completion).await?;
		let text = response_text_get(profile.provider,&responseBody)?;
		return Ok(AiCompletionResponse {text});
	}

	pub(super) async fn modelList_get(profile: &AiProfile) -> Result<Vec<AiAvailableModel>,AiTransportError>
	{
		let endpoint = modelListEndpoint_get(profile)?;
		let responseBody = request_send(profile,&endpoint,"GET",None,AiRequestOperation::ModelList).await?;
		return modelList_responseGet(profile.provider,&responseBody);
	}

	pub(super) async fn ollamaServerTest(profile: &AiProfile, allowedOrigins: &AiAllowedOrigins) -> Result<(),AiTransportError>
	{
		let endpoint = ollamaServerEndpoint_get(profile)?;
		if (!allowedOrigins.endpoint_isAllowed(&endpoint))
		{
			return Err(AiTransportError::OriginNotAllowed);
		}
		let responseBody = request_send(profile,&endpoint,"GET",None,AiRequestOperation::OllamaServerTest).await?;
		return ollamaServerResponse_validate(&responseBody);
	}

	pub(super) async fn modelInstall(profile: &AiProfile, allowedOrigins: &AiAllowedOrigins, onProgress: Rc<dyn Fn(AiModelInstallProgress)>) -> Result<(),AiTransportError>
	{
		let endpoint = modelInstallEndpoint_get(profile)?;
		if (!allowedOrigins.endpoint_isAllowed(&endpoint))
		{
			return Err(AiTransportError::OriginNotAllowed);
		}
		let body = modelInstallBody_get(profile)?;
		let response = request_start(profile,&endpoint,"POST",Some(&body),AiRequestOperation::ModelInstall).await?;
		return modelInstallResponse_read(&response,onProgress.as_ref()).await;
	}

	async fn request_send(profile: &AiProfile, endpoint: &Url, method: &str, body: Option<&str>, operation: AiRequestOperation) -> Result<String,AiTransportError>
	{
		let response = request_start(profile,endpoint,method,body,operation).await?;
		return response_body_read(&response).await;
	}

	async fn request_start(profile: &AiProfile, endpoint: &Url, method: &str, body: Option<&str>, operation: AiRequestOperation) -> Result<ActiveResponse,AiTransportError>
	{
		let permit = RequestPermit::acquire()?;
		let window = web_sys::window().ok_or(AiTransportError::Transport)?;
		let pageOrigin = window.location().origin().map_err(|_| AiTransportError::Transport)?;
		if (endpoint_isWebHomeOrigin(endpoint,&pageOrigin))
		{
			return Err(AiTransportError::WebHomeOriginForbidden);
		}
		if (endpoint.scheme() == "http")
		{
			let pageProtocol = window.location().protocol().map_err(|_| AiTransportError::InsecureTransport)?;
			if (pageProtocol != "http:")
			{
				return Err(AiTransportError::InsecureTransport);
			}
		}
		let headers = Headers::new().map_err(|_| AiTransportError::Transport)?;
		let accept = if (operation == AiRequestOperation::ModelInstall)
		{
			"application/x-ndjson, application/json"
		}
		else
		{
			"application/json"
		};
		headers.set("Accept",accept).map_err(|_| AiTransportError::Transport)?;
		if (body.is_some())
		{
			headers.set("Content-Type","application/json").map_err(|_| AiTransportError::Transport)?;
		}
		match profile.provider
		{
			AiProvider::OpenAI | AiProvider::Mistral => {
				headers.set("Authorization",&format!("Bearer {}",profile.credential)).map_err(|_| AiTransportError::Configuration(AiConfigError::InvalidCredential))?;
			},
			AiProvider::Anthropic => {
				headers.set("x-api-key",&profile.credential).map_err(|_| AiTransportError::Configuration(AiConfigError::InvalidCredential))?;
				headers.set("anthropic-version","2023-06-01").map_err(|_| AiTransportError::Transport)?;
				headers.set("anthropic-dangerous-direct-browser-access","true").map_err(|_| AiTransportError::Transport)?;
			},
			AiProvider::Gemini => {
				headers.set("x-goog-api-key",&profile.credential).map_err(|_| AiTransportError::Configuration(AiConfigError::InvalidCredential))?;
			},
			AiProvider::Ollama if !profile.credential.is_empty() => {
				headers.set("Authorization",&format!("Bearer {}",profile.credential)).map_err(|_| AiTransportError::Configuration(AiConfigError::InvalidCredential))?;
			},
			AiProvider::Ollama => {},
		}

		let controller = AbortController::new().map_err(|_| AiTransportError::Transport)?;
		let cleanupController = controller.clone();
		Owner::on_cleanup(move || cleanupController.abort());
		let requestInit = RequestInit::new();
		requestInit.set_method(method);
		requestInit.set_headers_headers(&headers);
		requestInit.set_credentials(RequestCredentials::Omit);
		requestInit.set_mode(RequestMode::Cors);
		requestInit.set_redirect(RequestRedirect::Error);
		requestInit.set_referrer_policy(ReferrerPolicy::NoReferrer);
		requestInit.set_signal(Some(&controller.signal()));
		if let Some(body) = body
		{
			requestInit.set_body(&JsValue::from_str(body));
		}
		let request = Request::new_with_str_and_init(endpoint.as_str(),&requestInit).map_err(|_| AiTransportError::Transport)?;

		let didTimeout = Rc::new(Cell::new(false));
		let didTimeoutInner = didTimeout.clone();
		let abortController = controller.clone();
		let timeout = Timeout::new(operation.timeoutMs_get(),move || {
			didTimeoutInner.set(true);
			abortController.abort();
		});
		let response = window.fetch_with_request(&request);
		let response = JsFuture::from(response).await.map_err(|_| {
			if (didTimeout.get()) {operation.timeoutError_get()} else {AiTransportError::Transport}
		})?;
		let response: Response = response.dyn_into().map_err(|_| AiTransportError::Transport)?;
		if (!(200..300).contains(&response.status()))
		{
			return Err(status_error_get(profile.provider,response.status(),operation));
		}
		if let Some(contentLength) = response.headers().get("Content-Length").map_err(|_| AiTransportError::InvalidResponse)?
			&& contentLength.parse::<usize>().ok().map(|length| length > operation.responseMaximumBytes_get()).unwrap_or(false)
		{
			return Err(AiTransportError::ResponseTooLarge);
		}
		return Ok(ActiveResponse {
			response,
			didTimeout,
			operation,
			_permit: permit,
			_timeout: timeout,
		});
	}

	async fn response_body_read(activeResponse: &ActiveResponse) -> Result<String,AiTransportError>
	{
		let Some(stream) = activeResponse.response.body() else {return Err(AiTransportError::InvalidResponse);};
		let reader = ReadableStreamDefaultReader::new(&stream).map_err(|_| AiTransportError::Transport)?;
		let mut bytes = Vec::new();
		loop
		{
			let result = JsFuture::from(reader.read()).await.map_err(|_| activeResponse.readError_get())?;
			let result: ReadableStreamReadResult = result.unchecked_into();
			if (result.get_done().unwrap_or(false)) {break;}
			let chunk = Uint8Array::new(&result.get_value());
			let chunkLength = chunk.length() as usize;
			if (bytes.len().saturating_add(chunkLength) > AI_RESPONSE_MAXIMUM_BYTES)
			{
				let _ = reader.cancel();
				return Err(AiTransportError::ResponseTooLarge);
			}
			let start = bytes.len();
			bytes.resize(start + chunkLength,0);
			chunk.copy_to(&mut bytes[start..]);
		}
		reader.release_lock();
		return String::from_utf8(bytes).map_err(|_| AiTransportError::InvalidResponse);
	}

	async fn modelInstallResponse_read(activeResponse: &ActiveResponse,onProgress: &dyn Fn(AiModelInstallProgress)) -> Result<(),AiTransportError>
	{
		let Some(stream) = activeResponse.response.body() else {return Err(AiTransportError::InvalidResponse);};
		let reader = ReadableStreamDefaultReader::new(&stream).map_err(|_| AiTransportError::Transport)?;
		let mut response = AiModelInstallStream::default();
		loop
		{
			let result = JsFuture::from(reader.read()).await.map_err(|_| activeResponse.readError_get())?;
			let result: ReadableStreamReadResult = result.unchecked_into();
			if (result.get_done().unwrap_or(false)) {break;}
			let chunk = Uint8Array::new(&result.get_value());
			let mut bytes = vec![0;chunk.length() as usize];
			chunk.copy_to(&mut bytes);
			if let Err(error) = response.chunk_push(&bytes,onProgress)
			{
				let _ = reader.cancel();
				return Err(error);
			}
		}
		reader.release_lock();
		return response.finish(onProgress);
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	fn profile_get(provider: AiProvider) -> AiProfile
	{
		return AiProfile {
			provider,
			model: if provider == AiProvider::Gemini {"models/gemini-test".to_string()} else {"model-test".to_string()},
			credential: if provider == AiProvider::Ollama {String::new()} else {"secret".to_string()},
			baseUrl: if provider == AiProvider::Ollama {"http://127.0.0.1:11434".to_string()} else {String::new()},
			maxOutputTokens: 512,
		};
	}

	#[test]
	fn everyProviderBuildsBoundedNonStreamingRequest()
	{
		let request = AiCompletionRequest::connectionTest_get();
		for provider in AiProvider::ALL
		{
			let profile = profile_get(provider);
			let endpoint = endpoint_get(&profile).unwrap();
			let body = request_body_get(&profile,&request).unwrap();
			assert!(matches!(endpoint.scheme(),"http" | "https"));
			assert!(body.len() <= AI_REQUEST_MAXIMUM_BYTES);
			assert!(!body.contains("secret"));
			if (matches!(provider,AiProvider::Mistral | AiProvider::Ollama))
			{
				assert!(body.contains("\"stream\":false"));
			}
			if (provider == AiProvider::Ollama)
			{
				assert_eq!(serde_json::from_str::<Value>(&body).unwrap().get("think"),Some(&Value::Bool(false)));
			}
		}
	}

	#[test]
	fn providerResponsesNormalizeText()
	{
		let cases = [
			(AiProvider::OpenAI,r#"{"output":[{"content":[{"type":"output_text","text":"ok"}]}]}"#),
			(AiProvider::Anthropic,r#"{"content":[{"type":"text","text":"ok"}]}"#),
			(AiProvider::Gemini,r#"{"candidates":[{"content":{"parts":[{"text":"ok"}]}}]}"#),
			(AiProvider::Mistral,r#"{"choices":[{"message":{"content":"ok"}}]}"#),
			(AiProvider::Ollama,r#"{"message":{"content":"ok"},"done":true}"#),
		];
		for (provider,body) in cases
		{
			assert_eq!(response_text_get(provider,body).unwrap(),"ok");
		}
	}

	#[test]
	fn ollamaStructuredCompletionUsesJsonSchemaAndDeterministicSampling()
	{
		let profile = profile_get(AiProvider::Ollama);
		let mut request = AiCompletionRequest::connectionTest_get();
		let schema = json!({
			"type": "object",
			"additionalProperties": false,
			"properties": {"actions": {"type": "array"}},
			"required": ["actions"],
		});
		request.responseJsonSchema = Some(schema.clone());

		let body = serde_json::from_str::<Value>(&request_body_get(&profile,&request).unwrap()).unwrap();
		assert_eq!(body.get("format"),Some(&schema));
		assert_eq!(body.pointer("/options/temperature"),Some(&json!(0)));

		let ordinaryBody = serde_json::from_str::<Value>(
			&request_body_get(&profile,&AiCompletionRequest::connectionTest_get()).unwrap(),
		).unwrap();
		assert!(ordinaryBody.get("format").is_none());
		assert!(ordinaryBody.pointer("/options/temperature").is_none());
	}

	#[test]
	fn ollamaThinkingOrTruncatedResponseHasADedicatedError()
	{
		assert_eq!(
			response_text_get(AiProvider::Ollama,r#"{"message":{"content":"","thinking":"private reasoning"},"done":true,"done_reason":"length"}"#),
			Err(AiTransportError::OllamaResponseWithoutText),
		);
		assert_eq!(
			response_text_get(AiProvider::Ollama,r#"{"message":{"content":""},"done":true,"done_reason":"length"}"#),
			Err(AiTransportError::OllamaResponseWithoutText),
		);
		assert_eq!(
			response_text_get(AiProvider::Ollama,r#"{"message":{"content":""},"done":true,"done_reason":"stop"}"#),
			Err(AiTransportError::InvalidResponse),
		);
		assert_eq!(
			response_text_get(AiProvider::Ollama,r#"{"message":{"content":"{\"partial\":true}"},"done":true,"done_reason":"length"}"#),
			Err(AiTransportError::OllamaResponseWithoutText),
		);
	}

	#[test]
	fn openAiModelDiscoveryDoesNotRequireAPreselectedModel()
	{
		let mut profile = profile_get(AiProvider::OpenAI);
		profile.model.clear();

		assert_eq!(modelListEndpoint_get(&profile).unwrap().as_str(),"https://api.openai.com/v1/models");
		assert_eq!(endpoint_get(&profile),Err(AiTransportError::Configuration(AiConfigError::InvalidModel)));
	}

	#[test]
	fn openAiModelListIsNormalizedAndInvalidEntriesAreIgnored()
	{
		let models = modelList_responseGet(AiProvider::OpenAI,r#"{
			"data": [
				{"id": "gpt-old", "created": 100},
				{"id": "gpt-new-z", "created": 300},
				{"id": "gpt-new-a", "created": 300},
				{"id": "gpt-old", "created": 200},
				{"id": "invalid model", "created": 400},
				{"id": "missing-created"},
				{"missing": "id"}
			]
		}"#).unwrap();

		assert_eq!(models,vec![
			AiAvailableModel {id: "gpt-new-a".to_string(),created: 300},
			AiAvailableModel {id: "gpt-new-z".to_string(),created: 300},
			AiAvailableModel {id: "gpt-old".to_string(),created: 200},
		]);
		assert_eq!(modelList_responseGet(AiProvider::OpenAI,r#"{"data":[]}"#),Err(AiTransportError::InvalidResponse));
	}

	#[test]
	fn ollamaModelInstallBuildsExactBoundedRequest()
	{
		let mut profile = profile_get(AiProvider::Ollama);
		profile.model = "qwen3.5:2b".to_string();
		profile.credential = "secret-that-must-not-enter-the-body".to_string();

		assert_eq!(modelInstallEndpoint_get(&profile).unwrap().as_str(),"http://127.0.0.1:11434/api/pull");
		let body = modelInstallBody_get(&profile).unwrap();
		assert_eq!(serde_json::from_str::<Value>(&body).unwrap(),json!({
			"model": "qwen3.5:2b",
			"stream": true,
		}));
		assert!(!body.contains("secret-that-must-not-enter-the-body"));
		assert!(body.len() <= AI_REQUEST_MAXIMUM_BYTES);
	}

	#[test]
	fn modelInstallIsRestrictedToOllamaAndValidModels()
	{
		assert_eq!(modelInstallEndpoint_get(&profile_get(AiProvider::OpenAI)),Err(AiTransportError::InvalidRequest));
		let mut profile = profile_get(AiProvider::Ollama);
		profile.model = "invalid model".to_string();
		assert_eq!(
			modelInstallEndpoint_get(&profile),
			Err(AiTransportError::Configuration(AiConfigError::InvalidModel)),
		);
	}

	#[test]
	fn ollamaServerCheckDoesNotRequireAModelAndValidatesVersion()
	{
		let mut profile = profile_get(AiProvider::Ollama);
		profile.model.clear();
		assert_eq!(ollamaServerEndpoint_get(&profile).unwrap().as_str(),"http://127.0.0.1:11434/api/version");
		assert_eq!(ollamaServerResponse_validate(r#"{"version":"0.12.6"}"#),Ok(()));
		assert_eq!(ollamaServerResponse_validate(r#"{"status":"success"}"#),Err(AiTransportError::InvalidResponse));
		assert_eq!(ollamaServerEndpoint_get(&profile_get(AiProvider::OpenAI)),Err(AiTransportError::InvalidRequest));
	}

	#[test]
	fn ollamaModelInstallStreamReportsProgressAcrossChunkBoundaries()
	{
		use std::cell::RefCell;

		let progress = Rc::new(RefCell::new(Vec::new()));
		let callbackProgress = progress.clone();
		let callback = move |value| callbackProgress.borrow_mut().push(value);
		let mut response = AiModelInstallStream::default();
		response.chunk_push(b"{\"status\":\"pulling manifest\"}\n{\"status\":\"pulling layer\",\"total\":100,\"completed\":4",&callback).unwrap();
		response.chunk_push(b"0}\n{\"status\":\"verifying sha256 digest\"}\n{\"status\":\"success\"}\n",&callback).unwrap();
		assert_eq!(response.finish(&callback),Ok(()));
		assert_eq!(*progress.borrow(),vec![
			AiModelInstallProgress::Indeterminate,
			AiModelInstallProgress::Determinate(40),
			AiModelInstallProgress::Indeterminate,
			AiModelInstallProgress::Determinate(100),
		]);
	}

	#[test]
	fn ollamaModelInstallStreamRequiresSuccessAndRejectsErrorBodies()
	{
		let callback = |_| {};
		let mut incomplete = AiModelInstallStream::default();
		incomplete.chunk_push(b"{\"status\":\"pulling manifest\"}\n",&callback).unwrap();
		assert_eq!(incomplete.finish(&callback),Err(AiTransportError::InvalidResponse));

		let mut failed = AiModelInstallStream::default();
		assert_eq!(
			failed.chunk_push(b"{\"error\":\"private upstream detail\"}\n",&callback),
			Err(AiTransportError::ProviderFailure),
		);
	}

	#[test]
	fn errorBodiesAreNeverReturnedAsResponseText()
	{
		assert_eq!(
			response_text_get(AiProvider::OpenAI,r#"{"error":{"message":"sensitive upstream detail"}}"#),
			Err(AiTransportError::InvalidResponse),
		);
	}

	#[test]
	fn statusCodesMapToStableErrors()
	{
		assert_eq!(status_error_get(AiProvider::OpenAI,401,AiRequestOperation::Completion),AiTransportError::Unauthorized);
		assert_eq!(status_error_get(AiProvider::OpenAI,429,AiRequestOperation::Completion),AiTransportError::RateLimited);
		assert_eq!(status_error_get(AiProvider::OpenAI,404,AiRequestOperation::Completion),AiTransportError::ModelUnavailable);
		assert_eq!(status_error_get(AiProvider::Ollama,404,AiRequestOperation::Completion),AiTransportError::OllamaModelUnavailable);
		assert_eq!(status_error_get(AiProvider::Ollama,404,AiRequestOperation::ModelInstall),AiTransportError::OllamaModelInstallUnavailable);
		assert_eq!(status_error_get(AiProvider::Ollama,404,AiRequestOperation::OllamaServerTest),AiTransportError::OllamaServerUnavailable);
		assert_eq!(status_error_get(AiProvider::Ollama,504,AiRequestOperation::OllamaServerTest),AiTransportError::OllamaServerTimeout);
		assert_eq!(status_error_get(AiProvider::Ollama,504,AiRequestOperation::ModelInstall),AiTransportError::ModelInstallTimeout);
		assert_eq!(status_error_get(AiProvider::Ollama,500,AiRequestOperation::ModelInstall),AiTransportError::ProviderFailure);
		assert_eq!(AiRequestOperation::Completion.timeoutMs_get(),180_000);
		assert_eq!(AiRequestOperation::ModelList.timeoutMs_get(),180_000);
		assert_eq!(AiRequestOperation::OllamaServerTest.timeoutMs_get(),10_000);
		assert_eq!(AiRequestOperation::ModelInstall.timeoutMs_get(),1_800_000);
		assert_eq!(AiRequestOperation::Completion.responseMaximumBytes_get(),2 * 1024 * 1024);
		assert_eq!(AiRequestOperation::ModelInstall.responseMaximumBytes_get(),8 * 1024 * 1024);
	}

	#[test]
	fn webHomeOriginCannotBecomeTheOllamaDestination()
	{
		let endpoint = Url::parse("https://home.example/api/chat").unwrap();

		assert!(endpoint_isWebHomeOrigin(&endpoint,"https://home.example"));
		assert!(endpoint_isWebHomeOrigin(&endpoint,"https://home.example:443"));
		assert!(!endpoint_isWebHomeOrigin(&endpoint,"https://llm.example"));
		assert!(!endpoint_isWebHomeOrigin(&endpoint,"https://home.example:8443"));
	}

	#[test]
	fn debugOutputNeverContainsPromptOrResponseText()
	{
		let request = AiCompletionRequest {
			messages: vec![AiMessage::user("prompt-that-must-not-leak")],
			maxOutputTokens: 512,
			responseJsonSchema: Some(json!({"private": "schema-that-must-not-leak"})),
		};
		let response = AiCompletionResponse {text: "response-that-must-not-leak".to_string()};

		assert!(!format!("{request:?}").contains("prompt-that-must-not-leak"));
		assert!(!format!("{request:?}").contains("schema-that-must-not-leak"));
		assert!(!format!("{response:?}").contains("response-that-must-not-leak"));
	}
}
