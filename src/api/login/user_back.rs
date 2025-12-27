use std::fs::File;
use std::time::Duration;
use base64ct::{Base64, Encoding};
use Hconfig::Errors;
use Hconfig::HConfig::HConfig;
use Hconfig::HConfigManager::HConfigManager;
use Hconfig::IO::json::WrapperJson;
use Hconfig::tinyjson::JsonValue;
use Htrace::components::level::Level;
use Htrace::{HTrace, HTraceError};
use leptos::prelude::{ServerFnError, ServerFnErrorErr};
use leptos_axum::extract;
use sha3::{Digest, Sha3_256};
use time::macros::format_description;
use time::OffsetDateTime;
use tower_sessions::Session;
use crate::api::{SESSION_LOGIN_NBTRY, SESSION_LOGIN_NBTRY_DELAY_RESET, SESSION_LOGIN_NBTRY_LAST, SESSION_LOGIN_NBTRY_MAX, SESSION_SIGN_NBTRY, SESSION_SIGN_NBTRY_DELAY_RESET, SESSION_SIGN_NBTRY_LAST, SESSION_SIGN_NBTRY_MAX};
use crate::api::login::components::LoginStatusErrors;

#[derive(Debug)]
pub enum UserBackHelperError
{
	HConfigError(Errors),
	ServerError(ServerFnErrorErr),
	LoginError(LoginStatusErrors),
}

impl Into<ServerFnError> for UserBackHelperError
{
	fn into(self) -> ServerFnError {
		match self {
			UserBackHelperError::HConfigError(err) => ServerFnError::new(format!("HConfigError: {}",err)),
			UserBackHelperError::ServerError(err) => ServerFnError::new(format!("ServerError: {}",err)),
			UserBackHelperError::LoginError(err) => ServerFnError::new(format!("LoginError: {}",err)),
		}
	}
}

pub struct UserBackHelper;

impl UserBackHelper {
	/// Check in the session if the user have to much try something (connection or sign up)
	pub async fn checkSession(keyTime: &str, keyNb: &str, maxRetry: u8, delayreset: Duration) -> Result<(Session, u8), UserBackHelperError>
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
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::LOCKED(timestamp)));
		}

		return Ok((session, retryValue));
	}

	/// Check if the user is already created and create it if not
	/// is already created, return Ok(false)
	/// if not created, return Ok(true)
	pub async fn signCheckAndCreate(generatedId: String, hashedPwd: String) -> Result<bool, UserBackHelperError>
	{
		let allowRegistration = crate::api::ALLOW_REGISTRATION.get().map(|ab| ab.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false);
		if(!allowRegistration) {
			return Err(UserBackHelperError::LoginError(LoginStatusErrors::SIGN_DISABLED));
		}

		let (session, retryValue) = match UserBackHelper::checkSession(SESSION_SIGN_NBTRY_LAST, SESSION_SIGN_NBTRY, SESSION_SIGN_NBTRY_MAX, SESSION_SIGN_NBTRY_DELAY_RESET).await
		{
			Ok(d) => d,
			Err(reason) => return Err(reason)
		};

		let mut config = Self::getUserConfig(generatedId,true)?;
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
	pub async fn loginCheckAndCreate(generatedId: String, hashedPwd: String) -> Result<(), UserBackHelperError>
	{
		let (session, retryValue) = match UserBackHelper::checkSession(SESSION_LOGIN_NBTRY_LAST, SESSION_LOGIN_NBTRY, SESSION_LOGIN_NBTRY_MAX, SESSION_LOGIN_NBTRY_DELAY_RESET).await
		{
			Ok(d) => d,
			Err(reason) => return Err(reason)
		};

		let config = Self::getUserConfig(generatedId,false)?;

		if let Some(configHashedPwd) = config.value_get("hashedPwd")
		{
			let configHashedPwd: String = configHashedPwd.try_into().unwrap_or_default();
			if configHashedPwd == hashedPwd
			{
				let _ = session.remove::<u8>(SESSION_LOGIN_NBTRY).await;
				let _ = session.remove::<Duration>(SESSION_LOGIN_NBTRY_LAST).await;
				return Ok(());
			}
		}

		HTraceError!("UserBackHelper.loginCheckAndCreate fail to insert SESSION_LOGIN_NBTRY : {}",session.insert(SESSION_LOGIN_NBTRY, retryValue+1).await);
		HTraceError!("UserBackHelper.loginCheckAndCreate fail to insert SESSION_LOGIN_NBTRY_LAST : {}",session.insert(SESSION_LOGIN_NBTRY_LAST, OffsetDateTime::now_utc().unix_timestamp()).await);
		return Err(UserBackHelperError::LoginError(LoginStatusErrors::USER_INVALID_PWD));
	}

	/// get config file corresponding to the user
	pub fn getUserConfig(generatedId: String, createIfAbsent: bool) -> Result<HConfig, UserBackHelperError>
	{
		let generatedId = saferGeneratedId(generatedId);
		if(!createIfAbsent)
		{
			let filepath = format!("{}/users/{}.json", HConfigManager::singleton().confPath_get(), generatedId);
			if let Err(_) = File::open(filepath.clone())
			{
				println!("trying open : {}",filepath);
				return Err(UserBackHelperError::LoginError(LoginStatusErrors::USER_NOT_FOUND));
			}
		}

		let userConfig = HConfig::new::<WrapperJson>(generatedId.clone(), "./config/users".to_string());
		let config = userConfig.map_err(|err| {
			HTrace!((Level::ERROR) "UserBackHelper::loginCheckAndCreate : {}",err);
			UserBackHelperError::HConfigError(err)
		})?;

		return Ok(config);
	}
}

pub fn saferGeneratedId(generatedId: String) -> String
{
	let mut hasher = Sha3_256::new();
	hasher.update(generatedId);
	let result = hasher.finalize();
	let returning = Base64::encode_string(&result);
	return returning.replace("/", "LL");
}