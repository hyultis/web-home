use super::automation::{
	AiActionCapability,AiAutomationContext,AiAutomationError,AiCapabilityCatalog,AiConfirmationPolicy,
	AiModuleCapabilities,AiModuleGrant,AiNamedValue,AiTextChoice,AiValidatedAction,AiValue,
	AiValueDefinition,
};
use crate::api::modules::components::{ModuleContent,ModuleID};
use crate::front::modules::components::{distant_time_simpler,Cache};
use crate::front::modules::module_holder::{ModuleHolder,ModuleHolderEpoch};
use crate::front::utils::dialog::{DialogData,DialogManager};
use crate::front::utils::toaster_helpers::{toastingErr,toastingSuccess};
use crate::front::utils::translate::TranslateText;
use leptoaster::expect_toaster;
use leptos::prelude::{
	AriaAttributes,ClassAttribute,CollectView,ElementChild,For,Get,GetUntracked,GlobalAttributes,
	IntoAny,Memo,OnAttribute,RwSignal,Set,
};
use leptos::{component,view,IntoView};
use serde::{Deserialize,Serialize};

pub(crate) const AI_INBOX_ENTRY_MAXIMUM: usize = 64;
pub(crate) const AI_INBOX_MAXIMUM_BYTES: usize = 1024 * 1024;
pub(crate) const AI_INBOX_ALERT_ACTION: &str = "ai.alert.create";
const AI_INBOX_VERSION: u8 = 1;
const AI_INBOX_ENTRY_VERSION: u8 = 1;
const AI_INBOX_CONTEXT_ID_MAXIMUM_BYTES: usize = 128;
const AI_INBOX_CONTEXT_NAME_MAXIMUM_BYTES: usize = 128;
const AI_INBOX_FINGERPRINT_MAXIMUM_BYTES: usize = 512;
const AI_INBOX_MODULE_TYPE_MAXIMUM_BYTES: usize = 128;
const AI_INBOX_ALERT_TITLE_MAXIMUM_BYTES: usize = 256;
const AI_INBOX_ALERT_MESSAGE_MAXIMUM_BYTES: usize = 16 * 1024;
const AI_INBOX_OPAQUE_ID_MAXIMUM_BYTES: usize = 512;

#[derive(Clone,Copy,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum AiInboxAlertLevel
{
	Information,
	Attention,
}

impl AiInboxAlertLevel
{
	pub(crate) fn id_get(self) -> &'static str
	{
		return match self
		{
			Self::Information => "information",
			Self::Attention => "attention",
		};
	}

	fn fromId(value: &str) -> Option<Self>
	{
		return match value
		{
			"information" => Some(Self::Information),
			"attention" => Some(Self::Attention),
			_ => None,
		};
	}
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AiInboxAction
{
	version: u8,
	pub(crate) createdAt: i64,
	pub(crate) contextId: String,
	pub(crate) contextName: String,
	pub(crate) contextDefinitionFingerprint: String,
	pub(crate) targetModuleType: String,
	pub(crate) action: AiValidatedAction,
}

impl AiInboxAction
{
	pub(crate) fn new(
		context: &AiAutomationContext,
		targetModuleType: &str,
		action: AiValidatedAction,
		createdAt: i64,
	) -> Result<Self,AiInboxError>
	{
		let entry = Self {
			version: AI_INBOX_ENTRY_VERSION,
			createdAt,
			contextId: context.id.clone(),
			contextName: context.name.clone(),
			contextDefinitionFingerprint: context.executionDefinitionFingerprint_get()?,
			targetModuleType: targetModuleType.to_string(),
			action,
		};
		entry.validate()?;
		return Ok(entry);
	}

	pub(crate) fn id_get(&self) -> &str
	{
		return &self.action.actionKey;
	}

