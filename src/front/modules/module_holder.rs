use std::cell::RefCell;
use std::collections::{HashMap,VecDeque};
use std::sync::Arc;
use leptoaster::ToasterContext;
use leptos::prelude::{ArcRwSignal, Owner, Set, Update, With, WithUntracked};
use leptos::reactive::spawn_local_scoped_with_cancellation;
use crate::api::modules::{API_module_remove, API_module_retrieve, API_module_update, API_module_updateIfCurrent, API_modules_retrieve, API_modules_update, ModuleApiError, ModuleReturnRetrieve, ModuleReturnUpdate};
use crate::api::modules::components::{ApiModulesID, ModuleContent, ModuleID};
use crate::front::ai::{AiConfigDocument,AiConfigError,AiConfigHolder,AiConfigSaveError};
use crate::front::ai::automation::{
	AiActionApplyResult,AiActionFuture,AiActionPersistence,AiAutomationCapable,AiAutomationEngine,AiAutomationError,
	AiAutomationEvent,AiAutomationHistoryEntry,AiConfirmationPolicy,AiExposureFuture,AiExposureRequest,
	AiModuleCapabilities,AiEventReservation,AiValidatedAction,AI_AUTOMATION_QUEUE_MAXIMUM,
};
use crate::front::ai::automation::engine::{AiExecutionOutcome,AiQueuedExecution};
use crate::front::ai::automation::runtime::completion_get;
use crate::front::ai::chat::{AiChatHolder,ChatDocument,ChatRuntime};
use crate::front::ai::inbox::{
	AiInboxAction,AiInboxAlert,AiInboxEntry,AiInboxError,AiInboxHolder,AiInboxMutation,
	AI_INBOX_ALERT_ACTION,
};
use crate::front::modules::components::{API_return_apply, ApiCall, Backable, BoxFuture, Cacheable, ModuleName, PausableStocker, RefreshTime};
use crate::front::modules::link::LinksHolder;
use crate::front::modules::module_actions;
use crate::front::modules::module_positions::ModulePositions;
use crate::front::modules::module_type::ModuleType;
use crate::front::utils::all_front_enum::{AllFrontErrorEnum, AllFrontUIEnum};
use crate::front::utils::toaster_helpers;
use crate::front::utils::toaster_helpers::{toastingErr,toastingInfo,toastingSuccess};
use crate::front::utils::users_data::{ClientCryptoContext, ClientState};
use time::OffsetDateTime;

