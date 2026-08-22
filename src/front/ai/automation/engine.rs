use super::{
	AiAutomationCheckpoint,AiAutomationContext,AiAutomationError,AiAutomationEvent,
	AiAutomationResponse,AiEventCausation,AiModuleCapabilities,AiValidatedAction,
	AI_AUTOMATION_CALLS_PER_DAY,AI_AUTOMATION_CALLS_PER_HOUR,AI_AUTOMATION_EVENT_CONTEXT_MAXIMUM,
	AI_AUTOMATION_QUEUE_MAXIMUM,
};
use crate::global_security::hash;
use std::collections::VecDeque;

const HOUR_SECONDS: i64 = 60 * 60;
const DAY_SECONDS: i64 = 24 * HOUR_SECONDS;
const CHECKPOINT_HISTORY_MAXIMUM: usize = 128;

#[derive(Clone,Debug,Eq,PartialEq)]
pub(crate) struct AiQueuedExecution
{
	pub(crate) contextId: String,
	pub(crate) executionId: String,
	definitionFingerprint: String,
	pub(crate) event: AiAutomationEvent,
}

impl AiQueuedExecution
{
	pub(crate) fn contextDefinition_isSame(
		&self,
		context: &AiAutomationContext,
	) -> Result<bool,AiAutomationError>
	{
		return Ok(self.definitionFingerprint == context.executionDefinitionFingerprint_get()?);
	}
}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub(crate) enum AiExecutionOutcome
{
	Succeeded,
	Rejected,
	FailedTerminal,
	Ambiguous,
}

#[derive(Default)]
pub(crate) struct AiAutomationEngine
{
	pending: VecDeque<AiQueuedExecution>,
	running: Option<AiQueuedExecution>,
}

impl AiAutomationEngine
{
	pub(crate) fn event_enqueue(
		&mut self,
		contexts: &mut [AiAutomationContext],
		event: AiAutomationEvent,
		modules: &[AiModuleCapabilities],
		now: i64,
	) -> Result<Vec<(String,Result<(),AiAutomationError>)>,AiAutomationError>
	{
		event.validate()?;
		if (event.causation == AiEventCausation::AiAction)
		{
			return Ok(Vec::new());
		}
		let mut results = Vec::new();
		for context in contexts.iter_mut()
			.filter(|context| context.enabled
				&& context.source.moduleId == event.sourceModuleId
				&& context.source.event == event.event)
			.take(AI_AUTOMATION_EVENT_CONTEXT_MAXIMUM)
		{
			let result = self.context_enqueue(context,event.clone(),modules,now);
			results.push((context.id.clone(),result));
		}
		return Ok(results);
	}

	fn context_enqueue(
		&mut self,
		context: &mut AiAutomationContext,
		event: AiAutomationEvent,
		modules: &[AiModuleCapabilities],
		now: i64,
	) -> Result<(),AiAutomationError>
	{
		context.validate()?;
		Self::contextPermissions_validate(context,modules)?;
		let definitionFingerprint = context.executionDefinitionFingerprint_get()?;
		let executionId = hash(format!("{}\0{}\0{}",context.id,definitionFingerprint,event.eventId));
		if (context.checkpoint.cursor.as_ref() == Some(&event.eventId)
			|| context.checkpoint.recentExecutions.contains(&executionId)
			|| self.pending.iter().any(|execution| execution.executionId == executionId)
			|| self.running.as_ref().is_some_and(|execution| execution.executionId == executionId))
		{
			return Err(AiAutomationError::DuplicateExecution);
		}
		if (self.pending.len() + usize::from(self.running.is_some()) >= AI_AUTOMATION_QUEUE_MAXIMUM)
		{
			return Err(AiAutomationError::QueueFull);
		}
		Self::budget_consume(&mut context.checkpoint,now)?;
		self.pending.push_back(AiQueuedExecution {
			contextId: context.id.clone(),
			executionId,
			definitionFingerprint,
			event,
		});
		return Ok(());
	}

	pub(crate) fn next(&mut self) -> Option<AiQueuedExecution>
	{
		if (self.running.is_some())
		{
			return None;
		}
		self.running = self.pending.pop_front();
		return self.running.clone();
	}