	fn validate(&self) -> Result<(),AiInboxError>
	{
		self.action.storage_validate()?;
		if (self.version != AI_INBOX_ENTRY_VERSION
			|| self.createdAt < 0
			|| !identifier_isValid(&self.contextId)
			|| !boundedText_isValid(&self.contextName,AI_INBOX_CONTEXT_NAME_MAXIMUM_BYTES)
			|| !boundedOpaque_isValid(&self.contextDefinitionFingerprint,AI_INBOX_FINGERPRINT_MAXIMUM_BYTES)
			|| !boundedOpaque_isValid(&self.targetModuleType,AI_INBOX_MODULE_TYPE_MAXIMUM_BYTES)
			|| self.action.confirmation != AiConfirmationPolicy::Confirm)
		{
			return Err(AiInboxError::InvalidDocument);
		}
		return Ok(());
	}
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AiInboxAlert
{
	version: u8,
	pub(crate) id: String,
	pub(crate) createdAt: i64,
	pub(crate) contextId: String,
	pub(crate) contextName: String,
	pub(crate) title: String,
	pub(crate) message: String,
	pub(crate) level: AiInboxAlertLevel,
}

impl AiInboxAlert
{
	pub(crate) fn fromAction(
		context: &AiAutomationContext,
		action: &AiValidatedAction,
		createdAt: i64,
	) -> Result<Self,AiInboxError>
	{
		if (action.targetModuleId.id != AiInboxHolder::MODULE_ID
			|| action.action != AI_INBOX_ALERT_ACTION
			|| action.confirmation != AiConfirmationPolicy::Automatic)
		{
			return Err(AiInboxError::InvalidDocument);
		}
		let title = textArgument_get(&action.arguments,"title")?;
		let message = textArgument_get(&action.arguments,"message")?;
		let level = AiInboxAlertLevel::fromId(&textArgument_get(&action.arguments,"level")?)
			.ok_or(AiInboxError::InvalidDocument)?;
		let entry = Self {
			version: AI_INBOX_ENTRY_VERSION,
			id: action.actionKey.clone(),
			createdAt,
			contextId: context.id.clone(),
			contextName: context.name.clone(),
			title,
			message,
			level,
		};
		entry.validate()?;
		return Ok(entry);
	}

	fn validate(&self) -> Result<(),AiInboxError>
	{
		if (self.version != AI_INBOX_ENTRY_VERSION
			|| self.createdAt < 0
			|| !boundedOpaque_isValid(&self.id,AI_INBOX_OPAQUE_ID_MAXIMUM_BYTES)
			|| !identifier_isValid(&self.contextId)
			|| !boundedText_isValid(&self.contextName,AI_INBOX_CONTEXT_NAME_MAXIMUM_BYTES)
			|| !boundedText_isValid(&self.title,AI_INBOX_ALERT_TITLE_MAXIMUM_BYTES)
			|| !boundedText_isValid(&self.message,AI_INBOX_ALERT_MESSAGE_MAXIMUM_BYTES))
		{
			return Err(AiInboxError::InvalidDocument);
		}
		return Ok(());
	}
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(tag="kind",content="entry",rename_all="snake_case",deny_unknown_fields)]
pub(crate) enum AiInboxEntry
{
	Action(AiInboxAction),
	Alert(AiInboxAlert),
}

impl AiInboxEntry
{
	pub(crate) fn id_get(&self) -> &str
	{
		return match self
		{
			Self::Action(entry) => entry.id_get(),
			Self::Alert(entry) => &entry.id,
		};
	}

	fn validate(&self) -> Result<(),AiInboxError>
	{
		return match self
		{
			Self::Action(entry) => entry.validate(),
			Self::Alert(entry) => entry.validate(),
		};
	}
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(default,deny_unknown_fields)]
pub(crate) struct AiInboxDocument
{
	version: u8,
	pub(crate) entries: Vec<AiInboxEntry>,
}

impl Default for AiInboxDocument
{
	fn default() -> Self
	{
		return Self {version: AI_INBOX_VERSION,entries: Vec::new()};
	}
}

impl AiInboxDocument
{
	pub(crate) fn validate(&self) -> Result<(),AiInboxError>
	{
		if (self.version != AI_INBOX_VERSION)
		{
			return Err(AiInboxError::UnsupportedVersion);
		}
		if (self.entries.len() > AI_INBOX_ENTRY_MAXIMUM)
		{
			return Err(AiInboxError::CapacityExceeded);
		}
		for (index,entry) in self.entries.iter().enumerate()
		{
			entry.validate()?;
			if (self.entries[..index].iter().any(|previous| previous.id_get() == entry.id_get()))
			{
				return Err(AiInboxError::InvalidDocument);
			}
		}
		let content = serde_json::to_string(self).map_err(|_| AiInboxError::InvalidDocument)?;
		if (content.len() > AI_INBOX_MAXIMUM_BYTES)
		{
			return Err(AiInboxError::CapacityExceeded);
		}
		return Ok(());
	}

