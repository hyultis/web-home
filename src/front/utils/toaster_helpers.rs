use std::collections::HashMap;
use std::sync::Arc;
use leptoaster::{ToastBuilder, ToastLevel, ToasterContext};
use leptos::prelude::Read;
use crate::front::utils::fluent::FluentManager::FluentManager;
use crate::front::utils::users_data::UserData;

pub async fn toastingSuccess(toaster: ToasterContext,keyTranslate: impl ToString)
{
	toasting(toaster,keyTranslate,ToastLevel::Success).await;
}

pub async fn toastingErr(toaster: ToasterContext,keyTranslate: impl ToString)
{
	toasting(toaster,keyTranslate,ToastLevel::Error).await;
}

pub async fn toastingInfo(toaster: ToasterContext,keyTranslate: impl ToString)
{
	toasting(toaster,keyTranslate,ToastLevel::Info).await;
}

pub async fn toastingWarn(toaster: ToasterContext,keyTranslate: impl ToString)
{
	toasting(toaster,keyTranslate,ToastLevel::Warn).await;
}

pub async fn toasting(toaster: ToasterContext,keyTranslate: impl ToString, level: ToastLevel)
{
	let (userData, setUserData) = UserData::cookie_signalGet();
	let userData = userData.read().clone().unwrap_or(UserData::new(&"EN".to_string()));

	toaster.toast(ToastBuilder::new(FluentManager::singleton().translateParamsLess(userData.lang_get(), keyTranslate.to_string()).await)
		.with_expiry(Some(5_000))
		.with_level(level));
}

pub async fn toastingParams(toaster: ToasterContext,keyTranslate: impl ToString, level: ToastLevel, params: Arc<HashMap<String,String>>)
{
	let (userData, setUserData) = UserData::cookie_signalGet();
	let userData = userData.read().clone().unwrap_or(UserData::new(&"EN".to_string()));

	toaster.toast(ToastBuilder::new(FluentManager::singleton().translate(userData.lang_get(), keyTranslate.to_string(),params).await)
		.with_expiry(Some(5_000))
		.with_level(level));
}