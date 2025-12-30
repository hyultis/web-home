use crate::api::login::components::LoginStatusErrors;

/// contains all error key present into translate files
#[derive(strum_macros::Display, PartialEq)]
#[strum(prefix = "FRONTERROR_")]
pub enum AllFrontErrorEnum
{
	#[strum(to_string = "Server error: {0}")]
	SERVER_ERROR(String),

	MODULE_OUTDATED,
	MODULE_NOTEXIST,

}

#[derive(strum_macros::Display)]
#[strum(prefix = "FRONTUI_")]
pub enum AllFrontUIEnum
{
	VALID,
	CLOSE,
	MUST_NOT_EMPTY,
	INVALID_URL,
	UPDATE,
	REFRESH,
	REMOVED,
	NOTITLE,
	HOME_CHANGE_OK,
	HOME_CHANGE_CANCEL,
	HOME_CHANGE_NEW
}

#[derive(strum_macros::Display)]
#[strum(prefix = "FRONTLOGIN_")]
pub enum AllFrontLoginEnum
{
	LOGIN_USER_CONNECTED,
	LOGIN_USER_DISCONNECTED,
	LOGIN_USER_SIGNEDUP,
	LOGIN_USER_NOT_FOUND,
	SIGN_DISABLED,
	SALT_INVALID,
	LOGIN_USER_INVALID_PWD,
	LOGIN_USER_ALREADY_EXISTS,
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
			LoginStatusErrors::USER_NOT_FOUND => AllFrontLoginEnum::LOGIN_USER_NOT_FOUND,
			LoginStatusErrors::USER_INVALID_PWD => AllFrontLoginEnum::LOGIN_USER_INVALID_PWD,
			LoginStatusErrors::USER_ALREADY_EXISTS => AllFrontLoginEnum::LOGIN_USER_ALREADY_EXISTS,
			LoginStatusErrors::LOCKED(_) => AllFrontLoginEnum::LOGIN_LOCKED,
			LoginStatusErrors::SERVER_ERROR => AllFrontLoginEnum::SERVER_ERROR,
			LoginStatusErrors::SIGN_DISABLED => AllFrontLoginEnum::SIGN_DISABLED,
			LoginStatusErrors::SALT_INVALID => AllFrontLoginEnum::SALT_INVALID
		}
	}
}