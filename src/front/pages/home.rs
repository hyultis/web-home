use leptos::prelude::{For, GetUntracked, OnTargetAttribute, With};
use leptos::prelude::{CollectView, Get, PropAttribute};
use crate::front::modules::components::Backable;
use crate::front::components::options_menu::OptionsMenu;
use crate::front::ai::workspace::AiWorkspaceButton;
use crate::front::ai::inbox::AiInboxButton;
use crate::front::ai::AiAllowedOrigins;
use crate::front::modules::module_holder::{ModuleHolder, ModuleHolderEpoch};
use crate::front::utils::all_front_enum::{AllFrontErrorEnum, AllFrontLoginEnum, AllFrontUIEnum};
use crate::front::utils::dialog::{DialogActionStyle, DialogData, DialogManager};
use crate::front::utils::toaster_helpers::{toastingErr, toastingSuccess};
use crate::front::utils::translate::{Translate, TranslateText};
use crate::front::utils::users_data::{ClientCryptoContext, ClientState};
use crate::{HWebTrace};
use leptoaster::{expect_toaster, ToasterContext};
use leptos::ev::MouseEvent;
use leptos::prelude::ElementChild;
use leptos::prelude::{
	use_context, ArcRwSignal, ClassAttribute, Effect, IntoAny, OnAttribute,
	on_cleanup, AriaAttributes, GlobalAttributes, RenderHtml, RwSignal, Set, Update,
};
use leptos::{component, island, view, IntoView};
use leptos_router::{hooks, NavigateOptions};
use leptos::logging::log;
use leptos::task::spawn_local;
use leptos_use::use_interval_fn;
use strum::IntoEnumIterator;
use crate::api::modules::components::ModuleID;
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::modules::module_positions::ModulePositions;
use crate::front::modules::module_type::{ModuleType, ModuleTypeDiscriminants, StringToModuleType};
// https://iconoir.com/
// plus


