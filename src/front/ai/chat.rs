mod domain;
mod view;

use crate::api::modules::components::{ModuleContent,ModuleID};
use crate::front::modules::components::Cache;
pub(crate) use domain::ChatDocument;
use domain::ChatError;
use leptos::prelude::{ArcRwSignal,GetUntracked,Set,Update};

pub(super) use view::AiChatView;

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub(super) enum ChatFeedbackKind
{
	Warning,
	Error,
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(super) struct ChatFeedback
{
	pub(super) conversationId: Option<String>,
	pub(super) key: &'static str,
	pub(super) kind: ChatFeedbackKind,
}

#[derive(Clone,Debug,Eq,PartialEq)]
pub(super) struct ChatPendingRequest
{
	pub(super) conversationId: String,
	pub(super) generation: u64,
}

#[derive(Clone,Debug,Default)]
pub(crate) struct ChatRuntime
{
	pub(super) selectedConversationId: Option<String>,
	pub(super) pending: Option<ChatPendingRequest>,
	pub(super) feedback: Option<ChatFeedback>,
	pub(super) contextTruncatedFor: Option<String>,
	pub(super) responseReady: bool,
	workspaceOpen: bool,
	requestGeneration: u64,
}

impl ChatRuntime
{
	fn fromDocument(document: &ChatDocument) -> Self
	{
		return Self {
			selectedConversationId: document.selectedFallback_get(),
			..Default::default()
		};
	}

	pub(crate) fn workspace_open(&mut self)
	{
		self.workspaceOpen = true;
		self.responseReady = false;
	}

	pub(crate) fn workspace_close(&mut self)
	{
		self.workspaceOpen = false;
	}

	pub(super) fn selection_reconcile(&mut self,document: &ChatDocument)
	{
		if (self.selectedConversationId.as_deref()
			.and_then(|id| document.conversation_get(id))
			.is_none())
		{
			self.selectedConversationId = document.selectedFallback_get();
		}
	}

	pub(super) fn request_start(&mut self,conversationId: String,truncated: bool) -> u64
	{
		self.requestGeneration = self.requestGeneration.wrapping_add(1);
		let generation = self.requestGeneration;
		self.pending = Some(ChatPendingRequest {
			conversationId: conversationId.clone(),
			generation,
		});
		self.feedback = None;
		self.responseReady = false;
		self.contextTruncatedFor = truncated.then_some(conversationId);
		return generation;
	}

	pub(super) fn request_isCurrent(&self,conversationId: &str,generation: u64) -> bool
	{
		return self.pending.as_ref()
			.map(|pending| pending.conversationId == conversationId && pending.generation == generation)
			.unwrap_or(false);
	}

	pub(super) fn request_success(&mut self,conversationId: &str,generation: u64)
	{
		if (!self.request_isCurrent(conversationId,generation))
		{
			return;
		}
		self.pending = None;
		self.feedback = None;
		self.responseReady = !self.workspaceOpen;
	}

	pub(super) fn request_error(&mut self,conversationId: &str,generation: u64,key: &'static str)
	{
		if (!self.request_isCurrent(conversationId,generation))
		{
			return;
		}
		self.pending = None;
		self.feedback = Some(ChatFeedback {
			conversationId: Some(conversationId.to_string()),
			key,
			kind: ChatFeedbackKind::Error,
		});
		self.responseReady = !self.workspaceOpen;
	}

	pub(super) fn request_cancel(&mut self)
	{
		let Some(pending) = self.pending.take() else {return;};
		self.requestGeneration = self.requestGeneration.wrapping_add(1);
		self.feedback = Some(ChatFeedback {
			conversationId: Some(pending.conversationId),
			key: "MODULE_CHAT_REQUEST_CANCELLED",
			kind: ChatFeedbackKind::Warning,
		});
	}

	pub(super) fn conversation_removed(&mut self,document: &ChatDocument,conversationId: &str)
	{
		if (self.pending.as_ref().map(|pending| pending.conversationId.as_str()) == Some(conversationId))
		{
			self.requestGeneration = self.requestGeneration.wrapping_add(1);
			self.pending = None;
		}
		if (self.feedback.as_ref().and_then(|feedback| feedback.conversationId.as_deref()) == Some(conversationId))
		{
			self.feedback = None;
		}
		if (self.contextTruncatedFor.as_deref() == Some(conversationId))
		{
			self.contextTruncatedFor = None;
		}
		self.selection_reconcile(document);
	}

	fn lifecycle_reset(&mut self,document: &ChatDocument)
	{
		let requestGeneration = self.requestGeneration.wrapping_add(1);
		*self = Self::fromDocument(document);
		self.requestGeneration = requestGeneration;
	}

	fn document_reload(&mut self,document: &ChatDocument)
	{
		let workspaceOpen = self.workspaceOpen;
		self.lifecycle_reset(document);
		self.workspaceOpen = workspaceOpen;
	}

	fn feedback_set(&mut self,key: &'static str,kind: ChatFeedbackKind)
	{
		self.feedback = Some(ChatFeedback {conversationId: None,key,kind});
		self.responseReady = !self.workspaceOpen;
	}
}

pub(crate) struct AiChatLegacy
{
	id: ModuleID,
	document: ChatDocument,
}

pub(crate) struct AiChatMigration
{
	pub(crate) content: Option<ModuleContent>,
	pub(crate) legacyIds: Vec<ModuleID>,
}

pub(crate) struct AiChatHolder
{
	document: ArcRwSignal<ChatDocument>,
	runtime: ArcRwSignal<ChatRuntime>,
	update: Cache,
	sended: Cache,
	persisted: bool,
	saveRunning: bool,
	migrationRunning: bool,
	migrationBlocked: bool,
	legacy: Vec<AiChatLegacy>,
	migrationDocument: Option<ChatDocument>,
}

impl AiChatHolder
{
	pub(crate) const MODULE_ID: &'static str = "AI_CHAT";
	pub(crate) const MODULE_NAME: &'static str = "AI_CHAT";
	pub(crate) const LEGACY_MODULE_NAME: &'static str = "CHAT";

	pub(crate) fn new() -> Self
	{
		let cache = Cache::default();
		return Self {
			document: ArcRwSignal::new(ChatDocument::default()),
			runtime: ArcRwSignal::new(ChatRuntime::default()),
			update: cache.clone(),
			sended: cache,
			persisted: false,
			saveRunning: false,
			migrationRunning: false,
			migrationBlocked: false,
			legacy: Vec::new(),
			migrationDocument: None,
		};
	}

	pub(crate) fn document_get(&self) -> ArcRwSignal<ChatDocument>
	{
		return self.document.clone();
	}

	pub(crate) fn runtime_get(&self) -> ArcRwSignal<ChatRuntime>
	{
		return self.runtime.clone();
	}

	pub(crate) fn id_get(&self) -> ModuleID
	{
		return ModuleID {id: Self::MODULE_ID.to_string()};
	}

	pub(crate) fn cache_time(&self) -> i64
	{
		return self.update.get();
	}

	pub(crate) fn cache_mustUpdate(&self) -> bool
	{
		return self.update.isNewer(&self.sended);
	}

	fn timestamp_next(&self) -> i64
	{
		return Cache::now().max(self.update.get().saturating_add(1));
	}

	pub(crate) fn import(&mut self,content: ModuleContent) -> Result<(),ChatError>
	{
		if (content.id.id != Self::MODULE_ID || content.typeModule != Self::MODULE_NAME)
		{
			return Err(ChatError::InvalidDocument);
		}
		let (document,purged) = ChatDocument::deserialize(&content.content,ChatDocument::now_get())?;
		self.document.set(document.clone());
		self.runtime.update(|runtime| runtime.lifecycle_reset(&document));
		self.update.update_from(content.timestamp);
		self.sended.update_from(content.timestamp);
		self.persisted = true;
		if (purged)
		{
			self.changed_mark();
		}
		return Ok(());
	}

	pub(crate) fn loaded_apply(&mut self,loaded: Self)
	{
		let document = loaded.document.get_untracked();
		self.document.set(document.clone());
		self.runtime.update(|runtime| runtime.document_reload(&document));
		self.update = loaded.update;
		self.sended = loaded.sended;
		self.persisted = loaded.persisted;
	}

	pub(crate) fn legacy_prepare(content: ModuleContent) -> Result<AiChatLegacy,ChatError>
	{
		if (content.typeModule != Self::LEGACY_MODULE_NAME || content.id.id == Self::MODULE_ID)
		{
			return Err(ChatError::InvalidDocument);
		}
		let (document,_) = ChatDocument::deserialize(&content.content,ChatDocument::now_get())?;
		return Ok(AiChatLegacy {id: content.id,document});
	}

	pub(crate) fn legacy_apply(&mut self,legacy: AiChatLegacy)
	{
		self.migrationBlocked = false;
		if let Some(existing) = self.legacy.iter_mut().find(|existing| existing.id == legacy.id)
		{
			*existing = legacy;
		}
		else
		{
			self.legacy.push(legacy);
		}
	}

	pub(crate) fn migration_isNeeded(&self) -> bool
	{
		return !self.legacy.is_empty()
			&& !self.saveRunning
			&& !self.migrationRunning
			&& !self.migrationBlocked;
	}

	pub(crate) fn migration_isRunning(&self) -> bool
	{
		return self.migrationRunning;
	}

	pub(crate) fn migration_begin(&mut self) -> bool
	{
		if (!self.migration_isNeeded())
		{
			return false;
		}
		self.migrationRunning = true;
		return true;
	}

	pub(crate) fn migration_prepare(&mut self) -> Result<AiChatMigration,ChatError>
	{
		let mut document = self.document.get_untracked();
		let mut changed = false;
		for legacy in &self.legacy
		{
			changed |= document.legacy_merge(&legacy.id.id,&legacy.document)?;
		}
		document.validate()?;
		let legacyIds = self.legacy.iter().map(|legacy| legacy.id.clone()).collect::<Vec<_>>();
		self.migrationDocument = Some(document.clone());
		if (!changed && self.persisted)
		{
			return Ok(AiChatMigration {content: None,legacyIds});
		}
		let timestamp = self.timestamp_next();
		self.update.update_from(timestamp);
		return Ok(AiChatMigration {
			content: Some(self.export_document(&document,timestamp)?),
			legacyIds,
		});
	}

	pub(crate) fn migration_saved_apply(&mut self,timestamp: Option<i64>)
	{
		if let Some(document) = self.migrationDocument.take()
		{
			self.document.set(document.clone());
			self.runtime.update(|runtime| runtime.selection_reconcile(&document));
		}
		if let Some(timestamp) = timestamp
		{
			self.sended.update_from(timestamp);
			self.persisted = true;
		}
	}

	pub(crate) fn migration_legacyRemoved(&mut self,id: &ModuleID)
	{
		self.legacy.retain(|legacy| &legacy.id != id);
	}

	pub(crate) fn migration_finish(&mut self,errorKey: Option<&'static str>)
	{
		self.migrationRunning = false;
		self.migrationDocument = None;
		self.migrationBlocked = errorKey.is_some();
		if let Some(errorKey) = errorKey
		{
			self.runtime.update(|runtime| runtime.feedback_set(errorKey,ChatFeedbackKind::Error));
		}
	}

	pub(crate) fn migration_remoteImport(&mut self,content: ModuleContent) -> Result<(),ChatError>
	{
		let migrationRunning = self.migrationRunning;
		let legacy = std::mem::take(&mut self.legacy);
		self.import(content)?;
		self.legacy = legacy;
		self.migrationRunning = migrationRunning;
		return Ok(());
	}

	pub(crate) fn changed_mark(&mut self)
	{
		let timestamp = self.timestamp_next();
		self.update.update_from(timestamp);
	}

	pub(crate) fn save_begin(&mut self) -> bool
	{
		if (self.saveRunning || self.migrationRunning)
		{
			return false;
		}
		self.saveRunning = true;
		return true;
	}

	pub(crate) fn save_prepare(&mut self) -> Result<Option<ModuleContent>,ChatError>
	{
		let purged = self.document.try_update(|document| document.expired_purge(ChatDocument::now_get()))
			.unwrap_or(false);
		if (purged)
		{
			let document = self.document.get_untracked();
			self.runtime.update(|runtime| runtime.selection_reconcile(&document));
			self.changed_mark();
		}
		if (!self.cache_mustUpdate())
		{
			return Ok(None);
		}
		let document = self.document.get_untracked();
		return Ok(Some(self.export_document(&document,self.update.get())?));
	}

	pub(crate) fn save_succeeded(&mut self,timestamp: i64)
	{
		if (timestamp > self.sended.get())
		{
			self.sended.update_from(timestamp);
		}
		self.persisted = true;
	}

	pub(crate) fn save_finish(&mut self,errorKey: Option<&'static str>)
	{
		self.saveRunning = false;
		if let Some(errorKey) = errorKey
		{
			self.runtime.update(|runtime| runtime.feedback_set(errorKey,ChatFeedbackKind::Error));
		}
	}

	pub(crate) fn save_remoteImport(&mut self,content: ModuleContent) -> Result<(),ChatError>
	{
		let workspaceOpen = self.runtime.get_untracked().workspaceOpen;
		self.import(content)?;
		self.runtime.update(|runtime| runtime.workspaceOpen = workspaceOpen);
		return Ok(());
	}

	fn export_document(&self,document: &ChatDocument,timestamp: i64) -> Result<ModuleContent,ChatError>
	{
		return Ok(ModuleContent {
			id: self.id_get(),
			typeModule: Self::MODULE_NAME.to_string(),
			timestamp,
			content: document.serialize()?,
			..Default::default()
		});
	}
}

#[cfg(test)]
mod tests
{
	use super::*;
	use leptos::prelude::{GetUntracked,Owner,Update};

	#[test]
	fn stableContentRoundTripKeepsConversations()
	{
		let owner = Owner::new();
		owner.with(|| {
			let mut holder = AiChatHolder::new();
			let document = holder.document_get();
			let now = ChatDocument::now_get();
			document.update(|document| {
				let id = document.conversation_create(now).unwrap();
				document.userMessage_add(&id,"hello".to_string(),now + 1).unwrap();
				document.assistantMessage_add(&id,"world".to_string(),now + 2).unwrap();
			});
			holder.changed_mark();
			let exported = holder.save_prepare().unwrap().unwrap();
			let mut imported = AiChatHolder::new();
			imported.import(exported).unwrap();

			assert_eq!(imported.document_get().get_untracked(),document.get_untracked());
		});
		owner.cleanup();
	}

	#[test]
	fn cancelledAndReplacedGenerationRejectLateResponse()
	{
		let mut runtime = ChatRuntime::default();
		let generation = runtime.request_start("conversation".to_string(),false);
		assert!(runtime.request_isCurrent("conversation",generation));
		runtime.request_cancel();
		assert!(!runtime.request_isCurrent("conversation",generation));

		let replacement = runtime.request_start("conversation".to_string(),true);
		assert_ne!(generation,replacement);
		assert!(!runtime.request_isCurrent("conversation",generation));
		assert!(runtime.request_isCurrent("conversation",replacement));
	}

	#[test]
	fn documentReloadCannotReuseAnInflightGeneration()
	{
		let mut runtime = ChatRuntime::default();
		let staleGeneration = runtime.request_start("conversation".to_string(),false);
		runtime.document_reload(&ChatDocument::default());
		let currentGeneration = runtime.request_start("conversation".to_string(),false);

		assert_ne!(staleGeneration,currentGeneration);
		assert!(!runtime.request_isCurrent("conversation",staleGeneration));
		assert!(runtime.request_isCurrent("conversation",currentGeneration));
	}

	#[test]
	fn closingWorkspaceKeepsRequestAndReportsItsCompletion()
	{
		let mut runtime = ChatRuntime::default();
		runtime.workspace_open();
		let generation = runtime.request_start("conversation".to_string(),false);
		runtime.workspace_close();

		assert!(runtime.request_isCurrent("conversation",generation));
		runtime.request_success("conversation",generation);
		assert!(runtime.responseReady);

		runtime.workspace_open();
		assert!(!runtime.responseReady);
	}

	#[test]
	fn legacyMigrationAppliesOnlyAfterStableSaveAcknowledgement()
	{
		let owner = Owner::new();
		owner.with(|| {
			let now = ChatDocument::now_get();
			let mut legacyDocument = ChatDocument::default();
			legacyDocument.conversation_create(now).unwrap();
			let legacyContent = ModuleContent {
				id: ModuleID {id: "legacy-chat".to_string()},
				typeModule: AiChatHolder::LEGACY_MODULE_NAME.to_string(),
				content: legacyDocument.serialize().unwrap(),
				..Default::default()
			};

			let mut holder = AiChatHolder::new();
			holder.legacy_apply(AiChatHolder::legacy_prepare(legacyContent).unwrap());
			let mut loadedStable = AiChatHolder::new();
			loadedStable.import(ModuleContent {
				id: ModuleID {id: AiChatHolder::MODULE_ID.to_string()},
				typeModule: AiChatHolder::MODULE_NAME.to_string(),
				content: ChatDocument::default().serialize().unwrap(),
				timestamp: 4,
				..Default::default()
			}).unwrap();
			holder.loaded_apply(loadedStable);

			assert!(holder.migration_begin());
			let migration = holder.migration_prepare().unwrap();
			assert!(holder.document_get().get_untracked().conversations.is_empty());
			let prepared = migration.content.unwrap();
			assert_eq!(ChatDocument::deserialize(&prepared.content,now).unwrap().0.conversations.len(),1);

			holder.migration_saved_apply(Some(prepared.timestamp));
			assert_eq!(holder.document_get().get_untracked().conversations.len(),1);
			assert_eq!(migration.legacyIds.len(),1);
		});
		owner.cleanup();
	}

	#[test]
	fn stableReloadKeepsTheSignalsExposedToAnOpenWorkspace()
	{
		let owner = Owner::new();
		owner.with(|| {
			let mut holder = AiChatHolder::new();
			let exposedDocument = holder.document_get();
			let exposedRuntime = holder.runtime_get();
			exposedRuntime.update(|runtime| runtime.workspace_open());

			let mut loaded = AiChatHolder::new();
			loaded.import(ModuleContent {
				id: ModuleID {id: AiChatHolder::MODULE_ID.to_string()},
				typeModule: AiChatHolder::MODULE_NAME.to_string(),
				content: ChatDocument::default().serialize().unwrap(),
				timestamp: 8,
				..Default::default()
			}).unwrap();
			holder.loaded_apply(loaded);

			exposedDocument.update(|document| {
				document.conversation_create(ChatDocument::now_get()).unwrap();
			});
			assert_eq!(holder.document_get().get_untracked().conversations.len(),1);
			assert!(holder.runtime_get().get_untracked().workspaceOpen);
		});
		owner.cleanup();
	}

	#[test]
	fn failedMigrationWaitsForAnotherLegacyImportBeforeRetrying()
	{
		let owner = Owner::new();
		owner.with(|| {
			let legacyContent = ModuleContent {
				id: ModuleID {id: "legacy-chat".to_string()},
				typeModule: AiChatHolder::LEGACY_MODULE_NAME.to_string(),
				content: ChatDocument::default().serialize().unwrap(),
				..Default::default()
			};
			let mut holder = AiChatHolder::new();
			holder.legacy_apply(AiChatHolder::legacy_prepare(legacyContent.clone()).unwrap());
			assert!(holder.migration_begin());
			holder.migration_finish(Some("MODULE_CHAT_MIGRATION_FAILED"));
			assert!(!holder.migration_isNeeded());

			holder.legacy_apply(AiChatHolder::legacy_prepare(legacyContent).unwrap());
			assert!(holder.migration_isNeeded());
		});
		owner.cleanup();
	}
}