	fn serialize(&self) -> Result<String,AiInboxError>
	{
		self.validate()?;
		return serde_json::to_string(self).map_err(|_| AiInboxError::InvalidDocument);
	}

	fn deserialize(content: &str) -> Result<Self,AiInboxError>
	{
		if (content.len() > AI_INBOX_MAXIMUM_BYTES)
		{
			return Err(AiInboxError::CapacityExceeded);
		}
		let document = serde_json::from_str::<Self>(content).map_err(|_| AiInboxError::InvalidDocument)?;
		document.validate()?;
		return Ok(document);
	}
}

#[derive(Clone,Debug)]
pub(crate) enum AiInboxMutation
{
	Add(Vec<AiInboxEntry>),
	Remove(String),
}

impl AiInboxMutation
{
	pub(crate) fn apply(&self,document: &mut AiInboxDocument) -> Result<bool,AiInboxError>
	{
		let changed = match self
		{
			Self::Add(entries) => {
				if (entries.len() > 8)
				{
					return Err(AiInboxError::CapacityExceeded);
				}
				let mut changed = false;
				for entry in entries
				{
					entry.validate()?;
					if let Some(existing) = document.entries.iter().find(|existing| existing.id_get() == entry.id_get())
					{
						if (existing != entry)
						{
							return Err(AiInboxError::InvalidDocument);
						}
						continue;
					}
					document.entries.push(entry.clone());
					changed = true;
				}
				changed
			},
			Self::Remove(id) => {
				if (!boundedOpaque_isValid(id,AI_INBOX_OPAQUE_ID_MAXIMUM_BYTES))
				{
					return Err(AiInboxError::InvalidDocument);
				}
				let previousLength = document.entries.len();
				document.entries.retain(|entry| entry.id_get() != id);
				document.entries.len() != previousLength
			},
		};
		document.validate()?;
		return Ok(changed);
	}
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub(crate) enum AiInboxError
{
	Automation(AiAutomationError),
	UnsupportedVersion,
	InvalidDocument,
	CapacityExceeded,
}

impl AiInboxError
{
	pub(crate) fn translateKey_get(self) -> &'static str
	{
		return match self
		{
			Self::CapacityExceeded => "FRONTAI_INBOX_FULL",
			Self::Automation(_) | Self::UnsupportedVersion | Self::InvalidDocument => "FRONTAI_INBOX_INVALID",
		};
	}
}

impl From<AiAutomationError> for AiInboxError
{
	fn from(error: AiAutomationError) -> Self
	{
		return Self::Automation(error);
	}
}

pub(crate) struct AiInboxHolder
{
	id: ModuleID,
	document: AiInboxDocument,
	update: Cache,
	sended: Cache,
	persisted: bool,
}

impl AiInboxHolder
{
	pub(crate) const MODULE_ID: &'static str = "AI_INBOX";
	pub(crate) const MODULE_NAME: &'static str = "AI_INBOX";

	pub(crate) fn new() -> Self
	{
		let cache = Cache::default();
		return Self {
			id: ModuleID {id: Self::MODULE_ID.to_string()},
			document: AiInboxDocument::default(),
			update: cache.clone(),
			sended: cache,
			persisted: false,
		};
	}

