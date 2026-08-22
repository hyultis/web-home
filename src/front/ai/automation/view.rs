use super::{
	AiActionCapability,AiAutomationContext,AiAutomationSource,AiAutomationTarget,AiAutomationTargetAction,
	AiConfirmationPolicy,AiModuleCapabilities,AiNamedValue,AiTextChoice,AiValue,AI_AUTOMATION_CONTEXT_MAXIMUM,
	AI_AUTOMATION_CALLS_PER_DAY,AI_AUTOMATION_CALLS_PER_HOUR,
	AI_AUTOMATION_CONTEXT_NAME_MAXIMUM_BYTES,AI_AUTOMATION_INSTRUCTIONS_MAXIMUM_BYTES,
};
use crate::api::modules::components::ModuleID;
use crate::front::ai::configuration::AiOptionsDraft;
use crate::front::modules::module_holder::ModuleHolder;
use crate::front::utils::translate::TranslateText;
use crate::front::utils::users_data::ClientState;
use leptos::prelude::{
	ArcRwSignal,AriaAttributes,BindAttribute,ClassAttribute,CollectView,Effect,ElementChild,For,Get,GetUntracked,
	GlobalAttributes,IntoAny,OnAttribute,OnTargetAttribute,PropAttribute,RwSignal,Set,Signal,
	Update,
};
use leptos::{component,view,IntoView};
use std::sync::Arc;

#[derive(Clone)]
struct SourceFieldChoice
{
	id: String,
	translateKey: &'static str,
}

#[derive(Clone)]
struct SourceChoice
{
	moduleId: ModuleID,
	moduleType: String,
	event: String,
	eventTranslateKey: &'static str,
	fields: Vec<SourceFieldChoice>,
}

#[derive(Clone)]
struct TargetChoice
{
	moduleId: ModuleID,
	moduleType: String,
	action: String,
	actionTranslateKey: &'static str,
	capability: AiActionCapability,
	fixedArguments: Vec<TargetArgumentChoice>,
}

#[derive(Clone)]
struct TargetArgumentChoice
{
	id: String,
	translateKey: &'static str,
	values: Vec<AiTextChoice>,
}

