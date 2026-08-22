use crate::front::ai::{
	AiAllowedOrigins,AiConfigDocument,AiConfigError,AiProfile,AiProvider,AI_OUTPUT_TOKENS_DEFAULT,
	AI_OUTPUT_TOKENS_MAXIMUM,AI_OUTPUT_TOKENS_MINIMUM,
};
use crate::front::ai::automation::AiAutomationContext;
use crate::front::ai::provider::{AiAvailableModel,AiCompletionRequest,AiModelInstallProgress,AiProviderClient};
use crate::global_security::hash;
use crate::front::modules::module_holder::{ModuleHolder,ModuleHolderEpoch};
use crate::front::utils::translate::TranslateText;
use crate::front::utils::users_data::ClientState;
use leptos::prelude::{
	ArcRwSignal,AriaAttributes,BindAttribute,ClassAttribute,CollectView,ElementChild,Get,GetUntracked,
	GlobalAttributes,IntoAny,OnAttribute,OnTargetAttribute,PropAttribute,RwSignal,Set,
};
use leptos::{component,view,IntoView};
use std::rc::Rc;

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub(crate) enum AiTestState
{
	Idle,
	OllamaServerRunning,
	ModelListRunning,
	ModelListSuccess,
	ModelInstallRunning,
	ModelInstallSuccess,
	Running,
	Success,
	Error(&'static str),
}

impl AiTestState
{
	pub(super) fn isBusy(self) -> bool
	{
		return matches!(self,Self::OllamaServerRunning | Self::ModelListRunning | Self::ModelInstallRunning | Self::Running);
	}
}

#[derive(Clone)]
pub(crate) struct AiOptionsDraft
{
	enabled: RwSignal<bool>,
	provider: RwSignal<AiProvider>,
	model: RwSignal<String>,
	credential: RwSignal<String>,
	baseUrl: RwSignal<String>,
	maxOutputTokens: RwSignal<String>,
	contexts: RwSignal<Vec<ArcRwSignal<AiAutomationContext>>>,
}

impl AiOptionsDraft
{
	pub(super) fn new(document: AiConfigDocument) -> Self
	{
		let enabled = document.profile.is_some();
		let profile = document.profile.unwrap_or_default();
		let contexts = document.contexts.into_iter().map(ArcRwSignal::new).collect();
		return Self {
			enabled: RwSignal::new(enabled),
			provider: RwSignal::new(profile.provider),
			model: RwSignal::new(profile.model),
			credential: RwSignal::new(profile.credential),
			baseUrl: RwSignal::new(profile.baseUrl),
			maxOutputTokens: RwSignal::new(profile.maxOutputTokens.to_string()),
			contexts: RwSignal::new(contexts),
		};
	}

	pub(super) fn document_get(&self) -> Result<AiConfigDocument,AiConfigError>
	{
		let mut document = AiConfigDocument::default();
		if (self.enabled.get_untracked())
		{
			document.profile = Some(self.profile_get()?);
		}
		document.contexts = self.contexts.get_untracked().into_iter()
			.map(|context| context.get_untracked())
			.collect();
		document.validate()?;
		return Ok(document);
	}

	pub(in crate::front::ai) fn contexts_signal(&self) -> RwSignal<Vec<ArcRwSignal<AiAutomationContext>>>
	{
		return self.contexts;
	}

	fn profile_get(&self) -> Result<AiProfile,AiConfigError>
	{
		let maxOutputTokens = self.maxOutputTokens.get_untracked().parse::<u32>()
			.map_err(|_| AiConfigError::InvalidOutputTokens)?;
		let profile = AiProfile {
			provider: self.provider.get_untracked(),
			model: self.model.get_untracked(),
			credential: self.credential.get_untracked(),
			baseUrl: self.baseUrl.get_untracked(),
			maxOutputTokens,
		};
		profile.validate()?;
		return Ok(profile);
	}

	fn connectionProfile_get(&self) -> AiProfile
	{
		return AiProfile {
			provider: self.provider.get_untracked(),
			model: String::new(),
			credential: self.credential.get_untracked(),
			baseUrl: self.baseUrl.get_untracked(),
			maxOutputTokens: AI_OUTPUT_TOKENS_DEFAULT,
		};
	}

	fn ollamaConnectionFingerprint_get(&self) -> Option<String>
	{
		return Self::ollamaConnectionFingerprint_create(
			self.provider.get_untracked(),
			self.baseUrl.get_untracked(),
			self.credential.get_untracked(),
		);
	}

	fn ollamaConnectionFingerprint_reactiveGet(&self) -> Option<String>
	{
		return Self::ollamaConnectionFingerprint_create(
			self.provider.get(),
			self.baseUrl.get(),
			self.credential.get(),
		);
	}

	fn ollamaConnectionFingerprint_create(provider: AiProvider,baseUrl: String,credential: String) -> Option<String>
	{
		if (provider != AiProvider::Ollama)
		{
			return None;
		}
		return Some(hash(format!("{baseUrl}\0{credential}")));
	}

	fn modelInstallProfile_get(&self) -> Result<AiProfile,AiConfigError>
	{
		let profile = AiProfile {
			provider: self.provider.get_untracked(),
			model: self.model.get_untracked(),
			credential: self.credential.get_untracked(),
			baseUrl: self.baseUrl.get_untracked(),
			maxOutputTokens: AI_OUTPUT_TOKENS_DEFAULT,
		};
		profile.validate()?;
		return Ok(profile);
	}

	pub(super) fn replace(&self, document: AiConfigDocument)
	{
		let enabled = document.profile.is_some();
		let profile = document.profile.unwrap_or_default();
		let contexts = document.contexts.into_iter().map(ArcRwSignal::new).collect();
		self.enabled.set(enabled);
		self.provider.set(profile.provider);
		self.model.set(profile.model);
		self.credential.set(profile.credential);
		self.baseUrl.set(profile.baseUrl);
		self.maxOutputTokens.set(profile.maxOutputTokens.to_string());
		self.contexts.set(contexts);
	}
}

#[component]
pub(in crate::front::ai) fn AiConfiguration(
	clientState: ClientState,
	configurationSaving: RwSignal<bool>,
	aiDraft: AiOptionsDraft,
	aiTestState: RwSignal<AiTestState>,
	ollamaValidatedFingerprint: RwSignal<Option<String>>,
	lifecycleEpoch: ModuleHolderEpoch,
) -> impl IntoView
{
	let allowedOrigins = leptos::prelude::expect_context::<AiAllowedOrigins>();
	let enabledDraft = aiDraft.clone();
	let providerDraft = aiDraft.clone();
	let profileDraft = aiDraft.clone();
	let modelListDraft = aiDraft.clone();
	let ollamaServerDraft = aiDraft.clone();
	let modelInstallDraft = aiDraft.clone();
	let testDraft = aiDraft.clone();
	let enabledState = clientState.clone();
	let profileState = clientState.clone();
	let modelListState = clientState.clone();
	let ollamaServerState = clientState.clone();
	let modelInstallState = clientState.clone();
	let testState = clientState.clone();
	let modelListActionState = modelListState.clone();
	let ollamaServerActionState = ollamaServerState.clone();
	let modelInstallActionState = modelInstallState.clone();
	let testActionState = testState.clone();
	let availableModels = RwSignal::new(Vec::<AiAvailableModel>::new());
	let modelInstallProgress = RwSignal::new(AiModelInstallProgress::Indeterminate);
	let ollamaServerAllowedOrigins = allowedOrigins.clone();
	let modelInstallAllowedOrigins = allowedOrigins.clone();

	let modelListLoad = move |_| {
		if (configurationSaving.get_untracked()
			|| aiTestState.get_untracked().isBusy()
			|| modelListActionState.passwordRotation_runningIsActive_untracked())
		{
			return;
		}
		let profile = modelListDraft.connectionProfile_get();
		availableModels.set(Vec::new());
		aiTestState.set(AiTestState::ModelListRunning);
		ModuleHolder::task_spawn(lifecycleEpoch,async move {
			let result = AiProviderClient::modelList_get(&profile).await;
			if (!ModuleHolder::lifecycle_isActive(lifecycleEpoch))
			{
				return;
			}
			match result
			{
				Ok(models) => {
					availableModels.set(models);
					aiTestState.set(AiTestState::ModelListSuccess);
				},
				Err(error) => aiTestState.set(AiTestState::Error(error.modelListTranslateKey_get())),
			}
		});
	};

	let ollamaServerTest = move |_| {
		if (configurationSaving.get_untracked()
			|| aiTestState.get_untracked().isBusy()
			|| ollamaServerActionState.passwordRotation_runningIsActive_untracked())
		{
			return;
		}
		let Some(fingerprint) = ollamaServerDraft.ollamaConnectionFingerprint_get() else
		{
			aiTestState.set(AiTestState::Error("FRONTAI_OLLAMA_SERVER_REQUEST_INVALID"));
			return;
		};
		let profile = ollamaServerDraft.connectionProfile_get();
		let allowedOrigins = ollamaServerAllowedOrigins.clone();
		ollamaValidatedFingerprint.set(None);
		aiTestState.set(AiTestState::OllamaServerRunning);
		ModuleHolder::task_spawn(lifecycleEpoch,async move {
			let result = AiProviderClient::ollamaServerTest(&profile,&allowedOrigins).await;
			if (!ModuleHolder::lifecycle_isActive(lifecycleEpoch))
			{
				return;
			}
			match result
			{
				Ok(()) => {
					ollamaValidatedFingerprint.set(Some(fingerprint));
					aiTestState.set(AiTestState::Idle);
				},
				Err(error) => aiTestState.set(AiTestState::Error(error.ollamaServerTranslateKey_get())),
			}
		});
	};

	let modelInstall = move |_| {
		if (configurationSaving.get_untracked()
			|| aiTestState.get_untracked().isBusy()
			|| modelInstallActionState.passwordRotation_runningIsActive_untracked())
		{
			return;
		}
		let profile = match modelInstallDraft.modelInstallProfile_get()
		{
			Ok(profile) => profile,
			Err(error) => {
				aiTestState.set(AiTestState::Error(error.translateKey_get()));
				return;
			},
		};
		let allowedOrigins = modelInstallAllowedOrigins.clone();
		modelInstallProgress.set(AiModelInstallProgress::Indeterminate);
		aiTestState.set(AiTestState::ModelInstallRunning);
		ModuleHolder::task_spawn(lifecycleEpoch,async move {
			let progress = Rc::new(move |progress| modelInstallProgress.set(progress));
			let result = AiProviderClient::modelInstall(&profile,&allowedOrigins,progress).await;
			if (!ModuleHolder::lifecycle_isActive(lifecycleEpoch))
			{
				return;
			}
			aiTestState.set(match result
			{
				Ok(()) => AiTestState::ModelInstallSuccess,
				Err(error) => AiTestState::Error(error.modelInstallTranslateKey_get()),
			});
		});
	};

	let connectionTest = move |_| {
		if (configurationSaving.get_untracked()
			|| aiTestState.get_untracked().isBusy()
			|| testActionState.passwordRotation_runningIsActive_untracked())
		{
			return;
		}
		let profile = match testDraft.profile_get()
		{
			Ok(profile) => profile,
			Err(error) => {
				aiTestState.set(AiTestState::Error(error.translateKey_get()));
				return;
			},
		};
		let allowedOrigins = allowedOrigins.clone();
		aiTestState.set(AiTestState::Running);
		ModuleHolder::task_spawn(lifecycleEpoch,async move {
			let request = AiCompletionRequest::connectionTest_get();
			let result = AiProviderClient::complete(&profile,&request,&allowedOrigins).await;
			if (!ModuleHolder::lifecycle_isActive(lifecycleEpoch))
			{
				return;
			}
			aiTestState.set(match result
			{
				Ok(_) => AiTestState::Success,
				Err(error) => AiTestState::Error(error.translateKey_get()),
			});
		});
	};

	return view! {
		<section class="options_section options_ai" aria-labelledby="ai-configuration-title">
			<h3 id="ai-configuration-title"><TranslateText key="FRONTUI_AI_TITLE"/></h3>
			<p class="options_help options_ai_intro"><TranslateText key="FRONTUI_AI_HELP"/></p>
			<label class="options_ai_toggle" for="ai-configuration-enabled">
				<input
					id="ai-configuration-enabled"
					type="checkbox"
					disabled=move || configurationSaving.get() || enabledState.passwordRotation_runningIsActive() || aiTestState.get().isBusy()
					prop:checked=move || enabledDraft.enabled.get()
					on:change:target=move |event| {
						enabledDraft.enabled.set(event.target().checked());
						aiTestState.set(AiTestState::Idle);
					}
				/>
				<span><TranslateText key="FRONTUI_AI_ENABLED"/></span>
			</label>
			{move || {
				let profileDraft = profileDraft.clone();
				if (!profileDraft.enabled.get())
				{
					return view! {
						<p class="options_ai_disabled"><TranslateText key="FRONTUI_AI_DISABLED"/></p>
					}.into_any();
				}
				let providerInputDraft = providerDraft.clone();
				let providerValue = providerDraft.provider;
				let providerUrlDraft = providerDraft.clone();
				let urlProfileDraft = profileDraft.clone();
				let ollamaServerProviderDraft = profileDraft.clone();
				let ollamaServerStatusDraft = profileDraft.clone();
				let modelAreaDraft = profileDraft.clone();
				let modelListProviderDraft = profileDraft.clone();
				let modelSelectionDraft = profileDraft.clone();
				let modelInputDraft = profileDraft.clone();
				let modelInstallProviderDraft = profileDraft.clone();
				let providerDisabledState = profileState.clone();
				let modelDisabledState = profileState.clone();
				let modelSelectionDisabledState = profileState.clone();
				let urlDisabledState = profileState.clone();
				let credentialDisabledState = profileState.clone();
				let tokensDisabledState = profileState.clone();
				let modelListDisabledState = modelListState.clone();
				let ollamaServerDisabledState = ollamaServerState.clone();
				let modelInstallDisabledState = modelInstallState.clone();
				let testDisabledState = testState.clone();
				let modelListLoad = modelListLoad.clone();
				let ollamaServerTest = ollamaServerTest.clone();
				let modelInstall = modelInstall.clone();
				let connectionTest = connectionTest.clone();
				view! {
					<div class="options_ai_fields">
						<div class="options_ai_connection_fields">
							<h4><TranslateText key="FRONTUI_AI_CONNECTION_SECTION"/></h4>
						<label class="options_field" for="ai-configuration-provider">
							<span><TranslateText key="FRONTUI_AI_PROVIDER"/></span>
							<select
								id="ai-configuration-provider"
								disabled=move || configurationSaving.get() || providerDisabledState.passwordRotation_runningIsActive() || aiTestState.get().isBusy()
								prop:value=move || providerValue.get().id_get()
								on:change:target=move |event| {
									let Some(provider) = AiProvider::fromId(&event.target().value()) else {return;};
									if (providerInputDraft.provider.get_untracked() != provider)
									{
										providerInputDraft.provider.set(provider);
										providerInputDraft.model.set(String::new());
										providerInputDraft.credential.set(String::new());
										providerInputDraft.baseUrl.set(String::new());
										providerInputDraft.maxOutputTokens.set(AI_OUTPUT_TOKENS_DEFAULT.to_string());
										availableModels.set(Vec::new());
										aiTestState.set(AiTestState::Idle);
									}
								}
							>
								<option value="openai"><TranslateText key="FRONTUI_AI_PROVIDER_OPENAI"/></option>
								<option value="anthropic"><TranslateText key="FRONTUI_AI_PROVIDER_ANTHROPIC"/></option>
								<option value="gemini"><TranslateText key="FRONTUI_AI_PROVIDER_GEMINI"/></option>
								<option value="mistral"><TranslateText key="FRONTUI_AI_PROVIDER_MISTRAL"/></option>
								<option value="ollama"><TranslateText key="FRONTUI_AI_PROVIDER_OLLAMA"/></option>
							</select>
						</label>
						{move || {
							let profileDraft = urlProfileDraft.clone();
							let urlDisabledState = urlDisabledState.clone();
							(providerUrlDraft.provider.get() == AiProvider::Ollama).then(move || view! {
								<label class="options_field" for="ai-configuration-url">
									<span><TranslateText key="FRONTUI_AI_URL"/></span>
									<input
										id="ai-configuration-url"
										type="url"
										name="ai-url"
										autocomplete="url"
										maxlength="4096"
										required
										disabled=move || configurationSaving.get() || urlDisabledState.passwordRotation_runningIsActive() || aiTestState.get().isBusy()
										on:input=move |_| aiTestState.set(AiTestState::Idle)
										bind:value=profileDraft.baseUrl
									/>
								</label>
								<p class="options_help"><TranslateText key="FRONTUI_AI_URL_HELP"/></p>
							})}}
						<label class="options_field" for="ai-configuration-credential">
							<span>{move || if profileDraft.provider.get() == AiProvider::Ollama {
								view! {<TranslateText key="FRONTUI_AI_CREDENTIAL_OPTIONAL"/>}.into_any()
							} else {
								view! {<TranslateText key="FRONTUI_AI_CREDENTIAL"/>}.into_any()
							}}</span>
							<input
								id="ai-configuration-credential"
								type="password"
								name="ai-credential"
								autocomplete="off"
								maxlength="16384"
								disabled=move || configurationSaving.get() || credentialDisabledState.passwordRotation_runningIsActive() || aiTestState.get().isBusy()
								on:input=move |_| {
									availableModels.set(Vec::new());
									aiTestState.set(AiTestState::Idle);
								}
								bind:value=profileDraft.credential
							/>
						</label>
						{move || {
							let ollamaServerDisabledState = ollamaServerDisabledState.clone();
							let ollamaServerTest = ollamaServerTest.clone();
							let ollamaServerStatusDraft = ollamaServerStatusDraft.clone();
							(ollamaServerProviderDraft.provider.get() == AiProvider::Ollama).then(move || view! {
								<div class="options_ai_server">
									<button
										type="button"
										disabled=move || configurationSaving.get() || ollamaServerDisabledState.passwordRotation_runningIsActive() || aiTestState.get().isBusy()
										on:click=ollamaServerTest.clone()
									>
										{move || if aiTestState.get() == AiTestState::OllamaServerRunning {
											view! {<TranslateText key="FRONTUI_AI_OLLAMA_SERVER_TESTING"/>}.into_any()
										} else {
											view! {<TranslateText key="FRONTUI_AI_OLLAMA_SERVER_TEST"/>}.into_any()
										}}
									</button>
									<p class="options_help"><TranslateText key="FRONTUI_AI_OLLAMA_SERVER_HELP"/></p>
									{move || {
										let currentFingerprint = ollamaServerStatusDraft.ollamaConnectionFingerprint_reactiveGet();
										(currentFingerprint.is_some() && currentFingerprint == ollamaValidatedFingerprint.get()).then(|| view! {
											<p class="options_ai_server_validated" role="status"><TranslateText key="FRONTUI_AI_OLLAMA_SERVER_SUCCESS"/></p>
										})
									}}
								</div>
								})
							}}
						</div>
						<div
							class="options_ai_model_fields"
							hidden=move || modelAreaDraft.provider.get() == AiProvider::Ollama
								&& modelAreaDraft.ollamaConnectionFingerprint_reactiveGet() != ollamaValidatedFingerprint.get()
						>
						<h4><TranslateText key="FRONTUI_AI_MODEL_SECTION"/></h4>
						{move || {
							let modelListDisabledState = modelListDisabledState.clone();
							let modelListLoad = modelListLoad.clone();
							(modelListProviderDraft.provider.get() == AiProvider::OpenAI).then(move || view! {
								<div class="options_ai_models">
									<button
										type="button"
										disabled=move || configurationSaving.get() || modelListDisabledState.passwordRotation_runningIsActive() || aiTestState.get().isBusy()
										on:click=modelListLoad.clone()
									>
										{move || if aiTestState.get() == AiTestState::ModelListRunning {
											view! {<TranslateText key="FRONTUI_AI_MODELS_LOADING"/>}.into_any()
										} else {
											view! {<TranslateText key="FRONTUI_AI_MODELS_LOAD"/>}.into_any()
										}}
									</button>
									<p class="options_help"><TranslateText key="FRONTUI_AI_MODELS_HELP"/></p>
								</div>
							})
						}}
						{move || {
							let models = availableModels.get();
							if (models.is_empty()) {return None;}
							let modelSelectionDraft = modelSelectionDraft.clone();
							let currentModel = modelSelectionDraft.model.get();
							let selectedModel = if models.iter().any(|model| model.id_get() == currentModel)
							{
								currentModel
							}
							else
							{
								String::new()
							};
							let modelSelectionDisabledState = modelSelectionDisabledState.clone();
							Some(view! {
								<label class="options_field" for="ai-configuration-model-selection">
									<span><TranslateText key="FRONTUI_AI_MODELS_AVAILABLE"/></span>
									<select
										id="ai-configuration-model-selection"
										disabled=move || configurationSaving.get() || modelSelectionDisabledState.passwordRotation_runningIsActive() || aiTestState.get().isBusy()
										prop:value=selectedModel
										on:change:target=move |event| {
											let model = event.target().value();
											if (!model.is_empty())
											{
												modelSelectionDraft.model.set(model);
												aiTestState.set(AiTestState::Idle);
											}
										}
									>
										<option value=""><TranslateText key="FRONTUI_AI_MODELS_SELECT"/></option>
										{models.into_iter().map(|model| {
											let id = model.id_get().to_string();
											let optionValue = id.clone();
											view! {<option value=optionValue>{id}</option>}
										}).collect_view()}
									</select>
								</label>
							})
						}}
						<label class="options_field" for="ai-configuration-model">
							<span><TranslateText key="FRONTUI_AI_MODEL"/></span>
							<input
								id="ai-configuration-model"
								type="text"
								name="ai-model"
								autocomplete="off"
								maxlength="256"
								required
								disabled=move || configurationSaving.get() || modelDisabledState.passwordRotation_runningIsActive() || aiTestState.get().isBusy()
								on:input=move |_| aiTestState.set(AiTestState::Idle)
								bind:value=modelInputDraft.model
							/>
						</label>
						{move || {
							let modelInstallDisabledState = modelInstallDisabledState.clone();
							let modelInstall = modelInstall.clone();
							(modelInstallProviderDraft.provider.get() == AiProvider::Ollama).then(move || view! {
								<div class="options_ai_models">
									<button
										type="button"
										disabled=move || configurationSaving.get() || modelInstallDisabledState.passwordRotation_runningIsActive() || aiTestState.get().isBusy()
										on:click=modelInstall.clone()
									>
										{move || if aiTestState.get() == AiTestState::ModelInstallRunning {
											view! {<TranslateText key="FRONTUI_AI_MODEL_INSTALLING"/>}.into_any()
										} else {
											view! {<TranslateText key="FRONTUI_AI_MODEL_INSTALL"/>}.into_any()
										}}
									</button>
									<p class="options_help"><TranslateText key="FRONTUI_AI_MODEL_INSTALL_HELP"/></p>
								</div>
							})
						}}
						<label class="options_field" for="ai-configuration-max-tokens">
							<span><TranslateText key="FRONTUI_AI_MAX_TOKENS"/></span>
							<input
								id="ai-configuration-max-tokens"
								type="number"
								name="ai-max-tokens"
								min=AI_OUTPUT_TOKENS_MINIMUM
								max=AI_OUTPUT_TOKENS_MAXIMUM
								required
								disabled=move || configurationSaving.get() || tokensDisabledState.passwordRotation_runningIsActive() || aiTestState.get().isBusy()
								on:input=move |_| aiTestState.set(AiTestState::Idle)
								bind:value=profileDraft.maxOutputTokens
							/>
						</label>
						<p class="options_ai_warning"><TranslateText key="FRONTUI_AI_BYOK_WARNING"/></p>
						<button
							type="button"
							class="options_ai_test"
							disabled=move || configurationSaving.get() || testDisabledState.passwordRotation_runningIsActive() || aiTestState.get().isBusy()
							on:click=connectionTest
						>
							{move || if aiTestState.get() == AiTestState::Running {
								view! {<TranslateText key="FRONTUI_AI_TESTING"/>}.into_any()
							} else {
								view! {<TranslateText key="FRONTUI_AI_TEST"/>}.into_any()
							}}
							</button>
						</div>
					</div>
				}.into_any()
			}}
			{move || match aiTestState.get()
			{
				AiTestState::Idle => None,
				AiTestState::OllamaServerRunning => Some(view! {
					<p class="options_ai_status" role="status"><TranslateText key="FRONTUI_AI_OLLAMA_SERVER_TESTING_STATUS"/></p>
				}.into_any()),
				AiTestState::ModelListRunning => Some(view! {
					<p class="options_ai_status" role="status"><TranslateText key="FRONTUI_AI_MODELS_LOADING_STATUS"/></p>
				}.into_any()),
				AiTestState::ModelListSuccess => Some(view! {
					<p class="options_ai_status options_ai_status--success" role="status"><TranslateText key="FRONTUI_AI_MODELS_SUCCESS"/></p>
				}.into_any()),
				AiTestState::ModelInstallRunning => Some(view! {
					<div class="options_ai_status" role="status">
						<p><TranslateText key="FRONTUI_AI_MODEL_INSTALLING_STATUS"/></p>
						<span id="ai-configuration-model-install-progress-label" class="visually_hidden"><TranslateText key="FRONTUI_AI_MODEL_INSTALL_PROGRESS_LABEL"/></span>
						{move || match modelInstallProgress.get()
						{
							AiModelInstallProgress::Indeterminate => view! {
								<div class="options_ai_progress">
									<progress max="100" aria-labelledby="ai-configuration-model-install-progress-label"></progress>
								</div>
							}.into_any(),
							AiModelInstallProgress::Determinate(percentage) => view! {
								<div class="options_ai_progress">
									<progress max="100" value=percentage aria-labelledby="ai-configuration-model-install-progress-label"></progress>
									<span aria-hidden="true">{format!("{percentage}%")}</span>
								</div>
							}.into_any(),
						}}
					</div>
				}.into_any()),
				AiTestState::ModelInstallSuccess => Some(view! {
					<p class="options_ai_status options_ai_status--success" role="status"><TranslateText key="FRONTUI_AI_MODEL_INSTALL_SUCCESS"/></p>
				}.into_any()),
				AiTestState::Running => Some(view! {
					<p class="options_ai_status" role="status"><TranslateText key="FRONTUI_AI_TESTING_STATUS"/></p>
				}.into_any()),
				AiTestState::Success => Some(view! {
					<p class="options_ai_status options_ai_status--success" role="status"><TranslateText key="FRONTUI_AI_TEST_SUCCESS"/></p>
				}.into_any()),
				AiTestState::Error(key) => Some(view! {
					<p class="options_ai_status options_ai_status--error" role="status"><TranslateText key=key/></p>
				}.into_any()),
			}}
		</section>
	};
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn ollamaValidationFingerprintTracksOriginAndCredentialWithoutRetainingThem()
	{
		let fingerprint = AiOptionsDraft::ollamaConnectionFingerprint_create(
			AiProvider::Ollama,
			"http://127.0.0.1:11434".to_string(),
			"private-credential".to_string(),
		).unwrap();

		assert!(!fingerprint.contains("127.0.0.1"));
		assert!(!fingerprint.contains("private-credential"));
		assert_ne!(
			Some(fingerprint.clone()),
			AiOptionsDraft::ollamaConnectionFingerprint_create(
				AiProvider::Ollama,
				"http://127.0.0.1:11435".to_string(),
				"private-credential".to_string(),
			),
		);
		assert_ne!(
			Some(fingerprint),
			AiOptionsDraft::ollamaConnectionFingerprint_create(
				AiProvider::Ollama,
				"http://127.0.0.1:11434".to_string(),
				"another-credential".to_string(),
			),
		);
		assert_eq!(
			AiOptionsDraft::ollamaConnectionFingerprint_create(
				AiProvider::OpenAI,
				String::new(),
				"private-credential".to_string(),
			),
			None,
		);
	}
}
