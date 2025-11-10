use leptoaster::{ToastBuilder, ToastLevel, ToasterContext};
use leptos::prelude::Read;
use crate::front::utils::fluent::FluentManager::FluentManager;
use crate::front::utils::users_data::UserData;

pub async fn toastingSuccess(toaster: ToasterContext,keyTranslate: impl Into<String>)
{
	toasting(toaster,keyTranslate,ToastLevel::Success).await;
}

pub async fn toastingErr(toaster: ToasterContext,keyTranslate: impl Into<String>)
{
	toasting(toaster,keyTranslate,ToastLevel::Error).await;
}

pub async fn toastingInfo(toaster: ToasterContext,keyTranslate: impl Into<String>)
{
	toasting(toaster,keyTranslate,ToastLevel::Error).await;
}

pub async fn toastingWarn(toaster: ToasterContext,keyTranslate: impl Into<String>)
{
	toasting(toaster,keyTranslate,ToastLevel::Error).await;
}

pub async fn toasting(toaster: ToasterContext,keyTranslate: impl Into<String>, level: ToastLevel)
{
	let (userData, setUserData) = UserData::cookie_signalGet();
	let userData = userData.read().clone().unwrap_or(UserData::new(&"EN".to_string()));

	toaster.toast(ToastBuilder::new(FluentManager::singleton().translateParamsLess(userData.lang_get(), keyTranslate).await)
		.with_expiry(Some(5_000))
		.with_level(level));
}