#[component]
pub(in crate::front::ai) fn AiAutomationEditor(
	aiDraft: AiOptionsDraft,
	configurationSaving: RwSignal<bool>,
	clientState: ClientState,
) -> impl IntoView
{
	let contexts = aiDraft.contexts_signal();
	let modules = ModuleHolder::aiAutomationModules_get();
	let (sources,targets,catalogError) = match modules
	{
		Ok(modules) => {
			let (sources,targets) = choices_get(&modules);
			(Arc::new(sources),Arc::new(targets),None)
		},
		Err(error) => (Arc::new(Vec::new()),Arc::new(Vec::new()),Some(error.translateKey_get())),
	};
	let selectedContextId = RwSignal::new(contexts.get_untracked().first()
		.map(|context| context.get_untracked().id));
	let disabled = Signal::derive(move || configurationSaving.get() || clientState.passwordRotation_runningIsActive());
	let createSources = sources.clone();
	let createTargets = targets.clone();
	let createContext = move |_| {
		if (disabled.get_untracked() || contexts.get_untracked().len() >= AI_AUTOMATION_CONTEXT_MAXIMUM)
		{
			return;
		}
		let (Some(source),Some(target)) = (createSources.first(),createTargets.first()) else {return};
		let context = ArcRwSignal::new(AiAutomationContext::new(
			AiAutomationSource {
				moduleId: source.moduleId.clone(),
				event: source.event.clone(),
				fields: source.fields.iter().map(|field| field.id.clone()).collect(),
			},
			AiAutomationTarget {
				moduleId: target.moduleId.clone(),
				actions: vec![AiAutomationTargetAction {
					action: target.action.clone(),
					confirmation: target.capability.confirmation_get(AiConfirmationPolicy::Confirm),
					fixedArguments: Vec::new(),
				}],
			},
		));
		let id = context.get_untracked().id;
		contexts.update(|contexts| contexts.push(context));
		selectedContextId.set(Some(id));
	};
	let hasTemplate = !sources.is_empty() && !targets.is_empty();

	return view! {
		<div class="ai_automation">
			<div class="ai_automation_intro">
				<div>
					<h2 id="ai-automation-title"><TranslateText key="FRONTAI_WORKSPACE_AUTOMATIONS_TITLE"/></h2>
					<p><TranslateText key="FRONTAI_WORKSPACE_AUTOMATIONS_HELP"/></p>
				</div>
				<button
					type="button"
					disabled=move || disabled.get() || !hasTemplate || (contexts.get().len() >= AI_AUTOMATION_CONTEXT_MAXIMUM)
					on:click=createContext
				>
					<i class="iconoir-plus" aria-hidden="true"></i>
					<span><TranslateText key="FRONTAI_AUTOMATION_ADD"/></span>
				</button>
			</div>
			{catalogError.map(|key| view! {
				<p class="options_ai_status options_ai_status--error" role="alert"><TranslateText key=key/></p>
			})}
			{(!hasTemplate).then(|| view! {
				<p class="ai_automation_empty"><TranslateText key="FRONTAI_AUTOMATION_CAPABILITIES_EMPTY"/></p>
			})}
			<div class="ai_automation_layout">
				<nav class="ai_automation_list" aria-labelledby="ai-automation-title">
					<For
						each=move || contexts.get()
						key=|context| context.get_untracked().id
						children=move |context| {
							let selectContext = context.clone();
							let selectedContext = context.clone();
							let nameContext = context.clone();
							let enabledContext = context.clone();
							view! {
								<button
									type="button"
									class:selected=move || selectedContextId.get().as_ref()
										.is_some_and(|id| id == &selectedContext.get().id)
									on:click=move |_| selectedContextId.set(Some(selectContext.get_untracked().id))
								>
									<span>{move || {
										let name = nameContext.get().name;
										if name.is_empty()
										{
											return view! {<TranslateText key="FRONTAI_AUTOMATION_UNTITLED"/>}.into_any();
										}
										return view! {{name}}.into_any();
									}}</span>
									<small>{move || if enabledContext.get().enabled {
										view! {<TranslateText key="FRONTAI_AUTOMATION_ENABLED_SHORT"/>}.into_any()
									} else {
										view! {<TranslateText key="FRONTAI_AUTOMATION_DISABLED_SHORT"/>}.into_any()
									}}</small>
								</button>
							}
						}
					/>
					{move || contexts.get().is_empty().then(|| view! {
						<p><TranslateText key="FRONTAI_AUTOMATION_LIST_EMPTY"/></p>
					})}
				</nav>
				<div class="ai_automation_editor">{move || {
					let Some(selectedId) = selectedContextId.get() else {
						return view! {<p class="ai_automation_no_selection"><TranslateText key="FRONTAI_AUTOMATION_NO_SELECTION"/></p>}.into_any();
					};
					let selected = contexts.get().into_iter()
						.find(|context| context.get_untracked().id == selectedId);
					let Some(context) = selected else {
						return view! {<p class="ai_automation_no_selection"><TranslateText key="FRONTAI_AUTOMATION_NO_SELECTION"/></p>}.into_any();
					};
					view! {
						<AiAutomationContextForm
							context
							contexts
							selectedContextId
							sources=sources.clone()
							targets=targets.clone()
							disabled
						/>
					}.into_any()
				}}</div>
			</div>
		</div>
	}.into_any();
}

