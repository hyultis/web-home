mod document;
mod editor;

use std::collections::HashSet;
use std::fmt::{Debug, Formatter};
use leptoaster::ToasterContext;
use leptos::prelude::{ClassAttribute,ElementChild,Get,GetUntracked,IntoAny,OnAttribute,PropAttribute,Set,Update,event_target_checked};
use leptos::prelude::{ArcRwSignal, RwSignal};
use leptos::children::ViewFn;
use leptos::{component,view,IntoView};
use serde::{Deserialize, Serialize};
use crate::api::modules::components::{ModuleContent, ModuleID};
use crate::front::ai::automation::{
	AiActionCapability,AiActionPersistence,AiActionPersistenceCandidate,AiAutomationCapable,
	AiAutomationError,AiCapabilityCatalog,AiModuleGrant,AiNamedValue,AiTextChoice,AiValidatedAction,
	AiValue,AiValueDefinition,
};
use crate::front::modules::components::{Backable, BoxFuture, Cache, Cacheable, ModuleName, ModuleSizeContrainte, RefreshTime};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::translate::TranslateText;

static MAX_LENGTH: usize = 100000;
const TODO_DOCUMENT_VERSION: u8 = 1;
const TODO_AI_ACTION_APPEND: &str = "todo.task.append";
const TODO_AI_ARGUMENT_HEADING: &str = "heading";
const TODO_AI_ARGUMENT_TASK: &str = "task";
const TODO_AI_HEADING_MAXIMUM_BYTES: usize = 1_024;
const TODO_AI_TASK_MAXIMUM_BYTES: usize = 4_096;
const TODO_AI_APPLIED_KEY_MAXIMUM_BYTES: usize = 64;
const TODO_AI_APPLIED_KEY_MAXIMUM: usize = 2_048;