	pub(crate) fn capabilities_get(&self) -> AiModuleCapabilities
	{
		return AiModuleCapabilities {
			moduleId: self.id.clone(),
			moduleType: "AI".to_string(),
			catalog: AiCapabilityCatalog {
				events: Vec::new(),
				actions: vec![AiActionCapability {
					id: AI_INBOX_ALERT_ACTION,
					translateKey: "FRONTAI_ALERT_CREATE_ACTION",
					arguments: vec![
						AiValueDefinition::text("title","FRONTAI_ALERT_TITLE",true,AI_INBOX_ALERT_TITLE_MAXIMUM_BYTES),
						AiValueDefinition::text("message","FRONTAI_ALERT_MESSAGE",true,AI_INBOX_ALERT_MESSAGE_MAXIMUM_BYTES),
						AiValueDefinition::textWithChoices(
							"level","FRONTAI_ALERT_LEVEL",true,16,
							vec![
								AiTextChoice {value: "information".to_string(),label: "Information".to_string()},
								AiTextChoice {value: "attention".to_string(),label: "Attention".to_string()},
							],
						),
					],
					promptRules: vec![
						"Create an alert only when the source justifies information the user should keep for later.",
						"Use level information for neutral notices and attention only when the user should notice a significant or time-sensitive item.",
						"Title and message are plain text, not Markdown or HTML.",
					],
					forcedConfirmation: Some(AiConfirmationPolicy::Automatic),
				}],
			},
			grant: AiModuleGrant {events: Vec::new(),actions: vec![AI_INBOX_ALERT_ACTION.to_string()]},
		};
	}

	pub(crate) fn document_get(&self) -> AiInboxDocument
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

	pub(crate) fn persisted_get(&self) -> bool
	{
		return self.persisted;
	}

	pub(crate) fn timestamp_next(&self) -> i64
	{
		return Cache::now().max(self.update.get().saturating_add(1));
	}

	pub(crate) fn export_document(&self,document: &AiInboxDocument,timestamp: i64) -> Result<ModuleContent,AiInboxError>
	{
		return Ok(ModuleContent {
			id: self.id.clone(),
			typeModule: Self::MODULE_NAME.to_string(),
			timestamp,
			content: document.serialize()?,
			..Default::default()
		});
	}

	pub(crate) fn import(&mut self,content: ModuleContent) -> Result<(),AiInboxError>
	{
		if (content.id.id != Self::MODULE_ID || content.typeModule != Self::MODULE_NAME)
		{
			return Err(AiInboxError::InvalidDocument);
		}
		let document = AiInboxDocument::deserialize(&content.content)?;
		self.document = document;
		self.update.update_from(content.timestamp);
		self.sended.update_from(content.timestamp);
		self.persisted = true;
		return Ok(());
	}

	pub(crate) fn loaded_apply(&mut self,loaded: Self)
	{
		if (!self.persisted || loaded.update.get() >= self.update.get())
		{
			*self = loaded;
		}
	}

