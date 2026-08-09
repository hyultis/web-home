use std::collections::HashMap;
use std::sync::Arc;
use leptoaster::{ToastBuilder, ToastLevel, ToasterContext};
use crate::api::IsToastable;
use crate::front::utils::all_front_enum::AllFrontErrorEnum;
use crate::front::utils::fluent::FluentManager::FluentManager;
use crate::front::utils::users_data::ClientState;
use crate::front::modules::module_holder::ModuleHolder;

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
	let lang = ClientState::expect().lang_get_untracked();
	toaster.toast(ToastBuilder::new(FluentManager::singleton().translateParamsLess(lang, keyTranslate.to_string()).await)
		.with_expiry(Some(5_000))
		.with_level(level));
}

pub async fn toastingParams(toaster: ToasterContext,keyTranslate: impl ToString, level: ToastLevel, params: Arc<HashMap<String,String>>)
{
	let lang = ClientState::expect().lang_get_untracked();
	toaster.toast(ToastBuilder::new(FluentManager::singleton().translate(lang, keyTranslate.to_string(),params).await)
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
