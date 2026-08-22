use super::chat::AiChatView;
use super::automation::history::AiAutomationHistoryView;
use super::automation::view::AiAutomationEditor;
use super::automation::{AiAutomationHistoryEntry,AiModuleCapabilities};
use super::configuration::{AiConfiguration,AiOptionsDraft,AiTestState};
use super::AiConfigSaveError;
use crate::front::modules::module_holder::{ModuleHolder,ModuleHolderEpoch};
use crate::front::utils::all_front_enum::AllFrontErrorEnum;
use crate::front::utils::dialog::{DialogData,DialogManager};
use crate::front::utils::toaster_helpers::toastingErr;
use crate::front::utils::translate::TranslateText;
use crate::front::utils::users_data::ClientState;
use leptoaster::expect_toaster;
use leptos::ev::KeyboardEvent;
use leptos::prelude::{
	AriaAttributes,Callback,Callable,ClassAttribute,ElementChild,Get,GetUntracked,GlobalAttributes,
	IntoAny,OnAttribute,RwSignal,Set,
};
use leptos::{component,view,IntoView};
use leptos_router::hooks;
#[cfg(feature="hydrate")]
use wasm_bindgen::JsCast;

#[derive(Clone,Copy,Debug,Default,Eq,PartialEq)]
enum AiWorkspaceTab
{
	#[default]
	Chat,
	Configuration,
	Automations,
	History,
}

impl AiWorkspaceTab
{
	#[cfg(feature="hydrate")]
	fn id_get(self) -> &'static str
	{
		return match self
		{
			Self::Chat => "ai-workspace-tab-chat",
			Self::Configuration => "ai-workspace-tab-configuration",
			Self::Automations => "ai-workspace-tab-automations",
			Self::History => "ai-workspace-tab-history",
		};
	}
}