#[island]
pub fn Home() -> impl IntoView
{
	let editMode = RwSignal::new(false);
	let moduleContent = ModuleHolder::getSingleton();
	let Some(dialogManager) = use_context::<DialogManager>() else {
		HWebTrace!("cannot get dialogManager in home");
		panic!("cannot get dialogManager in home");
	};
	let toaster = expect_toaster();
	let clientState = ClientState::expect();
	let lifecycleEpoch = ModuleHolder::lifecycle_open();
	let dialogManagerCleanup = dialogManager.clone();
	on_cleanup(move || {
		let lifecycleWasClosed = ModuleHolder::lifecycle_closeIf(lifecycleEpoch);
		if (lifecycleWasClosed || !ModuleHolder::lifecycle_isOpen())
		{
			dialogManagerCleanup.clear();
		}
	});

	// user data checker to force disconnect
	let toasterInner = toaster.clone();
	let clientStateConnection = clientState.clone();
	let dialogManagerConnection = dialogManager.clone();
	let disconnectRequested = RwSignal::new(false);
	Effect::new(move || {
		if ((!clientStateConnection.login_isConnected() || !clientStateConnection.crypto_isAvailable()) && !disconnectRequested.get_untracked())
		{
			disconnectRequested.set(true);
			let callback = user_disconnected(hooks::use_navigate(), toasterInner.clone(), clientStateConnection.clone(), dialogManagerConnection.clone(), false);
			callback(());
		}
	});

	Effect::new(move |_| {
		if (ModuleHolder::aiChat_migration_isNeeded())
		{
			ModuleHolder::aiChat_migration_start(lifecycleEpoch);
		}
	});

	// auto refresh cookie every 2 hour
	let clientStateRefresh = clientState.clone();
	let _ = use_interval_fn(
		move || {
			clientStateRefresh.refresh();
			log!("auto refresh cookie");
		},
		2 * 3600 * 1000,
	);

	// pre init ModuleHolder
	let aiAllowedOrigins = leptos::prelude::expect_context::<AiAllowedOrigins>();
	let moduleActions = ModuleActionFn::new(toaster.clone(),aiAllowedOrigins,lifecycleEpoch);
	let innerModuleActions = moduleActions.clone();
	moduleContent.update(|modules|{
		modules.moduleActions_set(lifecycleEpoch, innerModuleActions);
	});

	// initialise ModuleHolder
	let moduleContentInnerInitialLoad = moduleContent.clone();
	let toasterInnerInitialLoad = toaster.clone();
	let clientStateInitialLoad = clientState.clone();
	let is_initialized = RwSignal::new(false);
	Effect::new(move || {
		if (clientStateInitialLoad.passwordRotation_runningIsActive()
			|| clientStateInitialLoad.passwordRotation_pendingIsAvailable())
		{
			ModuleHolder::network_suspend();
			return;
		}
		ModuleHolder::network_resume();
		if(is_initialized.get_untracked()) {
			return;
		}
		is_initialized.set(true);

		ModuleHolder::task_spawn(
			lifecycleEpoch,
			ModuleHolder::network_deferredCall(moduleContentInnerInitialLoad.clone(), lifecycleEpoch, toasterInnerInitialLoad.clone(), |holder|ModuleHolder::network_modules_retrieve_caller(holder,true),None)
		);
	});

	let editModeValidateFn = editMode_validate(
		editMode.clone(),
		toaster.clone(),
		dialogManager.clone(),
		lifecycleEpoch,
	);

	let editModeCancelFn = editMode_cancel(
		editMode.clone(),
		toaster.clone(),
		dialogManager.clone(),
		lifecycleEpoch,
	);

	let editModeActivateFn = move |_| {

		HWebTrace!("editModeActivateFn");
		editMode.update(|content| {
			*content = true;
		});
	};

	let editModeAddModuleFn = editMode_AddBlock(dialogManager.clone(), lifecycleEpoch);

	// disconnect func
	let toasterInner = toaster.clone();
	let clientStateDisconnect = clientState.clone();
	let disconnectFn = move |_| {
		let dialogContent = DialogData::new()
			.setTitle(AllFrontLoginEnum::LOGIN_USER_WANT_DISCONNECTED)
			.setButtonValidateTitle(Some("FRONTUI_LOGOUT_ACTION"))
			.setValidateStyle(DialogActionStyle::Danger)
			.setOnValidate(user_disconnected(hooks::use_navigate(), toasterInner.clone(), clientStateDisconnect.clone(), dialogManager.clone(), true));

		dialogManager.open(dialogContent);
	};

	let moduleActionsInnerModuleView = moduleActions.clone();
	view! {
		<div class="home_body">
			<header class="header">
				<nav class="left" aria-labelledby="quick-links-title">
					<span id="quick-links-title" class="visually_hidden"><TranslateText key="FRONTUI_QUICK_LINKS"/></span>
					{move || {
						return ModuleHolder::getSingleton().with(|binding| {
							let tmp = binding.links_get();
							tmp.draw(editMode,moduleActionsInnerModuleView.clone(),tmp.id_get()).run()
						});
					}}
				</nav>
				<div class="right">
					{move || {
						let editModeValidateFn = editModeValidateFn.clone();
						let editModeCancelFn = editModeCancelFn.clone();
						let editModeActivateFn = editModeActivateFn.clone();
						let editModeAddModuleFn = editModeAddModuleFn.clone();
						if editMode.get()
						{
							view!{
								<div class="header_actions_group">
									<button type="button" class="icon_button" on:click=editModeAddModuleFn>
										<i class="iconoir-plus-circle" aria-hidden="true"></i>
										<span class="visually_hidden"><TranslateText key="FRONTUI_HOME_ADD_ACTION"/></span>
									</button>
									<button type="button" class="icon_button icon_button--success" on:click=editModeValidateFn>
										<i class="iconoir-check" aria-hidden="true"></i>
										<span class="visually_hidden"><TranslateText key="FRONTUI_HOME_SAVE_ACTION"/></span>
									</button>
									<button type="button" class="icon_button icon_button--danger" on:click=editModeCancelFn>
										<i class="iconoir-xmark" aria-hidden="true"></i>
										<span class="visually_hidden"><TranslateText key="FRONTUI_HOME_CANCEL_ACTION"/></span>
									</button>
								</div>
							}.into_any()
						}
						else
						{
							view!{
								<button type="button" class="icon_button" on:click=editModeActivateFn>
									<i class="iconoir-edit-pencil" aria-hidden="true"></i>
									<span class="visually_hidden"><TranslateText key="FRONTUI_HOME_EDIT_ACTION"/></span>
								</button>
							}.into_any()
						}
					}}
					<AiWorkspaceButton lifecycleEpoch/>
					<AiInboxButton lifecycleEpoch/>
					<OptionsMenu lifecycleEpoch/>
					<button type="button" class="icon_button icon_button--warning" on:click=disconnectFn>
						<i class="iconoir-key" aria-hidden="true"></i>
						<span class="visually_hidden"><TranslateText key="FRONTUI_LOGOUT_ACTION"/></span>
					</button>
				</div>
			</header>
			<main class="modules">
				<For
					each=move || ModuleHolder::getSingleton().with(|holder| holder.blocks_view())
					key=|(id,_)| id.clone()
					children=move |(moduleId, module)| {
						        view! {
						            <ModuleView
						                editMode=editMode.clone()
						                module=module.clone()
						                moduleActions=moduleActions.clone()
						                moduleId=moduleId.clone()
						            />
						        }
						    }
						/>
			</main>
		</div>
	}
}

