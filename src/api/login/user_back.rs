use std::fs::File;
use std::time::Duration;
use Hconfig::Errors;
use Hconfig::HConfig::HConfig;
use Hconfig::HConfigManager::HConfigManager;
use Hconfig::IO::json::WrapperJson;
use Hconfig::tinyjson::JsonValue;
use Htrace::components::level::Level;
use Htrace::{HTrace, HTraceError};
use leptos::prelude::{ServerFnErrorErr};
use leptos_axum::extract;
use time::macros::format_description;
use time::OffsetDateTime;
use tower_sessions::Session;
use crate::api::login::{saferGeneratedId, user_back, LoginStatus};
use crate::api::{SESSION_LOGIN_NBTRY, SESSION_LOGIN_NBTRY_DELAY_RESET, SESSION_LOGIN_NBTRY_LAST,
                 SESSION_LOGIN_NBTRY_MAX, SESSION_SIGN_NBTRY, SESSION_SIGN_NBTRY_DELAY_RESET, SESSION_SIGN_NBTRY_LAST, SESSION_SIGN_NBTRY_MAX, SESSION_USER_GENERATED_ID};

#[derive(Debug)]
pub enum UserBackHelperError
{
	HConfigError(Errors),
	ServerError(ServerFnErrorErr),
	Direct(LoginStatus),
}

pub struct UserBackHelper;

impl UserBackHelper {
	/// Check in the session if the user have to much try something (connection or sign up)
	pub async fn checkSession(keyTime: &str, keyNb: &str, maxRetry: u8, delayreset: Duration) -> (Result<(Session, u8), UserBackHelperError>)
	{
		let formatDesc = format_description!("[unix_timestamp precision:millisecond]");
		let session = match extract::<Session>().await {
			Ok(session) => session,
			Err(err) => return Err(UserBackHelperError::ServerError(err))
		};

		let retryLastTime = session.get::<i64>(keyTime).await.unwrap_or_default().unwrap_or_default();
		let mut retryValue = 0;

		if (OffsetDateTime::from_unix_timestamp(retryLastTime).unwrap_or(OffsetDateTime::UNIX_EPOCH) - OffsetDateTime::now_utc() < delayreset)
		{
			retryValue = session.get::<u8>(keyNb).await.unwrap_or_default().unwrap_or_default();
		} else {
			HTraceError!("UserBackHelper.checkSession fail to reset : {}",session.insert(keyNb, 0).await);
		}

		if (retryValue >= maxRetry)
		{
			let timestamp = OffsetDateTime::now_utc().unix_timestamp();
			return Err(UserBackHelperError::Direct(LoginStatus::LOCKED(timestamp)));
		}

		return Ok((session, retryValue));
	}

	/// Check if the user is already created and create it if not
	/// is already created, return Ok(false)
	/// if not created, return Ok(true)
	pub async fn signCheckAndCreate(generatedId: String, hashedPwd: String) -> Result<bool, UserBackHelperError>
	{
		let (session, retryValue) = match UserBackHelper::checkSession(SESSION_SIGN_NBTRY_LAST, SESSION_SIGN_NBTRY, SESSION_SIGN_NBTRY_MAX, SESSION_SIGN_NBTRY_DELAY_RESET).await
		{
			Ok(d) => d,
			Err(reason) => return Err(reason)
		};

		let generatedId = saferGeneratedId(generatedId);
		let userConfig = HConfig::new::<WrapperJson>(generatedId, "./config/users".to_string());
		let mut config = userConfig.map_err(|err| {
			HTrace!((Level::ERROR) "UserBackHelper::signCheckAndCreate : {}",err);
			UserBackHelperError::HConfigError(err)
		})?;

		if config.value_get("dateSignUp").is_some()
		{
			return Ok(false);
		}

		config.value_set("dateSignUp", JsonValue::String(format!("{}", OffsetDateTime::now_utc())));
		config.value_set("hashedPwd", JsonValue::String(hashedPwd));

		config.file_save().map_err(|err| UserBackHelperError::HConfigError(err))?;

		// si l'utlise c'est correct inscrit, il ne peut plus se resincrire pendant SESSION_SIGN_NBTRY_LAST temps
		HTraceError!("UserBackHelper.signCheckAndCreate fail to insert SESSION_SIGN_NBTRY : {}",session.insert(SESSION_SIGN_NBTRY, retryValue+1).await);
		HTraceError!("UserBackHelper.signCheckAndCreate fail to insert SESSION_SIGN_NBTRY_LAST : {}",session.insert(SESSION_SIGN_NBTRY_LAST, OffsetDateTime::now_utc().unix_timestamp()).await);

		return Ok(true);
	}