#[component]
fn AiAutomationContextForm(
	context: ArcRwSignal<AiAutomationContext>,
	contexts: RwSignal<Vec<ArcRwSignal<AiAutomationContext>>>,
	selectedContextId: RwSignal<Option<String>>,
	sources: Arc<Vec<SourceChoice>>,
	targets: Arc<Vec<TargetChoice>>,
	disabled: Signal<bool>,
) -> impl IntoView
{
	let nameContext = context.clone();
	let enabledContext = context.clone();
	let instructionsContext = context.clone();
	let sourceContext = context.clone();
	let sourceValueContext = context.clone();
	let sourceValueSources = sources.clone();
	let sourceOptions = sources.clone();
	let fieldsContext = context.clone();
	let fieldsSources = sources.clone();
	let actionsContext = context.clone();
	let actionTargets = targets.clone();
	let deleteContext = context.clone();
	let validityClassContext = context.clone();
	let validityTextContext = context.clone();
	let validityClassSources = sources.clone();
	let validityTextSources = sources.clone();
	let validityClassTargets = targets.clone();
	let validityTextTargets = targets.clone();
	let enabledValueContext = context.clone();
	let recentExecutionContext = context.clone();
	let appliedActionContext = context.clone();
	let hourlyBudgetContext = context.clone();
	let dailyBudgetContext = context.clone();
	let contextId = context.get_untracked().id;
	let nameValue = RwSignal::new(context.get_untracked().name);
	let instructionsValue = RwSignal::new(context.get_untracked().instructions);
	Effect::new(move |_| {
		let value = nameValue.get();
		nameContext.update(|context| {
			if (context.name != value) {context.name = value;}
		});
	});
	Effect::new(move |_| {
		let value = instructionsValue.get();
		instructionsContext.update(|context| {
			if (context.instructions != value) {context.instructions = value;}
		});
	});
	let removeContext = move |_| {
		if (disabled.get_untracked())
		{
			return;
		}
		let id = deleteContext.get_untracked().id;
		contexts.update(|contexts| contexts.retain(|context| context.get_untracked().id != id));
		selectedContextId.set(contexts.get_untracked().first().map(|context| context.get_untracked().id));
	};

	return view! {
		<div class="ai_automation_form">
			<div class="ai_automation_form_header">
				<h3><TranslateText key="FRONTAI_AUTOMATION_DETAILS"/></h3>
				<span class="ai_automation_validity" class:error=move || !contextCapabilities_areUsable(&validityClassContext.get(),&validityClassSources,&validityClassTargets)>
					{move || if contextCapabilities_areUsable(&validityTextContext.get(),&validityTextSources,&validityTextTargets) {
						view! {<TranslateText key="FRONTAI_AUTOMATION_READY"/>}.into_any()
					} else {
						view! {<TranslateText key="FRONTAI_AUTOMATION_INCOMPLETE"/>}.into_any()
					}}
				</span>
			</div>
			<section class="ai_automation_activity">
				<h4><TranslateText key="FRONTAI_AUTOMATION_ACTIVITY"/></h4>
				<dl>
					<div>
						<dt><TranslateText key="FRONTAI_AUTOMATION_RECENT_EXECUTIONS"/></dt>
						<dd>{move || recentExecutionContext.get().checkpoint.recentExecutions.len()}</dd>
					</div>
					<div>
						<dt><TranslateText key="FRONTAI_AUTOMATION_APPLIED_ACTIONS"/></dt>
						<dd>{move || appliedActionContext.get().checkpoint.appliedActions.len()}</dd>
					</div>
					<div>
						<dt><TranslateText key="FRONTAI_AUTOMATION_HOURLY_CALLS"/></dt>
						<dd>{move || format!("{} / {}",hourlyBudgetContext.get().checkpoint.hour.calls,AI_AUTOMATION_CALLS_PER_HOUR)}</dd>
					</div>
					<div>
						<dt><TranslateText key="FRONTAI_AUTOMATION_DAILY_CALLS"/></dt>
						<dd>{move || format!("{} / {}",dailyBudgetContext.get().checkpoint.day.calls,AI_AUTOMATION_CALLS_PER_DAY)}</dd>
					</div>
				</dl>
			</section>
			<label class="ai_automation_field" for=format!("ai-automation-name-{}",contextId)>
				<span><TranslateText key="FRONTAI_AUTOMATION_NAME"/></span>
				<input
					id=format!("ai-automation-name-{}",contextId)
					type="text"
					maxlength=AI_AUTOMATION_CONTEXT_NAME_MAXIMUM_BYTES
					disabled=move || disabled.get()
					bind:value=nameValue
				/>
			</label>
			<label class="ai_automation_toggle">
				<input
					type="checkbox"
					disabled=move || disabled.get()
					prop:checked=move || enabledValueContext.get().enabled
					on:change:target=move |event| enabledContext.update(|context| context.enabled = event.target().checked())
				/>
				<span><TranslateText key="FRONTAI_AUTOMATION_ENABLED"/></span>
			</label>
			<label class="ai_automation_field">
				<span><TranslateText key="FRONTAI_AUTOMATION_SOURCE"/></span>
				<select
					disabled=move || disabled.get()
					prop:value=move || sourceChoiceIndex_get(&sourceValueContext.get(),&sourceValueSources).map(|index| index.to_string()).unwrap_or_default()
					on:change:target=move |event| {
						let Ok(index) = event.target().value().parse::<usize>() else {return};
						let Some(source) = sourceOptions.get(index) else {return};
						sourceContext.update(|context| context.source = AiAutomationSource {
							moduleId: source.moduleId.clone(),
							event: source.event.clone(),
							fields: source.fields.iter().map(|field| field.id.clone()).collect(),
						});
					}
				>
					{sources.iter().enumerate().map(|(index,source)| view! {
						<option value=index.to_string()>
							{source.moduleType.clone()} " · " <TranslateText key=source.eventTranslateKey/>
						</option>
					}).collect_view()}
				</select>
			</label>
			<fieldset class="ai_automation_choices">
				<legend><TranslateText key="FRONTAI_AUTOMATION_FIELDS"/></legend>
				{move || {
					let contextValue = fieldsContext.get();
					let Some(source) = sourceChoice_get(&contextValue,&fieldsSources) else {return view!{}.into_any();};
					source.fields.iter().map(|field| {
						let fieldId = field.id.clone();
						let checkedId = field.id.clone();
						let checkedContext = fieldsContext.clone();
						let updateContext = fieldsContext.clone();
						view! {
							<label>
								<input
									type="checkbox"
									disabled=move || disabled.get()
									prop:checked=move || checkedContext.get().source.fields.contains(&checkedId)
									on:change:target=move |event| updateContext.update(|context| {
										if event.target().checked()
										{
											if (!context.source.fields.contains(&fieldId)) {context.source.fields.push(fieldId.clone());}
										}
										else {context.source.fields.retain(|field| field != &fieldId);}
									})
								/>
								<span><TranslateText key=field.translateKey/></span>
							</label>
						}
					}).collect_view().into_any()
				}}
			</fieldset>
			<label class="ai_automation_field ai_automation_field--stacked">
				<span><TranslateText key="FRONTAI_AUTOMATION_INSTRUCTIONS"/></span>
				<textarea
					rows="6"
					maxlength=AI_AUTOMATION_INSTRUCTIONS_MAXIMUM_BYTES
					disabled=move || disabled.get()
					bind:value=instructionsValue
				></textarea>
			</label>
			<fieldset class="ai_automation_choices ai_automation_targets">
				<legend><TranslateText key="FRONTAI_AUTOMATION_TARGETS"/></legend>
				{actionTargets.iter().cloned().map(|target| view! {
					<AiAutomationTargetRow context=actionsContext.clone() target disabled/>
				}.into_any()).collect_view()}
			</fieldset>
			<div class="ai_automation_danger">
				<button type="button" class="danger" disabled=move || disabled.get() on:click=removeContext>
					<i class="iconoir-trash" aria-hidden="true"></i>
					<span><TranslateText key="FRONTAI_AUTOMATION_DELETE"/></span>
				</button>
			</div>
		</div>
	};
}