	pub(crate) fn saved_apply(&mut self,document: AiInboxDocument,timestamp: i64)
	{
		if (!self.persisted || timestamp >= self.update.get())
		{
			self.document = document;
			self.update.update_from(timestamp);
			self.sended.update_from(timestamp);
			self.persisted = true;
		}
	}
}

#[component]
pub(crate) fn AiInboxButton(lifecycleEpoch: ModuleHolderEpoch) -> impl IntoView
{
	let dialogManager = leptos::prelude::expect_context::<DialogManager>();
	let toaster = expect_toaster();
	let openDialog = dialogManager.clone();
	let openToaster = toaster.clone();
	let openFn = move |_| {
		if (!ModuleHolder::aiInbox_isReady())
		{
			return;
		}
		let busyEntry = RwSignal::new(None::<String>);
		let bodyToaster = openToaster.clone();
		let dialog = DialogData::new()
			.setTitle("FRONTAI_INBOX_TITLE")
			.setBody(move || view! {
				<AiInboxView lifecycleEpoch busyEntry toaster=bodyToaster.clone()/>
			}.into_any())
			.setIsLarger(true)
			.setButtonValidateTitle(None::<String>)
			.setButtonCloseTitle(Some("FRONTAI_INBOX_CLOSE"))
			.setCanClose(move || busyEntry.get().is_none());
		openDialog.open(dialog);

		let refreshToaster = openToaster.clone();
		ModuleHolder::task_spawn(lifecycleEpoch,async move {
			if let Err(errorKey) = ModuleHolder::aiInbox_refresh(lifecycleEpoch).await
			{
				toastingErr(&refreshToaster,errorKey).await;
			}
		});
	};

	return view! {
		<button
			type="button"
			class="header_ai_inbox_button icon_button"
			disabled=move || !ModuleHolder::aiInbox_isReady()
			on:click=openFn
		>
			<i class="iconoir-bell" aria-hidden="true"></i>
			<span class="visually_hidden"><TranslateText key="FRONTAI_INBOX_OPEN"/></span>
			{move || {
				let count = ModuleHolder::aiInbox_entries_get().len();
				return (count > 0).then(|| view! {
					<span class="header_ai_inbox_badge" aria-hidden="true">{count}</span>
					<span class="visually_hidden" role="status">
						<TranslateText key="FRONTAI_INBOX_PENDING_STATUS"/>
					</span>
				});
			}}
		</button>
	}.into_any();
}

#[component]
fn AiInboxView(
	lifecycleEpoch: ModuleHolderEpoch,
	busyEntry: RwSignal<Option<String>>,
	toaster: leptoaster::ToasterContext,
) -> impl IntoView
{
	return view! {
		<div class="ai_inbox">
			<p class="ai_inbox_help"><TranslateText key="FRONTAI_INBOX_HELP"/></p>
			{move || if (ModuleHolder::aiInbox_entries_get().is_empty())
			{
				view! {<p class="ai_inbox_empty"><TranslateText key="FRONTAI_INBOX_EMPTY"/></p>}.into_any()
			}
			else
			{
				let entryToaster = toaster.clone();
				view! {
					<ol class="ai_inbox_list">
						<For
							each=move || {
								let mut entries = ModuleHolder::aiInbox_entries_get();
								entries.reverse();
								return entries;
							}
							key=|entry| entry.id_get().to_string()
							children=move |entry| view! {
								<AiInboxEntryView
									entry
									lifecycleEpoch
									busyEntry
									toaster=entryToaster.clone()
								/>
							}
						/>
					</ol>
				}.into_any()
			}}
		</div>
	}.into_any();
}

#[component]
fn AiInboxEntryView(
	entry: AiInboxEntry,
	lifecycleEpoch: ModuleHolderEpoch,
	busyEntry: RwSignal<Option<String>>,
	toaster: leptoaster::ToasterContext,
) -> impl IntoView
{
	return match entry
	{
		AiInboxEntry::Action(entry) => aiInboxAction_view(entry,lifecycleEpoch,busyEntry,toaster),
		AiInboxEntry::Alert(entry) => aiInboxAlert_view(entry,lifecycleEpoch,busyEntry,toaster),
	};
}

fn aiInboxAction_view(
	entry: AiInboxAction,
	lifecycleEpoch: ModuleHolderEpoch,
	busyEntry: RwSignal<Option<String>>,
	toaster: leptoaster::ToasterContext,
) -> leptos::prelude::AnyView
{
	let modules = ModuleHolder::aiAutomationModules_get().unwrap_or_default();
	let capability = modules.iter()
		.find(|module| module.moduleId == entry.action.targetModuleId)
		.and_then(|module| module.catalog.action_get(&entry.action.action));
	let actionLabel = capability.map(|capability| capability.translateKey);
	let arguments = entry.action.arguments.iter().map(|argument| {
		let translateKey = capability.and_then(|capability| capability.arguments.iter()
			.find(|definition| definition.id == argument.id)
			.map(|definition| definition.translateKey));
		return (argument.id.clone(),translateKey,argument.value.display_get());
	}).collect::<Vec<_>>();
	let entryId = entry.id_get().to_string();
	let validationEntry = entry.clone();
	let usable = Memo::new(move |_| ModuleHolder::aiInbox_actionIsUsable(&validationEntry));
	let applyId = entryId.clone();
	let applyToaster = toaster.clone();
	let applyFn = move |_| {
		if (busyEntry.get_untracked().is_some() || !usable.get_untracked())
		{
			return;
		}
		busyEntry.set(Some(applyId.clone()));
		let taskId = applyId.clone();
		let taskToaster = applyToaster.clone();
		ModuleHolder::task_spawn(lifecycleEpoch,async move {
			let result = ModuleHolder::aiInbox_actionApply(
				lifecycleEpoch,taskId,taskToaster.clone(),
			).await;
			if (ModuleHolder::lifecycle_isActive(lifecycleEpoch))
			{
				busyEntry.set(None);
			}
			match result
			{
				Ok(()) => toastingSuccess(&taskToaster,"FRONTAI_INBOX_ACTION_APPLIED").await,
				Err(errorKey) => toastingErr(&taskToaster,errorKey).await,
			}
		});
	};
	let rejectId = entryId.clone();
	let rejectToaster = toaster.clone();
	let rejectFn = move |_| {
		if (busyEntry.get_untracked().is_some())
		{
			return;
		}
		busyEntry.set(Some(rejectId.clone()));
		let taskId = rejectId.clone();
		let taskToaster = rejectToaster.clone();
		ModuleHolder::task_spawn(lifecycleEpoch,async move {
			let result = ModuleHolder::aiInbox_entryRemove(lifecycleEpoch,taskId).await;
			if (ModuleHolder::lifecycle_isActive(lifecycleEpoch))
			{
				busyEntry.set(None);
			}
			match result
			{
				Ok(()) => toastingSuccess(&taskToaster,"FRONTAI_INBOX_ACTION_REJECTED").await,
				Err(errorKey) => toastingErr(&taskToaster,errorKey).await,
			}
		});
	};
	let contextName = entry.contextName.clone();
	let targetModuleType = entry.targetModuleType.clone();
	let actionId = entry.action.action.clone();

	return view! {
		<li
			class="ai_inbox_entry ai_inbox_entry--action"
			class:ai_inbox_entry--stale=move || !usable.get()
		>
			<div class="ai_inbox_entry_header">
				<div>
					<span class="ai_inbox_kind"><TranslateText key="FRONTAI_INBOX_ACTION_KIND"/></span>
					<strong>{targetModuleType}{" · "}{match actionLabel
					{
						Some(key) => view! {<TranslateText key/>}.into_any(),
						None => view! {{actionId}}.into_any(),
					}}</strong>
				</div>
				<span class="ai_inbox_time">{distant_time_simpler(entry.createdAt)}</span>
			</div>
			<p class="ai_inbox_context"><TranslateText key="FRONTAI_INBOX_CONTEXT"/>{" "}{contextName}</p>
			{move || (!usable.get()).then(|| view! {
				<p class="ai_inbox_stale"><TranslateText key="FRONTAI_INBOX_ACTION_STALE"/></p>
			})}
			<details>
				<summary><TranslateText key="FRONTAI_INBOX_DETAILS"/></summary>
				<dl>{arguments.iter().map(|(id,translateKey,value)| {
					let label = match translateKey
					{
						Some(key) => view! {<TranslateText key=*key/>}.into_any(),
						None => view! {{id.clone()}}.into_any(),
					};
					return view! {<div><dt>{label}</dt><dd>{value.clone()}</dd></div>};
				}).collect_view()}</dl>
			</details>
			<div class="ai_inbox_entry_actions">
				<button
					type="button"
					disabled=move || busyEntry.get().is_some()
					on:click=rejectFn
				><TranslateText key="FRONTAI_INBOX_REJECT"/></button>
				<button
					type="button"
					class="validate"
					disabled=move || busyEntry.get().is_some()
						|| !usable.get()
					on:click=applyFn
				><TranslateText key="FRONTAI_INBOX_APPLY"/></button>
			</div>
		</li>
	}.into_any();
}

fn aiInboxAlert_view(
	entry: AiInboxAlert,
	lifecycleEpoch: ModuleHolderEpoch,
	busyEntry: RwSignal<Option<String>>,
	toaster: leptoaster::ToasterContext,
) -> leptos::prelude::AnyView
{
	let entryId = entry.id.clone();
	let dismissFn = move |_| {
		if (busyEntry.get_untracked().is_some())
		{
			return;
		}
		busyEntry.set(Some(entryId.clone()));
		let taskId = entryId.clone();
		let taskToaster = toaster.clone();
		ModuleHolder::task_spawn(lifecycleEpoch,async move {
			let result = ModuleHolder::aiInbox_entryRemove(lifecycleEpoch,taskId).await;
			if (ModuleHolder::lifecycle_isActive(lifecycleEpoch))
			{
				busyEntry.set(None);
			}
			if let Err(errorKey) = result
			{
				toastingErr(&taskToaster,errorKey).await;
			}
		});
	};
	let levelClass = format!("ai_inbox_level ai_inbox_level--{}",entry.level.id_get());
	let entryClass = format!("ai_inbox_entry ai_inbox_entry--alert ai_inbox_entry--{}",entry.level.id_get());
	let levelKey = match entry.level
	{
		AiInboxAlertLevel::Information => "FRONTAI_ALERT_LEVEL_INFORMATION",
		AiInboxAlertLevel::Attention => "FRONTAI_ALERT_LEVEL_ATTENTION",
	};

	return view! {
		<li class=entryClass>
			<div class="ai_inbox_entry_header">
				<div>
					<span class=levelClass><TranslateText key=levelKey/></span>
					<strong>{entry.title}</strong>
				</div>
				<span class="ai_inbox_time">{distant_time_simpler(entry.createdAt)}</span>
			</div>
			<p class="ai_inbox_context"><TranslateText key="FRONTAI_INBOX_CONTEXT"/>{" "}{entry.contextName}</p>
			<p class="ai_inbox_alert_message">{entry.message}</p>
			<div class="ai_inbox_entry_actions">
				<button
					type="button"
					disabled=move || busyEntry.get().is_some()
					on:click=dismissFn
				><TranslateText key="FRONTAI_INBOX_DISMISS"/></button>
			</div>
		</li>
	}.into_any();
}

fn textArgument_get(arguments: &[AiNamedValue],id: &str) -> Result<String,AiInboxError>
{
	return arguments.iter().find(|argument| argument.id == id)
		.and_then(|argument| match &argument.value
		{
			AiValue::Text(value) => Some(value.clone()),
			_ => None,
		})
		.ok_or(AiInboxError::InvalidDocument);
}

fn identifier_isValid(value: &str) -> bool
{
	return !value.is_empty()
		&& value.len() <= AI_INBOX_CONTEXT_ID_MAXIMUM_BYTES
		&& value.bytes().all(|character| character.is_ascii_alphanumeric() || matches!(character,b'.' | b'_' | b'-'));
}

fn boundedOpaque_isValid(value: &str,maximumBytes: usize) -> bool
{
	return !value.is_empty()
		&& value.len() <= maximumBytes
		&& value.trim() == value
		&& !value.chars().any(char::is_control);
}

fn boundedText_isValid(value: &str,maximumBytes: usize) -> bool
{
	return !value.trim().is_empty()
		&& value.len() <= maximumBytes
		&& !value.chars().any(|character| character == '\0');
}

#[cfg(test)]
mod tests
{
	use super::*;
	use crate::front::ai::automation::{AiAutomationSource,AiAutomationTarget,AiAutomationTargetAction};

