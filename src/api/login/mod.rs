#[cfg(feature = "ssr")]
pub mod user_back;
#[cfg(feature = "ssr")]
pub mod salt;

pub mod components;

#[cfg(feature = "ssr")]
use Htrace::HTrace;

use leptos::prelude::ServerFnError;
use leptos::server;
use components::LoginStatus;
use components::SaltReturn;

#[server]
pub async fn API_user_salt(generatedId: String) -> Result<SaltReturn, ServerFnError>
{
	return match salt::getSiteSaltForUser(generatedId)
	{
		Some(ok) => Ok(SaltReturn::SALT(ok)),
		None => Ok(SaltReturn::ERROR("Cannot generate salt".to_string())),
	};
}

#[server]
pub async fn API_user_login(generatedId: String, hashedPawd: String) -> Result<LoginStatus, ServerFnError>
{
	let result = match user_back::UserBackHelper::loginCheckAndCreate(generatedId, hashedPawd).await
	{
		Ok(result) => result,
		Err(user_back::UserBackHelperError::LoginError(err)) => return Ok(err),
		Err(error) => {
			HTrace!("API_user_login error : {:?}",error);
			return Ok(LoginStatus::SERVER_ERROR)
		},
	};

	return Ok(LoginStatus::USER_CONNECTED);
}

#[server]
pub async fn API_user_sign(generatedId: String, hashedPawd: String) -> Result<LoginStatus, ServerFnError>
{
	return match user_back::UserBackHelper::signCheckAndCreate(generatedId, hashedPawd).await
	{
		Ok(true) => Ok(LoginStatus::USER_CONNECTED),
		Ok(false) => Ok(LoginStatus::USER_ALREADY_EXISTS),
		Err(user_back::UserBackHelperError::LoginError(err)) => Ok(err),
		Err(error) => {
			HTrace!("API_user_login error : {:?}",error);
			Ok(LoginStatus::SERVER_ERROR)
		},
	};
}

#[server]
pub async fn API_user_getLayout() -> Result<Option<String>, ServerFnError>
{
	return Ok(None);
}