#[component]
fn AiAutomationTargetRow(
	context: ArcRwSignal<AiAutomationContext>,
	target: TargetChoice,
	disabled: Signal<bool>,
) -> impl IntoView
{
	let checkedContext = context.clone();
	let toggleContext = context.clone();
	let settingsContext = context.clone();
	let checkedTarget = target.clone();
	let toggleTarget = target.clone();
	let settingsTarget = target.clone();
	return view! {
		<div class="ai_automation_target">
			<label>
				<input
					type="checkbox"
					disabled=move || disabled.get()
					prop:checked=move || contextAction_get(&checkedContext.get(),&checkedTarget).is_some()
					on:change:target=move |event| contextAction_toggle(&toggleContext,&toggleTarget,event.target().checked())
				/>
				<span>{target.moduleType.clone()} " · " <TranslateText key=target.actionTranslateKey/></span>
			</label>
			{move || {
				let policyContext = settingsContext.clone();
				let policyTarget = settingsTarget.clone();
				let fixedContext = settingsContext.clone();
				let fixedTarget = settingsTarget.clone();
				contextAction_get(&settingsContext.get(),&settingsTarget).map(move |policy| view! {
					<div class="ai_automation_target_settings">
						{fixedTarget.fixedArguments.iter().cloned().map(|argument| view! {
							<AiAutomationTargetArgumentSelect
								context=fixedContext.clone()
								target=fixedTarget.clone()
								argument
								disabled
							/>
						}.into_any()).collect_view()}
						{if fixedTarget.capability.forcedConfirmation.is_some()
						{
							view! {
								<p class="ai_automation_policy ai_automation_policy--fixed">
									<span><TranslateText key="FRONTAI_AUTOMATION_CONFIRMATION"/></span>
									<strong><TranslateText key="FRONTAI_AUTOMATION_AUTOMATIC_REQUIRED"/></strong>
								</p>
							}.into_any()
						}
						else
						{
							view! {
								<label class="ai_automation_policy">
									<span><TranslateText key="FRONTAI_AUTOMATION_CONFIRMATION"/></span>
									<select
										disabled=move || disabled.get()
										prop:value=match policy {AiConfirmationPolicy::Confirm => "confirm",AiConfirmationPolicy::Automatic => "automatic"}
										on:change:target=move |event| {
											let policy = if event.target().value() == "automatic" {AiConfirmationPolicy::Automatic} else {AiConfirmationPolicy::Confirm};
											contextAction_policySet(&policyContext,&policyTarget,policy);
										}
									>
										<option value="confirm"><TranslateText key="FRONTAI_AUTOMATION_CONFIRM_EACH"/></option>
										<option value="automatic"><TranslateText key="FRONTAI_AUTOMATION_AUTOMATIC"/></option>
									</select>
								</label>
							}.into_any()
						}}
					</div>
				}.into_any())
			}}
		</div>
	}.into_any();
}

