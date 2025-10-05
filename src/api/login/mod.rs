#[cfg(feature = "ssr")]
mod user_back;
#[cfg(feature = "ssr")]
use Htrace::HTrace;

use std::fmt::Display;
use leptos::prelude::ServerFnError;
use leptos::server;
use serde::{Deserialize, Serialize};

#[server]
pub async fn API_user_login(generatedId: String, hashedPawd: String) -> Result<LoginStatus, ServerFnError>
{
	let result = match user_back::UserBackHelper::loginCheckAndCreate(generatedId, hashedPawd).await
	{
		Ok(result) => result,
		Err(user_back::UserBackHelperError::Direct(err)) => return Ok(err),
		Err(error) => {
			HTrace!("API_user_login error : {:?}",error);
			return Ok(LoginStatus::SERVER_ERROR)
		},
	};

	return Ok(LoginStatus::USER_CONNECTED);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoginStatus
{
	USER_IS_CONNECTED(bool),
	USER_CONNECTED,
	USER_DISCONNECTED,
	USER_NOT_FOUND,
	USER_INVALID_PWD,
	USER_ALREADY_EXISTS,
	LOCKED(i64), // duration in seconds
	SERVER_ERROR,
}

impl Display for LoginStatus
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
	{
		return write!(f, "{:?}", self);
	}
}

#[server]
pub async fn API_user_sign(generatedId: String, hashedPawd: String) -> Result<LoginStatus, ServerFnError>
{
	return match user_back::UserBackHelper::signCheckAndCreate(generatedId, hashedPawd).await
	{
		Ok(true) => Ok(LoginStatus::USER_CONNECTED),
		Ok(false) => Ok(LoginStatus::USER_ALREADY_EXISTS),
		Err(user_back::UserBackHelperError::Direct(err)) => Ok(err),
		Err(error) => {
			HTrace!("API_user_login error : {:?}",error);
			Ok(LoginStatus::SERVER_ERROR)
		},
	};
}

pub fn saferGeneratedId(generatedId: String) -> String
{
	return generatedId.replace("/", "L");
}

#[server]
pub async fn API_user_isConnected() -> Result<LoginStatus, ServerFnError>
{
	return match user_back::UserBackHelper::isConnected().await
	{
		Ok(Some(generatedId)) => Ok(LoginStatus::USER_IS_CONNECTED(true)),
		Ok(None) => Ok(LoginStatus::USER_IS_CONNECTED(false)),
		Err(err) => Ok(err),
	};
}


#[server]
pub async fn API_user_disconnect() -> Result<LoginStatus, ServerFnError>
{
	return Ok(user_back::UserBackHelper::disconnect().await);
}

#[server]
pub async fn API_user_getLayout() -> Result<Option<String>, ServerFnError>
{
	return Ok(None);
}