fn todoDocument_version() -> u8
{
	return TODO_DOCUMENT_VERSION;
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
struct TodoPersistedDocument
{
	#[serde(default = "todoDocument_version")]
	version: u8,
	content: String,
	aiGrant: AiModuleGrant,
	aiAppliedActionKeys: Vec<String>,
}

impl Default for TodoPersistedDocument
{
	fn default() -> Self
	{
		return Self {
			version: TODO_DOCUMENT_VERSION,
			content: String::new(),
			aiGrant: AiModuleGrant::default(),
			aiAppliedActionKeys: Vec::new(),
		};
	}
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TodoPersistedFormat
{
	Legacy(String),
	Document(TodoPersistedDocument),
}

#[derive(Serialize,Deserialize,Default)]
pub struct Todo
{
	content: ArcRwSignal<String>,
	aiGrant: ArcRwSignal<AiModuleGrant>,
	aiAppliedActionKeys: ArcRwSignal<Vec<String>>,
	_update: ArcRwSignal<Cache>,
	_sended: ArcRwSignal<Cache>,
}

impl Todo
{
	pub fn new() -> Self
	{
		Self {
			content: ArcRwSignal::new("".to_string()),
			aiGrant: ArcRwSignal::new(AiModuleGrant::default()),
			aiAppliedActionKeys: ArcRwSignal::new(Vec::new()),
			_update: ArcRwSignal::new(Default::default()),
			_sended: Default::default(),
		}
	}

	fn aiAppliedActionKey_isValid(key: &str) -> bool
	{
		return !key.is_empty()
			&& key.len() <= TODO_AI_APPLIED_KEY_MAXIMUM_BYTES
			&& key.trim() == key
			&& !key.chars().any(char::is_control);
	}

	fn persisted_normalize(document: &mut TodoPersistedDocument) -> bool
	{
		if (document.version != TODO_DOCUMENT_VERSION)
		{
			return false;
		}
		let mut uniqueKeys = HashSet::new();
		document.aiAppliedActionKeys.retain(|key| {
			return Self::aiAppliedActionKey_isValid(key) && uniqueKeys.insert(key.clone());
		});
		if (document.aiAppliedActionKeys.len() > TODO_AI_APPLIED_KEY_MAXIMUM)
		{
			document.aiAppliedActionKeys.drain(
				..document.aiAppliedActionKeys.len() - TODO_AI_APPLIED_KEY_MAXIMUM,
			);
		}
		return true;
	}

	fn persisted_parse(content: &str) -> Option<TodoPersistedDocument>
	{
		let persisted = serde_json::from_str::<TodoPersistedFormat>(content).ok()?;
		let mut document = match persisted
		{
			TodoPersistedFormat::Legacy(content) => TodoPersistedDocument {
				content,
				..Default::default()
			},
			TodoPersistedFormat::Document(document) => document,
		};
		return Self::persisted_normalize(&mut document).then_some(document);
	}

	fn persisted_get(&self) -> TodoPersistedDocument
	{
		let mut document = TodoPersistedDocument {
			content: self.content.get_untracked(),
			aiGrant: self.aiGrant.get_untracked(),
			aiAppliedActionKeys: self.aiAppliedActionKeys.get_untracked(),
			..Default::default()
		};
		Self::persisted_normalize(&mut document);
		return document;
	}

	fn persisted_apply(&self,mut document: TodoPersistedDocument)
	{
		if (!Self::persisted_normalize(&mut document))
		{
			return;
		}
		self.content.set(document.content);
		self.aiGrant.set(document.aiGrant);
		self.aiAppliedActionKeys.set(document.aiAppliedActionKeys);
	}

	fn aiAction_set(grant: &mut AiModuleGrant,enabled: bool)
	{
		grant.actions.retain(|action| action != TODO_AI_ACTION_APPEND);
		if (enabled)
		{
			grant.actions.push(TODO_AI_ACTION_APPEND.to_string());
		}
	}

	fn aiHeadingChoices_get(&self) -> Vec<AiTextChoice>
	{
		let document = document::TodoEditorDocument::source_parse(&self.content.get_untracked());
		let mut uniqueTitles = HashSet::new();
		return document.headingTitles_get().into_iter()
			.filter(|title| !title.is_empty()
				&& title.len() <= TODO_AI_HEADING_MAXIMUM_BYTES
				&& title.trim() == title
				&& !title.chars().any(char::is_control)
				&& uniqueTitles.insert(title.clone()))
			.take(64)
			.map(|title| AiTextChoice {value: title.clone(),label: title})
			.collect();
	}

	fn aiTextArgument_get(arguments: &[AiNamedValue],id: &str) -> Option<String>
	{
		return arguments.iter().find(|argument| argument.id == id)
			.and_then(|argument| match &argument.value
			{
				AiValue::Text(value) => Some(value.clone()),
				_ => None,
			});
	}

	fn aiTask_validate(task: &str) -> bool
	{
		return !task.is_empty()
			&& task.len() <= TODO_AI_TASK_MAXIMUM_BYTES
			&& task.trim() == task
			&& !task.chars().any(char::is_control);
	}

	fn aiActionDocument_apply(
		document: &mut TodoPersistedDocument,
		action: &AiValidatedAction,
	) -> Result<bool,AiAutomationError>
	{
		if (action.action != TODO_AI_ACTION_APPEND
			|| !document.aiGrant.action_allows(TODO_AI_ACTION_APPEND))
		{
			return Err(AiAutomationError::PermissionDenied);
		}
		if (document.aiAppliedActionKeys.contains(&action.actionKey))
		{
			return Ok(false);
		}
		let heading = Self::aiTextArgument_get(&action.arguments,TODO_AI_ARGUMENT_HEADING)
			.ok_or(AiAutomationError::InvalidValue)?;
		let task = Self::aiTextArgument_get(&action.arguments,TODO_AI_ARGUMENT_TASK)
			.ok_or(AiAutomationError::InvalidValue)?;
		if (heading.is_empty() || heading.len() > TODO_AI_HEADING_MAXIMUM_BYTES
			|| heading.trim() != heading || heading.chars().any(char::is_control)
			|| !Self::aiTask_validate(&task))
		{
			return Err(AiAutomationError::InvalidValue);
		}
		let mut editorDocument = document::TodoEditorDocument::source_parse(&document.content);
		if (!editorDocument.task_append(&heading,task))
		{
			return Err(AiAutomationError::InvalidValue);
		}
		let content = editorDocument.source_get();
		if (content.len() > MAX_LENGTH)
		{
			return Err(AiAutomationError::InvalidValue);
		}
		document.content = content;
		document.aiAppliedActionKeys.push(action.actionKey.clone());
		Self::persisted_normalize(document);
		return Ok(true);
	}
}

impl AiAutomationCapable for Todo
{
	fn ai_capabilities(&self) -> AiCapabilityCatalog
	{
		return AiCapabilityCatalog {
			events: Vec::new(),
			actions: vec![AiActionCapability {
				id: TODO_AI_ACTION_APPEND,
				translateKey: "MODULE_TODO_AI_APPEND_ACTION",
				arguments: vec![
					AiValueDefinition::textWithRetainedFixedChoices(
						TODO_AI_ARGUMENT_HEADING,"MODULE_TODO_AI_HEADING",
						TODO_AI_HEADING_MAXIMUM_BYTES,self.aiHeadingChoices_get(),
					),
					AiValueDefinition::text(TODO_AI_ARGUMENT_TASK,"MODULE_TODO_AI_TASK",true,TODO_AI_TASK_MAXIMUM_BYTES)
						.withTextConstraint(r"^[^\r\n]+$","Use one non-empty plain-text task line without a list marker."),
				],
				promptRules: vec![
					"Copy the fixed heading exactly and provide only the text of one unchecked task, without a Markdown marker.",
					"The target module appends the task at the end of that heading section, or at the end of the document if the heading no longer exists.",
					"Never request changes, deletion, completion, or replacement of an existing TODO line.",
				],
				forcedConfirmation: None,
			}],
		};
	}

	fn ai_grant(&self) -> AiModuleGrant
	{
		return self.aiGrant.get_untracked();
	}

	fn ai_actionPersistence_prepare(
		&self,
		action: &AiValidatedAction,
		base: Option<&ModuleContent>,
	) -> Result<AiActionPersistence,AiAutomationError>
	{
		if (action.action != TODO_AI_ACTION_APPEND)
		{
			return Ok(AiActionPersistence::Unsupported);
		}
		let localDirty = self._update.get_untracked().get() > self._sended.get_untracked().get();
		let (mut document,expectedTimestamp) = if let Some(base) = base
		{
			if (base.typeModule != Self::MODULE_NAME)
			{
				return Err(AiAutomationError::InvalidSource);
			}
			let remoteDocument = Self::persisted_parse(&base.content)
				.ok_or(AiAutomationError::InvalidSource)?;
			if (localDirty
				&& base.timestamp != self._sended.get_untracked().get()
				&& remoteDocument != self.persisted_get())
			{
				return Err(AiAutomationError::InvalidCheckpoint);
			}
			(remoteDocument,base.timestamp)
		}
		else
		{
			(self.persisted_get(),self._sended.get_untracked().get())
		};
		if (!Self::aiActionDocument_apply(&mut document,action)?)
		{
			return Ok(AiActionPersistence::AlreadyApplied);
		}
		let minimumTimestamp = expectedTimestamp.checked_add(1)
			.ok_or(AiAutomationError::InvalidCheckpoint)?;
		let minimumLocalTimestamp = self._update.get_untracked().get().checked_add(1)
			.ok_or(AiAutomationError::InvalidCheckpoint)?;
		let timestamp = Cache::now()
			.max(minimumLocalTimestamp)
			.max(minimumTimestamp);
		let content = serde_json::to_string(&document).map_err(|_| AiAutomationError::InvalidValue)?;
		return Ok(AiActionPersistence::Prepared(AiActionPersistenceCandidate {
			expectedTimestamp,timestamp,content,
		}));
	}

	fn ai_actionPersistence_saved(&self,content: &ModuleContent) -> Result<(),AiAutomationError>
	{
		if (content.typeModule != Self::MODULE_NAME
			|| Self::persisted_parse(&content.content).is_none())
		{
			return Err(AiAutomationError::InvalidSource);
		}
		return Ok(());
	}
}

impl Debug for Todo
{
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Todo")
			.field("content", &self.content.get_untracked())
			.field("aiGrant", &self.aiGrant.get_untracked())
			.field("aiAppliedActionKeys", &self.aiAppliedActionKeys.get_untracked())
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

	fn draw(&self, editMode: RwSignal<bool>,moduleActions: ModuleActionFn, moduleId: ModuleID) -> ViewFn
	{
		let contentInner = self.content.clone();
		let aiGrantInner = self.aiGrant.clone();
		let updateInner = self._update.clone();
		ViewFn::from(move || {
			return view! {
				<TodoDraw
					content=contentInner.clone()
					aiGrant=aiGrantInner.clone()
					update=updateInner.clone()
					editMode
					moduleActions=moduleActions.clone()
					moduleId=moduleId.clone()
				/>
			}.into_any();
		})
	}

	fn refresh_time(&self) -> RefreshTime {
		RefreshTime::MINUTES(1)
	}

	fn refresh(&self,moduleActions: ModuleActionFn, moduleId: ModuleID, _toaster: ToasterContext) -> Option<BoxFuture> {
		return Some(Box::pin(async move {
			(moduleActions.clone().getFn)((moduleId.clone()));
		}));
	}

	fn export(&self) -> ModuleContent
	{
		return ModuleContent{
			id: ModuleID::new(),
			typeModule: self.module_name(),
			timestamp: self._update.get_untracked().get(),
			content: serde_json::to_string(&self.persisted_get()).unwrap_or_default(),
			..Default::default()
		};
	}

	fn import(&mut self, import: ModuleContent)
	{
		let Some(importContent) = Self::persisted_parse(&import.content) else {return};

		self.persisted_apply(importContent);
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
		let content = Self::persisted_parse(&from.content)?;
		Some(Self {
			content: ArcRwSignal::new(content.content),
			aiGrant: ArcRwSignal::new(content.aiGrant),
			aiAppliedActionKeys: ArcRwSignal::new(content.aiAppliedActionKeys),
			_update: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
			_sended: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
		})
	}

	fn size(&self) -> ModuleSizeContrainte {
		ModuleSizeContrainte::default()
	}
}

#[component]
fn TodoDraw(
	content: ArcRwSignal<String>,
	aiGrant: ArcRwSignal<AiModuleGrant>,
	update: ArcRwSignal<Cache>,
	editMode: RwSignal<bool>,
	moduleActions: ModuleActionFn,
	moduleId: ModuleID,
) -> impl IntoView
{
	let permissionGrant = aiGrant.clone();
	let permissionCache = update.clone();
	view! {
		<>
			{editor::draw(content,update,moduleActions,moduleId)}
			{move || if (editMode.get())
			{
				view! {<TodoAiPermissions aiGrant=permissionGrant.clone() update=permissionCache.clone()/>}.into_any()
			}
			else
			{
				view! {}.into_any()
			}}
		</>
	}
}

#[component]
fn TodoAiPermissions(aiGrant: ArcRwSignal<AiModuleGrant>,update: ArcRwSignal<Cache>) -> impl IntoView
{
	let checkedGrant = aiGrant.clone();
	let changedGrant = aiGrant.clone();
	view! {
		<fieldset class="module_ai_permissions module_todo_ai_permissions">
			<legend><TranslateText key="MODULE_AI_PERMISSIONS"/></legend>
			<label class="module_ai_permission">
				<input
					type="checkbox"
					prop:checked=move || checkedGrant.get().action_allows(TODO_AI_ACTION_APPEND)
					on:change=move |event| {
						changedGrant.update(|grant| Todo::aiAction_set(grant,event_target_checked(&event)));
						update.update(|cache| cache.update());
					}
				/>
				<span><TranslateText key="MODULE_TODO_AI_APPEND_ACTION"/></span>
			</label>
			<p class="module_config_help"><TranslateText key="MODULE_TODO_AI_HELP"/></p>
		</fieldset>
	}
}

#[cfg(test)]
mod tests
{
	use leptos::prelude::{GetUntracked,Set,Update};

	use super::{Todo,TodoPersistedDocument,TODO_AI_ACTION_APPEND,TODO_AI_ARGUMENT_HEADING,TODO_AI_ARGUMENT_TASK};
	use crate::api::modules::components::{ModuleContent,ModuleID};
	use crate::front::ai::automation::{
		AiActionPersistence,AiAutomationCapable,AiAutomationError,AiConfirmationPolicy,AiNamedValue,
		AiValidatedAction,AiValue,
	};
	use crate::front::modules::components::{Backable,Cache,ModuleName};
	use crate::front::modules::module_type::ModuleType;

	fn action_get(key: &str,heading: &str,task: &str) -> AiValidatedAction
	{
		return AiValidatedAction {
			actionKey: key.to_string(),
			executionId: format!("execution-{key}"),
			targetModuleId: ModuleID {id: "todo-test".to_string()},
			action: TODO_AI_ACTION_APPEND.to_string(),
			arguments: vec![
				AiNamedValue {
					id: TODO_AI_ARGUMENT_HEADING.to_string(),
					value: AiValue::Text(heading.to_string()),
				},
				AiNamedValue {
					id: TODO_AI_ARGUMENT_TASK.to_string(),
					value: AiValue::Text(task.to_string()),
				},
			],
			confirmation: AiConfirmationPolicy::Confirm,
		};
	}

	fn todo_get(content: &str,sendedTimestamp: i64,updateTimestamp: i64) -> Todo
	{
		let todo = Todo::new();
		todo.content.set(content.to_string());
		todo.aiGrant.update(|grant| Todo::aiAction_set(grant,true));
		todo._sended.set(Cache::newFrom(sendedTimestamp));
		todo._update.set(Cache::newFrom(updateTimestamp));
		return todo;
	}

	#[test]
	fn legacyStringImportsIntoTheVersionedEnvelope()
	{
		let legacy = ModuleContent {
			typeModule: Todo::MODULE_NAME.to_string(),
			timestamp: 42,
			content: serde_json::to_string("# Work\n* existing").unwrap(),
			..Default::default()
		};
		let todo = Todo::newFromModuleContent(&legacy).unwrap();

		assert_eq!(todo.content.get_untracked(),"# Work\n* existing");
		assert!(!todo.aiGrant.get_untracked().action_allows(TODO_AI_ACTION_APPEND));
		assert!(todo.aiAppliedActionKeys.get_untracked().is_empty());
		let exported = todo.export();
		let value = serde_json::from_str::<serde_json::Value>(&exported.content).unwrap();
		assert_eq!(value.get("version").and_then(serde_json::Value::as_u64),Some(1));
		assert_eq!(value.get("content").and_then(serde_json::Value::as_str),Some("# Work\n* existing"));
	}

	#[test]
	fn appendActionIsIdempotentAndProducesANewerCandidateThanLocalEdits()
	{
		let mut todo = todo_get("# Work\n* existing",100,110);
		let action = action_get("action-one","Work","generated");

		let AiActionPersistence::Prepared(candidate) = todo.ai_actionPersistence_prepare(&action,None).unwrap()
		else {panic!("the append action should prepare a persisted candidate")};
		assert_eq!(candidate.expectedTimestamp,100);
		assert!(candidate.timestamp > 110);
		let document = Todo::persisted_parse(&candidate.content).unwrap();
		assert_eq!(document.content,"# Work\n* existing\n* generated");
		assert_eq!(document.aiAppliedActionKeys,vec!["action-one"]);

		let saved = ModuleContent {
			typeModule: Todo::MODULE_NAME.to_string(),
			timestamp: candidate.timestamp,
			content: candidate.content,
			..Default::default()
		};
		todo.import(saved.clone());
		assert_eq!(
			todo.ai_actionPersistence_prepare(&action,Some(&saved)).unwrap(),
			AiActionPersistence::AlreadyApplied,
		);
	}

	#[test]
	fn concurrentRemoteAppendIsPreservedWhenTheLocalDocumentIsClean()
	{
		let todo = todo_get("# Work\n* local",100,100);
		let mut remoteDocument = todo.persisted_get();
		remoteDocument.content = "# Work\n* remote".to_string();
		remoteDocument.aiAppliedActionKeys.push("other-action".to_string());
		let remote = ModuleContent {
			typeModule: Todo::MODULE_NAME.to_string(),
			timestamp: 120,
			content: serde_json::to_string(&remoteDocument).unwrap(),
			..Default::default()
		};

		let AiActionPersistence::Prepared(candidate) = todo.ai_actionPersistence_prepare(
			&action_get("new-action","Work","generated"),Some(&remote),
		).unwrap()
		else {panic!("a clean local document should merge on the remote base")};
		let merged = Todo::persisted_parse(&candidate.content).unwrap();
		assert_eq!(merged.content,"# Work\n* remote\n* generated");
		assert_eq!(merged.aiAppliedActionKeys,vec!["other-action","new-action"]);
		assert_eq!(candidate.expectedTimestamp,120);
		assert!(candidate.timestamp > 120);
	}

	#[test]
	fn concurrentRemoteEditRejectsInsteadOfOverwritingALocalEdit()
	{
		let todo = todo_get("# Work\nlocal edit",100,110);
		let mut remoteDocument = TodoPersistedDocument::default();
		remoteDocument.content = "# Work\nremote edit".to_string();
		Todo::aiAction_set(&mut remoteDocument.aiGrant,true);
		let remote = ModuleContent {
			typeModule: Todo::MODULE_NAME.to_string(),
			timestamp: 120,
			content: serde_json::to_string(&remoteDocument).unwrap(),
			..Default::default()
		};

		assert_eq!(
			todo.ai_actionPersistence_prepare(
				&action_get("new-action","Work","generated"),Some(&remote),
			),
			Err(AiAutomationError::InvalidCheckpoint),
		);
	}

	#[test]
	fn moduleTypeDelegatesThePersistentTodoAction()
	{
		let module = ModuleType::TODO(todo_get("# Work",100,100));

		assert!(matches!(
			module.ai_actionPersistence_prepare(
				&action_get("delegated-action","Work","generated"),None,
			).unwrap(),
			AiActionPersistence::Prepared(_),
		));
	}

	#[test]
	fn actionRejectsMultilineTasksWithoutChangingTheDocument()
	{
		let todo = todo_get("# Work",100,100);
		let before = todo.persisted_get();
		let mut candidate = before.clone();

		assert!(Todo::aiActionDocument_apply(
			&mut candidate,&action_get("invalid-action","Work","first\nsecond"),
		).is_err());
		assert_eq!(candidate,before);
	}
}