#[component]
fn AiAutomationTargetArgumentSelect(
	context: ArcRwSignal<AiAutomationContext>,
	target: TargetChoice,
	argument: TargetArgumentChoice,
	disabled: Signal<bool>,
) -> impl IntoView
{
	let selectedValue = contextAction_fixedValueGet(&context.get_untracked(),&target,&argument.id);
	let missingValue = (!selectedValue.is_empty()
		&& !argument.values.iter().any(|choice| choice.value == selectedValue))
		.then_some(selectedValue);
	let selectedContext = context.clone();
	let selectedTarget = target.clone();
	let selectedArgumentId = argument.id.clone();
	let updateArgumentId = argument.id.clone();
	return view! {
		<label class="ai_automation_target_argument">
			<span><TranslateText key=argument.translateKey/></span>
			<select
				disabled=move || disabled.get()
				prop:value=move || contextAction_fixedValueGet(
					&selectedContext.get(),&selectedTarget,&selectedArgumentId,
				)
				on:change:target=move |event| contextAction_fixedValueSet(
					&context,&target,&updateArgumentId,&event.target().value(),
				)
			>
				<option value=""><TranslateText key="FRONTAI_AUTOMATION_CHOOSE_VALUE"/></option>
				{missingValue.map(|value| view! {<option value=value.clone()>{value.clone()}</option>})}
				{argument.values.into_iter().map(|choice| view! {
					<option value=choice.value>{choice.label}</option>
				}).collect_view()}
			</select>
		</label>
	}.into_any();
}

fn choices_get(modules: &[AiModuleCapabilities]) -> (Vec<SourceChoice>,Vec<TargetChoice>)
{
	let mut sources = Vec::new();
	let mut targets = Vec::new();
	for module in modules
	{
		for event in &module.catalog.events
		{
			let Some(grant) = module.grant.events.iter().find(|grant| grant.event == event.id) else {continue};
			let fields = event.fields.iter()
				.filter(|field| grant.fields.iter().any(|allowed| allowed == field.id))
				.map(|field| SourceFieldChoice {id: field.id.to_string(),translateKey: field.translateKey})
				.collect::<Vec<_>>();
			if (!fields.is_empty())
			{
				sources.push(SourceChoice {
					moduleId: module.moduleId.clone(),
					moduleType: module.moduleType.clone(),
					event: event.id.to_string(),
					eventTranslateKey: event.translateKey,
					fields,
				});
			}
		}
		for action in &module.catalog.actions
		{
			if (module.grant.action_allows(action.id))
			{
				let fixedArguments = action.arguments.iter()
					.filter(|argument| argument.fixedByContext)
					.map(|argument| TargetArgumentChoice {
						id: argument.id.to_string(),
						translateKey: argument.translateKey,
						values: argument.allowedTextValues.clone(),
					})
					.collect();
				targets.push(TargetChoice {
					moduleId: module.moduleId.clone(),
					moduleType: module.moduleType.clone(),
					action: action.id.to_string(),
					actionTranslateKey: action.translateKey,
					capability: action.clone(),
					fixedArguments,
				});
			}
		}
	}
	return (sources,targets);
}

fn sourceChoiceIndex_get(context: &AiAutomationContext,sources: &[SourceChoice]) -> Option<usize>
{
	return sources.iter().position(|source| source.moduleId == context.source.moduleId && source.event == context.source.event);
}