	fn context_get() -> AiAutomationContext
	{
		let mut context = AiAutomationContext::new(
			AiAutomationSource {
				moduleId: ModuleID {id: "mail-source".to_string()},
				event: "mail.new".to_string(),
				fields: vec!["subject".to_string()],
			},
			AiAutomationTarget {
				moduleId: ModuleID {id: "calendar-target".to_string()},
				actions: vec![AiAutomationTargetAction {
					action: "calendar.event.create".to_string(),
					confirmation: AiConfirmationPolicy::Confirm,
					fixedArguments: Vec::new(),
				}],
			},
		);
		context.name = "Appointments".to_string();
		context.enabled = true;
		return context;
	}

	fn action_get() -> AiValidatedAction
	{
		return AiValidatedAction {
			actionKey: "action-key".to_string(),
			executionId: "execution-key".to_string(),
			targetModuleId: ModuleID {id: "calendar-target".to_string()},
			action: "calendar.event.create".to_string(),
			arguments: vec![AiNamedValue {id: "title".to_string(),value: AiValue::Text("Appointment".to_string())}],
			confirmation: AiConfirmationPolicy::Confirm,
		};
	}

	#[test]
	fn actionRoundTripPreservesTheDeferredRequest()
	{
		let action = AiInboxAction::new(&context_get(),"CALENDAR",action_get(),100).unwrap();
		let document = AiInboxDocument {entries: vec![AiInboxEntry::Action(action.clone())],..Default::default()};
		let restored = AiInboxDocument::deserialize(&document.serialize().unwrap()).unwrap();

		assert_eq!(restored,document);
		assert_eq!(restored.entries[0].id_get(),action.id_get());
	}

