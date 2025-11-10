
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
	CLOSE
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
}