	pub(crate) fn running_cancel(&mut self,contextId: &str)
	{
		if (self.running.as_ref().is_some_and(|execution| execution.contextId == contextId))
		{
			self.running = None;
		}
	}

	pub(crate) fn event_cancel(&mut self,event: &AiAutomationEvent)
	{
		self.pending.retain(|execution| {
			return execution.event.sourceModuleId != event.sourceModuleId
				|| execution.event.event != event.event
				|| execution.event.eventId != event.eventId;
		});
		if (self.running.as_ref().is_some_and(|execution| {
			return execution.event.sourceModuleId == event.sourceModuleId
				&& execution.event.event == event.event
				&& execution.event.eventId == event.eventId;
		}))
		{
			self.running = None;
		}
	}

	pub(crate) fn response_validate(
		&self,
		context: &AiAutomationContext,
		response: &AiAutomationResponse,
		modules: &[AiModuleCapabilities],
	) -> Result<Vec<AiValidatedAction>,AiAutomationError>
	{
		let execution = self.running.as_ref().ok_or(AiAutomationError::LifecycleClosed)?;
		if (execution.contextId != context.id)
		{
			return Err(AiAutomationError::LifecycleClosed);
		}
		response.validate()?;
		let mut validated = Vec::with_capacity(response.actions.len());
		for (index,request) in response.actions.iter().enumerate()
		{
			if (response.actions[..index].contains(request))
			{
				return Err(AiAutomationError::InvalidResponse);
			}
			let target = context.targets.iter().find(|target| target.moduleId == request.targetModuleId)
				.ok_or(AiAutomationError::PermissionDenied)?;
			let permission = target.actions.iter().find(|action| action.action == request.action)
				.ok_or(AiAutomationError::PermissionDenied)?;
			let module = modules.iter().find(|module| module.moduleId == request.targetModuleId)
				.ok_or(AiAutomationError::CapabilityUnavailable)?;
			if (!module.grant.action_allows(&request.action))
			{
				return Err(AiAutomationError::PermissionDenied);
			}
			let action = module.catalog.action_get(&request.action)
				.ok_or(AiAutomationError::CapabilityUnavailable)?;
			action.confirmation_validate(permission.confirmation)?;
			permission.responseArguments_validate(&request.arguments,action)
				.map_err(|_| AiAutomationError::InvalidResponse)?;
			if (!permission.responseArguments_match(&request.arguments))
			{
				return Err(AiAutomationError::PermissionDenied);
			}
			let actionKey = hash(format!("{}\0{index}",execution.executionId));
			if (context.checkpoint.appliedActions.contains(&actionKey))
			{
				return Err(AiAutomationError::DuplicateExecution);
			}
			validated.push(AiValidatedAction {
				actionKey,
				executionId: execution.executionId.clone(),
				targetModuleId: request.targetModuleId.clone(),
				action: request.action.clone(),
				arguments: request.arguments.clone(),
				confirmation: action.confirmation_get(permission.confirmation),
			});
		}
		return Ok(validated);
	}

	pub(crate) fn actionApplied_mark(context: &mut AiAutomationContext,action: &AiValidatedAction)
	{
		Self::history_push(&mut context.checkpoint.appliedActions,action.actionKey.clone());
	}

	pub(crate) fn finish(
		&mut self,
		context: &mut AiAutomationContext,
		outcome: AiExecutionOutcome,
	) -> Result<(),AiAutomationError>
	{
		if (!self.running.as_ref().is_some_and(|execution| execution.contextId == context.id))
		{
			return Err(AiAutomationError::LifecycleClosed);
		}
		let execution = self.running.take().ok_or(AiAutomationError::LifecycleClosed)?;
		Self::history_push(&mut context.checkpoint.recentExecutions,execution.executionId);
		if (outcome != AiExecutionOutcome::Ambiguous)
		{
			context.checkpoint.cursor = Some(execution.event.eventId);
		}
		return Ok(());
	}

	pub(crate) fn clear(&mut self)
	{
		self.pending.clear();
		self.running = None;
	}