	/// Check if the user is already created and create it if not
	/// is already created, return Ok(false)
	/// if not created, return Ok(true)
	pub async fn loginCheckAndCreate(generatedId: String, hashedPwd: String) -> Result<bool, UserBackHelperError>
	{
		let (session, retryValue) = match UserBackHelper::checkSession(SESSION_LOGIN_NBTRY_LAST, SESSION_LOGIN_NBTRY, SESSION_LOGIN_NBTRY_MAX, SESSION_LOGIN_NBTRY_DELAY_RESET).await
		{
			Ok(d) => d,
			Err(reason) => return Err(reason)
		};

		let generatedId = saferGeneratedId(generatedId);
		if let Err(_) = File::open(format!("{}/users/{}.json", HConfigManager::singleton().confPath_get(), generatedId))
		{
			return Err(UserBackHelperError::Direct(LoginStatus::USER_NOT_FOUND));
		}

		let userConfig = HConfig::new::<WrapperJson>(generatedId.clone(), "./config/users".to_string());
		let config = userConfig.map_err(|err| {
			HTrace!((Level::ERROR) "UserBackHelper::loginCheckAndCreate : {}",err);
			UserBackHelperError::HConfigError(err)
		})?;

		if let Some(configHashedPwd) = config.value_get("hashedPwd")
		{
			let configHashedPwd: String = configHashedPwd.try_into().unwrap_or_default();
			if configHashedPwd == hashedPwd
			{
				HTraceError!("UserBackHelper.loginCheckAndCreate connect SESSION_USER_GENERATED_ID : {}",session.insert(SESSION_USER_GENERATED_ID, generatedId).await);
				let _ = session.remove::<u8>(SESSION_LOGIN_NBTRY).await;
				let _ = session.remove::<Duration>(SESSION_LOGIN_NBTRY_LAST).await;
				return Ok(true);
			}
		}

		HTraceError!("UserBackHelper.loginCheckAndCreate fail to insert SESSION_LOGIN_NBTRY : {}",session.insert(SESSION_LOGIN_NBTRY, retryValue+1).await);
		HTraceError!("UserBackHelper.loginCheckAndCreate fail to insert SESSION_LOGIN_NBTRY_LAST : {}",session.insert(SESSION_LOGIN_NBTRY_LAST, OffsetDateTime::now_utc().unix_timestamp()).await);
		return Err(UserBackHelperError::Direct(LoginStatus::USER_INVALID_PWD));
	}

	pub async fn isConnected() -> Result<Option<String>, LoginStatus>
	{
		let session = match extract::<Session>().await {
			Ok(session) => session,
			Err(error) => {
				HTrace!("API_user_isConnected error : {:?}",error);
				return Err(LoginStatus::SERVER_ERROR);
			},
		};

		if let Ok(Some(generatedId)) = session.get::<String>(user_back::SESSION_USER_GENERATED_ID).await
		{
			return Ok(Some(generatedId));
		}

		return Ok(None);
	}

	pub async fn disconnect() -> LoginStatus
	{
		let session = match extract::<Session>().await {
			Ok(session) => session,
			Err(error) => {
				HTrace!("API_user_isConnected error : {:?}",error);
				return LoginStatus::SERVER_ERROR;
			},
		};

		HTraceError!("serBackHelper.disconnect fail to disconnect SESSION_USER_GENERATED_ID : {}",session.remove::<String>(user_back::SESSION_USER_GENERATED_ID).await);

		return LoginStatus::USER_DISCONNECTED;
	}
}