#[component]
pub(crate) fn AiWorkspaceButton(lifecycleEpoch: ModuleHolderEpoch) -> impl IntoView
{
	let dialogManager = leptos::prelude::expect_context::<DialogManager>();
	let clientState = ClientState::expect();
	let ollamaValidatedFingerprint = RwSignal::new(None::<String>);
	let openDialog = dialogManager.clone();
	let openState = clientState.clone();
	let openFn = move |_| {
		if (!ModuleHolder::aiConfig_isReady() || !ModuleHolder::aiChat_workspaceOpen(lifecycleEpoch))
		{
			return;
		}
		let Some((document,runtime)) = ModuleHolder::aiChat_get() else
		{
			ModuleHolder::aiChat_workspaceClose(lifecycleEpoch);
			return;
		};
		let selectedTab = RwSignal::new(AiWorkspaceTab::Chat);
		let configurationSaving = RwSignal::new(false);
		let configurationFeedback = RwSignal::new(None::<(&'static str,bool)>);
		let aiDocument = ModuleHolder::aiConfig_get();
		let history = aiDocument.history.clone();
		let historyModules = ModuleHolder::aiAutomationModules_get().unwrap_or_default();
		let aiDraft = AiOptionsDraft::new(aiDocument);
		let aiTestState = RwSignal::new(AiTestState::Idle);

		let headerSelectedTab = selectedTab;
		let bodyState = openState.clone();
		let bodyDraft = aiDraft.clone();
		let closeState = openState.clone();
		let dialog = DialogData::new()
			.setTitle("FRONTAI_WORKSPACE_TITLE")
			.setHeaderStart(move || view! {
				<AiWorkspaceTabs selectedTab=headerSelectedTab/>
			}.into_any())
			.setBody(move || view! {
				<AiWorkspace
					document=document.clone()
					runtime=runtime.clone()
					selectedTab
					clientState=bodyState.clone()
					aiDraft=bodyDraft.clone()
					aiTestState
					configurationSaving
					configurationFeedback
					ollamaValidatedFingerprint
					history=history.clone()
					historyModules=historyModules.clone()
					lifecycleEpoch
				/>
			}.into_any())
			.setIsWorkspace(true)
			.setButtonValidateTitle(None::<String>)
			.setButtonCloseTitle(Some("FRONTAI_WORKSPACE_CLOSE"))
			.setOnClose(move |_| ModuleHolder::aiChat_workspaceClose(lifecycleEpoch))
			.setCanClose(move || !configurationSaving.get()
				&& !aiTestState.get().isBusy()
				&& closeState.passwordRotation_canClose());
		openDialog.open(dialog);
	};

	return view! {
		<button
			type="button"
			class="header_ai_button"
			class:header_ai_button--pending=move || ModuleHolder::aiChat_get()
				.is_some_and(|(_,runtime)| runtime.get().pending.is_some())
			class:header_ai_button--attention=move || ModuleHolder::aiChat_get()
				.is_some_and(|(_,runtime)| runtime.get().responseReady)
			disabled=move || !ModuleHolder::aiConfig_isReady() || !ModuleHolder::aiChat_isReady()
			on:click=openFn
		>
			<i class="iconoir-chat-bubble-empty" aria-hidden="true"></i>
			<span><TranslateText key="FRONTAI_WORKSPACE_ACTION"/></span>
			{move || ModuleHolder::aiChat_get().and_then(|(_,runtime)| {
				let runtime = runtime.get();
				if (runtime.pending.is_some())
				{
					return Some(view! {
						<span class="visually_hidden" role="status"><TranslateText key="FRONTAI_WORKSPACE_REQUEST_PENDING"/></span>
					}.into_any());
				}
				if (runtime.responseReady)
				{
					return Some(view! {
						<span class="visually_hidden" role="status"><TranslateText key="FRONTAI_WORKSPACE_RESPONSE_READY"/></span>
					}.into_any());
				}
				return None;
			})}
		</button>
	}.into_any();
}

#[component]
fn AiWorkspaceTabs(selectedTab: RwSignal<AiWorkspaceTab>) -> impl IntoView
{
	let chatTab = move |_| selectedTab.set(AiWorkspaceTab::Chat);
	let configurationTab = move |_| selectedTab.set(AiWorkspaceTab::Configuration);
	let automationsTab = move |_| selectedTab.set(AiWorkspaceTab::Automations);
	let historyTab = move |_| selectedTab.set(AiWorkspaceTab::History);
	let tabKeyboard = move |event: KeyboardEvent| {
		let current = selectedTab.get_untracked();
		let next = match event.key().as_str()
		{
			"ArrowLeft" => Some(match current
			{
				AiWorkspaceTab::Chat => AiWorkspaceTab::History,
				AiWorkspaceTab::Configuration => AiWorkspaceTab::Chat,
				AiWorkspaceTab::Automations => AiWorkspaceTab::Configuration,
				AiWorkspaceTab::History => AiWorkspaceTab::Automations,
			}),
			"ArrowRight" => Some(match current
			{
				AiWorkspaceTab::Chat => AiWorkspaceTab::Configuration,
				AiWorkspaceTab::Configuration => AiWorkspaceTab::Automations,
				AiWorkspaceTab::Automations => AiWorkspaceTab::History,
				AiWorkspaceTab::History => AiWorkspaceTab::Chat,
			}),
			"Home" => Some(AiWorkspaceTab::Chat),
			"End" => Some(AiWorkspaceTab::History),
			_ => None,
		};
		if let Some(next) = next
		{
			event.prevent_default();
			selectedTab.set(next);
			workspaceTab_focus(next);
		}
	};

	return view! {
		<div class="ai_workspace_tabs" role="tablist" aria-labelledby="webhome-dialog-title" on:keydown=tabKeyboard>
			<button
				type="button"
				id="ai-workspace-tab-chat"
				role="tab"
				aria-controls="ai-workspace-panel-chat"
				aria-selected=move || (selectedTab.get() == AiWorkspaceTab::Chat).to_string()
				tabindex=move || if (selectedTab.get() == AiWorkspaceTab::Chat) {"0"} else {"-1"}
				class:selected=move || selectedTab.get() == AiWorkspaceTab::Chat
				on:click=chatTab
			><TranslateText key="FRONTAI_WORKSPACE_TAB_CHAT"/></button>
			<button
				type="button"
				id="ai-workspace-tab-configuration"
				role="tab"
				aria-controls="ai-workspace-panel-configuration"
				aria-selected=move || (selectedTab.get() == AiWorkspaceTab::Configuration).to_string()
				tabindex=move || if (selectedTab.get() == AiWorkspaceTab::Configuration) {"0"} else {"-1"}
				class:selected=move || selectedTab.get() == AiWorkspaceTab::Configuration
				on:click=configurationTab
			><TranslateText key="FRONTAI_WORKSPACE_TAB_CONFIGURATION"/></button>
			<button
				type="button"
				id="ai-workspace-tab-automations"
				role="tab"
				aria-controls="ai-workspace-panel-automations"
				aria-selected=move || (selectedTab.get() == AiWorkspaceTab::Automations).to_string()
				tabindex=move || if (selectedTab.get() == AiWorkspaceTab::Automations) {"0"} else {"-1"}
				class:selected=move || selectedTab.get() == AiWorkspaceTab::Automations
				on:click=automationsTab
			><TranslateText key="FRONTAI_WORKSPACE_TAB_AUTOMATIONS"/></button>
			<button
				type="button"
				id="ai-workspace-tab-history"
				role="tab"
				aria-controls="ai-workspace-panel-history"
				aria-selected=move || (selectedTab.get() == AiWorkspaceTab::History).to_string()
				tabindex=move || if (selectedTab.get() == AiWorkspaceTab::History) {"0"} else {"-1"}
				class:selected=move || selectedTab.get() == AiWorkspaceTab::History
				on:click=historyTab
			><TranslateText key="FRONTAI_WORKSPACE_TAB_HISTORY"/></button>
		</div>
	};
}

#[component]
fn AiWorkspace(
	document: leptos::prelude::ArcRwSignal<super::chat::ChatDocument>,
	runtime: leptos::prelude::ArcRwSignal<super::chat::ChatRuntime>,
	selectedTab: RwSignal<AiWorkspaceTab>,
	clientState: ClientState,
	aiDraft: AiOptionsDraft,
	aiTestState: RwSignal<AiTestState>,
	configurationSaving: RwSignal<bool>,
	configurationFeedback: RwSignal<Option<(&'static str,bool)>>,
	ollamaValidatedFingerprint: RwSignal<Option<String>>,
	history: Vec<AiAutomationHistoryEntry>,
	historyModules: Vec<AiModuleCapabilities>,
	lifecycleEpoch: ModuleHolderEpoch,
) -> impl IntoView
{
	let configurationState = clientState.clone();
	let automationState = clientState.clone();
	let configurationActionsState = clientState.clone();
	let automationActionsState = clientState.clone();
	let saveState = clientState.clone();
	let saveDraft = aiDraft.clone();
	let toaster = expect_toaster();
	let dialogManager = leptos::prelude::expect_context::<DialogManager>();
	let navigate = hooks::use_navigate();
	let configurationSave = Callback::new(move |_: ()| {
		if (configurationSaving.get_untracked()
			|| aiTestState.get_untracked().isBusy()
			|| saveState.passwordRotation_runningIsActive_untracked())
		{
			return;
		}
		let aiDocument = match saveDraft.document_get()
		{
			Ok(document) => document,
			Err(error) => {
				configurationFeedback.set(Some((error.translateKey_get(),true)));
				return;
			},
		};
		configurationSaving.set(true);
		configurationFeedback.set(None);
		let clientState = saveState.clone();
		let aiDraft = saveDraft.clone();
		let toaster = toaster.clone();
		let dialogManager = dialogManager.clone();
		let navigate = navigate.clone();
		ModuleHolder::task_spawn(lifecycleEpoch,async move {
			let result = ModuleHolder::aiConfig_userSave(lifecycleEpoch,aiDocument).await;
			configurationSaving.set(false);
			match result
			{
				Ok(()) => configurationFeedback.set(Some(("FRONTAI_WORKSPACE_CONFIGURATION_SAVED",false))),
				Err(error) => {
					if (error == AiConfigSaveError::OUTDATED)
					{
						aiDraft.replace(ModuleHolder::aiConfig_get());
						aiTestState.set(AiTestState::Idle);
					}
					configurationFeedback.set(Some((error.translateKey_get(),true)));
					if (error == AiConfigSaveError::AUTH_REQUIRED)
					{
						let storageClearFailed = clientState.local_clear().is_err();
						ModuleHolder::lifecycle_close();
						dialogManager.clear();
						toastingErr(&toaster,error).await;
						if (storageClearFailed)
						{
							toastingErr(&toaster,AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
						}
						navigate("/",Default::default());
					}
				},
			}
		});
	});

	return view! {
		<div class="ai_workspace">
			<section
				id="ai-workspace-panel-chat"
				class="ai_workspace_panel ai_workspace_panel--chat"
				role="tabpanel"
				aria-labelledby="ai-workspace-tab-chat"
				hidden=move || selectedTab.get() != AiWorkspaceTab::Chat
			>
				<AiChatView document=document.clone() runtime=runtime.clone() lifecycleEpoch/>
			</section>
			<section
				id="ai-workspace-panel-configuration"
				class="ai_workspace_panel ai_workspace_panel--configuration options_menu"
				role="tabpanel"
				aria-labelledby="ai-workspace-tab-configuration"
				hidden=move || selectedTab.get() != AiWorkspaceTab::Configuration
			>
				<AiConfiguration
					clientState=configurationState
					configurationSaving
					aiDraft=aiDraft.clone()
					aiTestState
					ollamaValidatedFingerprint
					lifecycleEpoch
				/>
				<AiWorkspaceSaveActions
					clientState=configurationActionsState
					configurationSaving
					aiTestState
					configurationFeedback
					onSave=configurationSave
				/>
			</section>
			<section
				id="ai-workspace-panel-automations"
				class="ai_workspace_panel ai_workspace_panel--automations"
				role="tabpanel"
				aria-labelledby="ai-workspace-tab-automations"
				hidden=move || selectedTab.get() != AiWorkspaceTab::Automations
			>
				<AiAutomationEditor
					aiDraft=aiDraft.clone()
					configurationSaving
					clientState=automationState
				/>
				<AiWorkspaceSaveActions
					clientState=automationActionsState
					configurationSaving
					aiTestState
					configurationFeedback
					onSave=configurationSave
				/>
			</section>
			<section
				id="ai-workspace-panel-history"
				class="ai_workspace_panel ai_workspace_panel--history"
				role="tabpanel"
				aria-labelledby="ai-workspace-tab-history"
				hidden=move || selectedTab.get() != AiWorkspaceTab::History
			>
				<AiAutomationHistoryView history modules=historyModules/>
			</section>
		</div>
	}.into_any();
}

#[component]
fn AiWorkspaceSaveActions(
	clientState: ClientState,
	configurationSaving: RwSignal<bool>,
	aiTestState: RwSignal<AiTestState>,
	configurationFeedback: RwSignal<Option<(&'static str,bool)>>,
	onSave: Callback<()>,
) -> impl IntoView
{
	return view! {
		<div class="ai_workspace_configuration_actions">
			{move || configurationFeedback.get().map(|(key,isError)| view! {
				<p class="options_ai_status" class:options_ai_status--error=isError class:options_ai_status--success=!isError role={if isError {"alert"} else {"status"}}>
					<TranslateText key=key/>
				</p>
			})}
			<button
				type="button"
				disabled=move || configurationSaving.get() || aiTestState.get().isBusy() || clientState.passwordRotation_runningIsActive()
				on:click=move |_| onSave.run(())
			>
				{move || if configurationSaving.get()
				{
					view!{<TranslateText key="FRONTAI_WORKSPACE_CONFIGURATION_SAVING"/>}.into_any()
				}
				else
				{
					view!{<TranslateText key="FRONTAI_WORKSPACE_CONFIGURATION_SAVE"/>}.into_any()
				}}
			</button>
		</div>
	};
}

#[cfg(feature="hydrate")]
fn workspaceTab_focus(tab: AiWorkspaceTab)
{
	leptos::leptos_dom::helpers::request_animation_frame(move || {
		let Some(element) = web_sys::window()
			.and_then(|window| window.document())
			.and_then(|document| document.get_element_by_id(tab.id_get()))
		else {return};
		if let Ok(element) = element.dyn_into::<web_sys::HtmlElement>()
		{
			let _ = element.focus();
		}
	});
}

#[cfg(not(feature="hydrate"))]
fn workspaceTab_focus(_: AiWorkspaceTab)
{
}
