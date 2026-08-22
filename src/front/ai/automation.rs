use crate::api::modules::components::{ModuleContent,ModuleID};
use regex::Regex;
use serde::{Deserialize,Serialize};
use std::pin::Pin;

pub(crate) mod engine;
pub(crate) mod history;
pub(crate) mod runtime;
pub(crate) mod view;
pub(crate) use engine::AiAutomationEngine;

pub(crate) const AI_AUTOMATION_CONTEXT_MAXIMUM: usize = 32;
pub(crate) const AI_AUTOMATION_CONTEXT_NAME_MAXIMUM_BYTES: usize = 128;
pub(crate) const AI_AUTOMATION_INSTRUCTIONS_MAXIMUM_BYTES: usize = 16 * 1024;
pub(crate) const AI_AUTOMATION_TARGET_ACTION_MAXIMUM: usize = 16;
pub(crate) const AI_AUTOMATION_RESPONSE_ACTION_MAXIMUM: usize = 8;
pub(crate) const AI_AUTOMATION_QUEUE_MAXIMUM: usize = 32;
pub(crate) const AI_AUTOMATION_EVENT_CONTEXT_MAXIMUM: usize = 4;
pub(crate) const AI_AUTOMATION_EXPOSURE_MAXIMUM_BYTES: usize = 128 * 1024;
pub(crate) const AI_AUTOMATION_CALLS_PER_HOUR: u16 = 10;
pub(crate) const AI_AUTOMATION_CALLS_PER_DAY: u16 = 50;
pub(crate) const AI_AUTOMATION_HISTORY_MAXIMUM: usize = 10;
const AI_AUTOMATION_CONTEXT_VERSION: u8 = 1;
const AI_AUTOMATION_SCHEMA_VERSION: u8 = 1;
const AI_AUTOMATION_HISTORY_VERSION: u8 = 1;
const AI_AUTOMATION_IDENTIFIER_MAXIMUM_BYTES: usize = 128;
const AI_AUTOMATION_MODULE_ID_MAXIMUM_BYTES: usize = 256;
const AI_AUTOMATION_CURSOR_MAXIMUM_BYTES: usize = 512;
const AI_AUTOMATION_CHECKPOINT_HISTORY_MAXIMUM: usize = 128;
const AI_AUTOMATION_CAPABILITY_RULE_MAXIMUM: usize = 16;
const AI_AUTOMATION_CAPABILITY_RULE_MAXIMUM_BYTES: usize = 1_024;
const AI_AUTOMATION_TEXT_PATTERN_MAXIMUM_BYTES: usize = 512;
const AI_AUTOMATION_HISTORY_VALUE_MAXIMUM_BYTES: usize = 512;

#[derive(Clone,Copy,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum AiConfirmationPolicy
{
	#[default]
	Confirm,
	Automatic,
}

#[derive(Clone,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct AiAutomationSource
{
	pub(crate) moduleId: ModuleID,
	pub(crate) event: String,
	pub(crate) fields: Vec<String>,
}

#[derive(Clone,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct AiAutomationTargetAction
{
	pub(crate) action: String,
	pub(crate) confirmation: AiConfirmationPolicy,
	pub(crate) fixedArguments: Vec<AiNamedValue>,
}

impl AiAutomationTargetAction
{
	fn validate(&self) -> Result<(),AiAutomationError>
	{
		let ids = self.fixedArguments.iter().map(|argument| argument.id.clone()).collect::<Vec<_>>();
		let totalBytes = self.fixedArguments.iter().try_fold(0usize,|size,argument| {
			return size.checked_add(argument.value.size_get());
		}).ok_or(AiAutomationError::InvalidTarget)?;
		if (self.fixedArguments.len() > AI_AUTOMATION_CAPABILITY_RULE_MAXIMUM
			|| !identifiers_areUniqueAndValid(&ids)
			|| totalBytes > AI_AUTOMATION_EXPOSURE_MAXIMUM_BYTES)
		{
			return Err(AiAutomationError::InvalidTarget);
		}
		return Ok(());
	}

	pub(super) fn fixedArguments_validate(&self,capability: &AiActionCapability) -> Result<(),AiAutomationError>
	{
		let definitions = capability.arguments.iter()
			.filter(|argument| argument.fixedByContext)
			.map(AiValueDefinition::contextValidation_get)
			.collect::<Vec<_>>();
		values_validate(&self.fixedArguments,&definitions)
			.map_err(|_| AiAutomationError::PermissionDenied)
	}

	pub(super) fn responseArguments_validate(
		&self,
		arguments: &[AiNamedValue],
		capability: &AiActionCapability,
	) -> Result<(),AiAutomationError>
	{
		let definitions = capability.arguments.iter().map(|definition| {
			if (definition.fixedByContext && self.fixedArgument_get(definition.id).is_some())
			{
				return definition.contextValidation_get();
			}
			return definition.clone();
		}).collect::<Vec<_>>();
		return values_validate(arguments,&definitions);
	}

	pub(super) fn fixedArgument_get(&self,id: &str) -> Option<&AiNamedValue>
	{
		return self.fixedArguments.iter().find(|argument| argument.id == id);
	}

	pub(super) fn responseArguments_match(&self,arguments: &[AiNamedValue]) -> bool
	{
		return self.fixedArguments.iter().all(|fixed| {
			return arguments.iter().find(|argument| argument.id == fixed.id) == Some(fixed);
		});
	}
}

#[derive(Clone,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct AiAutomationTarget
{
	pub(crate) moduleId: ModuleID,
	pub(crate) actions: Vec<AiAutomationTargetAction>,
}

#[derive(Clone,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct AiAutomationBudgetWindow
{
	pub(crate) startedAt: i64,
	pub(crate) calls: u16,
}

