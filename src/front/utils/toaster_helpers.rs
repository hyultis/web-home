use std::collections::HashMap;
use std::sync::Arc;
use leptoaster::{ToastBuilder, ToastLevel, ToasterContext};
use leptos::prelude::{expect_context, provide_context, Owner, WeakOwner};
use crate::api::IsToastable;
use crate::front::utils::all_front_enum::AllFrontErrorEnum;
use crate::front::utils::fluent::FluentManager::FluentManager;
use crate::front::utils::users_data::ClientState;
use crate::front::modules::module_holder::ModuleHolder;

#[derive(Clone)]
struct ToasterOwner(WeakOwner);

pub(crate) fn toasterOwner_provide()
{
	let owner = Owner::current().expect("ToasterOwner requires a reactive owner");
	provide_context(ToasterOwner(owner.downgrade()));
}

fn toastingPush(toaster: &ToasterContext, toasterOwner: ToasterOwner, toast: ToastBuilder)
{
	// Leptoaster allocates its expiry signal while toast() runs. Re-enter the
	// application owner so a route transition cannot dispose that signal.
	if let Some(owner) = toasterOwner.0.upgrade()
	{
		owner.with(|| toaster.toast(toast));
	}
}

pub async fn toastingSuccess(toaster: &ToasterContext,keyTranslate: impl ToString)
{
	toasting(toaster,keyTranslate,ToastLevel::Success).await;
}

pub async fn toastingErr(toaster: &ToasterContext,keyTranslate: impl ToString)
{
	toasting(toaster,keyTranslate,ToastLevel::Error).await;
}

pub async fn toastingInfo(toaster: &ToasterContext,keyTranslate: impl ToString)
{
	toasting(toaster,keyTranslate,ToastLevel::Info).await;
}

pub async fn toastingWarn(toaster: &ToasterContext,keyTranslate: impl ToString)
{
	toasting(toaster,keyTranslate,ToastLevel::Warn).await;
}

pub async fn toasting(toaster: &ToasterContext,keyTranslate: impl ToString, level: ToastLevel)
{
	let toasterOwner = expect_context::<ToasterOwner>();
	let lang = ClientState::expect().lang_get_untracked();
	toastingPush(toaster, toasterOwner, ToastBuilder::new(FluentManager::singleton().translateParamsLess(lang, keyTranslate.to_string()).await)
		.with_expiry(Some(5_000))
		.with_level(level));
}

pub async fn toastingParams(toaster: ToasterContext,keyTranslate: impl ToString, level: ToastLevel, params: Arc<HashMap<String,String>>)
{
	let toasterOwner = expect_context::<ToasterOwner>();
	let lang = ClientState::expect().lang_get_untracked();
	toastingPush(&toaster, toasterOwner, ToastBuilder::new(FluentManager::singleton().translate(lang, keyTranslate.to_string(),params).await)
		.with_expiry(Some(5_000))
		.with_level(level));
}

pub async fn toaster_api<T>(toaster: &ToasterContext, apiFn: Result<T,impl IsToastable>, success: Option<&str>) -> Option<T>
{
	match apiFn
	{
		Ok(result) => {
			if let Some(success) = success { toastingSuccess(toaster, success).await; }
			return Some(result);
		},
		Err(err) => {
			let authenticationRequired = err.authenticationRequired_get();
			if let Some(level) = err.level()
			{
				toasting(toaster, err.to_string(), level).await
			}
			if (authenticationRequired)
			{
				if (ClientState::expect().local_clear().is_err())
				{
					toastingErr(toaster, AllFrontErrorEnum::CRYPTO_STORAGE_FAILED).await;
				}
				ModuleHolder::lifecycle_close();
			}
		},
	};
	return None;
}

#[cfg(test)]
mod tests
{
	use super::*;
	use leptos::prelude::{GetUntracked, Set};

	#[test]
	fn toastingPush_keepsSignalAliveAfterRouteDisposal()
	{
		let applicationOwner = Owner::new();
		applicationOwner.with(|| {
			toasterOwner_provide();
			let toaster = ToasterContext::default();
			let routeOwner = Owner::new();

			let clearSignal = routeOwner.with(|| {
				toastingPush(
					&toaster,
					expect_context::<ToasterOwner>(),
					ToastBuilder::new("connected").with_expiry(Some(5_000)),
				);
				toaster.queue.get_untracked()[0].clear_signal
			});

			routeOwner.cleanup();
			clearSignal.set(true);
			assert!(clearSignal.get_untracked());
		});
	}
}
