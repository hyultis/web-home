use leptos::prelude::{For, GetUntracked, OnTargetAttribute, With};
use leptos::prelude::{CollectView, Get, PropAttribute};
use crate::front::modules::components::Backable;
use crate::front::modules::module_holder::ModuleHolder;
use crate::front::utils::all_front_enum::{AllFrontErrorEnum, AllFrontLoginEnum, AllFrontUIEnum};
use crate::front::utils::dialog::{DialogData, DialogManager};
use crate::front::utils::toaster_helpers::{toastingErr, toastingSuccess};
use crate::front::utils::users_data::{ClientCryptoContext, ClientState};
use crate::{HWebTrace};
use leptoaster::{expect_toaster, ToasterContext};
use leptos::ev::MouseEvent;
use leptos::prelude::ElementChild;
use leptos::prelude::{
	use_context, ArcRwSignal, ClassAttribute, Effect, IntoAny, OnAttribute,
	RenderHtml, RwSignal, Set, Update,
};
use leptos::{component, island, view, IntoView};
use leptos_router::{hooks, NavigateOptions};
use leptos::logging::log;
use leptos::reactive::spawn_local_scoped;
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

	// user data checker to force disconnect
	let toasterInner = toaster.clone();
	let clientStateConnection = clientState.clone();
	Effect::new(move || {
		if (!clientStateConnection.login_isConnected() || !clientStateConnection.crypto_isAvailable())
		{
			let callback = user_disconnected(hooks::use_navigate(), toasterInner.clone(), clientStateConnection.clone(), false);
			callback(());
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
	let moduleActions = ModuleActionFn::new(toaster.clone());
	let innerModuleActions = moduleActions.clone();
	moduleContent.update(|modules|{
		modules.moduleActions_set(innerModuleActions);
	});

	// initialise ModuleHolder
	let moduleContentInnerInitialLoad = moduleContent.clone();
	let toasterInnerInitialLoad = toaster.clone();
	let is_initialized = RwSignal::new(false);
	Effect::new(move || {
		if(is_initialized.get_untracked()) {
			return;
		}
		is_initialized.set(true);

		spawn_local_scoped(ModuleHolder::network_deferredCall(moduleContentInnerInitialLoad.clone(), toasterInnerInitialLoad.clone(), |holder|ModuleHolder::network_modules_retrieve_caller(holder,true),None));
	});

	let editModeValidateFn = editMode_validate(
		editMode.clone(),
		toaster.clone(),
		dialogManager.clone(),
	);

	let editModeCancelFn = editMode_cancel(
		editMode.clone(),
		toaster.clone(),
		dialogManager.clone(),
	);

	let editModeActivateFn = move |_| {

		HWebTrace!("editModeActivateFn");
		editMode.update(|content| {
			*content = true;
		});
	};

	let editModeAddModuleFn = editMode_AddBlock(dialogManager.clone());

	// disconnect func
	let toasterInner = toaster.clone();
	let clientStateDisconnect = clientState.clone();
	let disconnectFn = move |_| {
		let dialogContent = DialogData::new()
			.setTitle(AllFrontLoginEnum::LOGIN_USER_WANT_DISCONNECTED)
			.setOnValidate(user_disconnected(hooks::use_navigate(), toasterInner.clone(), clientStateDisconnect.clone(), true));

		dialogManager.open(dialogContent);
	};

	let moduleActionsInnerModuleView = moduleActions.clone();
	view! {
		<div class="home_body">
			<div class="header">
				<div class="left">
					{move || {
						return ModuleHolder::getSingleton().with(|binding| {
							let tmp = binding.links_get();
							tmp.draw(editMode,moduleActionsInnerModuleView.clone(),tmp.id_get()).run()
						});
					}}
				</div>
				<div class="right">
					<i class="iconoir-key" on:click=disconnectFn></i>
					{move || {
						let editModeValidateFn = editModeValidateFn.clone();
						let editModeCancelFn = editModeCancelFn.clone();
						let editModeActivateFn = editModeActivateFn.clone();
						let editModeAddModuleFn = editModeAddModuleFn.clone();
						if editMode.get()
						{
							view!{
								<i class="iconoir-plus-circle" on:click=editModeAddModuleFn></i>
								<i class="iconoir-check button_ok" on:click=editModeValidateFn></i>
								<i class="iconoir-xmark button_danger" on:click=editModeCancelFn></i>
							}.into_any()
						}
						else
						{
							view!{<i class="iconoir-edit-pencil" on:click=editModeActivateFn></i>}.into_any()
						}
					}}
				</div>
				<hr style="clear: both;"/>
			</div>
			<div class="modules">
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
			</div>
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
) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let moduleContentInnerValidate = ModuleHolder::getSingleton();
		let editModeInnerValidate = editModeInnerValidate.clone();
		let toasterInnerValidate = toasterInnerValidate.clone();

		let dialogContent = DialogData::new()
			.setTitle(AllFrontUIEnum::HOME_CHANGE_CANCEL)
			.setOnValidate(move |_| {
				let editModeInnerValidate = editModeInnerValidate.clone();
				let toasterInnerValidate = toasterInnerValidate.clone();
				spawn_local(async move {
					ModuleHolder::network_deferredCall(ModuleHolder::getSingleton(), toasterInnerValidate.clone(), |holder|ModuleHolder::network_modules_retrieve_caller(holder,false), Some(AllFrontUIEnum::HOME_CHANGE_CANCEL)).await;
					editModeInnerValidate.update(|content| {
						*content = false;
					});
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
) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let moduleContentInnerValidate = ModuleHolder::getSingleton();
		let editModeInnerValidate = editModeInnerValidate.clone();
		let toasterInnerValidate = toasterInnerValidate.clone();

		let dialogContent = DialogData::new()
			.setTitle(AllFrontUIEnum::HOME_CHANGE_OK)
			.setOnValidate(move |_| {
				let editModeInnerValidate = editModeInnerValidate.clone();
				let toasterInnerValidate = toasterInnerValidate.clone();
				spawn_local(async move {
					ModuleHolder::network_deferredCall(ModuleHolder::getSingleton(), toasterInnerValidate.clone(), |holder|ModuleHolder::network_modules_update_caller(holder), Some(AllFrontUIEnum::HOME_CHANGE_OK)).await;
					editModeInnerValidate.update(|content| {
						*content = false;
					});
				});
				return true;
			});

		dialogManager.open(dialogContent);
	};
}

fn editMode_AddBlock(dialogManager: DialogManager) -> impl Fn(MouseEvent) + Clone
{
	return move |_| {
		let selectedType = ArcRwSignal::new("".to_string());

		let selectedTypeInnerView = selectedType.clone();
		let moduleContentInnerValidate = ModuleHolder::getSingleton();
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
							<span>Type</span>
							<select on:change:target=move |ev| {
							      innerSelectedType.set(ev.target().value().parse().unwrap_or_default());
							    }
							    prop:value=move || innerSelectedType.get().to_string()>
								{move ||{
									ModuleTypeDiscriminants::iter().map(|x| {
										view!{<option value={x.to_string()}>{x.to_string()}</option>}.into_any()
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
					modules.blocks_insert(ModulePositions::new(moduleType));
				});

				return true;
			});

		dialogManager.open(dialogContent);
	}
}

fn user_disconnected(navigate: impl Fn(&str, NavigateOptions) + Clone + 'static, toaster: ToasterContext, clientState: ClientState, withToaster: bool) -> impl Fn(()) -> bool + Clone
{
	return move |_| {
		let navigate = navigate.clone();
		let toaster = toaster.clone();
		let clientState = clientState.clone();
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
