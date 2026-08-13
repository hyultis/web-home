#[cfg(feature = "ssr")]
pub mod user_back;
#[cfg(feature = "ssr")]
pub mod salt;
#[cfg(feature = "ssr")]
pub mod session;

pub mod components;

#[cfg(feature = "ssr")]
use Htrace::HTrace;

use leptos::server;
use leptos::server_fn::codec::Json;
use components::{AccountPreferencesError, LoginStatusErrors, PasswordRotationError, PasswordRotationFinalize, PasswordRotationSnapshot};

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
		Err(user_back::UserBackHelperError::LoginError(LoginStatusErrors::USER_NOT_FOUND)) => Err(LoginStatusErrors::USER_INVALID_PWD),
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
		Ok(_) => Ok(()),
		Err(user_back::UserBackHelperError::LoginError(err)) => Err(err),
		Err(error) => {
			HTrace!("API_user_login error : {:?}",error);
			Err(LoginStatusErrors::SERVER_ERROR)
		},
	};
}

#[server]
pub async fn API_user_logout() -> Result<(), LoginStatusErrors>
{
	return match user_back::AuthenticatedUser::logout().await
	{
		Ok(_) => Ok(()),
		Err(user_back::UserBackHelperError::LoginError(err)) => Err(err),
		Err(error) => {
			HTrace!("API_user_logout error : {:?}", error);
			Err(LoginStatusErrors::SERVER_ERROR)
		},
	};
}

#[server]
pub async fn API_user_preferences_get() -> Result<Option<String>, AccountPreferencesError>
{
	return user_back::UserBackHelper::accountPreferences_get().await.map_err(|error| {
		if (error == AccountPreferencesError::SERVER_ERROR)
		{
			HTrace!("account preferences retrieval failed");
		}
		return error;
	});
}

#[server]
pub async fn API_user_preferences_set(content: String) -> Result<(), AccountPreferencesError>
{
	return user_back::UserBackHelper::accountPreferences_set(content).await.map_err(|error| {
		if (error == AccountPreferencesError::SERVER_ERROR)
		{
			HTrace!("account preferences persistence failed");
		}
		return error;
	});
}

#[server]
pub async fn API_user_passwordRotation_prepare() -> Result<PasswordRotationSnapshot, PasswordRotationError>
{
	return user_back::UserBackHelper::passwordRotation_prepare().await.map_err(|error| {
		if (error == PasswordRotationError::SERVER_ERROR)
		{
			HTrace!("password rotation preparation failed");
		}
		return error;
	});
}

#[server(input = Json)]
pub async fn API_user_passwordRotation_finalize(request: PasswordRotationFinalize) -> Result<(), PasswordRotationError>
{
	return user_back::UserBackHelper::passwordRotation_finalize(request).await.map_err(|error| {
		if (error == PasswordRotationError::SERVER_ERROR)
		{
			HTrace!("password rotation finalization failed");
		}
		return error;
	});
}

#[cfg(test)]
mod contract_tests
{
	use leptos::server_fn::codec::{Encoding, Json};
	use leptos::server_fn::{ContentType, Http, ServerFn};

	use super::ApiUserPasswordrotationFinalize;

	trait HttpInput
	{
		type Encoding;
	}

	impl<Input,Output> HttpInput for Http<Input,Output>
	{
		type Encoding = Input;
	}

	#[test]
	fn passwordRotationFinalization_usesJsonRequestBody()
	{
		type RotationInput = <<ApiUserPasswordrotationFinalize as ServerFn>::Protocol as HttpInput>::Encoding;

		assert_eq!(<RotationInput as ContentType>::CONTENT_TYPE,<Json as ContentType>::CONTENT_TYPE);
		assert_eq!(<RotationInput as Encoding>::METHOD,http::Method::POST);
	}
}