	fn contextPermissions_validate(
		context: &AiAutomationContext,
		modules: &[AiModuleCapabilities],
	) -> Result<(),AiAutomationError>
	{
		let source = modules.iter().find(|module| module.moduleId == context.source.moduleId)
			.ok_or(AiAutomationError::CapabilityUnavailable)?;
		let event = source.catalog.event_get(&context.source.event)
			.ok_or(AiAutomationError::CapabilityUnavailable)?;
		if (!source.grant.event_allows(&context.source.event,&context.source.fields)
			|| context.source.fields.iter().any(|field| !event.fields.iter().any(|available| available.id == field)))
		{
			return Err(AiAutomationError::PermissionDenied);
		}
		for target in &context.targets
		{
			let module = modules.iter().find(|module| module.moduleId == target.moduleId)
				.ok_or(AiAutomationError::CapabilityUnavailable)?;
			for action in &target.actions
			{
				let capability = module.catalog.action_get(&action.action)
					.ok_or(AiAutomationError::CapabilityUnavailable)?;
				capability.confirmation_validate(action.confirmation)?;
				if (!module.grant.action_allows(&action.action))
				{
					return Err(AiAutomationError::PermissionDenied);
				}
				action.fixedArguments_validate(capability)?;
			}
		}
		return Ok(());
	}

	fn budget_consume(checkpoint: &mut AiAutomationCheckpoint,now: i64) -> Result<(),AiAutomationError>
	{
		if (checkpoint.hour.startedAt <= 0 || now.saturating_sub(checkpoint.hour.startedAt) >= HOUR_SECONDS)
		{
			checkpoint.hour.startedAt = now;
			checkpoint.hour.calls = 0;
		}
		if (checkpoint.day.startedAt <= 0 || now.saturating_sub(checkpoint.day.startedAt) >= DAY_SECONDS)
		{
			checkpoint.day.startedAt = now;
			checkpoint.day.calls = 0;
		}
		if (checkpoint.hour.calls >= AI_AUTOMATION_CALLS_PER_HOUR
			|| checkpoint.day.calls >= AI_AUTOMATION_CALLS_PER_DAY)
		{
			return Err(AiAutomationError::BudgetExceeded);
		}
		checkpoint.hour.calls += 1;
		checkpoint.day.calls += 1;
		return Ok(());
	}

	fn history_push(history: &mut Vec<String>,value: String)
	{
		if (history.contains(&value))
		{
			return;
		}
		history.push(value);
		if (history.len() > CHECKPOINT_HISTORY_MAXIMUM)
		{
			history.remove(0);
		}
	}
}

#[cfg(test)]
mod tests
{
	use super::*;
	use crate::api::modules::components::ModuleID;
	use crate::front::ai::automation::{
		AiActionCapability,AiAutomationResponseAction,AiAutomationSource,AiAutomationTarget,
		AiAutomationTargetAction,AiCapabilityCatalog,AiConfirmationPolicy,AiEventCapability,AiEventGrant,AiModuleGrant,
		AiNamedValue,AiTextChoice,AiValue,AiValueDefinition,AiValueKind,
	};

