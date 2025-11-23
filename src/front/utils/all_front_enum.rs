use crate::api::login::components::LoginStatus;

/// contains all error key present into translate files
#[derive(strum_macros::Display)]
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
}

#[derive(strum_macros::Display)]
#[strum(prefix = "FRONTLOGIN_")]
pub enum AllFrontLoginEnum
{
	LOGIN_USER_CONNECTED,
	LOGIN_USER_DISCONNECTED,
	LOGIN_USER_SIGNEDUP,
	LOGIN_USER_NOT_FOUND,
	LOGIN_USER_INVALID_PWD,
	LOGIN_USER_ALREADY_EXISTS,
	LOGIN_LOCKED,
	LOGIN_USER_WANT_DISCONNECTED,
}

impl AllFrontLoginEnum
{
	pub fn fromLoginStatus(status: LoginStatus) -> Self
	{
		match status {
			LoginStatus::USER_IS_CONNECTED(_) => AllFrontLoginEnum::LOGIN_USER_CONNECTED,
			LoginStatus::USER_CONNECTED => AllFrontLoginEnum::LOGIN_USER_CONNECTED,
			LoginStatus::USER_DISCONNECTED => AllFrontLoginEnum::LOGIN_USER_DISCONNECTED,
			LoginStatus::USER_NOT_FOUND => AllFrontLoginEnum::LOGIN_USER_NOT_FOUND,
			LoginStatus::USER_INVALID_PWD => AllFrontLoginEnum::LOGIN_USER_INVALID_PWD,
			LoginStatus::USER_ALREADY_EXISTS => AllFrontLoginEnum::LOGIN_USER_ALREADY_EXISTS,
			LoginStatus::LOCKED(_) => AllFrontLoginEnum::LOGIN_LOCKED,
			LoginStatus::SERVER_ERROR => AllFrontLoginEnum::LOGIN_USER_NOT_FOUND,
		}
	}
}