fn contextCapabilities_areUsable(
	context: &AiAutomationContext,
	sources: &[SourceChoice],
	targets: &[TargetChoice],
) -> bool
{
	if (context.validate().is_err())
	{
		return false;
	}
	let Some(source) = sourceChoice_get(context,sources) else {return false};
	if (context.source.fields.iter().any(|field| !source.fields.iter().any(|available| available.id == *field)))
	{
		return false;
	}
	return context.targets.iter().all(|target| target.actions.iter().all(|action| {
		return targets.iter()
			.find(|available| available.moduleId == target.moduleId && available.action == action.action)
			.is_some_and(|available| action.fixedArguments_validate(&available.capability).is_ok()
				&& available.capability.confirmation_validate(action.confirmation).is_ok());
	}));
}

fn sourceChoice_get<'a>(context: &AiAutomationContext,sources: &'a [SourceChoice]) -> Option<&'a SourceChoice>
{
	return sourceChoiceIndex_get(context,sources).and_then(|index| sources.get(index));
}

fn contextAction_get(context: &AiAutomationContext,target: &TargetChoice) -> Option<AiConfirmationPolicy>
{
	return context.targets.iter().find(|current| current.moduleId == target.moduleId)
		.and_then(|current| current.actions.iter().find(|action| action.action == target.action))
		.map(|action| action.confirmation);
}

fn contextAction_toggle(context: &ArcRwSignal<AiAutomationContext>,target: &TargetChoice,enabled: bool)
{
	context.update(|context| {
		let targetIndex = context.targets.iter().position(|current| current.moduleId == target.moduleId);
		if (enabled)
		{
			if let Some(index) = targetIndex
			{
				if (!context.targets[index].actions.iter().any(|action| action.action == target.action))
				{
					context.targets[index].actions.push(AiAutomationTargetAction {
						action: target.action.clone(),
						confirmation: target.capability.confirmation_get(AiConfirmationPolicy::Confirm),
						fixedArguments: Vec::new(),
					});
				}
			}
			else
			{
				context.targets.push(AiAutomationTarget {
					moduleId: target.moduleId.clone(),
					actions: vec![AiAutomationTargetAction {
						action: target.action.clone(),
						confirmation: target.capability.confirmation_get(AiConfirmationPolicy::Confirm),
						fixedArguments: Vec::new(),
					}],
				});
			}
		}
		else if let Some(index) = targetIndex
		{
			context.targets[index].actions.retain(|action| action.action != target.action);
			if (context.targets[index].actions.is_empty())
			{
				context.targets.remove(index);
			}
		}
	});
}

fn contextAction_fixedValueGet(
	context: &AiAutomationContext,
	target: &TargetChoice,
	argumentId: &str,
) -> String
{
	return context.targets.iter().find(|current| current.moduleId == target.moduleId)
		.and_then(|current| current.actions.iter().find(|action| action.action == target.action))
		.and_then(|action| action.fixedArgument_get(argumentId))
		.and_then(|argument| match &argument.value
		{
			AiValue::Text(value) => Some(value.clone()),
			_ => None,
		})
		.unwrap_or_default();
}

fn contextAction_fixedValueSet(
	context: &ArcRwSignal<AiAutomationContext>,
	target: &TargetChoice,
	argumentId: &str,
	value: &str,
)
{
	let isAllowed = target.fixedArguments.iter()
		.find(|argument| argument.id == argumentId)
		.is_some_and(|argument| argument.values.iter().any(|choice| choice.value == value));
	context.update(|context| {
		let Some(action) = context.targets.iter_mut()
			.find(|current| current.moduleId == target.moduleId)
			.and_then(|current| current.actions.iter_mut().find(|action| action.action == target.action))
		else
		{
			return;
		};
		action.fixedArguments.retain(|argument| argument.id != argumentId);
		if (isAllowed)
		{
			action.fixedArguments.push(AiNamedValue {
				id: argumentId.to_string(),
				value: AiValue::Text(value.to_string()),
			});
			action.fixedArguments.sort_unstable_by_key(|argument| {
				return target.fixedArguments.iter().position(|available| available.id == argument.id)
					.unwrap_or(usize::MAX);
			});
		}
	});
}

fn contextAction_policySet(
	context: &ArcRwSignal<AiAutomationContext>,
	target: &TargetChoice,
	policy: AiConfirmationPolicy,
)
{
	if (target.capability.forcedConfirmation.is_some())
	{
		return;
	}
	context.update(|context| {
		if let Some(action) = context.targets.iter_mut()
			.find(|current| current.moduleId == target.moduleId)
			.and_then(|current| current.actions.iter_mut().find(|action| action.action == target.action))
		{
			action.confirmation = policy;
		}
	});
}