	#[test]
	fn mutationsMergeDistinctEntriesAndNeverOverwriteAnId()
	{
		let first = AiInboxEntry::Action(AiInboxAction::new(&context_get(),"CALENDAR",action_get(),100).unwrap());
		let mut secondAction = action_get();
		secondAction.actionKey = "second-action".to_string();
		let second = AiInboxEntry::Action(AiInboxAction::new(&context_get(),"CALENDAR",secondAction,101).unwrap());
		let mut document = AiInboxDocument::default();

		assert!(AiInboxMutation::Add(vec![first.clone()]).apply(&mut document).unwrap());
		assert!(AiInboxMutation::Add(vec![second]).apply(&mut document).unwrap());
		assert!(!AiInboxMutation::Add(vec![first.clone()]).apply(&mut document).unwrap());
		let mut conflicting = first;
		if let AiInboxEntry::Action(action) = &mut conflicting
		{
			action.contextName = "Other".to_string();
		}
		assert_eq!(AiInboxMutation::Add(vec![conflicting]).apply(&mut document),Err(AiInboxError::InvalidDocument));
		assert!(AiInboxMutation::Remove("action-key".to_string()).apply(&mut document).unwrap());
		assert_eq!(document.entries.len(),1);
	}

	#[test]
	fn alertCapabilityIsExplicitAndAlwaysAutomatic()
	{
		let holder = AiInboxHolder::new();
		let module = holder.capabilities_get();
		let capability = module.catalog.action_get(AI_INBOX_ALERT_ACTION).unwrap();

		assert!(module.grant.action_allows(AI_INBOX_ALERT_ACTION));
		assert_eq!(capability.forcedConfirmation,Some(AiConfirmationPolicy::Automatic));
		assert_eq!(capability.confirmation_validate(AiConfirmationPolicy::Confirm),Err(AiAutomationError::PermissionDenied));
		assert!(capability.confirmation_validate(AiConfirmationPolicy::Automatic).is_ok());
	}

