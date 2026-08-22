use leptoaster::expect_toaster;
use leptos::ev::{KeyboardEvent, MouseEvent, SubmitEvent};
use leptos::prelude::{AriaAttributes, BindAttribute, ClassAttribute, ElementChild, Get, GetUntracked, GlobalAttributes, IntoAny, OnAttribute, PropAttribute, RwSignal, Set, StyleAttribute};
use leptos::task::spawn_local;
use leptos::{component, view, IntoView};
use leptos_router::hooks;
#[cfg(feature="hydrate")]
use wasm_bindgen::JsCast;

use crate::api::login::components::{AccountPreferencesError, PasswordRotationError};
use crate::front::modules::module_holder::{ModuleHolder,ModuleHolderEpoch};
use crate::front::utils::all_front_enum::{AllFrontErrorEnum, AllFrontLoginEnum};
use crate::front::utils::dialog::{DialogData, DialogManager};
use crate::front::utils::toaster_helpers::{toastingErr, toastingSuccess};
use crate::front::utils::translate::TranslateText;
use crate::front::utils::users_data::{ClientCryptoContext, ClientState};

#[component]
pub(crate) fn OptionsMenu(lifecycleEpoch: ModuleHolderEpoch) -> impl IntoView
{
	let clientState = ClientState::expect();
	let dialogManager = leptos::prelude::expect_context::<DialogManager>();

	let openFn = move |_| {
		clientState.preferencesPreview_begin();
		let preferencesSaving = RwSignal::new(false);
		let passwordRotationSucceeded = RwSignal::new(false);

		let bodyState = clientState.clone();
		let validateState = clientState.clone();
		let closeState = clientState.clone();
		let closeGuardState = clientState.clone();
		let validateToaster = expect_toaster();
		let validateDialog = dialogManager.clone();
		let validateNavigate = hooks::use_navigate();
		let dialogContent = DialogData::new()
			.setTitle("FRONTUI_OPTIONS_TITLE")
			.setBody(move || view! {
				<OptionsPreferences
					clientState=bodyState.clone()
					preferencesSaving
					passwordRotationSucceeded
				/>
			}.into_any())
			.setButtonValidateTitle(Some("FRONTUI_OPTIONS_SAVE"))
			.setButtonCloseTitle(Some("FRONTUI_OPTIONS_CANCEL"))
			.setOnValidate(move |_| {
				if (preferencesSaving.get_untracked() || !validateState.passwordRotation_canClose())
				{
					return false;
				}
				preferencesSaving.set(true);
				let clientState = validateState.clone();
				let toaster = validateToaster.clone();
				let dialogManager = validateDialog.clone();
				let navigate = validateNavigate.clone();
				ModuleHolder::task_spawn(lifecycleEpoch,async move {
					let result = clientState.preferencesPreview_commit().await;
					preferencesSaving.set(false);
					match result
					{
						Ok(()) => {
							dialogManager.clear();
							toastingSuccess(&toaster,"FRONTUI_OPTIONS_PREFERENCES_SUCCESS").await;
						},
						Err(AccountPreferencesError::AUTH_REQUIRED) => {
							let storageClearFailed = clientState.local_clear().is_err();
							ModuleHolder::lifecycle_close();
							dialogManager.clear();
							toastingErr(&toaster,AccountPreferencesError::AUTH_REQUIRED).await;
							if (storageClearFailed)
							{
								toastingErr(&toaster,AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
							}
							navigate("/",Default::default());
						},
						Err(error) => toastingErr(&toaster,error).await,
					}
				});
				return false;
			})
			.setOnClose(move |_| closeState.preferencesPreview_cancel())
			.setCanClose(move || !preferencesSaving.get() && closeGuardState.passwordRotation_canClose());

		dialogManager.open(dialogContent);
	};

	return view! {
		<button type="button" class="icon_button" on:click=openFn>
			<i class="iconoir-settings" aria-hidden="true"></i>
			<span class="visually_hidden"><TranslateText key="FRONTUI_OPTIONS_ACTION"/></span>
		</button>
	};
}

#[component]
fn OptionsPreferences(
	clientState: ClientState,
	preferencesSaving: RwSignal<bool>,
	passwordRotationSucceeded: RwSignal<bool>,
) -> impl IntoView
{
	let languageState = clientState.clone();
	let languageInputState = clientState.clone();
	let hueState = clientState.clone();
	let hueTextState = clientState.clone();
	let hueOutputState = clientState.clone();
	let hueStyleState = clientState.clone();
	let hueMouseState = clientState.clone();
	let hueKeyboardState = clientState.clone();
	let passwordState = clientState.clone();

	return view! {
		<div class="options_menu">
			<section class="options_section" aria-labelledby="options-preferences-title">
				<h3 id="options-preferences-title"><TranslateText key="FRONTUI_OPTIONS_PREFERENCES"/></h3>
				{move || preferencesSaving.get().then(|| view! {
					<p class="options_rotation_status" role="status">
						<TranslateText key="FRONTUI_OPTIONS_PREFERENCES_SAVING"/>
					</p>
				})}
				<label class="options_field" for="options-language">
					<span><TranslateText key="FRONTUI_OPTIONS_LANGUAGE"/></span>
					<select
						id="options-language"
						disabled=move || preferencesSaving.get()
						prop:value=move || languageState.lang_get()
						on:change=move |event| {
							let _ = languageInputState.preferencesPreview_langSet(&leptos::prelude::event_target_value(&event));
						}
					>
						<option value="EN"><TranslateText key="FRONTUI_OPTIONS_LANGUAGE_EN"/></option>
						<option value="FR"><TranslateText key="FRONTUI_OPTIONS_LANGUAGE_FR"/></option>
					</select>
				</label>
				<div class="options_field">
					<span id="options-primary-hue-label"><TranslateText key="FRONTUI_OPTIONS_PRIMARY_HUE"/></span>
					<div class="options_hue_control">
						<button
							id="options-primary-hue"
							type="button"
							class="options_hue_wheel"
							role="slider"
							aria-labelledby="options-primary-hue-label"
							aria-describedby="options-primary-hue-help"
							aria-valuemin="0"
							aria-valuemax="359"
							aria-valuenow=move || hueState.primaryHue_get().to_string()
							aria-valuetext=move || format!("{}°",hueTextState.primaryHue_get())
							disabled=move || preferencesSaving.get()
							style=move || format!("--options-selected-hue: {};",hueStyleState.primaryHue_get())
							on:click=move |event: MouseEvent| {
								if (event.detail() == 0)
								{
									return;
								}
								if let Some(value) = hueFromPointer(event.current_target(),event.client_x(),event.client_y())
								{
									let _ = hueMouseState.preferencesPreview_primaryHueSet(value);
								}
							}
							on:keydown=move |event: KeyboardEvent| {
								let current = hueKeyboardState.primaryHue_get();
								let next = match event.key().as_str()
								{
									"ArrowLeft" | "ArrowDown" => Some(if current == 0 {359} else {current - 1}),
									"ArrowRight" | "ArrowUp" => Some(if current == 359 {0} else {current + 1}),
									"Home" => Some(0),
									"End" => Some(359),
									_ => None,
								};
								if let Some(next) = next
								{
									event.prevent_default();
									let _ = hueKeyboardState.preferencesPreview_primaryHueSet(next);
								}
							}
						>
							<span class="options_hue_wheel_center" aria-hidden="true"></span>
							<span class="options_hue_marker" aria-hidden="true"></span>
						</button>
						<output aria-live="polite">{move || format!("{}°", hueOutputState.primaryHue_get())}</output>
					</div>
				</div>
				<p id="options-primary-hue-help" class="options_help"><TranslateText key="FRONTUI_OPTIONS_PRIMARY_HUE_HELP"/></p>
			</section>
			<OptionsPasswordRotation
				clientState=passwordState
				preferencesSaving
				passwordRotationSucceeded
			/>
		</div>
	};
}

#[component]
fn OptionsPasswordRotation(
	clientState: ClientState,
	preferencesSaving: RwSignal<bool>,
	passwordRotationSucceeded: RwSignal<bool>,
) -> impl IntoView
{
	let currentPassword = RwSignal::new(String::new());
	let newPassword = RwSignal::new(String::new());
	let confirmation = RwSignal::new(String::new());
	let toaster = expect_toaster();
	let dialogManager = leptos::prelude::expect_context::<DialogManager>();
	let navigate = hooks::use_navigate();
	let submitState = clientState.clone();
	let pendingLogoutState = clientState.clone();
	let pendingLogoutToaster = toaster.clone();
	let pendingLogoutDialog = dialogManager.clone();
	let pendingLogoutNavigate = navigate.clone();

	let submit = move |event: SubmitEvent| {
		event.prevent_default();
		if (submitState.passwordRotation_runningIsActive_untracked()
			|| preferencesSaving.get_untracked()
			|| passwordRotationSucceeded.get_untracked())
		{
			return;
		}

		let resumePending = submitState.passwordRotation_pendingIsAvailable_untracked();
		let currentValue = currentPassword.get_untracked();
		let newValue = newPassword.get_untracked();
		let confirmationValue = confirmation.get_untracked();
		currentPassword.set(String::new());
		newPassword.set(String::new());
		confirmation.set(String::new());

		ModuleHolder::network_suspend();
		submitState.passwordRotation_runningSet(true);
		let clientState = submitState.clone();
		let toaster = toaster.clone();
		let dialogManager = dialogManager.clone();
		let navigate = navigate.clone();
		spawn_local(async move {
			let result = if (resumePending)
			{
				clientState.passwordRotation_resume().await.map(|_| ())
			}
			else
			{
				clientState.passwordRotation_change(currentValue,newValue,confirmationValue).await
			};
			clientState.passwordRotation_runningSet(false);

			match result
			{
				Ok(()) => {
					ModuleHolder::network_resume();
					passwordRotationSucceeded.set(true);
					toastingSuccess(&toaster,"FRONTUI_OPTIONS_PASSWORD_SUCCESS").await;
				},
				Err(PasswordRotationError::AUTH_REQUIRED) => {
					let storageClearFailed = clientState.local_clear().is_err();
					ModuleHolder::lifecycle_close();
					dialogManager.clear();
					toastingErr(&toaster,PasswordRotationError::AUTH_REQUIRED).await;
					if (storageClearFailed)
					{
						toastingErr(&toaster,AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
					}
					navigate("/",Default::default());
				},
				Err(error) => {
					if (!clientState.passwordRotation_pendingIsAvailable_untracked())
					{
						ModuleHolder::network_resume();
					}
					toastingErr(&toaster,error).await;
				},
			}
		});
	};
	let pendingLogout = move |_| {
		if (pendingLogoutState.passwordRotation_runningIsActive_untracked())
		{
			return;
		}
		pendingLogoutState.passwordRotation_runningSet(true);
		let clientState = pendingLogoutState.clone();
		let toaster = pendingLogoutToaster.clone();
		let dialogManager = pendingLogoutDialog.clone();
		let navigate = pendingLogoutNavigate.clone();
		spawn_local(async move {
			let disconnectError = ClientCryptoContext::logout().await;
			let storageClearFailed = clientState.local_clear().is_err();
			clientState.passwordRotation_runningSet(false);
			ModuleHolder::lifecycle_close();
			dialogManager.clear();
			if let Some(error) = disconnectError
			{
				toastingErr(&toaster,error).await;
			}
			if (storageClearFailed)
			{
				toastingErr(&toaster,AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
			}
			else
			{
				toastingSuccess(&toaster,AllFrontLoginEnum::LOGIN_USER_DISCONNECTED).await;
			}
			navigate("/",Default::default());
		});
	};

	let pendingState = clientState.clone();
	let workingState = clientState.clone();
	let buttonState = clientState.clone();
	let buttonLabelState = clientState.clone();
	let pendingLogoutStateView = clientState.clone();
	let currentInputState = clientState.clone();
	let newInputState = clientState.clone();
	let confirmationInputState = clientState.clone();
	let pendingLogoutButtonState = clientState.clone();

	return view! {
		<section class="options_section options_security" aria-labelledby="options-security-title">
			<h3 id="options-security-title"><TranslateText key="FRONTUI_OPTIONS_SECURITY"/></h3>
			{move || passwordRotationSucceeded.get().then(|| view! {
				<p class="options_rotation_status options_rotation_status--success" role="status">
					<TranslateText key="FRONTUI_OPTIONS_PASSWORD_SUCCESS_LOCKED"/>
				</p>
			})}
			<form
				class="options_password_form"
				hidden=move || passwordRotationSucceeded.get()
				on:submit=submit
			>
				{move || {
					let currentInputState = currentInputState.clone();
					let newInputState = newInputState.clone();
					let confirmationInputState = confirmationInputState.clone();
					if (pendingState.passwordRotation_pendingIsAvailable())
					{
						view! {
							<p class="options_rotation_status options_rotation_status--pending">
								<TranslateText key="FRONTUI_OPTIONS_PASSWORD_PENDING"/>
							</p>
						}.into_any()
					}
					else
					{
						view! {
							<label class="options_field" for="options-current-password">
								<span><TranslateText key="FRONTUI_OPTIONS_PASSWORD_CURRENT"/></span>
								<input
									id="options-current-password"
									type="password"
									name="current-password"
									autocomplete="current-password"
									required
									disabled=move || currentInputState.passwordRotation_runningIsActive() || preferencesSaving.get() || passwordRotationSucceeded.get()
									bind:value=currentPassword
								/>
							</label>
							<label class="options_field" for="options-new-password">
								<span><TranslateText key="FRONTUI_OPTIONS_PASSWORD_NEW"/></span>
								<input
									id="options-new-password"
									type="password"
									name="new-password"
									autocomplete="new-password"
									minlength="12"
									required
									disabled=move || newInputState.passwordRotation_runningIsActive() || preferencesSaving.get() || passwordRotationSucceeded.get()
									bind:value=newPassword
								/>
							</label>
							<label class="options_field" for="options-confirm-password">
								<span><TranslateText key="FRONTUI_OPTIONS_PASSWORD_CONFIRM"/></span>
								<input
									id="options-confirm-password"
									type="password"
									name="confirm-password"
									autocomplete="new-password"
									minlength="12"
									required
									disabled=move || confirmationInputState.passwordRotation_runningIsActive() || preferencesSaving.get() || passwordRotationSucceeded.get()
									bind:value=confirmation
								/>
							</label>
							<p class="options_help"><TranslateText key="FRONTUI_OPTIONS_PASSWORD_HELP"/></p>
						}.into_any()
					}
				}}
				{move || workingState.passwordRotation_runningIsActive().then(|| view! {
					<p class="options_rotation_status" role="status"><TranslateText key="FRONTUI_OPTIONS_PASSWORD_WORKING"/></p>
				})}
				<button
					type="submit"
					class="options_password_submit"
					disabled=move || buttonState.passwordRotation_runningIsActive() || preferencesSaving.get() || passwordRotationSucceeded.get()
				>
					{move || {
						if (buttonLabelState.passwordRotation_pendingIsAvailable())
						{
							view! { <TranslateText key="FRONTUI_OPTIONS_PASSWORD_RETRY"/> }.into_any()
						}
						else
						{
							view! { <TranslateText key="FRONTUI_OPTIONS_PASSWORD_CHANGE"/> }.into_any()
						}
					}}
				</button>
				{move || {
					let pendingLogoutButtonState = pendingLogoutButtonState.clone();
					pendingLogoutStateView.passwordRotation_pendingIsAvailable().then(|| view! {
						<button
							type="button"
							class="options_password_logout"
							disabled=move || pendingLogoutButtonState.passwordRotation_runningIsActive() || passwordRotationSucceeded.get()
							on:click=pendingLogout.clone()
						>
							<TranslateText key="FRONTUI_OPTIONS_PASSWORD_LOGOUT"/>
						</button>
					})
				}}
			</form>
		</section>
	};
}

#[cfg(feature="hydrate")]
fn hueFromPointer(target: Option<web_sys::EventTarget>, clientX: i32, clientY: i32) -> Option<u16>
{
	let element = target?.dyn_into::<web_sys::Element>().ok()?;
	let rect = element.get_bounding_client_rect();
	let x = f64::from(clientX) - rect.left() - rect.width() / 2.0;
	let y = f64::from(clientY) - rect.top() - rect.height() / 2.0;
	let radius = x.hypot(y);
	let maximumRadius = rect.width().min(rect.height()) / 2.0;
	if (!x.is_finite()
		|| !y.is_finite()
		|| !maximumRadius.is_finite()
		|| maximumRadius <= 0.0
		|| radius < maximumRadius * (2.0 / 3.0)
		|| radius > maximumRadius)
	{
		return None;
	}
	let angle = (y.atan2(x).to_degrees() + 90.0).rem_euclid(360.0);
	return hueFromAngle(angle);
}

#[cfg(not(feature="hydrate"))]
fn hueFromPointer(_: Option<web_sys::EventTarget>, _: i32, _: i32) -> Option<u16>
{
	return None;
}

#[cfg(any(feature="hydrate",test))]
fn hueFromAngle(angle: f64) -> Option<u16>
{
	if (!angle.is_finite())
	{
		return None;
	}
	return Some((angle.rem_euclid(360.0).round() as u16) % 360);
}

#[cfg(test)]
mod tests
{
	use super::hueFromAngle;

	#[test]
	fn hueWheel_normalizesAngles()
	{
		assert_eq!(hueFromAngle(0.0),Some(0));
		assert_eq!(hueFromAngle(90.0),Some(90));
		assert_eq!(hueFromAngle(359.4),Some(359));
		assert_eq!(hueFromAngle(359.6),Some(0));
		assert_eq!(hueFromAngle(-90.0),Some(270));
		assert_eq!(hueFromAngle(f64::NAN),None);
	}
}