	fn modules_get() -> Vec<AiModuleCapabilities>
	{
		return vec![
			AiModuleCapabilities {
				moduleId: ModuleID {id: "source".to_string()},
				moduleType: "FAKE_SOURCE".to_string(),
				catalog: AiCapabilityCatalog {
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
					actions: Vec::new(),
				},
				grant: AiModuleGrant {
					events: vec![AiEventGrant {event: "item.created".to_string(),fields: vec!["title".to_string()]}],
					actions: Vec::new(),
				},
			},
			AiModuleCapabilities {
				moduleId: ModuleID {id: "target".to_string()},
				moduleType: "FAKE_TARGET".to_string(),
				catalog: AiCapabilityCatalog {
					events: Vec::new(),
					actions: vec![AiActionCapability {
						id: "item.add",
						translateKey: "TEST_ACTION",
						arguments: vec![AiValueDefinition {
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
						forcedConfirmation: None,
					}],
				},
				grant: AiModuleGrant {events: Vec::new(),actions: vec!["item.add".to_string()]},
			},
		];
	}

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
		context.name = "Fake workflow".to_string();
		context.instructions = "Create the target item.".to_string();
		context.enabled = true;
		return context;
	}

	fn event_get(causation: AiEventCausation) -> AiAutomationEvent
	{
		return AiAutomationEvent::new(
			ModuleID {id: "source".to_string()},
			"item.created".to_string(),
			"event-1".to_string(),
			10,
			causation,
		);
	}

	#[test]
	fn eventQueueRejectsReplayRemovedPermissionAndAiCausation()
	{
		let mut engine = AiAutomationEngine::default();
		let mut contexts = vec![context_get()];
		let modules = modules_get();
		assert_eq!(engine.event_enqueue(&mut contexts,event_get(AiEventCausation::External),&modules,100).unwrap(),vec![(contexts[0].id.clone(),Ok(()))]);
		assert_eq!(contexts[0].checkpoint.hour.calls,1);
		assert_eq!(engine.event_enqueue(&mut contexts,event_get(AiEventCausation::External),&modules,100).unwrap(),vec![(contexts[0].id.clone(),Err(AiAutomationError::DuplicateExecution))]);
		assert_eq!(contexts[0].checkpoint.hour.calls,1);
		assert!(engine.event_enqueue(&mut contexts,event_get(AiEventCausation::AiAction),&modules,100).unwrap().is_empty());

		let mut deniedModules = modules.clone();
		deniedModules[0].grant.events.clear();
		let mut other = context_get();
		other.id = "other-context".to_string();
		assert_eq!(engine.event_enqueue(&mut [other],event_get(AiEventCausation::External),&deniedModules,100).unwrap()[0].1,Err(AiAutomationError::PermissionDenied));
	}

	#[test]
	fn responseIsRevalidatedAndActionsAreIdempotent()
	{
		let mut engine = AiAutomationEngine::default();
		let mut context = context_get();
		let modules = modules_get();
		assert!(engine.event_enqueue(std::slice::from_mut(&mut context),event_get(AiEventCausation::External),&modules,100).unwrap()[0].1.is_ok());
		engine.next().unwrap();
		let response = AiAutomationResponse::test_get(vec![AiAutomationResponseAction {
			targetModuleId: ModuleID {id: "target".to_string()},
			action: "item.add".to_string(),
			arguments: vec![AiNamedValue {id: "title".to_string(),value: AiValue::Text("Meeting".to_string())}],
		}]);
		let actions = engine.response_validate(&context,&response,&modules).unwrap();
		assert_eq!(actions.len(),1);
		assert_eq!(actions[0].confirmation,AiConfirmationPolicy::Confirm);
		AiAutomationEngine::actionApplied_mark(&mut context,&actions[0]);
		assert_eq!(engine.response_validate(&context,&response,&modules),Err(AiAutomationError::DuplicateExecution));
		engine.finish(&mut context,AiExecutionOutcome::Succeeded).unwrap();
		assert_eq!(context.checkpoint.cursor.as_deref(),Some("event-1"));
	}

	#[test]
	fn fixedTargetArgumentIsRequiredAndCannotBeChangedByTheModel()
	{
		let mut modules = modules_get();
		modules[1].catalog.actions[0].arguments.insert(0,AiValueDefinition::textWithFixedChoices(
			"collection","TEST_COLLECTION",128,
			vec![
				AiTextChoice {value: "personal".to_string(),label: "Personal".to_string()},
				AiTextChoice {value: "work".to_string(),label: "Work".to_string()},
			],
		));
		let mut context = context_get();
		let mut engine = AiAutomationEngine::default();
		assert_eq!(
			engine.event_enqueue(
				std::slice::from_mut(&mut context),event_get(AiEventCausation::External),&modules,100,
			).unwrap()[0].1,
			Err(AiAutomationError::PermissionDenied),
		);

		context.targets[0].actions[0].fixedArguments.push(AiNamedValue {
			id: "collection".to_string(),value: AiValue::Text("personal".to_string()),
		});
		assert!(engine.event_enqueue(
			std::slice::from_mut(&mut context),event_get(AiEventCausation::External),&modules,100,
		).unwrap()[0].1.is_ok());
		engine.next().unwrap();
		let response = |collection: &str| AiAutomationResponse::test_get(vec![AiAutomationResponseAction {
			targetModuleId: ModuleID {id: "target".to_string()},
			action: "item.add".to_string(),
			arguments: vec![
				AiNamedValue {id: "collection".to_string(),value: AiValue::Text(collection.to_string())},
				AiNamedValue {id: "title".to_string(),value: AiValue::Text("Meeting".to_string())},
			],
		}]);
		assert_eq!(
			engine.response_validate(&context,&response("work"),&modules),
			Err(AiAutomationError::PermissionDenied),
		);
		assert!(engine.response_validate(&context,&response("personal"),&modules).is_ok());
	}

	#[test]
	fn responseRejectsAnExactlyDuplicatedAction()
	{
		let mut engine = AiAutomationEngine::default();
		let mut context = context_get();
		let modules = modules_get();
		engine.event_enqueue(
			std::slice::from_mut(&mut context),event_get(AiEventCausation::External),&modules,100,
		).unwrap();
		engine.next().unwrap();
		let action = AiAutomationResponseAction {
			targetModuleId: ModuleID {id: "target".to_string()},
			action: "item.add".to_string(),
			arguments: vec![AiNamedValue {id: "title".to_string(),value: AiValue::Text("Meeting".to_string())}],
		};
		let response = AiAutomationResponse::test_get(vec![action.clone(),action]);

		assert_eq!(engine.response_validate(&context,&response,&modules),Err(AiAutomationError::InvalidResponse));
	}

	#[test]
	fn executionIdentityChangesWithTheEffectiveContextDefinition()
	{
		let modules = modules_get();
		let mut firstContext = context_get();
		let mut changedContext = firstContext.clone();
		changedContext.instructions = "Create the target only when explicitly requested.".to_string();
		let mut firstEngine = AiAutomationEngine::default();
		let mut changedEngine = AiAutomationEngine::default();

		firstEngine.event_enqueue(
			std::slice::from_mut(&mut firstContext),event_get(AiEventCausation::External),&modules,100,
		).unwrap();
		changedEngine.event_enqueue(
			std::slice::from_mut(&mut changedContext),event_get(AiEventCausation::External),&modules,100,
		).unwrap();

		let firstExecution = firstEngine.next().unwrap();
		let changedExecution = changedEngine.next().unwrap();
		assert_ne!(firstExecution.executionId,changedExecution.executionId);
		assert!(firstExecution.contextDefinition_isSame(&firstContext).unwrap());
		assert!(!firstExecution.contextDefinition_isSame(&changedContext).unwrap());
	}

	#[test]
	fn cancellingAnEventRemovesAllOfItsQueuedContexts()
	{
		let modules = modules_get();
		let mut contexts = vec![context_get(),context_get()];
		contexts[1].id = "second-context".to_string();
		let event = event_get(AiEventCausation::External);
		let mut engine = AiAutomationEngine::default();
		let results = engine.event_enqueue(&mut contexts,event.clone(),&modules,100).unwrap();
		assert_eq!(results.len(),2);

		engine.event_cancel(&event);

		assert!(engine.next().is_none());
	}

	#[test]
	fn noActionInOneContextDoesNotConsumeTheNextContext()
	{
		let modules = modules_get();
		let mut contexts = vec![context_get(),context_get()];
		contexts[0].id = "summary-context".to_string();
		contexts[1].id = "todo-context".to_string();
		let mut engine = AiAutomationEngine::default();
		let results = engine.event_enqueue(
			&mut contexts,event_get(AiEventCausation::External),&modules,100,
		).unwrap();
		assert_eq!(results.len(),2);
		assert!(results.iter().all(|(_,result)| result.is_ok()));

		let first = engine.next().unwrap();
		assert_eq!(first.contextId,"summary-context");
		assert!(engine.response_validate(
			&contexts[0],&AiAutomationResponse::test_get(Vec::new()),&modules,
		).unwrap().is_empty());
		engine.finish(&mut contexts[0],AiExecutionOutcome::Succeeded).unwrap();

		let second = engine.next().unwrap();
		assert_eq!(second.contextId,"todo-context");
		assert!(contexts[1].checkpoint.cursor.is_none());
		engine.finish(&mut contexts[1],AiExecutionOutcome::Succeeded).unwrap();
		assert!(engine.next().is_none());
	}

	#[test]
	fn budgetAndAmbiguousOutcomeDoNotAdvanceCursor()
	{
		let modules = modules_get();
		let mut context = context_get();
		context.checkpoint.hour.startedAt = 100;
		context.checkpoint.hour.calls = AI_AUTOMATION_CALLS_PER_HOUR;
		let mut engine = AiAutomationEngine::default();
		assert_eq!(engine.event_enqueue(std::slice::from_mut(&mut context),event_get(AiEventCausation::External),&modules,100).unwrap()[0].1,Err(AiAutomationError::BudgetExceeded));

		context.checkpoint.hour.calls = 0;
		assert!(engine.event_enqueue(std::slice::from_mut(&mut context),event_get(AiEventCausation::External),&modules,100).unwrap()[0].1.is_ok());
		engine.next().unwrap();
		engine.finish(&mut context,AiExecutionOutcome::Ambiguous).unwrap();
		assert!(context.checkpoint.cursor.is_none());
	}

	#[test]
	fn missingTargetInvalidResponseAndLifecycleCloseFailClosed()
	{
		let mut engine = AiAutomationEngine::default();
		let mut context = context_get();
		let mut missingTargetModules = modules_get();
		missingTargetModules.pop();
		assert_eq!(
			engine.event_enqueue(std::slice::from_mut(&mut context),event_get(AiEventCausation::External),&missingTargetModules,100).unwrap()[0].1,
			Err(AiAutomationError::CapabilityUnavailable),
		);

		let modules = modules_get();
		assert!(engine.event_enqueue(std::slice::from_mut(&mut context),event_get(AiEventCausation::External),&modules,100).unwrap()[0].1.is_ok());
		engine.next().unwrap();
		let invalidResponse = AiAutomationResponse::test_get(vec![AiAutomationResponseAction {
			targetModuleId: ModuleID {id: "target".to_string()},
			action: "item.add".to_string(),
			arguments: vec![AiNamedValue {id: "title".to_string(),value: AiValue::Integer(1)}],
		}]);
		assert_eq!(engine.response_validate(&context,&invalidResponse,&modules),Err(AiAutomationError::InvalidResponse));
		engine.clear();
		assert_eq!(engine.response_validate(&context,&AiAutomationResponse::test_get(Vec::new()),&modules),Err(AiAutomationError::LifecycleClosed));
	}

	#[test]
	fn providerValueOutsideTheTargetFormatIsAnInvalidResponse()
	{
		let mut modules = modules_get();
		modules[1].catalog.actions[0].arguments[0] = AiValueDefinition::text("title","TEST_TITLE",true,64)
			.withTextConstraint(
				r"^[0-9]{4}-[0-9]{2}-[0-9]{2}(T[0-9]{2}:[0-9]{2}(:[0-9]{2})?)?$",
				"Local date or local date-time without a UTC offset.",
			);
		let mut engine = AiAutomationEngine::default();
		let mut context = context_get();
		engine.event_enqueue(
			std::slice::from_mut(&mut context),event_get(AiEventCausation::External),&modules,100,
		).unwrap();
		engine.next().unwrap();
		let response = AiAutomationResponse::test_get(vec![AiAutomationResponseAction {
			targetModuleId: ModuleID {id: "target".to_string()},
			action: "item.add".to_string(),
			arguments: vec![AiNamedValue {
				id: "title".to_string(),
				value: AiValue::Text("2026-08-21T13:01:40Z".to_string()),
			}],
		}]);

		assert_eq!(
			engine.response_validate(&context,&response,&modules),
			Err(AiAutomationError::InvalidResponse),
		);
		assert_eq!(AiAutomationError::InvalidResponse.translateKey_get(),"FRONTAI_AUTOMATION_RESPONSE_INVALID");
	}

	#[test]
	fn providerCalendarResponseUsesTheDeclaredTargetObjectAndPassesLocalValidation()
	{
		let collection = "https://calendar.invalid/user/appointments/";
		let targetId = ModuleID {id: "calendar-instance".to_string()};
		let mut modules = modules_get();
		modules[1].moduleId = targetId.clone();
		modules[1].catalog.actions = vec![AiActionCapability {
			id: "calendar.event.create",
			translateKey: "TEST_CALENDAR_CREATE",
			arguments: vec![
				AiValueDefinition::textWithFixedChoices(
					"collection","TEST_COLLECTION",4_096,
					vec![AiTextChoice {value: collection.to_string(),label: "Appointments".to_string()}],
				),
				AiValueDefinition::text("title","TEST_TITLE",true,4_096),
				AiValueDefinition::text("start","TEST_START",true,64).withTextConstraint(
					r"^[0-9]{4}-[0-9]{2}-[0-9]{2}(T[0-9]{2}:[0-9]{2}(:[0-9]{2})?)?$",
					"Local date or local date-time without a UTC offset.",
				),
				AiValueDefinition::text("end","TEST_END",false,64).withTextConstraint(
					r"^[0-9]{4}-[0-9]{2}-[0-9]{2}(T[0-9]{2}:[0-9]{2}(:[0-9]{2})?)?$",
					"Local date or local date-time without a UTC offset.",
				),
				AiValueDefinition::boolean("all_day","TEST_ALL_DAY",true),
				AiValueDefinition::text("timezone","TEST_TIMEZONE",false,128),
			],
			promptRules: Vec::new(),
			forcedConfirmation: None,
		}];
		modules[1].grant.actions = vec!["calendar.event.create".to_string()];

		let mut context = context_get();
		context.targets[0] = AiAutomationTarget {
			moduleId: targetId,
			actions: vec![AiAutomationTargetAction {
				action: "calendar.event.create".to_string(),
				confirmation: AiConfirmationPolicy::Confirm,
				fixedArguments: vec![AiNamedValue {
					id: "collection".to_string(),
					value: AiValue::Text(collection.to_string()),
				}],
			}],
		};
		let mut engine = AiAutomationEngine::default();
		engine.event_enqueue(
			std::slice::from_mut(&mut context),event_get(AiEventCausation::External),&modules,100,
		).unwrap();
		engine.next().unwrap();
		let response = AiAutomationResponse::parse(&format!(r#"{{
			"actions":[{{
				"action":"calendar.event.create",
				"arguments":[
					{{"id":"collection","value":{{"type":"text","value":"{collection}"}}}},
					{{"id":"title","value":{{"type":"text","value":"Appointment"}}}},
					{{"id":"start","value":{{"type":"text","value":"2026-08-24T16:45:00"}}}},
					{{"id":"end","value":{{"type":"text","value":"2026-08-24T17:45:00"}}}},
					{{"id":"all_day","value":{{"type":"boolean","value":false}}}}
				],
				"targetModuleId":{{"id":"calendar-instance"}}
			}}],
			"schemaVersion":1
		}}"#)).unwrap();

		let actions = engine.response_validate(&context,&response,&modules).unwrap();
		assert_eq!(actions.len(),1);
		assert_eq!(actions[0].targetModuleId.id,"calendar-instance");
	}

	#[test]
	fn fanoutAndQueueAreBounded()
	{
		let modules = modules_get();
		let mut contexts = (0..(AI_AUTOMATION_EVENT_CONTEXT_MAXIMUM + 1)).map(|index| {
			let mut context = context_get();
			context.id = format!("context-{index}");
			return context;
		}).collect::<Vec<_>>();
		let mut engine = AiAutomationEngine::default();
		assert_eq!(
			engine.event_enqueue(&mut contexts,event_get(AiEventCausation::External),&modules,100).unwrap().len(),
			AI_AUTOMATION_EVENT_CONTEXT_MAXIMUM,
		);

		engine.clear();
		for index in 0..AI_AUTOMATION_QUEUE_MAXIMUM
		{
			let mut context = context_get();
			context.id = format!("queue-context-{index}");
			let mut event = event_get(AiEventCausation::External);
			event.eventId = format!("event-{index}");
			assert!(engine.context_enqueue(&mut context,event,&modules,100).is_ok());
		}
		let mut overflowContext = context_get();
		overflowContext.id = "queue-overflow".to_string();
		let mut overflowEvent = event_get(AiEventCausation::External);
		overflowEvent.eventId = "event-overflow".to_string();
		assert_eq!(engine.context_enqueue(&mut overflowContext,overflowEvent,&modules,100),Err(AiAutomationError::QueueFull));
	}
}
