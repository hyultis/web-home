#[cfg(feature = "ssr")]
pub mod user_back;
#[cfg(feature = "ssr")]
pub mod salt;

pub mod components;

#[cfg(feature = "ssr")]
use Htrace::HTrace;

use leptos::prelude::ServerFnError;
use leptos::server;
use components::LoginStatusErrors;

#[server]
pub async fn API_user_salt(generatedId: String) -> Result<String, LoginStatusErrors>
{
	return match salt::getSiteSaltForUser(generatedId)
	{
		Some(ok) => Ok(ok),
		None => Err(LoginStatusErrors::SALT_INVALID),
	};
}

#[server]
pub async fn API_user_login(generatedId: String, hashedPawd: String) -> Result<(), LoginStatusErrors>
{
	return match user_back::UserBackHelper::loginCheckAndCreate(generatedId, hashedPawd).await
	{
		Ok(_) => Ok(()),
		Err(user_back::UserBackHelperError::LoginError(err)) => Err(err),
		Err(error) => {
			HTrace!("API_user_login error : {:?}",error);
			Err(LoginStatusErrors::SERVER_ERROR)
		},
	}
}

#[server]
pub async fn API_user_sign(generatedId: String, hashedPawd: String) -> Result<(), LoginStatusErrors>
{
	return match user_back::UserBackHelper::signCheckAndCreate(generatedId, hashedPawd).await
	{
		Ok(true) => Ok(()),
		Ok(false) => Err(LoginStatusErrors::USER_ALREADY_EXISTS),
		Err(user_back::UserBackHelperError::LoginError(err)) => Err(err),
		Err(error) => {
			HTrace!("API_user_login error : {:?}",error);
			Err(LoginStatusErrors::SERVER_ERROR)
		},
	};
}

#[server]
pub async fn API_user_getLayout() -> Result<Option<String>, ServerFnError>
{
	return Ok(None);
}