#[derive(Clone,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct AiAutomationCheckpoint
{
	pub(crate) cursor: Option<String>,
	pub(crate) hour: AiAutomationBudgetWindow,
	pub(crate) day: AiAutomationBudgetWindow,
	pub(crate) recentExecutions: Vec<String>,
	pub(crate) appliedActions: Vec<String>,
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct AiAutomationContext
{
	version: u8,
	pub(crate) id: String,
	pub(crate) name: String,
	pub(crate) enabled: bool,
	pub(crate) source: AiAutomationSource,
	pub(crate) instructions: String,
	pub(crate) targets: Vec<AiAutomationTarget>,
	pub(crate) checkpoint: AiAutomationCheckpoint,
}

impl Default for AiAutomationContext
{
	fn default() -> Self
	{
		return Self {
			version: AI_AUTOMATION_CONTEXT_VERSION,
			id: String::new(),
			name: String::new(),
			enabled: false,
			source: AiAutomationSource::default(),
			instructions: String::new(),
			targets: Vec::new(),
			checkpoint: AiAutomationCheckpoint::default(),
		};
	}
}

impl AiAutomationContext
{
	pub(crate) fn new(source: AiAutomationSource,target: AiAutomationTarget) -> Self
	{
		return Self {
			id: uuid::Uuid::new_v4().to_string(),
			name: String::new(),
			source,
			targets: vec![target],
			..Default::default()
		};
	}

	pub(crate) fn validate(&self) -> Result<(),AiAutomationError>
	{
		if (self.version != AI_AUTOMATION_CONTEXT_VERSION)
		{
			return Err(AiAutomationError::UnsupportedVersion);
		}
		if (!identifier_isValid(&self.id)
			|| self.name.is_empty()
			|| self.name.len() > AI_AUTOMATION_CONTEXT_NAME_MAXIMUM_BYTES
			|| self.name.trim() != self.name
			|| self.name.chars().any(char::is_control))
		{
			return Err(AiAutomationError::InvalidContext);
		}
		if (!moduleId_isValid(&self.source.moduleId)
			|| !identifier_isValid(&self.source.event)
			|| self.source.fields.is_empty()
			|| !identifiers_areUniqueAndValid(&self.source.fields))
		{
			return Err(AiAutomationError::InvalidSource);
		}
		if (self.instructions.len() > AI_AUTOMATION_INSTRUCTIONS_MAXIMUM_BYTES
			|| self.instructions.trim() != self.instructions
			|| self.instructions.chars().any(|character| character == '\0'))
		{
			return Err(AiAutomationError::InvalidInstructions);
		}
		if (self.targets.is_empty() || self.targets.len() > AI_AUTOMATION_TARGET_ACTION_MAXIMUM)
		{
			return Err(AiAutomationError::InvalidTarget);
		}
		let mut targetModules = Vec::with_capacity(self.targets.len());
		let mut actionCount = 0;
		for target in &self.targets
		{
			if (!moduleId_isValid(&target.moduleId)
				|| target.actions.is_empty()
				|| !targetModules.iter().all(|moduleId| moduleId != &target.moduleId))
			{
				return Err(AiAutomationError::InvalidTarget);
			}
			targetModules.push(target.moduleId.clone());
			let actions = target.actions.iter().map(|action| action.action.clone()).collect::<Vec<_>>();
			if (!identifiers_areUniqueAndValid(&actions)
				|| target.actions.iter().any(|action| action.validate().is_err()))
			{
				return Err(AiAutomationError::InvalidTarget);
			}
			actionCount += actions.len();
		}
		if (actionCount > AI_AUTOMATION_TARGET_ACTION_MAXIMUM)
		{
			return Err(AiAutomationError::InvalidTarget);
		}
		self.checkpoint.validate()?;
		return Ok(());
	}

	pub(crate) fn checkpoint_reconcile(&mut self,current: Option<&Self>)
	{
		self.checkpoint = match current
		{
			Some(current) if self.executionDefinition_isSame(current) => current.checkpoint.clone(),
			_ => AiAutomationCheckpoint::default(),
		};
	}

	pub(crate) fn executionDefinition_isSame(&self,other: &Self) -> bool
	{
		return self.version == other.version
			&& self.source == other.source
			&& self.instructions == other.instructions
			&& self.targets == other.targets;
	}

	pub(crate) fn executionDefinitionFingerprint_get(&self) -> Result<String,AiAutomationError>
	{
		let definition = serde_json::to_string(&(
			self.version,&self.source,&self.instructions,&self.targets,
		)).map_err(|_| AiAutomationError::InvalidContext)?;
		return Ok(crate::global_security::hash(definition));
	}
}

impl AiAutomationCheckpoint
{
	fn validate(&self) -> Result<(),AiAutomationError>
	{
		if (self.cursor.as_ref().is_some_and(|cursor| !boundedOpaqueValue_isValid(cursor,AI_AUTOMATION_CURSOR_MAXIMUM_BYTES))
			|| self.hour.calls > AI_AUTOMATION_CALLS_PER_HOUR
			|| self.day.calls > AI_AUTOMATION_CALLS_PER_DAY
			|| self.recentExecutions.len() > AI_AUTOMATION_CHECKPOINT_HISTORY_MAXIMUM
			|| self.appliedActions.len() > AI_AUTOMATION_CHECKPOINT_HISTORY_MAXIMUM
			|| !opaqueValues_areUniqueAndValid(&self.recentExecutions)
			|| !opaqueValues_areUniqueAndValid(&self.appliedActions))
		{
			return Err(AiAutomationError::InvalidCheckpoint);
		}
		return Ok(());
	}
}

pub(crate) fn automationContexts_validate(contexts: &[AiAutomationContext]) -> Result<(),AiAutomationError>
{
	if (contexts.len() > AI_AUTOMATION_CONTEXT_MAXIMUM)
	{
		return Err(AiAutomationError::TooManyContexts);
	}
	let mut ids = Vec::with_capacity(contexts.len());
	for context in contexts
	{
		context.validate()?;
		if (ids.iter().any(|id| id == &context.id))
		{
			return Err(AiAutomationError::DuplicateContext);
		}
		ids.push(context.id.clone());
	}
	return Ok(());
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub(crate) enum AiValueKind
{
	Text,
	Integer,
	Boolean,
}

impl AiValueKind
{
	pub(crate) fn id_get(self) -> &'static str
	{
		return match self
		{
			Self::Text => "text",
			Self::Integer => "integer",
			Self::Boolean => "boolean",
		};
	}
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) struct AiValueDefinition
{
	pub(crate) id: &'static str,
	pub(crate) translateKey: &'static str,
	pub(crate) kind: AiValueKind,
	pub(crate) required: bool,
	pub(crate) maximumBytes: usize,
	pub(crate) allowedTextValues: Vec<AiTextChoice>,
	pub(crate) fixedByContext: bool,
	fixedChoiceMayDisappear: bool,
	textConstraint: Option<AiTextConstraint>,
}

#[derive(Clone,Debug,Eq,PartialEq)]
struct AiTextConstraint
{
	pattern: &'static str,
	description: &'static str,
}

impl AiValueDefinition
{
	pub(crate) fn text(id: &'static str,translateKey: &'static str,required: bool,maximumBytes: usize) -> Self
	{
		return Self {
			id,translateKey,kind: AiValueKind::Text,required,maximumBytes,
			allowedTextValues: Vec::new(),fixedByContext: false,fixedChoiceMayDisappear: false,
			textConstraint: None,
		};
	}

	pub(crate) fn textWithChoices(
		id: &'static str,
		translateKey: &'static str,
		required: bool,
		maximumBytes: usize,
		allowedTextValues: Vec<AiTextChoice>,
	) -> Self
	{
		return Self {
			id,translateKey,kind: AiValueKind::Text,required,maximumBytes,allowedTextValues,
			fixedByContext: false,fixedChoiceMayDisappear: false,textConstraint: None,
		};
	}

	pub(crate) fn textWithFixedChoices(
		id: &'static str,
		translateKey: &'static str,
		maximumBytes: usize,
		allowedTextValues: Vec<AiTextChoice>,
	) -> Self
	{
		return Self {
			id,translateKey,kind: AiValueKind::Text,required: true,maximumBytes,allowedTextValues,
			fixedByContext: true,fixedChoiceMayDisappear: false,textConstraint: None,
		};
	}

	pub(crate) fn textWithRetainedFixedChoices(
		id: &'static str,
		translateKey: &'static str,
		maximumBytes: usize,
		allowedTextValues: Vec<AiTextChoice>,
	) -> Self
	{
		let mut definition = Self::textWithFixedChoices(id,translateKey,maximumBytes,allowedTextValues);
		definition.fixedChoiceMayDisappear = true;
		return definition;
	}

	pub(crate) fn withTextConstraint(mut self,pattern: &'static str,description: &'static str) -> Self
	{
		self.textConstraint = Some(AiTextConstraint {pattern,description});
		return self;
	}

	pub(crate) fn integer(id: &'static str,translateKey: &'static str,required: bool) -> Self
	{
		return Self {
			id,translateKey,kind: AiValueKind::Integer,required,maximumBytes: size_of::<i64>(),
			allowedTextValues: Vec::new(),fixedByContext: false,fixedChoiceMayDisappear: false,
			textConstraint: None,
		};
	}

	pub(crate) fn boolean(id: &'static str,translateKey: &'static str,required: bool) -> Self
	{
		return Self {
			id,translateKey,kind: AiValueKind::Boolean,required,maximumBytes: size_of::<bool>(),
			allowedTextValues: Vec::new(),fixedByContext: false,fixedChoiceMayDisappear: false,
			textConstraint: None,
		};
	}

	fn contextValidation_get(&self) -> Self
	{
		let mut definition = self.clone();
		if (definition.fixedChoiceMayDisappear)
		{
			definition.allowedTextValues.clear();
		}
		return definition;
	}
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) struct AiTextChoice
{
	pub(crate) value: String,
	pub(crate) label: String,
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) struct AiEventCapability
{
	pub(crate) id: &'static str,
	pub(crate) translateKey: &'static str,
	pub(crate) fields: Vec<AiValueDefinition>,
	pub(crate) promptRules: Vec<&'static str>,
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) struct AiActionCapability
{
	pub(crate) id: &'static str,
	pub(crate) translateKey: &'static str,
	pub(crate) arguments: Vec<AiValueDefinition>,
	pub(crate) promptRules: Vec<&'static str>,
	pub(crate) forcedConfirmation: Option<AiConfirmationPolicy>,
}

impl AiActionCapability
{
	pub(crate) fn confirmation_get(&self,configured: AiConfirmationPolicy) -> AiConfirmationPolicy
	{
		return self.forcedConfirmation.unwrap_or(configured);
	}

	pub(crate) fn confirmation_validate(&self,configured: AiConfirmationPolicy) -> Result<(),AiAutomationError>
	{
		if (self.forcedConfirmation.is_some_and(|forced| forced != configured))
		{
			return Err(AiAutomationError::PermissionDenied);
		}
		return Ok(());
	}
}

#[derive(Clone,Debug,Default,Eq,PartialEq)]
pub(crate) struct AiCapabilityCatalog
{
	pub(crate) events: Vec<AiEventCapability>,
	pub(crate) actions: Vec<AiActionCapability>,
}

impl AiCapabilityCatalog
{
	pub(crate) fn isEmpty(&self) -> bool
	{
		return self.events.is_empty() && self.actions.is_empty();
	}

	pub(crate) fn validate(&self) -> Result<(),AiAutomationError>
	{
		let eventIds = self.events.iter().map(|event| event.id.to_string()).collect::<Vec<_>>();
		let actionIds = self.actions.iter().map(|action| action.id.to_string()).collect::<Vec<_>>();
		if (!identifiers_areUniqueAndValid(&eventIds) || !identifiers_areUniqueAndValid(&actionIds))
		{
			return Err(AiAutomationError::InvalidCapability);
		}
		if (self.events.iter().any(|event| event.translateKey.is_empty()
				|| !capabilityRules_areValid(&event.promptRules))
			|| self.actions.iter().any(|action| action.translateKey.is_empty()
				|| !capabilityRules_areValid(&action.promptRules)))
		{
			return Err(AiAutomationError::InvalidCapability);
		}
		for values in self.events.iter().map(|event| &event.fields)
			.chain(self.actions.iter().map(|action| &action.arguments))
		{
			let ids = values.iter().map(|value| value.id.to_string()).collect::<Vec<_>>();
			if (!identifiers_areUniqueAndValid(&ids)
				|| values.iter().any(|value| value.translateKey.is_empty()
					|| value.maximumBytes == 0
					|| value.maximumBytes > AI_AUTOMATION_EXPOSURE_MAXIMUM_BYTES
					|| (value.fixedByContext && (value.kind != AiValueKind::Text || !value.required))
					|| !value.allowedTextValues_areValid()
					|| !value.textConstraint_isValid()))
			{
				return Err(AiAutomationError::InvalidCapability);
			}
		}
		return Ok(());
	}

	pub(crate) fn event_get(&self,id: &str) -> Option<&AiEventCapability>
	{
		return self.events.iter().find(|event| event.id == id);
	}

	pub(crate) fn action_get(&self,id: &str) -> Option<&AiActionCapability>
	{
		return self.actions.iter().find(|action| action.id == id);
	}
}

impl AiValueDefinition
{
	fn textConstraint_isValid(&self) -> bool
	{
		let Some(constraint) = &self.textConstraint else {return true;};
		return self.kind == AiValueKind::Text
			&& boundedOpaqueValue_isValid(constraint.pattern,AI_AUTOMATION_TEXT_PATTERN_MAXIMUM_BYTES)
			&& boundedOpaqueValue_isValid(constraint.description,AI_AUTOMATION_CAPABILITY_RULE_MAXIMUM_BYTES)
			&& Regex::new(constraint.pattern).is_ok();
	}

	fn textConstraint_matches(&self,value: &str) -> bool
	{
		let Some(constraint) = &self.textConstraint else {return true;};
		return Regex::new(constraint.pattern).is_ok_and(|pattern| pattern.is_match(value));
	}

	fn allowedTextValues_areValid(&self) -> bool
	{
		if (self.allowedTextValues.is_empty())
		{
			return true;
		}
		if (self.kind != AiValueKind::Text || self.allowedTextValues.len() > 64)
		{
			return false;
		}
		return self.allowedTextValues.iter().enumerate().all(|(index,choice)| {
			return boundedOpaqueValue_isValid(&choice.value,self.maximumBytes)
				&& boundedOpaqueValue_isValid(&choice.label,1_024)
				&& self.allowedTextValues[..index].iter().all(|previous| previous.value != choice.value);
		});
	}
}

#[derive(Clone,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct AiEventGrant
{
	pub(crate) event: String,
	pub(crate) fields: Vec<String>,
}

#[derive(Clone,Debug,Default,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct AiModuleGrant
{
	pub(crate) events: Vec<AiEventGrant>,
	pub(crate) actions: Vec<String>,
}

impl AiModuleGrant
{
	pub(crate) fn validate(&self,catalog: &AiCapabilityCatalog) -> Result<(),AiAutomationError>
	{
		let eventIds = self.events.iter().map(|grant| grant.event.clone()).collect::<Vec<_>>();
		if (!identifiers_areUniqueAndValid(&eventIds) || !identifiers_areUniqueAndValid(&self.actions))
		{
			return Err(AiAutomationError::InvalidCapability);
		}
		for grant in &self.events
		{
			let event = catalog.event_get(&grant.event).ok_or(AiAutomationError::InvalidCapability)?;
			if (grant.fields.is_empty()
				|| !identifiers_areUniqueAndValid(&grant.fields)
				|| grant.fields.iter().any(|field| !event.fields.iter().any(|available| available.id == field)))
			{
				return Err(AiAutomationError::InvalidCapability);
			}
		}
		if (self.actions.iter().any(|action| catalog.action_get(action).is_none()))
		{
			return Err(AiAutomationError::InvalidCapability);
		}
		return Ok(());
	}

	pub(crate) fn event_allows(&self,event: &str,fields: &[String]) -> bool
	{
		return self.events.iter().find(|grant| grant.event == event)
			.is_some_and(|grant| fields.iter().all(|field| grant.fields.contains(field)));
	}

	pub(crate) fn action_allows(&self,action: &str) -> bool
	{
		return self.actions.iter().any(|allowed| allowed == action);
	}
}

#[derive(Clone,Debug)]
pub(crate) struct AiModuleCapabilities
{
	pub(crate) moduleId: ModuleID,
	pub(crate) moduleType: String,
	pub(crate) catalog: AiCapabilityCatalog,
	pub(crate) grant: AiModuleGrant,
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(rename_all="snake_case",tag="type",content="value")]
pub(crate) enum AiValue
{
	Text(String),
	Integer(i64),
	Boolean(bool),
}

impl AiValue
{
	fn kind_get(&self) -> AiValueKind
	{
		return match self
		{
			Self::Text(_) => AiValueKind::Text,
			Self::Integer(_) => AiValueKind::Integer,
			Self::Boolean(_) => AiValueKind::Boolean,
		};
	}

	fn size_get(&self) -> usize
	{
		return match self
		{
			Self::Text(value) => value.len(),
			Self::Integer(_) => size_of::<i64>(),
			Self::Boolean(_) => size_of::<bool>(),
		};
	}

	pub(crate) fn display_get(&self) -> String
	{
		return match self
		{
			Self::Text(value) => value.clone(),
			Self::Integer(value) => value.to_string(),
			Self::Boolean(value) => value.to_string(),
		};
	}
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AiNamedValue
{
	pub(crate) id: String,
	pub(crate) value: AiValue,
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AiAutomationHistoryArgument
{
	pub(crate) id: String,
	pub(crate) value: String,
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AiAutomationHistoryEntry
{
	version: u8,
	pub(crate) actionKey: String,
	pub(crate) appliedAt: i64,
	pub(crate) contextName: String,
	pub(crate) targetModuleId: ModuleID,
	pub(crate) targetModuleType: String,
	pub(crate) action: String,
	pub(crate) arguments: Vec<AiAutomationHistoryArgument>,
}

impl AiAutomationHistoryEntry
{
	pub(crate) fn new(
		contextName: &str,
		targetModuleType: &str,
		action: &AiValidatedAction,
		appliedAt: i64,
	) -> Result<Self,AiAutomationError>
	{
		let history = Self {
			version: AI_AUTOMATION_HISTORY_VERSION,
			actionKey: action.actionKey.clone(),
			appliedAt,
			contextName: contextName.to_string(),
			targetModuleId: action.targetModuleId.clone(),
			targetModuleType: targetModuleType.to_string(),
			action: action.action.clone(),
			arguments: action.arguments.iter().take(AI_AUTOMATION_CAPABILITY_RULE_MAXIMUM).map(|argument| AiAutomationHistoryArgument {
				id: argument.id.clone(),
				value: historyValuePreview_get(&argument.value.display_get()),
			}).collect(),
		};
		history.validate()?;
		return Ok(history);
	}

	fn validate(&self) -> Result<(),AiAutomationError>
	{
		let argumentIds = self.arguments.iter().map(|argument| argument.id.clone()).collect::<Vec<_>>();
		if (self.version != AI_AUTOMATION_HISTORY_VERSION
			|| !boundedOpaqueValue_isValid(&self.actionKey,AI_AUTOMATION_CURSOR_MAXIMUM_BYTES)
			|| self.appliedAt < 0
			|| !boundedOpaqueValue_isValid(&self.contextName,AI_AUTOMATION_CONTEXT_NAME_MAXIMUM_BYTES)
			|| !moduleId_isValid(&self.targetModuleId)
			|| !boundedOpaqueValue_isValid(&self.targetModuleType,AI_AUTOMATION_IDENTIFIER_MAXIMUM_BYTES)
			|| !identifier_isValid(&self.action)
			|| self.arguments.len() > AI_AUTOMATION_CAPABILITY_RULE_MAXIMUM
			|| !identifiers_areUniqueAndValid(&argumentIds)
			|| self.arguments.iter().any(|argument| argument.value.len() > AI_AUTOMATION_HISTORY_VALUE_MAXIMUM_BYTES
				|| argument.value.chars().any(|character| character == '\0')))
		{
			return Err(AiAutomationError::InvalidCheckpoint);
		}
		return Ok(());
	}
}

pub(crate) fn automationHistory_validate(history: &[AiAutomationHistoryEntry]) -> Result<(),AiAutomationError>
{
	if (history.len() > AI_AUTOMATION_HISTORY_MAXIMUM)
	{
		return Err(AiAutomationError::InvalidCheckpoint);
	}
	for (index,entry) in history.iter().enumerate()
	{
		entry.validate()?;
		if (history[..index].iter().any(|previous| previous.actionKey == entry.actionKey))
		{
			return Err(AiAutomationError::InvalidCheckpoint);
		}
	}
	return Ok(());
}

pub(crate) fn automationHistory_add(
	history: &mut Vec<AiAutomationHistoryEntry>,
	entry: AiAutomationHistoryEntry,
) -> Result<(),AiAutomationError>
{
	entry.validate()?;
	history.retain(|previous| previous.actionKey != entry.actionKey);
	history.push(entry);
	if (history.len() > AI_AUTOMATION_HISTORY_MAXIMUM)
	{
		history.drain(..history.len() - AI_AUTOMATION_HISTORY_MAXIMUM);
	}
	return automationHistory_validate(history);
}

fn historyValuePreview_get(value: &str) -> String
{
	let value = value.replace('\0',"�");
	if (value.len() <= AI_AUTOMATION_HISTORY_VALUE_MAXIMUM_BYTES)
	{
		return value;
	}
	let mut end = AI_AUTOMATION_HISTORY_VALUE_MAXIMUM_BYTES.saturating_sub('…'.len_utf8());
	while (end > 0 && !value.is_char_boundary(end))
	{
		end -= 1;
	}
	let mut preview = value[..end].to_string();
	preview.push('…');
	return preview;
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AiAutomationResponseAction
{
	#[serde(with="aiResponseModuleId")]
	pub(crate) targetModuleId: ModuleID,
	pub(crate) action: String,
	pub(crate) arguments: Vec<AiNamedValue>,
}

#[allow(non_snake_case)]
mod aiResponseModuleId
{
	use super::ModuleID;
	use serde::{Deserialize,Deserializer,Serialize,Serializer};

	#[derive(Deserialize,Serialize)]
	#[serde(deny_unknown_fields)]
	struct ResponseModuleId
	{
		id: String,
	}

	pub(super) fn serialize<S: Serializer>(moduleId: &ModuleID,serializer: S) -> Result<S::Ok,S::Error>
	{
		return ResponseModuleId {id: moduleId.id.clone()}.serialize(serializer);
	}

	pub(super) fn deserialize<'de,D: Deserializer<'de>>(deserializer: D) -> Result<ModuleID,D::Error>
	{
		let response = ResponseModuleId::deserialize(deserializer)?;
		return Ok(ModuleID {id: response.id});
	}
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AiAutomationResponse
{
	schemaVersion: u8,
	pub(crate) actions: Vec<AiAutomationResponseAction>,
}

impl AiAutomationResponse
{
	#[cfg(test)]
	pub(crate) fn parse(content: &str) -> Result<Self,AiAutomationError>
	{
		let response = serde_json::from_str::<Self>(content.trim())
			.map_err(|_| AiAutomationError::InvalidValue)?;
		response.validate()?;
		return Ok(response);
	}

	#[cfg(test)]
	pub(crate) fn test_get(actions: Vec<AiAutomationResponseAction>) -> Self
	{
		return Self {schemaVersion: AI_AUTOMATION_SCHEMA_VERSION,actions};
	}

	pub(crate) fn validate(&self) -> Result<(),AiAutomationError>
	{
		if (self.schemaVersion != AI_AUTOMATION_SCHEMA_VERSION)
		{
			return Err(AiAutomationError::UnsupportedVersion);
		}
		if (self.actions.len() > AI_AUTOMATION_RESPONSE_ACTION_MAXIMUM)
		{
			return Err(AiAutomationError::TooManyActions);
		}
		return Ok(());
	}
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub(crate) enum AiEventCausation
{
	External,
	AiAction,
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) struct AiAutomationEvent
{
	schemaVersion: u8,
	pub(crate) sourceModuleId: ModuleID,
	pub(crate) event: String,
	pub(crate) eventId: String,
	pub(crate) occurredAt: i64,
	pub(crate) causation: AiEventCausation,
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) struct AiEventReservationCandidate
{
	pub(crate) expectedTimestamp: i64,
	pub(crate) timestamp: i64,
	pub(crate) content: String,
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) enum AiEventReservation
{
	Unsupported,
	AlreadyHandled,
	Prepared(AiEventReservationCandidate),
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) struct AiActionPersistenceCandidate
{
	pub(crate) expectedTimestamp: i64,
	pub(crate) timestamp: i64,
	pub(crate) content: String,
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) enum AiActionPersistence
{
	Unsupported,
	AlreadyApplied,
	Prepared(AiActionPersistenceCandidate),
}

impl AiAutomationEvent
{
	pub(crate) fn new(
		sourceModuleId: ModuleID,
		event: String,
		eventId: String,
		occurredAt: i64,
		causation: AiEventCausation,
	) -> Self
	{
		return Self {schemaVersion: AI_AUTOMATION_SCHEMA_VERSION,sourceModuleId,event,eventId,occurredAt,causation};
	}

	fn validate(&self) -> Result<(),AiAutomationError>
	{
		if (self.schemaVersion != AI_AUTOMATION_SCHEMA_VERSION)
		{
			return Err(AiAutomationError::UnsupportedVersion);
		}
		if (!moduleId_isValid(&self.sourceModuleId)
			|| !identifier_isValid(&self.event)
			|| !boundedOpaqueValue_isValid(&self.eventId,AI_AUTOMATION_CURSOR_MAXIMUM_BYTES)
			|| self.occurredAt < 0)
		{
			return Err(AiAutomationError::InvalidSource);
		}
		return Ok(());
	}
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) struct AiExposureRequest
{
	pub(crate) event: AiAutomationEvent,
	pub(crate) fields: Vec<String>,
}

impl AiExposureRequest
{
	pub(crate) fn validate(&self) -> Result<(),AiAutomationError>
	{
		self.event.validate()?;
		if (self.fields.is_empty() || !identifiers_areUniqueAndValid(&self.fields))
		{
			return Err(AiAutomationError::InvalidSource);
		}
		return Ok(());
	}
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) struct AiExposure
{
	schemaVersion: u8,
	pub(crate) values: Vec<AiNamedValue>,
}

impl AiExposure
{
	pub(crate) fn new(values: Vec<AiNamedValue>) -> Self
	{
		return Self {schemaVersion: AI_AUTOMATION_SCHEMA_VERSION,values};
	}

	pub(crate) fn validate(&self,definitions: &[AiValueDefinition]) -> Result<(),AiAutomationError>
	{
		if (self.schemaVersion != AI_AUTOMATION_SCHEMA_VERSION)
		{
			return Err(AiAutomationError::UnsupportedVersion);
		}
		values_validate(&self.values,definitions)?;
		let totalBytes = self.values.iter().try_fold(0usize,|size,value| size.checked_add(value.value.size_get()))
			.ok_or(AiAutomationError::InvalidValue)?;
		if (totalBytes > AI_AUTOMATION_EXPOSURE_MAXIMUM_BYTES)
		{
			return Err(AiAutomationError::InvalidValue);
		}
		return Ok(());
	}
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AiValidatedAction
{
	pub(crate) actionKey: String,
	pub(crate) executionId: String,
	pub(crate) targetModuleId: ModuleID,
	pub(crate) action: String,
	pub(crate) arguments: Vec<AiNamedValue>,
	pub(crate) confirmation: AiConfirmationPolicy,
}

impl AiValidatedAction
{
	pub(crate) fn storage_validate(&self) -> Result<(),AiAutomationError>
	{
		let argumentIds = self.arguments.iter().map(|argument| argument.id.clone()).collect::<Vec<_>>();
		let totalBytes = self.arguments.iter().try_fold(0usize,|size,argument| {
			return size.checked_add(argument.value.size_get());
		}).ok_or(AiAutomationError::InvalidValue)?;
		if (!boundedOpaqueValue_isValid(&self.actionKey,AI_AUTOMATION_CURSOR_MAXIMUM_BYTES)
			|| !boundedOpaqueValue_isValid(&self.executionId,AI_AUTOMATION_CURSOR_MAXIMUM_BYTES)
			|| !moduleId_isValid(&self.targetModuleId)
			|| !identifier_isValid(&self.action)
			|| self.arguments.len() > AI_AUTOMATION_CAPABILITY_RULE_MAXIMUM
			|| !identifiers_areUniqueAndValid(&argumentIds)
			|| totalBytes > AI_AUTOMATION_EXPOSURE_MAXIMUM_BYTES)
		{
			return Err(AiAutomationError::InvalidValue);
		}
		return Ok(());
	}

	pub(crate) fn delayed_validate(
		&self,
		context: &AiAutomationContext,
		definitionFingerprint: &str,
		modules: &[AiModuleCapabilities],
	) -> Result<(),AiAutomationError>
	{
		self.storage_validate()?;
		context.validate()?;
		if (!context.enabled
			|| context.executionDefinitionFingerprint_get()? != definitionFingerprint
			|| context.checkpoint.appliedActions.contains(&self.actionKey))
		{
			return Err(AiAutomationError::CapabilityUnavailable);
		}
		let target = context.targets.iter().find(|target| target.moduleId == self.targetModuleId)
			.ok_or(AiAutomationError::PermissionDenied)?;
		let permission = target.actions.iter().find(|permission| permission.action == self.action)
			.ok_or(AiAutomationError::PermissionDenied)?;
		let module = modules.iter().find(|module| module.moduleId == self.targetModuleId)
			.ok_or(AiAutomationError::CapabilityUnavailable)?;
		if (!module.grant.action_allows(&self.action))
		{
			return Err(AiAutomationError::PermissionDenied);
		}
		let capability = module.catalog.action_get(&self.action)
			.ok_or(AiAutomationError::CapabilityUnavailable)?;
		capability.confirmation_validate(permission.confirmation)?;
		if (self.confirmation != capability.confirmation_get(permission.confirmation))
		{
			return Err(AiAutomationError::PermissionDenied);
		}
		permission.responseArguments_validate(&self.arguments,capability)?;
		if (!permission.responseArguments_match(&self.arguments))
		{
			return Err(AiAutomationError::PermissionDenied);
		}
		return Ok(());
	}
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
#[cfg_attr(not(feature="hydrate"),allow(dead_code))]
pub(crate) enum AiActionApplyResult
{
	Applied,
	Rejected,
	Ambiguous,
}

pub(crate) type AiExposureFuture = Pin<Box<dyn Future<Output = Result<AiExposure,AiAutomationError>> + 'static>>;
pub(crate) type AiActionFuture = Pin<Box<dyn Future<Output = AiActionApplyResult> + 'static>>;

pub(crate) trait AiAutomationCapable
{
	fn ai_capabilities(&self) -> AiCapabilityCatalog
	{
		return AiCapabilityCatalog::default();
	}

	fn ai_grant(&self) -> AiModuleGrant
	{
		return AiModuleGrant::default();
	}

	fn ai_exposure(&self,_request: AiExposureRequest) -> Option<AiExposureFuture>
	{
		return None;
	}

	fn ai_action_apply(&self,_action: AiValidatedAction) -> Option<AiActionFuture>
	{
		return None;
	}

	fn ai_actionPersistence_prepare(
		&self,
		_action: &AiValidatedAction,
		_base: Option<&ModuleContent>,
	) -> Result<AiActionPersistence,AiAutomationError>
	{
		return Ok(AiActionPersistence::Unsupported);
	}

	fn ai_actionPersistence_saved(&self,_content: &ModuleContent) -> Result<(),AiAutomationError>
	{
		return Ok(());
	}

	fn ai_eventRetry(&self,_event: &AiAutomationEvent)
	{
	}

	fn ai_eventReservation_prepare(
		&self,
		_event: &AiAutomationEvent,
		_base: Option<&ModuleContent>,
	) -> Result<AiEventReservation,AiAutomationError>
	{
		return Ok(AiEventReservation::Unsupported);
	}

	fn ai_eventReservation_saved(&self,_content: &ModuleContent) -> Result<(),AiAutomationError>
	{
		return Ok(());
	}
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub(crate) enum AiAutomationError
{
	UnsupportedVersion,
	TooManyContexts,
	DuplicateContext,
	InvalidContext,
	InvalidSource,
	InvalidInstructions,
	InvalidTarget,
	InvalidCheckpoint,
	InvalidCapability,
	CapabilityUnavailable,
	PermissionDenied,
	InvalidValue,
	InvalidResponse,
	TooManyActions,
	QueueFull,
	BudgetExceeded,
	DuplicateExecution,
	LifecycleClosed,
}

impl AiAutomationError
{
	pub(crate) fn translateKey_get(self) -> &'static str
	{
		return match self
		{
			Self::TooManyContexts => "FRONTAI_AUTOMATION_TOO_MANY_CONTEXTS",
			Self::InvalidInstructions => "FRONTAI_AUTOMATION_INSTRUCTIONS_INVALID",
			Self::CapabilityUnavailable => "FRONTAI_AUTOMATION_CAPABILITY_UNAVAILABLE",
			Self::PermissionDenied => "FRONTAI_AUTOMATION_PERMISSION_DENIED",
			Self::QueueFull => "FRONTAI_AUTOMATION_QUEUE_FULL",
			Self::BudgetExceeded => "FRONTAI_AUTOMATION_BUDGET_EXCEEDED",
			Self::LifecycleClosed => "FRONTAI_AUTOMATION_INTERRUPTED",
			Self::InvalidResponse => "FRONTAI_AUTOMATION_RESPONSE_INVALID",
			Self::UnsupportedVersion | Self::DuplicateContext
			| Self::InvalidContext | Self::InvalidSource | Self::InvalidTarget
			| Self::InvalidCheckpoint | Self::InvalidCapability | Self::InvalidValue
			| Self::TooManyActions | Self::DuplicateExecution => "FRONTAI_AUTOMATION_INVALID",
		};
	}
}

pub(super) fn values_validate(values: &[AiNamedValue],definitions: &[AiValueDefinition]) -> Result<(),AiAutomationError>
{
	let ids = values.iter().map(|value| value.id.clone()).collect::<Vec<_>>();
	if (!identifiers_areUniqueAndValid(&ids))
	{
		return Err(AiAutomationError::InvalidValue);
	}
	for definition in definitions
	{
		let value = values.iter().find(|value| value.id == definition.id);
		if (definition.required && value.is_none())
		{
			return Err(AiAutomationError::InvalidValue);
		}
		if let Some(value) = value
			&& (value.value.kind_get() != definition.kind
				|| value.value.size_get() > definition.maximumBytes
				|| match &value.value
				{
					AiValue::Text(text) => {
						(!definition.allowedTextValues.is_empty()
							&& !definition.allowedTextValues.iter().any(|choice| choice.value == *text))
							|| !definition.textConstraint_matches(text)
					},
					_ => false,
				})
		{
			return Err(AiAutomationError::InvalidValue);
		}
	}
	if (values.iter().any(|value| !definitions.iter().any(|definition| definition.id == value.id)))
	{
		return Err(AiAutomationError::InvalidValue);
	}
	return Ok(());
}

fn identifier_isValid(value: &str) -> bool
{
	return !value.is_empty()
		&& value.len() <= AI_AUTOMATION_IDENTIFIER_MAXIMUM_BYTES
		&& value.bytes().all(|character| character.is_ascii_alphanumeric() || matches!(character,b'.' | b'_' | b'-'));
}

fn moduleId_isValid(moduleId: &ModuleID) -> bool
{
	return boundedOpaqueValue_isValid(&moduleId.id,AI_AUTOMATION_MODULE_ID_MAXIMUM_BYTES);
}

fn boundedOpaqueValue_isValid(value: &str,maximumBytes: usize) -> bool
{
	return !value.is_empty()
		&& value.len() <= maximumBytes
		&& value.trim() == value
		&& !value.chars().any(char::is_control);
}

fn capabilityRules_areValid(rules: &[&str]) -> bool
{
	return rules.len() <= AI_AUTOMATION_CAPABILITY_RULE_MAXIMUM
		&& rules.iter().all(|rule| boundedOpaqueValue_isValid(rule,AI_AUTOMATION_CAPABILITY_RULE_MAXIMUM_BYTES));
}

fn identifiers_areUniqueAndValid(values: &[String]) -> bool
{
	return values.iter().enumerate().all(|(index,value)| {
		identifier_isValid(value) && values[..index].iter().all(|previous| previous != value)
	});
}

fn opaqueValues_areUniqueAndValid(values: &[String]) -> bool
{
	return values.iter().enumerate().all(|(index,value)| {
		boundedOpaqueValue_isValid(value,AI_AUTOMATION_CURSOR_MAXIMUM_BYTES)
			&& values[..index].iter().all(|previous| previous != value)
	});
}

#[cfg(test)]
mod tests
{
	use super::*;

	fn context_get() -> AiAutomationContext
	{
		let mut context = AiAutomationContext::new(
			AiAutomationSource {
				moduleId: ModuleID {id: "source".to_string()},
				event: "item.created".to_string(),
				fields: vec!["title".to_string()],
			},
			AiAutomationTarget {
				moduleId: ModuleID {id: "target".to_string()},
				actions: vec![AiAutomationTargetAction {
					action: "item.add".to_string(),
					confirmation: AiConfirmationPolicy::Confirm,
					fixedArguments: Vec::new(),
				}],
			},
		);
		context.name = "Test context".to_string();
		context.instructions = "Create one target item.".to_string();
		return context;
	}

	#[test]
	fn contextsRejectDuplicateIdsAndUnboundedInstructions()
	{
		let context = context_get();
		assert_eq!(automationContexts_validate(&[context.clone(),context]),Err(AiAutomationError::DuplicateContext));
		assert_eq!(
			automationContexts_validate(&vec![context_get();AI_AUTOMATION_CONTEXT_MAXIMUM + 1]),
			Err(AiAutomationError::TooManyContexts),
		);

		let mut context = context_get();
		context.instructions = "x".repeat(AI_AUTOMATION_INSTRUCTIONS_MAXIMUM_BYTES + 1);
		assert_eq!(context.validate(),Err(AiAutomationError::InvalidInstructions));

		let mut context = context_get();
		context.instructions.clear();
		assert!(context.validate().is_ok());

		let mut context = context_get();
		context.targets[0].actions = (0..(AI_AUTOMATION_TARGET_ACTION_MAXIMUM + 1)).map(|index| {
			return AiAutomationTargetAction {
				action: format!("item.add-{index}"),
				confirmation: AiConfirmationPolicy::Confirm,
				fixedArguments: Vec::new(),
			};
		}).collect();
		assert_eq!(context.validate(),Err(AiAutomationError::InvalidTarget));
	}

	#[test]
	fn checkpointReconciliationPreservesOnlyAnUnchangedExecutionDefinition()
	{
		let mut current = context_get();
		current.checkpoint.cursor = Some("mailbox:1:42".to_string());
		current.checkpoint.hour.calls = 3;

		let mut renamed = current.clone();
		renamed.name = "Renamed context".to_string();
		renamed.enabled = !renamed.enabled;
		renamed.checkpoint = AiAutomationCheckpoint::default();
		renamed.checkpoint_reconcile(Some(&current));
		assert_eq!(renamed.checkpoint,current.checkpoint);

		let mut changed = current.clone();
		changed.instructions = "Use a different policy.".to_string();
		changed.checkpoint_reconcile(Some(&current));
		assert_eq!(changed.checkpoint,AiAutomationCheckpoint::default());

		let mut created = context_get();
		created.checkpoint.cursor = Some("stale".to_string());
		created.checkpoint_reconcile(None);
		assert_eq!(created.checkpoint,AiAutomationCheckpoint::default());
	}

	#[test]
	fn legacyTargetActionWithoutFixedArgumentsRemainsReadableButEmpty()
	{
		let mut serialized = serde_json::to_value(context_get()).unwrap();
		serialized.pointer_mut("/targets/0/actions/0").unwrap()
			.as_object_mut().unwrap().remove("fixedArguments");

		let restored = serde_json::from_value::<AiAutomationContext>(serialized).unwrap();

		assert!(restored.targets[0].actions[0].fixedArguments.is_empty());
	}

	#[test]
	fn actionValuesAreClosedByTheirCapabilitySchema()
	{
		let definitions = vec![AiValueDefinition {
			id: "title",
			translateKey: "TEST_TITLE",
			kind: AiValueKind::Text,
			required: true,
			maximumBytes: 16,
			allowedTextValues: Vec::new(),
			fixedByContext: false,
			fixedChoiceMayDisappear: false,
			textConstraint: None,
		}];
		assert!(values_validate(&[AiNamedValue {
			id: "title".to_string(),
			value: AiValue::Text("Meeting".to_string()),
		}],&definitions).is_ok());
		assert_eq!(values_validate(&[AiNamedValue {
			id: "url".to_string(),
			value: AiValue::Text("https://invalid.example".to_string()),
		}],&definitions),Err(AiAutomationError::InvalidValue));

		let responses = (0..(AI_AUTOMATION_RESPONSE_ACTION_MAXIMUM + 1)).map(|index| {
			return AiAutomationResponseAction {
				targetModuleId: ModuleID {id: format!("target-{index}")},
				action: "item.add".to_string(),
				arguments: Vec::new(),
			};
		}).collect();
		assert_eq!(AiAutomationResponse::test_get(responses).validate(),Err(AiAutomationError::TooManyActions));
	}

	#[test]
	fn textChoicesRejectLabelsAndUnknownValues()
	{
		let definitions = vec![AiValueDefinition::textWithChoices(
			"collection","TEST_COLLECTION",true,128,
			vec![AiTextChoice {
				value: "https://calendar.invalid/private/".to_string(),
				label: "Personal".to_string(),
			}],
		)];

		assert!(values_validate(&[AiNamedValue {
			id: "collection".to_string(),
			value: AiValue::Text("https://calendar.invalid/private/".to_string()),
		}],&definitions).is_ok());
		assert_eq!(values_validate(&[AiNamedValue {
			id: "collection".to_string(),
			value: AiValue::Text("Personal".to_string()),
		}],&definitions),Err(AiAutomationError::InvalidValue));
	}

	#[test]
	fn textConstraintsRejectProviderValuesOutsideTheDeclaredFormat()
	{
		let definitions = vec![AiValueDefinition::text("start","TEST_START",true,64).withTextConstraint(
			r"^[0-9]{4}-[0-9]{2}-[0-9]{2}(T[0-9]{2}:[0-9]{2}(:[0-9]{2})?)?$",
			"Local date or local date-time without a UTC offset.",
		)];
		assert!(values_validate(&[AiNamedValue {
			id: "start".to_string(),
			value: AiValue::Text("2026-08-24T16:45".to_string()),
		}],&definitions).is_ok());
		assert_eq!(values_validate(&[AiNamedValue {
			id: "start".to_string(),
			value: AiValue::Text("2026-08-21T13:01:40Z".to_string()),
		}],&definitions),Err(AiAutomationError::InvalidValue));

		let invalid = AiValueDefinition::text("start","TEST_START",true,64)
			.withTextConstraint("(","Invalid pattern.");
		assert!(!invalid.textConstraint_isValid());
	}

	#[test]
	fn appliedActionHistoryKeepsTenNewestBoundedDisplaySnapshots()
	{
		let action_get = |index: usize,value: String| AiValidatedAction {
			actionKey: format!("action-{index}"),
			executionId: format!("execution-{index}"),
			targetModuleId: ModuleID {id: "calendar-instance".to_string()},
			action: "calendar.event.create".to_string(),
			arguments: vec![AiNamedValue {id: "description".to_string(),value: AiValue::Text(value)}],
			confirmation: AiConfirmationPolicy::Confirm,
		};
		let mut history = Vec::new();
		for index in 0..(AI_AUTOMATION_HISTORY_MAXIMUM + 1)
		{
			let value = if (index == AI_AUTOMATION_HISTORY_MAXIMUM)
			{
				format!("{}\0", "é".repeat(300))
			}
			else
			{
				format!("Appointment {index}")
			};
			let entry = AiAutomationHistoryEntry::new("Mail appointments","CALENDAR",&action_get(index,value),index as i64).unwrap();
			automationHistory_add(&mut history,entry).unwrap();
		}

		assert_eq!(history.len(),AI_AUTOMATION_HISTORY_MAXIMUM);
		assert_eq!(history.first().unwrap().actionKey,"action-1");
		let preview = &history.last().unwrap().arguments[0].value;
		assert!(preview.len() <= AI_AUTOMATION_HISTORY_VALUE_MAXIMUM_BYTES);
		assert!(preview.ends_with('…'));
		assert!(!preview.contains('\0'));

		let replacement = AiAutomationHistoryEntry::new(
			"Renamed automation","CALENDAR",&action_get(5,"Updated".to_string()),100,
		).unwrap();
		automationHistory_add(&mut history,replacement).unwrap();
		assert_eq!(history.len(),AI_AUTOMATION_HISTORY_MAXIMUM);
		assert_eq!(history.last().unwrap().actionKey,"action-5");
		assert_eq!(history.last().unwrap().contextName,"Renamed automation");
	}

	#[test]
	fn retainedFixedChoiceSurvivesItsRemovalFromTheCurrentCatalog()
	{
		let permission = AiAutomationTargetAction {
			action: "todo.task.append".to_string(),
			confirmation: AiConfirmationPolicy::Confirm,
			fixedArguments: vec![AiNamedValue {
				id: "heading".to_string(),
				value: AiValue::Text("Removed heading".to_string()),
			}],
		};
		let retainedCapability = AiActionCapability {
			id: "todo.task.append",
			translateKey: "TEST_TODO_APPEND",
			arguments: vec![
				AiValueDefinition::textWithRetainedFixedChoices(
					"heading","TEST_HEADING",1_024,vec![AiTextChoice {
						value: "Current heading".to_string(),label: "Current heading".to_string(),
					}],
				),
				AiValueDefinition::text("task","TEST_TASK",true,4_096),
			],
			promptRules: Vec::new(),
			forcedConfirmation: None,
		};
		let response = vec![
			permission.fixedArguments[0].clone(),
			AiNamedValue {id: "task".to_string(),value: AiValue::Text("Generated".to_string())},
		];

		assert!(permission.fixedArguments_validate(&retainedCapability).is_ok());
		assert!(permission.responseArguments_validate(&response,&retainedCapability).is_ok());
		assert!(permission.responseArguments_match(&response));

		let mut strictCapability = retainedCapability;
		strictCapability.arguments[0] = AiValueDefinition::textWithFixedChoices(
			"heading","TEST_HEADING",1_024,vec![AiTextChoice {
				value: "Current heading".to_string(),label: "Current heading".to_string(),
			}],
		);
		assert_eq!(
			permission.fixedArguments_validate(&strictCapability),
			Err(AiAutomationError::PermissionDenied),
		);
	}

	#[test]
	fn grantsOnlyReferenceDeclaredCapabilities()
	{
		let catalog = AiCapabilityCatalog {
			events: vec![AiEventCapability {
				id: "item.created",
				translateKey: "TEST_EVENT",
				fields: vec![AiValueDefinition {
					id: "title",
					translateKey: "TEST_TITLE",
					kind: AiValueKind::Text,
					required: true,
					maximumBytes: 64,
					allowedTextValues: Vec::new(),
					fixedByContext: false,
					fixedChoiceMayDisappear: false,
					textConstraint: None,
				}],
				promptRules: Vec::new(),
			}],
			actions: vec![AiActionCapability {
				id: "item.add",
				translateKey: "TEST_ACTION",
				arguments: Vec::new(),
				promptRules: Vec::new(),
				forcedConfirmation: None,
			}],
		};
		let mut grant = AiModuleGrant {
			events: vec![AiEventGrant {event: "item.created".to_string(),fields: vec!["title".to_string()]}],
			actions: vec!["item.add".to_string()],
		};
		assert!(grant.validate(&catalog).is_ok());
		grant.events[0].fields = vec!["content".to_string()];
		assert_eq!(grant.validate(&catalog),Err(AiAutomationError::InvalidCapability));
	}

	#[test]
	fn eventsAndExposuresAreVersionedTypedAndBounded()
	{
		let event = AiAutomationEvent::new(
			ModuleID {id: "source".to_string()},
			"item.created".to_string(),
			"item-1".to_string(),
			1,
			AiEventCausation::External,
		);
		assert!(AiExposureRequest {event,fields: vec!["title".to_string()]}.validate().is_ok());

		let definitions = vec![
			AiValueDefinition::text("title","TEST_TITLE",true,70 * 1024),
			AiValueDefinition::integer("count","TEST_COUNT",true),
			AiValueDefinition::boolean("active","TEST_ACTIVE",true),
		];
		let exposure = AiExposure::new(vec![
			AiNamedValue {id: "title".to_string(),value: AiValue::Text("Meeting".to_string())},
			AiNamedValue {id: "count".to_string(),value: AiValue::Integer(1)},
			AiNamedValue {id: "active".to_string(),value: AiValue::Boolean(true)},
		]);
		assert!(exposure.validate(&definitions).is_ok());

		let oversizedDefinitions = vec![
			AiValueDefinition::text("first","TEST_FIRST",true,70 * 1024),
			AiValueDefinition::text("second","TEST_SECOND",true,70 * 1024),
		];
		let oversized = AiExposure::new(vec![
			AiNamedValue {id: "first".to_string(),value: AiValue::Text("x".repeat(70 * 1024))},
			AiNamedValue {id: "second".to_string(),value: AiValue::Text("y".repeat(70 * 1024))},
		]);
		assert_eq!(oversized.validate(&oversizedDefinitions),Err(AiAutomationError::InvalidValue));
	}
}