	#[test]
	fn alertActionBecomesAStoredPlainTextNotice()
	{
		let mut action = action_get();
		action.targetModuleId = ModuleID {id: AiInboxHolder::MODULE_ID.to_string()};
		action.action = AI_INBOX_ALERT_ACTION.to_string();
		action.confirmation = AiConfirmationPolicy::Automatic;
		action.arguments = vec![
			AiNamedValue {id: "title".to_string(),value: AiValue::Text("Important message".to_string())},
			AiNamedValue {id: "message".to_string(),value: AiValue::Text("Review it later.".to_string())},
			AiNamedValue {id: "level".to_string(),value: AiValue::Text("attention".to_string())},
		];

		let alert = AiInboxAlert::fromAction(&context_get(),&action,120).unwrap();

		assert_eq!(alert.id,"action-key");
		assert_eq!(alert.title,"Important message");
		assert_eq!(alert.message,"Review it later.");
		assert_eq!(alert.level,AiInboxAlertLevel::Attention);
	}

	#[test]
	fn unknownFieldsVersionsAndBoundsFailClosed()
	{
		assert_eq!(AiInboxDocument::deserialize(r#"{"version":2,"entries":[]}"#),Err(AiInboxError::UnsupportedVersion));
		assert_eq!(AiInboxDocument::deserialize(r#"{"version":1,"entries":[],"unknown":true}"#),Err(AiInboxError::InvalidDocument));
		let mut documentWithEntry = AiInboxDocument::default();
		documentWithEntry.entries.push(AiInboxEntry::Action(
			AiInboxAction::new(&context_get(),"CALENDAR",action_get(),100).unwrap(),
		));
		let mut value = serde_json::to_value(documentWithEntry).unwrap();
		value["entries"][0].as_object_mut().unwrap()
			.insert("unknown".to_string(),serde_json::Value::Bool(true));
		assert_eq!(AiInboxDocument::deserialize(&serde_json::to_string(&value).unwrap()),Err(AiInboxError::InvalidDocument));
		let mut document = AiInboxDocument::default();
		document.entries = (0..=AI_INBOX_ENTRY_MAXIMUM).map(|index| {
			let mut action = action_get();
			action.actionKey = format!("action-{index}");
			return AiInboxEntry::Action(AiInboxAction::new(&context_get(),"CALENDAR",action,index as i64).unwrap());
		}).collect();
		assert_eq!(document.validate(),Err(AiInboxError::CapacityExceeded));
	}
}