#[component]
fn ModuleView(module: ArcRwSignal<ModulePositions<ModuleType>>, editMode: RwSignal<bool>, moduleActions: ModuleActionFn, moduleId: ModuleID) -> impl IntoView {
	let moduleActionsInnerModuleView = moduleActions.clone();
	return view! {
        {move || {
            module.with(|module| {
		        return module.draw(editMode,moduleActionsInnerModuleView.clone(),moduleId.clone());
	        })
        }}
    };
}

fn editMode_cancel(
	editModeInnerValidate: RwSignal<bool>,
	toasterInnerValidate: ToasterContext,
	dialogManager: DialogManager,
	lifecycleEpoch: ModuleHolderEpoch,
) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let editModeInnerValidate = editModeInnerValidate.clone();
		let toasterInnerValidate = toasterInnerValidate.clone();

		let dialogContent = DialogData::new()
			.setTitle(AllFrontUIEnum::HOME_CHANGE_CANCEL)
			.setButtonValidateTitle(Some("FRONTUI_HOME_CANCEL_ACTION"))
			.setValidateStyle(DialogActionStyle::Warning)
			.setOnValidate(move |_| {
				let editModeInnerValidate = editModeInnerValidate.clone();
				let toasterInnerValidate = toasterInnerValidate.clone();
				ModuleHolder::task_spawn(lifecycleEpoch, async move {
					ModuleHolder::network_deferredCall(ModuleHolder::getSingleton(), lifecycleEpoch, toasterInnerValidate.clone(), |holder|ModuleHolder::network_modules_retrieve_caller(holder,false), Some(AllFrontUIEnum::HOME_CHANGE_CANCEL)).await;
					if (ModuleHolder::lifecycle_isActive(lifecycleEpoch))
					{
						editModeInnerValidate.update(|content| {
							*content = false;
						});
					}
				});
				return true;
			});

		dialogManager.open(dialogContent);
	};
}