thread_local! {
    static MODULE_HOLDER_SINGLETON: RefCell<Option<ArcRwSignal<ModuleHolder>>> =
        const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ModuleHolderEpoch(u64);

struct ModuleRefreshTask
{
	owner: Owner,
	futures: Vec<BoxFuture>,
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
enum AiEventReservationResult
{
	Proceed,
	AlreadyHandled,
}

impl ModuleRefreshTask
{
	fn spawn(self)
	{
		self.owner.with(|| {
			spawn_local_scoped_with_cancellation(async move {
				for oneFuture in self.futures
				{
					oneFuture.await;
				}
			});
		});
	}
}

#[cfg(test)]
mod tests
{
	use std::sync::Arc;
	use std::sync::atomic::{AtomicBool, Ordering};

	use crate::api::modules::ModuleApiError;
	use crate::api::modules::components::{ModuleContent, ModuleID};
	use crate::front::ai::{AiConfigDocument,AiConfigHolder,AiProfile,AiProvider};
	use crate::front::ai::automation::{
		AiAutomationContext,AiAutomationEvent,AiAutomationHistoryEntry,AiAutomationSource,AiAutomationTarget,
		AiAutomationTargetAction,AiConfirmationPolicy,AiNamedValue,AiValidatedAction,AiValue,
		AiEventCausation,
	};
	use crate::front::ai::chat::{AiChatHolder,ChatDocument};
	use crate::front::ai::inbox::{AiInboxAction,AiInboxDocument,AiInboxEntry,AiInboxHolder};
	use crate::front::modules::components::{API_return_apply,PausableStocker};
	use crate::front::modules::module_actions::ModuleActionFn;
	use crate::front::modules::module_positions::ModulePositions;
	use crate::front::modules::module_type::ModuleType;
	use crate::front::modules::todo::Todo;
	use crate::front::utils::all_front_enum::AllFrontErrorEnum;
	use crate::front::utils::users_data::ClientCryptoContext;
	use leptoaster::ToasterContext;
	use leptos::prelude::{ArcRwSignal,GetUntracked,Owner};

	use super::ModuleHolder;

	#[test]
	fn networkError_authRequiredMarksLocalSessionInvalid()
	{
		let mut result = API_return_apply::default();
		ModuleHolder::network_error_apply(&mut result, ModuleApiError::AUTH_REQUIRED);

		assert!(result.authenticationRequired);
		assert!(matches!(result.error.as_slice(), [AllFrontErrorEnum::SESSION_EXPIRED]));
	}

	#[test]
	fn moduleUploadPreparation_containsOnlyEncryptedContent()
	{
		let holder = ModuleHolder::new();
		let crypto = ClientCryptoContext::test_get();
		let prepared = holder.network_modules_update_prepare(&crypto).unwrap();

		assert!(!prepared.is_empty());
		assert!(prepared.iter().all(|content| crypto.decrypt(&content.content).is_ok()));
		assert!(prepared.iter().all(|content| content.typeModule != AiConfigHolder::MODULE_NAME));
		assert!(prepared.iter().all(|content| content.typeModule != AiChatHolder::MODULE_NAME));
	}

	#[test]
	fn aiChatUploadPreparation_usesStableClientEncryption()
	{
		let mut holder = ModuleHolder::new();
		holder._aiChat.changed_mark();
		let crypto = ClientCryptoContext::test_get();
		let mut chat = holder._aiChat.save_prepare().unwrap().unwrap();
		ModuleHolder::export_crypt_content(&mut chat,&crypto).unwrap();

		assert!(!chat.content.contains("conversations"));
		let plaintext = crypto.decrypt(&chat.content).unwrap();
		assert!(plaintext.contains("\"conversations\""));
		assert_eq!(chat.id.id,AiChatHolder::MODULE_ID);
	}

	#[test]
	fn aiConfiguration_usesStableEncryptedSpecialContent()
	{
		let holder = ModuleHolder::new();
		let crypto = ClientCryptoContext::test_get();
		let mut document = AiConfigDocument::default();
		document.profile = Some(AiProfile {
				provider: AiProvider::OpenAI,
				model: "gpt-test".to_string(),
				credential: "private-provider-key".to_string(),
				baseUrl: String::new(),
			maxOutputTokens: 512,
		});
		let mut context = AiAutomationContext::new(
			AiAutomationSource {
				moduleId: ModuleID {id: "source-module".to_string()},
				event: "item.created".to_string(),
				fields: vec!["title".to_string()],
			},
			AiAutomationTarget {
				moduleId: ModuleID {id: "target-module".to_string()},
				actions: vec![AiAutomationTargetAction {
					action: "item.add".to_string(),
					confirmation: AiConfirmationPolicy::Confirm,
					fixedArguments: Vec::new(),
				}],
			},
		);
		context.name = "Private automation".to_string();
		context.instructions = "Use the exposed title.".to_string();
		document.contexts.push(context);
		let action = AiValidatedAction {
			actionKey: "private-action".to_string(),
			executionId: "private-execution".to_string(),
			targetModuleId: ModuleID {id: "target-module".to_string()},
			action: "calendar.event.create".to_string(),
			arguments: vec![AiNamedValue {
				id: "title".to_string(),value: AiValue::Text("Private appointment".to_string()),
			}],
			confirmation: AiConfirmationPolicy::Confirm,
		};
		document.automationHistory_add(AiAutomationHistoryEntry::new(
			"Private automation","CALENDAR",&action,100,
		).unwrap()).unwrap();
		let mut content = holder._aiConfig.export_document(&document,holder._aiConfig.timestamp_next()).unwrap();
		ModuleHolder::export_crypt_content(&mut content,&crypto).unwrap();

		assert_eq!(content.id.id,AiConfigHolder::MODULE_ID);
		assert_eq!(content.typeModule,AiConfigHolder::MODULE_NAME);
		assert!(!content.content.contains("private-provider-key"));
		assert!(!content.content.contains("Private automation"));
		assert!(!content.content.contains("Private appointment"));
		assert!(!content.content.contains("Use the exposed title."));

		ModuleHolder::import_decrypt_content(&mut content,&crypto).unwrap();
		let mut restored = AiConfigHolder::new();
		restored.import(content).unwrap();
		assert_eq!(restored.document_get(),document);
	}

	#[test]
	fn aiInbox_usesStableEncryptedSpecialContent()
	{
		let holder = ModuleHolder::new();
		let crypto = ClientCryptoContext::test_get();
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
		context.name = "Private automation".to_string();
		context.enabled = true;
		let action = AiValidatedAction {
			actionKey: "private-action".to_string(),
			executionId: "private-execution".to_string(),
			targetModuleId: ModuleID {id: "calendar-target".to_string()},
			action: "calendar.event.create".to_string(),
			arguments: vec![AiNamedValue {
				id: "title".to_string(),value: AiValue::Text("Private appointment".to_string()),
			}],
			confirmation: AiConfirmationPolicy::Confirm,
		};
		let mut document = AiInboxDocument::default();
		document.entries.push(AiInboxEntry::Action(
			AiInboxAction::new(&context,"CALENDAR",action,100).unwrap(),
		));
		let mut content = holder._aiInbox.export_document(
			&document,holder._aiInbox.timestamp_next(),
		).unwrap();
		ModuleHolder::export_crypt_content(&mut content,&crypto).unwrap();

		assert_eq!(content.id.id,AiInboxHolder::MODULE_ID);
		assert_eq!(content.typeModule,AiInboxHolder::MODULE_NAME);
		assert!(!content.content.contains("Private automation"));
		assert!(!content.content.contains("Private appointment"));

		ModuleHolder::import_decrypt_content(&mut content,&crypto).unwrap();
		let mut restored = AiInboxHolder::new();
		restored.import(content).unwrap();
		assert_eq!(restored.document_get(),document);
	}

	#[test]
	fn initialRetrieveAlwaysRequestsStableAiContents()
	{
		let holder = ModuleHolder::new();
		let requested = holder.network_modules_retrieve_prepare(true);
		let aiRequest = requested.iter().find(|module| module.key.id == AiConfigHolder::MODULE_ID).unwrap();
		let chatRequest = requested.iter().find(|module| module.key.id == AiChatHolder::MODULE_ID).unwrap();
		let inboxRequest = requested.iter().find(|module| module.key.id == AiInboxHolder::MODULE_ID).unwrap();

		assert_eq!(aiRequest.timestamp,i64::MIN);
		assert_eq!(chatRequest.timestamp,i64::MIN);
		assert_eq!(inboxRequest.timestamp,i64::MIN);
	}

	#[test]
	fn reservedAiIdentityRejectsAnotherModuleTypeBeforeDecryption()
	{
		let crypto = ClientCryptoContext::test_get();
		let mut result = API_return_apply::default();
		let content = ModuleContent {
			id: ModuleID {id: AiConfigHolder::MODULE_ID.to_string()},
			typeModule: "TODO".to_string(),
			content: "not encrypted".to_string(),
			..Default::default()
		};

		assert!(!ModuleHolder::module_inner_retrieve(
			&mut result,
			content,
			ModuleID {id: AiConfigHolder::MODULE_ID.to_string()},
			&crypto,
		));
		assert_eq!(result.error,vec![AllFrontErrorEnum::AI_CONFIG_INVALID]);
	}

	#[test]
	fn reservedAiChatIdentityRejectsAnotherModuleTypeBeforeDecryption()
	{
		let crypto = ClientCryptoContext::test_get();
		let mut result = API_return_apply::default();
		let content = ModuleContent {
			id: ModuleID {id: AiChatHolder::MODULE_ID.to_string()},
			typeModule: "TODO".to_string(),
			content: "not encrypted".to_string(),
			..Default::default()
		};

		assert!(!ModuleHolder::module_inner_retrieve(
			&mut result,
			content,
			ModuleID {id: AiChatHolder::MODULE_ID.to_string()},
			&crypto,
		));
		assert_eq!(result.error,vec![AllFrontErrorEnum::AI_CHAT_INVALID]);
	}

	#[test]
	fn reservedAiInboxIdentityRejectsAnotherModuleTypeBeforeDecryption()
	{
		let crypto = ClientCryptoContext::test_get();
		let mut result = API_return_apply::default();
		let content = ModuleContent {
			id: ModuleID {id: AiInboxHolder::MODULE_ID.to_string()},
			typeModule: "TODO".to_string(),
			content: "not encrypted".to_string(),
			..Default::default()
		};

		assert!(!ModuleHolder::module_inner_retrieve(
			&mut result,
			content,
			ModuleID {id: AiInboxHolder::MODULE_ID.to_string()},
			&crypto,
		));
		assert_eq!(result.error,vec![AllFrontErrorEnum::AI_INBOX_INVALID]);
	}

	#[test]
	fn localCryptoError_doesNotScheduleModuleMutation()
	{
		let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
		let result = runtime.block_on(ModuleHolder::network_localError_get(AllFrontErrorEnum::CRYPTO_CONTEXT_MISSING));

		assert!(matches!(result.error.as_slice(), [AllFrontErrorEnum::CRYPTO_CONTEXT_MISSING]));
		assert!(result.retrieve.is_empty());
		assert!(result.update.is_empty());
		assert!(result.moduleIdToRefresh.is_empty());
	}

	#[test]
	fn lifecycleClose_resetsDecryptedStateAndOwnedResources()
	{
		let parentOwner = Owner::new();
		parentOwner.with(|| {
			let mut holder = ModuleHolder::new();
			let (epoch, previousOwner) = holder.lifecycle_open_inner();
			assert!(previousOwner.is_none());

			let moduleId = ModuleID {id: "account-a-module".to_string()};
			holder._links.id_set(ModuleID {id: "account-a-links".to_string()});
			holder._blocks.insert(
				moduleId.clone(),
				ArcRwSignal::new(ModulePositions::new(ModuleType::TODO(Todo::new()))),
			);
			holder._crons.insert(moduleId, PausableStocker::test_paused());
			holder._moduleActions = Some(ModuleActionFn::test_get(epoch));
			holder._blockNb = 7;
			holder._aiConfigReady = true;
			let mut aiConfig = AiConfigDocument::default();
			aiConfig.profile = Some(AiProfile {
				provider: AiProvider::OpenAI,
				model: "gpt-test".to_string(),
				credential: "account-a-private-key".to_string(),
				baseUrl: String::new(),
				maxOutputTokens: 512,
			});
			holder._aiConfig.import(ModuleContent {
				id: ModuleID {id: AiConfigHolder::MODULE_ID.to_string()},
				typeModule: AiConfigHolder::MODULE_NAME.to_string(),
				content: aiConfig.serialize().unwrap(),
				..Default::default()
			}).unwrap();
			holder._aiChatReady = true;
			holder._aiInboxReady = true;
			holder._aiChat.import(ModuleContent {
				id: ModuleID {id: AiChatHolder::MODULE_ID.to_string()},
				typeModule: AiChatHolder::MODULE_NAME.to_string(),
				content: r#"{"conversations":[]}"#.to_string(),
				..Default::default()
			}).unwrap();
			holder._aiInbox.import(ModuleContent {
				id: ModuleID {id: AiInboxHolder::MODULE_ID.to_string()},
				typeModule: AiInboxHolder::MODULE_NAME.to_string(),
				content: r#"{"version":1,"entries":[]}"#.to_string(),
				..Default::default()
			}).unwrap();
			holder._aiAutomationInbox.push_back(AiAutomationEvent::new(
				ModuleID {id: "account-a-module".to_string()},
				"item.created".to_string(),
				"event-1".to_string(),
				1,
				AiEventCausation::External,
			));
			holder._aiAutomationRunning = true;

			let ownerWasCleaned = Arc::new(AtomicBool::new(false));
			let ownerWasCleanedInner = ownerWasCleaned.clone();
			holder._taskOwner.as_ref().unwrap().with(|| {
				Owner::on_cleanup(move || ownerWasCleanedInner.store(true, Ordering::Relaxed));
			});

			let taskOwner = holder.lifecycle_close_inner().unwrap();
			assert!(!holder.lifecycle_epoch_isActive(epoch));
			assert!(holder._blocks.is_empty());
			assert!(holder._crons.is_empty());
			assert!(holder._moduleActions.is_none());
			assert_eq!(holder._blockNb, 0);
			assert_ne!(holder._links.id_get().id, "account-a-links");
			assert!(!holder._aiConfigReady);
			assert_eq!(holder._aiConfig.document_get(),AiConfigDocument::default());
			assert!(!holder._aiChatReady);
			assert_eq!(holder._aiChat.document_get().get_untracked(),ChatDocument::default());
			assert!(!holder._aiInboxReady);
			assert_eq!(holder._aiInbox.document_get(),AiInboxDocument::default());
			assert!(holder._aiAutomationInbox.is_empty());
			assert!(!holder._aiAutomationRunning);

			taskOwner.cleanup();
			assert!(ownerWasCleaned.load(Ordering::Relaxed));
		});
		parentOwner.cleanup();
	}

	#[test]
	fn accountTransition_rejectsOldCloseAndLateNetworkResult()
	{
		let parentOwner = Owner::new();
		parentOwner.with(|| {
			let mut holder = ModuleHolder::new();
			let (accountAEpoch, _) = holder.lifecycle_open_inner();
			holder._blockNb = 8;

			let (accountBEpoch, accountAOwner) = holder.lifecycle_open_inner();
			accountAOwner.unwrap().cleanup();
			assert!(!holder.lifecycle_epoch_isActive(accountAEpoch));
			assert!(holder.lifecycle_epoch_isActive(accountBEpoch));
			assert_eq!(holder._blockNb, 0);

			let (oldCloseApplied, oldOwner) = holder.lifecycle_closeIf_inner(accountAEpoch);
			assert!(!oldCloseApplied);
			assert!(oldOwner.is_none());
			assert!(holder.lifecycle_epoch_isActive(accountBEpoch));

			let mut staleResult = API_return_apply::default();
			staleResult.retrieve.push(Box::new(|holder| holder._blockNb = 13));
			holder.network_apply(accountAEpoch, staleResult, ToasterContext::default());
			assert_eq!(holder._blockNb, 0);

			let mut currentResult = API_return_apply::default();
			currentResult.retrieve.push(Box::new(|holder| holder._blockNb = 21));
			holder.network_apply(accountBEpoch, currentResult, ToasterContext::default());
			assert_eq!(holder._blockNb, 21);

			holder.lifecycle_close_inner().unwrap().cleanup();
		});
		parentOwner.cleanup();
	}

	#[test]
	fn networkSuspension_invalidatesInflightGeneration()
	{
		let parentOwner = Owner::new();
		parentOwner.with(|| {
			let mut holder = ModuleHolder::new();
			let (epoch, _) = holder.lifecycle_open_inner();
			let generation = holder.network_generation_get(epoch).unwrap();

			holder.network_suspend_inner();
			assert!(holder.network_generation_get(epoch).is_none());
			assert!(!holder.network_generation_isActive(epoch,generation));

			holder.network_resume_inner();
			let resumedGeneration = holder.network_generation_get(epoch).unwrap();
			assert_ne!(resumedGeneration,generation);
			assert!(holder.network_generation_isActive(epoch,resumedGeneration));
		});
		parentOwner.cleanup();
	}

	#[test]
	fn blocksView_ordersModulesFromTopToBottomThenLeftToRight()
	{
		let parentOwner = Owner::new();
		parentOwner.with(|| {
			let mut holder = ModuleHolder::new();
			for (id,position,depth) in [
				("lower",[0,100],0),
				("upper-right",[200,0],0),
				("upper-left-b",[0,0],2),
				("upper-left-a",[0,0],1),
			]
			{
				let moduleId = ModuleID {id: id.to_string()};
				let moduleContent = ModuleContent {
					id: moduleId.clone(),
					pos: position,
					depth,
					..Default::default()
				};
				holder._blocks.insert(
					moduleId,
					ArcRwSignal::new(ModulePositions::newFromModuleContent(moduleContent,ModuleType::TODO(Todo::new()))),
				);
			}

			let orderedIds = holder.blocks_view().into_iter()
				.map(|(id,_)| id.id)
				.collect::<Vec<_>>();

			assert_eq!(orderedIds,["upper-left-a","upper-left-b","upper-right","lower"]);
		});
		parentOwner.cleanup();
	}
}

pub struct ModuleHolder
{
	_links: LinksHolder,
	_aiConfig: AiConfigHolder,
	_aiConfigReady: bool,
	_aiChat: AiChatHolder,
	_aiChatReady: bool,
	_aiInbox: AiInboxHolder,
	_aiInboxReady: bool,
	_aiAutomation: AiAutomationEngine,
	_aiAutomationInbox: VecDeque<AiAutomationEvent>,
	_aiAutomationRunning: bool,
	_blocks: HashMap<ModuleID, ArcRwSignal<ModulePositions<ModuleType>>>,
	_crons: HashMap<ModuleID, PausableStocker>,
	_moduleActions: Option<module_actions::ModuleActionFn>,
	_blockNb: usize,
	_epochCounter: u64,
	_activeEpoch: Option<ModuleHolderEpoch>,
	_taskOwner: Option<Owner>,
	_networkGeneration: u64,
	_networkSuspended: bool,
}

impl ModuleHolder
{
	pub fn getSingleton() -> ArcRwSignal<ModuleHolder> {
		MODULE_HOLDER_SINGLETON.with(|slot| {
			let mut slot = slot.borrow_mut();
			slot.get_or_insert_with(|| ArcRwSignal::new(ModuleHolder::new())).clone()
		})
	}

	fn new() -> Self
	{
		Self {
			_links: LinksHolder::new(),
			_aiConfig: AiConfigHolder::new(),
			_aiConfigReady: false,
			_aiChat: AiChatHolder::new(),
			_aiChatReady: false,
			_aiInbox: AiInboxHolder::new(),
			_aiInboxReady: false,
			_aiAutomation: AiAutomationEngine::default(),
			_aiAutomationInbox: VecDeque::new(),
			_aiAutomationRunning: false,
			_blocks: HashMap::new(),
			_crons: Default::default(),
			_moduleActions: None,
			_blockNb: 0,
			_epochCounter: 0,
			_activeEpoch: None,
			_taskOwner: None,
			_networkGeneration: 0,
			_networkSuspended: false,
		}
	}

	pub(crate) fn lifecycle_open() -> ModuleHolderEpoch
	{
		let (epoch, previousOwner) = Self::getSingleton()
			.try_update(|holder| holder.lifecycle_open_inner())
			.expect("the permanent ModuleHolder singleton must remain writable while Home opens");
		if let Some(previousOwner) = previousOwner
		{
			previousOwner.cleanup();
		}
		return epoch;
	}

	pub(crate) fn lifecycle_close()
	{
		let owner = Self::getSingleton()
			.try_update(|holder| holder.lifecycle_close_inner())
			.flatten();
		if let Some(owner) = owner
		{
			owner.cleanup();
		}
	}

	pub(crate) fn lifecycle_closeIf(epoch: ModuleHolderEpoch) -> bool
	{
		let Some((wasClosed, owner)) = Self::getSingleton()
			.try_update(|holder| holder.lifecycle_closeIf_inner(epoch))
		else
		{
			return false;
		};
		if let Some(owner) = owner
		{
			owner.cleanup();
		}
		return wasClosed;
	}

	pub(crate) fn task_spawn(epoch: ModuleHolderEpoch, task: impl Future<Output = ()> + 'static)
	{
		let owner = Self::getSingleton().with_untracked(|holder| holder.lifecycle_owner_get(epoch));
		if let Some(owner) = owner
		{
			owner.with(|| spawn_local_scoped_with_cancellation(task));
		}
	}

	pub(crate) fn lifecycle_isActive(epoch: ModuleHolderEpoch) -> bool
	{
		return Self::getSingleton().with_untracked(|holder| holder.lifecycle_epoch_isActive(epoch));
	}

	pub(crate) fn network_isActive(epoch: ModuleHolderEpoch) -> bool
	{
		return Self::getSingleton().with_untracked(|holder| holder.network_generation_get(epoch).is_some());
	}

	pub(crate) fn aiConfig_get() -> AiConfigDocument
	{
		return Self::getSingleton().with_untracked(|holder| holder._aiConfig.document_get());
	}

	pub(crate) fn aiConfig_isReady() -> bool
	{
		return Self::getSingleton().with(|holder| holder._aiConfigReady);
	}

	pub(crate) fn aiAutomationModules_get() -> Result<Vec<AiModuleCapabilities>,AiAutomationError>
	{
		return Self::getSingleton().with_untracked(Self::aiAutomationModules_inner);
	}

	fn aiAutomationModules_inner(&self) -> Result<Vec<AiModuleCapabilities>,AiAutomationError>
	{
		let inbox = self._aiInbox.capabilities_get();
		inbox.catalog.validate()?;
		inbox.grant.validate(&inbox.catalog)?;
		let mut modules = vec![inbox];
		for (moduleId,module) in &self._blocks
		{
			let snapshot = module.with_untracked(|module| {
				let inner = module.inner();
				let catalog = inner.ai_capabilities();
				if (catalog.isEmpty())
				{
					return Ok(None);
				}
				catalog.validate()?;
				let grant = inner.ai_grant();
				grant.validate(&catalog)?;
				return Ok(Some(AiModuleCapabilities {
					moduleId: moduleId.clone(),
					moduleType: inner.module_name(),
					catalog,
					grant,
				}));
			})?;
			if let Some(snapshot) = snapshot
			{
				modules.push(snapshot);
			}
		}
		modules.sort_by(|left,right| left.moduleType.cmp(&right.moduleType)
			.then_with(|| left.moduleId.cmp(&right.moduleId)));
		return Ok(modules);
	}

	pub(crate) fn aiInbox_isReady() -> bool
	{
		return Self::getSingleton().with(|holder| holder._aiInboxReady);
	}

	pub(crate) fn aiInbox_entries_get() -> Vec<AiInboxEntry>
	{
		return Self::getSingleton().with(|holder| {
			if (!holder._aiInboxReady)
			{
				return Vec::new();
			}
			return holder._aiInbox.document_get().entries;
		});
	}

	pub(crate) fn aiInbox_actionIsUsable(entry: &AiInboxAction) -> bool
	{
		return Self::getSingleton().with(|holder| {
			if (!holder._aiInboxReady || !holder._aiConfigReady)
			{
				return false;
			}
			let document = holder._aiConfig.document_get();
			let Some(context) = document.contexts.iter().find(|context| context.id == entry.contextId)
			else {return false};
			let Ok(modules) = holder.aiAutomationModules_inner() else {return false};
			return entry.action.delayed_validate(
				context,&entry.contextDefinitionFingerprint,&modules,
			).is_ok();
		});
	}

	pub(crate) fn aiAutomation_exposurePrepare(
		epoch: ModuleHolderEpoch,
		moduleId: &ModuleID,
		request: AiExposureRequest,
	) -> Result<AiExposureFuture,AiAutomationError>
	{
		return Self::getSingleton().with_untracked(|holder| {
			if (!holder.lifecycle_epoch_isActive(epoch) || request.event.sourceModuleId != *moduleId)
			{
				return Err(AiAutomationError::LifecycleClosed);
			}
			let module = holder._blocks.get(moduleId).ok_or(AiAutomationError::CapabilityUnavailable)?;
			return module.with_untracked(|module| module.inner().ai_exposure(request))
				.ok_or(AiAutomationError::CapabilityUnavailable);
		});
	}

	pub(crate) fn aiAutomation_actionPrepare(
		epoch: ModuleHolderEpoch,
		action: AiValidatedAction,
	) -> Result<AiActionFuture,AiAutomationError>
	{
		return Self::getSingleton().with_untracked(|holder| {
			if (!holder.lifecycle_epoch_isActive(epoch))
			{
				return Err(AiAutomationError::LifecycleClosed);
			}
			let module = holder._blocks.get(&action.targetModuleId)
				.ok_or(AiAutomationError::CapabilityUnavailable)?;
			return module.with_untracked(|module| {
				let persistence = module.inner().ai_actionPersistence_prepare(&action,None)?;
				if (persistence != AiActionPersistence::Unsupported)
				{
					return Ok(Box::pin(async move {
						return Self::aiAutomation_persistentActionApply(epoch,action).await;
					}) as AiActionFuture);
				}
				return module.inner().ai_action_apply(action)
					.ok_or(AiAutomationError::CapabilityUnavailable);
			});
		});
	}

	async fn aiAutomation_persistentActionApply(
		epoch: ModuleHolderEpoch,
		action: AiValidatedAction,
	) -> AiActionApplyResult
	{
		let Some(crypto) = ClientState::expect().crypto_get() else {return AiActionApplyResult::Rejected};
		let mut remoteBase = None::<ModuleContent>;
		for _ in 0..6
		{
			let prepared = Self::getSingleton().with_untracked(|holder| -> Result<_,AiAutomationError> {
				let generation = holder.network_generation_get(epoch)
					.ok_or(AiAutomationError::LifecycleClosed)?;
				let module = holder._blocks.get(&action.targetModuleId)
					.ok_or(AiAutomationError::CapabilityUnavailable)?;
				return module.with_untracked(|module| {
					let mut base = remoteBase.clone().unwrap_or_else(|| module.export());
					base.id = action.targetModuleId.clone();
					let persistence = module.inner().ai_actionPersistence_prepare(
						&action,remoteBase.as_ref(),
					)?;
					return Ok((generation,persistence,base));
				});
			});
			let Ok((generation,persistence,mut base)) = prepared else {return AiActionApplyResult::Rejected};
			let candidate = match persistence
			{
				AiActionPersistence::Unsupported => return AiActionApplyResult::Rejected,
				AiActionPersistence::AlreadyApplied => {
					if let Some(remote) = remoteBase.take()
					{
						if (Self::aiAutomation_actionPersistenceImport(epoch,generation,&action,remote).is_err())
						{
							return AiActionApplyResult::Rejected;
						}
					}
					return AiActionApplyResult::Applied;
				},
				AiActionPersistence::Prepared(candidate) => candidate,
			};
			if (candidate.timestamp <= candidate.expectedTimestamp)
			{
				return AiActionApplyResult::Rejected;
			}
			base.timestamp = candidate.timestamp;
			base.content = candidate.content;
			let plaintext = base;
			let mut encrypted = plaintext.clone();
			if (Self::export_crypt_content(&mut encrypted,&crypto).is_err())
			{
				return AiActionApplyResult::Rejected;
			}
			let result = match API_module_updateIfCurrent(encrypted,candidate.expectedTimestamp).await
			{
				Ok(result) => result,
				Err(_) => return AiActionApplyResult::Ambiguous,
			};
			if (!Self::getSingleton().with_untracked(|holder| {
				holder.network_generation_isActive(epoch,generation)
			}))
			{
				return AiActionApplyResult::Ambiguous;
			}
			match result
			{
				ModuleReturnUpdate::OK => {
					if (Self::aiAutomation_actionPersistenceImport(
						epoch,generation,&action,plaintext,
					).is_err())
					{
						return AiActionApplyResult::Rejected;
					}
					return AiActionApplyResult::Applied;
				},
				ModuleReturnUpdate::OUTDATED(mut remote) => {
					if (remote.id != action.targetModuleId || remote.typeModule != plaintext.typeModule)
					{
						return AiActionApplyResult::Rejected;
					}
					if (Self::import_decrypt_content(&mut remote,&crypto).is_err())
					{
						return AiActionApplyResult::Rejected;
					}
					remoteBase = Some(remote);
				},
			}
		}
		return AiActionApplyResult::Rejected;
	}

	fn aiAutomation_actionPersistenceImport(
		epoch: ModuleHolderEpoch,
		generation: u64,
		action: &AiValidatedAction,
		content: ModuleContent,
	) -> Result<(),AiAutomationError>
	{
		let module = Self::getSingleton().with_untracked(|holder| {
			if (!holder.network_generation_isActive(epoch,generation))
			{
				return None;
			}
			return holder._blocks.get(&action.targetModuleId).cloned();
		}).ok_or(AiAutomationError::LifecycleClosed)?;
		return module.try_update(|module| {
			module.import(content.clone());
			return module.inner().ai_actionPersistence_saved(&content);
		}).ok_or(AiAutomationError::LifecycleClosed)?;
	}

	pub(crate) fn aiAutomation_eventsPublish(
		epoch: ModuleHolderEpoch,
		events: Vec<AiAutomationEvent>,
	) -> Vec<AiAutomationEvent>
	{
		if (events.is_empty()) {return Vec::new();}
		let retryEvents = events.clone();
		let moduleHolder = Self::getSingleton();
		let Some((mustStart,rejected,toaster)) = moduleHolder.try_update(|holder| {
			if (!holder.network_generation_get(epoch).is_some()
				|| !holder._aiConfigReady
				|| !holder._aiInboxReady
				|| holder._aiConfig.document_get().profile.is_none())
			{
				return (false,events,None);
			}
			let contexts = holder._aiConfig.document_get().contexts;
			let mut rejected = Vec::new();
			for event in events
			{
				let hasConsumer = contexts.iter().any(|context| context.enabled
					&& context.source.moduleId == event.sourceModuleId
					&& context.source.event == event.event);
				let isQueued = holder._aiAutomationInbox.iter().any(|queued| {
					return queued.sourceModuleId == event.sourceModuleId
						&& queued.event == event.event
						&& queued.eventId == event.eventId;
				});
				if (!hasConsumer || isQueued) {continue;}
				if (holder._aiAutomationInbox.len() >= AI_AUTOMATION_QUEUE_MAXIMUM)
				{
					rejected.push(event);
					continue;
				}
				holder._aiAutomationInbox.push_back(event);
			}
			let mustStart = !holder._aiAutomationRunning && !holder._aiAutomationInbox.is_empty();
			if (mustStart) {holder._aiAutomationRunning = true;}
			let toaster = holder._moduleActions.as_ref().map(|actions| actions.aiAutomationUi_get().0);
			return (mustStart,rejected,toaster);
		})
		else
		{
			return retryEvents;
		};
		if (!rejected.is_empty())
			&& let Some(toaster) = toaster.clone()
		{
			Self::task_spawn(epoch,async move {
				toastingErr(&toaster,"FRONTAI_AUTOMATION_QUEUE_FULL").await;
			});
		}
		if (mustStart)
		{
			Self::task_spawn(epoch,async move {Self::aiAutomation_runLoop(epoch).await;});
		}
		return rejected;
	}

	pub(crate) fn aiAutomation_sourceBaselinePersist(
		epoch: ModuleHolderEpoch,
		event: AiAutomationEvent,
		toaster: ToasterContext,
	)
	{
		Self::task_spawn(epoch,async move {
			if let Err(errorKey) = Self::aiAutomation_eventReservation(epoch,&event).await
			{
				toastingErr(&toaster,errorKey).await;
			}
		});
	}

	async fn aiAutomation_runLoop(epoch: ModuleHolderEpoch)
	{
		loop
		{
			let event = Self::getSingleton().try_update(|holder| {
				if (!holder.network_generation_get(epoch).is_some())
				{
					holder._aiAutomationInbox.clear();
					holder._aiAutomationRunning = false;
					return None;
				}
				let event = holder._aiAutomationInbox.pop_front();
				if (event.is_none()) {holder._aiAutomationRunning = false;}
				return event;
			}).flatten();
			let Some(event) = event else {return;};
			let result = Self::aiAutomation_eventProcess(epoch,event).await;
			if let Err(errorKey) = result
			{
				let ui = Self::getSingleton().with_untracked(|holder| {
					return holder._moduleActions.as_ref().map(module_actions::ModuleActionFn::aiAutomationUi_get);
				});
				if let Some((toaster,_)) = ui
				{
					toastingErr(&toaster,errorKey).await;
				}
				let _ = Self::getSingleton().try_update(|holder| holder._aiAutomation.clear());
			}
		}
	}

	async fn aiAutomation_eventProcess(epoch: ModuleHolderEpoch,event: AiAutomationEvent) -> Result<(),&'static str>
	{
		let retryEvent = event.clone();
		let prepared = Self::getSingleton().try_update(|holder| -> Result<_,AiAutomationError> {
			if (!holder.network_generation_get(epoch).is_some()
				|| !holder._aiConfigReady || !holder._aiInboxReady)
			{
				return Err(AiAutomationError::LifecycleClosed);
			}
			let modules = holder.aiAutomationModules_inner()?;
			let mut document = holder._aiConfig.document_get();
			let results = holder._aiAutomation.event_enqueue(
				&mut document.contexts,event,&modules,OffsetDateTime::now_utc().unix_timestamp(),
			)?;
			return Ok((document,results));
		}).ok_or(AiAutomationError::LifecycleClosed)
			.and_then(|result| result)
			.map_err(AiAutomationError::translateKey_get)?;
		let (document,results) = prepared;
		let ui = Self::getSingleton().with_untracked(|holder| {
			return holder._moduleActions.as_ref().map(module_actions::ModuleActionFn::aiAutomationUi_get);
		}).ok_or("FRONTAI_AUTOMATION_INTERRUPTED")?;
		let (toaster,allowedOrigins) = ui;
		let mut accepted = false;
		let mut retry = false;
		for (_,result) in results
		{
			match result
			{
				Ok(()) => accepted = true,
				Err(AiAutomationError::DuplicateExecution) => {},
				Err(error @ (AiAutomationError::BudgetExceeded | AiAutomationError::QueueFull)) => {
					retry = true;
					toastingErr(&toaster,error.translateKey_get()).await;
				},
				Err(error) => toastingErr(&toaster,error.translateKey_get()).await,
			}
		}
		if (retry)
		{
			Self::aiAutomation_eventRetry(epoch,&retryEvent);
		}
		if (!accepted)
		{
			return Ok(());
		}
		Self::aiAutomation_documentSave(epoch,document).await?;
		match Self::aiAutomation_eventReservation(epoch,&retryEvent).await
		{
			Ok(AiEventReservationResult::Proceed) => {},
			Ok(AiEventReservationResult::AlreadyHandled) => {
				let _ = Self::getSingleton().try_update(|holder| holder._aiAutomation.event_cancel(&retryEvent));
				return Ok(());
			},
			Err(errorKey) => {
				let _ = Self::getSingleton().try_update(|holder| holder._aiAutomation.event_cancel(&retryEvent));
				Self::aiAutomation_eventRetry(epoch,&retryEvent);
				return Err(errorKey);
			},
		}

		while let Some(execution) = Self::getSingleton().try_update(|holder| holder._aiAutomation.next()).flatten()
		{
			Self::aiAutomation_executionProcess(
				epoch,execution,&toaster,&allowedOrigins,
			).await?;
		}
		return Ok(());
	}

	fn aiAutomation_eventRetry(epoch: ModuleHolderEpoch,event: &AiAutomationEvent)
	{
		Self::getSingleton().with_untracked(|holder| {
			if (!holder.lifecycle_epoch_isActive(epoch))
			{
				return;
			}
			if let Some(module) = holder._blocks.get(&event.sourceModuleId)
			{
				module.with_untracked(|module| module.inner().ai_eventRetry(event));
			}
		});
	}

	async fn aiAutomation_eventReservation(
		epoch: ModuleHolderEpoch,
		event: &AiAutomationEvent,
	) -> Result<AiEventReservationResult,&'static str>
	{
		let crypto = ClientState::expect().crypto_get()
			.ok_or("FRONTAI_AUTOMATION_RESERVATION_FAILED")?;
		let mut remoteBase = None::<ModuleContent>;
		for _ in 0..4
		{
			let prepared = Self::getSingleton().with_untracked(|holder| -> Result<_,AiAutomationError> {
				let generation = holder.network_generation_get(epoch)
					.ok_or(AiAutomationError::LifecycleClosed)?;
				let module = holder._blocks.get(&event.sourceModuleId)
					.ok_or(AiAutomationError::CapabilityUnavailable)?;
				return module.with_untracked(|module| {
					let mut base = remoteBase.clone().unwrap_or_else(|| module.export());
					base.id = event.sourceModuleId.clone();
					let reservation = module.inner().ai_eventReservation_prepare(
						event,remoteBase.as_ref(),
					)?;
					return Ok((generation,reservation,base));
				});
			}).map_err(AiAutomationError::translateKey_get)?;
			let (generation,reservation,mut base) = prepared;
			let candidate = match reservation
			{
				AiEventReservation::Unsupported => return Ok(AiEventReservationResult::Proceed),
				AiEventReservation::AlreadyHandled => {
					if let Some(remote) = remoteBase.take()
					{
						Self::aiAutomation_eventReservationImport(epoch,generation,event,remote)?;
					}
					return Ok(AiEventReservationResult::AlreadyHandled);
				},
				AiEventReservation::Prepared(candidate) => candidate,
			};
			if (base.timestamp != candidate.expectedTimestamp || candidate.timestamp <= candidate.expectedTimestamp)
			{
				return Err("FRONTAI_AUTOMATION_RESERVATION_FAILED");
			}
			base.timestamp = candidate.timestamp;
			base.content = candidate.content;
			let plaintext = base;
			let mut encrypted = plaintext.clone();
			Self::export_crypt_content(&mut encrypted,&crypto)
				.map_err(|_| "FRONTAI_AUTOMATION_RESERVATION_FAILED")?;
			let result = API_module_updateIfCurrent(encrypted,candidate.expectedTimestamp).await
				.map_err(|error| match error
				{
					ModuleApiError::AUTH_REQUIRED => "FRONTAI_AUTOMATION_INTERRUPTED",
					ModuleApiError::NOT_FOUND | ModuleApiError::SERVER_ERROR =>
						"FRONTAI_AUTOMATION_RESERVATION_FAILED",
				})?;
			if (!Self::getSingleton().with_untracked(|holder| {
				holder.network_generation_isActive(epoch,generation)
			}))
			{
				return Err("FRONTAI_AUTOMATION_INTERRUPTED");
			}
			match result
			{
				ModuleReturnUpdate::OK => {
					Self::aiAutomation_eventReservationImport(epoch,generation,event,plaintext)?;
					return Ok(AiEventReservationResult::Proceed);
				},
				ModuleReturnUpdate::OUTDATED(mut remote) => {
					if (remote.id != event.sourceModuleId || remote.typeModule != plaintext.typeModule)
					{
						return Err("FRONTAI_AUTOMATION_RESERVATION_FAILED");
					}
					Self::import_decrypt_content(&mut remote,&crypto)
						.map_err(|_| "FRONTAI_AUTOMATION_RESERVATION_FAILED")?;
					remoteBase = Some(remote);
				},
			}
		}
		return Err("FRONTAI_AUTOMATION_RESERVATION_FAILED");
	}

	fn aiAutomation_eventReservationImport(
		epoch: ModuleHolderEpoch,
		generation: u64,
		event: &AiAutomationEvent,
		content: ModuleContent,
	) -> Result<(),&'static str>
	{
		let module = Self::getSingleton().with_untracked(|holder| {
			if (!holder.network_generation_isActive(epoch,generation))
			{
				return None;
			}
			return holder._blocks.get(&event.sourceModuleId).cloned();
		}).ok_or("FRONTAI_AUTOMATION_INTERRUPTED")?;
		let applied = module.try_update(|module| {
			module.import(content.clone());
			return module.inner().ai_eventReservation_saved(&content);
		}).ok_or("FRONTAI_AUTOMATION_INTERRUPTED")?;
		return applied.map_err(AiAutomationError::translateKey_get);
	}

	async fn aiAutomation_executionProcess(
		epoch: ModuleHolderEpoch,
		execution: AiQueuedExecution,
		toaster: &ToasterContext,
		allowedOrigins: &crate::front::ai::AiAllowedOrigins,
	) -> Result<(),&'static str>
	{
		let snapshot = Self::getSingleton().try_update(|holder| -> Result<_,AiAutomationError> {
			if (!holder.network_generation_get(epoch).is_some() || !holder._aiInboxReady)
			{
				return Err(AiAutomationError::LifecycleClosed);
			}
			let document = holder._aiConfig.document_get();
			let Some(context) = document.contexts.iter()
				.find(|context| context.id == execution.contextId && context.enabled)
				.cloned()
			else
			{
				holder._aiAutomation.running_cancel(&execution.contextId);
				return Ok(None);
			};
			if (!execution.contextDefinition_isSame(&context)?)
			{
				holder._aiAutomation.running_cancel(&execution.contextId);
				return Ok(None);
			}
			let profile = document.profile.ok_or(AiAutomationError::CapabilityUnavailable)?;
			return Ok(Some((context,profile,holder.aiAutomationModules_inner()?)));
		}).ok_or(AiAutomationError::LifecycleClosed)
			.and_then(|result| result)
			.map_err(AiAutomationError::translateKey_get)?;
		let Some((context,profile,modules)) = snapshot else {return Ok(());};
		let request = AiExposureRequest {
			event: execution.event.clone(),fields: context.source.fields.clone(),
		};
		let exposureFuture = match Self::aiAutomation_exposurePrepare(epoch,&context.source.moduleId,request)
		{
			Ok(future) => future,
			Err(error) => {
				Self::aiAutomation_executionFinish(epoch,&context.id,AiExecutionOutcome::FailedTerminal).await?;
				toastingErr(toaster,error.translateKey_get()).await;
				return Ok(());
			},
		};
		let exposure = match exposureFuture.await
		{
			Ok(exposure) => exposure,
			Err(error) => {
				if (!Self::lifecycle_isActive(epoch)) {return Err("FRONTAI_AUTOMATION_INTERRUPTED");}
				Self::aiAutomation_executionFinish(epoch,&context.id,AiExecutionOutcome::FailedTerminal).await?;
				toastingErr(toaster,error.translateKey_get()).await;
				return Ok(());
			},
		};
		if (!Self::lifecycle_isActive(epoch)) {return Err("FRONTAI_AUTOMATION_INTERRUPTED");}
		if (!Self::aiAutomation_executionContinue(epoch,&context)?) {return Ok(());}
		let response = match completion_get(&context,&exposure,&modules,&profile,allowedOrigins).await
		{
			Ok(response) => response,
			Err(error) => {
				if (!Self::lifecycle_isActive(epoch)) {return Err("FRONTAI_AUTOMATION_INTERRUPTED");}
				let outcome = if (error.isTimeout()) {AiExecutionOutcome::Ambiguous} else {AiExecutionOutcome::FailedTerminal};
				Self::aiAutomation_executionFinish(epoch,&context.id,outcome).await?;
				toastingErr(toaster,error.translateKey_get()).await;
				return Ok(());
			},
		};
		if (!Self::lifecycle_isActive(epoch)) {return Err("FRONTAI_AUTOMATION_INTERRUPTED");}
		if (!Self::aiAutomation_executionContinue(epoch,&context)?) {return Ok(());}
		let actions = Self::getSingleton().with_untracked(|holder| {
			return holder._aiAutomation.response_validate(&context,&response,&modules);
		});
		let actions = match actions
		{
			Ok(actions) => actions,
			Err(error) => {
				Self::aiAutomation_executionFinish(epoch,&context.id,AiExecutionOutcome::FailedTerminal).await?;
				toastingErr(toaster,error.translateKey_get()).await;
				return Ok(());
			},
		};
		let mut immediateActions = Vec::new();
		let mut inboxEntries = Vec::new();
		let createdAt = OffsetDateTime::now_utc().unix_timestamp();
		for action in actions
		{
			if (action.targetModuleId.id == AiInboxHolder::MODULE_ID)
			{
				if (action.action != AI_INBOX_ALERT_ACTION)
				{
					Self::aiAutomation_executionFinish(epoch,&context.id,AiExecutionOutcome::FailedTerminal).await?;
					toastingErr(toaster,"FRONTAI_AUTOMATION_CAPABILITY_UNAVAILABLE").await;
					return Ok(());
				}
				let alert = match AiInboxAlert::fromAction(&context,&action,createdAt)
				{
					Ok(alert) => alert,
					Err(error) => {
						Self::aiAutomation_executionFinish(epoch,&context.id,AiExecutionOutcome::FailedTerminal).await?;
						toastingErr(toaster,error.translateKey_get()).await;
						return Ok(());
					},
				};
				inboxEntries.push(AiInboxEntry::Alert(alert));
				continue;
			}
			if (action.confirmation == AiConfirmationPolicy::Confirm)
			{
				let Some(targetModule) = modules.iter()
					.find(|module| module.moduleId == action.targetModuleId)
				else
				{
					Self::aiAutomation_executionFinish(epoch,&context.id,AiExecutionOutcome::FailedTerminal).await?;
					toastingErr(toaster,"FRONTAI_AUTOMATION_CAPABILITY_UNAVAILABLE").await;
					return Ok(());
				};
				let pending = match AiInboxAction::new(&context,&targetModule.moduleType,action,createdAt)
				{
					Ok(pending) => pending,
					Err(error) => {
						Self::aiAutomation_executionFinish(epoch,&context.id,AiExecutionOutcome::FailedTerminal).await?;
						toastingErr(toaster,error.translateKey_get()).await;
						return Ok(());
					},
				};
				inboxEntries.push(AiInboxEntry::Action(pending));
				continue;
			}
			immediateActions.push(action);
		}
		if (!inboxEntries.is_empty())
		{
			if let Err(errorKey) = Self::aiInbox_entriesAdd(epoch,inboxEntries).await
			{
				Self::aiAutomation_executionFinish(epoch,&context.id,AiExecutionOutcome::FailedTerminal).await?;
				toastingErr(toaster,errorKey).await;
				return Ok(());
			}
			toastingInfo(toaster,"FRONTAI_INBOX_ITEMS_ADDED").await;
		}
		let hadImmediateActions = !immediateActions.is_empty();

		let mut outcome = AiExecutionOutcome::Succeeded;
		let mut refreshTargets = Vec::new();
		for action in immediateActions
		{
			if (!Self::aiAutomation_executionContinue(epoch,&context)?) {return Ok(());}
			let currentModules = match Self::aiAutomationModules_get()
			{
				Ok(modules) => modules,
				Err(error) => {
					outcome = AiExecutionOutcome::FailedTerminal;
					toastingErr(toaster,error.translateKey_get()).await;
					break;
				},
			};
			if (!currentModules.iter().any(|module| module.moduleId == action.targetModuleId))
			{
				outcome = AiExecutionOutcome::FailedTerminal;
				toastingErr(toaster,"FRONTAI_AUTOMATION_CAPABILITY_UNAVAILABLE").await;
				break;
			}
			let actionFuture = match Self::aiAutomation_actionPrepare(epoch,action.clone())
			{
				Ok(future) => future,
				Err(error) => {
					outcome = AiExecutionOutcome::Rejected;
					toastingErr(toaster,error.translateKey_get()).await;
					break;
				},
			};
			match actionFuture.await
			{
				AiActionApplyResult::Applied => {
					if (!Self::lifecycle_isActive(epoch)) {return Err("FRONTAI_AUTOMATION_INTERRUPTED");}
					if (!Self::aiAutomation_executionContinue(epoch,&context)?) {return Ok(());}
					Self::aiAutomation_actionAppliedSave(epoch,&context.id,&action).await?;
					if (!refreshTargets.contains(&action.targetModuleId))
					{
						refreshTargets.push(action.targetModuleId.clone());
					}
				},
				AiActionApplyResult::Rejected => {
					outcome = AiExecutionOutcome::Rejected;
					toastingErr(toaster,"FRONTAI_AUTOMATION_ACTION_REJECTED").await;
					break;
				},
				AiActionApplyResult::Ambiguous => {
					outcome = AiExecutionOutcome::Ambiguous;
					toastingErr(toaster,"FRONTAI_AUTOMATION_ACTION_AMBIGUOUS").await;
					break;
				},
			}
		}
		Self::aiAutomation_executionFinish(epoch,&context.id,outcome).await?;
		if (!refreshTargets.is_empty())
		{
			Self::module_refresh(epoch,refreshTargets,toaster.clone());
		}
		if (outcome == AiExecutionOutcome::Succeeded && hadImmediateActions)
		{
			toastingSuccess(toaster,"FRONTAI_AUTOMATION_COMPLETED").await;
		}
		return Ok(());
	}

	fn aiAutomation_executionContinue(
		epoch: ModuleHolderEpoch,
		executionContext: &crate::front::ai::automation::AiAutomationContext,
	) -> Result<bool,&'static str>
	{
		return Self::getSingleton().try_update(|holder| {
			if (!holder.network_generation_get(epoch).is_some())
			{
				return None;
			}
			let isCurrent = holder._aiConfig.document_get().contexts.iter().any(|context| {
				return context.id == executionContext.id
					&& context.enabled
					&& context.executionDefinition_isSame(executionContext);
			});
			if (!isCurrent)
			{
				holder._aiAutomation.running_cancel(&executionContext.id);
			}
			return Some(isCurrent);
		}).flatten().ok_or("FRONTAI_AUTOMATION_INTERRUPTED");
	}

	async fn aiAutomation_actionAppliedSave(
		epoch: ModuleHolderEpoch,
		contextId: &str,
		action: &AiValidatedAction,
	) -> Result<(),&'static str>
	{
		let document = Self::getSingleton().with_untracked(|holder| -> Result<_,AiConfigError> {
			let mut document = holder._aiConfig.document_get();
			let targetModuleType = holder._blocks.get(&action.targetModuleId)
				.ok_or(AiAutomationError::CapabilityUnavailable)?
				.with_untracked(|module| module.inner().module_name());
			let context = document.contexts.iter_mut().find(|context| context.id == contextId)
				.ok_or(AiAutomationError::CapabilityUnavailable)?;
			let contextName = context.name.clone();
			AiAutomationEngine::actionApplied_mark(context,action);
			let historyEntry = AiAutomationHistoryEntry::new(
				&contextName,&targetModuleType,action,OffsetDateTime::now_utc().unix_timestamp(),
			)?;
			document.automationHistory_add(historyEntry)?;
			return Ok(document);
		}).map_err(AiConfigError::translateKey_get)?;
		return Self::aiAutomation_documentSave(epoch,document).await;
	}

	async fn aiAutomation_executionFinish(
		epoch: ModuleHolderEpoch,
		contextId: &str,
		outcome: AiExecutionOutcome,
	) -> Result<(),&'static str>
	{
		let document = Self::getSingleton().try_update(|holder| {
			let mut document = holder._aiConfig.document_get();
			let context = document.contexts.iter_mut().find(|context| context.id == contextId)
				.ok_or(AiAutomationError::CapabilityUnavailable)?;
			holder._aiAutomation.finish(context,outcome)?;
			return Ok::<_,AiAutomationError>(document);
		}).ok_or(AiAutomationError::LifecycleClosed)
			.and_then(|result| result)
			.map_err(AiAutomationError::translateKey_get)?;
		return Self::aiAutomation_documentSave(epoch,document).await;
	}

	async fn aiAutomation_documentSave(epoch: ModuleHolderEpoch,document: AiConfigDocument) -> Result<(),&'static str>
	{
		return Self::aiConfig_save(epoch,document).await
			.map_err(AiConfigSaveError::translateKey_get);
	}

	async fn aiInbox_entriesAdd(
		epoch: ModuleHolderEpoch,
		entries: Vec<AiInboxEntry>,
	) -> Result<(),&'static str>
	{
		if (entries.is_empty())
		{
			return Ok(());
		}
		return Self::aiInbox_mutate(epoch,AiInboxMutation::Add(entries)).await;
	}

	pub(crate) async fn aiInbox_entryRemove(
		epoch: ModuleHolderEpoch,
		entryId: String,
	) -> Result<(),&'static str>
	{
		return Self::aiInbox_mutate(epoch,AiInboxMutation::Remove(entryId)).await;
	}

	pub(crate) async fn aiInbox_actionApply(
		epoch: ModuleHolderEpoch,
		entryId: String,
		toaster: ToasterContext,
	) -> Result<(),&'static str>
	{
		Self::aiInbox_refresh(epoch).await?;
		let (contextId,action) = Self::getSingleton().with_untracked(|holder| -> Result<_,AiAutomationError> {
			if (!holder.network_generation_get(epoch).is_some() || !holder._aiInboxReady || !holder._aiConfigReady)
			{
				return Err(AiAutomationError::LifecycleClosed);
			}
			let inboxDocument = holder._aiInbox.document_get();
			let entry = inboxDocument.entries.iter().find_map(|entry| match entry
			{
				AiInboxEntry::Action(entry) if entry.id_get() == entryId => Some(entry),
				_ => None,
			}).ok_or(AiAutomationError::CapabilityUnavailable)?;
			let aiDocument = holder._aiConfig.document_get();
			let context = aiDocument.contexts.iter().find(|context| context.id == entry.contextId)
				.ok_or(AiAutomationError::CapabilityUnavailable)?;
			let modules = holder.aiAutomationModules_inner()?;
			entry.action.delayed_validate(context,&entry.contextDefinitionFingerprint,&modules)?;
			return Ok((entry.contextId.clone(),entry.action.clone()));
		}).map_err(AiAutomationError::translateKey_get)?;

		let actionFuture = Self::aiAutomation_actionPrepare(epoch,action.clone())
			.map_err(AiAutomationError::translateKey_get)?;
		match actionFuture.await
		{
			AiActionApplyResult::Applied => {},
			AiActionApplyResult::Rejected => return Err("FRONTAI_AUTOMATION_ACTION_REJECTED"),
			AiActionApplyResult::Ambiguous => return Err("FRONTAI_AUTOMATION_ACTION_AMBIGUOUS"),
		}
		if (!Self::lifecycle_isActive(epoch))
		{
			return Err("FRONTAI_INBOX_SAVE_INTERRUPTED");
		}
		Self::aiAutomation_actionAppliedSave(epoch,&contextId,&action).await?;
		Self::aiInbox_entryRemove(epoch,entryId).await?;
		Self::module_refresh(epoch,vec![action.targetModuleId],toaster);
		return Ok(());
	}

	pub(crate) async fn aiInbox_refresh(epoch: ModuleHolderEpoch) -> Result<(),&'static str>
	{
		let clientState = ClientState::expect();
		if (!clientState.login_isConnected_untracked())
		{
			return Err("FRONTAI_INBOX_SAVE_AUTH_REQUIRED");
		}
		let crypto = clientState.crypto_get().ok_or("FRONTERROR_CRYPTO_CONTEXT_MISSING")?;
		let generation = Self::getSingleton().with_untracked(|holder| {
			if (!holder._aiInboxReady)
			{
				return None;
			}
			return holder.network_generation_get(epoch);
		}).ok_or("FRONTAI_INBOX_SAVE_INTERRUPTED")?;
		let result = API_module_retrieve(ApiModulesID {
			key: ModuleID {id: AiInboxHolder::MODULE_ID.to_string()},
			timestamp: i64::MIN,
		}).await.map_err(Self::aiInbox_networkErrorKey)?;
		if (!Self::getSingleton().with_untracked(|holder| holder.network_generation_isActive(epoch,generation)))
		{
			return Err("FRONTAI_INBOX_SAVE_INTERRUPTED");
		}
		let ModuleReturnRetrieve::UPDATED(mut content) = result else {return Ok(())};
		if (content.id.id != AiInboxHolder::MODULE_ID || content.typeModule != AiInboxHolder::MODULE_NAME)
		{
			return Err("FRONTAI_INBOX_INVALID");
		}
		Self::import_decrypt_content(&mut content,&crypto)
			.map_err(|_| "FRONTERROR_CRYPTO_DECRYPT_FAILED")?;
		let mut imported = AiInboxHolder::new();
		imported.import(content).map_err(AiInboxError::translateKey_get)?;
		let applied = Self::getSingleton().try_update(|holder| {
			if (!holder.network_generation_isActive(epoch,generation))
			{
				return false;
			}
			holder._aiInbox.loaded_apply(imported);
			return true;
		}).unwrap_or(false);
		if (!applied)
		{
			return Err("FRONTAI_INBOX_SAVE_INTERRUPTED");
		}
		return Ok(());
	}

	async fn aiInbox_mutate(
		epoch: ModuleHolderEpoch,
		mutation: AiInboxMutation,
	) -> Result<(),&'static str>
	{
		let clientState = ClientState::expect();
		if (!clientState.login_isConnected_untracked())
		{
			return Err("FRONTAI_INBOX_SAVE_AUTH_REQUIRED");
		}
		let crypto = clientState.crypto_get().ok_or("FRONTERROR_CRYPTO_CONTEXT_MISSING")?;
		for _ in 0..6
		{
			let prepared = Self::getSingleton().with_untracked(|holder| -> Result<_,&'static str> {
				let generation = holder.network_generation_get(epoch)
					.ok_or("FRONTAI_INBOX_SAVE_INTERRUPTED")?;
				if (!holder._aiInboxReady)
				{
					return Err("FRONTAI_INBOX_SAVE_INTERRUPTED");
				}
				let mut document = holder._aiInbox.document_get();
				let changed = mutation.apply(&mut document)
					.map_err(AiInboxError::translateKey_get)?;
				if (!changed)
				{
					return Ok(None);
				}
				let expectedTimestamp = if holder._aiInbox.persisted_get()
				{
					holder._aiInbox.cache_time()
				}
				else
				{
					i64::MIN
				};
				let timestamp = holder._aiInbox.timestamp_next();
				let content = holder._aiInbox.export_document(&document,timestamp)
					.map_err(AiInboxError::translateKey_get)?;
				return Ok(Some((generation,expectedTimestamp,document,content)));
			})?;
			let Some((generation,expectedTimestamp,document,mut content)) = prepared
			else {return Ok(())};
			let timestamp = content.timestamp;
			Self::export_crypt_content(&mut content,&crypto)
				.map_err(|_| "FRONTERROR_CRYPTO_ENCRYPT_FAILED")?;
			let result = API_module_updateIfCurrent(content,expectedTimestamp).await
				.map_err(Self::aiInbox_networkErrorKey)?;
			if (!Self::getSingleton().with_untracked(|holder| holder.network_generation_isActive(epoch,generation)))
			{
				return Err("FRONTAI_INBOX_SAVE_INTERRUPTED");
			}
			match result
			{
				ModuleReturnUpdate::OK => {
					let applied = Self::getSingleton().try_update(|holder| {
						if (!holder.network_generation_isActive(epoch,generation))
						{
							return false;
						}
						holder._aiInbox.saved_apply(document,timestamp);
						return true;
					}).unwrap_or(false);
					if (!applied)
					{
						return Err("FRONTAI_INBOX_SAVE_INTERRUPTED");
					}
					return Ok(());
				},
				ModuleReturnUpdate::OUTDATED(mut remote) => {
					if (remote.id.id != AiInboxHolder::MODULE_ID || remote.typeModule != AiInboxHolder::MODULE_NAME)
					{
						return Err("FRONTAI_INBOX_INVALID");
					}
					Self::import_decrypt_content(&mut remote,&crypto)
						.map_err(|_| "FRONTERROR_CRYPTO_DECRYPT_FAILED")?;
					let mut imported = AiInboxHolder::new();
					imported.import(remote).map_err(AiInboxError::translateKey_get)?;
					let applied = Self::getSingleton().try_update(|holder| {
						if (!holder.network_generation_isActive(epoch,generation))
						{
							return false;
						}
						holder._aiInbox.loaded_apply(imported);
						return true;
					}).unwrap_or(false);
					if (!applied)
					{
						return Err("FRONTAI_INBOX_SAVE_INTERRUPTED");
					}
				},
			}
		}
		return Err("FRONTAI_INBOX_SAVE_CONFLICT");
	}

	fn aiInbox_networkErrorKey(error: ModuleApiError) -> &'static str
	{
		return match error
		{
			ModuleApiError::AUTH_REQUIRED => "FRONTAI_INBOX_SAVE_AUTH_REQUIRED",
			ModuleApiError::NOT_FOUND | ModuleApiError::SERVER_ERROR => "FRONTAI_INBOX_SAVE_SERVER_ERROR",
		};
	}

	pub(crate) fn aiChat_get() -> Option<(ArcRwSignal<ChatDocument>,ArcRwSignal<ChatRuntime>)>
	{
		return Self::getSingleton().with(|holder| {
			if (!holder._aiChatReady || holder._aiChat.migration_isRunning())
			{
				return None;
			}
			return Some((holder._aiChat.document_get(),holder._aiChat.runtime_get()));
		});
	}

	pub(crate) fn aiChat_isReady() -> bool
	{
		return Self::getSingleton().with(|holder| holder._aiChatReady && !holder._aiChat.migration_isRunning());
	}

	pub(crate) fn aiChat_workspaceOpen(epoch: ModuleHolderEpoch) -> bool
	{
		return Self::getSingleton().try_update(|holder| {
			if (!holder.lifecycle_epoch_isActive(epoch) || !holder._aiChatReady || holder._aiChat.migration_isRunning())
			{
				return false;
			}
			holder._aiChat.runtime_get().update(|runtime| runtime.workspace_open());
			return true;
		}).unwrap_or(false);
	}

	pub(crate) fn aiChat_workspaceClose(epoch: ModuleHolderEpoch)
	{
		let _ = Self::getSingleton().try_update(|holder| {
			if (holder.lifecycle_epoch_isActive(epoch))
			{
				holder._aiChat.runtime_get().update(|runtime| runtime.workspace_close());
			}
		});
	}

	pub(crate) fn aiChat_changed(epoch: ModuleHolderEpoch)
	{
		let moduleHolder = Self::getSingleton();
		let shouldStart = moduleHolder.try_update(|holder| {
			if (!holder.network_generation_get(epoch).is_some() || !holder._aiChatReady)
			{
				return false;
			}
			holder._aiChat.changed_mark();
			return holder._aiChat.save_begin();
		}).unwrap_or(false);
		if (shouldStart)
		{
			Self::task_spawn(epoch,async move {Self::aiChat_saveLoop(epoch).await;});
		}
	}

	pub(crate) fn aiChat_migration_isNeeded() -> bool
	{
		return Self::getSingleton().with(|holder| holder._aiChatReady && holder._aiChat.migration_isNeeded());
	}

	pub(crate) fn aiChat_migration_start(epoch: ModuleHolderEpoch)
	{
		let moduleHolder = Self::getSingleton();
		let shouldStart = moduleHolder.try_update(|holder| {
			return holder.lifecycle_epoch_isActive(epoch)
				&& holder._aiChatReady
				&& holder._aiChat.migration_begin();
		}).unwrap_or(false);
		if (shouldStart)
		{
			Self::task_spawn(epoch,async move {Self::aiChat_migrationRun(epoch).await;});
		}
	}

	async fn aiChat_saveLoop(epoch: ModuleHolderEpoch)
	{
		let clientState = ClientState::expect();
		let Some(crypto) = clientState.crypto_get()
		else
		{
			let _ = Self::getSingleton().try_update(|holder| holder._aiChat.save_finish(Some("MODULE_CHAT_SAVE_CRYPTO_ERROR")));
			return;
		};
		loop
		{
			let prepared = Self::getSingleton().try_update(|holder| {
				if (!holder.network_generation_get(epoch).is_some())
				{
					return Err("MODULE_CHAT_SAVE_INTERRUPTED");
				}
				return holder._aiChat.save_prepare().map_err(|error| error.translateKey_get());
			});
			let prepared = match prepared
			{
				Some(Ok(Some(content))) => content,
				Some(Ok(None)) => {
					let _ = Self::getSingleton().try_update(|holder| holder._aiChat.save_finish(None));
					return;
				},
				Some(Err(errorKey)) => {
					let _ = Self::getSingleton().try_update(|holder| holder._aiChat.save_finish(Some(errorKey)));
					return;
				},
				None => return,
			};
			let timestamp = prepared.timestamp;
			let mut encrypted = prepared;
			if (Self::export_crypt_content(&mut encrypted,&crypto).is_err())
			{
				let _ = Self::getSingleton().try_update(|holder| holder._aiChat.save_finish(Some("MODULE_CHAT_SAVE_CRYPTO_ERROR")));
				return;
			}
			let result = match API_module_update(encrypted,false).await
			{
				Ok(result) => result,
				Err(error) => {
					let key = Self::aiChat_networkErrorKey(error);
					let _ = Self::getSingleton().try_update(|holder| holder._aiChat.save_finish(Some(key)));
					return;
				},
			};
			if (!Self::lifecycle_isActive(epoch))
			{
				return;
			}
			match result
			{
				ModuleReturnUpdate::OK => {
					let mustContinue = Self::getSingleton().try_update(|holder| {
						if (!holder.lifecycle_epoch_isActive(epoch))
						{
							return false;
						}
						holder._aiChat.save_succeeded(timestamp);
						if (holder._aiChat.cache_mustUpdate())
						{
							return true;
						}
						holder._aiChat.save_finish(None);
						return false;
					}).unwrap_or(false);
					if (!mustContinue)
					{
						return;
					}
				},
				ModuleReturnUpdate::OUTDATED(mut content) => {
					if (content.id.id != AiChatHolder::MODULE_ID
						|| content.typeModule != AiChatHolder::MODULE_NAME
						|| Self::import_decrypt_content(&mut content,&crypto).is_err())
					{
						let _ = Self::getSingleton().try_update(|holder| holder._aiChat.save_finish(Some("MODULE_CHAT_SAVE_CRYPTO_ERROR")));
						return;
					}
					let _ = Self::getSingleton().try_update(|holder| {
						let errorKey = if (holder._aiChat.save_remoteImport(content).is_ok())
						{
							"MODULE_CHAT_SAVE_OUTDATED"
						}
						else
						{
							"MODULE_CHAT_ERROR_DOCUMENT_INVALID"
						};
						holder._aiChat.save_finish(Some(errorKey));
					});
					return;
				},
			}
		}
	}

	async fn aiChat_migrationRun(epoch: ModuleHolderEpoch)
	{
		let clientState = ClientState::expect();
		let Some(crypto) = clientState.crypto_get()
		else
		{
			let _ = Self::getSingleton().try_update(|holder| holder._aiChat.migration_finish(Some("MODULE_CHAT_MIGRATION_FAILED")));
			return;
		};
		for _ in 0..3
		{
			let prepared = Self::getSingleton().try_update(|holder| holder._aiChat.migration_prepare());
			let migration = match prepared
			{
				Some(Ok(migration)) => migration,
				_ => {
					let _ = Self::getSingleton().try_update(|holder| holder._aiChat.migration_finish(Some("MODULE_CHAT_MIGRATION_FAILED")));
					return;
				},
			};
			let savedTimestamp = if let Some(mut content) = migration.content
			{
				let timestamp = content.timestamp;
				if (Self::export_crypt_content(&mut content,&crypto).is_err())
				{
					let _ = Self::getSingleton().try_update(|holder| holder._aiChat.migration_finish(Some("MODULE_CHAT_MIGRATION_FAILED")));
					return;
				}
				match API_module_update(content,false).await
				{
					Ok(ModuleReturnUpdate::OK) => Some(timestamp),
					Ok(ModuleReturnUpdate::OUTDATED(mut remote)) => {
						if (remote.id.id != AiChatHolder::MODULE_ID
							|| remote.typeModule != AiChatHolder::MODULE_NAME
							|| Self::import_decrypt_content(&mut remote,&crypto).is_err()
							|| !Self::getSingleton().try_update(|holder| holder._aiChat.migration_remoteImport(remote).is_ok()).unwrap_or(false))
						{
							let _ = Self::getSingleton().try_update(|holder| holder._aiChat.migration_finish(Some("MODULE_CHAT_MIGRATION_FAILED")));
							return;
						}
						continue;
					},
					Err(error) => {
						let key = Self::aiChat_networkErrorKey(error);
						let _ = Self::getSingleton().try_update(|holder| holder._aiChat.migration_finish(Some(key)));
						return;
					},
				}
			}
			else
			{
				None
			};
			if (!Self::lifecycle_isActive(epoch))
			{
				return;
			}
			let _ = Self::getSingleton().try_update(|holder| holder._aiChat.migration_saved_apply(savedTimestamp));
			for legacyId in migration.legacyIds
			{
				match API_module_remove(legacyId.clone()).await
				{
					Ok(()) | Err(ModuleApiError::NOT_FOUND) => {
						let _ = Self::getSingleton().try_update(|holder| holder._aiChat.migration_legacyRemoved(&legacyId));
					},
					Err(error) => {
						let key = Self::aiChat_networkErrorKey(error);
						let _ = Self::getSingleton().try_update(|holder| holder._aiChat.migration_finish(Some(key)));
						return;
					},
				}
			}
			let _ = Self::getSingleton().try_update(|holder| holder._aiChat.migration_finish(None));
			return;
		}
		let _ = Self::getSingleton().try_update(|holder| holder._aiChat.migration_finish(Some("MODULE_CHAT_MIGRATION_FAILED")));
	}

	fn aiChat_networkErrorKey(error: ModuleApiError) -> &'static str
	{
		return match error
		{
			ModuleApiError::AUTH_REQUIRED => "MODULE_CHAT_SAVE_AUTH_REQUIRED",
			ModuleApiError::NOT_FOUND | ModuleApiError::SERVER_ERROR => "MODULE_CHAT_SAVE_SERVER_ERROR",
		};
	}

	pub(crate) async fn aiConfig_save(epoch: ModuleHolderEpoch, document: AiConfigDocument) -> Result<(),AiConfigSaveError>
	{
		document.validate()?;
		let clientState = ClientState::expect();
		if (!clientState.login_isConnected_untracked())
		{
			return Err(AiConfigSaveError::AUTH_REQUIRED);
		}
		let crypto = clientState.crypto_get().ok_or(AiConfigSaveError::CRYPTO_CONTEXT_MISSING)?;
		let moduleHolder = Self::getSingleton();
		let prepared: Result<Option<(u64,i64,ModuleContent)>,AiConfigSaveError> = moduleHolder.with_untracked(|holder| {
			let generation = holder.network_generation_get(epoch).ok_or(AiConfigSaveError::LIFECYCLE_CLOSED)?;
			if (holder._aiConfig.document_get() == document)
			{
				return Ok(None);
			}
			let timestamp = holder._aiConfig.timestamp_next();
			let mut content = holder._aiConfig.export_document(&document,timestamp)?;
			content.content = crypto.encrypt(&content.content).map_err(|_| AiConfigSaveError::CRYPTO_ENCRYPT_FAILED)?;
			return Ok(Some((generation,timestamp,content)));
		});
		let prepared = prepared?;
		let Some((generation,timestamp,content)) = prepared else {return Ok(());};

		let result = API_module_update(content,false).await.map_err(|error| match error
		{
			ModuleApiError::AUTH_REQUIRED => AiConfigSaveError::AUTH_REQUIRED,
			ModuleApiError::NOT_FOUND | ModuleApiError::SERVER_ERROR => AiConfigSaveError::SERVER_ERROR,
		})?;
		if (!moduleHolder.with_untracked(|holder| holder.network_generation_isActive(epoch,generation)))
		{
			return Err(AiConfigSaveError::LIFECYCLE_CLOSED);
		}

		match result
		{
			ModuleReturnUpdate::OK => {
				let applied = moduleHolder.try_update(|holder| {
					if (!holder.network_generation_isActive(epoch,generation))
					{
						return false;
					}
					holder._aiConfig.saved_apply(document,timestamp);
					return true;
				}).unwrap_or(false);
				if (!applied)
				{
					return Err(AiConfigSaveError::LIFECYCLE_CLOSED);
				}
				return Ok(());
			},
			ModuleReturnUpdate::OUTDATED(mut content) => {
				if (content.typeModule != AiConfigHolder::MODULE_NAME)
				{
					return Err(AiConfigSaveError::SERVER_ERROR);
				}
				content.content = crypto.decrypt(&content.content).map_err(|_| AiConfigSaveError::CRYPTO_DECRYPT_FAILED)?;
				let applied = moduleHolder.try_update(|holder| {
					if (!holder.network_generation_isActive(epoch,generation))
					{
						return Err(AiConfigSaveError::LIFECYCLE_CLOSED);
					}
					return holder._aiConfig.import(content).map_err(AiConfigSaveError::Configuration);
				});
				match applied
				{
					Some(Ok(())) => return Err(AiConfigSaveError::OUTDATED),
					Some(Err(error)) => return Err(error),
					None => return Err(AiConfigSaveError::LIFECYCLE_CLOSED),
				}
			},
		}
	}

	pub(crate) async fn aiConfig_userSave(
		epoch: ModuleHolderEpoch,
		mut document: AiConfigDocument,
	) -> Result<(),AiConfigSaveError>
	{
		let current = Self::getSingleton().with_untracked(|holder| {
			holder.network_generation_get(epoch).ok_or(AiConfigSaveError::LIFECYCLE_CLOSED)?;
			return Ok::<_,AiConfigSaveError>(holder._aiConfig.document_get());
		})?;
		document.automationRuntime_reconcile(&current);
		return Self::aiConfig_save(epoch,document).await;
	}

	pub(crate) fn lifecycle_isOpen() -> bool
	{
		return Self::getSingleton().with_untracked(|holder| holder._activeEpoch.is_some());
	}

	pub(crate) fn network_suspend()
	{
		let _ = Self::getSingleton().try_update(|holder| holder.network_suspend_inner());
	}

	pub(crate) fn network_resume()
	{
		let _ = Self::getSingleton().try_update(|holder| holder.network_resume_inner());
	}

	fn network_suspend_inner(&mut self)
	{
		if (self._networkSuspended)
		{
			return;
		}
		self._networkGeneration = self._networkGeneration.checked_add(1)
			.expect("ModuleHolder network generation exhausted");
		self._networkSuspended = true;
		for cron in self._crons.values_mut()
		{
			cron.pause();
		}
	}

	fn network_resume_inner(&mut self)
	{
		if (!self._networkSuspended)
		{
			return;
		}
		self._networkSuspended = false;
		for cron in self._crons.values_mut()
		{
			cron.resume();
		}
	}

	fn lifecycle_open_inner(&mut self) -> (ModuleHolderEpoch, Option<Owner>)
	{
		let previousOwner = self.lifecycle_close_inner();
		self._epochCounter = self._epochCounter.checked_add(1)
			.expect("ModuleHolder lifecycle epoch exhausted");
		let epoch = ModuleHolderEpoch(self._epochCounter);
		self._activeEpoch = Some(epoch);
		self._taskOwner = Some(Owner::new());
		return (epoch, previousOwner);
	}

	fn lifecycle_close_inner(&mut self) -> Option<Owner>
	{
		self._activeEpoch = None;
		self._networkGeneration = self._networkGeneration.checked_add(1)
			.expect("ModuleHolder network generation exhausted");
		self._networkSuspended = false;
		let owner = self._taskOwner.take();
		self._crons.clear();
		self._moduleActions = None;
		self._blocks.clear();
		self._links = LinksHolder::new();
		self._aiConfig = AiConfigHolder::new();
		self._aiConfigReady = false;
		self._aiChat = AiChatHolder::new();
		self._aiChatReady = false;
		self._aiInbox = AiInboxHolder::new();
		self._aiInboxReady = false;
		self._aiAutomation.clear();
		self._aiAutomationInbox.clear();
		self._aiAutomationRunning = false;
		self._blockNb = 0;
		return owner;
	}

	fn lifecycle_closeIf_inner(&mut self, epoch: ModuleHolderEpoch) -> (bool, Option<Owner>)
	{
		if (!self.lifecycle_epoch_isActive(epoch))
		{
			return (false, None);
		}
		return (true, self.lifecycle_close_inner());
	}

	fn lifecycle_epoch_isActive(&self, epoch: ModuleHolderEpoch) -> bool
	{
		return self._activeEpoch == Some(epoch);
	}

	fn network_generation_get(&self, epoch: ModuleHolderEpoch) -> Option<u64>
	{
		if (!self.lifecycle_epoch_isActive(epoch) || self._networkSuspended)
		{
			return None;
		}
		return Some(self._networkGeneration);
	}

	fn network_generation_isActive(&self, epoch: ModuleHolderEpoch, generation: u64) -> bool
	{
		return self.lifecycle_epoch_isActive(epoch)
			&& !self._networkSuspended
			&& self._networkGeneration == generation;
	}

	fn lifecycle_owner_get(&self, epoch: ModuleHolderEpoch) -> Option<Owner>
	{
		if (!self.lifecycle_epoch_isActive(epoch))
		{
			return None;
		}
		return self._taskOwner.clone();
	}

	pub(crate) fn moduleActions_set(&mut self, epoch: ModuleHolderEpoch, ma: module_actions::ModuleActionFn)
	{
		if (self.lifecycle_epoch_isActive(epoch))
		{
			self._moduleActions = Some(ma);
		}
	}

	fn network_apply(&mut self, epoch: ModuleHolderEpoch, mut toApply: API_return_apply,toaster: ToasterContext) -> Option<ModuleRefreshTask>
	{
		if (!self.lifecycle_epoch_isActive(epoch))
		{
			return None;
		}
		toApply.retrieve.into_iter().for_each(|f| f(self));
		toApply.update.into_iter().for_each(|f| f(self));

		return self.module_refresh_prepare(epoch, toApply.moduleIdToRefresh.drain(..).collect(), toaster);
	}

	pub(crate) fn network_deferredCall(moduleHolder: ArcRwSignal<ModuleHolder>, epoch: ModuleHolderEpoch, toaster: ToasterContext, apiCall: impl FnOnce(ArcRwSignal<ModuleHolder>) -> Option<ApiCall>, toastingSuccess: Option<AllFrontUIEnum>) -> impl Future<Output = ()>
	{
		async move {
			let Some(networkGeneration) = moduleHolder.with_untracked(|holder| holder.network_generation_get(epoch)) else {return};
			let Some(apiCall) = apiCall(moduleHolder.clone()) else {return;};
			let mut apiResult = apiCall.await;
			if (!moduleHolder.with_untracked(|holder| holder.network_generation_isActive(epoch,networkGeneration)))
			{
				return;
			}
			let hasErrors = !apiResult.error.is_empty();
			let authenticationRequired = apiResult.authenticationRequired;
			let authenticationWasLocal = if (authenticationRequired)
			{
				ClientState::expect().login_isConnected_untracked()
			}
			else
			{
				false
			};

			// if they are some error
			for err in apiResult.error.drain(..) {
				if (!authenticationRequired || authenticationWasLocal)
				{
					toastingErr(&toaster, err).await;
					if (!moduleHolder.with_untracked(|holder| holder.network_generation_isActive(epoch,networkGeneration)))
					{
						return;
					}
				}
			};

			if (authenticationRequired)
			{
				if (ClientState::expect().local_clear().is_err())
				{
					toastingErr(&toaster, AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
				}
				Self::lifecycle_closeIf(epoch);
				return;
			}

			if (!hasErrors)
			{
				if let Some(toastingSuccess) = toastingSuccess
				{
					toaster_helpers::toastingSuccess(&toaster, toastingSuccess).await;
					if (!moduleHolder.with_untracked(|holder| holder.network_generation_isActive(epoch,networkGeneration)))
					{
						return;
					}
				}
			}

			let refreshTask = moduleHolder
				.try_update(|holder| holder.network_apply(epoch, apiResult,toaster))
				.flatten();
			if let Some(refreshTask) = refreshTask
			{
				refreshTask.spawn();
			}
		}
	}

	fn network_error_apply(apiReturn: &mut API_return_apply, error: ModuleApiError)
	{
		if (error == ModuleApiError::AUTH_REQUIRED)
		{
			apiReturn.authenticationRequired = true;
		}
		apiReturn.error.push(error.into());
	}

	fn network_deferredCall_inner<AsyncCaller, AsyncReturn, DataType, DataPrepare>(moduleHolder: ArcRwSignal<ModuleHolder>, prepare: DataPrepare, async_caller: AsyncCaller) -> Option<ApiCall>
	where
		DataPrepare: Fn(&ModuleHolder, &ClientCryptoContext) -> Result<DataType, AllFrontErrorEnum> + 'static,
		AsyncCaller: Fn(DataType) -> AsyncReturn + 'static,
		AsyncReturn: Future<Output = API_return_apply> + 'static,
		DataType: 'static,
	{
		let clientState = ClientState::expect();
		if (!clientState.login_isConnected_untracked())
		{
			return None;
		}
		let Some(crypto) = clientState.crypto_get()
		else
		{
			return Some(Self::network_localError_get(AllFrontErrorEnum::CRYPTO_CONTEXT_MISSING));
		};

		return moduleHolder.with_untracked(|holder| {
			if (holder._networkSuspended)
			{
				return None;
			}
			let preparedVar = match prepare(holder, &crypto)
			{
				Ok(preparedVar) => preparedVar,
				Err(error) => return Some(Self::network_localError_get(error)),
			};
			return Some(
				Box::pin(async move {
					return async_caller(preparedVar).await;
				}) as ApiCall
			);
		});
	}

	fn network_localError_get(error: AllFrontErrorEnum) -> ApiCall
	{
		return Box::pin(async move {
			let mut result = API_return_apply::default();
			result.error.push(error);
			return result;
		});
	}

	////////////////////////////////////////
	// START MODULES UPDATE ZONE ---
	////////////////////////////////////////

	pub fn network_modules_update_caller(moduleHolder: ArcRwSignal<ModuleHolder>) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(
			moduleHolder,
			|holder, crypto| holder.network_modules_update_prepare(crypto).map(|modules| (modules, true)),
			Self::network_modules_update_async,
		);
	}

	fn network_modules_update_prepare(
		&self,
		crypto: &ClientCryptoContext,
	) -> Result<Vec<ModuleContent>, AllFrontErrorEnum>
	{
		let mut moduleToUpdateData = vec![];

		let mut thisModuleContent = self._links.export();
		Self::export_crypt_content(&mut thisModuleContent, crypto)?;
		thisModuleContent.id = self._links.id_get();
		moduleToUpdateData.push(thisModuleContent);

		for (key, oneModule) in self._blocks.iter()
		{
			let mut thisModuleContent =oneModule.with_untracked(|module| module.export());
			Self::export_crypt_content(&mut thisModuleContent, crypto)?;
			thisModuleContent.id = key.clone();
			moduleToUpdateData.push(thisModuleContent);
		}

		return Ok(moduleToUpdateData);
	}

	async fn network_modules_update_async((moduleToUpdate, overwrite): (Vec<ModuleContent>, bool)) -> API_return_apply
	{
		if(moduleToUpdate.len()==0) {return API_return_apply::default();}

		let mut apiReturn = API_return_apply::default();

		match API_modules_update(moduleToUpdate, overwrite).await
		{
			Ok(_) => {},
			Err(err) => {
				Self::network_error_apply(&mut apiReturn, err);
				return apiReturn;
			}
		};

		return apiReturn;
	}

	////////////////////////////////////////
	// START MODULES UPDATE ZONE ---
	////////////////////////////////////////

	////////////////////////////////////////
	// START MONO MODULE UPDATE ZONE ---
	////////////////////////////////////////

	pub fn network_module_update_caller(moduleHolder: ArcRwSignal<ModuleHolder>, module: ModuleID) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(
			moduleHolder,
			move |holder, crypto| holder.network_module_update_prepare(module.clone(), crypto).map(|modules| (modules, false)),
			Self::network_modules_update_async,
		);
	}

	fn network_module_update_prepare(&self, moduleId: ModuleID, crypto: &ClientCryptoContext) -> Result<Vec<ModuleContent>, AllFrontErrorEnum>
	{
		let mut moduleToRetrieveData = vec![];

		for (key, oneModule) in self._blocks.iter()
			.filter(|(moduleIdSearch, _)| *moduleIdSearch == &moduleId)
		{
			let mut thisModuleContent =oneModule.with_untracked(|module| module.export());
			Self::export_crypt_content(&mut thisModuleContent, crypto)?;
			thisModuleContent.id = key.clone();
			moduleToRetrieveData.push(thisModuleContent);
		}

		return Ok(moduleToRetrieveData);
	}

	////////////////////////////////////////
	// END MONO MODULE UPDATE ZONE ---
	////////////////////////////////////////

	////////////////////////////////////////
	// START MODULES RETRIEVE ZONE ---
	////////////////////////////////////////

	pub fn network_modules_retrieve_caller(moduleHolder: ArcRwSignal<ModuleHolder>, forceUpdate: bool) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(moduleHolder, move |holder, crypto| Ok((holder.network_modules_retrieve_prepare(forceUpdate), crypto.clone())), Self::network_modules_retrieve_async);
	}

	fn network_modules_retrieve_prepare(
		&self,
		forceUpdate: bool,
	) -> Vec<ApiModulesID>
	{
		let mut moduleToRetrieveData = vec![];
		if (forceUpdate || self._aiConfig.cache_mustUpdate())
		{
			moduleToRetrieveData.push(ApiModulesID{
				key: self._aiConfig.id_get(),
				timestamp: if forceUpdate {i64::MIN} else {self._aiConfig.cache_time()},
			});
		}
		if (forceUpdate || self._aiChat.cache_mustUpdate())
		{
			moduleToRetrieveData.push(ApiModulesID{
				key: self._aiChat.id_get(),
				timestamp: if forceUpdate {i64::MIN} else {self._aiChat.cache_time()},
			});
		}
		if (forceUpdate || self._aiInbox.cache_mustUpdate())
		{
			moduleToRetrieveData.push(ApiModulesID{
				key: self._aiInbox.id_get(),
				timestamp: if forceUpdate {i64::MIN} else {self._aiInbox.cache_time()},
			});
		}
		if (forceUpdate || self._links.cache_mustUpdate())
		{
			moduleToRetrieveData.push(ApiModulesID{ key: self._links.id_get(), timestamp: self._links.cache_time() });
		}

		for (key, oneModule) in self._blocks.iter()
		{
			let (cacheMustUpdate,cacheTime) = oneModule.with_untracked(|module| (module.inner().cache_mustUpdate(),module.inner().cache_time()));
			if (forceUpdate || cacheMustUpdate)
			{
				moduleToRetrieveData.push(ApiModulesID{ key: key.clone(), timestamp: cacheTime });
			}
		}

		return moduleToRetrieveData;
	}

	async fn network_modules_retrieve_async((moduleToRetrieve, crypto): (Vec<ApiModulesID>, ClientCryptoContext)) -> API_return_apply
	{
		if(moduleToRetrieve.len()==0) {return API_return_apply::default();}

		let mut apiReturn = API_return_apply::default();
		let aiConfigRequested = moduleToRetrieve.iter()
			.any(|module| module.key.id == AiConfigHolder::MODULE_ID);
		let aiChatRequested = moduleToRetrieve.iter()
			.any(|module| module.key.id == AiChatHolder::MODULE_ID);
		let aiInboxRequested = moduleToRetrieve.iter()
			.any(|module| module.key.id == AiInboxHolder::MODULE_ID);

		let apiReturnModules = match API_modules_retrieve(moduleToRetrieve).await
		{
			Ok(r) => r,
			Err(err) => {
				Self::network_error_apply(&mut apiReturn, err);
				return apiReturn;
			}
		};

		let mut aiConfigValid = true;
		let mut aiChatValid = true;
		let mut aiInboxValid = true;
		for (moduleId, moduleResult) in apiReturnModules {
			let ModuleReturnRetrieve::UPDATED(content) = moduleResult else {continue;};
			let aiReservedContent = moduleId.id == AiConfigHolder::MODULE_ID
				|| content.typeModule == AiConfigHolder::MODULE_NAME;
			let aiChatReservedContent = moduleId.id == AiChatHolder::MODULE_ID
				|| content.typeModule == AiChatHolder::MODULE_NAME;
			let aiInboxReservedContent = moduleId.id == AiInboxHolder::MODULE_ID
				|| content.typeModule == AiInboxHolder::MODULE_NAME;
			let errorCount = apiReturn.error.len();
			if (Self::module_inner_retrieve(&mut apiReturn, content, moduleId.clone(), &crypto))
			{
				apiReturn.moduleIdToRefresh.push(moduleId);
			}
			else if (apiReturn.error.len() > errorCount)
			{
				if (aiReservedContent) {aiConfigValid = false;}
				if (aiChatReservedContent) {aiChatValid = false;}
				if (aiInboxReservedContent) {aiInboxValid = false;}
			}
		}
		if (aiConfigRequested && aiConfigValid)
		{
			apiReturn.retrieve.push(Box::new(|moduleHolder| moduleHolder._aiConfigReady = true));
		}
		if (aiChatRequested && aiChatValid)
		{
			apiReturn.retrieve.push(Box::new(|moduleHolder| moduleHolder._aiChatReady = true));
		}
		if (aiInboxRequested && aiInboxValid)
		{
			apiReturn.retrieve.push(Box::new(|moduleHolder| moduleHolder._aiInboxReady = true));
		}

		return apiReturn;
	}

	////////////////////////////////////////
	// END MODULES RETRIEVE ZONE ---
	////////////////////////////////////////

	////////////////////////////////////////
	// START MONO MODULE RETRIEVE ZONE ---
	////////////////////////////////////////

	pub fn network_module_retrieve_caller(moduleHolder: ArcRwSignal<ModuleHolder>, module: ModuleID, forceUpdate: bool) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(moduleHolder, move |holder, crypto| Ok((holder.network_module_retrieve_prepare(module.clone(), forceUpdate), crypto.clone())), Self::network_module_retrieve_async);
	}

	fn network_module_retrieve_prepare(
		&self,
		moduleId: ModuleID,
		forceUpdate: bool,
	) -> Option<ApiModulesID>
	{
		for (key, oneModule) in self._blocks.iter()
			.filter(|(moduleIdSearch, _)| *moduleIdSearch == &moduleId)
		{
			let (cacheMustUpdate,cacheTime) = oneModule.with_untracked(|module| (module.inner().cache_mustUpdate(),module.inner().cache_time()));
			if (forceUpdate || cacheMustUpdate)
			{
				return Some(ApiModulesID{ key: key.clone(), timestamp: cacheTime });
			}
		}

		return None;
	}

	// do not apply auto module refresh
	async fn network_module_retrieve_async((moduleToRetrieveRaw, crypto): (Option<ApiModulesID>, ClientCryptoContext)) -> API_return_apply
	{
		let Some(moduleToRetrieve) = moduleToRetrieveRaw else {return API_return_apply::default();};

		let mut apiReturn = API_return_apply::default();

		let moduleId = moduleToRetrieve.key.clone();
		let moduleResult = match API_module_retrieve(moduleToRetrieve).await
		{
			Ok(r) => r,
			Err(err) => {
				Self::network_error_apply(&mut apiReturn, err);
				return apiReturn;
			}
		};

		let ModuleReturnRetrieve::UPDATED(content) = moduleResult else {return apiReturn};
		Self::module_inner_retrieve(&mut apiReturn, content, moduleId, &crypto);
		return apiReturn;
	}

	fn module_inner_retrieve(apiReturn: &mut API_return_apply, mut content: ModuleContent, moduleId: ModuleID, crypto: &ClientCryptoContext) -> bool
	{
		if ((moduleId.id == AiConfigHolder::MODULE_ID) != (content.typeModule == AiConfigHolder::MODULE_NAME))
		{
			apiReturn.error.push(AllFrontErrorEnum::AI_CONFIG_INVALID);
			return false;
		}
		if ((moduleId.id == AiChatHolder::MODULE_ID) != (content.typeModule == AiChatHolder::MODULE_NAME))
		{
			apiReturn.error.push(AllFrontErrorEnum::AI_CHAT_INVALID);
			return false;
		}
		if ((moduleId.id == AiInboxHolder::MODULE_ID) != (content.typeModule == AiInboxHolder::MODULE_NAME))
		{
			apiReturn.error.push(AllFrontErrorEnum::AI_INBOX_INVALID);
			return false;
		}
		if (Self::import_decrypt_content(&mut content, crypto).is_err())
		{
			apiReturn.error.push(AllFrontErrorEnum::CRYPTO_DECRYPT_FAILED);
			return false;
		}

		if (content.typeModule == LinksHolder::MODULE_NAME)
		{
			let addReturnWork = move |moduleHolder: &mut ModuleHolder| {
				moduleHolder._links.id_set(content.id.clone());
				moduleHolder._links.import(content);
			};
			apiReturn.retrieve.push(Box::new(addReturnWork));
			return true;
		}
		if (content.typeModule == AiConfigHolder::MODULE_NAME)
		{
			let mut imported = AiConfigHolder::new();
			if (imported.import(content).is_err())
			{
				apiReturn.error.push(AllFrontErrorEnum::AI_CONFIG_INVALID);
				return false;
			}
			apiReturn.retrieve.push(Box::new(move |moduleHolder| moduleHolder._aiConfig = imported));
			return false;
		}
		if (content.typeModule == AiChatHolder::MODULE_NAME)
		{
			let mut imported = AiChatHolder::new();
			if (imported.import(content).is_err())
			{
				apiReturn.error.push(AllFrontErrorEnum::AI_CHAT_INVALID);
				return false;
			}
			apiReturn.retrieve.push(Box::new(move |moduleHolder| moduleHolder._aiChat.loaded_apply(imported)));
			return false;
		}
		if (content.typeModule == AiInboxHolder::MODULE_NAME)
		{
			let mut imported = AiInboxHolder::new();
			if (imported.import(content).is_err())
			{
				apiReturn.error.push(AllFrontErrorEnum::AI_INBOX_INVALID);
				return false;
			}
			apiReturn.retrieve.push(Box::new(move |moduleHolder| moduleHolder._aiInbox.loaded_apply(imported)));
			return false;
		}
		if (content.typeModule == AiChatHolder::LEGACY_MODULE_NAME)
		{
			let legacy = match AiChatHolder::legacy_prepare(content)
			{
				Ok(legacy) => legacy,
				Err(_) => {
					apiReturn.error.push(AllFrontErrorEnum::AI_CHAT_INVALID);
					return false;
				},
			};
			apiReturn.retrieve.push(Box::new(move |moduleHolder| moduleHolder._aiChat.legacy_apply(legacy)));
			return false;
		}

		let addReturnWork = move |moduleHolder: &mut ModuleHolder| {
			if let Some(foundModule) = moduleHolder._blocks.get_mut(&moduleId)
			{
				foundModule.update(|module| module.import(content.clone()));
			}
			else
			{
				let Some(moduleType) = ModuleType::newFromModuleContent(&content) else {return;};
				let thisModule = ModulePositions::newFromModuleContent(content, moduleType);
				if let Some(existing) = moduleHolder._blocks.get_mut(&moduleId)
				{
					existing.set(thisModule);
				}
				else
				{moduleHolder._blocks.insert(moduleId.clone(), ArcRwSignal::new(thisModule));}
			}

			let refreshTime = moduleHolder._blocks.get_mut(&moduleId).unwrap().with_untracked(|module| module.inner().refresh_time());
			if let Some(actions) = &moduleHolder._moduleActions
			{
				Self::add_cron(
					refreshTime,
					moduleId.clone(),
					&mut moduleHolder._crons,
					actions.clone(),
				);
			}
		};
		apiReturn.retrieve.push(Box::new(addReturnWork));
		return true;
	}

	////////////////////////////////////////
	// END MONO MODULE RETRIEVE ZONE ---
	////////////////////////////////////////

	////////////////////////////////////////
	// START MODULE REMOVE ZONE ---
	////////////////////////////////////////


	pub fn network_module_remove_caller(moduleHolder: ArcRwSignal<ModuleHolder>, moduleToRemove: ModuleID) -> Option<ApiCall>
	{
		return Self::network_deferredCall_inner(moduleHolder, move |_, _| Ok(moduleToRemove.clone()), Self::network_module_remove_async);
	}

	async fn network_module_remove_async(moduleToRetrieve: ModuleID) -> API_return_apply
	{
		let mut apiReturn = API_return_apply::default();

		match API_module_remove(moduleToRetrieve.clone()).await
		{
			Ok(_) => {},
			Err(err) => {
				Self::network_error_apply(&mut apiReturn, err);
				return apiReturn;
			}
		};

		let addReturnWork = move |moduleHolder: &mut ModuleHolder| {
			 moduleHolder._crons.remove(&moduleToRetrieve);
			moduleHolder._blocks.remove(&moduleToRetrieve);
		};
		apiReturn.retrieve.push(Box::new(addReturnWork));

		return apiReturn;
	}


	////////////////////////////////////////
	// START MODULE REMOVE ZONE ---
	////////////////////////////////////////

	pub(super) fn module_refresh(epoch: ModuleHolderEpoch, modulesId: Vec<ModuleID>, toaster: ToasterContext)
	{
		let refreshTask = Self::getSingleton().with_untracked(|holder| {
			holder.module_refresh_prepare(epoch, modulesId, toaster)
		});
		if let Some(refreshTask) = refreshTask
		{
			refreshTask.spawn();
		}
	}

	fn module_refresh_prepare(&self, epoch: ModuleHolderEpoch, modulesId: Vec<ModuleID>, toaster: ToasterContext) -> Option<ModuleRefreshTask>
	{
		if (self._networkSuspended)
		{
			return None;
		}
		let owner = self.lifecycle_owner_get(epoch)?;
		let mut allBoxedFuture = vec![];
		for moduleId in modulesId
		{
			let Some(oneModule) = self._blocks.get(&moduleId)
			else
			{
				continue;
			};

			if let Some(actions) = &self._moduleActions
			{
				let tmp = oneModule.with_untracked(|module| module.inner()
					.refresh(actions.clone(), moduleId.clone(), toaster.clone()));
				if let Some(refreshFutur) = tmp
				{
					allBoxedFuture.push(refreshFutur);
				}
			}
		}
		if (allBoxedFuture.is_empty())
		{
			return None;
		}
		return Some(ModuleRefreshTask {
			owner,
			futures: allBoxedFuture,
		});
	}

	fn add_cron(
		refreshTimeRaw: RefreshTime,
		moduleId: ModuleID,
		crons: &mut HashMap<ModuleID, PausableStocker>,
		moduleActions: module_actions::ModuleActionFn,
	)
	{
		let timeMinute = match refreshTimeRaw
		{
			RefreshTime::NONE => {
				crons.remove(&moduleId);
				return;
			},
			RefreshTime::MINUTES(i) => i as u32,
			RefreshTime::HOURS(h) => h as u32 * 60,
		};

		let timeMillisecond = timeMinute * 60 * 1000;



		if let Some(cron) = crons.get_mut(&moduleId) {
			cron.set_interval(timeMillisecond);
			return;
		}

		let refresh_fn = moduleActions.refreshFn.clone();
		let tick_module_id = moduleId.clone();
		let tick = Arc::new(move || {
			(refresh_fn)(tick_module_id.clone());
		});

		crons.insert(moduleId, PausableStocker::new(timeMillisecond, tick));

	}

	pub fn links_get(&self) -> &LinksHolder
	{
		return &self._links;
	}

	pub fn links_get_mut(&mut self) -> &mut LinksHolder
	{
		return &mut self._links;
	}

	pub fn blocks_get(&self) -> &HashMap<ModuleID, ArcRwSignal<ModulePositions<ModuleType>>>
	{
		return &self._blocks;
	}

	pub fn blocks_view(&self) -> Vec<(ModuleID, ArcRwSignal<ModulePositions<ModuleType>>)> {
		let mut blocks = self._blocks
			.iter()
			.map(|(id, module)| (id.clone(), module.clone()))
			.collect::<Vec<_>>();
		blocks.sort_by(|(leftId,leftModule),(rightId,rightModule)| {
			let leftOrder = leftModule.with(|module| module.visual_order_get());
			let rightOrder = rightModule.with(|module| module.visual_order_get());
			return leftOrder.cmp(&rightOrder).then_with(|| leftId.cmp(rightId));
		});
		return blocks;
	}

	pub(crate) fn blocks_insert(&mut self, epoch: ModuleHolderEpoch, newmodule: ModulePositions<ModuleType>)
	{
		if (!self.lifecycle_epoch_isActive(epoch))
		{
			return;
		}
		newmodule.depth_set(self._blockNb as u32);
		self._blocks.insert(ModuleID::new(), ArcRwSignal::new(newmodule));
		self._blockNb += 1;
	}

	/// This function is used to decrypt the content of a moduleContent before generating the module
	/// return if the content have been correctly decrypted
	fn import_decrypt_content(moduleContent: &mut ModuleContent, crypto: &ClientCryptoContext) -> Result<(), AllFrontErrorEnum>
	{
		moduleContent.content = crypto.decrypt(&moduleContent.content)
			.map_err(|_| AllFrontErrorEnum::CRYPTO_DECRYPT_FAILED)?;
		return Ok(());
	}

	/// This function is used to encrypt the content of a moduleContent before sending it to the server
	/// return if the content have been correctly encrypted
	fn export_crypt_content(moduleContent: &mut ModuleContent, crypto: &ClientCryptoContext) -> Result<(), AllFrontErrorEnum>
	{
		moduleContent.content = crypto.encrypt(&moduleContent.content)
			.map_err(|_| AllFrontErrorEnum::CRYPTO_ENCRYPT_FAILED)?;
		return Ok(());
	}
}
