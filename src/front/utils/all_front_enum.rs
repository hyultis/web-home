use crate::api::login::components::LoginStatusErrors;
use crate::api::modules::ModuleApiError;

/// contains all error key present into translate files
#[derive(Debug, strum_macros::Display, PartialEq)]
#[strum(prefix = "FRONTERROR_")]
pub enum AllFrontErrorEnum
{
	SERVER_ERROR,
	SESSION_EXPIRED,
	CRYPTO_CONTEXT_MISSING,
	CRYPTO_STORAGE_FAILED,
	CRYPTO_ENCRYPT_FAILED,
	CRYPTO_DECRYPT_FAILED,
	MODULE_OUTDATED,
	MODULE_NOTEXIST,
}

impl From<ModuleApiError> for AllFrontErrorEnum
{
	fn from(error: ModuleApiError) -> Self
	{
		return match error
		{
			ModuleApiError::AUTH_REQUIRED => Self::SESSION_EXPIRED,
			ModuleApiError::NOT_FOUND => Self::MODULE_NOTEXIST,
			ModuleApiError::SERVER_ERROR => Self::SERVER_ERROR,
		};
	}
}

#[derive(strum_macros::Display)]
#[strum(prefix = "FRONTUI_")]
pub enum AllFrontUIEnum
{
	VALID,
	CLOSE,
	INVALID_URL,
	UPDATE,
	REFRESH,
	REMOVED,
	NOTITLE,
	HOME_CHANGE_OK,
	HOME_CHANGE_CANCEL,
	HOME_CHANGE_NEW
}

#[derive(strum_macros::Display, PartialEq)]
#[strum(prefix = "FRONTLOGIN_")]
pub enum AllFrontLoginEnum
{
	LOGIN_USER_CONNECTED,
	LOGIN_USER_DISCONNECTED,
	LOGIN_USER_SIGNEDUP,
	LOGIN_CREDENTIALS_INVALID,
	SIGN_DISABLED,
	SIGN_PASSWORD_TOO_SHORT,
	SALT_INVALID,
	LOGIN_LOCKED,
	LOGIN_USER_WANT_DISCONNECTED,
	SERVER_ERROR
}

impl AllFrontLoginEnum
{
	pub fn fromLoginStatus(status: LoginStatusErrors) -> Self
	{
		match status {
			LoginStatusErrors::USER_DISCONNECTED => AllFrontLoginEnum::LOGIN_USER_DISCONNECTED,
			LoginStatusErrors::USER_NOT_FOUND |
			LoginStatusErrors::USER_INVALID_PWD |
			LoginStatusErrors::USER_ALREADY_EXISTS => AllFrontLoginEnum::LOGIN_CREDENTIALS_INVALID,
			LoginStatusErrors::LOCKED(_) => AllFrontLoginEnum::LOGIN_LOCKED,
			LoginStatusErrors::SERVER_ERROR => AllFrontLoginEnum::SERVER_ERROR,
			LoginStatusErrors::SIGN_DISABLED => AllFrontLoginEnum::SIGN_DISABLED,
			LoginStatusErrors::SALT_INVALID => AllFrontLoginEnum::SALT_INVALID
		}
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn loginStatus_doesNotExposeAccountExistence()
	{
		assert!(matches!(
			AllFrontLoginEnum::fromLoginStatus(LoginStatusErrors::USER_NOT_FOUND),
			AllFrontLoginEnum::LOGIN_CREDENTIALS_INVALID
		));
		assert!(matches!(
			AllFrontLoginEnum::fromLoginStatus(LoginStatusErrors::USER_ALREADY_EXISTS),
			AllFrontLoginEnum::LOGIN_CREDENTIALS_INVALID
		));
	}

	#[test]
	fn moduleAuthenticationError_requestsSessionFeedback()
	{
		let error = AllFrontErrorEnum::from(ModuleApiError::AUTH_REQUIRED);
		assert_eq!(error, AllFrontErrorEnum::SESSION_EXPIRED);
		assert_eq!(error.to_string(), "FRONTERROR_SESSION_EXPIRED");
	}
}