fn editMode_validate(
	editModeInnerValidate: RwSignal<bool>,
	toasterInnerValidate: ToasterContext,
	dialogManager: DialogManager,
	lifecycleEpoch: ModuleHolderEpoch,
) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let editModeInnerValidate = editModeInnerValidate.clone();
		let toasterInnerValidate = toasterInnerValidate.clone();

		let dialogContent = DialogData::new()
			.setTitle(AllFrontUIEnum::HOME_CHANGE_OK)
			.setButtonValidateTitle(Some("FRONTUI_HOME_SAVE_ACTION"))
			.setOnValidate(move |_| {
				let editModeInnerValidate = editModeInnerValidate.clone();
				let toasterInnerValidate = toasterInnerValidate.clone();
				ModuleHolder::task_spawn(lifecycleEpoch, async move {
					ModuleHolder::network_deferredCall(ModuleHolder::getSingleton(), lifecycleEpoch, toasterInnerValidate.clone(), |holder|ModuleHolder::network_modules_update_caller(holder), Some(AllFrontUIEnum::HOME_CHANGE_OK)).await;
					if (ModuleHolder::lifecycle_isActive(lifecycleEpoch))
					{
						editModeInnerValidate.update(|content| {
							*content = false;
						});
					}
				});
				return true;
			});

		dialogManager.open(dialogContent);
	};
}

fn editMode_AddBlock(dialogManager: DialogManager, lifecycleEpoch: ModuleHolderEpoch) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let selectedType = ArcRwSignal::new("".to_string());

		let selectedTypeInnerView = selectedType.clone();
		let dialogContent = DialogData::new()
			.setTitle(AllFrontUIEnum::HOME_CHANGE_NEW)
			.setBody(move || {
				let innerSelectedType = RwSignal::new("".to_string());

				let selectedTypeEffect = selectedTypeInnerView.clone();
				Effect::new(move |_| {
					selectedTypeEffect.clone().update(|e| *e = innerSelectedType.get());
				});

				view!{
					<div>
						<label>
							<span><Translate key="FRONTUI_MODULE_TYPE"/></span>
							<select on:change:target=move |ev| {
							      innerSelectedType.set(ev.target().value().parse().unwrap_or_default());
							    }
							    prop:value=move || innerSelectedType.get().to_string()>
								{move ||{
									ModuleTypeDiscriminants::iter().map(|moduleType| {
										let value = moduleType.to_string();
										view!{<option value={value}><TranslateText key={moduleType.translateKey_get()}/></option>}.into_any()
									}).collect_view()
								}}
							</select>
						</label>
					</div>
				}.into_any()
			})
			.setOnValidate(move |_| {
				let selectedType = selectedType.clone().get();

				ModuleHolder::getSingleton().update(|modules| {

					let Some(moduleType) = StringToModuleType(selectedType) else {return;};
					modules.blocks_insert(lifecycleEpoch, ModulePositions::new(moduleType));
				});

				return true;
			});

		dialogManager.open(dialogContent);
	}
}

fn user_disconnected(navigate: impl Fn(&str, NavigateOptions) + Clone + 'static, toaster: ToasterContext, clientState: ClientState, dialogManager: DialogManager, withToaster: bool) -> impl Fn(()) -> bool + Clone
{
	return move |_| {
		ModuleHolder::lifecycle_close();
		dialogManager.clear();
		let navigate = navigate.clone();
		let toaster = toaster.clone();
		let clientState = clientState.clone();
		// Logout owns only App-level contexts and must finish after the ModuleHolder Owner is closed.
		spawn_local(async move {
			let disconnectError = ClientCryptoContext::logout().await;
			let storageClearFailed = clientState.local_clear().is_err();

			if let Some(reason) = disconnectError
			{
				toastingErr(&toaster, reason).await;
				HWebTrace!("server session logout failed");
			}
			if (storageClearFailed)
			{
				toastingErr(&toaster, AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
			}
			else if(withToaster) {
				toastingSuccess(&toaster, AllFrontLoginEnum::LOGIN_USER_DISCONNECTED).await;
			}
			HWebTrace!("user disconnected");
			navigate("/", Default::default());
		});
		return true;
	};
}
